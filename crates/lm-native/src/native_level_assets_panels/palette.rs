use super::{AggregatePanels, PasteTarget, pasted_text, text};
use crate::native_clipboard;
use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, NativeLevelAssetsControllerEdit,
    PaletteControllerEdit,
};
use lm_graphics::{Bgr555, PaletteChange, PaletteEntryOwner, PaletteOwnership, Rgb8};
use lm_project::NativeLevelAssetsFile;

enum NativePaste {
    Color(Bgr555),
    Row([Bgr555; 16]),
}

impl AggregatePanels {
    pub(super) fn palette_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        ownership: &PaletteOwnership,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let colors = &file.assets.palette.colors;
        self.selected_color = self.selected_color.min(colors.len().saturating_sub(1));
        let (mut copy_result, grid_paste) = self.palette_clipboard_grid(ui, colors);
        let mut native_paste = None;
        let color = colors.get(self.selected_color).copied()?;
        let rgb = color.to_rgb8();
        let mut value = [rgb.red, rgb.green, rgb.blue];
        ui.label(
            text(catalog, Key::NativeAssetsPaletteColorFormat)
                .replace("{index}", &format!("{:03X}", self.selected_color))
                .replace("{value}", &format!("{:04X}", color.0)),
        );
        let owner = ownership.owner(self.selected_color);
        let editable = owner == Some(PaletteEntryOwner::Editable);
        let row = palette_row(colors, self.selected_color).ok();
        let row_editable = row.is_some_and(|_| {
            let start = self.selected_color / 16 * 16;
            (start..start + 16)
                .all(|index| ownership.owner(index) == Some(PaletteEntryOwner::Editable))
        });
        if let Some(result) = grid_paste {
            native_paste = Some(result.and_then(|paste| match paste {
                NativePaste::Color(color) if editable => Ok(vec![PaletteChange {
                    index: self.selected_color,
                    color,
                }]),
                NativePaste::Row(row_colors) if row_editable => {
                    row_color_changes(row_colors, self.selected_color, colors.len())
                }
                _ => Err("selected palette entries are read-only".into()),
            }));
        }
        ui.label(match owner {
            Some(PaletteEntryOwner::Editable) => {
                text(catalog, Key::NativeAssetsPaletteOwnershipEditable)
            }
            Some(PaletteEntryOwner::Fixed) => text(catalog, Key::NativeAssetsPaletteOwnershipFixed),
            Some(PaletteEntryOwner::ExAnimation { record }) => {
                text(catalog, Key::NativeAssetsPaletteOwnershipExAnimationFormat)
                    .replace("{record}", &format!("{record:04X}"))
            }
            None => text(catalog, Key::NativeAssetsPaletteOwnershipInvalid),
        });
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, Key::NativeAssetsPaletteCopyColor))
                .clicked()
            {
                copy_result = Some(native_clipboard::copy_palette_color_to_system(
                    ui.ctx(),
                    color,
                ));
            }
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(text(catalog, Key::NativeAssetsPalettePasteColor)),
                )
                .clicked()
            {
                match native_clipboard::request_palette_color_paste(ui.ctx()) {
                    Ok(Some(color)) => {
                        native_paste = Some(Ok(vec![PaletteChange {
                            index: self.selected_color,
                            color,
                        }]))
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::PaletteColor),
                    Err(error) => native_paste = Some(Err(error)),
                }
            }
            if ui
                .add_enabled(
                    row.is_some(),
                    egui::Button::new(text(catalog, Key::NativeAssetsPaletteCopyRow)),
                )
                .clicked()
            {
                copy_result = Some(native_clipboard::copy_palette_row_to_system(
                    ui.ctx(),
                    row.expect("enabled row is complete"),
                ));
            }
            if ui
                .add_enabled(
                    row_editable,
                    egui::Button::new(text(catalog, Key::NativeAssetsPalettePasteRow)),
                )
                .clicked()
            {
                match native_clipboard::request_palette_row_paste(ui.ctx()) {
                    Ok(Some(row_colors)) => {
                        native_paste = Some(row_color_changes(
                            row_colors,
                            self.selected_color,
                            colors.len(),
                        ))
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::PaletteRow),
                    Err(error) => native_paste = Some(Err(error)),
                }
            }
        });
        ui.small(text(catalog, Key::NativeAssetsPaletteShortcutNotice));
        if let Some(result) = copy_result {
            match result {
                Ok(()) => {}
                Err(error) => return Some(Err(error)),
            }
        }
        if let Some(changes) = native_paste {
            return Some(changes.map(|changes| {
                NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                    changes,
                )])
            }));
        }
        if let Some(text) = pasted_text(ui) {
            let changes = match self.paste_target.take() {
                Some(PasteTarget::PaletteColor) if editable => {
                    native_clipboard::decode_palette_color(&text).map(|color| {
                        vec![PaletteChange {
                            index: self.selected_color,
                            color,
                        }]
                    })
                }
                Some(PasteTarget::PaletteRow) => {
                    palette_row_changes(&text, self.selected_color, colors.len())
                }
                _ => return None,
            };
            return Some(changes.map(|changes| {
                NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                    changes,
                )])
            }));
        }
        ui.add_enabled_ui(editable, |ui| ui.color_edit_button_srgb(&mut value))
            .inner
            .changed()
            .then(|| {
                Ok(NativeLevelAssetsControllerEdit::Palette(vec![
                    PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                        index: self.selected_color,
                        color: Bgr555::from_rgb8(Rgb8 {
                            red: value[0],
                            green: value[1],
                            blue: value[2],
                        }),
                    }]),
                ]))
            })
    }

    fn palette_clipboard_grid(
        &mut self,
        ui: &mut egui::Ui,
        colors: &[Bgr555],
    ) -> (
        Option<Result<(), String>>,
        Option<Result<NativePaste, String>>,
    ) {
        let mut copy_result = None;
        let mut native_paste = None;
        egui::Grid::new("aggregate-palette").show(ui, |ui| {
            for (index, color) in colors.iter().copied().enumerate() {
                let rgb = color.to_rgb8();
                let response = ui.add_sized(
                    [22.0, 22.0],
                    egui::Button::new("")
                        .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)),
                );
                if response.clicked() {
                    self.selected_color = index;
                    let modifiers = ui.input(|input| input.modifiers);
                    if modifiers.ctrl {
                        copy_result = Some(if modifiers.alt {
                            palette_row(colors, index).and_then(|row| {
                                native_clipboard::copy_palette_row_to_system(ui.ctx(), row)
                            })
                        } else {
                            native_clipboard::copy_palette_color_to_system(ui.ctx(), color)
                        });
                    }
                }
                if response.secondary_clicked() && ui.input(|input| input.modifiers.ctrl) {
                    self.selected_color = index;
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
                                PasteTarget::PaletteRow
                            } else {
                                PasteTarget::PaletteColor
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
        (copy_result, native_paste)
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

#[cfg(test)]
mod tests {
    use super::palette_row_changes;
    use crate::native_clipboard;
    use lm_graphics::Bgr555;

    #[test]
    fn complete_aggregate_palette_panel_has_no_literal_widget_text() {
        let source = include_str!("palette.rs");
        for literal_widget in [
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "ui.small(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "aggregate Palette panel regressed to fixed widget text: {literal_widget}"
            );
        }
        assert_eq!(source.matches("Button::new(\"\")").count(), 1);
    }

    #[test]
    fn aggregate_row_paste_is_aligned_and_complete() {
        let row = [Bgr555(0x1234); 16];
        let text = native_clipboard::encode_palette_row(&row).unwrap();
        let changes = palette_row_changes(&text, 31, 32).unwrap();
        assert_eq!((changes[0].index, changes[15].index), (16, 31));
        assert!(palette_row_changes(&text, 32, 33).is_err());
    }
}
