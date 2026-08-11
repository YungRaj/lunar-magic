use crate::{
    document_loader::DocumentLoader,
    graphics_batch,
    graphics_insertion_dialog::{
        GraphicsInsertionDialog, GraphicsInsertionFamily, GraphicsInsertionRequest,
    },
    graphics_painter::{
        GraphicsCharacterShortcut, GraphicsColorMapEditor, GraphicsDisplayPalette,
        GraphicsEditorStatus, GraphicsTileGrid, GraphicsTileTransform, PalettePointerAction,
        TILE_EDITOR_SIDE, TILE_GRID_COLUMNS, TilePixelPointerAction, TilePixelPointerCapture,
        TilePointerAction, apply_tile_keyboard_navigation, apply_tile_navigation,
        apply_tile_palette_keyboard, apply_tile_palette_step, color_selection_marker,
        graphics_navigation_controls, graphics_transform_controls, paint_tile, palette_color,
        palette_pointer_action, shortcut_transform, take_graphics_character_shortcut,
        take_graphics_refresh_shortcut, take_internal_graphics_cache_unlock,
        take_tile_grid_shortcut, take_tile_shift, tile_button, tile_coordinate, tile_page_range,
        tile_pixel_pointer_action, tile_pointer_action,
    },
    level_graphics_export::{
        CurrentLevelGraphicsAssignments, LUNAR_MAGIC_ALL_GFX_FILE_SIZES, LevelGraphicsExportMode,
        current_level_graphics_assignments, current_level_graphics_files, extracted_graphics_paths,
        extracted_joined_graphics_paths, take_level_graphics_export_shortcut,
    },
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, GraphicsController, GraphicsControllerEdit, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_graphics::{
    ExAnimationFeature, GraphicsTileChange, IndexedTile, PaletteInterchangeFile, TileShift,
};

mod commit;
mod external_edit;
mod graphics_import;
mod lifecycle;
mod ownership;

const STANDARD_GFX_LIMIT: usize = 0x34;
const EXGFX_FIRST: usize = 0x80;
const EXGFX_LIMIT: usize = 0x1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}
struct Workspace {
    controller: GraphicsController,
    profile: RevisionProfile,
    palette: PaletteInterchangeFile,
    slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
    level: Option<u16>,
    internal_cache: Option<crate::vanilla_map16_preview::VanillaInternalGraphicsCache>,
    internal_cache_error: Option<String>,
    internal_cache_special_world: bool,
    internal_cache_convert_berry: bool,
    external_sprite_assets: lm_graphics::ExternalSpriteAssets,
}

enum PendingLoad {
    Ownership {
        profiled: ProfiledControllerSnapshot,
        level: Option<u16>,
    },
    RawImport {
        expected_revision: u64,
    },
}

#[derive(Clone)]
enum PendingGraphicsFormatWarningTarget {
    Directory(std::path::PathBuf),
    Joined(std::path::PathBuf),
}

#[derive(Clone)]
struct PendingGraphicsFormatWarning {
    source: graphics_import::GraphicsImportSource,
    target: PendingGraphicsFormatWarningTarget,
    combined: Option<(
        graphics_import::GraphicsImportSource,
        PendingGraphicsFormatWarningTarget,
    )>,
}

const GRAPHICS_FORMAT_WARNING_TITLE: &str = "Graphics Format Change Warning!";
const GRAPHICS_FORMAT_WARNING_BODY: &str = "The GFX are about to be inserted as 4bpp, but any ExGFX already in the ROM are still stored in 3bpp format.  Make sure to re-insert the ExGFX too after this so the program can store them as 4bpp as well (if you don't yet have an external copy of them, you should cancel this and extract the ExGFX first).  Unless for some reason you actually like looking at garbled graphics...\n\nProceed anyway?";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuickGraphicsInsertion {
    Standard,
    ExGraphics,
}

#[derive(Default)]
pub(crate) struct RomGraphicsEditor {
    workspace: Option<Workspace>,
    selected_tile: usize,
    edit_tile: Option<IndexedTile>,
    foreground_color: u8,
    background_color: u8,
    display_palette: GraphicsDisplayPalette,
    tile_grid: GraphicsTileGrid,
    color_map: GraphicsColorMapEditor,
    pending_shift: Option<TileShift>,
    pending_character_shortcut: Option<GraphicsCharacterShortcut>,
    pixel_pointer_capture: TilePixelPointerCapture,
    clipboard_paste_target: Option<usize>,
    status: GraphicsEditorStatus,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    pending_level_graphics_export: Option<LevelGraphicsExportMode>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    persistence: crate::persistence_worker::PersistenceWorker,
    next_persistence_request: u64,
    io_status: Option<String>,
    graphics_batch: graphics_batch::GraphicsBatchWorker,
    level_graphics_batch_running: bool,
    graphics_import: graphics_import::GraphicsImportWorker,
    pending_graphics_format_warning: Option<PendingGraphicsFormatWarning>,
    ordinary_insertion_dialog: Option<GraphicsInsertionDialog>,
    external_editor: external_edit::ExternalGraphicsEditor,
    external_tool_id: Option<String>,
    internal_cache_unlocked: bool,
    convert_berry_gfx_tile: Option<bool>,
    install_4bpp_on_insert: Option<bool>,
}

impl RomGraphicsEditor {
    pub(crate) fn toggle_grid_color(&mut self) -> &'static str {
        self.tile_grid
            .apply_f8(egui::Modifiers {
                ctrl: true,
                alt: true,
                ..egui::Modifiers::NONE
            })
            .expect("Ctrl+Alt+F8 always reports the selected grid color")
    }

    pub(crate) fn set_convert_berry_gfx_tile(&mut self, enabled: bool) {
        self.convert_berry_gfx_tile = Some(enabled);
    }

    pub(crate) fn toggle_install_4bpp_on_insert(&mut self) -> bool {
        let enabled = !self.install_4bpp_on_insert.unwrap_or(true);
        self.install_4bpp_on_insert = Some(enabled);
        enabled
    }

    #[cfg(test)]
    pub(crate) fn install_4bpp_on_insert(&self) -> bool {
        self.install_4bpp_on_insert.unwrap_or(true)
    }

    pub(crate) fn open_ordinary_import(
        &mut self,
        app: &AppState,
        family: GraphicsInsertionFamily,
    ) -> Result<(), String> {
        if self.graphics_import.is_running()
            || self.pending_graphics_format_warning.is_some()
            || self.ordinary_insertion_dialog.is_some()
        {
            return Err("a graphics insertion is already running".into());
        }
        if self.error.is_some() {
            return Err("dismiss the current ROM graphics error before inserting GFX".into());
        }
        if modified_controller(self.workspace.as_ref()) {
            return Err("commit or discard staged graphics edits before inserting GFX".into());
        }
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let image =
            lm_rom::RomImage::from_bytes(snapshot.rom_bytes).map_err(|error| error.to_string())?;
        let copier_prefix_len = match image.copier_header() {
            lm_rom::CopierHeader::Absent => 0,
            lm_rom::CopierHeader::Present => lm_rom::COPIER_HEADER_LEN,
        };
        // Lunar Magic's Options-menu toggle is the session default for both insertion dialogs.
        // Once the irreversible runtime is present, clearing the option cannot uninstall it.
        let use_4bpp = lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&image)
            || self.install_4bpp_on_insert.unwrap_or(true);
        self.ordinary_insertion_dialog = Some(GraphicsInsertionDialog::new(
            family,
            copier_prefix_len,
            image.logical_len(),
            use_4bpp,
        ));
        Ok(())
    }

    pub(crate) fn start_quick_import(
        &mut self,
        app: &AppState,
        action: QuickGraphicsInsertion,
        joined_standard: bool,
    ) -> Result<(), String> {
        if self.graphics_import.is_running() || self.pending_graphics_format_warning.is_some() {
            return Err("a graphics insertion is already running".into());
        }
        if self.error.is_some() {
            return Err("dismiss the current ROM graphics error before inserting GFX".into());
        }
        if modified_controller(self.workspace.as_ref()) {
            return Err("commit or discard staged graphics edits before inserting GFX".into());
        }
        let (mut source, target) = quick_graphics_import_source(app, action, joined_standard)?;
        source.convert_berry_gfx_tile = self.convert_berry_gfx_tile.unwrap_or(true);
        self.start_graphics_import_or_warn(source, target);
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn start_insert_all_graphics(
        &mut self,
        app: &AppState,
        joined_standard: bool,
    ) -> Result<(), String> {
        if self.graphics_import.is_running()
            || self.pending_graphics_format_warning.is_some()
            || self.ordinary_insertion_dialog.is_some()
        {
            return Err("a graphics insertion is already running".into());
        }
        if self.error.is_some() {
            return Err("dismiss the current ROM graphics error before inserting GFX".into());
        }
        if modified_controller(self.workspace.as_ref()) {
            return Err("commit or discard staged graphics edits before inserting GFX".into());
        }
        let (mut standard_source, standard_target) =
            quick_graphics_import_source(app, QuickGraphicsInsertion::Standard, joined_standard)?;
        standard_source.convert_berry_gfx_tile = self.convert_berry_gfx_tile.unwrap_or(true);
        let (mut extended_source, extended_target) =
            quick_graphics_import_source(app, QuickGraphicsInsertion::ExGraphics, joined_standard)?;
        extended_source.convert_berry_gfx_tile = self.convert_berry_gfx_tile.unwrap_or(true);
        let pending = PendingGraphicsFormatWarning {
            source: standard_source,
            target: standard_target,
            combined: Some((extended_source, extended_target)),
        };
        if pending.source.smw_us_v1_standard_install
            && lm_profile::requires_smw_us_v1_4bpp_graphics_warning(&pending.source.image)
        {
            self.pending_graphics_format_warning = Some(pending);
            self.io_status = None;
        } else {
            self.start_pending_graphics_import(pending);
        }
        if let Some(error) = self.error.take() {
            Err(error)
        } else {
            Ok(())
        }
    }

    fn refresh_internal_cache(&mut self, level: Option<u16>, special_world_passed: bool) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if workspace.level == level
            && workspace.internal_cache_special_world == special_world_passed
            && workspace.internal_cache_convert_berry == self.convert_berry_gfx_tile.unwrap_or(true)
        {
            return;
        }
        workspace.level = level;
        workspace.internal_cache_special_world = special_world_passed;
        workspace.internal_cache_convert_berry = self.convert_berry_gfx_tile.unwrap_or(true);
        let result = level
            .ok_or_else(|| "no active level is available".to_owned())
            .and_then(|level| {
                crate::vanilla_map16_preview::load_profiled_internal_graphics_cache_with_berry_conversion(
                    workspace.image.clone(),
                    &workspace.profile,
                    level,
                    special_world_passed,
                    Some(&workspace.external_sprite_assets),
                    self.convert_berry_gfx_tile.unwrap_or(true),
                )
            });
        match result {
            Ok(cache) => {
                workspace.internal_cache = Some(cache);
                workspace.internal_cache_error = None;
            }
            Err(error) => {
                workspace.internal_cache = None;
                workspace.internal_cache_error = Some(error);
                self.internal_cache_unlocked = false;
            }
        }
        if self.internal_cache_unlocked {
            match self.synchronize_active_graphics_into_internal_cache() {
                Ok(()) => self.reload_edit_tile_from_selection(),
                Err(error) => {
                    self.internal_cache_unlocked = false;
                    self.error = Some(error);
                }
            }
        }
    }

    fn synchronize_active_graphics_into_internal_cache(&mut self) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "graphics workspace is closed".to_owned())?;
        let level = workspace
            .level
            .ok_or_else(|| "no active level is available".to_owned())?;
        let assignments = current_level_graphics_assignments(
            &workspace.image,
            &workspace.profile,
            level,
            workspace.internal_cache_special_world,
        )?;
        let file = usize::from(workspace.slot);
        let tiles = workspace.controller.graphics().tiles.clone();
        let cache = self
            .workspace
            .as_mut()
            .and_then(|workspace| workspace.internal_cache.as_mut())
            .ok_or_else(|| "internal graphics cache is unavailable".to_owned())?;
        overlay_current_graphics_file(cache, &assignments, file, &tiles)
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        special_world_passed: bool,
        joined_graphics_files: &mut bool,
        convert_berry_gfx_tile: bool,
    ) -> (bool, Option<Command>) {
        self.convert_berry_gfx_tile = Some(convert_berry_gfx_tile);
        let revision = app.project_revision();
        if let Some(result) = self.loader.show(context) {
            self.finish_load(result, revision);
        }
        self.refresh_internal_cache(app.current_level(), special_world_passed);
        if let Some(completion) = self.persistence.show(context) {
            self.io_status = Some(match completion.result {
                Ok(()) => "Raw graphics file extracted successfully.".into(),
                Err(error) => format!("Raw graphics extraction failed: {error}"),
            });
        }
        if let Some(result) = self.graphics_batch.show(context) {
            let level_graphics = std::mem::take(&mut self.level_graphics_batch_running);
            self.io_status = Some(if level_graphics {
                match result {
                    Ok(Some(_)) => "Saved FG/BG/SP GFX to files.".into(),
                    Ok(None) => "GFX extraction cancelled.".into(),
                    Err(error) => {
                        self.error = Some(error);
                        "Couldn't save FG/BG/SP GFX to file!".into()
                    }
                }
            } else {
                match result {
                    Ok(Some(count)) => format!("Extracted {count} GFX files successfully."),
                    Ok(None) => "GFX extraction cancelled.".into(),
                    Err(error) => format!("GFX extraction failed: {error}"),
                }
            });
        }
        if let Some(result) = self.external_editor.show(context) {
            match result.and_then(|completion| {
                ensure_external_edit_revision(completion.expected_revision, revision)?;
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("graphics workspace is closed")?;
                workspace
                    .controller
                    .import_raw(&completion.bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {
                    self.io_status = Some("Externally edited graphics staged successfully.".into());
                }
                Err(error) => self.error = Some(error),
            }
        }
        let import_command = match self.graphics_import.show(context) {
            Some(Ok(Some(commit))) => {
                self.io_status = Some("GFX directory prepared successfully.".into());
                Some(commit.into_command())
            }
            Some(Ok(None)) => {
                self.io_status = Some("GFX insertion cancelled.".into());
                None
            }
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        let mut command =
            import_command.or_else(|| match self.manifest_loader.show(context, revision) {
                Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                    Ok(command) => Some(command),
                    Err(error) => {
                        self.error = Some(error);
                        None
                    }
                },
                Some(Err(error)) => {
                    self.error = Some(error);
                    None
                }
                None => None,
            });
        self.graphics_format_warning(context);
        self.ordinary_graphics_insertion_dialog(context, app, *joined_graphics_files);
        if self.workspace.is_some() {
            egui::Window::new("ROM Graphics Editor")
                .default_size([780.0, 680.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, app, joined_graphics_files)
                        && command.is_none()
                    {
                        command = Some(ui_command);
                    }
                });
        }
        self.level_graphics_export_confirmation(context, app, special_world_passed);
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        app: &AppState,
        joined_graphics_files: &mut bool,
    ) -> Option<Command> {
        take_graphics_refresh_shortcut(ui);
        let revision = app.project_revision();
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        let file_work_running = self.loader.is_running()
            || self.persistence.is_running()
            || self.graphics_batch.is_running()
            || self.graphics_import.is_running()
            || self.pending_graphics_format_warning.is_some()
            || self.ordinary_insertion_dialog.is_some()
            || self.external_editor.is_running();
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let rows = workspace.palette.palette.colors.len() / 16;
        let special_graphics_available = pristine_special_graphics(&workspace.profile);
        let native_exgraphics = supports_native_exgraphics(&workspace.profile, &workspace.image);
        let exgraphics_available = supports_exgraphics(&workspace.profile);
        let exgraphics_insert_available = exgraphics_available || native_exgraphics;
        let configured_graphics_tools = app
            .external_tools()
            .iter()
            .filter(|tool| {
                tool.uses_graphics_editor_argument()
                    && !tool.uses_graphics_editor_working_directory()
            })
            .map(|tool| (tool.id.clone(), tool.name.clone()))
            .collect::<Vec<_>>();
        if !configured_graphics_tools
            .iter()
            .any(|(id, _)| Some(id) == self.external_tool_id.as_ref())
        {
            self.external_tool_id = configured_graphics_tools.first().map(|(id, _)| id.clone());
        }
        let previous_display_palette = self.display_palette;
        egui::ComboBox::from_label("Palette row")
            .selected_text(self.display_palette.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.display_palette,
                    GraphicsDisplayPalette::Default,
                    "Default",
                );
                for row in 0..rows {
                    ui.selectable_value(
                        &mut self.display_palette,
                        GraphicsDisplayPalette::Row(row),
                        format!("{row:X}"),
                    );
                }
            });
        if self.display_palette != previous_display_palette {
            self.status
                .set_pointer_action(self.display_palette.status());
        }
        let palette = workspace.palette.clone();
        let mut hovered_color = None;
        let mut selected_foreground = None;
        let mut selected_background = None;
        ui.horizontal_wrapped(|ui| {
            for color in 0_u8..16 {
                let fill = palette_color(&palette, self.display_palette, color);
                let response = ui.add_sized(
                    [26.0, 26.0],
                    egui::Button::new(color_selection_marker(
                        color,
                        self.foreground_color,
                        self.background_color,
                    ))
                    .fill(fill),
                );
                match palette_pointer_action(&response) {
                    Some(PalettePointerAction::SelectForeground) => {
                        self.foreground_color = color;
                        selected_foreground = Some(color);
                    }
                    Some(PalettePointerAction::SelectBackground) => {
                        self.background_color = color;
                        selected_background = Some(color);
                    }
                    None => {}
                }
                if response.hovered() {
                    hovered_color = Some(color);
                }
            }
        });
        ui.checkbox(joined_graphics_files, "Use joined AllGFX.bin files")
            .on_hover_text("Original global joined-GFX mode (command $24BD)");
        self.status.update_palette_hover(
            hovered_color,
            ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO),
        );
        if let Some(color) = selected_foreground {
            self.status.select_foreground_color(color);
        }
        if let Some(color) = selected_background {
            self.status.select_background_color(color);
        }
        ui.separator();
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Configured graphics editor")
                .selected_text(
                    self.external_tool_id
                        .as_deref()
                        .and_then(|selected| {
                            configured_graphics_tools
                                .iter()
                                .find(|(id, _)| id == selected)
                                .map(|(_, name)| name.as_str())
                        })
                        .unwrap_or("None"),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &configured_graphics_tools {
                        ui.selectable_value(&mut self.external_tool_id, Some(id.clone()), name);
                    }
                });
            if ui
                .add_enabled(
                    !stale && !file_work_running && self.external_tool_id.is_some(),
                    egui::Button::new("Edit with configured tool"),
                )
                .clicked()
            {
                self.begin_configured_external_edit(app);
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Edit with executable…"),
                )
                .clicked()
            {
                self.begin_direct_external_edit(revision);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Insert raw GFX/ExGFX…"),
                )
                .clicked()
            {
                self.begin_raw_import(revision);
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract raw GFX/ExGFX…"),
                )
                .clicked()
            {
                self.begin_raw_export();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && app.current_level().is_some(),
                    egui::Button::new("Extract current level GFX…"),
                )
                .on_hover_text(
                    "Choose a new directory for the active level's decoded FG/BG/SP files",
                )
                .clicked()
            {
                self.pending_level_graphics_export =
                    Some(LevelGraphicsExportMode::ChooseNewDirectory);
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract all standard GFX…"),
                )
                .clicked()
            {
                self.begin_graphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && special_graphics_available,
                    egui::Button::new("Extract GFX32/GFX33…"),
                )
                .on_hover_text("Uses the authenticated pristine SMW special-pointer operands")
                .clicked()
            {
                self.begin_special_graphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && exgraphics_available,
                    egui::Button::new("Extract installed ExGFX…"),
                )
                .on_hover_text("Exports every nonempty ExGFX pointer from the installed table")
                .clicked()
            {
                self.begin_exgraphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract AllGFX.bin…"),
                )
                .clicked()
            {
                self.begin_all_gfx_export();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && !modified_controller(self.workspace.as_ref()),
                    egui::Button::new("Insert all standard GFX…"),
                )
                .on_hover_text("Commit or discard staged tile edits before inserting a directory")
                .clicked()
            {
                self.begin_graphics_import();
            }
            if ui
                .add_enabled(
                    !stale
                        && !file_work_running
                        && !modified_controller(self.workspace.as_ref())
                        && special_graphics_available,
                    egui::Button::new("Insert GFX32/GFX33…"),
                )
                .on_hover_text("Uses the authenticated pristine SMW special-pointer operands")
                .clicked()
            {
                self.begin_special_graphics_import();
            }
            if ui
                .add_enabled(
                    !stale
                        && !file_work_running
                        && !modified_controller(self.workspace.as_ref())
                        && exgraphics_insert_available,
                    egui::Button::new("Insert ExGFX…"),
                )
                .on_hover_text("Atomically inserts the canonical ExGFX files found in a directory")
                .clicked()
            {
                self.begin_exgraphics_import();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && !modified_controller(self.workspace.as_ref()),
                    egui::Button::new("Insert AllGFX.bin…"),
                )
                .on_hover_text("Commit or discard staged tile edits before inserting AllGFX.bin")
                .clicked()
            {
                self.begin_all_gfx_import();
            }
        });
        if let Some(status) = &self.io_status {
            ui.label(status);
        }
        ui.separator();
        ui.columns(2, |columns| {
            self.tile_list(
                &mut columns[0],
                &palette,
                !stale && !file_work_running,
                !stale && !file_work_running && app.current_level().is_some(),
                *joined_graphics_files,
            );
            self.pixel_editor(
                &mut columns[1],
                &palette,
                stale || file_work_running,
                pasted.as_deref(),
            );
        });
        self.status.show(ui);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        let commit_enabled =
            modified && !stale && !file_work_running && !self.manifest_loader.is_running();
        let commit_clicked = ui
            .add_enabled(commit_enabled, egui::Button::new("Commit graphics to ROM"))
            .clicked();
        if commit_enabled && commit_clicked {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !file_work_running && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged graphics changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn tile_list(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        edits_enabled: bool,
        level_export_enabled: bool,
        joined_graphics_files: bool,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let diagnostic = self.internal_cache_unlocked;
        let diagnostic_paste_context = diagnostic.then(|| diagnostic_paste_context(workspace));
        let internal_cache_available = workspace.internal_cache.is_some();
        let internal_cache_error = workspace.internal_cache_error.clone();
        let tiles = if diagnostic {
            let Some(cache) = &workspace.internal_cache else {
                self.internal_cache_unlocked = false;
                self.error = Some(
                    workspace
                        .internal_cache_error
                        .clone()
                        .unwrap_or_else(|| "internal graphics cache is unavailable".into()),
                );
                return;
            };
            ui.small("Internal GFX data — transient working cache; F9 publishes current-level FG/BG/SP slots");
            &cache.tiles
        } else {
            &workspace.controller.graphics().tiles
        };
        let tile_count = tiles.len();
        self.selected_tile = self.selected_tile.min(tiles.len().saturating_sub(1));
        let selection_before = self.selected_tile;
        let mut reload_selected_edit_tile = false;
        let page = tile_page_range(self.selected_tile, tile_count);
        let (page_start, page_end) = (page.start, page.end);
        let mut responses = Vec::with_capacity(page_end.saturating_sub(page_start));
        let mut selected_by_pointer = None;
        let selected_tile = self
            .edit_tile
            .clone()
            .or_else(|| tiles.get(self.selected_tile).cloned());
        let mut selected_paste = None;
        let mut clipboard_paste = None;
        let mut copied = false;
        let mut paste_status = None;
        let row_count = palette.palette.colors.len() / 16;
        let (page_control, palette_control) =
            graphics_navigation_controls(ui, tile_count > 0, row_count > 0);
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                egui::Grid::new("rom-graphics-tiles")
                    .spacing([0.0, 0.0])
                    .show(ui, |ui| {
                        for (offset, tile) in tiles[page_start..page_end].iter().enumerate() {
                            let index = page_start + offset;
                            let response = tile_button(
                                ui,
                                tile,
                                palette,
                                self.display_palette,
                                index == self.selected_tile,
                                self.tile_grid,
                            );
                            match tile_pointer_action(&response, index) {
                                Some(TilePointerAction::Select(index)) => {
                                    self.selected_tile = index;
                                    selected_by_pointer = Some(index);
                                    reload_selected_edit_tile = true;
                                }
                                Some(TilePointerAction::Copy(index)) => {
                                    self.selected_tile = index;
                                    reload_selected_edit_tile = true;
                                    match native_clipboard::copy_graphics_tile_to_system(
                                        ui.ctx(),
                                        tile,
                                    ) {
                                        Ok(()) => copied = true,
                                        Err(error) => self.error = Some(error),
                                    }
                                }
                                Some(TilePointerAction::PasteSelected(index)) => {
                                    let owner = (!diagnostic)
                                        .then(|| workspace.controller.ownership().owner(index))
                                        .flatten();
                                    if edits_enabled
                                        && ((diagnostic
                                            && diagnostic_sheet_paste_editable(
                                                index,
                                                diagnostic_paste_context
                                                    .expect("diagnostic paste context exists"),
                                            ))
                                            || (!diagnostic && ownership::is_editable(owner)))
                                        && let Some(tile) = selected_tile.clone()
                                    {
                                        selected_paste = Some((index, tile));
                                    }
                                }
                                Some(TilePointerAction::PasteClipboard(index)) => {
                                    let owner = (!diagnostic)
                                        .then(|| workspace.controller.ownership().owner(index))
                                        .flatten();
                                    if edits_enabled
                                        && ((diagnostic
                                            && diagnostic_sheet_paste_editable(
                                                index,
                                                diagnostic_paste_context
                                                    .expect("diagnostic paste context exists"),
                                            ))
                                            || (!diagnostic && ownership::is_editable(owner)))
                                    {
                                        self.clipboard_paste_target = Some(index);
                                        match native_clipboard::request_graphics_tile_paste(
                                            ui.ctx(),
                                        ) {
                                            Ok(Some(tile)) => {
                                                self.clipboard_paste_target = None;
                                                clipboard_paste = Some((index, tile));
                                            }
                                            Ok(None) => {}
                                            Err(error) => {
                                                self.clipboard_paste_target = None;
                                                self.error = Some(error);
                                            }
                                        }
                                    }
                                }
                                None => {}
                            }
                            responses.push(response);
                            if index % TILE_GRID_COLUMNS == TILE_GRID_COLUMNS - 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
        if let Some((index, tile)) = selected_paste
            && self.apply_tile_at(index, tile)
        {
            paste_status = Some(format!("Pasted selected tile over tile 0x{index:X}."));
        }
        if let Some((index, tile)) = clipboard_paste
            && self.apply_tile_at(index, tile.clone())
        {
            self.clipboard_paste_target = None;
            self.selected_tile = index;
            self.edit_tile = Some(tile);
            reload_selected_edit_tile = false;
            paste_status = Some(format!("Pasted tile from clipboard over tile 0x{index:X}."));
        }
        let selected_owner = (!diagnostic)
            .then(|| {
                self.workspace.as_ref().and_then(|workspace| {
                    workspace.controller.ownership().owner(self.selected_tile)
                })
            })
            .flatten();
        let tile_shift_enabled =
            edits_enabled && (diagnostic || ownership::is_editable(selected_owner));
        let navigation_status = if let Some(navigation) = page_control {
            apply_tile_navigation(&mut self.selected_tile, &responses, tile_count, navigation)
        } else {
            apply_tile_keyboard_navigation(
                ui,
                &mut self.selected_tile,
                &responses,
                tile_count,
                tile_shift_enabled,
            )
        };
        let palette_status = if let Some(step) = palette_control {
            apply_tile_palette_step(&mut self.display_palette, row_count, step)
        } else {
            apply_tile_palette_keyboard(
                ui,
                self.selected_tile,
                &responses,
                &mut self.display_palette,
                row_count,
            )
        };
        let hovered_owner = (!diagnostic)
            .then(|| {
                responses
                    .iter()
                    .position(egui::Response::hovered)
                    .and_then(|offset| {
                        self.workspace.as_ref().and_then(|workspace| {
                            workspace.controller.ownership().owner(page_start + offset)
                        })
                    })
            })
            .flatten();
        self.status.update_tile_hover(
            &responses,
            page_start,
            ui.input(|input| input.modifiers),
            hovered_owner,
            ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO),
        );
        if let Some(status) = navigation_status.or(palette_status) {
            self.status.set(status);
        }
        if take_internal_graphics_cache_unlock(ui, self.selected_tile, &responses) {
            if internal_cache_available {
                match self.synchronize_active_graphics_into_internal_cache() {
                    Ok(()) => {
                        self.internal_cache_unlocked = true;
                        reload_selected_edit_tile = true;
                        self.status.set("Internal GFX data viewing unlocked.");
                    }
                    Err(error) => self.error = Some(error),
                }
            } else {
                self.error = Some(
                    internal_cache_error
                        .unwrap_or_else(|| "internal graphics cache is unavailable".into()),
                );
            }
        }
        if let Some(index) = selected_by_pointer {
            self.status.select_tile(index);
        }
        if let Some(status) = paste_status {
            self.status.set(status);
        }
        if copied {
            self.status.set("Copied tile to clipboard.");
        }
        let owner = (!diagnostic)
            .then(|| {
                self.workspace.as_ref().and_then(|workspace| {
                    workspace.controller.ownership().owner(self.selected_tile)
                })
            })
            .flatten();
        self.pending_shift = take_tile_shift(
            ui,
            self.selected_tile,
            &responses,
            edits_enabled && (diagnostic || ownership::is_editable(owner)),
        );
        self.pending_character_shortcut =
            take_graphics_character_shortcut(ui, self.selected_tile, &responses);
        if let Some(status) =
            take_tile_grid_shortcut(ui, self.selected_tile, &responses, &mut self.tile_grid)
        {
            self.status.set(status);
        }
        if level_export_enabled && take_level_graphics_export_shortcut(ui) {
            self.pending_level_graphics_export = Some(if joined_graphics_files {
                LevelGraphicsExportMode::ReplaceJoined
            } else {
                LevelGraphicsExportMode::ReplaceExtracted
            });
        }
        if reload_selected_edit_tile || self.selected_tile != selection_before {
            self.reload_edit_tile_from_selection();
        }
    }

    fn level_graphics_export_confirmation(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        special_world_passed: bool,
    ) {
        let Some(mode) = self.pending_level_graphics_export else {
            return;
        };
        let mut accepted = false;
        let mut cancelled = false;
        egui::Window::new("Save level GFX to Graphics folder?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.label("Do you want to save the current level GFX to file,");
                ui.label("so it can be inserted to the ROM later?");
                ui.label("Don't do this if you haven't extracted the graphics yet!");
                ui.horizontal(|ui| {
                    accepted = ui.button("Yes").clicked();
                    cancelled = ui.button("No").clicked();
                });
            });
        if accepted {
            self.pending_level_graphics_export = None;
            self.begin_level_graphics_batch(app, special_world_passed, mode);
        } else if cancelled || context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.pending_level_graphics_export = None;
        }
    }

    fn begin_level_graphics_batch(
        &mut self,
        app: &AppState,
        special_world_passed: bool,
        mode: LevelGraphicsExportMode,
    ) {
        let Some(level) = app.current_level() else {
            self.error = Some("no active level is available for GFX extraction".into());
            return;
        };
        let Some(workspace) = &self.workspace else {
            return;
        };
        let slots = match current_level_graphics_files(
            &workspace.image,
            &workspace.profile,
            level,
            special_world_passed,
        ) {
            Ok(slots) => slots,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let raw = match workspace.controller.export_raw() {
            Ok(raw) => raw,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let mut raw_4bpp_overrides = slots
            .contains(&usize::from(workspace.slot))
            .then(|| vec![(usize::from(workspace.slot), raw)])
            .unwrap_or_default();
        if self.internal_cache_unlocked {
            let Some(cache) = workspace.internal_cache.as_ref() else {
                self.error = Some("internal graphics cache is unavailable".into());
                return;
            };
            let assignments = match current_level_graphics_assignments(
                &workspace.image,
                &workspace.profile,
                level,
                special_world_passed,
            ) {
                Ok(assignments) => assignments,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
            raw_4bpp_overrides = match internal_cache_level_graphics_overrides(cache, &assignments)
            {
                Ok(overrides) => overrides,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
        }
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "level",
            exgraphics_names: false,
            encoding: graphics_batch::GraphicsBatchEncoding::Decoded4Bpp,
            convert_berry_gfx_tile: true,
            raw_4bpp_overrides,
            file_layouts: Vec::new(),
        };
        let start = match mode {
            LevelGraphicsExportMode::ChooseNewDirectory => {
                let Some(directory) = crate::dialogs::choose_level_graphics_directory() else {
                    return;
                };
                self.graphics_batch.start(source, directory)
            }
            LevelGraphicsExportMode::ReplaceExtracted => {
                let Some(rom_path) = app.document_path.as_deref() else {
                    self.error = Some("the open ROM has no document path".into());
                    return;
                };
                match extracted_graphics_paths(rom_path, &source.file_numbers) {
                    Ok(paths) => self.graphics_batch.start_replace(
                        source,
                        paths.standard_directory,
                        paths.exgraphics_directory,
                        paths.required_existing,
                    ),
                    Err(error) => Err(error),
                }
            }
            LevelGraphicsExportMode::ReplaceJoined => {
                let Some(rom_path) = app.document_path.as_deref() else {
                    self.error = Some("the open ROM has no document path".into());
                    return;
                };
                match extracted_joined_graphics_paths(rom_path, &source.file_numbers) {
                    Ok(paths) => self.graphics_batch.start_replace_joined(
                        source,
                        paths.all_gfx_path,
                        paths.exgraphics_directory,
                        paths.required_existing,
                        LUNAR_MAGIC_ALL_GFX_FILE_SIZES.to_vec(),
                    ),
                    Err(error) => Err(error),
                }
            }
        };
        match start {
            Ok(()) => {
                self.level_graphics_batch_running = true;
                self.io_status = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn pixel_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        stale: bool,
        pasted: Option<&str>,
    ) {
        let diagnostic = self.internal_cache_unlocked;
        if let Some(text) = pasted {
            let target = self
                .clipboard_paste_target
                .take()
                .unwrap_or(self.selected_tile);
            let target_editable = self
                .workspace
                .as_ref()
                .is_some_and(|workspace| paste_target_editable(workspace, diagnostic, target));
            if !stale && target_editable {
                match native_clipboard::decode_graphics_tile(text) {
                    Ok(tile) => {
                        if self.apply_tile_at(target, tile.clone()) {
                            self.selected_tile = target;
                            self.edit_tile = Some(tile);
                            self.status.set(format!(
                                "Pasted tile from clipboard over tile 0x{target:X}."
                            ));
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        let selected = self.workspace.as_ref().and_then(|workspace| {
            if diagnostic {
                workspace
                    .internal_cache
                    .as_ref()
                    .and_then(|cache| cache.tiles.get(self.selected_tile))
            } else {
                workspace
                    .controller
                    .graphics()
                    .tiles
                    .get(self.selected_tile)
            }
        });
        let has_tile = selected.is_some();
        if !has_tile {
            ui.label("No graphics tiles");
            return;
        }
        ui.label(format!("Tile {:03X}", self.selected_tile));
        let owner = (!diagnostic)
            .then(|| {
                self.workspace.as_ref().and_then(|workspace| {
                    workspace.controller.ownership().owner(self.selected_tile)
                })
            })
            .flatten();
        let editable = if diagnostic {
            ui.label("Internal working-cache tile; edits are transient unless F9 owns its current-level file.");
            true
        } else {
            ownership::show(ui, owner)
        };
        let paste_editable = self.workspace.as_ref().is_some_and(|workspace| {
            paste_target_editable(workspace, diagnostic, self.selected_tile)
        });
        let Some(mut tile) = self.edit_tile.clone().or_else(|| selected.cloned()) else {
            ui.label("No graphics tiles");
            return;
        };
        let character_shortcut = self.pending_character_shortcut.take();
        if character_shortcut == Some(GraphicsCharacterShortcut::EditColorMap) {
            self.color_map.open_dialog();
        }
        let clicked_mapping =
            self.color_map
                .show(ui, palette, self.display_palette, &tile, !stale && editable);
        let mapped = (character_shortcut == Some(GraphicsCharacterShortcut::ApplyColorMap)
            && !stale
            && editable)
            .then(|| self.color_map.apply(&tile))
            .flatten()
            .or(clicked_mapping);
        if let Some(mapped) = mapped {
            tile = mapped;
            self.stage_tile(tile.clone());
        }
        if let Some(direction) = self.pending_shift.take() {
            tile = tile.shifted_wrapping(direction);
            self.stage_tile(tile.clone());
        }
        let mut native_clipboard_tile = None;
        ui.horizontal(|ui| {
            if ui.button("Copy tile").clicked() {
                if let Err(error) = native_clipboard::copy_graphics_tile_to_system(ui.ctx(), &tile)
                {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale && paste_editable, egui::Button::new("Paste tile"))
                .clicked()
            {
                match native_clipboard::request_graphics_tile_paste(ui.ctx()) {
                    Ok(Some(tile)) => native_clipboard_tile = Some(tile),
                    Ok(None) => {}
                    Err(error) => self.error = Some(error),
                }
            }
        });
        if let Some(pasted) = native_clipboard_tile
            && self.apply_tile_at(self.selected_tile, pasted.clone())
        {
            tile = pasted.clone();
            self.edit_tile = Some(pasted);
            self.status.set(format!(
                "Pasted tile from clipboard over tile 0x{:X}.",
                self.selected_tile
            ));
        }
        let enabled = !stale && editable;
        let clicked_transform = graphics_transform_controls(ui, enabled);
        let transform = shortcut_transform(character_shortcut)
            .filter(|_| enabled)
            .or(clicked_transform);
        if let Some(transform) = transform {
            let transformed = match transform {
                GraphicsTileTransform::RotateClockwise => tile.rotated_clockwise(),
                GraphicsTileTransform::FlipHorizontal => tile.flipped(true, false),
                GraphicsTileTransform::FlipVertical => tile.flipped(false, true),
            };
            tile = transformed;
            self.stage_tile(tile.clone());
        }
        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::splat(TILE_EDITOR_SIDE),
            egui::Sense::click_and_drag(),
        );
        self.status.update_pixel_editor_hover(
            response.hovered(),
            self.selected_tile,
            ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO),
        );
        paint_tile(ui.painter(), rect, &tile, palette, self.display_palette);
        if let Some(action) = tile_pixel_pointer_action(
            &response,
            ui.input(|input| input.modifiers),
            &mut self.pixel_pointer_capture,
        ) && let Some(position) = response.interact_pointer_pos()
            && let Some((x, y)) = tile_coordinate(rect, position)
        {
            match action {
                TilePixelPointerAction::PaintForeground if !stale && editable => {
                    self.apply_pixel(x, y, self.foreground_color, tile);
                }
                TilePixelPointerAction::PaintBackground if !stale && editable => {
                    self.apply_pixel(x, y, self.background_color, tile);
                }
                TilePixelPointerAction::PickForeground => {
                    self.foreground_color = tile.pixel(x, y).unwrap_or(0);
                    self.status.select_foreground_color(self.foreground_color);
                }
                TilePixelPointerAction::PickBackground => {
                    self.background_color = tile.pixel(x, y).unwrap_or(0);
                    self.status.select_background_color(self.background_color);
                }
                _ => {}
            }
        }
    }
    fn apply_pixel(&mut self, x: usize, y: usize, color: u8, mut tile: IndexedTile) {
        if let Err(error) = tile.set_pixel(x, y, color) {
            self.error = Some(error.to_string());
            return;
        }
        self.stage_tile(tile);
    }
    fn stage_tile(&mut self, tile: IndexedTile) {
        self.edit_tile = Some(tile);
    }

    fn reload_edit_tile_from_selection(&mut self) {
        self.edit_tile = self.selected_tile_clone();
    }
    fn apply_tile_at(&mut self, index: usize, tile: IndexedTile) -> bool {
        if self.internal_cache_unlocked {
            let Some(cache) = self
                .workspace
                .as_mut()
                .and_then(|workspace| workspace.internal_cache.as_mut())
            else {
                self.error = Some("internal graphics cache is unavailable".into());
                return false;
            };
            return match replace_internal_cache_tile(cache, index, tile) {
                Ok(()) => true,
                Err(error) => {
                    self.error = Some(error);
                    false
                }
            };
        }
        let edit = GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange { index, tile }]);
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("graphics workspace is closed".into());
            return false;
        };
        match workspace.controller.apply_edits(&[edit]) {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn selected_tile_clone(&self) -> Option<IndexedTile> {
        let workspace = self.workspace.as_ref()?;
        if self.internal_cache_unlocked {
            workspace
                .internal_cache
                .as_ref()?
                .tiles
                .get(self.selected_tile)
                .cloned()
        } else {
            workspace
                .controller
                .graphics()
                .tiles
                .get(self.selected_tile)
                .cloned()
        }
    }
}

fn replace_internal_cache_tile(
    cache: &mut crate::vanilla_map16_preview::VanillaInternalGraphicsCache,
    index: usize,
    tile: IndexedTile,
) -> Result<(), String> {
    let cache_tile = cache
        .tiles
        .get_mut(index)
        .ok_or_else(|| format!("internal graphics tile {index:X} is unavailable"))?;
    *cache_tile = tile;
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) struct DiagnosticSheetPasteContext {
    pub(crate) extended_foreground_background: bool,
    pub(crate) vanilla_animation_enabled: bool,
    pub(crate) special_world_passed: bool,
}

pub(crate) const fn diagnostic_sheet_paste_editable(
    index: usize,
    context: DiagnosticSheetPasteContext,
) -> bool {
    if index > 0x5ff {
        return false;
    }
    if !context.extended_foreground_background && index >= 0x300 && index < 0x400 {
        return false;
    }
    if context.vanilla_animation_enabled
        && ((index > 0x40 && index < 0x82)
            || index == 0x90
            || index == 0x91
            || (index >= 0xda && index < 0xde)
            || (index >= 0xea && index < 0xee))
    {
        return false;
    }
    if context.special_world_passed && index >= 0x480 && index < 0x500 {
        return false;
    }
    true
}

fn diagnostic_paste_context(workspace: &Workspace) -> DiagnosticSheetPasteContext {
    let assignments = workspace.level.and_then(|level| {
        current_level_graphics_assignments(
            &workspace.image,
            &workspace.profile,
            level,
            workspace.internal_cache_special_world,
        )
        .ok()
    });
    let extended_foreground_background = assignments
        .as_ref()
        .is_some_and(|assignments| assignments.super_graphics_bypass_enabled);
    let vanilla_animation_enabled = workspace
        .level
        .and_then(|level| {
            lm_project::Project::new(workspace.image.clone())
                .load_installed_exanimation_features(
                    usize::from(level),
                    workspace.profile.exanimation_feature_installation,
                )
                .ok()
        })
        .map_or(true, |features| {
            features
                .options
                .enabled(ExAnimationFeature::VanillaAnimation)
        });
    DiagnosticSheetPasteContext {
        extended_foreground_background,
        vanilla_animation_enabled,
        special_world_passed: workspace.internal_cache_special_world,
    }
}

fn paste_target_editable(workspace: &Workspace, diagnostic: bool, index: usize) -> bool {
    paste_target_permitted(
        diagnostic,
        index,
        diagnostic.then(|| diagnostic_paste_context(workspace)),
        ownership::is_editable(workspace.controller.ownership().owner(index)),
    )
}

const fn paste_target_permitted(
    diagnostic: bool,
    index: usize,
    diagnostic_context: Option<DiagnosticSheetPasteContext>,
    ordinary_editable: bool,
) -> bool {
    if diagnostic {
        match diagnostic_context {
            Some(context) => diagnostic_sheet_paste_editable(index, context),
            None => false,
        }
    } else {
        ordinary_editable
    }
}

pub(crate) fn overlay_current_graphics_file(
    cache: &mut crate::vanilla_map16_preview::VanillaInternalGraphicsCache,
    assignments: &CurrentLevelGraphicsAssignments,
    file: usize,
    tiles: &[IndexedTile],
) -> Result<(), String> {
    let destinations = assignments
        .foreground_background
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, assigned)| (assigned, slot * 0x80))
        .chain(
            assignments
                .sprites
                .iter()
                .copied()
                .enumerate()
                .map(|(slot, assigned)| (assigned, 0x400 + slot * 0x80)),
        )
        .filter(|(assigned, _)| *assigned == file)
        .collect::<Vec<_>>();
    if destinations.is_empty() {
        return Ok(());
    }
    let source = tiles.get(..0x80).ok_or_else(|| {
        format!(
            "active GFX{file:02X} has {} decoded tiles instead of 80",
            tiles.len()
        )
    })?;
    for (assigned, start) in destinations {
        let end = start + 0x80;
        let destination = cache.tiles.get_mut(start..end).ok_or_else(|| {
            format!(
                "internal cache does not contain active GFX{assigned:02X} slot {start:X}..{end:X}"
            )
        })?;
        destination.clone_from_slice(source);
    }
    Ok(())
}

pub(crate) fn internal_cache_level_graphics_overrides(
    cache: &crate::vanilla_map16_preview::VanillaInternalGraphicsCache,
    assignments: &CurrentLevelGraphicsAssignments,
) -> Result<Vec<(usize, Vec<u8>)>, String> {
    let mut overrides = Vec::<(usize, Vec<u8>)>::new();
    let sources = assignments
        .foreground_background
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, file)| (file, slot * 0x80))
        .chain(
            assignments
                .sprites
                .iter()
                .copied()
                .enumerate()
                .map(|(slot, file)| (file, 0x400 + slot * 0x80)),
        );
    for (file, start) in sources {
        if file == 0x7f {
            continue;
        }
        let end = start + 0x80;
        let tiles = cache.tiles.get(start..end).ok_or_else(|| {
            format!("internal cache does not contain current-level slot {start:X}..{end:X}")
        })?;
        let raw = lm_graphics::GraphicsFile4bpp {
            tiles: tiles.to_vec(),
        }
        .encode()
        .map_err(|error| format!("cannot encode current-level GFX{file:02X}: {error}"))?;
        if let Some((_, existing)) = overrides
            .iter_mut()
            .find(|(existing_file, _)| *existing_file == file)
        {
            *existing = raw;
        } else {
            overrides.push((file, raw));
        }
    }
    Ok(overrides)
}

impl RomGraphicsEditor {
    fn begin_direct_external_edit(&mut self, revision: u64) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let bytes = match workspace.controller.export_raw() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let Some(executable) = crate::dialogs::choose_external_graphics_editor() else {
            return;
        };
        let file_name = crate::dialogs::raw_graphics_file_name(workspace.slot);
        match self
            .external_editor
            .stage(executable, &file_name, &bytes, revision)
        {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_configured_external_edit(&mut self, app: &AppState) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(tool_id) = self.external_tool_id.as_deref() else {
            self.error = Some("select a configured graphics editor".into());
            return;
        };
        let Some(tool) = app.external_tools().iter().find(|tool| tool.id == tool_id) else {
            self.error = Some(format!(
                "configured graphics editor {tool_id:?} is unavailable"
            ));
            return;
        };
        if !tool.uses_graphics_editor_argument() {
            self.error = Some(format!(
                "configured graphics editor {tool_id:?} does not reference {{graphics}} or %1"
            ));
            return;
        }
        if tool.uses_graphics_editor_working_directory() {
            self.error = Some(format!(
                "configured graphics editor {tool_id:?} cannot use {{graphics}} or %1 as its working directory"
            ));
            return;
        }
        let bytes = match workspace.controller.export_raw() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let file_name = crate::dialogs::raw_graphics_file_name(workspace.slot);
        match self.external_editor.stage_configured(
            tool,
            app.tool_context(),
            &file_name,
            &bytes,
            app.project_revision(),
        ) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_raw_import(&mut self, revision: u64) {
        let maximum = match self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.controller.export_raw().ok())
            .and_then(|bytes| u64::try_from(bytes.len()).ok())
        {
            Some(maximum) => maximum,
            None => {
                self.error = Some("could not determine the current raw graphics size".into());
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_raw_graphics() else {
            return;
        };
        let request = crate::document_loader::BoundedRead::new(path, maximum, "raw GFX/ExGFX file");
        match self.loader.start(vec![request]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad::RawImport {
                    expected_revision: revision,
                });
                self.io_status = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_raw_export(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(path) = crate::dialogs::choose_raw_graphics_save_path(workspace.slot) else {
            return;
        };
        let bytes = match workspace.controller.export_raw() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        self.next_persistence_request = self.next_persistence_request.wrapping_add(1);
        let target = match crate::persistence_worker::PersistenceTarget::save_as(path) {
            Ok(target) => target,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if let Err(error) = self
            .persistence
            .start(self.next_persistence_request, target, bytes)
        {
            self.error = Some(error);
        } else {
            self.io_status = None;
        }
    }

    fn begin_graphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(directory) = crate::dialogs::choose_graphics_directory() else {
            return;
        };
        let mut source = match standard_graphics_batch_source(
            workspace.image.clone(),
            workspace.profile.graphics,
            pristine_special_graphics(&workspace.profile),
        ) {
            Ok(source) => source,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        source.convert_berry_gfx_tile = self.convert_berry_gfx_tile.unwrap_or(true);
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_graphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_graphics_import_directory() else {
            return;
        };
        let pristine_install = pristine_special_graphics(&workspace.profile)
            && !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&workspace.image);
        let (slots, file_numbers) = if pristine_install {
            let files = (0..0x34).collect::<Vec<_>>();
            (files.clone(), files)
        } else {
            let slots = standard_graphics_slots(workspace.profile.graphics);
            (slots.clone(), slots)
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots,
            file_numbers,
            family: "standard",
            description: "Insert all standard GFX files",
            smw_us_v1_special: false,
            smw_us_v1_standard_install: pristine_install,
            smw_us_v1_exgraphics: false,
            exgraphics_names: false,
            ordinary_options: None,
            convert_berry_gfx_tile: self.convert_berry_gfx_tile.unwrap_or(true),
        };
        self.start_graphics_import_or_warn(
            source,
            PendingGraphicsFormatWarningTarget::Directory(directory),
        );
    }

    fn begin_all_gfx_export(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(path) = crate::dialogs::choose_all_gfx_save_path() else {
            return;
        };
        let (slots, file_numbers, file_layouts, encoding) =
            match lunar_magic_standard_graphics_sources(&workspace.profile, &workspace.image) {
                Ok(sources) => sources,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers,
            family: "standard",
            exgraphics_names: false,
            encoding,
            convert_berry_gfx_tile: true,
            raw_4bpp_overrides: Vec::new(),
            file_layouts,
        };
        match self.graphics_batch.start_joined(source, path) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_all_gfx_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_all_gfx_file() else {
            return;
        };
        let pristine_install = pristine_special_graphics(&workspace.profile)
            && !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&workspace.image);
        let (slots, file_numbers) = if pristine_install {
            let files = (0..0x34).collect::<Vec<_>>();
            (files.clone(), files)
        } else {
            let slots = standard_graphics_slots(workspace.profile.graphics);
            (slots.clone(), slots)
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots,
            file_numbers,
            family: "standard",
            description: "Insert AllGFX.bin",
            smw_us_v1_special: false,
            smw_us_v1_standard_install: pristine_install,
            smw_us_v1_exgraphics: false,
            exgraphics_names: false,
            ordinary_options: None,
            convert_berry_gfx_tile: self.convert_berry_gfx_tile.unwrap_or(true),
        };
        self.start_graphics_import_or_warn(
            source,
            PendingGraphicsFormatWarningTarget::Joined(path),
        );
    }

    fn begin_special_graphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let layouts = match lm_profile::smw_us_v1_special_graphics_layouts(&workspace.image) {
            Ok(layouts) => layouts,
            Err(error) => {
                self.error = Some(format!("cannot resolve live GFX32/GFX33: {error}"));
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_graphics_directory() else {
            return;
        };
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: layouts.gfx33,
            slots: vec![0, 1],
            file_numbers: vec![0x33, 0x32],
            family: "special",
            exgraphics_names: false,
            encoding: graphics_batch::GraphicsBatchEncoding::Native,
            convert_berry_gfx_tile: true,
            raw_4bpp_overrides: Vec::new(),
            file_layouts: vec![(0, layouts.gfx33), (0, layouts.gfx32)],
        };
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_special_graphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let layouts = match lm_profile::smw_us_v1_special_graphics_layouts(&workspace.image) {
            Ok(layouts) => layouts,
            Err(error) => {
                self.error = Some(format!("cannot resolve live GFX32/GFX33: {error}"));
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_graphics_import_directory() else {
            return;
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: layouts.gfx33,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: vec![0, 1],
            file_numbers: vec![0x33, 0x32],
            family: "special",
            description: "Insert GFX32/GFX33 files",
            smw_us_v1_special: true,
            smw_us_v1_standard_install: false,
            smw_us_v1_exgraphics: false,
            exgraphics_names: false,
            ordinary_options: None,
            convert_berry_gfx_tile: self.convert_berry_gfx_tile.unwrap_or(true),
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_exgraphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(directory) = crate::dialogs::choose_exgraphics_directory() else {
            return;
        };
        let source =
            match exgraphics_batch_source(workspace.image.clone(), workspace.profile.graphics) {
                Ok(source) => source,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_exgraphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_exgraphics_import_directory() else {
            return;
        };
        let slots = match graphics_import::enumerate_exgraphics_files(
            &directory,
            if supports_native_exgraphics(&workspace.profile, &workspace.image) {
                EXGFX_LIMIT
            } else {
                workspace.profile.graphics.pointers.entries
            },
        ) {
            Ok(slots) => slots,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: slots.clone(),
            file_numbers: slots,
            family: "extended",
            description: "Insert ExGFX files",
            smw_us_v1_special: false,
            smw_us_v1_standard_install: false,
            smw_us_v1_exgraphics: supports_native_exgraphics(&workspace.profile, &workspace.image),
            exgraphics_names: true,
            ordinary_options: None,
            convert_berry_gfx_tile: self.convert_berry_gfx_tile.unwrap_or(true),
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn start_graphics_import_or_warn(
        &mut self,
        source: graphics_import::GraphicsImportSource,
        target: PendingGraphicsFormatWarningTarget,
    ) {
        if source.smw_us_v1_standard_install
            && lm_profile::requires_smw_us_v1_4bpp_graphics_warning(&source.image)
        {
            self.pending_graphics_format_warning = Some(PendingGraphicsFormatWarning {
                source,
                target,
                combined: None,
            });
            self.io_status = None;
            return;
        }
        self.start_graphics_import(source, target);
    }

    fn ordinary_graphics_insertion_dialog(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        joined_standard: bool,
    ) {
        let completion = self
            .ordinary_insertion_dialog
            .as_mut()
            .and_then(|dialog| dialog.show(context));
        match completion {
            Some(None) => {
                self.ordinary_insertion_dialog = None;
                self.io_status = Some("GFX insertion cancelled.".into());
            }
            Some(Some(request)) => {
                self.ordinary_insertion_dialog = None;
                if let Err(error) =
                    self.start_ordinary_graphics_import(app, request, joined_standard)
                {
                    self.error = Some(error);
                }
            }
            None => {}
        }
    }

    fn start_ordinary_graphics_import(
        &mut self,
        app: &AppState,
        request: GraphicsInsertionRequest,
        joined_standard: bool,
    ) -> Result<(), String> {
        let action = match request.family {
            GraphicsInsertionFamily::Standard => QuickGraphicsInsertion::Standard,
            GraphicsInsertionFamily::ExGraphics => QuickGraphicsInsertion::ExGraphics,
        };
        let (mut source, target) = quick_graphics_import_source(app, action, joined_standard)?;
        source.convert_berry_gfx_tile = self.convert_berry_gfx_tile.unwrap_or(true);
        let has_4bpp = lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&source.image);
        if request.family == GraphicsInsertionFamily::ExGraphics && request.use_4bpp && !has_4bpp {
            return Err("insert regular GFX as 4bpp before 4bpp ExGFX insertion".into());
        }
        let supported_first_native = match request.family {
            GraphicsInsertionFamily::Standard => source.smw_us_v1_standard_install,
            GraphicsInsertionFamily::ExGraphics => source.smw_us_v1_exgraphics,
        };
        if !has_4bpp && !supported_first_native {
            let requested_format = if request.use_4bpp { "4bpp" } else { "3bpp" };
            return Err(match request.family {
                GraphicsInsertionFamily::Standard => {
                    format!(
                        "ordinary first-time {requested_format} GFX insertion is not yet available"
                    )
                }
                GraphicsInsertionFamily::ExGraphics => {
                    "insert regular GFX as 4bpp before ordinary ExGFX insertion".into()
                }
            });
        }
        // Lunar Magic documents the 4bpp patch as irreversible. Clearing the box after the patch
        // exists therefore retains the installed format and changes no runtime byte.
        let expansion_target = (request.expand_rom
            && source.image.logical_len() < request.family.expansion_target())
        .then(|| request.family.expansion_target());
        source.ordinary_options = Some(graphics_import::OrdinaryGraphicsImportOptions {
            logical_pc_address: request.logical_pc_address,
            expansion_target,
            use_4bpp: request.use_4bpp,
        });
        source.description = match request.family {
            GraphicsInsertionFamily::Standard => {
                "Insert all standard GFX files with ordinary options"
            }
            GraphicsInsertionFamily::ExGraphics => "Insert ExGFX files with ordinary options",
        };
        self.start_graphics_import_or_warn(source, target);
        if let Some(error) = self.error.take() {
            Err(error)
        } else {
            Ok(())
        }
    }

    fn start_graphics_import(
        &mut self,
        source: graphics_import::GraphicsImportSource,
        target: PendingGraphicsFormatWarningTarget,
    ) {
        let result = match target {
            PendingGraphicsFormatWarningTarget::Directory(directory) => {
                self.graphics_import.start(source, directory)
            }
            PendingGraphicsFormatWarningTarget::Joined(path) => {
                self.graphics_import.start_joined(source, path)
            }
        };
        match result {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn graphics_format_warning(&mut self, context: &egui::Context) {
        if self.pending_graphics_format_warning.is_none() {
            return;
        }
        let mut proceed = false;
        let mut cancel = false;
        egui::Window::new(GRAPHICS_FORMAT_WARNING_TITLE)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.set_max_width(560.0);
                ui.label(GRAPHICS_FORMAT_WARNING_BODY);
                ui.horizontal(|ui| {
                    proceed = ui.button("Yes").clicked();
                    cancel = ui.button("No").clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape));
                });
            });
        if cancel {
            self.pending_graphics_format_warning = None;
            self.io_status = Some("GFX insertion cancelled.".into());
        } else if proceed && let Some(pending) = self.pending_graphics_format_warning.take() {
            self.start_pending_graphics_import(pending);
        }
    }

    fn start_pending_graphics_import(&mut self, pending: PendingGraphicsFormatWarning) {
        let result = match pending.combined {
            Some((extended_source, PendingGraphicsFormatWarningTarget::Directory(extended))) => {
                match pending.target {
                    PendingGraphicsFormatWarningTarget::Directory(standard) => self
                        .graphics_import
                        .start_combined(pending.source, standard, extended_source, extended),
                    PendingGraphicsFormatWarningTarget::Joined(standard) => self
                        .graphics_import
                        .start_combined_joined(pending.source, standard, extended_source, extended),
                }
            }
            Some((_, PendingGraphicsFormatWarningTarget::Joined(_))) => {
                Err("combined ExGFX insertion requires the ExGraphics directory".into())
            }
            None => {
                self.start_graphics_import(pending.source, pending.target);
                return;
            }
        };
        match result {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }
}

fn modified_controller(workspace: Option<&Workspace>) -> bool {
    workspace.is_some_and(|workspace| workspace.controller.is_modified())
}

fn quick_graphics_import_source(
    app: &AppState,
    action: QuickGraphicsInsertion,
    joined_standard: bool,
) -> Result<
    (
        graphics_import::GraphicsImportSource,
        PendingGraphicsFormatWarningTarget,
    ),
    String,
> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let rom_path = snapshot
        .document_path
        .as_deref()
        .ok_or("save the ROM to a named path before quick GFX insertion")?;
    let parent = rom_path
        .parent()
        .ok_or("the open ROM path has no parent directory")?;
    let image = lm_rom::RomImage::from_bytes(snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let (layout, options, smw_us_special) = match app.profiled_controller_snapshot() {
        Ok(profiled) => {
            let allocation = profiled
                .profile
                .allocation_policy_for_rom(
                    0..image.logical_len(),
                    &image,
                    snapshot.identity.internal_header_offset,
                )
                .map_err(|error| error.to_string())?;
            (
                profiled.profile.graphics,
                lm_project::GraphicsSaveOptions {
                    allocation,
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
                pristine_special_graphics(&profiled.profile),
            )
        }
        Err(lm_app::AppError::NoRevisionProfile)
            if snapshot.identity.game == lm_rom::SupportedGame::SuperMarioWorld
                && snapshot.identity.region == lm_rom::Region::NorthAmerica
                && snapshot.identity.revision == 0
                && snapshot.identity.mapper == lm_rom::Mapper::LoRom =>
        {
            (
                lm_profile::smw_us_v1_vanilla_graphics_layout(),
                lm_project::GraphicsSaveOptions {
                    allocation: lm_rats::AllocationPolicy {
                        search: 0..image.logical_len(),
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0x00, 0xff],
                        protected: Vec::new(),
                    },
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
                true,
            )
        }
        Err(error) => return Err(error.to_string()),
    };
    let checksum_field = snapshot.identity.internal_header_offset + 0x1c;
    match action {
        QuickGraphicsInsertion::Standard => {
            let pristine_install =
                smw_us_special && !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&image);
            let (slots, file_numbers) = if pristine_install {
                let files = (0..0x34).collect::<Vec<_>>();
                (files.clone(), files)
            } else {
                let slots = standard_graphics_slots(layout);
                (slots.clone(), slots)
            };
            let source = graphics_import::GraphicsImportSource {
                expected_revision: snapshot.revision,
                image,
                layout,
                checksum_field,
                options,
                slots,
                file_numbers,
                family: "standard",
                description: "Quick insert all standard GFX files",
                smw_us_v1_special: false,
                smw_us_v1_standard_install: pristine_install,
                smw_us_v1_exgraphics: false,
                exgraphics_names: false,
                ordinary_options: None,
                convert_berry_gfx_tile: true,
            };
            let target = if joined_standard {
                PendingGraphicsFormatWarningTarget::Joined(
                    parent.join("Graphics").join("AllGFX.bin"),
                )
            } else {
                PendingGraphicsFormatWarningTarget::Directory(parent.join("Graphics"))
            };
            Ok((source, target))
        }
        QuickGraphicsInsertion::ExGraphics => {
            let native_exgraphics = smw_us_special;
            let directory = parent.join("ExGraphics");
            let slots = graphics_import::enumerate_exgraphics_files(
                &directory,
                if native_exgraphics {
                    EXGFX_LIMIT
                } else {
                    layout.pointers.entries
                },
            )?;
            Ok((
                graphics_import::GraphicsImportSource {
                    expected_revision: snapshot.revision,
                    image,
                    layout,
                    checksum_field,
                    options,
                    slots: slots.clone(),
                    file_numbers: slots,
                    family: "extended",
                    description: "Quick insert ExGFX files",
                    smw_us_v1_special: false,
                    smw_us_v1_standard_install: false,
                    smw_us_v1_exgraphics: native_exgraphics,
                    exgraphics_names: true,
                    ordinary_options: None,
                    convert_berry_gfx_tile: true,
                },
                PendingGraphicsFormatWarningTarget::Directory(directory),
            ))
        }
    }
}

fn ensure_external_edit_revision(expected: u64, current: u64) -> Result<(), String> {
    crate::rom_load::ensure_current_revision(expected, current, "external graphics reload")
}

pub(crate) fn pristine_special_graphics(profile: &RevisionProfile) -> bool {
    profile.game == lm_rom::SupportedGame::SuperMarioWorld
        && profile.region == lm_rom::Region::NorthAmerica
        && profile.revision == 0
        && profile.mapper == lm_rom::Mapper::LoRom
        && profile.graphics == lm_profile::smw_us_v1_vanilla_graphics_layout()
}

fn supports_exgraphics(profile: &RevisionProfile) -> bool {
    (EXGFX_FIRST + 1..=EXGFX_LIMIT).contains(&profile.graphics.pointers.entries)
}

fn supports_native_exgraphics(profile: &RevisionProfile, _image: &lm_rom::RomImage) -> bool {
    pristine_special_graphics(profile)
}

fn standard_graphics_slots(layout: lm_project::GraphicsRomLayout) -> Vec<usize> {
    (0..layout.pointers.entries.min(STANDARD_GFX_LIMIT)).collect()
}

type StandardGraphicsSources = (
    Vec<usize>,
    Vec<usize>,
    Vec<(usize, lm_project::GraphicsRomLayout)>,
    graphics_batch::GraphicsBatchEncoding,
);

fn lunar_magic_standard_graphics_sources(
    profile: &RevisionProfile,
    image: &lm_rom::RomImage,
) -> Result<StandardGraphicsSources, String> {
    let mut slots = standard_graphics_slots(profile.graphics);
    let mut file_numbers = slots.clone();
    if !pristine_special_graphics(profile) {
        return Ok((
            slots,
            file_numbers,
            Vec::new(),
            graphics_batch::GraphicsBatchEncoding::Native,
        ));
    }
    let special = lm_profile::smw_us_v1_special_graphics_layouts(image)
        .map_err(|error| format!("cannot resolve live GFX32/GFX33: {error}"))?;
    let mut file_layouts = slots
        .iter()
        .copied()
        .map(|slot| (slot, profile.graphics))
        .collect::<Vec<_>>();
    slots.extend([0x32, 0x33]);
    file_numbers.extend([0x32, 0x33]);
    file_layouts.extend([(0, special.gfx32), (0, special.gfx33)]);
    Ok((
        slots,
        file_numbers,
        file_layouts,
        graphics_batch::GraphicsBatchEncoding::LunarMagicStandard,
    ))
}

pub(crate) fn standard_graphics_batch_source(
    image: lm_rom::RomImage,
    layout: lm_project::GraphicsRomLayout,
    smw_us_special: bool,
) -> Result<graphics_batch::GraphicsBatchSource, String> {
    let (slots, file_numbers, file_layouts, encoding) = if smw_us_special {
        let special = lm_profile::smw_us_v1_special_graphics_layouts(&image)
            .map_err(|error| format!("cannot resolve live GFX32/GFX33: {error}"))?;
        let mut slots = standard_graphics_slots(layout);
        let mut file_numbers = slots.clone();
        let mut file_layouts = slots
            .iter()
            .copied()
            .map(|slot| (slot, layout))
            .collect::<Vec<_>>();
        slots.extend([0x32, 0x33]);
        file_numbers.extend([0x32, 0x33]);
        file_layouts.extend([(0, special.gfx32), (0, special.gfx33)]);
        (
            slots,
            file_numbers,
            file_layouts,
            graphics_batch::GraphicsBatchEncoding::LunarMagicStandard,
        )
    } else {
        let slots = standard_graphics_slots(layout);
        (
            slots.clone(),
            slots,
            Vec::new(),
            graphics_batch::GraphicsBatchEncoding::Native,
        )
    };
    Ok(graphics_batch::GraphicsBatchSource {
        image,
        layout,
        slots,
        file_numbers,
        family: "standard",
        exgraphics_names: false,
        encoding,
        convert_berry_gfx_tile: true,
        raw_4bpp_overrides: Vec::new(),
        file_layouts,
    })
}

pub(crate) fn exgraphics_batch_source(
    image: lm_rom::RomImage,
    layout: lm_project::GraphicsRomLayout,
) -> Result<graphics_batch::GraphicsBatchSource, String> {
    if lm_profile::probe_smw_us_v1_exgraphics_runtime_for_mapper(&image, layout.mapper).is_ok() {
        return native_smw_us_v1_exgraphics_batch_source(image, layout);
    }
    let slots = installed_exgraphics_slots(&image, layout)?;
    if slots.is_empty() {
        return Err("the installed graphics table contains no ExGFX files".into());
    }
    Ok(graphics_batch::GraphicsBatchSource {
        image,
        layout,
        slots: slots.clone(),
        file_numbers: slots,
        family: "extended",
        exgraphics_names: true,
        encoding: graphics_batch::GraphicsBatchEncoding::Native,
        convert_berry_gfx_tile: true,
        raw_4bpp_overrides: Vec::new(),
        file_layouts: Vec::new(),
    })
}

fn native_smw_us_v1_exgraphics_batch_source(
    image: lm_rom::RomImage,
    layout: lm_project::GraphicsRomLayout,
) -> Result<graphics_batch::GraphicsBatchSource, String> {
    let project = lm_project::Project::new(image.clone());
    let packed_3bpp = !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&image);
    let mut slots = Vec::new();
    let mut files = Vec::new();
    for file_number in (0x60_usize..=0x63).chain(0x80..=0xfff) {
        let route = lm_profile::smw_us_v1_exgraphics_pointer_in_rom(
            &image,
            u16::try_from(file_number).expect("the native ExGFX range fits u16"),
            layout.mapper,
        )
        .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?;
        let pointer = image
            .read(route.pointer_offset, 3)
            .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?;
        if pointer == [0, 0, 0] || pointer == [0xff, 0xff, 0xff] {
            continue;
        }
        let mut bytes = match route.encoding {
            lm_profile::SmwUsV1ExGraphicsEncoding::Raw2048 => project
                .load_tagged_payload(route.pointer_offset, layout.mapper)
                .map(|loaded| loaded.bytes)
                .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?,
            lm_profile::SmwUsV1ExGraphicsEncoding::Lz2 => project
                .load_decompressed_graphics_file(
                    0,
                    lm_project::GraphicsRomLayout {
                        mapper: layout.mapper,
                        pointers: lm_project::LevelPointerTable {
                            offset: route.pointer_offset,
                            entries: 1,
                            stride: 3,
                        },
                        split_pointer_planes: None,
                        compression: layout.compression,
                        maximum_compressed_len: layout.maximum_compressed_len,
                        maximum_decompressed_len: layout.maximum_decompressed_len,
                    },
                )
                .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?,
        };
        if packed_3bpp && (0x80..0xe00).contains(&file_number) {
            bytes = expand_native_exgraphics_3bpp(&bytes)
                .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?;
        }
        slots.push(file_number);
        files.push((file_number, bytes));
    }
    if slots.is_empty() {
        return Err("the installed graphics table contains no ExGFX files".into());
    }
    Ok(graphics_batch::GraphicsBatchSource {
        image,
        layout,
        slots: slots.clone(),
        file_numbers: slots,
        family: "extended",
        exgraphics_names: true,
        encoding: graphics_batch::GraphicsBatchEncoding::Native,
        convert_berry_gfx_tile: true,
        raw_4bpp_overrides: files,
        file_layouts: Vec::new(),
    })
}

fn expand_native_exgraphics_3bpp(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() % 0x18 != 0 {
        return Err(format!(
            "packed 3bpp data has {:#X} bytes instead of complete 0x18-byte tiles",
            bytes.len()
        ));
    }
    let mut editable = Vec::with_capacity(bytes.len() / 3 * 4);
    for tile in bytes.chunks_exact(0x18) {
        editable.extend_from_slice(&tile[..0x10]);
        for plane_2 in &tile[0x10..] {
            editable.extend_from_slice(&[*plane_2, 0]);
        }
    }
    Ok(editable)
}

fn installed_exgraphics_slots(
    image: &lm_rom::RomImage,
    layout: lm_project::GraphicsRomLayout,
) -> Result<Vec<usize>, String> {
    if !(EXGFX_FIRST + 1..=EXGFX_LIMIT).contains(&layout.pointers.entries) {
        return Err(format!(
            "profile graphics table has {} entries; ExGFX requires 129 through 4096",
            layout.pointers.entries
        ));
    }
    let project = lm_project::Project::new(image.clone());
    (0x60..=0x63)
        .chain(EXGFX_FIRST..layout.pointers.entries)
        .filter_map(|slot| match layout.read_pointer(&project, slot) {
            Ok(pointer) if pointer.get() == 0 => None,
            Ok(_) => Some(Ok(slot)),
            Err(error) => Some(Err(format!("ExGFX{slot:02X}: {error}"))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        PendingGraphicsFormatWarningTarget, QuickGraphicsInsertion, RomGraphicsEditor,
        diagnostic_sheet_paste_editable, ensure_external_edit_revision, installed_exgraphics_slots,
        internal_cache_level_graphics_overrides, lunar_magic_standard_graphics_sources,
        overlay_current_graphics_file, paste_target_permitted, pristine_special_graphics,
        quick_graphics_import_source, replace_internal_cache_tile, supports_exgraphics,
        supports_native_exgraphics,
    };
    use crate::{
        graphics_insertion_dialog::{GraphicsInsertionFamily, GraphicsInsertionRequest},
        level_graphics_export::CurrentLevelGraphicsAssignments,
        vanilla_map16_preview::VanillaInternalGraphicsCache,
    };
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable};
    use lm_rom::{Mapper, RomImage};

    #[test]
    fn quick_standard_insertion_uses_lunar_magics_fixed_sibling_targets() {
        let root = tempfile::tempdir().unwrap();
        let mut app = lm_app::AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let (separate, separate_target) =
            quick_graphics_import_source(&app, QuickGraphicsInsertion::Standard, false).unwrap();
        assert_eq!(separate.file_numbers, (0..0x34).collect::<Vec<_>>());
        assert!(separate.smw_us_v1_standard_install);
        assert!(matches!(
            separate_target,
            PendingGraphicsFormatWarningTarget::Directory(path)
                if path == root.path().join("Graphics")
        ));
        let (_, joined_target) =
            quick_graphics_import_source(&app, QuickGraphicsInsertion::Standard, true).unwrap();
        assert!(matches!(
            joined_target,
            PendingGraphicsFormatWarningTarget::Joined(path)
                if path == root.path().join("Graphics").join("AllGFX.bin")
        ));
    }

    #[test]
    fn quick_exgraphics_insertion_supports_pristine_three_bpp_rom_and_exact_undo() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("ExGraphics")).unwrap();
        let editable = (0..0x1000_usize)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        std::fs::write(root.path().join("ExGraphics/ExGFX80.bin"), &editable).unwrap();
        std::fs::write(root.path().join("ExGraphics/ExGFXE00.bin"), &editable).unwrap();
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = lm_app::AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut editor = RomGraphicsEditor::default();
        editor
            .start_quick_import(&app, QuickGraphicsInsertion::ExGraphics, false)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("pristine ExGFX insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(commit.into_command()).unwrap();
        let installed = RomImage::from_bytes(app.controller_snapshot().unwrap().rom_bytes).unwrap();
        assert_eq!(installed.logical_len(), 0x20_0000);
        assert!(!lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &installed
        ));
        assert!(lm_profile::probe_smw_us_v1_exgraphics_runtime(&installed).is_ok());
        for file_number in [0x80, 0xe00] {
            let route = lm_profile::smw_us_v1_exgraphics_pointer_in_rom(
                &installed,
                file_number,
                Mapper::LoRom,
            )
            .unwrap();
            assert_ne!(installed.read(route.pointer_offset, 3).unwrap(), [0xff; 3]);
        }
        let export_directory = root.path().join("ExportedExGraphics");
        std::fs::create_dir(&export_directory).unwrap();
        let source = super::exgraphics_batch_source(
            installed.clone(),
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut export = super::graphics_batch::GraphicsBatchWorker::default();
        export.start(source, export_directory.clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let Some(result) = export.poll() {
                assert_eq!(result.unwrap(), Some(2));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let ordinary = std::fs::read(export_directory.join("ExGFX80.bin")).unwrap();
        assert_eq!(ordinary.len(), 0x1000);
        for (tile, source_tile) in ordinary.chunks_exact(0x20).zip(editable.chunks_exact(0x20)) {
            assert_eq!(&tile[..0x10], &source_tile[..0x10]);
            for row in 0..8 {
                assert_eq!(tile[0x10 + row * 2], source_tile[0x10 + row * 2]);
                assert_eq!(tile[0x11 + row * 2], 0);
            }
        }
        assert_eq!(
            std::fs::read(export_directory.join("ExGFXE00.bin")).unwrap(),
            editable
        );
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, original);
        assert!(
            editor
                .start_ordinary_graphics_import(
                    &app,
                    GraphicsInsertionRequest {
                        family: GraphicsInsertionFamily::ExGraphics,
                        logical_pc_address: 0x10_0000,
                        expand_rom: true,
                        use_4bpp: true,
                    },
                    false,
                )
                .unwrap_err()
                .contains("insert regular GFX as 4bpp")
        );
    }

    #[test]
    fn insert_all_graphics_is_one_reopenable_undoable_standard_and_exgfx_commit() {
        let root = tempfile::tempdir().unwrap();
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let standard_source = super::standard_graphics_batch_source(
            image,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            true,
        )
        .unwrap();
        let graphics = root.path().join("Graphics");
        std::fs::create_dir(&graphics).unwrap();
        let mut extraction = super::graphics_batch::GraphicsBatchWorker::default();
        extraction.start(standard_source, graphics).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let Some(result) = extraction.poll() {
                assert_eq!(result.unwrap(), Some(0x34));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let exgraphics = root.path().join("ExGraphics");
        std::fs::create_dir(&exgraphics).unwrap();
        let exgfx80 = (0..0x1000_usize)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        std::fs::write(exgraphics.join("ExGFX80.bin"), &exgfx80).unwrap();

        let mut app = lm_app::AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut editor = RomGraphicsEditor::default();
        editor.start_insert_all_graphics(&app, false).unwrap();
        assert!(editor.pending_graphics_format_warning.is_none());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("combined graphics insertion prepares one commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(commit.description, "Insert all GFX and ExGFX to ROM");
        app.dispatch(commit.into_command()).unwrap();
        assert_eq!(app.project_revision(), 1);
        let installed = RomImage::from_bytes(app.controller_snapshot().unwrap().rom_bytes).unwrap();
        assert!(lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &installed
        ));
        assert!(lm_profile::probe_smw_us_v1_exgraphics_runtime(&installed).is_ok());
        assert!(
            lm_rom::detect_identity(&installed)
                .unwrap()
                .checksum_matches()
        );
        let route =
            lm_profile::smw_us_v1_exgraphics_pointer_in_rom(&installed, 0x80, Mapper::LoRom)
                .unwrap();
        assert_ne!(installed.read(route.pointer_offset, 3).unwrap(), [0; 3]);
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, original);
        app.dispatch(lm_app::Command::Redo).unwrap();
        assert_eq!(app.project_revision(), 3);
        let reopened = RomImage::from_bytes(app.controller_snapshot().unwrap().rom_bytes).unwrap();
        assert!(lm_profile::probe_smw_us_v1_exgraphics_runtime(&reopened).is_ok());
    }

    #[test]
    #[ignore = "requires Wine, Lunar Magic 3.63, and the legally retained pristine SMW ROM"]
    fn lunar_magic_import_all_graphics_and_atomic_rust_route_reexport_the_same_assets() {
        let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let wine = std::env::var_os("WINE_BIN")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("wine"));
        let lunar_magic = std::env::var_os("LUNAR_MAGIC_EXE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| repository.join("lm363/Lunar Magic.exe"));
        let pristine = std::env::var_os("LM_PRISTINE_GFX_ROM")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| repository.join("sysLMRestore/smwOrig.smc"));
        let root = tempfile::tempdir().unwrap();
        let oracle_path = root.path().join("oracle.smc");
        std::fs::copy(&pristine, &oracle_path).unwrap();
        let export = std::process::Command::new(&wine)
            .arg(&lunar_magic)
            .args(["-ExportGFX", "oracle.smc"])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            export.status.success(),
            "Lunar Magic GFX export failed: {}",
            String::from_utf8_lossy(&export.stderr)
        );
        let exgraphics = root.path().join("ExGraphics");
        std::fs::create_dir(&exgraphics).unwrap();
        let exgfx80 = (0..0x1000_usize)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        std::fs::write(exgraphics.join("ExGFX80.bin"), exgfx80).unwrap();
        let original = std::fs::read(&pristine).unwrap();
        let import = std::process::Command::new(&wine)
            .arg(&lunar_magic)
            .args(["-ImportAllGraphics", "oracle.smc"])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            import.status.success(),
            "Lunar Magic combined import failed: {}",
            String::from_utf8_lossy(&import.stderr)
        );

        let mut app = lm_app::AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.document_path = Some(root.path().join("rust.smc"));
        let mut editor = RomGraphicsEditor::default();
        editor.start_insert_all_graphics(&app, false).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result.unwrap().unwrap();
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(commit.into_command()).unwrap();
        let rust = app.controller_snapshot().unwrap().rom_bytes;
        let oracle = std::fs::read(oracle_path).unwrap();
        assert_eq!(rust.len(), oracle.len());
        let rust_identity =
            lm_rom::detect_identity(&RomImage::from_bytes(rust.clone()).unwrap()).unwrap();
        let oracle_identity =
            lm_rom::detect_identity(&RomImage::from_bytes(oracle.clone()).unwrap()).unwrap();
        assert!(rust_identity.checksum_matches());
        assert!(oracle_identity.checksum_matches());
        assert_eq!(rust_identity.game, oracle_identity.game);
        assert_eq!(rust_identity.mapper, oracle_identity.mapper);
        assert_eq!(rust_identity.region, oracle_identity.region);

        for (label, bytes) in [("rust", &rust), ("oracle", &oracle)] {
            let directory = root.path().join(format!("{label}-export"));
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("result.smc"), bytes).unwrap();
            for operation in ["-ExportGFX", "-ExportExGFX"] {
                let output = std::process::Command::new(&wine)
                    .arg(&lunar_magic)
                    .args([operation, "result.smc"])
                    .current_dir(&directory)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "Lunar Magic {operation} failed for {label}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        for number in 0..0x34 {
            let name = format!("GFX{number:02X}.bin");
            assert_eq!(
                std::fs::read(root.path().join("rust-export/Graphics").join(&name)).unwrap(),
                std::fs::read(root.path().join("oracle-export/Graphics").join(&name)).unwrap(),
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read(root.path().join("rust-export/ExGraphics/ExGFX80.bin")).unwrap(),
            std::fs::read(root.path().join("oracle-export/ExGraphics/ExGFX80.bin")).unwrap()
        );
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, original);
    }

    #[test]
    fn ordinary_insertion_dialog_opens_from_app_state_without_graphics_workspace() {
        let mut app = lm_app::AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomGraphicsEditor::default();
        editor
            .open_ordinary_import(&app, GraphicsInsertionFamily::Standard)
            .unwrap();
        assert!(
            editor
                .ordinary_insertion_dialog
                .as_ref()
                .unwrap()
                .uses_4bpp()
        );
    }

    #[test]
    fn ordinary_insertion_dialog_honors_the_session_4bpp_default() {
        let mut app = lm_app::AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomGraphicsEditor::default();
        assert!(!editor.toggle_install_4bpp_on_insert());
        editor
            .open_ordinary_import(&app, GraphicsInsertionFamily::Standard)
            .unwrap();
        assert!(
            !editor
                .ordinary_insertion_dialog
                .as_ref()
                .unwrap()
                .uses_4bpp()
        );
    }

    #[test]
    fn quick_standard_insertion_commits_reopens_and_undoes_from_fixed_directory() {
        let root = tempfile::tempdir().unwrap();
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = lm_app::AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let source = super::standard_graphics_batch_source(
            image,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            true,
        )
        .unwrap();
        std::fs::create_dir(root.path().join("Graphics")).unwrap();
        let mut extraction = super::graphics_batch::GraphicsBatchWorker::default();
        extraction
            .start(source, root.path().join("Graphics"))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let Some(result) = extraction.poll() {
                assert_eq!(result.unwrap(), Some(0x34));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        let mut three_bpp_app = lm_app::AppState::default();
        three_bpp_app.load_rom(original.clone()).unwrap();
        three_bpp_app.document_path = Some(root.path().join("ordinary-3bpp.smc"));
        let mut three_bpp_editor = RomGraphicsEditor::default();
        three_bpp_editor
            .start_ordinary_graphics_import(
                &three_bpp_app,
                GraphicsInsertionRequest {
                    family: GraphicsInsertionFamily::Standard,
                    logical_pc_address: 0x40000,
                    expand_rom: false,
                    use_4bpp: false,
                },
                false,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let three_bpp_commit = loop {
            if let Some(result) = three_bpp_editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("ordinary 3bpp insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        three_bpp_app
            .dispatch(three_bpp_commit.into_command())
            .unwrap();
        let three_bpp_image =
            RomImage::from_bytes(three_bpp_app.controller_snapshot().unwrap().rom_bytes).unwrap();
        assert_eq!(three_bpp_image.logical_len(), 0x80000);
        assert!(!lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &three_bpp_image
        ));
        let three_bpp_project = lm_project::Project::new(three_bpp_image.clone());
        let three_bpp_layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let first_pointer = three_bpp_layout
            .read_pointer(&three_bpp_project, 0)
            .unwrap()
            .to_pc(lm_rom::Mapper::LoRom)
            .unwrap();
        assert!(first_pointer >= 0x40000);
        let reexport_directory = root.path().join("ThreeBppReexport");
        std::fs::create_dir(&reexport_directory).unwrap();
        let source =
            super::standard_graphics_batch_source(three_bpp_image, three_bpp_layout, true).unwrap();
        let mut reexport = super::graphics_batch::GraphicsBatchWorker::default();
        reexport.start(source, reexport_directory.clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let Some(result) = reexport.poll() {
                assert_eq!(result.unwrap(), Some(0x34));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        for number in 0..0x34 {
            let name = format!("GFX{number:02X}.bin");
            assert_eq!(
                std::fs::read(reexport_directory.join(&name)).unwrap(),
                std::fs::read(root.path().join("Graphics").join(&name)).unwrap(),
                "{name}"
            );
        }
        three_bpp_app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(
            three_bpp_app.controller_snapshot().unwrap().rom_bytes,
            original
        );

        let mut joined = Vec::new();
        for number in 0..0x34 {
            joined.extend(
                std::fs::read(
                    root.path()
                        .join("Graphics")
                        .join(format!("GFX{number:02X}.bin")),
                )
                .unwrap(),
            );
        }
        std::fs::write(root.path().join("Graphics").join("AllGFX.bin"), joined).unwrap();
        let mut joined_three_bpp_app = lm_app::AppState::default();
        joined_three_bpp_app.load_rom(original.clone()).unwrap();
        joined_three_bpp_app.document_path = Some(root.path().join("joined-3bpp.smc"));
        let mut joined_three_bpp_editor = RomGraphicsEditor::default();
        joined_three_bpp_editor
            .start_ordinary_graphics_import(
                &joined_three_bpp_app,
                GraphicsInsertionRequest {
                    family: GraphicsInsertionFamily::Standard,
                    logical_pc_address: 0x40000,
                    expand_rom: true,
                    use_4bpp: false,
                },
                true,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let joined_three_bpp_commit = loop {
            if let Some(result) = joined_three_bpp_editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("joined ordinary 3bpp insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        joined_three_bpp_app
            .dispatch(joined_three_bpp_commit.into_command())
            .unwrap();
        let joined_three_bpp_image = RomImage::from_bytes(
            joined_three_bpp_app
                .controller_snapshot()
                .unwrap()
                .rom_bytes,
        )
        .unwrap();
        assert_eq!(joined_three_bpp_image.logical_len(), 0x100000);
        assert!(!lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &joined_three_bpp_image
        ));
        joined_three_bpp_app
            .dispatch(lm_app::Command::Undo)
            .unwrap();
        assert_eq!(
            joined_three_bpp_app
                .controller_snapshot()
                .unwrap()
                .rom_bytes,
            original
        );

        let mut ordinary_first_app = lm_app::AppState::default();
        ordinary_first_app.load_rom(original.clone()).unwrap();
        ordinary_first_app.document_path = Some(root.path().join("ordinary-first.smc"));
        let mut ordinary_first_editor = RomGraphicsEditor::default();
        ordinary_first_editor
            .start_ordinary_graphics_import(
                &ordinary_first_app,
                GraphicsInsertionRequest {
                    family: GraphicsInsertionFamily::Standard,
                    logical_pc_address: 0x40000,
                    expand_rom: true,
                    use_4bpp: true,
                },
                false,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let ordinary_first_commit = loop {
            if let Some(result) = ordinary_first_editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("ordinary first insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        ordinary_first_app
            .dispatch(ordinary_first_commit.into_command())
            .unwrap();
        let ordinary_first_image =
            RomImage::from_bytes(ordinary_first_app.controller_snapshot().unwrap().rom_bytes)
                .unwrap();
        assert_eq!(ordinary_first_image.logical_len(), 0x100000);
        assert!(lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &ordinary_first_image
        ));
        let ordinary_first_project = lm_project::Project::new(ordinary_first_image);
        let ordinary_layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let first_pointer = ordinary_layout
            .read_pointer(&ordinary_first_project, 0)
            .unwrap()
            .to_pc(lm_rom::Mapper::LoRom)
            .unwrap();
        assert!(first_pointer >= 0x40000);
        let special_layouts =
            lm_profile::smw_us_v1_special_graphics_layouts(&ordinary_first_project.rom).unwrap();
        let gfx33_pointer = special_layouts
            .gfx33
            .read_pointer(&ordinary_first_project, 0)
            .unwrap()
            .to_pc(lm_rom::Mapper::LoRom)
            .unwrap();
        let gfx32_pointer = special_layouts
            .gfx32
            .read_pointer(&ordinary_first_project, 0)
            .unwrap()
            .to_pc(lm_rom::Mapper::LoRom)
            .unwrap();
        assert_eq!(gfx33_pointer / 0x8000, gfx32_pointer / 0x8000);
        assert!(gfx33_pointer >= 0x40000);
        assert!(gfx33_pointer < first_pointer);
        ordinary_first_app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(
            ordinary_first_app.controller_snapshot().unwrap().rom_bytes,
            original
        );

        let mut editor = RomGraphicsEditor::default();
        editor
            .start_quick_import(&app, QuickGraphicsInsertion::Standard, false)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result.unwrap().expect("quick insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(commit.into_command()).unwrap();
        let installed = app.controller_snapshot().unwrap().rom_bytes;
        let installed_image = RomImage::from_bytes(installed).unwrap();
        assert!(lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &installed_image
        ));
        assert_eq!(app.project_revision(), 1);
        std::fs::create_dir(root.path().join("ExGraphics")).unwrap();
        std::fs::write(
            root.path().join("ExGraphics").join("ExGFX80.bin"),
            vec![0; 0x1000],
        )
        .unwrap();
        editor
            .start_quick_import(&app, QuickGraphicsInsertion::ExGraphics, false)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let exgraphics_commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("quick ExGFX insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(exgraphics_commit.into_command()).unwrap();
        let exgraphics_image =
            RomImage::from_bytes(app.controller_snapshot().unwrap().rom_bytes).unwrap();
        assert!(lm_profile::probe_smw_us_v1_exgraphics_runtime(&exgraphics_image).is_ok());
        assert_eq!(
            exgraphics_image.read(0x1bcc0, 16).unwrap(),
            &[
                0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0
            ],
            "reserved ExGFX table must retain the ExAnimation authentication trailer"
        );
        assert_eq!(app.project_revision(), 2);

        let quick_exgraphics_bytes = app.controller_snapshot().unwrap().rom_bytes;
        let exgfx80_path = root.path().join("ExGraphics").join("ExGFX80.bin");
        let mut changed_exgfx80 = std::fs::read(&exgfx80_path).unwrap();
        changed_exgfx80[0] = 0x5a;
        std::fs::write(&exgfx80_path, &changed_exgfx80).unwrap();
        editor
            .start_ordinary_graphics_import(
                &app,
                GraphicsInsertionRequest {
                    family: GraphicsInsertionFamily::ExGraphics,
                    logical_pc_address: 0x190000,
                    expand_rom: false,
                    use_4bpp: false,
                },
                false,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let ordinary_exgraphics_commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("ordinary ExGFX insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(ordinary_exgraphics_commit.into_command())
            .unwrap();
        let ordinary_exgraphics_image =
            RomImage::from_bytes(app.controller_snapshot().unwrap().rom_bytes).unwrap();
        let route = lm_profile::smw_us_v1_exgraphics_pointer_in_rom(
            &ordinary_exgraphics_image,
            0x80,
            lm_rom::Mapper::LoRom,
        )
        .unwrap();
        let pointer = lm_rom::SnesPointer24::decode(
            ordinary_exgraphics_image
                .read(route.pointer_offset, 3)
                .unwrap(),
        )
        .unwrap()
        .to_pc(lm_rom::Mapper::LoRom)
        .unwrap();
        assert!(pointer >= 0x190000);
        assert_eq!(app.project_revision(), 3);
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(
            app.controller_snapshot().unwrap().rom_bytes,
            quick_exgraphics_bytes
        );

        let gfx00_path = root.path().join("Graphics").join("GFX00.bin");
        let mut changed_gfx00 = std::fs::read(&gfx00_path).unwrap();
        changed_gfx00[0] ^= 1;
        std::fs::write(&gfx00_path, &changed_gfx00).unwrap();
        editor
            .start_ordinary_graphics_import(
                &app,
                GraphicsInsertionRequest {
                    family: GraphicsInsertionFamily::Standard,
                    logical_pc_address: 0x90000,
                    expand_rom: false,
                    use_4bpp: false,
                },
                false,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let ordinary_commit = loop {
            if let Some(result) = editor.graphics_import.poll() {
                break result
                    .unwrap()
                    .expect("ordinary insertion prepares a commit");
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        app.dispatch(ordinary_commit.into_command()).unwrap();
        let ordinary_bytes = app.controller_snapshot().unwrap().rom_bytes;
        let ordinary_project =
            lm_project::Project::new(RomImage::from_bytes(ordinary_bytes).unwrap());
        let (reopened_source, _) =
            quick_graphics_import_source(&app, QuickGraphicsInsertion::Standard, false).unwrap();
        let pointer = reopened_source
            .layout
            .read_pointer(&ordinary_project, 0)
            .unwrap();
        assert!(pointer.get() >= 0x90000);
        assert_eq!(
            ordinary_project
                .load_decompressed_graphics_file(0, reopened_source.layout)
                .unwrap(),
            changed_gfx00
        );
        assert_eq!(app.project_revision(), 5);
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.project_revision(), 6);
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.project_revision(), 7);
        app.dispatch(lm_app::Command::Undo).unwrap();
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, original);
    }

    #[test]
    fn non_vanilla_standard_export_retains_profile_table_and_native_encoding() {
        let profile = lm_profile::test_support::profile();
        let image = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let (slots, file_numbers, file_layouts, encoding) =
            lunar_magic_standard_graphics_sources(&profile, &image).unwrap();
        assert_eq!(slots, file_numbers);
        assert_eq!(slots.len(), profile.graphics.pointers.entries.min(0x34));
        assert!(file_layouts.is_empty());
        assert_eq!(
            encoding,
            super::graphics_batch::GraphicsBatchEncoding::Native
        );
    }

    #[test]
    fn special_pair_actions_require_the_exact_recovered_split_layout() {
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        assert!(pristine_special_graphics(&profile));
        profile.graphics.split_pointer_planes = None;
        assert!(!pristine_special_graphics(&profile));
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile
            .graphics
            .split_pointer_planes
            .as_mut()
            .unwrap()
            .bank_offset += 1;
        assert!(!pristine_special_graphics(&profile));
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile.region = lm_rom::Region::Japan;
        assert!(!pristine_special_graphics(&profile));
    }

    #[test]
    fn native_first_exgfx_insert_supports_pristine_three_bpp_and_installed_four_bpp_roms() {
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let mut bytes = vec![0xff; 0x8000];
        let pristine = RomImage::from_bytes(bytes.clone()).unwrap();
        assert!(supports_native_exgraphics(&profile, &pristine));
        for offset in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS {
            bytes[offset] = lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER;
        }
        let four_bpp = RomImage::from_bytes(bytes).unwrap();
        assert!(supports_native_exgraphics(&profile, &four_bpp));
        profile.region = lm_rom::Region::Japan;
        assert!(!supports_native_exgraphics(&profile, &four_bpp));
    }

    #[test]
    fn installed_exgraphics_enumeration_uses_nonzero_pointer_entries_only() {
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x100,
                entries: 0x83,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        };
        let mut bytes = vec![0; 0x8000];
        for slot in [0x60, 0x63, 0x80, 0x82] {
            let offset = layout.pointers.offset + slot * 3;
            bytes[offset..offset + 3].copy_from_slice(&[0x00, 0x81, 0x80]);
        }
        let image = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            installed_exgraphics_slots(&image, layout).unwrap(),
            [0x60, 0x63, 0x80, 0x82]
        );

        let mut profile = lm_profile::test_support::profile();
        profile.graphics = layout;
        assert!(supports_exgraphics(&profile));
        profile.graphics.pointers.entries = 0x80;
        assert!(!supports_exgraphics(&profile));
    }

    #[test]
    fn external_reload_is_bound_to_the_revision_that_was_staged() {
        ensure_external_edit_revision(12, 12).unwrap();
        let error = ensure_external_edit_revision(12, 13).unwrap_err();
        assert!(error.contains("external graphics reload"), "{error}");
    }

    #[test]
    fn diagnostic_internal_cache_tile_edits_are_bounded_and_transient() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let changed = IndexedTile::new([9; IndexedTile::PIXEL_COUNT]);
        let mut cache = VanillaInternalGraphicsCache {
            tiles: vec![blank.clone(); 0x4000],
        };
        replace_internal_cache_tile(&mut cache, 0x1800, changed.clone()).unwrap();
        assert_eq!(cache.tiles[0x1800], changed);
        assert_eq!(cache.tiles[0x17ff], blank);
        assert!(replace_internal_cache_tile(&mut cache, 0x4000, blank).is_err());
    }

    #[test]
    fn diagnostic_sheet_paste_accepts_current_level_cache_only() {
        let vanilla = super::DiagnosticSheetPasteContext {
            extended_foreground_background: false,
            vanilla_animation_enabled: true,
            special_world_passed: false,
        };
        assert!(diagnostic_sheet_paste_editable(0x000, vanilla));
        assert!(!diagnostic_sheet_paste_editable(0x041, vanilla));
        assert!(!diagnostic_sheet_paste_editable(0x081, vanilla));
        assert!(diagnostic_sheet_paste_editable(0x082, vanilla));
        for tile in [0x090, 0x091, 0x0da, 0x0dd, 0x0ea, 0x0ed] {
            assert!(!diagnostic_sheet_paste_editable(tile, vanilla));
        }
        assert!(!diagnostic_sheet_paste_editable(0x300, vanilla));
        assert!(diagnostic_sheet_paste_editable(0x5ff, vanilla));
        assert!(!diagnostic_sheet_paste_editable(0x600, vanilla));
        assert!(!diagnostic_sheet_paste_editable(0x3fff, vanilla));

        let bypass_without_vanilla_animation = super::DiagnosticSheetPasteContext {
            extended_foreground_background: true,
            vanilla_animation_enabled: false,
            special_world_passed: false,
        };
        assert!(diagnostic_sheet_paste_editable(
            0x041,
            bypass_without_vanilla_animation
        ));
        assert!(diagnostic_sheet_paste_editable(
            0x300,
            bypass_without_vanilla_animation
        ));

        let special_world = super::DiagnosticSheetPasteContext {
            special_world_passed: true,
            ..bypass_without_vanilla_animation
        };
        assert!(!diagnostic_sheet_paste_editable(0x480, special_world));
        assert!(!diagnostic_sheet_paste_editable(0x4ff, special_world));
        assert!(diagnostic_sheet_paste_editable(0x500, special_world));
    }

    #[test]
    fn every_installed_clipboard_route_obeys_diagnostic_paste_guards() {
        let context = super::DiagnosticSheetPasteContext {
            extended_foreground_background: false,
            vanilla_animation_enabled: true,
            special_world_passed: false,
        };
        for index in 0..0x4000 {
            assert_eq!(
                paste_target_permitted(true, index, Some(context), true),
                diagnostic_sheet_paste_editable(index, context),
                "diagnostic clipboard mismatch at tile {index:03X}"
            );
        }
        assert!(paste_target_permitted(true, 0x5ff, Some(context), false));
        assert!(!paste_target_permitted(true, 0x600, Some(context), true));
        assert!(!paste_target_permitted(true, 0x002, None, true));
        assert!(paste_target_permitted(false, 0x600, None, true));
        assert!(!paste_target_permitted(false, 0x002, Some(context), false));
    }

    #[test]
    fn retained_lunar_magic_diagnostic_paste_oracle_binds_every_observed_boundary() {
        let fixture = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/graphics-cache-paste/oracle.tsv"
        );
        let fields = fixture
            .lines()
            .skip(1)
            .map(|line| line.split_once('\t').expect("oracle row has two columns"))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields["maximum_page"], "3F");
        assert_eq!(fields["super_gfx_bypass"], "0");
        assert_eq!(fields["vanilla_animation_enabled"], "1");
        assert_eq!(fields["special_world_passed"], "0");
        assert_eq!(fields["ordinary_target_changed"], "1");
        assert_eq!(fields["ordinary_target_matches_source"], "1");
        assert_eq!(fields["fixed_animation_target_changed"], "0");
        assert_eq!(fields["unused_fg_target_changed"], "0");
        assert_eq!(fields["last_editable_target_changed"], "1");
        assert_eq!(fields["last_editable_target_matches_source"], "1");
        assert_eq!(fields["beyond_limit_target_changed"], "0");

        let observed = super::DiagnosticSheetPasteContext {
            extended_foreground_background: fields["super_gfx_bypass"] == "1",
            vanilla_animation_enabled: fields["vanilla_animation_enabled"] == "1",
            special_world_passed: fields["special_world_passed"] == "1",
        };
        assert!(diagnostic_sheet_paste_editable(0x002, observed));
        assert!(!diagnostic_sheet_paste_editable(0x041, observed));
        assert!(!diagnostic_sheet_paste_editable(0x300, observed));
        assert!(diagnostic_sheet_paste_editable(0x5ff, observed));
        assert!(!diagnostic_sheet_paste_editable(0x600, observed));
    }

    #[test]
    fn retained_lunar_magic_pixel_buffer_oracle_proves_staging_before_paste() {
        let fixture = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/graphics-pixel-buffer/oracle.tsv"
        );
        let fields = fixture
            .lines()
            .skip(1)
            .map(|line| line.split_once('\t').expect("oracle row has two columns"))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields["tile"], "600");
        assert_eq!(fields["maximum_page"], "3F");
        assert_eq!(fields["flip_changed_edit_buffer"], "1");
        assert_eq!(fields["flip_changed_planar_backing"], "0");
        assert_eq!(fields["second_flip_restored_edit_buffer"], "1");
        assert_eq!(fields["foreground_paint_changed_edit_buffer"], "1");
        assert_eq!(fields["foreground_paint_changed_decoded_backing"], "0");
        assert_eq!(fields["foreground_paint_changed_planar_backing"], "0");
        assert_eq!(fields["painted_edit_pixel_zero"], "1");
        assert_eq!(fields["backing_pixel_zero"], "0");
        assert_eq!(fields["background_paint_restored_edit_buffer"], "1");
        assert_eq!(fields["background_paint_restored_decoded"], "1");
        assert_eq!(fields["background_paint_restored_planar"], "1");
    }

    #[test]
    fn f9_cache_publication_uses_exact_slots_skips_7f_and_last_duplicate_wins() {
        let mut cache = VanillaInternalGraphicsCache {
            tiles: (0..0x4000)
                .map(|index| IndexedTile::new([u8::try_from((index / 0x80) & 0x0f).unwrap(); 64]))
                .collect(),
        };
        cache.tiles[0x400] = IndexedTile::new([0x0e; 64]);
        let assignments = CurrentLevelGraphicsAssignments {
            foreground_background: vec![0x14, 0x17, 0x14],
            sprites: vec![0x20, 0x7f],
            super_graphics_bypass_enabled: true,
        };
        let overrides = internal_cache_level_graphics_overrides(&cache, &assignments).unwrap();
        assert_eq!(
            overrides.iter().map(|(file, _)| *file).collect::<Vec<_>>(),
            [0x14, 0x17, 0x20]
        );

        let expected_duplicate_winner = GraphicsFile4bpp {
            tiles: cache.tiles[0x100..0x180].to_vec(),
        }
        .encode()
        .unwrap();
        assert_eq!(overrides[0].1, expected_duplicate_winner);

        let expected_sprite = GraphicsFile4bpp {
            tiles: cache.tiles[0x400..0x480].to_vec(),
        }
        .encode()
        .unwrap();
        assert_eq!(overrides[2].1, expected_sprite);
    }

    #[test]
    fn diagnostic_unlock_overlays_staged_active_file_into_every_assigned_slot() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let changed = IndexedTile::new([0x0b; IndexedTile::PIXEL_COUNT]);
        let mut cache = VanillaInternalGraphicsCache {
            tiles: vec![blank.clone(); 0x4000],
        };
        let assignments = CurrentLevelGraphicsAssignments {
            foreground_background: vec![0x14, 0x17, 0x14, 0x7f],
            sprites: vec![0x20, 0x14, 0x13, 0x22],
            super_graphics_bypass_enabled: true,
        };
        let staged = vec![changed.clone(); 0x80];
        overlay_current_graphics_file(&mut cache, &assignments, 0x14, &staged).unwrap();
        for start in [0x000, 0x100, 0x480] {
            assert!(
                cache.tiles[start..start + 0x80]
                    .iter()
                    .all(|tile| tile == &changed)
            );
        }
        assert_eq!(cache.tiles[0x080], blank);
        assert!(
            overlay_current_graphics_file(&mut cache, &assignments, 0x14, &staged[..0x7f]).is_err()
        );
        overlay_current_graphics_file(&mut cache, &assignments, 0x32, &[]).unwrap();
    }
}
