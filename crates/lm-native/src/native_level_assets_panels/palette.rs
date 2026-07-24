use super::{AggregatePanels, PasteTarget, pasted_text};
use crate::native_clipboard;
use eframe::egui;
use lm_app::{NativeLevelAssetsControllerEdit, PaletteControllerEdit};
use lm_graphics::{Bgr555, PaletteChange, PaletteEntryOwner, PaletteOwnership, Rgb8};
use lm_project::NativeLevelAssetsFile;

impl AggregatePanels {
    pub(super) fn palette_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        ownership: &PaletteOwnership,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let colors = &file.assets.palette.colors;
        self.selected_color = self.selected_color.min(colors.len().saturating_sub(1));
        egui::Grid::new("aggregate-palette").show(ui, |ui| {
            for (i, color) in colors.iter().copied().enumerate() {
                let rgb = color.to_rgb8();
                if ui
                    .add_sized(
                        [22.0, 22.0],
                        egui::Button::new("")
                            .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)),
                    )
                    .clicked()
                {
                    self.selected_color = i;
                }
                if i % 16 == 15 {
                    ui.end_row();
                }
            }
        });
        let color = colors.get(self.selected_color).copied()?;
        let rgb = color.to_rgb8();
        let mut value = [rgb.red, rgb.green, rgb.blue];
        ui.label(format!(
            "Color {:03X} / {:04X}",
            self.selected_color, color.0
        ));
        let owner = ownership.owner(self.selected_color);
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
                self.paste_target = Some(PasteTarget::PaletteColor);
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
        if editable
            && self.paste_target == Some(PasteTarget::PaletteColor)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            return Some(native_clipboard::decode_palette_color(&text).map(|color| {
                NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                    vec![PaletteChange {
                        index: self.selected_color,
                        color,
                    }],
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
}
