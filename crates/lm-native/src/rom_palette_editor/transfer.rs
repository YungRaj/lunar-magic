use super::RomPaletteEditor;
use crate::{dialogs, document_loader::BoundedRead};
use eframe::egui;
use lm_graphics::{
    PaletteMaskFile, RawSnesPaletteFile, RgbChannelExpansion, RgbPaletteFile, TplPaletteFile,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PendingTransfer {
    Raw,
    Tpl,
    Rgb,
}

pub(crate) enum DecodedImport {
    Raw(RawSnesPaletteFile, PaletteMaskFile),
    Supported {
        palette: lm_graphics::Palette,
        mask: PaletteMaskFile,
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
                    DecodedImport::Raw(source, mask) => {
                        workspace
                            .controller
                            .import_raw_palette(&source, &mask)
                            .map_err(|error| error.to_string())?;
                        self.palette_mask = mask.encode();
                        Ok(())
                    }
                    DecodedImport::Supported {
                        palette,
                        mask,
                        rgb_expansion,
                    } => {
                        workspace
                            .controller
                            .import_supported_palette_with_mask(&palette, &mask)
                            .map_err(|error| error.to_string())?;
                        if let Some(expansion) = rgb_expansion {
                            self.rgb_expansion = Some(expansion);
                        }
                        self.palette_mask = mask.encode();
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
                self.start_palette_import(
                    path,
                    RawSnesPaletteFile::FILE_LEN as u64,
                    "raw 257-color palette",
                    PendingTransfer::Raw,
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export raw palette…"))
                .clicked()
            {
                self.start_raw_export(revision);
            }
        });
        ui.small("Raw transfer preserves all 257 native words and automatically applies a same-name .palmask sidecar when present.");
        self.supported_palette_file_controls(ui, stale, busy, revision);
    }

    fn supported_palette_file_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        busy: bool,
        revision: u64,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import TPL v2…"))
                .clicked()
                && let Some(path) = dialogs::choose_tpl_palette_document()
            {
                self.start_palette_import(
                    path,
                    TplPaletteFile::FILE_LEN as u64,
                    "TPL v2 palette",
                    PendingTransfer::Tpl,
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export TPL v2…"))
                .clicked()
            {
                self.start_tpl_export(revision);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import RGB24…"))
                .clicked()
                && let Some(path) = dialogs::choose_rgb_palette_document()
            {
                self.start_palette_import(
                    path,
                    RgbPaletteFile::FILE_LEN as u64,
                    "RGB24 palette",
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
        ui.small("TPL/RGB transfer uses retained installed-to-supported ordering; an automatic same-name .palmask preserves unselected colors and clears selected row-zero entries 1–15.");
    }

    fn start_palette_import(
        &mut self,
        path: std::path::PathBuf,
        maximum: u64,
        description: &'static str,
        pending: PendingTransfer,
    ) {
        self.start_import(palette_import_requests(path, maximum, description), pending);
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
        self.start_palette_export(revision, path, bytes);
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
        self.start_palette_export(revision, path, bytes);
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
        self.start_palette_export(revision, path, bytes);
    }

    fn start_palette_export(&mut self, revision: u64, path: std::path::PathBuf, bytes: Vec<u8>) {
        if self.palette_mask.len() != PaletteMaskFile::FILE_LEN {
            self.error = Some(format!(
                "palette mask has {} entries instead of {}",
                self.palette_mask.len(),
                PaletteMaskFile::FILE_LEN
            ));
            return;
        }
        let result = if self.palette_mask.iter().all(|entry| *entry != 0) {
            let mask_path = path.with_extension("palmask");
            self.transfer_persistence
                .start_create_removing(revision, path, bytes, mask_path)
        } else {
            let mask_path = path.with_extension("palmask");
            self.transfer_persistence.start_create_pair(
                revision,
                path,
                bytes,
                mask_path,
                self.palette_mask.clone(),
            )
        };
        if let Err(error) = result {
            self.error = Some(error);
        }
    }
}

pub(crate) fn encode_raw_export(palette: &lm_graphics::Palette) -> Result<Vec<u8>, String> {
    RawSnesPaletteFile {
        palette: palette.clone(),
    }
    .encode()
    .map_err(|error| error.to_string())
}

pub(crate) fn encode_tpl_export(palette: &lm_graphics::Palette) -> Result<Vec<u8>, String> {
    TplPaletteFile {
        palette: palette.clone(),
    }
    .encode()
    .map_err(|error| error.to_string())
}

pub(crate) fn encode_rgb_export(
    palette: &lm_graphics::Palette,
    expansion: RgbChannelExpansion,
) -> Result<Vec<u8>, String> {
    RgbPaletteFile::from_snes_palette(palette, expansion)
        .and_then(|file| file.encode())
        .map_err(|error| error.to_string())
}

pub(crate) fn palette_import_requests(
    path: std::path::PathBuf,
    maximum: u64,
    description: &'static str,
) -> Vec<BoundedRead> {
    let mask = path.with_extension("palmask");
    vec![
        BoundedRead::new(path, maximum, description),
        BoundedRead::optional(
            mask,
            PaletteMaskFile::FILE_LEN as u64,
            "257-entry palette selection mask",
        ),
    ]
}

fn palette_and_optional_mask(
    loaded: crate::document_loader::LoadedDocument,
    description: &str,
) -> Result<(Vec<u8>, PaletteMaskFile), String> {
    let mut files = loaded.files.into_iter();
    let (_, palette) = files
        .next()
        .ok_or_else(|| format!("{description} loader returned no palette"))?;
    let mask = files
        .next()
        .map(|(_, bytes)| PaletteMaskFile::decode(&bytes).map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or_else(PaletteMaskFile::all_selected);
    if files.next().is_some() {
        return Err(format!(
            "{description} loader returned more than one palette mask"
        ));
    }
    Ok((palette, mask))
}

pub(crate) fn decode_import(
    pending: Option<PendingTransfer>,
    loaded: crate::document_loader::LoadedDocument,
) -> Result<DecodedImport, String> {
    match pending.ok_or("raw palette load lost its request kind")? {
        PendingTransfer::Raw => {
            let (bytes, mask) = palette_and_optional_mask(loaded, "raw palette")?;
            Ok(DecodedImport::Raw(
                RawSnesPaletteFile::decode(&bytes).map_err(|error| error.to_string())?,
                mask,
            ))
        }
        PendingTransfer::Tpl => {
            let (bytes, mask) = palette_and_optional_mask(loaded, "TPL v2 palette")?;
            let file = TplPaletteFile::decode(&bytes).map_err(|error| error.to_string())?;
            Ok(DecodedImport::Supported {
                palette: file.palette,
                mask,
                rgb_expansion: None,
            })
        }
        PendingTransfer::Rgb => {
            let (bytes, mask) = palette_and_optional_mask(loaded, "RGB24 palette")?;
            let file = RgbPaletteFile::decode_with_mask(&bytes, &mask)
                .map_err(|error| error.to_string())?;
            let expansion = file.detected_expansion;
            Ok(DecodedImport::Supported {
                palette: file.to_snes_palette(),
                mask,
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_EXPORT: AtomicU64 = AtomicU64::new(0);

    fn export_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-palette-mask-{name}-{}-{}.tpl",
            std::process::id(),
            NEXT_EXPORT.fetch_add(1, Ordering::Relaxed)
        ))
    }

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
                (PathBuf::from("palette.palmask"), mask.clone()),
            ],
        };
        let DecodedImport::Raw(actual, actual_mask) =
            decode_import(Some(PendingTransfer::Raw), loaded).unwrap()
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
            mask,
            rgb_expansion: None,
        } = decoded
        else {
            panic!("TPL transfer decodes as supported native words");
        };
        assert_eq!(actual, palette);
        assert!(mask.entries().iter().all(|entry| *entry == 1));

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
            mask,
            rgb_expansion: Some(RgbChannelExpansion::HighBits),
        } = decoded
        else {
            panic!("RGB transfer retains its detected expansion");
        };
        assert_eq!(actual, palette);
        assert!(mask.entries().iter().all(|entry| *entry == 1));
    }

    #[test]
    fn supported_mask_is_retained_and_limits_rgb_expansion_evidence() {
        let mut rgb = vec![0; RgbPaletteFile::FILE_LEN];
        rgb[0..3].copy_from_slice(&[248, 248, 248]);
        rgb[3..6].copy_from_slice(&[255, 255, 255]);
        let mut entries = vec![0; PaletteMaskFile::FILE_LEN];
        entries[0] = 0x80;
        entries[256] = 7;
        let decoded = decode_import(
            Some(PendingTransfer::Rgb),
            LoadedDocument {
                files: vec![
                    (PathBuf::from("palette.pal"), rgb),
                    (PathBuf::from("palette.palmask"), entries.clone()),
                ],
            },
        )
        .unwrap();
        let DecodedImport::Supported {
            mask,
            rgb_expansion: Some(RgbChannelExpansion::HighBits),
            ..
        } = decoded
        else {
            panic!("masked RGB transfer retains selector and selected expansion evidence");
        };
        assert_eq!(mask.encode(), entries);
    }

    #[test]
    fn import_requests_use_automatic_same_name_palmask_sibling() {
        let requests = palette_import_requests(
            PathBuf::from("palettes/Level 105.tpl"),
            TplPaletteFile::FILE_LEN as u64,
            "TPL v2 palette",
        );
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, PathBuf::from("palettes/Level 105.tpl"));
        assert_eq!(
            requests[1].path,
            PathBuf::from("palettes/Level 105.palmask")
        );
    }

    #[test]
    fn export_publishes_palmask_only_when_a_color_is_disabled() {
        let unmasked = export_path("all-enabled");
        let stale_mask = unmasked.with_extension("palmask");
        std::fs::write(&stale_mask, vec![0; PaletteMaskFile::FILE_LEN]).unwrap();
        let mut editor = RomPaletteEditor {
            palette_mask: vec![1; PaletteMaskFile::FILE_LEN],
            ..RomPaletteEditor::default()
        };
        editor.start_palette_export(7, unmasked.clone(), vec![1, 2, 3]);
        editor.transfer_persistence.wait_for_test().result.unwrap();
        assert_eq!(std::fs::read(&unmasked).unwrap(), [1, 2, 3]);
        assert!(!stale_mask.exists());
        std::fs::remove_file(unmasked).unwrap();

        let masked = export_path("disabled");
        editor.palette_mask[17] = 0;
        editor.start_palette_export(8, masked.clone(), vec![4, 5]);
        editor.transfer_persistence.wait_for_test().result.unwrap();
        assert_eq!(std::fs::read(&masked).unwrap(), [4, 5]);
        let mask_path = masked.with_extension("palmask");
        let exported_mask = std::fs::read(&mask_path).unwrap();
        assert_eq!(exported_mask, editor.palette_mask);
        std::fs::remove_file(masked).unwrap();
        std::fs::remove_file(mask_path).unwrap();
    }
}
