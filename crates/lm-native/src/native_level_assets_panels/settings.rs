use super::AggregatePanels;
use crate::level_editor_forms;
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
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
