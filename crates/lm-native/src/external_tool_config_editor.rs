use eframe::egui;
use lm_app::{ExternalTool, LocalizationCatalog, ToolEvent};
use std::path::PathBuf;

const ORIGINAL_EMULATOR_DIALOG_ID: u16 = 0x0407;

#[derive(Clone, Debug, Default)]
struct ToolDraft {
    id: String,
    name: String,
    executable: String,
    arguments: String,
    working_directory: String,
    project_opened: bool,
    project_saved: bool,
    level_changed: bool,
}

impl ToolDraft {
    fn from_tool(tool: &ExternalTool) -> Self {
        Self {
            id: tool.id.clone(),
            name: tool.name.clone(),
            executable: tool.executable.to_string_lossy().into_owned(),
            arguments: tool.arguments.join("\n"),
            working_directory: tool.working_directory.clone().unwrap_or_default(),
            project_opened: tool.subscriptions.contains(&ToolEvent::ProjectOpened),
            project_saved: tool.subscriptions.contains(&ToolEvent::ProjectSaved),
            level_changed: tool.subscriptions.contains(&ToolEvent::LevelChanged),
        }
    }

    fn emulator(index: usize) -> Self {
        Self {
            id: format!("emulator-{}", index + 1),
            name: "SNES Emulator".into(),
            arguments: "{rom}".into(),
            ..Self::default()
        }
    }

    fn build(&self) -> ExternalTool {
        let mut subscriptions = Vec::new();
        if self.project_opened {
            subscriptions.push(ToolEvent::ProjectOpened);
        }
        if self.project_saved {
            subscriptions.push(ToolEvent::ProjectSaved);
        }
        if self.level_changed {
            subscriptions.push(ToolEvent::LevelChanged);
        }
        ExternalTool {
            id: self.id.trim().into(),
            name: self.name.trim().into(),
            executable: PathBuf::from(self.executable.trim()),
            arguments: self.arguments.lines().map(str::to_owned).collect(),
            working_directory: (!self.working_directory.trim().is_empty())
                .then(|| self.working_directory.trim().to_owned()),
            subscriptions,
        }
    }
}

#[derive(Default)]
pub(crate) struct ExternalToolConfigEditor {
    open: bool,
    drafts: Vec<ToolDraft>,
    selected: usize,
    error: Option<String>,
}

impl ExternalToolConfigEditor {
    pub(crate) fn open(&mut self, tools: &[ExternalTool]) {
        self.drafts = tools.iter().map(ToolDraft::from_tool).collect();
        self.selected = self.selected.min(self.drafts.len().saturating_sub(1));
        self.error = None;
        self.open = true;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Vec<ExternalTool>> {
        if !self.open {
            return None;
        }
        let mut replacement = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new(dialog_title(catalog))
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(170.0);
                        for (index, draft) in self.drafts.iter().enumerate() {
                            let label = if draft.name.trim().is_empty() {
                                &draft.id
                            } else {
                                &draft.name
                            };
                            if ui.selectable_label(self.selected == index, label).clicked() {
                                self.selected = index;
                            }
                        }
                        if ui.button("Add emulator").clicked() {
                            self.drafts.push(ToolDraft::emulator(self.drafts.len()));
                            self.selected = self.drafts.len() - 1;
                        }
                        if ui
                            .add_enabled(!self.drafts.is_empty(), egui::Button::new("Remove"))
                            .clicked()
                        {
                            self.drafts.remove(self.selected);
                            self.selected = self.selected.min(self.drafts.len().saturating_sub(1));
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_min_width(400.0);
                        let Some(draft) = self.drafts.get_mut(self.selected) else {
                            ui.label("Add an emulator or external tool to begin.");
                            return;
                        };
                        field(ui, "Stable ID", &mut draft.id);
                        field(ui, "Display name", &mut draft.name);
                        field(
                            ui,
                            &dialog_control_text(catalog, 0x66, "Executable path"),
                            &mut draft.executable,
                        );
                        ui.label(dialog_control_text(catalog, 0x68, "Arguments"));
                        ui.small("One direct process argument per line; use {rom}, {project_dir}, {level_hex}, or {level_dec}.");
                        ui.add(egui::TextEdit::multiline(&mut draft.arguments).desired_rows(4));
                        field(ui, "Working directory template (optional)", &mut draft.working_directory);
                        ui.separator();
                        ui.label("Run automatically after:");
                        ui.checkbox(&mut draft.project_opened, "ROM opened");
                        ui.checkbox(&mut draft.project_saved, "ROM saved");
                        ui.checkbox(&mut draft.level_changed, "Level changed");
                    });
                });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(dialog_control_text(catalog, 2, "Cancel")).clicked() {
                        close = true;
                    }
                    if ui.button(dialog_control_text(catalog, 1, "Apply")).clicked() {
                        replacement = Some(self.drafts.iter().map(ToolDraft::build).collect());
                    }
                });
            });
        self.open = open && !close;
        replacement
    }

    pub(crate) fn applied(&mut self) {
        self.open = false;
        self.error = None;
    }

    pub(crate) fn rejected(&mut self, error: String) {
        self.error = Some(error);
    }
}

pub(crate) fn menu_text(catalog: Option<&LocalizationCatalog>) -> String {
    dialog_title(catalog)
}

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_EMULATOR_DIALOG_ID))
        .unwrap_or("Setup SNES Emulator…")
        .to_owned()
}

fn dialog_control_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| {
            catalog.original_dialog_control_text(ORIGINAL_EMULATOR_DIALOG_ID, control_id)
        })
        .unwrap_or(fallback)
        .to_owned()
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::{OriginalDialogTextKey, UiTextKey, validate_tools};

    #[test]
    fn draft_round_trip_preserves_argument_boundaries_working_directory_and_events() {
        let source = ExternalTool {
            id: "emu".into(),
            name: "Emulator".into(),
            executable: PathBuf::from("/Applications/Emu"),
            arguments: vec!["--fullscreen".into(), "{rom}".into()],
            working_directory: Some("{project_dir}".into()),
            subscriptions: vec![ToolEvent::ProjectSaved, ToolEvent::LevelChanged],
        };
        let rebuilt = ToolDraft::from_tool(&source).build();
        assert_eq!(rebuilt, source);
        validate_tools(&[rebuilt]).unwrap();
    }

    #[test]
    fn original_setup_emulator_inventory_maps_exact_controls_with_fallbacks() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_EMULATOR_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Configurer l’émulateur SNES".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_EMULATOR_DIALOG_ID,
                    item_index: 2,
                    control_id: 0x66,
                },
                "Chemin de l’émulateur :".into(),
            ),
        ])
        .unwrap();
        assert_eq!(menu_text(Some(&catalog)), "Configurer l’émulateur SNES");
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x66, "Executable path"),
            "Chemin de l’émulateur :"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x68, "Arguments"),
            "Arguments"
        );
    }
}
