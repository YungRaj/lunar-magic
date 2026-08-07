mod workspace;

use eframe::egui;
use lm_app::{AppState, Command};
use workspace::{BuiltInRuntime, BuiltInRuntimeWorkspace};

#[derive(Default)]
pub(crate) struct BuiltInRuntimeInstaller {
    workspace: Option<BuiltInRuntimeWorkspace>,
    error: Option<String>,
}

impl BuiltInRuntimeInstaller {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match BuiltInRuntimeWorkspace::load(app) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> Option<Command> {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("Install Built-in Runtime")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        self.show_error(context);
        command
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_mut()?;
        let stale = workspace.is_stale(project_revision);
        let mut cancel = false;
        ui.label("Target: Super Mario World (USA), revision 0, LoROM");
        egui::ComboBox::from_label("Recovered runtime family")
            .selected_text(workspace.runtime.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::ExpandedSettings,
                    BuiltInRuntime::ExpandedSettings.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::CompleteLayer3,
                    BuiltInRuntime::CompleteLayer3.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::Lfix3Core,
                    BuiltInRuntime::Lfix3Core.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::Map16Runtime,
                    BuiltInRuntime::Map16Runtime.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::ExpandedExAnimation,
                    BuiltInRuntime::ExpandedExAnimation.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::Layer2Runtime,
                    BuiltInRuntime::Layer2Runtime.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::Sprite19Fix,
                    BuiltInRuntime::Sprite19Fix.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::SupportPatchB,
                    BuiltInRuntime::SupportPatchB.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::ExpandedSharedPalettes,
                    BuiltInRuntime::ExpandedSharedPalettes.label(),
                );
                ui.selectable_value(
                    &mut workspace.runtime,
                    BuiltInRuntime::Lz2SpeedGraphics,
                    BuiltInRuntime::Lz2SpeedGraphics.label(),
                );
            });
        ui.label(workspace.runtime.description());
        if workspace.selection_is_installed() {
            ui.label("The selected current runtime is already installed and authenticated.");
        }
        if let Some(description) = workspace.migration_description() {
            ui.label(description);
        }
        ui.label(
            "Installation may expand the ROM. All allocations, hooks, checksum repair, and \
             history changes commit atomically.",
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this installer opened. Reopen before installing.",
            );
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !stale && !workspace.selection_is_installed(),
                    egui::Button::new(if workspace.selection_migrates_legacy_runtime() {
                        "Migrate transactionally"
                    } else {
                        "Install transactionally"
                    }),
                )
                .clicked()
            {
                match workspace.prepare(project_revision) {
                    Ok(prepared) => command = Some(prepared),
                    Err(error) => self.error = Some(error),
                }
            }
        });
        if cancel {
            self.workspace = None;
        }
        command
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Built-in runtime installation error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.workspace = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::{smw_us_v1_custom_palette_installation, smw_us_v1_expanded_settings_layout};
    use lm_rom::{CopierHeader, Mapper, RomImage};
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_rom_settings_install_reopens_and_undoes_exactly() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        installer.commit_succeeded();
        assert!(!installer.is_open());
        let first_record = app
            .project()
            .unwrap()
            .load_expanded_level_settings(0, smw_us_v1_expanded_settings_layout())
            .unwrap();
        assert_eq!(first_record.encoded().len(), 32);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn complete_layer3_selection_routes_the_recovered_group_installer() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let mut app = AppState::default();
        app.load_rom(fixture.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::CompleteLayer3;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), fixture);
    }

    #[test]
    fn expanded_palette_selection_installs_and_enables_custom_palette_storage() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::ExpandedSharedPalettes;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        assert!(
            smw_us_v1_custom_palette_installation()
                .resolve(&app.project().unwrap().rom)
                .unwrap()
                .is_some()
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn lfix3_runtime_selection_installs_reopens_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Lfix3Core;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        installer.commit_succeeded();

        let project = app.project().unwrap();
        assert_eq!(project.history.undo_len(), 1);
        assert_eq!(
            project.rom.read(0x0002_da17, 5).unwrap(),
            &[0x22, 0x08, 0x80, 0x10, 0xea]
        );
        assert!(
            lm_rom::SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc)
                .unwrap()
                .is_complementary()
        );
        assert!(
            lm_profile::detect_smw_us_v1_current_lfix3_runtime(project.rom.logical_bytes())
                .unwrap()
                .is_some()
        );
        let revision = app.project_revision();
        assert!(matches!(
            app.dispatch(Command::InstallLfix3 { rev: revision }),
            Err(lm_app::AppError::Lfix3AlreadyInstalled)
        ));
        assert_eq!(app.project().unwrap().history.undo_len(), 1);

        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Lfix3Core;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_is_installed()
        );
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .prepare(app.project_revision())
                .is_err()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ2-Orig installed-graphics ROM"]
    fn lz2_speed_selection_installs_reopens_and_undoes_exactly() {
        let original = fs::read(std::env::var_os("LM_LZ2_ORIGINAL_ROM").unwrap()).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Lz2SpeedGraphics;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        assert!(matches!(
            command,
            Command::InstallLz2SpeedRuntime { rev: 0 }
        ));
        app.dispatch(command).unwrap();
        assert_eq!(
            lm_profile::detect_smw_us_v1_graphics_compression_mode(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ3 installed-graphics ROM"]
    fn lz2_speed_selection_migrates_directly_from_lz3_and_undoes_exactly() {
        let original = fs::read(std::env::var_os("LM_LZ3_ROM").unwrap()).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Lz2SpeedGraphics;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        assert_eq!(
            lm_profile::detect_smw_us_v1_graphics_compression_mode(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed
        );
        assert_eq!(app.project().unwrap().save_snapshot().len(), original.len());
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn map16_runtime_selection_installs_auxiliary_hooks_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Map16Runtime;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        installer.commit_succeeded();

        let project = app.project().unwrap();
        assert_eq!(project.history.undo_len(), 1);
        let secondary = lm_profile::load_smw_us_v1_secondary_map16(project).unwrap();
        assert!(secondary.installed);
        assert!(secondary.blocks.iter().all(Option::is_none));
        assert!(
            lm_rom::SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc)
                .unwrap()
                .is_complementary()
        );
        assert!(
            lm_profile::detect_smw_us_v1_current_map16_runtime(project.rom.logical_bytes())
                .unwrap()
                .is_some()
        );
        let revision = app.project_revision();
        assert!(matches!(
            app.dispatch(Command::InstallMap16Runtime { rev: revision }),
            Err(lm_app::AppError::Map16RuntimeAlreadyInstalled)
        ));
        assert_eq!(app.project().unwrap().history.undo_len(), 1);

        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Map16Runtime;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_is_installed()
        );
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .prepare(app.project_revision())
                .is_err()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn expanded_exanimation_selection_installs_reopens_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::ExpandedExAnimation;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        assert!(matches!(
            command,
            Command::InstallExpandedExAnimationRuntime { rev: 0 }
        ));
        app.dispatch(command).unwrap();
        installer.commit_succeeded();

        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                app.project().unwrap().rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::ExpandedExAnimation;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_is_installed()
        );
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .prepare(app.project_revision())
                .is_err()
        );

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn map16_runtime_selection_supports_occupied_expansion_and_both_header_variants() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        for copier_header in [CopierHeader::Absent, CopierHeader::Present] {
            let mut image = RomImage::from_bytes(pristine.clone()).unwrap();
            image.expand(Mapper::LoRom, 0x100_000, 0xa5).unwrap();
            image.update_snes_checksum(0x7fdc).unwrap();
            image.set_copier_header(copier_header, 0x5a);
            let original = image.as_file_bytes().to_vec();
            let original_prefix =
                (copier_header == CopierHeader::Present).then(|| original[..0x200].to_vec());

            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            let mut installer = BuiltInRuntimeInstaller::default();
            installer.open(&app);
            installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Map16Runtime;
            let command = installer
                .workspace
                .as_ref()
                .unwrap()
                .prepare(app.project_revision())
                .unwrap();
            app.dispatch(command).unwrap();

            let installed = app.project().unwrap();
            assert_eq!(installed.rom.logical_len(), 0x200_000);
            assert_eq!(installed.rom.copier_header(), copier_header);
            if let Some(prefix) = &original_prefix {
                assert_eq!(&installed.save_snapshot()[..0x200], prefix);
            }
            assert_eq!(
                lm_profile::detect_smw_us_v1_current_map16_runtime(installed.rom.logical_bytes())
                    .unwrap()
                    .unwrap()
                    .payload,
                0x108000..0x110000
            );
            assert!(
                lm_rom::detect_identity(&installed.rom)
                    .unwrap()
                    .checksum_matches()
            );
            assert_eq!(installed.history.undo_len(), 1);

            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original);
        }
    }

    #[test]
    fn map16_runtime_selection_migrates_stage_three_and_undoes_exactly() {
        const STAGE_MARKER_OFFSET: usize = 0x37_65c;
        const STAGE_FOUR_HOOK_OFFSET: usize = 0x37_7a0;
        const STAGE_THREE_MARKER: [u8; 4] = [0x4c, 0x4d, 0x11, 0x01];
        const STAGE_THREE_HOOK: [u8; 0x14] = [
            0x20, 0x08, 0xf6, 0xc9, 0xda, 0xf0, 0x19, 0x4c, 0x02, 0xf6, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ];

        let mut source = AppState::default();
        source
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        source
            .dispatch(Command::InstallMap16Runtime { rev: 0 })
            .unwrap();
        let mut stage_three = source.project().unwrap().save_snapshot();
        stage_three[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + STAGE_THREE_MARKER.len()]
            .copy_from_slice(&STAGE_THREE_MARKER);
        stage_three[STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_THREE_HOOK.len()]
            .copy_from_slice(&STAGE_THREE_HOOK);
        let checksum = lm_rom::compute_snes_checksum(&stage_three, 0x7fdc).unwrap();
        stage_three[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());

        let mut app = AppState::default();
        app.load_rom(stage_three.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Map16Runtime;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_migrates_legacy_runtime()
        );
        app.dispatch(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .prepare(app.project_revision())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            lm_profile::probe_smw_us_v1_map16_runtime_generation(
                app.project().unwrap().rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageFourCurrent
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), stage_three);
    }

    #[test]
    #[ignore = "requires an externally supplied Lunar Magic 3.01 format-$102 ROM"]
    fn layer2_runtime_selection_migrates_authentic_format_102_and_undoes_exactly() {
        let source = fs::read(
            std::env::var_os("LM_LAYER2_FORMAT_102_ROM").expect("LM_LAYER2_FORMAT_102_ROM"),
        )
        .unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Layer2Runtime;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_migrates_legacy_runtime()
        );
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        assert!(matches!(command, Command::InstallLayer2Runtime { rev: 0 }));
        app.dispatch(command).unwrap();
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert_eq!(
            lm_profile::probe_smw_us_v1_layer2_runtime_generation(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1Layer2RuntimeGeneration::Format103Current
        );
        assert!(matches!(
            app.dispatch(Command::InstallLayer2Runtime {
                rev: app.project_revision()
            }),
            Err(lm_app::AppError::Layer2RuntimeAlreadyInstalled)
        ));
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
    }

    #[test]
    fn sprite19_fix_selection_installs_authenticates_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Sprite19Fix;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        installer.commit_succeeded();

        assert_eq!(
            lm_profile::detect_smw_us_v1_sprite19_fix(app.project().unwrap().rom.logical_bytes())
                .unwrap(),
            lm_profile::SmwUsV1Sprite19FixState::Installed
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        let revision = app.project_revision();
        assert!(matches!(
            app.dispatch(Command::InstallSprite19Fix { rev: revision }),
            Err(lm_app::AppError::Sprite19FixAlreadyInstalled)
        ));
        assert_eq!(app.project().unwrap().history.undo_len(), 1);

        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::Sprite19Fix;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_is_installed()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn support_patch_b_selection_installs_exact_runtime_and_undoes() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut installer = BuiltInRuntimeInstaller::default();
        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::SupportPatchB;
        let command = installer
            .workspace
            .as_ref()
            .unwrap()
            .prepare(app.project_revision())
            .unwrap();
        app.dispatch(command).unwrap();
        installer.commit_succeeded();

        assert_eq!(
            lm_profile::detect_smw_us_v1_support_patch_b(
                app.project().unwrap().rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1SupportPatchBState::Installed
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        let revision = app.project_revision();
        assert!(matches!(
            app.dispatch(Command::InstallSupportPatchB { rev: revision }),
            Err(lm_app::AppError::SupportPatchBAlreadyInstalled)
        ));
        assert_eq!(app.project().unwrap().history.undo_len(), 1);

        installer.open(&app);
        installer.workspace.as_mut().unwrap().runtime = BuiltInRuntime::SupportPatchB;
        assert!(
            installer
                .workspace
                .as_ref()
                .unwrap()
                .selection_is_installed()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn support_patch_b_install_preserves_headered_container_and_undoes() {
        let logical = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = lm_rom::RomImage::from_bytes(logical).unwrap();
        assert!(image.set_copier_header(lm_rom::CopierHeader::Present, 0xa5));
        let original = image.as_file_bytes().to_vec();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();

        app.dispatch(Command::InstallSupportPatchB { rev: 0 })
            .unwrap();
        let project = app.project().unwrap();
        assert_eq!(project.rom.copier_header(), lm_rom::CopierHeader::Present);
        assert_eq!(project.rom.copier_header_bytes().unwrap(), &[0xa5; 0x200]);
        assert_eq!(
            lm_profile::detect_smw_us_v1_support_patch_b(project.rom.logical_bytes()).unwrap(),
            lm_profile::SmwUsV1SupportPatchBState::Installed
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
