use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    ssc_sidecar_editor_form::{SscSourceForm, diagnostic},
};
use eframe::egui;
use lm_app::SscSidecarController;
use lm_graphics::{
    EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES, EXTERNAL_SPRITE_GRAPHICS_SLOTS,
    EXTERNAL_SPRITE_PALETTE_RGB_MAX_BYTES, EXTERNAL_SPRITE_PALETTE_SNES_MAX_BYTES,
    ExternalSpriteAssets,
};
use lm_level::MAX_SSC_SOURCE_LEN;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct SscSidecarEditor {
    controller: Option<SscSidecarController>,
    form: SscSourceForm,
    loaded_revision: Option<u64>,
    entry_index: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
    resolved: Option<lm_level::SscResolvedTable>,
    external_assets: ExternalSpriteAssets,
    asset_revision: u64,
}

impl SscSidecarEditor {
    pub(crate) fn resolved(&self) -> Option<&lm_level::SscResolvedTable> {
        self.resolved.as_ref()
    }

    pub(crate) const fn external_assets(&self) -> &ExternalSpriteAssets {
        &self.external_assets
    }

    pub(crate) const fn asset_revision(&self) -> u64 {
        self.asset_revision
    }

    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_ssc_sidecar() else {
            return;
        };
        let mut requests = external_sprite_requests(&path);
        requests.insert(
            0,
            BoundedRead::new(
                path,
                u64::try_from(MAX_SSC_SOURCE_LEN).unwrap_or(u64::MAX),
                "SSC sidecar",
            ),
        );
        if let Err(error) = self.loader.start(requests) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() || self.persistence.is_running() {
            self.error = Some("wait for SSC I/O to finish before closing".into());
            return false;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
        self.poll_io(context);
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new("Lossless SSC Custom-Sprite Metadata")
                .default_size([840.0, 680.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn poll_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(|loaded| {
                let mut files = loaded.files.into_iter();
                let (path, bytes) = files
                    .next()
                    .ok_or_else(|| "SSC loader returned no file".to_string())?;
                let controller = SscSidecarController::decode(path, &bytes)
                    .map_err(|error| error.to_string())?;
                let assets = decode_external_sprite_assets(files)?;
                Ok((controller, assets))
            }) {
                Ok((controller, assets)) => {
                    self.controller = Some(controller);
                    self.external_assets = assets;
                    self.asset_revision = self.asset_revision.wrapping_add(1);
                    self.loaded_revision = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if self.loaded_revision != Some(controller.revision()) {
            self.form = SscSourceForm::load(controller.value());
            self.resolved = Some(lm_level::SscResolvedTable::from_sidecar(controller.value()));
            self.loaded_revision = Some(controller.revision());
            self.entry_index = self
                .entry_index
                .min(controller.value().entries().len().saturating_sub(1));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let source_len = controller.value().source().len();
        let entry_count = controller.value().entries().len();
        let entry_diagnostic = controller
            .value()
            .entries()
            .get(self.entry_index)
            .map(diagnostic);
        ui.label(format!(
            "Lossless source: {source_len} bytes; valid metadata records: {entry_count}"
        ));
        let graphics_slots = (0..EXTERNAL_SPRITE_GRAPHICS_SLOTS)
            .filter(|&slot| self.external_assets.has_graphics_slot(slot))
            .count();
        ui.label(format!(
            "External sprite assets: {graphics_slots}/{} graphics slots; palette {}",
            EXTERNAL_SPRITE_GRAPHICS_SLOTS,
            if self.external_assets.has_palette() {
                "loaded"
            } else {
                "not found"
            }
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.bytes)
                .desired_rows(18)
                .code_editor(),
        );
        if ui.button("Replace complete lossless source").clicked() {
            self.replace_form();
        }
        ui.separator();
        ui.heading("Recovered-record diagnostics");
        ui.add(
            egui::Slider::new(&mut self.entry_index, 0..=entry_count.saturating_sub(1))
                .text("Parsed record"),
        );
        ui.label(entry_diagnostic.unwrap_or_else(|| "No valid metadata records.".into()));
    }

    fn replace_form(&mut self) {
        match self.form.parse() {
            Ok(bytes) => {
                let Some(controller) = self.controller.as_mut() else {
                    return;
                };
                if let Err(error) = controller.replace_source(controller.revision(), &bytes) {
                    self.error = Some(error.to_string());
                } else {
                    self.loaded_revision = None;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let mut history = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(controller.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(controller.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            save = ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked();
            ui.label(if controller.is_modified() {
                "Modified"
            } else {
                "Saved"
            });
        });
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Some(undo) = history {
            let result = if undo {
                controller.undo(controller.revision())
            } else {
                controller.redo(controller.revision())
            };
            match result {
                Ok(true) => self.loaded_revision = None,
                Ok(false) => {}
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if save && let Err(error) = self.persistence.begin(controller) {
            self.error = Some(error);
        }
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved SSC sidecar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved custom-sprite metadata changes?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("SSC sidecar error")
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

    fn clear(&mut self) {
        self.controller = None;
        self.loaded_revision = None;
        self.pending_close = None;
        self.resolved = None;
        self.external_assets = ExternalSpriteAssets::default();
        self.asset_revision = self.asset_revision.wrapping_add(1);
    }
}

pub(crate) fn external_sprite_requests(ssc_path: &Path) -> Vec<BoundedRead> {
    let Some(parent) = ssc_path.parent() else {
        return Vec::new();
    };
    let directory = parent
        .ancestors()
        .take(3)
        .map(|ancestor| ancestor.join("ExternalGraphics"))
        .find(|candidate| candidate.is_dir());
    let Some(directory) = directory else {
        return Vec::new();
    };
    let mut requests = Vec::new();
    for slot in 0..EXTERNAL_SPRITE_GRAPHICS_SLOTS {
        let path = directory.join(format!("ExSpriteGFX{slot:02X}.bin"));
        if path.is_file() {
            requests.push(BoundedRead::new(
                path,
                u64::try_from(EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES).unwrap_or(u64::MAX),
                format!("external sprite graphics slot {slot:02X}"),
            ));
        }
    }
    let mw3 = directory.join("ExSpritePalette00.mw3");
    let pal = directory.join("ExSpritePalette00.pal");
    if mw3.is_file() {
        requests.push(BoundedRead::new(
            mw3,
            u64::try_from(EXTERNAL_SPRITE_PALETTE_SNES_MAX_BYTES).unwrap_or(u64::MAX),
            "external sprite MW3 palette",
        ));
    } else if pal.is_file() {
        requests.push(BoundedRead::new(
            pal,
            u64::try_from(EXTERNAL_SPRITE_PALETTE_RGB_MAX_BYTES).unwrap_or(u64::MAX),
            "external sprite RGB palette",
        ));
    }
    requests
}

pub(crate) fn decode_external_sprite_assets(
    files: impl Iterator<Item = (PathBuf, Vec<u8>)>,
) -> Result<ExternalSpriteAssets, String> {
    let mut assets = ExternalSpriteAssets::default();
    for (path, bytes) in files {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(format!(
                "external sprite asset has a non-Unicode filename: {}",
                path.display()
            ));
        };
        if let Some(slot) = name
            .strip_prefix("ExSpriteGFX")
            .and_then(|suffix| suffix.strip_suffix(".bin"))
            .and_then(|slot| usize::from_str_radix(slot, 16).ok())
        {
            assets
                .set_graphics_slot(slot, &bytes)
                .map_err(|error| format!("could not decode {}: {error}", path.display()))?;
        } else if name == "ExSpritePalette00.mw3" {
            assets
                .set_snes_palette(&bytes)
                .map_err(|error| format!("could not decode {}: {error}", path.display()))?;
        } else if name == "ExSpritePalette00.pal" {
            assets
                .set_rgb_palette(&bytes)
                .map_err(|error| format!("could not decode {}: {error}", path.display()))?;
        } else {
            return Err(format!(
                "SSC loader returned an unexpected external asset: {}",
                path.display()
            ));
        }
    }
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-ssc-external-assets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn discovers_nearest_external_directory_and_prefers_mw3_palette() {
        let root = fixture_directory();
        let sprites = root.join("Sprites");
        let external = root.join("ExternalGraphics");
        fs::create_dir_all(&sprites).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("ExSpriteGFX00.bin"), [0; 32]).unwrap();
        fs::write(external.join("ExSpritePalette00.mw3"), [0; 2]).unwrap();
        fs::write(external.join("ExSpritePalette00.pal"), [0; 3]).unwrap();
        let requests = external_sprite_requests(&sprites.join("list.ssc"));
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].path.file_name().and_then(|name| name.to_str()),
            Some("ExSpriteGFX00.bin")
        );
        assert_eq!(
            requests[1].path.file_name().and_then(|name| name.to_str()),
            Some("ExSpritePalette00.mw3")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn decoded_group_preserves_slots_and_palette_and_rejects_unknown_files() {
        let files = vec![
            (PathBuf::from("ExSpriteGFX07.bin"), vec![0; 32]),
            (PathBuf::from("ExSpritePalette00.pal"), vec![1, 2, 3]),
        ];
        let assets = decode_external_sprite_assets(files.into_iter()).unwrap();
        assert!(assets.has_graphics_slot(7));
        assert!(assets.has_palette());
        assert!(
            decode_external_sprite_assets(
                vec![(PathBuf::from("unexpected.bin"), vec![0])].into_iter()
            )
            .is_err()
        );
    }
}
