use super::RomLevelAssetsEditor;
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_graphics::{
    Bgr555, Palette, PaletteInterchangeFile, PaletteMaskFile, RawSnesPaletteFile,
    RgbChannelExpansion, RgbPaletteFile, TplPaletteFile, apply_raw_palette_import,
};

use crate::rom_palette_editor::transfer::{
    DecodedImport, PendingTransfer, decode_import, encode_raw_export, encode_rgb_export,
    encode_tpl_export, palette_import_requests,
};

impl RomLevelAssetsEditor {
    pub(super) fn poll_palette_file_io(&mut self, context: &egui::Context, revision: u64) {
        if let Some(result) = self.palette_loader.show(context) {
            let pending = self.pending_palette_transfer.take();
            let result = result.and_then(|loaded| {
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("native level-assets workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the level palette was loading".into());
                }
                let file = match pending {
                    None => {
                        let [(_, bytes)] = loaded.into_exact::<1>("level palette")?;
                        decode_palette_file(&bytes)?
                    }
                    Some(kind) => {
                        let (palette, expansion) = imported_native_palette(
                            workspace.controller.assets().palette.clone(),
                            decode_import(Some(kind), loaded)?,
                        )?;
                        if let Some(expansion) = expansion {
                            self.palette_rgb_expansion = Some(expansion);
                        }
                        PaletteInterchangeFile {
                            source_palette: workspace.source_slot,
                            palette,
                        }
                    }
                };
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
            {
                self.pending_palette_transfer = None;
                if let Err(error) = self.palette_loader.start(vec![BoundedRead::new(
                    path,
                    PaletteInterchangeFile::MAX_FILE_LEN as u64,
                    "portable level palette",
                )]) {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export full .lmpal…"))
                .clicked()
            {
                self.start_palette_export(revision);
            }
        });
        self.native_palette_file_controls(ui, stale, busy, revision);
        ui.small("Every import is staged through the active ownership map; exports snapshot the current staged palette and never overwrite an existing file.");
    }

    fn native_palette_file_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        busy: bool,
        revision: u64,
    ) {
        for (label, kind, maximum, description) in [
            (
                "Import raw…",
                PendingTransfer::Raw,
                RawSnesPaletteFile::FILE_LEN as u64,
                "raw 257-color palette",
            ),
            (
                "Import TPL v2…",
                PendingTransfer::Tpl,
                TplPaletteFile::FILE_LEN as u64,
                "TPL v2 palette",
            ),
            (
                "Import RGB24…",
                PendingTransfer::Rgb,
                RgbPaletteFile::FILE_LEN as u64,
                "RGB24 palette",
            ),
        ] {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!stale && !busy, egui::Button::new(label))
                    .clicked()
                    && let Some(path) = choose_native_palette_import(kind)
                {
                    let requests = palette_import_requests(path, maximum, description);
                    match self.palette_loader.start(requests) {
                        Ok(()) => self.pending_palette_transfer = Some(kind),
                        Err(error) => self.error = Some(error),
                    }
                }
                let export_label = match kind {
                    PendingTransfer::Raw => "Export raw…",
                    PendingTransfer::Tpl => "Export TPL v2…",
                    PendingTransfer::Rgb => "Export RGB24…",
                };
                if ui
                    .add_enabled(!stale && !busy, egui::Button::new(export_label))
                    .clicked()
                {
                    self.start_native_palette_export(revision, kind);
                }
            });
        }
        ui.small("Raw/TPL/RGB imports automatically apply a same-name .palmask when present; full exports remove a stale mask sidecar.");
    }

    fn start_native_palette_export(&mut self, revision: u64, kind: PendingTransfer) {
        let result = self
            .workspace
            .as_ref()
            .ok_or_else(|| "native level-assets workspace is closed".to_owned())
            .and_then(|workspace| {
                let installed = &workspace.controller.assets().palette;
                let supported = || supported_palette(installed);
                match kind {
                    PendingTransfer::Raw => encode_raw_export(installed),
                    PendingTransfer::Tpl => encode_tpl_export(&supported()?),
                    PendingTransfer::Rgb => encode_rgb_export(
                        &supported()?,
                        self.palette_rgb_expansion
                            .unwrap_or(RgbChannelExpansion::ReplicatedBits),
                    ),
                }
            });
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = choose_native_palette_export(kind) else {
            return;
        };
        let mask_path = path.with_extension("palmask");
        if let Err(error) = self
            .palette_persistence
            .start_create_removing(revision, path, bytes, mask_path)
        {
            self.error = Some(error);
        }
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

fn choose_native_palette_import(kind: PendingTransfer) -> Option<std::path::PathBuf> {
    match kind {
        PendingTransfer::Raw => dialogs::choose_raw_palette_document(),
        PendingTransfer::Tpl => dialogs::choose_tpl_palette_document(),
        PendingTransfer::Rgb => dialogs::choose_rgb_palette_document(),
    }
}

fn choose_native_palette_export(kind: PendingTransfer) -> Option<std::path::PathBuf> {
    match kind {
        PendingTransfer::Raw => dialogs::choose_raw_palette_save_path(),
        PendingTransfer::Tpl => dialogs::choose_tpl_palette_save_path(),
        PendingTransfer::Rgb => dialogs::choose_rgb_palette_save_path(),
    }
}

fn imported_native_palette(
    mut installed: Palette,
    decoded: DecodedImport,
) -> Result<(Palette, Option<RgbChannelExpansion>), String> {
    match decoded {
        DecodedImport::Raw(source, mask) => {
            apply_raw_palette_import(&mut installed, &source, &mask)
                .map_err(|error| error.to_string())?;
            Ok((installed, None))
        }
        DecodedImport::Supported {
            palette,
            mask,
            rgb_expansion,
        } => {
            apply_supported_import(&mut installed, &palette, &mask)?;
            Ok((installed, rgb_expansion))
        }
    }
}

fn apply_supported_import(
    installed: &mut Palette,
    source: &Palette,
    mask: &PaletteMaskFile,
) -> Result<(), String> {
    if installed.colors.len() != RawSnesPaletteFile::COLOR_COUNT || source.colors.len() != 256 {
        return Err(format!(
            "supported palette requires 257 installed and 256 source colors, got {} and {}",
            installed.colors.len(),
            source.colors.len()
        ));
    }
    let mut staged = installed.clone();
    for (supported, color) in source.colors.iter().copied().enumerate() {
        if mask.is_selected(supported).unwrap_or(false) {
            let installed_index = if supported == 0 { 0 } else { supported + 1 };
            staged.colors[installed_index] = color;
        }
    }
    for row in 1..16 {
        let supported = row * Palette::COLORS_PER_ROW;
        if mask.is_selected(supported).unwrap_or(false) {
            staged.colors[supported + 1] = Bgr555(0);
        }
    }
    *installed = staged;
    Ok(())
}

fn supported_palette(installed: &Palette) -> Result<Palette, String> {
    if installed.colors.len() != RawSnesPaletteFile::COLOR_COUNT {
        return Err(format!(
            "supported palette export requires 257 installed colors, got {}",
            installed.colors.len()
        ));
    }
    let mut colors = Vec::with_capacity(256);
    colors.push(installed.colors[0]);
    colors.extend_from_slice(&installed.colors[2..]);
    Ok(Palette { colors })
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

    #[test]
    fn supported_transfer_uses_exact_installed_mapping_and_selected_row_zero_clearing() {
        let mut installed = Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        };
        let source = Palette {
            colors: (0_u16..256).map(|word| Bgr555(0x1000 + word)).collect(),
        };
        let mut entries = vec![0; PaletteMaskFile::FILE_LEN];
        entries[0] = 1;
        entries[16] = 0x7f;
        entries[255] = 1;
        let mask = PaletteMaskFile::decode(&entries).unwrap();

        apply_supported_import(&mut installed, &source, &mask).unwrap();
        assert_eq!(installed.colors[0], source.colors[0]);
        assert_eq!(installed.colors[1], Bgr555(1));
        assert_eq!(installed.colors[17], Bgr555(0));
        assert_eq!(installed.colors[256], source.colors[255]);
        assert_eq!(installed.colors[18], Bgr555(18));
        assert_eq!(
            supported_palette(&installed).unwrap().colors[255],
            source.colors[255]
        );
    }

    #[test]
    fn authentic_import_helpers_are_shape_checked_and_failure_atomic() {
        let mut wrong = Palette {
            colors: vec![Bgr555(7); 256],
        };
        let before = wrong.clone();
        assert!(
            apply_supported_import(
                &mut wrong,
                &Palette {
                    colors: vec![Bgr555(8); 256],
                },
                &PaletteMaskFile::all_selected(),
            )
            .is_err()
        );
        assert_eq!(wrong, before);

        let installed = Palette {
            colors: vec![Bgr555(9); RawSnesPaletteFile::COLOR_COUNT],
        };
        let source = RawSnesPaletteFile {
            palette: Palette {
                colors: vec![Bgr555(0x1234); RawSnesPaletteFile::COLOR_COUNT],
            },
        };
        let (imported, expansion) = imported_native_palette(
            installed,
            DecodedImport::Raw(source, PaletteMaskFile::all_selected()),
        )
        .unwrap();
        assert_eq!(imported.colors[0], Bgr555(0));
        assert_eq!(imported.colors[16], Bgr555(0));
        assert_eq!(imported.colors[256], Bgr555(0x1234));
        assert_eq!(expansion, None);
    }
}
