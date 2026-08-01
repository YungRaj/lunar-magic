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
                    BuiltInRuntime::ExpandedSharedPalettes,
                    BuiltInRuntime::ExpandedSharedPalettes.label(),
                );
            });
        ui.label(workspace.runtime.description());
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
                .add_enabled(!stale, egui::Button::new("Install transactionally"))
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
    fn lfix3_selection_installs_exact_hooks_checksums_and_undoes() {
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
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
