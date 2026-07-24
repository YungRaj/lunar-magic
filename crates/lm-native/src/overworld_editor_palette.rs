use crate::native_clipboard;
use eframe::egui;
use lm_app::OverworldControllerEdit;
use lm_graphics::{Bgr555, Palette, PaletteChange, PaletteEntryOwner, PaletteOwnership, Rgb8};

#[derive(Default)]
pub(crate) struct OverworldPalettePanel {
    selected: usize,
}

impl OverworldPalettePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        self.selected = self.selected.min(palette.colors.len().saturating_sub(1));
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("overworld-palette-grid")
                .spacing([3.0, 3.0])
                .show(ui, |ui| {
                    for (index, color) in palette.colors.iter().copied().enumerate() {
                        let rgb = color.to_rgb8();
                        let button = egui::Button::new("  ")
                            .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue))
                            .selected(index == self.selected);
                        if ui.add_sized([24.0, 24.0], button).clicked() {
                            self.selected = index;
                        }
                        if index % 16 == 15 {
                            ui.end_row();
                        }
                    }
                });
        });
        let color = palette.colors.get(self.selected).copied()?;
        ui.label(format!(
            "Color {:03X} — BGR555 {:04X}",
            self.selected, color.0
        ));
        let owner = ownership.owner(self.selected);
        let editable = owner == Some(PaletteEntryOwner::Editable);
        ui.label(match owner {
            Some(PaletteEntryOwner::Editable) => "Ownership: editable".into(),
            Some(PaletteEntryOwner::Fixed) => "Ownership: fixed (read-only)".into(),
            Some(PaletteEntryOwner::ExAnimation { record }) => {
                format!("Ownership: ExAnimation record {record:04X} (read-only)")
            }
            None => "Ownership: invalid (read-only)".into(),
        });
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui.button("Copy color").clicked() {
                copy_result = Some(native_clipboard::encode_palette_color(color));
            }
            if ui
                .add_enabled(editable, egui::Button::new("Paste color"))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
        }
        if editable && let Some(text) = pasted_text(ui) {
            return Some(native_clipboard::decode_palette_color(&text).map(|color| {
                OverworldControllerEdit::PaletteChanges(vec![PaletteChange {
                    index: self.selected,
                    color,
                }])
            }));
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
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}
