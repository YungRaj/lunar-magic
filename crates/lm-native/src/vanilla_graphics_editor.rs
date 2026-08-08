use crate::{
    graphics_batch,
    graphics_painter::{
        GraphicsCharacterShortcut, GraphicsColorMapEditor, GraphicsDisplayPalette,
        GraphicsEditorStatus, GraphicsTileGrid, GraphicsTileTransform, PalettePointerAction,
        TILE_EDITOR_SIDE, TILE_GRID_COLUMNS, TilePixelPointerAction, TilePixelPointerCapture,
        TilePointerAction, apply_tile_keyboard_navigation, apply_tile_navigation,
        apply_tile_palette_keyboard, apply_tile_palette_step, color_selection_marker,
        graphics_navigation_controls, graphics_transform_controls, paint_tile,
        palette_pointer_action, shortcut_transform, take_graphics_character_shortcut,
        take_graphics_refresh_shortcut, take_internal_graphics_cache_unlock,
        take_tile_grid_shortcut, take_tile_shift, tile_button, tile_coordinate, tile_page_range,
        tile_pixel_pointer_action, tile_pointer_action,
    },
    level_graphics_export::{
        LUNAR_MAGIC_ALL_GFX_FILE_SIZES, LevelGraphicsExportMode, extracted_graphics_paths,
        extracted_joined_graphics_paths, pristine_current_level_graphics_assignments,
        pristine_current_level_graphics_files, take_level_graphics_export_shortcut,
    },
    native_clipboard,
    rom_graphics_editor::{
        DiagnosticSheetPasteContext, diagnostic_sheet_paste_editable,
        internal_cache_level_graphics_overrides, overlay_current_graphics_file,
    },
};
use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, GraphicsController, GraphicsControllerEdit, RomExpansionCommand,
};
use lm_graphics::{
    Bgr555, GraphicsTileChange, IndexedTile, Palette, PaletteInterchangeFile, TileShift,
};
use lm_project::GraphicsSaveOptions;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    slot: u16,
}

#[derive(Default)]
pub(crate) struct VanillaGraphicsEditor {
    key: Option<EditorKey>,
    controller: Option<GraphicsController>,
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
    error: Option<String>,
    pending_level_graphics_export: Option<LevelGraphicsExportMode>,
    graphics_batch: graphics_batch::GraphicsBatchWorker,
    internal_cache: Option<crate::vanilla_map16_preview::VanillaInternalGraphicsCache>,
    internal_cache_unlocked: bool,
    internal_cache_level: Option<u16>,
    internal_cache_special_world: bool,
}

impl VanillaGraphicsEditor {
    pub(crate) fn handles(app: &AppState) -> bool {
        app.revision_profile().is_none()
            && app.controller_snapshot().is_ok_and(|snapshot| {
                matches!(snapshot.mode, EditorMode::Graphics(_)) && is_supported(&snapshot)
            })
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        app: &AppState,
        special_world_passed: bool,
        joined_graphics_files: &mut bool,
    ) -> Option<Command> {
        take_graphics_refresh_shortcut(ui);
        if let Some(result) = self.graphics_batch.show(ui.ctx()) {
            match result {
                Ok(Some(_)) => self.status.set("Saved FG/BG/SP GFX to files."),
                Ok(None) => self.status.set("GFX extraction cancelled."),
                Err(error) => {
                    self.status.set("Couldn't save FG/BG/SP GFX to file!");
                    self.error = Some(error);
                }
            }
        }
        let snapshot = app.controller_snapshot().ok()?;
        let EditorMode::Graphics(slot) = snapshot.mode else {
            self.clear();
            return None;
        };
        if !is_supported(&snapshot) || app.revision_profile().is_some() {
            self.clear();
            return None;
        }
        let key = EditorKey {
            revision: snapshot.revision,
            slot,
        };
        if self.key != Some(key) {
            self.load(&snapshot, key, app.current_level(), special_world_passed);
        }
        self.refresh_internal_cache(&snapshot, app.current_level(), special_world_passed);
        let file_work_running = self.graphics_batch.is_running();
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        if !file_work_running && let Some(text) = pasted {
            self.paste_tile(&text);
        }
        ui.heading(format!("GFX{slot:02X} — built-in SMW graphics editor"));
        ui.label("Vanilla split pointer planes detected automatically.");
        if ui
            .add_enabled(
                !file_work_running && app.current_level().is_some(),
                egui::Button::new("Extract current level GFX…"),
            )
            .on_hover_text("Choose a new directory for the active level's decoded FG/BG/SP files")
            .clicked()
        {
            self.pending_level_graphics_export = Some(LevelGraphicsExportMode::ChooseNewDirectory);
        }
        ui.checkbox(joined_graphics_files, "Use joined AllGFX.bin files")
            .on_hover_text("Original global joined-GFX mode (command $24BD)");
        ui.separator();
        let palette = grayscale_palette();
        let Some(controller) = self.controller.as_ref() else {
            ui.colored_label(
                egui::Color32::RED,
                self.error.as_deref().unwrap_or("graphics load failed"),
            );
            return None;
        };
        let tile_count = controller.graphics().tiles.len();
        self.selected_tile = self.selected_tile.min(tile_count.saturating_sub(1));
        let mut hovered_color = None;
        let mut selected_foreground = None;
        let mut selected_background = None;
        ui.horizontal(|ui| {
            ui.label("Paint color");
            for color in 0_u8..16 {
                let fill =
                    crate::graphics_painter::palette_color(&palette, self.display_palette, color);
                let response = ui.add(
                    egui::Button::new(color_selection_marker(
                        color,
                        self.foreground_color,
                        self.background_color,
                    ))
                    .min_size(egui::Vec2::splat(22.0))
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
        ui.columns(2, |columns| {
            self.tile_list(
                &mut columns[0],
                &palette,
                !file_work_running,
                !file_work_running && app.current_level().is_some(),
                *joined_graphics_files,
                &snapshot,
            );
            self.pixel_editor(&mut columns[1], &palette, !file_work_running);
        });
        self.status.show(ui);
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        let expanded = snapshot.rom_bytes.len() > 0x80_000;
        if !expanded {
            ui.label("Graphics relocation needs one expanded free-space bank.");
            if ui.button("Expand ROM to 1 MiB").clicked() {
                return Some(Command::ExpandRom(RomExpansionCommand {
                    expected_revision: snapshot.revision,
                    mapper: snapshot.identity.mapper,
                    target_logical_len: 0x10_0000,
                    fill: 0xff,
                    checksum_field: snapshot.identity.internal_header_offset + 0x1c,
                }));
            }
        }
        let modified = self
            .controller
            .as_ref()
            .is_some_and(GraphicsController::is_modified);
        self.level_graphics_export_confirmation(ui.ctx(), app, &snapshot, special_world_passed);
        let file_work_running = self.graphics_batch.is_running();
        let commit_clicked = ui
            .add_enabled(
                expanded && modified && !file_work_running,
                egui::Button::new("Commit graphics changes to ROM"),
            )
            .clicked();
        if expanded && modified && !file_work_running && commit_clicked {
            match prepare_commit(
                self.controller
                    .as_ref()
                    .ok_or("graphics controller is closed")
                    .map_err(str::to_owned),
                &snapshot,
            ) {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        None
    }

    fn level_graphics_export_confirmation(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        snapshot: &lm_app::ControllerSnapshot,
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
            self.begin_level_graphics_batch(app, snapshot, special_world_passed, mode);
        } else if cancelled || context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.pending_level_graphics_export = None;
        }
    }

    fn begin_level_graphics_batch(
        &mut self,
        app: &AppState,
        snapshot: &lm_app::ControllerSnapshot,
        special_world_passed: bool,
        mode: LevelGraphicsExportMode,
    ) {
        let Some(level) = app.current_level() else {
            self.error = Some("no active level is available for GFX extraction".into());
            return;
        };
        let source = match pristine_level_graphics_batch_source(
            snapshot,
            self.controller.as_ref(),
            self.internal_cache_unlocked
                .then_some(self.internal_cache.as_ref())
                .flatten(),
            level,
            special_world_passed,
        ) {
            Ok(source) => source,
            Err(error) => {
                self.error = Some(error);
                return;
            }
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
        if let Err(error) = start {
            self.error = Some(error);
        }
    }

    fn load(
        &mut self,
        snapshot: &lm_app::ControllerSnapshot,
        key: EditorKey,
        level: Option<u16>,
        special_world_passed: bool,
    ) {
        match GraphicsController::decode_editable(
            snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        ) {
            Ok(controller) => {
                self.edit_tile = controller.graphics().tiles.first().cloned();
                self.controller = Some(controller);
                self.selected_tile = 0;
                self.foreground_color = 1;
                self.background_color = 0;
                self.display_palette = GraphicsDisplayPalette::default();
                self.status = GraphicsEditorStatus::default();
                self.clipboard_paste_target = None;
                self.pixel_pointer_capture = TilePixelPointerCapture::None;
                self.error = None;
                self.internal_cache = level.and_then(|level| {
                    let project = lm_project::Project::new(
                        RomImage::from_bytes(snapshot.rom_bytes.clone()).ok()?,
                    );
                    let loaded = project
                        .load_level_slot(
                            usize::from(level),
                            lm_profile::smw_us_v1_vanilla_level_layout(),
                            &lm_level::SpriteLengthTable::standard(),
                        )
                        .ok()?;
                    crate::vanilla_map16_preview::load_pristine_internal_graphics_cache(
                        snapshot.rom_bytes.clone(),
                        level,
                        loaded.layer1.header,
                        special_world_passed,
                    )
                    .ok()
                });
                self.internal_cache_unlocked = false;
                self.internal_cache_level = level;
                self.internal_cache_special_world = special_world_passed;
            }
            Err(error) => {
                self.controller = None;
                self.edit_tile = None;
                self.internal_cache = None;
                self.internal_cache_unlocked = false;
                self.internal_cache_level = None;
                self.internal_cache_special_world = false;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn refresh_internal_cache(
        &mut self,
        snapshot: &lm_app::ControllerSnapshot,
        level: Option<u16>,
        special_world_passed: bool,
    ) {
        if self.internal_cache_level == level
            && self.internal_cache_special_world == special_world_passed
        {
            return;
        }
        self.internal_cache_level = level;
        self.internal_cache_special_world = special_world_passed;
        self.internal_cache = level.and_then(|level| {
            let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).ok()?;
            let project = lm_project::Project::new(image);
            let loaded = project
                .load_level_slot(
                    usize::from(level),
                    lm_profile::smw_us_v1_vanilla_level_layout(),
                    &lm_level::SpriteLengthTable::standard(),
                )
                .ok()?;
            crate::vanilla_map16_preview::load_pristine_internal_graphics_cache(
                snapshot.rom_bytes.clone(),
                level,
                loaded.layer1.header,
                special_world_passed,
            )
            .ok()
        });
        if self.internal_cache.is_none() {
            self.internal_cache_unlocked = false;
        } else if self.internal_cache_unlocked
            && let Err(error) = self.synchronize_active_graphics_into_internal_cache(snapshot)
        {
            self.internal_cache_unlocked = false;
            self.error = Some(error);
        } else if self.internal_cache_unlocked {
            self.reload_edit_tile_from_selection();
        }
    }

    fn synchronize_active_graphics_into_internal_cache(
        &mut self,
        snapshot: &lm_app::ControllerSnapshot,
    ) -> Result<(), String> {
        let level = self
            .internal_cache_level
            .ok_or_else(|| "no active level is available".to_owned())?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|e| e.to_string())?;
        let assignments = pristine_current_level_graphics_assignments(
            &image,
            level,
            self.internal_cache_special_world,
        )?;
        let file = usize::from(
            self.key
                .ok_or_else(|| "graphics workspace is closed".to_owned())?
                .slot,
        );
        let tiles = self
            .controller
            .as_ref()
            .ok_or_else(|| "graphics controller is closed".to_owned())?
            .graphics()
            .tiles
            .clone();
        let cache = self
            .internal_cache
            .as_mut()
            .ok_or_else(|| "internal graphics cache is unavailable".to_owned())?;
        overlay_current_graphics_file(cache, &assignments, file, &tiles)
    }

    fn tile_list(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        edits_enabled: bool,
        level_export_enabled: bool,
        joined_graphics_files: bool,
        snapshot: &lm_app::ControllerSnapshot,
    ) {
        let Some(controller) = &self.controller else {
            return;
        };
        let diagnostic = self.internal_cache_unlocked;
        let diagnostic_paste_context = DiagnosticSheetPasteContext {
            extended_foreground_background: false,
            vanilla_animation_enabled: true,
            special_world_passed: self.internal_cache_special_world,
        };
        let tiles = if diagnostic {
            let Some(cache) = &self.internal_cache else {
                self.internal_cache_unlocked = false;
                self.error = Some("internal graphics cache is unavailable".into());
                return;
            };
            ui.small("Internal GFX data — transient working cache; F9 publishes current-level FG/BG/SP slots");
            &cache.tiles
        } else {
            &controller.graphics().tiles
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
        let mut copied = false;
        let mut paste_status = None;
        let row_count = palette.palette.colors.len() / 16;
        let (page_control, palette_control) =
            graphics_navigation_controls(ui, tile_count > 0, row_count > 0);
        egui::ScrollArea::vertical()
            .max_height(430.0)
            .show(ui, |ui| {
                egui::Grid::new("vanilla-graphics-tiles")
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
                                    match native_clipboard::encode_graphics_tile(tile) {
                                        Ok(text) => {
                                            ui.ctx().copy_text(text);
                                            copied = true;
                                        }
                                        Err(error) => self.error = Some(error),
                                    }
                                }
                                Some(TilePointerAction::PasteSelected(index)) => {
                                    if edits_enabled
                                        && (!diagnostic
                                            || diagnostic_sheet_paste_editable(
                                                index,
                                                diagnostic_paste_context,
                                            ))
                                        && let Some(tile) = selected_tile.clone()
                                    {
                                        selected_paste = Some((index, tile));
                                    }
                                }
                                Some(TilePointerAction::PasteClipboard(index))
                                    if edits_enabled
                                        && (!diagnostic
                                            || diagnostic_sheet_paste_editable(
                                                index,
                                                diagnostic_paste_context,
                                            )) =>
                                {
                                    self.clipboard_paste_target = Some(index);
                                    ui.ctx()
                                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                                }
                                Some(TilePointerAction::PasteClipboard(_)) | None => {}
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
        let navigation_status = if let Some(navigation) = page_control {
            apply_tile_navigation(&mut self.selected_tile, &responses, tile_count, navigation)
        } else {
            apply_tile_keyboard_navigation(
                ui,
                &mut self.selected_tile,
                &responses,
                tile_count,
                edits_enabled,
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
        let unlock_requested =
            take_internal_graphics_cache_unlock(ui, self.selected_tile, &responses);
        self.status.update_tile_hover(
            &responses,
            page_start,
            ui.input(|input| input.modifiers),
            None,
            ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO),
        );
        if let Some(status) = navigation_status.or(palette_status) {
            self.status.set(status);
        }
        if unlock_requested {
            if self.internal_cache.is_some() {
                match self.synchronize_active_graphics_into_internal_cache(snapshot) {
                    Ok(()) => {
                        self.internal_cache_unlocked = true;
                        reload_selected_edit_tile = true;
                        self.status.set("Internal GFX data viewing unlocked.");
                    }
                    Err(error) => self.error = Some(error),
                }
            } else {
                self.error = Some(
                    "internal graphics data cannot be materialized without an active supported level"
                        .into(),
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
        self.pending_shift = take_tile_shift(ui, self.selected_tile, &responses, edits_enabled);
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

    fn pixel_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        edits_enabled: bool,
    ) {
        let tile = self.edit_tile.clone().or_else(|| {
            self.internal_cache_unlocked
                .then(|| self.internal_cache.as_ref())
                .flatten()
                .and_then(|cache| cache.tiles.get(self.selected_tile))
                .or_else(|| {
                    (!self.internal_cache_unlocked)
                        .then(|| self.controller.as_ref())
                        .flatten()
                        .and_then(|controller| controller.graphics().tiles.get(self.selected_tile))
                })
                .cloned()
        });
        let Some(mut tile) = tile else {
            ui.label("No tiles in this graphics file.");
            return;
        };
        if let Some(direction) = self.pending_shift.take() {
            tile = tile.shifted_wrapping(direction);
            self.stage_tile(tile.clone());
        }
        let character_shortcut = self.pending_character_shortcut.take();
        if character_shortcut == Some(GraphicsCharacterShortcut::EditColorMap) {
            self.color_map.open_dialog();
        }
        ui.label(format!("Tile {:03X}", self.selected_tile));
        let clicked_mapping =
            self.color_map
                .show(ui, palette, self.display_palette, &tile, edits_enabled);
        let mapped = character_shortcut
            .filter(|shortcut| {
                edits_enabled && *shortcut == GraphicsCharacterShortcut::ApplyColorMap
            })
            .and_then(|_| self.color_map.apply(&tile))
            .or(clicked_mapping);
        if let Some(mapped) = mapped {
            tile = mapped;
            self.stage_tile(tile.clone());
        }
        let clicked_transform = graphics_transform_controls(ui, edits_enabled);
        let transform = shortcut_transform(character_shortcut)
            .filter(|_| edits_enabled)
            .or(clicked_transform);
        if let Some(transform) = transform {
            tile = match transform {
                GraphicsTileTransform::RotateClockwise => tile.rotated_clockwise(),
                GraphicsTileTransform::FlipHorizontal => tile.flipped(true, false),
                GraphicsTileTransform::FlipVertical => tile.flipped(false, true),
            };
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
                TilePixelPointerAction::PaintForeground
                | TilePixelPointerAction::PaintBackground
                    if edits_enabled =>
                {
                    let color = match action {
                        TilePixelPointerAction::PaintForeground => self.foreground_color,
                        TilePixelPointerAction::PaintBackground => self.background_color,
                        _ => unreachable!(),
                    };
                    if let Err(error) = tile.set_pixel(x, y, color) {
                        self.error = Some(error.to_string());
                        return;
                    }
                    self.stage_tile(tile);
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

    fn apply_tile(&mut self, tile: IndexedTile) {
        self.apply_tile_at(self.selected_tile, tile);
    }

    fn stage_tile(&mut self, tile: IndexedTile) {
        self.edit_tile = Some(tile);
    }

    fn reload_edit_tile_from_selection(&mut self) {
        self.edit_tile = self.selected_tile_clone();
    }

    fn apply_tile_at(&mut self, index: usize, tile: IndexedTile) -> bool {
        if self.internal_cache_unlocked {
            let Some(cache_tile) = self
                .internal_cache
                .as_mut()
                .and_then(|cache| cache.tiles.get_mut(index))
            else {
                self.error = Some(format!("internal graphics tile {index:X} is unavailable"));
                return false;
            };
            *cache_tile = tile;
            return true;
        }
        let edit = GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange { index, tile }]);
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("graphics controller is closed".into());
            return false;
        };
        match controller.apply_edits(&[edit]) {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn selected_tile_clone(&self) -> Option<IndexedTile> {
        if self.internal_cache_unlocked {
            self.internal_cache
                .as_ref()?
                .tiles
                .get(self.selected_tile)
                .cloned()
        } else {
            self.controller
                .as_ref()?
                .graphics()
                .tiles
                .get(self.selected_tile)
                .cloned()
        }
    }

    fn paste_tile(&mut self, text: &str) {
        let target = self
            .clipboard_paste_target
            .take()
            .unwrap_or(self.selected_tile);
        if self.internal_cache_unlocked
            && !diagnostic_sheet_paste_editable(
                target,
                DiagnosticSheetPasteContext {
                    extended_foreground_background: false,
                    vanilla_animation_enabled: true,
                    special_world_passed: self.internal_cache_special_world,
                },
            )
        {
            return;
        }
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

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.edit_tile = None;
        self.internal_cache = None;
        self.internal_cache_unlocked = false;
        self.internal_cache_level = None;
        self.internal_cache_special_world = false;
        self.error = None;
        self.pending_level_graphics_export = None;
        self.status = GraphicsEditorStatus::default();
        self.clipboard_paste_target = None;
        self.pixel_pointer_capture = TilePixelPointerCapture::None;
    }
}

fn pristine_level_graphics_batch_source(
    snapshot: &lm_app::ControllerSnapshot,
    controller: Option<&GraphicsController>,
    internal_cache: Option<&crate::vanilla_map16_preview::VanillaInternalGraphicsCache>,
    level: u16,
    special_world_passed: bool,
) -> Result<graphics_batch::GraphicsBatchSource, String> {
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let slots = pristine_current_level_graphics_files(&image, level, special_world_passed)?;
    let controller = controller.ok_or_else(|| "graphics controller is closed".to_owned())?;
    let raw = controller.export_raw().map_err(|error| error.to_string())?;
    let EditorMode::Graphics(active_slot) = snapshot.mode else {
        return Err("graphics workspace is no longer active".into());
    };
    let raw_4bpp_overrides = if let Some(cache) = internal_cache {
        let assignments =
            pristine_current_level_graphics_assignments(&image, level, special_world_passed)?;
        internal_cache_level_graphics_overrides(cache, &assignments)?
    } else if slots.contains(&usize::from(active_slot)) {
        vec![(usize::from(active_slot), raw)]
    } else {
        Vec::new()
    };
    Ok(graphics_batch::GraphicsBatchSource {
        image,
        layout: lm_profile::smw_us_v1_vanilla_graphics_layout(),
        slots: slots.clone(),
        file_numbers: slots,
        family: "level",
        exgraphics_names: false,
        encoding: graphics_batch::GraphicsBatchEncoding::Decoded4Bpp,
        raw_4bpp_overrides,
        file_layouts: Vec::new(),
    })
}

fn grayscale_palette() -> PaletteInterchangeFile {
    PaletteInterchangeFile {
        source_palette: 0,
        palette: Palette {
            colors: (0_u16..16)
                .map(|component| Bgr555(component | (component << 5) | (component << 10)))
                .collect(),
        },
    }
}

fn is_supported(snapshot: &lm_app::ControllerSnapshot) -> bool {
    snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
        && matches!(
            snapshot.mode,
            EditorMode::Graphics(slot) if usize::from(slot) < lm_profile::SMW_US_V1_VANILLA_GRAPHICS_FILES
        )
}

fn prepare_commit(
    controller: Result<&GraphicsController, String>,
    snapshot: &lm_app::ControllerSnapshot,
) -> Result<Command, String> {
    let controller = controller?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let logical_len = image.logical_len();
    if logical_len <= 0x80_000 {
        return Err("expand the ROM before committing graphics changes".into());
    }
    let layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
    let planes = layout
        .split_pointer_planes
        .ok_or_else(|| "built-in graphics layout lost its pointer planes".to_owned())?;
    let plane_range =
        |offset| ProtectedRange(offset..offset + (planes.entries - 1) * planes.stride + 1);
    controller
        .prepare_commit(
            "Edit pristine SMW graphics",
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x80_000..logical_len,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        plane_range(planes.low_offset),
                        plane_range(planes.high_offset),
                        plane_range(planes.bank_offset),
                        ProtectedRange(
                            snapshot.identity.internal_header_offset
                                ..snapshot.identity.internal_header_offset + 0x40,
                        ),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .map(lm_app::PreparedRomCommit::into_command)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pristine_editor_loads_the_complete_diagnostic_cache_but_keeps_it_locked() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0x14)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let key = EditorKey {
            revision: snapshot.revision,
            slot: 0x14,
        };
        let mut editor = VanillaGraphicsEditor::default();
        editor.load(&snapshot, key, Some(0x105), false);

        assert_eq!(editor.key, Some(key));
        assert!(!editor.internal_cache_unlocked);
        assert_eq!(
            editor.internal_cache.as_ref().unwrap().tiles.len(),
            crate::vanilla_map16_preview::INTERNAL_GRAPHICS_CACHE_TILES
        );
        assert_eq!(editor.selected_tile, 0);

        editor.internal_cache_unlocked = true;
        editor.clear();
        assert!(editor.internal_cache.is_none());
        assert!(!editor.internal_cache_unlocked);
    }

    #[test]
    fn pristine_level_export_uses_exact_assignments_and_active_staged_slot() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0x14)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let changed = IndexedTile::new([0x0d; IndexedTile::PIXEL_COUNT]);
        controller
            .apply_edits(&[GraphicsControllerEdit::ApplyChanges(vec![
                GraphicsTileChange {
                    index: 0,
                    tile: changed,
                },
            ])])
            .unwrap();

        let source =
            pristine_level_graphics_batch_source(&snapshot, Some(&controller), None, 0x105, false)
                .unwrap();
        assert_eq!(
            source.slots,
            [0x14, 0x17, 0x1b, 0x15, 0x00, 0x01, 0x13, 0x20]
        );
        assert_eq!(source.file_numbers, source.slots);
        assert_eq!(source.raw_4bpp_overrides.len(), 1);
        assert_eq!(source.raw_4bpp_overrides[0].0, 0x14);
        assert_eq!(
            source.raw_4bpp_overrides[0].1,
            controller.export_raw().unwrap()
        );
    }

    #[test]
    fn pristine_editor_flips_enter_the_graphics_controller_staging_path() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut editor = VanillaGraphicsEditor {
            controller: Some(controller),
            selected_tile: 0,
            ..VanillaGraphicsEditor::default()
        };
        let original = IndexedTile::new(std::array::from_fn(|index| index.to_le_bytes()[0] & 0x0f));
        editor.apply_tile(original.clone());
        assert_eq!(editor.error, None);
        editor.apply_tile(original.flipped(true, false));
        assert_eq!(editor.error, None);
        let controller = editor.controller.as_ref().unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            controller.graphics().tiles[0],
            original.flipped(true, false)
        );
    }

    #[test]
    fn pristine_editor_typed_paste_enters_the_graphics_controller_staging_path() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut editor = VanillaGraphicsEditor {
            controller: Some(controller),
            selected_tile: 1,
            ..VanillaGraphicsEditor::default()
        };
        let tile = IndexedTile::new(std::array::from_fn(|index| {
            index.to_le_bytes()[0].wrapping_mul(3) & 0x0f
        }));
        let encoded = native_clipboard::encode_graphics_tile(&tile).unwrap();
        editor.paste_tile(&encoded);
        assert_eq!(editor.error, None);
        let controller = editor.controller.as_ref().unwrap();
        assert!(controller.is_modified());
        assert_eq!(controller.graphics().tiles[1], tile);
    }

    #[test]
    fn pristine_pixel_edit_buffer_stages_until_sheet_paste() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut editor = VanillaGraphicsEditor {
            edit_tile: controller.graphics().tiles.first().cloned(),
            controller: Some(controller),
            selected_tile: 0,
            ..VanillaGraphicsEditor::default()
        };
        let source_before = editor.selected_tile_clone().unwrap();
        let staged = source_before.flipped(true, false);

        editor.stage_tile(staged.clone());
        assert_eq!(editor.edit_tile, Some(staged.clone()));
        assert_eq!(editor.selected_tile_clone(), Some(source_before.clone()));
        assert!(!editor.controller.as_ref().unwrap().is_modified());

        assert!(editor.apply_tile_at(2, staged.clone()));
        assert_eq!(editor.selected_tile_clone(), Some(source_before));
        assert_eq!(
            editor.controller.as_ref().unwrap().graphics().tiles[2],
            staged
        );
    }

    #[test]
    fn pristine_selected_tile_paste_writes_the_target_without_changing_selection() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut editor = VanillaGraphicsEditor {
            controller: Some(controller),
            selected_tile: 1,
            ..VanillaGraphicsEditor::default()
        };
        let source = editor.controller.as_ref().unwrap().graphics().tiles[1].clone();
        assert!(editor.apply_tile_at(2, source.clone()));
        assert_eq!(editor.selected_tile, 1);
        assert_eq!(
            editor.controller.as_ref().unwrap().graphics().tiles[2],
            source
        );
    }

    #[test]
    fn pristine_diagnostic_cache_edits_are_transient_and_bounded_like_original() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let changed = IndexedTile::new([0x0c; IndexedTile::PIXEL_COUNT]);
        let mut editor = VanillaGraphicsEditor {
            internal_cache: Some(crate::vanilla_map16_preview::VanillaInternalGraphicsCache {
                tiles: vec![blank.clone(); 0x4000],
            }),
            internal_cache_unlocked: true,
            selected_tile: 0x1800,
            ..VanillaGraphicsEditor::default()
        };
        assert!(editor.apply_tile_at(0x1800, changed.clone()));
        assert_eq!(editor.selected_tile_clone(), Some(changed));
        assert!(editor.controller.is_none());
        assert!(!editor.apply_tile_at(0x4000, blank));
    }

    #[test]
    fn pristine_diagnostic_clipboard_paste_stops_after_current_level_cache() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let changed = IndexedTile::new([0x0d; IndexedTile::PIXEL_COUNT]);
        let mut editor = VanillaGraphicsEditor {
            internal_cache: Some(crate::vanilla_map16_preview::VanillaInternalGraphicsCache {
                tiles: vec![blank.clone(); 0x4000],
            }),
            internal_cache_unlocked: true,
            selected_tile: 0x600,
            ..VanillaGraphicsEditor::default()
        };
        let encoded = native_clipboard::encode_graphics_tile(&changed).unwrap();
        editor.paste_tile(&encoded);
        assert_eq!(editor.internal_cache.as_ref().unwrap().tiles[0x600], blank);

        editor.clipboard_paste_target = Some(0x5ff);
        editor.paste_tile(&encoded);
        assert_eq!(
            editor.internal_cache.as_ref().unwrap().tiles[0x5ff],
            changed
        );
    }

    #[test]
    fn pristine_diagnostic_high_tile_can_stage_but_only_paste_to_eligible_backing() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let changed = IndexedTile::new([0x0d; IndexedTile::PIXEL_COUNT]);
        let mut editor = VanillaGraphicsEditor {
            internal_cache: Some(crate::vanilla_map16_preview::VanillaInternalGraphicsCache {
                tiles: vec![blank.clone(); 0x4000],
            }),
            internal_cache_unlocked: true,
            selected_tile: 0x600,
            edit_tile: Some(blank.clone()),
            ..VanillaGraphicsEditor::default()
        };

        editor.stage_tile(changed.clone());
        assert_eq!(editor.edit_tile, Some(changed.clone()));
        assert_eq!(editor.selected_tile_clone(), Some(blank.clone()));
        assert!(!diagnostic_sheet_paste_editable(
            0x600,
            DiagnosticSheetPasteContext {
                extended_foreground_background: false,
                vanilla_animation_enabled: true,
                special_world_passed: false,
            }
        ));
        assert!(editor.apply_tile_at(0x5ff, changed.clone()));
        assert_eq!(editor.internal_cache.as_ref().unwrap().tiles[0x600], blank);
        assert_eq!(
            editor.internal_cache.as_ref().unwrap().tiles[0x5ff],
            changed
        );
    }

    #[test]
    fn pristine_diagnostic_f9_publishes_exact_cache_slots() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0x14)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        let assignments =
            pristine_current_level_graphics_assignments(&image, 0x105, false).unwrap();
        let loaded = lm_project::Project::new(image)
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let mut cache = crate::vanilla_map16_preview::load_pristine_internal_graphics_cache(
            snapshot.rom_bytes.clone(),
            0x105,
            loaded.layer1.header,
            false,
        )
        .unwrap();
        cache.tiles[0] = IndexedTile::new([0x0e; IndexedTile::PIXEL_COUNT]);
        let source = pristine_level_graphics_batch_source(
            &snapshot,
            Some(&controller),
            Some(&cache),
            0x105,
            false,
        )
        .unwrap();
        assert_eq!(source.raw_4bpp_overrides.len(), 8);
        let expected = lm_graphics::GraphicsFile4bpp {
            tiles: cache.tiles[..0x80].to_vec(),
        }
        .encode()
        .unwrap();
        let gfx14 = source
            .raw_4bpp_overrides
            .iter()
            .find(|(file, _)| *file == assignments.foreground_background[0])
            .unwrap();
        assert_eq!(gfx14.1, expected);
    }

    #[test]
    fn pristine_cache_refresh_follows_special_world_without_losing_staged_file_edits() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0x14)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let key = EditorKey {
            revision: snapshot.revision,
            slot: 0x14,
        };
        let mut editor = VanillaGraphicsEditor::default();
        editor.load(&snapshot, key, Some(0x105), false);
        let changed = IndexedTile::new([0x0a; IndexedTile::PIXEL_COUNT]);
        assert!(editor.apply_tile_at(0, changed.clone()));
        let ordinary_sp2 = editor.internal_cache.as_ref().unwrap().tiles[0x480..0x500].to_vec();

        editor.refresh_internal_cache(&snapshot, Some(0x105), true);
        assert_eq!(
            editor.controller.as_ref().unwrap().graphics().tiles[0],
            changed
        );
        assert_ne!(
            editor.internal_cache.as_ref().unwrap().tiles[0x480..0x500],
            ordinary_sp2
        );
        assert!(editor.internal_cache_special_world);
    }
}
