use super::{AppState, PendingClose, PendingLoad, RomGraphicsEditor, Workspace, egui};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::{GraphicsOwnershipFile, RevisionProfileControllers};
use lm_graphics::{PaletteInterchangeFile, PaletteOwnership};
use lm_rom::RomImage;

impl RomGraphicsEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let profiled = match app.profiled_controller_snapshot() {
            Ok(profiled) => profiled,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if !matches!(profiled.snapshot.mode, lm_app::EditorMode::Graphics(_)) {
            self.error =
                Some("select a graphics file before opening the ROM graphics editor".into());
            return;
        }
        let Some(path) = crate::dialogs::choose_graphics_ownership() else {
            return;
        };
        let request = BoundedRead::new(
            path,
            u64::try_from(GraphicsOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "graphics ownership evidence",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => self.pending_load = Some(PendingLoad::Ownership { profiled }),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.persistence.is_running()
            || self.graphics_batch.is_running()
            || self.graphics_import.is_running()
            || self.external_editor.is_running()
        {
            self.error =
                Some("wait for graphics background file work to finish before closing".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.controller.is_modified() {
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

    pub(super) fn finish_load(&mut self, result: Result<LoadedDocument, String>, revision: u64) {
        let pending = self.pending_load.take();
        match pending {
            Some(PendingLoad::Ownership { profiled }) => {
                match result.and_then(|loaded| decode_loaded(profiled, loaded, revision)) {
                    Ok(workspace) => {
                        self.workspace = Some(workspace);
                        self.selected_tile = 0;
                        self.foreground_color = 1;
                        self.background_color = 0;
                        self.display_palette = Default::default();
                        self.status = Default::default();
                        self.clipboard_paste_target = None;
                        self.pixel_pointer_capture =
                            crate::graphics_painter::TilePixelPointerCapture::None;
                        self.search_start.clear();
                        self.search_end.clear();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Some(PendingLoad::RawImport { expected_revision }) => {
                let outcome = result.and_then(|loaded| {
                    crate::rom_load::ensure_current_revision(
                        expected_revision,
                        revision,
                        "raw graphics import",
                    )?;
                    let [(_, bytes)] = loaded.into_exact::<1>("raw graphics")?;
                    let workspace = self
                        .workspace
                        .as_mut()
                        .ok_or("graphics workspace is closed")?;
                    workspace
                        .controller
                        .import_raw(&bytes)
                        .map_err(|error| error.to_string())
                });
                match outcome {
                    Ok(()) => self.io_status = Some("Raw graphics staged successfully.".into()),
                    Err(error) => self.error = Some(error),
                }
            }
            None => self.error = Some("graphics load lost its pending operation".into()),
        }
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged graphics changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("These changes have not been committed to the ROM.");
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
            egui::Window::new("ROM graphics error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_load = None;
        self.pending_close = None;
        self.clipboard_paste_target = None;
        self.pixel_pointer_capture = crate::graphics_painter::TilePixelPointerCapture::None;
        self.status = Default::default();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_loaded(
    profiled: lm_app::ProfiledControllerSnapshot,
    loaded: LoadedDocument,
    current_revision: u64,
) -> Result<Workspace, String> {
    crate::rom_load::ensure_current_revision(
        profiled.snapshot.revision,
        current_revision,
        "graphics ownership evidence",
    )?;
    let lm_app::EditorMode::Graphics(slot) = profiled.snapshot.mode else {
        return Err("select a graphics file before opening the ROM graphics editor".into());
    };
    let [(_, bytes)] = loaded.into_exact::<1>("graphics ownership")?;
    let ownership = GraphicsOwnershipFile::decode(&bytes)
        .map_err(|error| error.to_string())?
        .ownership;
    let controller = profiled
        .profile
        .decode_graphics(&profiled.snapshot, ownership)
        .map_err(|error| error.to_string())?;
    let mut palette_snapshot = profiled.snapshot.clone();
    palette_snapshot.mode = lm_app::EditorMode::Palette(0);
    let palette = profiled
        .profile
        .decode_palette(
            &palette_snapshot,
            PaletteOwnership::editable(profiled.profile.palette.colors_per_palette),
        )
        .map_err(|error| error.to_string())?;
    let palette = PaletteInterchangeFile {
        source_palette: 0,
        palette: palette.palette().clone(),
    };
    if palette.palette.colors.len() < 16 || palette.palette.colors.len() % 16 != 0 {
        return Err("native palette must contain complete 16-color rows".into());
    }
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    Ok(Workspace {
        controller,
        profile: profiled.profile,
        palette,
        slot,
        image,
        internal_header: profiled.snapshot.identity.internal_header_offset,
    })
}
