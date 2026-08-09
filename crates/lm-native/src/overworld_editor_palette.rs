use crate::{native_clipboard, overworld_editor_render::OverworldAnimationOwner};
use eframe::egui;
use lm_app::OverworldControllerEdit;
use lm_graphics::{Bgr555, Palette, PaletteChange, PaletteEntryOwner, PaletteOwnership, Rgb8};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Color,
    Row,
}

enum NativePaste {
    Color(Bgr555),
    Row([Bgr555; 16]),
}

#[derive(Default)]
pub(crate) struct OverworldPalettePanel {
    selected: usize,
    paste_target: Option<PasteTarget>,
    navigation: Option<OverworldAnimationOwner>,
}

impl OverworldPalettePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        ownership: &PaletteOwnership,
        animation_ownership: &[Option<OverworldAnimationOwner>],
    ) -> Option<Result<OverworldControllerEdit, String>> {
        self.selected = self.selected.min(palette.colors.len().saturating_sub(1));
        let (mut copy_result, grid_paste) =
            self.palette_clipboard_grid(ui, palette, animation_ownership);
        let mut native_paste = None;
        let color = palette.colors.get(self.selected).copied()?;
        ui.label(format!(
            "Color {:03X} — BGR555 {:04X}",
            self.selected, color.0
        ));
        let owner = ownership.owner(self.selected);
        let animation_owner = animation_ownership.get(self.selected).copied().flatten();
        let editable = owner == Some(PaletteEntryOwner::Editable);
        let row = palette_row(&palette.colors, self.selected).ok();
        let row_editable = row.is_some_and(|_| {
            let start = self.selected / 16 * 16;
            (start..start + 16)
                .all(|index| ownership.owner(index) == Some(PaletteEntryOwner::Editable))
        });
        if let Some(result) = grid_paste {
            native_paste = Some(result.and_then(|paste| match paste {
                NativePaste::Color(color) if editable => Ok(vec![PaletteChange {
                    index: self.selected,
                    color,
                }]),
                NativePaste::Row(colors) if row_editable => {
                    row_color_changes(colors, self.selected, palette.colors.len())
                }
                _ => Err("selected palette entries are read-only".into()),
            }));
        }
        ui.label(match animation_owner {
            Some(owner) => format!(
                "Animation ownership: {:?} record {:02X} (Ctrl+Shift+click to navigate)",
                owner.domain, owner.record
            ),
            None => match owner {
                Some(PaletteEntryOwner::Editable) => "Ownership: editable".into(),
                Some(PaletteEntryOwner::Fixed) => "Ownership: fixed (read-only)".into(),
                Some(PaletteEntryOwner::ExAnimation { record }) => {
                    format!("Ownership: ExAnimation record {record:04X} (read-only)")
                }
                None => "Ownership: invalid (read-only)".into(),
            },
        });
        ui.horizontal(|ui| {
            if ui.button("Copy color").clicked() {
                copy_result = Some(native_clipboard::copy_palette_color_to_system(
                    ui.ctx(),
                    color,
                ));
            }
            if ui
                .add_enabled(editable, egui::Button::new("Paste color"))
                .clicked()
            {
                match native_clipboard::request_palette_color_paste(ui.ctx()) {
                    Ok(Some(color)) => {
                        native_paste = Some(Ok(vec![PaletteChange {
                            index: self.selected,
                            color,
                        }]))
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::Color),
                    Err(error) => native_paste = Some(Err(error)),
                }
            }
            if ui
                .add_enabled(row.is_some(), egui::Button::new("Copy row"))
                .clicked()
            {
                copy_result = Some(native_clipboard::copy_palette_row_to_system(
                    ui.ctx(),
                    row.expect("enabled row is complete"),
                ));
            }
            if ui
                .add_enabled(row_editable, egui::Button::new("Paste row"))
                .clicked()
            {
                match native_clipboard::request_palette_row_paste(ui.ctx()) {
                    Ok(Some(colors)) => {
                        native_paste = Some(row_color_changes(
                            colors,
                            self.selected,
                            palette.colors.len(),
                        ))
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::Row),
                    Err(error) => native_paste = Some(Err(error)),
                }
            }
        });
        ui.small("Ctrl+left/right copies or pastes a color; add Alt for its complete row.");
        if let Some(result) = copy_result {
            match result {
                Ok(()) => {}
                Err(error) => return Some(Err(error)),
            }
        }
        if let Some(changes) = native_paste {
            return Some(changes.map(OverworldControllerEdit::PaletteChanges));
        }
        if let Some(text) = pasted_text(ui) {
            let changes = match self.paste_target.take() {
                Some(PasteTarget::Color) if editable => {
                    native_clipboard::decode_palette_color(&text).map(|color| {
                        vec![PaletteChange {
                            index: self.selected,
                            color,
                        }]
                    })
                }
                Some(PasteTarget::Row) => {
                    palette_row_changes(&text, self.selected, palette.colors.len())
                }
                _ => return None,
            };
            return Some(changes.map(OverworldControllerEdit::PaletteChanges));
        }
        let rgb = color.to_rgb8();
        let mut value = [rgb.red, rgb.green, rgb.blue];
        ui.add_enabled_ui(editable, |ui| ui.color_edit_button_srgb(&mut value))
            .inner
            .changed()
            .then(|| {
                Ok(OverworldControllerEdit::PaletteChanges(vec![
                    PaletteChange {
                        index: self.selected,
                        color: Bgr555::from_rgb8(Rgb8 {
                            red: value[0],
                            green: value[1],
                            blue: value[2],
                        }),
                    },
                ]))
            })
    }

    fn palette_clipboard_grid(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        animation_ownership: &[Option<OverworldAnimationOwner>],
    ) -> (
        Option<Result<(), String>>,
        Option<Result<NativePaste, String>>,
    ) {
        let mut copy_result = None;
        let mut native_paste = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("overworld-palette-grid")
                .spacing([3.0, 3.0])
                .show(ui, |ui| {
                    for (index, color) in palette.colors.iter().copied().enumerate() {
                        let rgb = color.to_rgb8();
                        let button = egui::Button::new("  ")
                            .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue))
                            .selected(index == self.selected);
                        let response = ui.add_sized([24.0, 24.0], button);
                        if response.clicked() {
                            self.selected = index;
                            let modifiers = ui.input(|input| input.modifiers);
                            if modifiers.ctrl && modifiers.shift {
                                self.navigation =
                                    crate::overworld_editor_render::ctrl_shift_animation_navigation(
                                        modifiers,
                                        animation_ownership.get(index).copied().flatten(),
                                    );
                            } else if modifiers.ctrl {
                                copy_result = Some(if modifiers.alt {
                                    palette_row(&palette.colors, index).and_then(|row| {
                                        native_clipboard::copy_palette_row_to_system(ui.ctx(), row)
                                    })
                                } else {
                                    native_clipboard::copy_palette_color_to_system(ui.ctx(), color)
                                });
                            }
                        }
                        if response.secondary_clicked() && ui.input(|input| input.modifiers.ctrl) {
                            self.selected = index;
                            let row = ui.input(|input| input.modifiers.alt);
                            let result = if row {
                                native_clipboard::request_palette_row_paste(ui.ctx())
                                    .map(|value| value.map(NativePaste::Row))
                            } else {
                                native_clipboard::request_palette_color_paste(ui.ctx())
                                    .map(|value| value.map(NativePaste::Color))
                            };
                            match result {
                                Ok(Some(paste)) => native_paste = Some(Ok(paste)),
                                Ok(None) => {
                                    self.paste_target = Some(if row {
                                        PasteTarget::Row
                                    } else {
                                        PasteTarget::Color
                                    });
                                }
                                Err(error) => native_paste = Some(Err(error)),
                            }
                        }
                        if index % 16 == 15 {
                            ui.end_row();
                        }
                    }
                });
        });
        (copy_result, native_paste)
    }

    pub(crate) fn take_navigation(&mut self) -> Option<OverworldAnimationOwner> {
        self.navigation.take()
    }
}

fn palette_row(colors: &[Bgr555], selected: usize) -> Result<&[Bgr555], String> {
    let start = selected / 16 * 16;
    colors
        .get(start..start.saturating_add(16))
        .ok_or_else(|| "selected color does not belong to a complete palette row".to_string())
}

fn palette_row_changes(
    text: &str,
    selected: usize,
    color_count: usize,
) -> Result<Vec<PaletteChange>, String> {
    row_color_changes(
        native_clipboard::decode_palette_row(text)?,
        selected,
        color_count,
    )
}

fn row_color_changes(
    colors: [Bgr555; 16],
    selected: usize,
    color_count: usize,
) -> Result<Vec<PaletteChange>, String> {
    let start = selected / 16 * 16;
    if start.saturating_add(16) > color_count {
        return Err("selected color does not belong to a complete palette row".into());
    }
    Ok(colors
        .into_iter()
        .enumerate()
        .map(|(offset, color)| PaletteChange {
            index: start + offset,
            color,
        })
        .collect())
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::palette_row_changes;
    use crate::native_clipboard;
    use lm_graphics::Bgr555;

    #[test]
    fn overworld_row_paste_is_aligned_and_complete() {
        let row = [Bgr555(0x2345); 16];
        let text = native_clipboard::encode_palette_row(&row).unwrap();
        let changes = palette_row_changes(&text, 47, 64).unwrap();
        assert_eq!((changes[0].index, changes[15].index), (32, 47));
        assert!(palette_row_changes(&text, 64, 65).is_err());
    }
}
