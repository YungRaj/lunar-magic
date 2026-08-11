use eframe::egui;
use lm_app::{AppState, Command, ControllerSnapshot};
use lm_profile::{
    SmwUsV1VramPatchState, detect_smw_us_v1_vram_patch,
    smw_us_v1_normal_vram_patch_installation_plan,
};
use lm_project::{Project, RatsOwnershipManifest, RomMutation};
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
    pub(crate) fn open(&mut self, app: &AppState) {
        match detect(app) {
            Ok(state) => {
                self.model = Some(dialog_model(&state));
                self.error = None;
                self.open = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if self.open {
            egui::Window::new("Change VRAM Patch Options")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        "The VRAM patch by smkdan allows using an extra 2 GFX slots for more \
                         graphics (BG2 and BG3). It's also required for horizontal levels to be \
                         resized vertically.",
                    );
                    ui.label("Any changes will be applied on the next level save.");
                    ui.separator();
                    if let Some(model) = &mut self.model {
                        ui.group(|ui| {
                            ui.label("VRAM Patch Type");
                            ui.add_enabled_ui(model.none_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::None,
                                    "None - Do not install patch",
                                )
                                .on_hover_text(
                                    "This will not install the VRAM patch. It can make some \
                                     features unavailable. This option is only available if the \
                                     patch has not yet been installed.",
                                );
                            });
                            ui.add_enabled_ui(model.normal_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Normal,
                                    "Normal Version",
                                )
                                .on_hover_text(
                                    "Installs the regular version of the VRAM patch. This is the \
                                     default setting.",
                                );
                            });
                            ui.add_enabled_ui(model.hd_enabled, |ui| {
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Hd16x9,
                                    "HD Version 16:9 (352 width)",
                                );
                                ui.radio_value(
                                    &mut model.selected,
                                    VramPatchSelection::Hd21x9,
                                    "HD Version 21:9 (448 width)",
                                );
                            });
                        });
                        if !model.recognized {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "The installed VRAM patch version is not recognized. Lunar Magic \
                                 disables every choice to avoid overwriting an unknown patch.",
                            );
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                        let can_confirm = self.model.is_some_and(|model| model.recognized);
                        if ui
                            .add_enabled(can_confirm, egui::Button::new("OK"))
                            .clicked()
                        {
                            self.pending = self.model.map(|model| model.selected);
                            self.open = false;
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("VRAM patch options error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }

    pub(crate) fn take_pending(&mut self) -> Option<VramPatchSelection> {
        self.pending.take()
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
            let plan = smw_us_v1_normal_vram_patch_installation_plan(staged.rom.logical_len())
                .map_err(|error| error.to_string())?;
            staged
                .install_relocatable_patch(&plan)
                .map_err(|error| error.to_string())?;
        }
        SmwUsV1VramPatchState::Installed {
            requires_replacement: false,
            ..
        } => {}
        SmwUsV1VramPatchState::Installed {
            owner,
            requires_replacement: true,
            ..
        } => {
            let mut plan = smw_us_v1_normal_vram_patch_installation_plan(staged.rom.logical_len())
                .map_err(|error| error.to_string())?;
            // A recognized older generation owns the fixed hooks. Lunar Magic overwrites their
            // current operands while reclaiming that authenticated RATS allocation.
            for write in &mut plan.writes {
                write.expected = staged
                    .rom
                    .read(write.offset, write.replacement.len())
                    .map_err(|error| error.to_string())?
                    .to_vec();
            }
            staged
                .replace_relocatable_patch(
                    &plan,
                    &RatsOwnershipManifest {
                        owned: vec![owner],
                        retained: Vec::new(),
                    },
                    0xff,
                )
                .map_err(|error| error.to_string())?;
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
    fn deferred_normal_is_one_undoable_level_save_and_reopens_installed() {
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
        let combined =
            prepare_level_save_command(&snapshot, VramPatchSelection::Normal, level_save).unwrap();
        app.dispatch(combined).unwrap();

        let reopened = app.controller_snapshot().unwrap();
        let image = RomImage::from_bytes(reopened.rom_bytes).unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&image).unwrap(),
            SmwUsV1VramPatchState::Installed {
                version: 1,
                generation: 0x0115,
                requires_replacement: false,
                ..
            }
        ));
        app.dispatch(Command::Undo).unwrap();
        let undone = app.controller_snapshot().unwrap();
        assert_eq!(undone.rom_bytes, vanilla_bytes());
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
    fn recognized_old_generation_is_replaced_atomically_and_undo_restores_it() {
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
        app.dispatch(
            prepare_level_save_command(&snapshot, VramPatchSelection::Normal, command).unwrap(),
        )
        .unwrap();
        let reopened = app.controller_snapshot().unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&RomImage::from_bytes(reopened.rom_bytes).unwrap())
                .unwrap(),
            SmwUsV1VramPatchState::Installed {
                generation: 0x0115,
                requires_replacement: false,
                ..
            }
        ));
        app.dispatch(Command::Undo).unwrap();
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
