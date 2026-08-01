use crate::native_clipboard;
use eframe::egui;
use lm_app::OverworldControllerEdit;
use lm_graphics::{Bgr555, Palette, PaletteChange, PaletteEntryOwner, PaletteOwnership, Rgb8};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Color,
    Row,
}

#[derive(Default)]
pub(crate) struct OverworldPalettePanel {
    selected: usize,
    paste_target: Option<PasteTarget>,
}

impl OverworldPalettePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        self.selected = self.selected.min(palette.colors.len().saturating_sub(1));
        let mut copy_result = self.palette_clipboard_grid(ui, palette);
        let color = palette.colors.get(self.selected).copied()?;
        ui.label(format!(
            "Color {:03X} — BGR555 {:04X}",
            self.selected, color.0
        ));
        let owner = ownership.owner(self.selected);
        let editable = owner == Some(PaletteEntryOwner::Editable);
        let row = palette_row(&palette.colors, self.selected).ok();
        let row_editable = row.is_some_and(|_| {
            let start = self.selected / 16 * 16;
            (start..start + 16)
                .all(|index| ownership.owner(index) == Some(PaletteEntryOwner::Editable))
        });
        ui.label(match owner {
            Some(PaletteEntryOwner::Editable) => "Ownership: editable".into(),
            Some(PaletteEntryOwner::Fixed) => "Ownership: fixed (read-only)".into(),
            Some(PaletteEntryOwner::ExAnimation { record }) => {
                format!("Ownership: ExAnimation record {record:04X} (read-only)")
            }
            None => "Ownership: invalid (read-only)".into(),
        });
        ui.horizontal(|ui| {
            if ui.button("Copy color").clicked() {
                copy_result = Some(native_clipboard::encode_palette_color(color));
            }
            if ui
                .add_enabled(editable, egui::Button::new("Paste color"))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Color);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui
                .add_enabled(row.is_some(), egui::Button::new("Copy row"))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_palette_row(
                    row.expect("enabled row is complete"),
                ));
            }
            if ui
                .add_enabled(row_editable, egui::Button::new("Paste row"))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Row);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        ui.small("Ctrl+left/right copies or pastes a color; add Alt for its complete row.");
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
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
    ) -> Option<Result<String, String>> {
        let mut copy_result = None;
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
                            if modifiers.ctrl {
                                copy_result = Some(if modifiers.alt {
                                    palette_row(&palette.colors, index)
                                        .and_then(native_clipboard::encode_palette_row)
                                } else {
                                    native_clipboard::encode_palette_color(color)
                                });
                            }
                        }
                        if response.secondary_clicked() && ui.input(|input| input.modifiers.ctrl) {
                            self.selected = index;
                            self.paste_target = Some(if ui.input(|input| input.modifiers.alt) {
                                PasteTarget::Row
                            } else {
                                PasteTarget::Color
                            });
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                        }
                        if index % 16 == 15 {
                            ui.end_row();
                        }
                    }
                });
        });
        copy_result
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
    let start = selected / 16 * 16;
    if start.saturating_add(16) > color_count {
        return Err("selected color does not belong to a complete palette row".into());
    }
    Ok(native_clipboard::decode_palette_row(text)?
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
