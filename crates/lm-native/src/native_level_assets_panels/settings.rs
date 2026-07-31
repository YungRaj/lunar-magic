use super::AggregatePanels;
use crate::level_editor_forms;
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_level::{ExpandedLevelHeader, SuperGraphicsBypass};
use lm_project::NativeLevelAssetsFile;

impl AggregatePanels {
    pub(super) fn settings_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        if file.assets.expanded_settings.is_none() {
            ui.label("This aggregate has no expanded-settings record.");
            return None;
        }
        ui.heading("Super GFX Bypass");
        ui.checkbox(&mut self.bypass_enabled, "Use per-level GFX/ExGFX files");
        egui::Grid::new("aggregate-super-gfx")
            .num_columns(4)
            .show(ui, |ui| {
                for (slot, label) in ["FG1", "FG2", "FG3", "BG1", "BG2", "BG3"]
                    .into_iter()
                    .enumerate()
                {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.bypass_foreground_background[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
                for (slot, label) in ["SP1", "SP2", "SP3", "SP4"].into_iter().enumerate() {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.bypass_sprites[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if ui.button("Apply Super GFX bypass").clicked() {
            let settings = file
                .assets
                .expanded_settings
                .as_ref()
                .expect("presence checked above");
            let mut header = ExpandedLevelHeader::from(settings);
            let result = header
                .set_super_graphics_bypass(SuperGraphicsBypass {
                    enabled: self.bypass_enabled,
                    foreground_background: self.bypass_foreground_background,
                    sprites: self.bypass_sprites,
                })
                .map_err(|error| error.to_string())
                .map(|()| {
                    [0].into_iter()
                        .chain(2..=11)
                        .map(|index| (index, header.fields[index]))
                        .collect()
                })
                .map(NativeLevelAssetsControllerEdit::ExpandedSettingsWords);
            return Some(result);
        }
        ui.separator();
        ui.label("Raw expanded words (unproven fields remain editable and lossless):");
        egui::Grid::new("aggregate-settings").show(ui, |ui| {
            for (index, value) in self.settings.iter_mut().enumerate() {
                ui.label(format!("Word {index:X}"));
                ui.text_edit_singleline(value);
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.button("Apply all words").clicked().then(|| {
            self.settings
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    level_editor_forms::parse_hex_u16(value, &format!("settings word {index:X}"))
                        .map(|word| (index, word))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NativeLevelAssetsControllerEdit::ExpandedSettingsWords)
        })
    }
}
