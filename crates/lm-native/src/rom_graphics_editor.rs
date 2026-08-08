use crate::{
    document_loader::DocumentLoader,
    graphics_batch,
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
        LUNAR_MAGIC_ALL_GFX_FILE_SIZES, LevelGraphicsExportMode, current_level_graphics_files,
        extracted_graphics_paths, extracted_joined_graphics_paths,
        take_level_graphics_export_shortcut,
    },
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, GraphicsController, GraphicsControllerEdit, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_graphics::{GraphicsTileChange, IndexedTile, PaletteInterchangeFile, TileShift};

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

#[derive(Default)]
pub(crate) struct RomGraphicsEditor {
    workspace: Option<Workspace>,
    selected_tile: usize,
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
    external_editor: external_edit::ExternalGraphicsEditor,
    external_tool_id: Option<String>,
    internal_cache_unlocked: bool,
}

impl RomGraphicsEditor {
    fn refresh_internal_cache(&mut self, level: Option<u16>, special_world_passed: bool) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if workspace.level == level
            && workspace.internal_cache_special_world == special_world_passed
        {
            return;
        }
        workspace.level = level;
        workspace.internal_cache_special_world = special_world_passed;
        let result = level
            .ok_or_else(|| "no active level is available".to_owned())
            .and_then(|level| {
                crate::vanilla_map16_preview::load_profiled_internal_graphics_cache(
                    workspace.image.clone(),
                    &workspace.profile,
                    level,
                    special_world_passed,
                    Some(&workspace.external_sprite_assets),
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
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        special_world_passed: bool,
        joined_graphics_files: &mut bool,
    ) -> (bool, Option<Command>) {
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
            ui.small("Internal GFX data — diagnostic working cache (read-only until owned-bank save routing is recovered)");
            &cache.tiles
        } else {
            &workspace.controller.graphics().tiles
        };
        let tile_count = tiles.len();
        self.selected_tile = self.selected_tile.min(tiles.len().saturating_sub(1));
        let page = tile_page_range(self.selected_tile, tile_count);
        let (page_start, page_end) = (page.start, page.end);
        let mut responses = Vec::with_capacity(page_end.saturating_sub(page_start));
        let mut selected_by_pointer = None;
        let selected_tile = tiles.get(self.selected_tile).cloned();
        let mut selected_paste = None;
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
                                }
                                Some(TilePointerAction::Copy(index)) => {
                                    self.selected_tile = index;
                                    match native_clipboard::encode_graphics_tile(tile) {
                                        Ok(text) => {
                                            ui.ctx().copy_text(text);
                                            copied = true;
                                        }
                                        Err(error) => self.error = Some(error),
                                    }
                                }
                                Some(TilePointerAction::PasteSelected(index)) => {
                                    let owner = (!diagnostic)
                                        .then(|| workspace.controller.ownership().owner(index))
                                        .flatten();
                                    if !diagnostic
                                        && edits_enabled
                                        && ownership::is_editable(owner)
                                        && let Some(tile) = selected_tile.clone()
                                    {
                                        selected_paste = Some((index, tile));
                                    }
                                }
                                Some(TilePointerAction::PasteClipboard(index)) => {
                                    let owner = (!diagnostic)
                                        .then(|| workspace.controller.ownership().owner(index))
                                        .flatten();
                                    if !diagnostic && edits_enabled && ownership::is_editable(owner)
                                    {
                                        self.clipboard_paste_target = Some(index);
                                        ui.ctx()
                                            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
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
        let selected_owner = (!diagnostic)
            .then(|| {
                self.workspace.as_ref().and_then(|workspace| {
                    workspace.controller.ownership().owner(self.selected_tile)
                })
            })
            .flatten();
        let tile_shift_enabled =
            !diagnostic && edits_enabled && ownership::is_editable(selected_owner);
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
                self.internal_cache_unlocked = true;
                self.status.set("Internal GFX data viewing unlocked.");
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
            !diagnostic && edits_enabled && ownership::is_editable(owner),
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
        let raw_4bpp_overrides = slots
            .contains(&usize::from(workspace.slot))
            .then(|| vec![(usize::from(workspace.slot), raw)])
            .unwrap_or_default();
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "level",
            exgraphics_names: false,
            encoding: graphics_batch::GraphicsBatchEncoding::Decoded4Bpp,
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
        if let Some(text) = pasted.filter(|_| !diagnostic) {
            let target = self
                .clipboard_paste_target
                .take()
                .unwrap_or(self.selected_tile);
            let target_editable = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.controller.ownership().owner(target))
                .is_some_and(|owner| ownership::is_editable(Some(owner)));
            if !stale && target_editable {
                match native_clipboard::decode_graphics_tile(text) {
                    Ok(tile) => {
                        if self.apply_tile_at(target, tile) {
                            self.selected_tile = target;
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
            ui.label("Diagnostic cache tile; saving this bank is not yet enabled.");
            false
        } else {
            ownership::show(ui, owner)
        };
        let Some(mut tile) = selected.cloned() else {
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
            self.apply_tile(mapped);
            if let Some(current) = self.workspace.as_ref().and_then(|workspace| {
                workspace
                    .controller
                    .graphics()
                    .tiles
                    .get(self.selected_tile)
            }) {
                tile = current.clone();
            }
        }
        if let Some(direction) = self.pending_shift.take() {
            let shifted = tile.shifted_wrapping(direction);
            self.apply_tile(shifted);
            if let Some(current) = self
                .workspace
                .as_ref()
                .and_then(|w| w.controller.graphics().tiles.get(self.selected_tile))
            {
                tile = current.clone();
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Copy tile").clicked() {
                match native_clipboard::encode_graphics_tile(&tile) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(!stale && editable, egui::Button::new("Paste tile"))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
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
            self.apply_tile(transformed);
            if let Some(current) = self
                .workspace
                .as_ref()
                .and_then(|w| w.controller.graphics().tiles.get(self.selected_tile))
            {
                tile = current.clone();
            }
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
        self.apply_tile(tile);
    }
    fn apply_tile(&mut self, tile: IndexedTile) {
        self.apply_tile_at(self.selected_tile, tile);
    }
    fn apply_tile_at(&mut self, index: usize, tile: IndexedTile) -> bool {
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
        if let Err(error) = self.persistence.start(
            self.next_persistence_request,
            crate::persistence_worker::PersistenceTarget::Create(path),
            bytes,
        ) {
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
            raw_4bpp_overrides: Vec::new(),
            file_layouts,
        };
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
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
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
        };
        match self.graphics_import.start_joined(source, path) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
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
        let slots = match installed_exgraphics_slots(&workspace.image, workspace.profile.graphics) {
            Ok(slots) if !slots.is_empty() => slots,
            Ok(_) => {
                self.error = Some("the installed graphics table contains no ExGFX files".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_exgraphics_directory() else {
            return;
        };
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "extended",
            exgraphics_names: true,
            encoding: graphics_batch::GraphicsBatchEncoding::Native,
            raw_4bpp_overrides: Vec::new(),
            file_layouts: Vec::new(),
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
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }
}

fn modified_controller(workspace: Option<&Workspace>) -> bool {
    workspace.is_some_and(|workspace| workspace.controller.is_modified())
}

fn ensure_external_edit_revision(expected: u64, current: u64) -> Result<(), String> {
    crate::rom_load::ensure_current_revision(expected, current, "external graphics reload")
}

fn pristine_special_graphics(profile: &RevisionProfile) -> bool {
    profile.game == lm_rom::SupportedGame::SuperMarioWorld
        && profile.region == lm_rom::Region::NorthAmerica
        && profile.revision == 0
        && profile.mapper == lm_rom::Mapper::LoRom
        && profile.graphics == lm_profile::smw_us_v1_vanilla_graphics_layout()
}

fn supports_exgraphics(profile: &RevisionProfile) -> bool {
    (EXGFX_FIRST + 1..=EXGFX_LIMIT).contains(&profile.graphics.pointers.entries)
}

fn supports_native_exgraphics(profile: &RevisionProfile, image: &lm_rom::RomImage) -> bool {
    pristine_special_graphics(profile)
        && (lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(image)
            || lm_profile::probe_smw_us_v1_exgraphics_runtime(image).is_ok())
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
        ensure_external_edit_revision, installed_exgraphics_slots,
        lunar_magic_standard_graphics_sources, pristine_special_graphics, supports_exgraphics,
        supports_native_exgraphics,
    };
    use crate::level_graphics_export::legacy_level_graphics_files;
    use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable};
    use lm_rom::{Mapper, RomImage};

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
    fn native_first_exgfx_insert_requires_authenticated_four_bpp_prerequisite() {
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let mut bytes = vec![0xff; 0x8000];
        let pristine = RomImage::from_bytes(bytes.clone()).unwrap();
        assert!(!supports_native_exgraphics(&profile, &pristine));
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
    fn legacy_level_export_uses_the_exact_object_then_sprite_assignment_order() {
        let mut profile = lm_profile::test_support::profile();
        profile.game = lm_rom::SupportedGame::SuperMarioWorld;
        profile.region = lm_rom::Region::NorthAmerica;
        profile.revision = 0;
        let mut bytes = vec![0; 0x8000];
        bytes[lm_profile::SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET
            ..lm_profile::SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET + 4]
            .copy_from_slice(&[0x14, 0x17, 0x19, 0x15]);
        bytes[lm_profile::SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET
            ..lm_profile::SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET + 4]
            .copy_from_slice(&[0x00, 0x01, 0x13, 0x22]);
        let image = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            legacy_level_graphics_files(
                &image,
                &profile,
                lm_level::LegacyLevelHeader::default(),
                false,
            )
            .unwrap(),
            [0x14, 0x17, 0x19, 0x15, 0x00, 0x01, 0x13, 0x22]
        );
    }
}
