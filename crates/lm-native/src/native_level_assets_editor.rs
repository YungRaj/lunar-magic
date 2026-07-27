use crate::{
    animation_modes, dialogs,
    document_loader::{BoundedRead, DocumentLoader, LoadedDocument},
    document_persistence::DocumentPersistence,
    native_level_assets_panels::AggregatePanels,
};
use eframe::egui;
use lm_app::NativeLevelAssetsDocumentController;
use lm_graphics::PaletteOwnership;
use lm_level::SpriteLengthTable;
use lm_project::NativeLevelAssetsFile;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    path: PathBuf,
    bytes: Vec<u8>,
    lengths: SpriteLengthTable,
    modes: [bool; 256],
    maximum_records: String,
}

struct Document {
    controller: NativeLevelAssetsDocumentController,
    ownership: PaletteOwnership,
    modes: [bool; 256],
}

#[derive(Default)]
pub(crate) struct NativeLevelAssetsEditor {
    document: Option<Document>,
    pending_open: Option<PendingOpen>,
    panels: AggregatePanels,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl NativeLevelAssetsEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_native_level_assets_document() else {
            return;
        };
        let Some(length_path) = dialogs::choose_sprite_length_table() else {
            return;
        };
        let Some(mode_path) = dialogs::choose_exanimation_size_modes() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![
            BoundedRead::new(
                path,
                NativeLevelAssetsFile::MAX_FILE_LEN as u64,
                "native level assets",
            ),
            BoundedRead::new(
                length_path,
                SpriteLengthTable::ENCODED_LEN as u64,
                "sprite length table",
            ),
            BoundedRead::new(mode_path, 256, "ExAnimation size-mode table"),
        ]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for level-assets loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for level-assets persistence to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let Some(document) = &self.document else {
            return true;
        };
        if !document.controller.is_modified() {
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
        if let Some(result) = self.loader.show(context) {
            match result.and_then(pending_from_loaded) {
                Ok(pending) => self.pending_open = Some(pending),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(document) = self.document.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, &mut document.controller)
        {
            self.error = Some(error);
        }
        self.open_configuration(context);
        if self.document.is_some() {
            egui::Window::new("Native Level Assets Editor")
                .default_size([860.0, 680.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn open_configuration(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Open native level assets")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Maximum ExAnimation records from the matching revision profile:");
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.maximum_records);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let result = pending
            .maximum_records
            .trim()
            .parse::<usize>()
            .map_err(|e| format!("invalid maximum ExAnimation record count: {e}"))
            .and_then(|maximum| {
                decode(
                    pending.path.clone(),
                    &pending.bytes,
                    pending.lengths.clone(),
                    pending.modes,
                    maximum,
                )
            });
        match result {
            Ok(document) => {
                self.document = Some(document);
                self.panels.invalidate();
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let edit = self.document.as_ref().and_then(|document| {
            self.panels.show(
                ui,
                document.controller.revision(),
                document.controller.value(),
                (None, None),
                &document.modes,
                &document.ownership,
            )
        });
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply(edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let (undo, redo, modified) = (
            document.controller.can_undo(),
            document.controller.can_redo(),
            document.controller.is_modified(),
        );
        let mut action = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui.add_enabled(undo, egui::Button::new("Undo")).clicked() {
                action = Some(true);
            }
            if ui.add_enabled(redo, egui::Button::new("Redo")).clicked() {
                action = Some(false);
            }
            if ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new("Save aggregate"),
                )
                .clicked()
            {
                save = true;
            }
            ui.label(if modified { "Modified" } else { "Saved" });
        });
        if let (Some(undo), Some(document)) = (action, self.document.as_mut()) {
            let revision = document.controller.revision();
            let result = if undo {
                document.controller.undo(revision)
            } else {
                document.controller.redo(revision)
            };
            if let Err(error) = result {
                self.error = Some(error.to_string());
            } else {
                self.panels.invalidate();
            }
        }
        if save {
            self.save();
        }
    }

    fn apply(&mut self, edit: lm_app::NativeLevelAssetsControllerEdit) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Err(error) = document.controller.apply_edits(
            document.controller.revision(),
            &[edit],
            &document.ownership,
        ) {
            self.error = Some(error.to_string());
        } else {
            self.panels.invalidate();
        }
    }

    fn save(&mut self) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Err(error) = self.persistence.begin(&mut document.controller) {
            self.error = Some(error);
        }
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved native assets")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Discard changes across all native level-asset domains?");
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
            egui::Window::new("Native-assets editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }
    fn clear(&mut self) {
        self.document = None;
        self.pending_open = None;
        self.pending_close = None;
        self.panels.invalidate();
    }
}

fn pending_from_loaded(loaded: LoadedDocument) -> Result<PendingOpen, String> {
    let [(path, bytes), (_, length_bytes), (_, mode_bytes)] =
        loaded.into_exact::<3>("native-assets")?;
    let lengths = SpriteLengthTable::decode(&length_bytes)
        .map_err(|n| format!("sprite length table requires 1024 bytes, got {n}"))?;
    Ok(PendingOpen {
        path,
        bytes,
        lengths,
        modes: animation_modes::decode(&mode_bytes)?,
        maximum_records: "32".into(),
    })
}

fn decode(
    path: PathBuf,
    bytes: &[u8],
    lengths: SpriteLengthTable,
    modes: [bool; 256],
    maximum: usize,
) -> Result<Document, String> {
    let controller =
        NativeLevelAssetsDocumentController::decode(path, bytes, lengths, maximum, &modes)
            .map_err(|e| e.to_string())?;
    let ownership = PaletteOwnership::editable(controller.value().assets.palette.colors.len());
    Ok(Document {
        controller,
        ownership,
        modes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_group_binds_all_revision_interpretation_inputs() {
        let mut mode_bytes = vec![0; 256];
        mode_bytes[44] = 1;
        let pending = pending_from_loaded(LoadedDocument {
            files: vec![
                (PathBuf::from("assets.lmnat"), vec![7, 8]),
                (
                    PathBuf::from("sprite-lengths.bin"),
                    vec![0; SpriteLengthTable::ENCODED_LEN],
                ),
                (PathBuf::from("modes.bin"), mode_bytes),
            ],
        })
        .unwrap();
        assert_eq!(pending.path, PathBuf::from("assets.lmnat"));
        assert_eq!(pending.bytes, [7, 8]);
        assert!(pending.modes[44]);
        assert_eq!(pending.maximum_records, "32");
    }

    #[test]
    fn loaded_group_rejects_wrong_shape_tables() {
        assert!(
            pending_from_loaded(LoadedDocument {
                files: vec![
                    (PathBuf::from("assets.lmnat"), Vec::new()),
                    (PathBuf::from("sprite-lengths.bin"), vec![0; 1023]),
                    (PathBuf::from("modes.bin"), vec![0; 256]),
                ],
            })
            .is_err()
        );
        assert!(
            pending_from_loaded(LoadedDocument {
                files: vec![
                    (PathBuf::from("assets.lmnat"), Vec::new()),
                    (
                        PathBuf::from("sprite-lengths.bin"),
                        vec![0; SpriteLengthTable::ENCODED_LEN],
                    ),
                    (PathBuf::from("modes.bin"), vec![0; 255]),
                ],
            })
            .is_err()
        );
    }
}
