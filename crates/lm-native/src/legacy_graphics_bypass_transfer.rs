use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    persistence_worker::{PersistenceTarget, PersistenceWorker},
};
use eframe::egui;
use lm_app::{AppState, Command, ControllerSnapshot};
use lm_level::LegacyGraphicsBypassTable;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyGraphicsBypassTransferAction {
    Extract,
    Insert,
}

#[derive(Default)]
pub(crate) struct LegacyGraphicsBypassTransfer {
    loader: DocumentLoader,
    persistence: PersistenceWorker,
    pending_import: Option<ControllerSnapshot>,
    completion: Option<String>,
    error: Option<String>,
}

impl LegacyGraphicsBypassTransfer {
    pub(crate) fn start(&mut self, app: &AppState, action: LegacyGraphicsBypassTransferAction) {
        if self.loader.is_running()
            || self.persistence.is_running()
            || self.pending_import.is_some()
        {
            self.error = Some("an old ExGFX bypass-list transfer is already active".into());
            return;
        }
        let result = match action {
            LegacyGraphicsBypassTransferAction::Extract => dialogs::choose_bypass_list_save_path()
                .map_or(Ok(()), |path| self.start_export_to(app, path)),
            LegacyGraphicsBypassTransferAction::Insert => dialogs::choose_bypass_list_document()
                .map_or(Ok(()), |path| self.start_import_from(app, path)),
        };
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    fn start_export_to(&mut self, app: &AppState, path: PathBuf) -> Result<(), String> {
        self.ensure_idle()?;
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let bytes = lm_app::export_legacy_graphics_bypass_list(&snapshot)?;
        let target = PersistenceTarget::save_as(path)?;
        self.persistence
            .start(snapshot.revision, target, bytes.to_vec())
    }

    fn start_import_from(&mut self, app: &AppState, path: PathBuf) -> Result<(), String> {
        self.ensure_idle()?;
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        self.loader.start(vec![BoundedRead::new(
            path,
            LegacyGraphicsBypassTable::ENCODED_LEN as u64,
            "old ExGFX bypass list",
        )])?;
        self.pending_import = Some(snapshot);
        Ok(())
    }

    fn ensure_idle(&self) -> Result<(), String> {
        if self.loader.is_running()
            || self.persistence.is_running()
            || self.pending_import.is_some()
        {
            Err("an old ExGFX bypass-list transfer is already active".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        if let Some(completion) = self.persistence.show(context) {
            match completion.result {
                Ok(()) => {
                    self.completion = Some(format!(
                        "Old ExGFX bypass list extracted to:\n{}",
                        match completion.target {
                            PersistenceTarget::Create(path)
                            | PersistenceTarget::ReplaceAs(path) => path.display().to_string(),
                            _ => "selected destination".into(),
                        }
                    ));
                }
                Err(error) => self.error = Some(error),
            }
        }

        let command = self.loader.show(context).and_then(|result| {
            let pending = self
                .pending_import
                .take()
                .ok_or_else(|| "bypass-list loader lost its revision snapshot".to_owned());
            match result.and_then(|loaded| {
                let [(_, bytes)] = loaded.into_exact::<1>("old ExGFX bypass list")?;
                let snapshot = pending?;
                if snapshot.revision != app.project_revision() {
                    return Err("the ROM changed while Bypass.lst was loading".into());
                }
                lm_app::prepare_legacy_graphics_bypass_list_import(&snapshot, &bytes)
                    .map(lm_app::PreparedRomCommit::into_command)
            }) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        });

        if let Some(message) = self.completion.clone() {
            egui::Window::new("Bypass List Extraction Complete")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(message);
                    if ui.button("OK").clicked() {
                        self.completion = None;
                    }
                });
        }
        if let Some(message) = self.error.clone() {
            egui::Window::new("Old ExGFX Bypass List Error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(message);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn extraction_publishes_exact_400_byte_table_without_an_envelope() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Bypass.lst");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let expected =
            lm_app::export_legacy_graphics_bypass_list(&app.controller_snapshot().unwrap())
                .unwrap();
        let mut transfer = LegacyGraphicsBypassTransfer::default();
        transfer.start_export_to(&app, target.clone()).unwrap();
        assert!(transfer.persistence.wait_for_test().result.is_ok());
        assert_eq!(fs::read(target).unwrap(), expected);

        let replacement = directory.path().join("replacement.lst");
        fs::write(&replacement, b"old bytes").unwrap();
        transfer.start_export_to(&app, replacement.clone()).unwrap();
        assert!(transfer.persistence.wait_for_test().result.is_ok());
        assert_eq!(fs::read(replacement).unwrap(), expected);
    }

    #[test]
    fn second_transfer_is_rejected_while_file_io_is_active() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Bypass.lst");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut transfer = LegacyGraphicsBypassTransfer::default();
        transfer.start_export_to(&app, target.clone()).unwrap();
        assert!(transfer.start_import_from(&app, target).is_err());
        let _ = transfer.persistence.wait_for_test();
    }
}
