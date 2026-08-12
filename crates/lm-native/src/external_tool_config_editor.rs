use eframe::egui;
use lm_app::{ExternalTool, LocalizationCatalog, ToolEvent};
use std::path::PathBuf;

const ORIGINAL_SNES_EMULATOR_DIALOG_ID: u16 = 0x0407;
const ORIGINAL_GBA_EMULATOR_DIALOG_ID: u16 = 0x0408;
const ORIGINAL_TILE_EDITOR_DIALOG_ID: u16 = 0x0409;

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
    use_short_rom_path: bool,
    replace_tile_editor_palette: bool,
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
            use_short_rom_path: tool
                .arguments
                .iter()
                .any(|argument| argument.contains("{rom_8dot3}")),
            replace_tile_editor_palette: tool.replace_tile_editor_palette,
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

    fn gba_emulator(index: usize) -> Self {
        Self {
            id: format!("gba-emulator-{}", index + 1),
            name: "GBA Emulator".into(),
            arguments: "{rom}".into(),
            ..Self::default()
        }
    }

    fn tile_editor(index: usize) -> Self {
        Self {
            id: format!("tile-editor-{}", index + 1),
            name: "Tile Editor".into(),
            arguments: "{graphics}".into(),
            ..Self::default()
        }
    }

    fn original_dialog_id(&self) -> u16 {
        let id = self.id.trim().to_ascii_lowercase();
        if id.contains("tile-editor") {
            ORIGINAL_TILE_EDITOR_DIALOG_ID
        } else if id.starts_with("gba-") {
            ORIGINAL_GBA_EMULATOR_DIALOG_ID
        } else {
            ORIGINAL_SNES_EMULATOR_DIALOG_ID
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
            arguments: self
                .arguments
                .lines()
                .map(|argument| {
                    if self.use_short_rom_path {
                        argument.replace("{rom}", "{rom_8dot3}")
                    } else {
                        argument.replace("{rom_8dot3}", "{rom}")
                    }
                })
                .collect(),
            working_directory: (!self.working_directory.trim().is_empty())
                .then(|| self.working_directory.trim().to_owned()),
            subscriptions,
            replace_tile_editor_palette: self.replace_tile_editor_palette,
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
        let dialog_id = self.drafts.get(self.selected).map_or(
            ORIGINAL_SNES_EMULATOR_DIALOG_ID,
            ToolDraft::original_dialog_id,
        );
        egui::Window::new(dialog_title(catalog, dialog_id))
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
                        if ui.button("Add SNES emulator").clicked() {
                            self.drafts.push(ToolDraft::emulator(self.drafts.len()));
                            self.selected = self.drafts.len() - 1;
                        }
                        if ui.button("Add GBA emulator").clicked() {
                            self.drafts
                                .push(ToolDraft::gba_emulator(self.drafts.len()));
                            self.selected = self.drafts.len() - 1;
                        }
                        if ui.button("Add tile editor").clicked() {
                            self.drafts.push(ToolDraft::tile_editor(self.drafts.len()));
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
                            &dialog_control_text(catalog, dialog_id, 0x66, "Executable path"),
                            &mut draft.executable,
                        );
                        ui.label(dialog_control_text(catalog, dialog_id, 0x68, "Arguments"));
                        ui.small("One direct process argument per line; use {rom}, {project_dir}, {level_hex}, or {level_dec}.");
                        ui.add(egui::TextEdit::multiline(&mut draft.arguments).desired_rows(4));
                        if dialog_id == ORIGINAL_TILE_EDITOR_DIALOG_ID {
                            ui.radio_value(
                                &mut draft.replace_tile_editor_palette,
                                true,
                                dialog_control_text(
                                    catalog,
                                    dialog_id,
                                    0x67,
                                    "Replace yychr.pal file with current palette.",
                                ),
                            );
                            ui.radio_value(
                                &mut draft.replace_tile_editor_palette,
                                false,
                                dialog_control_text(
                                    catalog,
                                    dialog_id,
                                    0x6b,
                                    "Set transparent colors to blue.",
                                ),
                            );
                        } else {
                            ui.checkbox(
                                &mut draft.use_short_rom_path,
                                dialog_control_text(
                                    catalog,
                                    dialog_id,
                                    0x67,
                                    "Use Windows 8.3 short path for ROM",
                                ),
                            );
                        }
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
                    if ui
                        .button(dialog_control_text(catalog, dialog_id, 2, "Cancel"))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .button(dialog_control_text(catalog, dialog_id, 1, "Apply"))
                        .clicked()
                    {
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
    dialog_title(catalog, ORIGINAL_SNES_EMULATOR_DIALOG_ID)
}

fn dialog_title(catalog: Option<&LocalizationCatalog>, dialog_id: u16) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(dialog_id))
        .unwrap_or(match dialog_id {
            ORIGINAL_GBA_EMULATOR_DIALOG_ID => "Setup GBA Emulator…",
            ORIGINAL_TILE_EDITOR_DIALOG_ID => "Setup Tile Editor…",
            _ => "Setup SNES Emulator…",
        })
        .to_owned()
}

fn dialog_control_text(
    catalog: Option<&LocalizationCatalog>,
    dialog_id: u16,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(dialog_id, control_id))
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
            replace_tile_editor_palette: false,
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
                    dialog_id: ORIGINAL_SNES_EMULATOR_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Configurer l’émulateur SNES".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_SNES_EMULATOR_DIALOG_ID,
                    item_index: 2,
                    control_id: 0x66,
                },
                "Chemin de l’émulateur :".into(),
            ),
        ])
        .unwrap();
        assert_eq!(menu_text(Some(&catalog)), "Configurer l’émulateur SNES");
        assert_eq!(
            dialog_control_text(
                Some(&catalog),
                ORIGINAL_SNES_EMULATOR_DIALOG_ID,
                0x66,
                "Executable path"
            ),
            "Chemin de l’émulateur :"
        );
        assert_eq!(
            dialog_control_text(
                Some(&catalog),
                ORIGINAL_SNES_EMULATOR_DIALOG_ID,
                0x68,
                "Arguments"
            ),
            "Arguments"
        );
    }

    #[test]
    fn gba_profiles_persist_their_kind_and_select_dialog_0408() {
        let draft = ToolDraft::gba_emulator(2);
        let tool = draft.build();
        let reopened = ToolDraft::from_tool(&tool);
        assert_eq!(
            reopened.original_dialog_id(),
            ORIGINAL_GBA_EMULATOR_DIALOG_ID
        );
        assert_eq!(
            dialog_title(None, reopened.original_dialog_id()),
            "Setup GBA Emulator…"
        );
        assert_eq!(tool.id, "gba-emulator-3");
        assert_eq!(tool.arguments, ["{rom}"]);
    }

    #[test]
    fn tile_editor_profiles_persist_identity_and_select_original_dialog_0409() {
        let draft = ToolDraft::tile_editor(1);
        assert!(!draft.replace_tile_editor_palette);
        let tool = draft.build();
        let reopened = ToolDraft::from_tool(&tool);
        assert_eq!(
            reopened.original_dialog_id(),
            ORIGINAL_TILE_EDITOR_DIALOG_ID
        );
        assert_eq!(
            dialog_title(None, reopened.original_dialog_id()),
            "Setup Tile Editor…"
        );
        assert_eq!(tool.id, "tile-editor-2");
        assert_eq!(tool.arguments, ["{graphics}"]);
    }

    #[test]
    fn original_tile_editor_template_localizes_path_arguments_and_actions() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_TILE_EDITOR_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Configurer l’éditeur de tuiles".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_TILE_EDITOR_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x66,
                },
                "Chemin de l’éditeur :".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_TILE_EDITOR_DIALOG_ID,
                    item_index: 2,
                    control_id: 1,
                },
                "Valider".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_TILE_EDITOR_DIALOG_ID,
                    item_index: 3,
                    control_id: 0x67,
                },
                "Remplacer la palette YY-CHR".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_TILE_EDITOR_DIALOG_ID,
                    item_index: 4,
                    control_id: 0x6b,
                },
                "Afficher la transparence en bleu".into(),
            ),
        ])
        .unwrap();

        assert_eq!(
            dialog_title(Some(&catalog), ORIGINAL_TILE_EDITOR_DIALOG_ID),
            "Configurer l’éditeur de tuiles"
        );
        assert_eq!(
            dialog_control_text(
                Some(&catalog),
                ORIGINAL_TILE_EDITOR_DIALOG_ID,
                0x66,
                "Executable path"
            ),
            "Chemin de l’éditeur :"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), ORIGINAL_TILE_EDITOR_DIALOG_ID, 1, "Apply"),
            "Valider"
        );
        assert_eq!(
            dialog_control_text(
                Some(&catalog),
                ORIGINAL_TILE_EDITOR_DIALOG_ID,
                0x67,
                "Replace"
            ),
            "Remplacer la palette YY-CHR"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), ORIGINAL_TILE_EDITOR_DIALOG_ID, 0x6b, "Blue"),
            "Afficher la transparence en bleu"
        );

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(
            dialog_title(Some(&reopened), ORIGINAL_TILE_EDITOR_DIALOG_ID),
            "Configurer l’éditeur de tuiles"
        );
    }

    #[test]
    fn short_path_choice_persists_compatibly_inside_argument_templates() {
        let mut draft = ToolDraft::emulator(0);
        draft.arguments = "--rom={rom}\n--literal".into();
        draft.use_short_rom_path = true;
        let tool = draft.build();
        assert_eq!(tool.arguments, ["--rom={rom_8dot3}", "--literal"]);
        let reopened = ToolDraft::from_tool(&tool);
        assert!(reopened.use_short_rom_path);
        let mut restored = reopened;
        restored.use_short_rom_path = false;
        assert_eq!(restored.build().arguments, ["--rom={rom}", "--literal"]);
    }
}
