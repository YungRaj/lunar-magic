use super::{ColorForm, RomSharedPaletteEditor, format_bytes};
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_graphics::SmwPaletteFile;

impl RomSharedPaletteEditor {
    pub(super) fn start_complete_import(&mut self) {
        let Some(path) = dialogs::choose_shared_palette_document() else {
            return;
        };
        if let Err(error) = self.transfer_loader.start(vec![BoundedRead::new(
            path,
            SmwPaletteFile::MAX_FILE_LEN as u64,
            "native shared palette",
        )]) {
            self.error = Some(error);
        }
    }

    pub(super) fn poll_transfer_file_io(&mut self, context: &egui::Context, revision: u64) {
        if let Some(result) = self.transfer_loader.show(context) {
            let result = result.and_then(|loaded| {
                let [(_, bytes)] = loaded.into_exact::<1>("native shared palette")?;
                let file = decode_shared_palette(&bytes)?;
                {
                    let workspace = self
                        .workspace
                        .as_mut()
                        .ok_or("shared-palette workspace is closed")?;
                    if workspace.revision != revision {
                        return Err("the ROM changed while the shared palette was loading".into());
                    }
                    workspace.replace_file(file)?;
                }
                let current = &self
                    .workspace
                    .as_ref()
                    .ok_or("shared-palette workspace is closed")?
                    .current;
                self.form = ColorForm::load(current, 0)?;
                self.auxiliary = format_bytes(current.auxiliary_bytes());
                self.selected = 0;
                self.page = 0;
                self.loaded = Some(0);
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.transfer_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn complete_file_controls(&mut self, ui: &mut egui::Ui, stale: bool, revision: u64) {
        let busy = self.transfer_loader.is_running() || self.transfer_persistence.is_running();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !stale && !busy,
                    egui::Button::new("Import complete .smwpal…"),
                )
                .clicked()
            {
                self.start_complete_import();
            }
            if ui
                .add_enabled(
                    !stale && !busy,
                    egui::Button::new("Export complete .smwpal…"),
                )
                .clicked()
            {
                self.start_complete_export(revision);
            }
        });
        ui.small("Complete transfer preserves exact legacy or expanded native byte ordering.");
    }

    pub(super) fn start_complete_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("shared-palette workspace is closed".into());
            return;
        };
        let Some(path) = dialogs::choose_shared_palette_save_path() else {
            return;
        };
        let bytes = workspace.current.encode();
        if let Err(error) =
            self.transfer_persistence
                .start(revision, PersistenceTarget::Create(path), bytes)
        {
            self.error = Some(error);
        }
    }
}

fn decode_shared_palette(bytes: &[u8]) -> Result<SmwPaletteFile, String> {
    SmwPaletteFile::decode(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_shared_palette_helper_round_trips_both_exact_backends() {
        let legacy =
            SmwPaletteFile::legacy(vec![0x12; SmwPaletteFile::LEGACY_PALETTE_LEN]).unwrap();
        let expanded = SmwPaletteFile::expanded(
            vec![0x34; SmwPaletteFile::EXPANDED_PALETTE_LEN],
            (0_u8..16).collect(),
        )
        .unwrap();
        for expected in [legacy, expanded] {
            let bytes = expected.encode();
            assert_eq!(decode_shared_palette(&bytes).unwrap(), expected);
            assert!(decode_shared_palette(&bytes[..bytes.len() - 1]).is_err());
        }
    }
}
