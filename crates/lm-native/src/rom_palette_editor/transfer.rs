use super::RomPaletteEditor;
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_graphics::{
    PaletteMaskFile, RawSnesPaletteFile, RgbChannelExpansion, RgbPaletteFile, TplPaletteFile,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingTransfer {
    Raw,
    RawWithMask,
    Tpl,
    Rgb,
}

enum DecodedImport {
    Raw(RawSnesPaletteFile, PaletteMaskFile),
    Supported {
        palette: lm_graphics::Palette,
        rgb_expansion: Option<RgbChannelExpansion>,
    },
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
                match decode_import(pending, loaded)? {
                    DecodedImport::Raw(source, mask) => workspace
                        .controller
                        .import_raw_palette(&source, &mask)
                        .map_err(|error| error.to_string()),
                    DecodedImport::Supported {
                        palette,
                        rgb_expansion,
                    } => {
                        workspace
                            .controller
                            .import_supported_palette(&palette)
                            .map_err(|error| error.to_string())?;
                        if let Some(expansion) = rgb_expansion {
                            self.rgb_expansion = Some(expansion);
                        }
                        Ok(())
                    }
                }
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
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import TPL v2…"))
                .clicked()
                && let Some(path) = dialogs::choose_tpl_palette_document()
            {
                self.start_import(
                    vec![BoundedRead::new(
                        path,
                        TplPaletteFile::FILE_LEN as u64,
                        "TPL v2 palette",
                    )],
                    PendingTransfer::Tpl,
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export TPL v2…"))
                .clicked()
            {
                self.start_tpl_export(revision);
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import RGB24…"))
                .clicked()
                && let Some(path) = dialogs::choose_rgb_palette_document()
            {
                self.start_import(
                    vec![BoundedRead::new(
                        path,
                        RgbPaletteFile::FILE_LEN as u64,
                        "RGB24 palette",
                    )],
                    PendingTransfer::Rgb,
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export RGB24…"))
                .clicked()
            {
                self.start_rgb_export(revision);
            }
        });
        ui.small("TPL/RGB transfer uses the retained installed-to-supported ordering and clears row-zero entries 1–15 on import.");
    }

    fn start_raw_import(&mut self, requests: Vec<BoundedRead>, pending: PendingTransfer) {
        self.start_import(requests, pending);
    }

    fn start_import(&mut self, requests: Vec<BoundedRead>, pending: PendingTransfer) {
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

    fn start_tpl_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("ROM palette workspace is closed".into());
            return;
        };
        let bytes = match workspace
            .controller
            .supported_palette()
            .map_err(|error| error.to_string())
            .and_then(|palette| encode_tpl_export(&palette))
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = dialogs::choose_tpl_palette_save_path() else {
            return;
        };
        if let Err(error) =
            self.transfer_persistence
                .start(revision, PersistenceTarget::Create(path), bytes)
        {
            self.error = Some(error);
        }
    }

    fn start_rgb_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("ROM palette workspace is closed".into());
            return;
        };
        let expansion = self
            .rgb_expansion
            .unwrap_or(RgbChannelExpansion::ReplicatedBits);
        let bytes = match workspace
            .controller
            .supported_palette()
            .map_err(|error| error.to_string())
            .and_then(|palette| encode_rgb_export(&palette, expansion))
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = dialogs::choose_rgb_palette_save_path() else {
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

fn encode_tpl_export(palette: &lm_graphics::Palette) -> Result<Vec<u8>, String> {
    TplPaletteFile {
        palette: palette.clone(),
    }
    .encode()
    .map_err(|error| error.to_string())
}

fn encode_rgb_export(
    palette: &lm_graphics::Palette,
    expansion: RgbChannelExpansion,
) -> Result<Vec<u8>, String> {
    RgbPaletteFile::from_snes_palette(palette, expansion)
        .and_then(|file| file.encode())
        .map_err(|error| error.to_string())
}

fn decode_import(
    pending: Option<PendingTransfer>,
    loaded: crate::document_loader::LoadedDocument,
) -> Result<DecodedImport, String> {
    match pending.ok_or("raw palette load lost its request kind")? {
        PendingTransfer::Raw => {
            let [(_, bytes)] = loaded.into_exact::<1>("raw palette")?;
            Ok(DecodedImport::Raw(
                RawSnesPaletteFile::decode(&bytes).map_err(|error| error.to_string())?,
                PaletteMaskFile::all_selected(),
            ))
        }
        PendingTransfer::RawWithMask => {
            let [(_, palette), (_, mask)] = loaded.into_exact::<2>("raw palette and mask")?;
            Ok(DecodedImport::Raw(
                RawSnesPaletteFile::decode(&palette).map_err(|error| error.to_string())?,
                PaletteMaskFile::decode(&mask).map_err(|error| error.to_string())?,
            ))
        }
        PendingTransfer::Tpl => {
            let [(_, bytes)] = loaded.into_exact::<1>("TPL v2 palette")?;
            let file = TplPaletteFile::decode(&bytes).map_err(|error| error.to_string())?;
            Ok(DecodedImport::Supported {
                palette: file.palette,
                rgb_expansion: None,
            })
        }
        PendingTransfer::Rgb => {
            let [(_, bytes)] = loaded.into_exact::<1>("RGB24 palette")?;
            let file = RgbPaletteFile::decode(&bytes).map_err(|error| error.to_string())?;
            let expansion = file.detected_expansion;
            Ok(DecodedImport::Supported {
                palette: file.to_snes_palette(),
                rgb_expansion: Some(expansion),
            })
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
        let DecodedImport::Raw(actual, actual_mask) =
            decode_import(Some(PendingTransfer::RawWithMask), loaded).unwrap()
        else {
            panic!("raw transfer decodes as raw");
        };
        assert_eq!(actual, expected);
        assert_eq!(actual_mask.encode(), mask);

        let exported = encode_raw_export(&expected.palette).unwrap();
        assert_eq!(RawSnesPaletteFile::decode(&exported).unwrap(), expected);
        let unmasked = LoadedDocument {
            files: vec![(PathBuf::from("palette.pal"), exported)],
        };
        let DecodedImport::Raw(_, selected) =
            decode_import(Some(PendingTransfer::Raw), unmasked).unwrap()
        else {
            panic!("raw transfer decodes as raw");
        };
        assert!(selected.entries().iter().all(|entry| *entry == 1));
    }

    #[test]
    fn tpl_and_rgb_helpers_round_trip_supported_order_and_rgb_convention() {
        let palette = Palette {
            colors: (0_u16..256).map(Bgr555).collect(),
        };
        let tpl = encode_tpl_export(&palette).unwrap();
        let decoded = decode_import(
            Some(PendingTransfer::Tpl),
            LoadedDocument {
                files: vec![(PathBuf::from("palette.tpl"), tpl)],
            },
        )
        .unwrap();
        let DecodedImport::Supported {
            palette: actual,
            rgb_expansion: None,
        } = decoded
        else {
            panic!("TPL transfer decodes as supported native words");
        };
        assert_eq!(actual, palette);

        let rgb = encode_rgb_export(&palette, RgbChannelExpansion::HighBits).unwrap();
        let decoded = decode_import(
            Some(PendingTransfer::Rgb),
            LoadedDocument {
                files: vec![(PathBuf::from("palette.pal"), rgb)],
            },
        )
        .unwrap();
        let DecodedImport::Supported {
            palette: actual,
            rgb_expansion: Some(RgbChannelExpansion::HighBits),
        } = decoded
        else {
            panic!("RGB transfer retains its detected expansion");
        };
        assert_eq!(actual, palette);
    }
}
