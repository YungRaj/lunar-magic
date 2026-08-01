use super::RomLevelAssetsEditor;
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_graphics::PaletteInterchangeFile;

impl RomLevelAssetsEditor {
    pub(super) fn poll_palette_file_io(&mut self, context: &egui::Context, revision: u64) {
        if let Some(result) = self.palette_loader.show(context) {
            let result = result.and_then(|loaded| {
                let [(_, bytes)] = loaded.into_exact::<1>("level palette")?;
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("native level-assets workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the level palette was loading".into());
                }
                let file = decode_palette_file(&bytes)?;
                workspace
                    .controller
                    .replace_palette_file(&file)
                    .map_err(|error| error.to_string())?;
                self.panels.invalidate();
                self.bypass_validation = None;
                self.bypass_layer2_texture = None;
                self.bypass_inspection = None;
                self.bypass_preview.invalidate();
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.palette_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn palette_file_controls(&mut self, ui: &mut egui::Ui, stale: bool, revision: u64) {
        let busy = self.palette_loader.is_running()
            || self.palette_persistence.is_running()
            || self.mwl_loader.is_running()
            || self.legacy_mwl_loader.is_running()
            || self.manifest_loader.is_running()
            || self.mwl_batch_worker.is_running()
            || self.image_batch_worker.is_running();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import full .lmpal…"))
                .clicked()
                && let Some(path) = dialogs::choose_palette_document()
                && let Err(error) = self.palette_loader.start(vec![BoundedRead::new(
                    path,
                    PaletteInterchangeFile::MAX_FILE_LEN as u64,
                    "portable level palette",
                )])
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export full .lmpal…"))
                .clicked()
            {
                self.start_palette_export(revision);
            }
        });
        ui.small("Full palette transfer preserves source-level provenance and enforces active ownership.");
    }

    fn start_palette_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("native level-assets workspace is closed".into());
            return;
        };
        let Some(path) = dialogs::choose_palette_save_path(workspace.source_slot) else {
            return;
        };
        let file = PaletteInterchangeFile {
            source_palette: workspace.source_slot,
            palette: workspace.controller.assets().palette.clone(),
        };
        match encode_palette_file(&file) {
            Ok(bytes) => {
                if let Err(error) =
                    self.palette_persistence
                        .start(revision, PersistenceTarget::Create(path), bytes)
                {
                    self.error = Some(error);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn decode_palette_file(bytes: &[u8]) -> Result<PaletteInterchangeFile, String> {
    PaletteInterchangeFile::decode(bytes).map_err(|error| error.to_string())
}

fn encode_palette_file(file: &PaletteInterchangeFile) -> Result<Vec<u8>, String> {
    file.encode().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    #[test]
    fn native_level_palette_helpers_round_trip_exact_words_and_provenance() {
        let expected = PaletteInterchangeFile {
            source_palette: 0x1ff,
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(0x1234), Bgr555(0x7fff)],
            },
        };
        let bytes = encode_palette_file(&expected).unwrap();
        assert_eq!(decode_palette_file(&bytes).unwrap(), expected);
        assert!(decode_palette_file(&bytes[..bytes.len() - 1]).is_err());
    }
}
