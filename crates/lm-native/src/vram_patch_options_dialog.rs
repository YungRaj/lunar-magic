use eframe::egui;
use lm_app::{AppState, Command, ControllerSnapshot, ExtendedUiTextKey, LocalizationCatalog};
use lm_profile::{
    SmwUsV1VramPatchState, detect_smw_us_v1_vram_patch,
};
use lm_project::{Project, RomMutation};
use lm_rom::RomImage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VramPatchSelection {
    None,
    Normal,
    Hd16x9,
    Hd21x9,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VramPatchDialogModel {
    selected: VramPatchSelection,
    none_enabled: bool,
    normal_enabled: bool,
    hd_enabled: bool,
    recognized: bool,
}

#[derive(Default)]
pub(crate) struct VramPatchOptionsDialog {
    open: bool,
    model: Option<VramPatchDialogModel>,
    pending: Option<VramPatchSelection>,
    error: Option<String>,
}

impl VramPatchOptionsDialog {
    pub(crate) fn open(&mut self, app: &AppState, pending: Option<VramPatchSelection>) {
        match detect(app) {
            Ok(state) => {
                let mut model = dialog_model(&state);
                if let Some(selection) = pending
                    && selection_enabled(&model, selection)
                {
                    model.selected = selection;
                }
                self.model = Some(model);
                self.error = None;
                self.open = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if self.open {
            egui::Window::new(text(catalog, ExtendedUiTextKey::VramPatchTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(text(catalog, ExtendedUiTextKey::VramPatchDescription));
                    ui.label(text(catalog, ExtendedUiTextKey::VramPatchDeferredNotice));
                    ui.separator();
                    if let Some(model) = &mut self.model {
                        ui.group(|ui| {
                            ui.label(text(catalog, ExtendedUiTextKey::VramPatchType));
                            ui.add_enabled_ui(model.none_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::None,
                                    text(catalog, ExtendedUiTextKey::VramPatchNone),
                                )
                                .on_hover_text(text(catalog, ExtendedUiTextKey::VramPatchNoneHelp));
                            });
                            ui.add_enabled_ui(model.normal_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Normal,
                                    text(catalog, ExtendedUiTextKey::VramPatchNormal),
                                )
                                .on_hover_text(text(
                                    catalog,
                                    ExtendedUiTextKey::VramPatchNormalHelp,
                                ));
                            });
                            ui.add_enabled_ui(model.hd_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Hd16x9,
                                    text(catalog, ExtendedUiTextKey::VramPatchHd16x9),
                                );
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Hd21x9,
                                    text(catalog, ExtendedUiTextKey::VramPatchHd21x9),
                                );
                            });
                        });
                        if !model.recognized {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                text(catalog, ExtendedUiTextKey::VramPatchUnknownNotice),
                            );
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::VramPatchCancel))
                            .clicked()
                        {
                            self.open = false;
                        }
                        let can_confirm = self.model.is_some_and(|model| model.recognized);
                        if ui
                            .add_enabled(
                                can_confirm,
                                egui::Button::new(text(catalog, ExtendedUiTextKey::VramPatchOk)),
                            )
                            .clicked()
                        {
                            self.pending = self.model.map(|model| model.selected);
                            self.open = false;
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::VramPatchErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::VramPatchOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    pub(crate) fn take_pending(&mut self) -> Option<VramPatchSelection> {
        self.pending.take()
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

pub(crate) fn effective_selection(app: &AppState) -> Option<VramPatchSelection> {
    match detect(app).ok()? {
        // Absence is not a user choice. Installing this optional runtime as a side effect of an
        // ordinary object/sprite save made otherwise valid edited ROMs fail during the title
        // transition. The dialog may still default its explicit radio choice to Normal, but a
        // level commit remains patch-free until the user confirms that choice.
        SmwUsV1VramPatchState::Absent => None,
        SmwUsV1VramPatchState::Installed { .. } => None,
        SmwUsV1VramPatchState::Unknown { .. } => None,
    }
}

const fn selection_enabled(model: &VramPatchDialogModel, selection: VramPatchSelection) -> bool {
    match selection {
        VramPatchSelection::None => model.none_enabled,
        VramPatchSelection::Normal => model.normal_enabled,
        VramPatchSelection::Hd16x9 | VramPatchSelection::Hd21x9 => model.hd_enabled,
    }
}

fn detect(app: &AppState) -> Result<SmwUsV1VramPatchState, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let rom = RomImage::from_bytes(snapshot.rom_bytes).map_err(|error| error.to_string())?;
    detect_smw_us_v1_vram_patch(&rom).map_err(|error| error.to_string())
}

fn dialog_model(state: &SmwUsV1VramPatchState) -> VramPatchDialogModel {
    match state {
        SmwUsV1VramPatchState::Absent => VramPatchDialogModel {
            selected: VramPatchSelection::Normal,
            none_enabled: true,
            normal_enabled: true,
            hd_enabled: false,
            recognized: true,
        },
        SmwUsV1VramPatchState::Installed { version, .. } => VramPatchDialogModel {
            selected: match version {
                2 => VramPatchSelection::Hd16x9,
                3 => VramPatchSelection::Hd21x9,
                _ => VramPatchSelection::Normal,
            },
            none_enabled: false,
            normal_enabled: true,
            hd_enabled: false,
            recognized: true,
        },
        SmwUsV1VramPatchState::Unknown { .. } => VramPatchDialogModel {
            selected: VramPatchSelection::Normal,
            none_enabled: false,
            normal_enabled: false,
            hd_enabled: false,
            recognized: false,
        },
    }
}

/// Composes the deferred choice with one prepared level save as a single revision-bound ROM
/// mutation. A failed runtime install leaves both the level edit and the active project unchanged.
pub(crate) fn prepare_level_save_command(
    snapshot: &ControllerSnapshot,
    selection: VramPatchSelection,
    command: Command,
) -> Result<Command, String> {
    if selection != VramPatchSelection::Normal {
        return Ok(command);
    }
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let (expected_revision, description, mutation) = match command {
        Command::CommitRomMutation {
            expected_revision,
            description,
            mutation,
        } => (expected_revision, description, mutation),
        Command::CommitRomWrites {
            expected_revision,
            description,
            writes,
        } => (
            expected_revision,
            description,
            RomMutation {
                mapper: snapshot.identity.mapper,
                expected_len: image.logical_len(),
                appended: Vec::new(),
                writes,
            },
        ),
        _ => return Err("VRAM patch selection can only be applied by a level ROM commit".into()),
    };
    if expected_revision != snapshot.revision {
        return Err(format!(
            "stale level save revision: expected {expected_revision}, snapshot is {}",
            snapshot.revision
        ));
    }

    let original = image.logical_bytes().to_vec();
    let mut staged = Project::new(image);
    staged
        .apply_mutation("stage level save with deferred VRAM option", &mutation)
        .map_err(|error| error.to_string())?;
    match detect_smw_us_v1_vram_patch(&staged.rom).map_err(|error| error.to_string())? {
        SmwUsV1VramPatchState::Absent => {
            return Err(
                "Normal VRAM patch installation is disabled because the standalone runtime does not yet pass post-title gameplay verification; the level edit was not changed"
                    .into(),
            );
        }
        SmwUsV1VramPatchState::Installed {
            requires_replacement: false,
            ..
        } => {}
        SmwUsV1VramPatchState::Installed {
            requires_replacement: true,
            ..
        } => {
            return Err(
                "Normal VRAM patch replacement is disabled until the standalone runtime passes post-title gameplay verification; the level edit was not changed"
                    .into(),
            );
        }
        SmwUsV1VramPatchState::Unknown { .. } => {
            return Err(
                "the installed VRAM patch is not recognized; no level or patch bytes were changed"
                    .into(),
            );
        }
    }

    let combined = RomMutation::between(
        snapshot.identity.mapper,
        &original,
        staged.rom.logical_bytes(),
    )
    .map_err(|error| error.to_string())?;
    Ok(Command::CommitRomMutation {
        expected_revision,
        description: format!("{description} and apply Normal VRAM patch"),
        mutation: combined,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::RomMutation;
    use lm_rats::{RatsBlock, make_header};
    use lm_rom::{Mapper, pc_to_snes};
    use std::{fs, path::PathBuf};

    #[test]
    fn complete_vram_patch_form_uses_every_typed_key() {
        let sources = [
            include_str!("vram_patch_options_dialog.rs"),
            include_str!("application/rom_windows.rs"),
        ]
        .concat();
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("VramPatch"))
        {
            assert!(sources.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for literal in [
            "Window::new(\"Change VRAM Patch Options\")",
            "ui.button(\"Cancel\")",
            "Window::new(\"VRAM patch options error\")",
        ] {
            assert!(!sources.contains(literal));
        }
    }

    fn vanilla_bytes() -> Vec<u8> {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("oracle-work/lm363/pristine-us/level-save-000/before.smc"),
        )
        .unwrap()
    }

    fn old_or_unknown_runtime_bytes(recognized: bool) -> Vec<u8> {
        let image = RomImage::from_bytes(vanilla_bytes()).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        bytes.resize(0x10_0000, 0xff);
        let payload = 0x08_1000;
        let mut body = vec![0xea; 0x40];
        if recognized {
            let trailer = body.len() - 4;
            body[trailer..].copy_from_slice(&[b'L', b'M', 0x14, 0x01]);
            bytes[payload - lm_rats::HEADER_LEN..payload]
                .copy_from_slice(&make_header(body.len()).unwrap());
            bytes[payload..payload + body.len()].copy_from_slice(&body);
        }
        let pointer = pc_to_snes(Mapper::LoRom, payload).unwrap().to_le_bytes();
        bytes[lm_profile::SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK
            ..lm_profile::SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK + 4]
            .copy_from_slice(&[0x5c, pointer[0], pointer[1], pointer[2]]);
        bytes[lm_profile::SMW_US_V1_VRAM_PATCH_SECONDARY_HOOK] = 0x5c;
        bytes[lm_profile::SMW_US_V1_LM_VRAM_VERSION_OFFSET] = 1;
        bytes
    }

    #[test]
    fn pristine_dialog_defaults_to_normal_and_also_allows_none() {
        assert_eq!(
            dialog_model(&SmwUsV1VramPatchState::Absent),
            VramPatchDialogModel {
                selected: VramPatchSelection::Normal,
                none_enabled: true,
                normal_enabled: true,
                hd_enabled: false,
                recognized: true,
            }
        );
    }

    #[test]
    fn pristine_rom_does_not_install_a_runtime_until_the_dialog_confirms_one() {
        let mut app = AppState::default();
        app.load_rom(vanilla_bytes()).unwrap();
        assert_eq!(effective_selection(&app), None);

        let mut dialog = VramPatchOptionsDialog::default();
        dialog.open(&app, Some(VramPatchSelection::None));
        assert_eq!(dialog.model.unwrap().selected, VramPatchSelection::None);
    }

    #[test]
    fn unknown_runtime_has_no_automatic_save_selection() {
        let mut app = AppState::default();
        app.load_rom(old_or_unknown_runtime_bytes(false)).unwrap();
        assert_eq!(effective_selection(&app), None);
    }

    #[test]
    fn installed_normal_disables_none_and_unknown_disables_every_choice() {
        let installed = dialog_model(&SmwUsV1VramPatchState::Installed {
            version: 1,
            generation: 0x0115,
            owner: RatsBlock {
                header_offset: 0x80000,
                payload: 0x80008..0x83398,
            },
            requires_replacement: false,
        });
        assert_eq!(installed.selected, VramPatchSelection::Normal);
        assert!(!installed.none_enabled);
        assert!(installed.normal_enabled);

        let unknown = dialog_model(&SmwUsV1VramPatchState::Unknown {
            version: 0x44,
            primary_hook: true,
            secondary_hook: true,
        });
        assert!(!unknown.recognized);
        assert!(!unknown.none_enabled);
        assert!(!unknown.normal_enabled);
        assert!(!unknown.hd_enabled);
    }

    #[test]
    fn deferred_normal_rejects_before_mutating_a_pristine_rom() {
        let mut app = AppState::default();
        app.load_rom(vanilla_bytes()).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let level_save = Command::CommitRomMutation {
            expected_revision: snapshot.revision,
            description: "Save unchanged level 105".into(),
            mutation: RomMutation::unchanged(
                snapshot.identity.mapper,
                RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .unwrap()
                    .logical_len(),
            ),
        };
        assert!(
            prepare_level_save_command(&snapshot, VramPatchSelection::Normal, level_save).is_err()
        );
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, vanilla_bytes());
    }

    #[test]
    fn deferred_none_does_not_mutate_the_prepared_level_save() {
        let mut app = AppState::default();
        app.load_rom(vanilla_bytes()).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let command = Command::CommitRomMutation {
            expected_revision: snapshot.revision,
            description: "Save level 105".into(),
            mutation: RomMutation::unchanged(snapshot.identity.mapper, 0x80_000),
        };
        assert_eq!(
            prepare_level_save_command(&snapshot, VramPatchSelection::None, command.clone())
                .unwrap(),
            command
        );
    }

    #[test]
    fn recognized_old_generation_is_rejected_without_mutation() {
        let old_bytes = old_or_unknown_runtime_bytes(true);
        let mut app = AppState::default();
        app.load_rom(old_bytes.clone()).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&image).unwrap(),
            SmwUsV1VramPatchState::Installed {
                generation: 0x0114,
                requires_replacement: true,
                ..
            }
        ));
        let command = Command::CommitRomMutation {
            expected_revision: snapshot.revision,
            description: "Save level 105".into(),
            mutation: RomMutation::unchanged(snapshot.identity.mapper, image.logical_len()),
        };
        assert!(
            prepare_level_save_command(&snapshot, VramPatchSelection::Normal, command).is_err()
        );
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, old_bytes);
    }

    #[test]
    fn unknown_runtime_rejects_the_combined_save_without_mutating_the_project() {
        let unknown_bytes = old_or_unknown_runtime_bytes(false);
        let mut app = AppState::default();
        app.load_rom(unknown_bytes.clone()).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&image).unwrap(),
            SmwUsV1VramPatchState::Unknown { .. }
        ));
        let command = Command::CommitRomMutation {
            expected_revision: snapshot.revision,
            description: "Save level 105".into(),
            mutation: RomMutation::unchanged(snapshot.identity.mapper, image.logical_len()),
        };
        assert!(
            prepare_level_save_command(&snapshot, VramPatchSelection::Normal, command).is_err()
        );
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, unknown_bytes);
    }
}
