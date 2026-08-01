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
                self.workspace = Some(workspace);
                self.page = 0;
                self.tile = 0;
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
                self.complete_template = None;
                self.pending_legacy_page = None;
                self.bitmap_session = None;
                self.bitmap_extra_slot_4.clear();
                self.bitmap_extra_slot_5.clear();
                self.bitmap_palette_row = 4;
                self.bitmap_map16_start =
                    format!("{:04X}", self.page * lm_level::Map16Page::TILE_COUNT);
                self.bitmap_original_texture = None;
                self.bitmap_converted_texture = None;
                self.bitmap_preview_zoom = 1;
                self.bitmap_preview_scroll = egui::Vec2::ZERO;
                self.bitmap_fixed_palette_entries =
                    [false; lm_graphics::Palette::COLORS_PER_ROW - 1];
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
        if self.legacy_page_loader.is_running() {
            self.error = Some("wait for Map16 page loading to finish before closing".into());
            return false;
        }
        if self.legacy_page_persistence.is_running() {
            self.error = Some("wait for Map16 page saving to finish before closing".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.controller.is_modified() && self.bitmap_session.is_none() {
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
        self.bitmap_fixed_palette_entries = [false; lm_graphics::Palette::COLORS_PER_ROW - 1];
        self.complete_template = None;
        self.pending_legacy_page = None;
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
