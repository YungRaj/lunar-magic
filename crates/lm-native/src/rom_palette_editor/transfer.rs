use super::RomPaletteEditor;
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_graphics::{PaletteMaskFile, RawSnesPaletteFile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingTransfer {
    Raw,
    RawWithMask,
}

impl RomPaletteEditor {
    pub(super) fn poll_transfer_file_io(&mut self, context: &egui::Context, revision: u64) {
        if let Some(result) = self.transfer_loader.show(context) {
            let pending = self.pending_transfer.take();
            let result = result.and_then(|loaded| {
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("ROM palette workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the raw palette was loading".into());
                }
                let (source, mask) = decode_raw_import(pending, loaded)?;
                workspace
                    .controller
                    .import_raw_palette(&source, &mask)
                    .map_err(|error| error.to_string())
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

    pub(super) fn raw_palette_file_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        revision: u64,
    ) {
        let busy = self.transfer_loader.is_running()
            || self.transfer_persistence.is_running()
            || self.manifest_loader.is_running();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import raw palette…"))
                .clicked()
                && let Some(path) = dialogs::choose_raw_palette_document()
            {
                self.start_raw_import(
                    vec![BoundedRead::new(
                        path,
                        RawSnesPaletteFile::FILE_LEN as u64,
                        "raw 257-color palette",
                    )],
                    PendingTransfer::Raw,
                );
            }
            if ui
                .add_enabled(
                    !stale && !busy,
                    egui::Button::new("Import raw palette + .palm…"),
                )
                .clicked()
                && let Some(palette) = dialogs::choose_raw_palette_document()
                && let Some(mask) = dialogs::choose_palette_mask_document()
            {
                self.start_raw_import(
                    vec![
                        BoundedRead::new(
                            palette,
                            RawSnesPaletteFile::FILE_LEN as u64,
                            "raw 257-color palette",
                        ),
                        BoundedRead::new(
                            mask,
                            PaletteMaskFile::FILE_LEN as u64,
                            "257-entry palette selection mask",
                        ),
                    ],
                    PendingTransfer::RawWithMask,
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export raw palette…"))
                .clicked()
            {
                self.start_raw_export(revision);
            }
        });
        ui.small("Raw transfer preserves all 257 native words; optional .palm selection follows Lunar Magic row-zero clearing.");
    }

    fn start_raw_import(&mut self, requests: Vec<BoundedRead>, pending: PendingTransfer) {
        match self.transfer_loader.start(requests) {
            Ok(()) => self.pending_transfer = Some(pending),
            Err(error) => self.error = Some(error),
        }
    }

    fn start_raw_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("ROM palette workspace is closed".into());
            return;
        };
        let bytes = match encode_raw_export(workspace.controller.palette()) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = dialogs::choose_raw_palette_save_path() else {
            return;
        };
        if let Err(error) =
            self.transfer_persistence
                .start(revision, PersistenceTarget::Create(path), bytes)
        {
            self.error = Some(error);
        }
    }
}

fn encode_raw_export(palette: &lm_graphics::Palette) -> Result<Vec<u8>, String> {
    RawSnesPaletteFile {
        palette: palette.clone(),
    }
    .encode()
    .map_err(|error| error.to_string())
}

fn decode_raw_import(
    pending: Option<PendingTransfer>,
    loaded: crate::document_loader::LoadedDocument,
) -> Result<(RawSnesPaletteFile, PaletteMaskFile), String> {
    match pending.ok_or("raw palette load lost its request kind")? {
        PendingTransfer::Raw => {
            let [(_, bytes)] = loaded.into_exact::<1>("raw palette")?;
            Ok((
                RawSnesPaletteFile::decode(&bytes).map_err(|error| error.to_string())?,
                PaletteMaskFile::all_selected(),
            ))
        }
        PendingTransfer::RawWithMask => {
            let [(_, palette), (_, mask)] = loaded.into_exact::<2>("raw palette and mask")?;
            Ok((
                RawSnesPaletteFile::decode(&palette).map_err(|error| error.to_string())?,
                PaletteMaskFile::decode(&mask).map_err(|error| error.to_string())?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_loader::LoadedDocument;
    use lm_graphics::{Bgr555, Palette};
    use std::path::PathBuf;

    #[test]
    fn raw_transfer_helper_preserves_exact_words_and_noncanonical_mask_bytes() {
        let expected = RawSnesPaletteFile {
            palette: Palette {
                colors: (0_u16..=256).map(Bgr555).collect(),
            },
        };
        let raw = expected.encode().unwrap();
        let mask = (0..PaletteMaskFile::FILE_LEN)
            .map(|index| if index % 2 == 0 { 0 } else { 0x80 })
            .collect::<Vec<_>>();
        let loaded = LoadedDocument {
            files: vec![
                (PathBuf::from("palette.bin"), raw),
                (PathBuf::from("palette.palm"), mask.clone()),
            ],
        };
        let (actual, actual_mask) =
            decode_raw_import(Some(PendingTransfer::RawWithMask), loaded).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual_mask.encode(), mask);

        let exported = encode_raw_export(&expected.palette).unwrap();
        assert_eq!(RawSnesPaletteFile::decode(&exported).unwrap(), expected);
        let unmasked = LoadedDocument {
            files: vec![(PathBuf::from("palette.pal"), exported)],
        };
        let (_, selected) = decode_raw_import(Some(PendingTransfer::Raw), unmasked).unwrap();
        assert!(selected.entries().iter().all(|entry| *entry == 1));
    }
}
