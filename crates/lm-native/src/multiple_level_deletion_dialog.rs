use eframe::egui;
use lm_app::{AppState, Command, LevelDeletionPartition, UiTextKey};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DeletionMode {
    #[default]
    Modified,
    Unmodified,
    All,
}

#[derive(Default)]
pub(crate) struct MultipleLevelDeletionDialog {
    open: bool,
    partition: Option<LevelDeletionPartition>,
    mode: DeletionMode,
    clear_original_level_area: bool,
    clear_only: bool,
}

pub(crate) struct MultipleLevelDeletionRequest {
    pub(crate) levels: Vec<u16>,
    pub(crate) command: Command,
}

impl MultipleLevelDeletionDialog {
    pub(crate) fn open(&mut self, app: &AppState) -> Result<(), String> {
        self.partition = Some(
            app.level_deletion_partition()
                .map_err(|error| error.to_string())?,
        );
        self.mode = DeletionMode::Modified;
        self.clear_original_level_area = false;
        self.clear_only = false;
        self.open = true;
        Ok(())
    }

    pub(crate) fn open_clear_original_level_area(&mut self, app: &AppState) -> Result<(), String> {
        self.partition = Some(
            app.level_deletion_partition()
                .map_err(|error| error.to_string())?,
        );
        self.mode = DeletionMode::Unmodified;
        self.clear_original_level_area = true;
        self.clear_only = true;
        self.open = true;
        Ok(())
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    fn selected_levels(&self) -> Vec<u16> {
        let Some(partition) = &self.partition else {
            return Vec::new();
        };
        let mut levels = match self.mode {
            DeletionMode::Modified => partition.modified.clone(),
            DeletionMode::Unmodified => partition.unmodified.clone(),
            DeletionMode::All => {
                let mut levels = partition.modified.clone();
                levels.extend_from_slice(&partition.unmodified);
                levels.sort_unstable();
                levels
            }
        };
        levels.sort_unstable();
        levels
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
    ) -> Option<MultipleLevelDeletionRequest> {
        if !self.open {
            return None;
        }
        let mut request = None;
        let counts = self.partition.as_ref().map(|partition| {
            (
                partition.modified.len(),
                partition.unmodified.len(),
                partition.modified.len() + partition.unmodified.len(),
            )
        })?;
        let localize = |key: UiTextKey| {
            app.localization().map_or_else(
                || key.english().to_owned(),
                |catalog| catalog.text(key).to_owned(),
            )
        };
        let title = if self.clear_only {
            localize(UiTextKey::FileClearOriginalLevelArea)
        } else {
            localize(UiTextKey::DeleteMultipleLevelsWindowTitle)
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                if self.clear_only {
                    ui.label(localize(UiTextKey::ClearOriginalLevelAreaDescription));
                } else {
                    ui.label(localize(UiTextKey::DeleteMultipleLevelsDescription));
                    ui.separator();
                    ui.radio_value(
                        &mut self.mode,
                        DeletionMode::Modified,
                        format!(
                            "{} ({})",
                            localize(UiTextKey::DeleteMultipleLevelsModified),
                            counts.0
                        ),
                    );
                    ui.radio_value(
                        &mut self.mode,
                        DeletionMode::Unmodified,
                        format!(
                            "{} ({})",
                            localize(UiTextKey::DeleteMultipleLevelsUnmodified),
                            counts.1
                        ),
                    );
                    ui.radio_value(
                        &mut self.mode,
                        DeletionMode::All,
                        format!(
                            "{} ({})",
                            localize(UiTextKey::DeleteMultipleLevelsAll),
                            counts.2
                        ),
                    );
                    let can_clear = self.mode != DeletionMode::Modified;
                    if !can_clear {
                        self.clear_original_level_area = false;
                    }
                    ui.add_enabled_ui(can_clear, |ui| {
                        ui.checkbox(
                            &mut self.clear_original_level_area,
                            localize(UiTextKey::DeleteMultipleLevelsClearOriginal),
                        );
                    });
                }
                ui.separator();
                if !self.clear_only {
                    ui.label(localize(UiTextKey::DeleteMultipleLevelsDependencyWarning));
                }
                let selected = self.selected_levels();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !selected.is_empty(),
                            egui::Button::new(localize(UiTextKey::CommonDelete)),
                        )
                        .clicked()
                    {
                        request = Some(MultipleLevelDeletionRequest {
                            command: Command::DeleteLevels {
                                rev: app.project_revision(),
                                levels: selected.clone(),
                                clear_original_level_area: self.clear_original_level_area,
                            },
                            levels: selected,
                        });
                    }
                    if ui.button(localize(UiTextKey::CommonCancel)).clicked() {
                        self.open = false;
                    }
                });
            });
        if request.is_some() {
            self.open = false;
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_selection_is_exact_sorted_and_clear_is_never_attached_to_modified_only() {
        let mut dialog = MultipleLevelDeletionDialog {
            open: true,
            partition: Some(LevelDeletionPartition {
                modified: vec![0x101, 0],
                unmodified: vec![0x100, 1],
            }),
            mode: DeletionMode::Modified,
            clear_original_level_area: true,
            clear_only: false,
        };
        assert_eq!(dialog.selected_levels(), vec![0, 0x101]);
        dialog.mode = DeletionMode::Unmodified;
        assert_eq!(dialog.selected_levels(), vec![1, 0x100]);
        dialog.mode = DeletionMode::All;
        assert_eq!(dialog.selected_levels(), vec![0, 1, 0x100, 0x101]);
    }
}
