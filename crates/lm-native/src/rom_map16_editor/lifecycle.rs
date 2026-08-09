use super::{AppState, Controller, PendingClose, RomMap16Editor, Workspace, egui};
use lm_app::RevisionProfileControllers;
use lm_rom::RomImage;

impl RomMap16Editor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match decode(app) {
            Ok(workspace) => {
                let logical_len = workspace.image.logical_len();
                let document_path = app.document_path.clone();
                self.workspace = Some(workspace);
                self.page = 0;
                self.tile = 0;
                self.rectangle_drag_anchor = None;
                if logical_len == 0x80_000 {
                    self.search_start = "80000".into();
                    self.search_end = "100000".into();
                } else {
                    self.search_start.clear();
                    self.search_end.clear();
                }
                self.preview_level = "105".into();
                self.preview_tileset = 0;
                self.preview_palette = 0;
                self.page_texture = None;
                self.page_texture_key = None;
                self.page_zoom_percent = 100;
                self.complete_template = None;
                self.pending_complete_revision = None;
                self.pending_selected_import = None;
                self.selected_width = "1".into();
                self.selected_height = "1".into();
                self.selected_use_file_origin = true;
                self.pending_legacy_import = None;
                self.initialize_associated_sidecars(document_path);
                self.pending_bitmap_import = None;
                self.clipboard_paste_target = None;
                self.rectangle_clipboard_paste_target = None;
                self.staged_revision = 0;
                self.undo_history.clear();
                self.redo_history.clear();
                self.bitmap_session = None;
                self.bitmap_extra_slot_4.clear();
                self.bitmap_extra_slot_5.clear();
                self.bitmap_original_texture = None;
                self.bitmap_converted_texture = None;
                self.bitmap_preview_zoom = 1;
                self.bitmap_preview_scroll = egui::Vec2::ZERO;
                self.pending_snes_tileset = None;
                self.snes_tileset_preview = None;
                if !self.snes_tileset_options_initialized {
                    self.snes_tileset_include_palette = false;
                    self.snes_tileset_palette_row = 0;
                    self.snes_tileset_deduplicate = true;
                    self.snes_tileset_graphics_offset = 0;
                    self.snes_tileset_map_offset = 0;
                    self.snes_tileset_color_filter = false;
                    self.snes_tileset_color_filter_index = 0;
                    self.snes_tileset_color_maps =
                        std::array::from_fn(|_| std::array::from_fn(|index| index as u8));
                    self.snes_tileset_options_initialized = true;
                }
                self.invalidate();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running() {
            self.error = Some("wait for RATS ownership loading to finish before closing".into());
            return false;
        }
        if self.bitmap_loader.is_running() {
            self.error = Some("wait for bitmap loading to finish before closing".into());
            return false;
        }
        if self.bitmap_clipboard_loader.is_running() {
            self.error = Some("wait for clipboard bitmap loading to finish before closing".into());
            return false;
        }
        if self.complete_loader.is_running() {
            self.error = Some("wait for complete Map16 loading to finish before closing".into());
            return false;
        }
        if self.complete_persistence.is_running() {
            self.error = Some("wait for complete Map16 saving to finish before closing".into());
            return false;
        }
        if self.selected_loader.is_running() {
            self.error = Some("wait for selected Map16 loading to finish before closing".into());
            return false;
        }
        if self.selected_persistence.is_running() {
            self.error = Some("wait for selected Map16 saving to finish before closing".into());
            return false;
        }
        if self.legacy_page_loader.is_running() {
            self.error = Some("wait for Map16 page loading to finish before closing".into());
            return false;
        }
        if self.legacy_page_persistence.is_running() {
            self.error = Some("wait for Map16 page saving to finish before closing".into());
            return false;
        }
        if self.associated_sidecar_loader.is_running() {
            self.error =
                Some("wait for associated Map16 sidecars to finish loading before closing".into());
            return false;
        }
        if self.associated_sidecar_persistence.is_running() {
            self.error =
                Some("wait for associated Map16 sidecar export to finish before closing".into());
            return false;
        }
        if self.pending_sidecar_export.is_some() {
            self.error =
                Some("answer the associated Map16 sidecar export prompt before closing".into());
            return false;
        }
        if self.snes_tileset_loader.is_running() {
            self.error = Some("wait for SNES tileset loading to finish before closing".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.controller.is_modified()
            && self.bitmap_session.is_none()
            && self.snes_tileset_preview.is_none()
        {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged Map16 changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    "These Map16 changes or bitmap import have not been committed to the ROM.",
                );
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

    pub(super) fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM Map16 error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.bitmap_session = None;
        self.bitmap_original_texture = None;
        self.bitmap_converted_texture = None;
        self.bitmap_preview_zoom = 1;
        self.bitmap_preview_scroll = egui::Vec2::ZERO;
        self.complete_template = None;
        self.pending_complete_revision = None;
        self.pending_selected_import = None;
        self.pending_legacy_import = None;
        self.associated_sidecar_paths = None;
        self.associated_m16 = None;
        self.associated_s16 = None;
        self.pending_sidecar_export = None;
        self.sidecar_export_in_flight = None;
        self.pending_bitmap_import = None;
        self.pending_snes_tileset = None;
        self.snes_tileset_preview = None;
        self.clipboard_paste_target = None;
        self.rectangle_clipboard_paste_target = None;
        self.rectangle_drag_anchor = None;
        self.staged_revision = 0;
        self.undo_history.clear();
        self.redo_history.clear();
        self.pending_close = None;
        self.invalidate();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode(app: &AppState) -> Result<Workspace, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let (controller, profile) = if let Ok(profiled) = app.profiled_controller_snapshot() {
        match profiled.profile.decode_map16(&profiled.snapshot) {
            Ok(controller) => (Controller::Profile(controller), Some(profiled.profile)),
            Err(_) => (
                Controller::Smw(
                    lm_app::SmwMap16Controller::decode(&snapshot)
                        .map_err(|error| error.to_string())?,
                ),
                None,
            ),
        }
    } else {
        (
            Controller::Smw(
                lm_app::SmwMap16Controller::decode(&snapshot).map_err(|error| error.to_string())?,
            ),
            None,
        )
    };
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let internal_header = snapshot.identity.internal_header_offset;
    Ok(Workspace {
        controller,
        profile,
        snapshot,
        image,
        internal_header,
    })
}
