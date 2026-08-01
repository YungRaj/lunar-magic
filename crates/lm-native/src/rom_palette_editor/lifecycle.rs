use super::{AppState, PendingClose, PendingLoad, RomPaletteEditor, Workspace, egui};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::{PaletteOwnershipFile, RevisionProfileControllers};
use lm_rom::RomImage;

impl RomPaletteEditor {
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
        if !matches!(profiled.snapshot.mode, lm_app::EditorMode::Palette(_)) {
            self.error = Some("select a palette before opening the ROM palette editor".into());
            return;
        }
        let Some(path) = crate::dialogs::choose_palette_ownership() else {
            return;
        };
        let request = BoundedRead::new(
            path,
            u64::try_from(PaletteOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "palette ownership evidence",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => self.pending_load = Some(PendingLoad { profiled }),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.transfer_loader.is_running()
            || self.transfer_persistence.is_running()
        {
            self.error = Some("wait for palette file work to finish before closing".into());
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

    pub(super) fn finish_ownership_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        revision: u64,
    ) {
        let pending = self.pending_load.take();
        match result.and_then(|loaded| decode_loaded(pending, loaded, revision)) {
            Ok(workspace) => {
                let colors = workspace.controller.palette().colors.len();
                self.workspace = Some(workspace);
                self.selected = 0;
                self.rgb_expansion = None;
                self.palette_mask = vec![1; colors];
                self.palette_mask_edit = false;
                self.palette_paste_target = None;
                self.search_start.clear();
                self.search_end.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged palette changes?")
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
            egui::Window::new("ROM palette error").show(context, |ui| {
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
        self.pending_transfer = None;
        self.rgb_expansion = None;
        self.palette_mask.clear();
        self.palette_mask_edit = false;
        self.palette_paste_target = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_loaded(
    pending: Option<PendingLoad>,
    loaded: LoadedDocument,
    current_revision: u64,
) -> Result<Workspace, String> {
    let profiled = pending
        .ok_or_else(|| "palette ownership load lost its controller snapshot".to_string())?
        .profiled;
    crate::rom_load::ensure_current_revision(
        profiled.snapshot.revision,
        current_revision,
        "palette ownership evidence",
    )?;
    let lm_app::EditorMode::Palette(slot) = profiled.snapshot.mode else {
        return Err("select a palette before opening the ROM palette editor".into());
    };
    let [(_, bytes)] = loaded.into_exact::<1>("palette ownership")?;
    let ownership = PaletteOwnershipFile::decode(&bytes)
        .map_err(|error| error.to_string())?
        .ownership;
    let controller = profiled
        .profile
        .decode_palette(&profiled.snapshot, ownership)
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    Ok(Workspace {
        controller,
        profile: profiled.profile,
        slot,
        image,
        internal_header: profiled.snapshot.identity.internal_header_offset,
    })
}
