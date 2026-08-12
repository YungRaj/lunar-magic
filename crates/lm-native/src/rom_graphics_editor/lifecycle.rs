use super::{AppState, PendingClose, PendingLoad, RomGraphicsEditor, Workspace, egui};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::{
    ExtendedUiTextKey, GraphicsOwnershipFile, LocalizationCatalog, RevisionProfileControllers,
    UiTextKey,
};
use lm_graphics::{
    EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES, EXTERNAL_SPRITE_GRAPHICS_SLOTS,
    PaletteInterchangeFile, PaletteOwnership,
};
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
        let mut requests = vec![BoundedRead::new(
            path,
            u64::try_from(GraphicsOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "graphics ownership evidence",
        )];
        if let Some(parent) = app
            .document_path
            .as_deref()
            .and_then(std::path::Path::parent)
        {
            let directory = parent.join("ExternalGraphics");
            for slot in 0..EXTERNAL_SPRITE_GRAPHICS_SLOTS {
                requests.push(BoundedRead::optional(
                    directory.join(format!("ExSpriteGFX{slot:02X}.bin")),
                    u64::try_from(EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES).unwrap_or(u64::MAX),
                    format!("external sprite graphics slot {slot:02X}"),
                ));
            }
        }
        match self.loader.start(requests) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad::Ownership {
                    profiled,
                    level: app.current_level(),
                })
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.persistence.is_running()
            || self.graphics_batch.is_running()
            || self.graphics_import.is_running()
            || self.pending_graphics_format_warning.is_some()
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
            Some(PendingLoad::Ownership { profiled, level }) => {
                match result.and_then(|loaded| decode_loaded(profiled, level, loaded, revision)) {
                    Ok(workspace) => {
                        self.edit_tile = workspace.controller.graphics().tiles.first().cloned();
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
                        self.internal_cache_unlocked = false;
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
                    Ok(()) => {
                        self.reload_edit_tile_from_selection();
                        self.io_status = Some("Raw graphics staged successfully.".into());
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            None => self.error = Some("graphics load lost its pending operation".into()),
        }
    }

    pub(super) fn close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(super::text(
            catalog,
            ExtendedUiTextKey::GraphicsDiscardTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label(super::text(
                catalog,
                ExtendedUiTextKey::GraphicsUnsavedNotice,
            ));
            ui.horizontal(|ui| {
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonCancel,
                    ))
                    .clicked()
                {
                    self.pending_close = None;
                }
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::UnsavedDiscard,
                    ))
                    .clicked()
                {
                    self.clear();
                    approved = pending == PendingClose::Application;
                }
            });
        });
        approved
    }

    pub(super) fn show_error(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(super::text(catalog, ExtendedUiTextKey::GraphicsErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(crate::frontend_ui::localized_text(
                            catalog,
                            UiTextKey::CommonOk,
                        ))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.edit_tile = None;
        self.pending_load = None;
        self.pending_close = None;
        self.pending_level_graphics_export = None;
        self.pending_graphics_format_warning = None;
        self.clipboard_paste_target = None;
        self.pixel_pointer_capture = crate::graphics_painter::TilePixelPointerCapture::None;
        self.status = Default::default();
        self.internal_cache_unlocked = false;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_loaded(
    profiled: lm_app::ProfiledControllerSnapshot,
    level: Option<u16>,
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
    let mut files = loaded.files.into_iter();
    let (_, bytes) = files
        .next()
        .ok_or("graphics ownership loader returned no ownership file")?;
    let external_sprite_assets = crate::ssc_sidecar_editor::decode_external_sprite_assets(files)?;
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
    let (internal_cache, internal_cache_error) = match level {
        Some(level) => match crate::vanilla_map16_preview::load_profiled_internal_graphics_cache(
            image.clone(),
            &profiled.profile,
            level,
            false,
            Some(&external_sprite_assets),
        ) {
            Ok(cache) => (Some(cache), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, Some("no active level is available".into())),
    };
    Ok(Workspace {
        controller,
        profile: profiled.profile,
        palette,
        slot,
        image,
        internal_header: profiled.snapshot.identity.internal_header_offset,
        level,
        internal_cache,
        internal_cache_error,
        internal_cache_special_world: false,
        internal_cache_convert_berry: true,
        external_sprite_assets,
    })
}
