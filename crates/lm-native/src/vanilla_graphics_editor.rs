use crate::{
    graphics_batch,
    graphics_painter::{
        GraphicsCharacterShortcut, GraphicsColorMapEditor, GraphicsDisplayPalette,
        GraphicsEditorStatus, GraphicsTileGrid, GraphicsTileTransform, TILE_EDITOR_SIDE,
        TILE_GRID_COLUMNS, TilePixelPointerAction, TilePointerAction,
        apply_tile_keyboard_navigation, apply_tile_navigation, apply_tile_palette_keyboard,
        apply_tile_palette_step, color_selection_marker, graphics_navigation_controls,
        graphics_transform_controls, paint_tile, shortcut_transform,
        take_graphics_character_shortcut, take_graphics_refresh_shortcut,
        take_graphics_save_shortcut, take_tile_grid_shortcut, take_tile_shift, tile_button,
        tile_coordinate, tile_page_range, tile_pixel_pointer_action, tile_pointer_action,
    },
    level_graphics_export::{
        pristine_current_level_graphics_files, take_level_graphics_export_shortcut,
    },
    native_clipboard,
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
    foreground_color: u8,
    background_color: u8,
    display_palette: GraphicsDisplayPalette,
    tile_grid: GraphicsTileGrid,
    color_map: GraphicsColorMapEditor,
    pending_shift: Option<TileShift>,
    pending_character_shortcut: Option<GraphicsCharacterShortcut>,
    clipboard_paste_target: Option<usize>,
    status: GraphicsEditorStatus,
    error: Option<String>,
    pending_level_graphics_export: bool,
    graphics_batch: graphics_batch::GraphicsBatchWorker,
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
            self.load(&snapshot, key);
        }
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
            .on_hover_text("F8 — saves the active level's FG/BG/SP files as decoded 4bpp")
            .clicked()
        {
            self.pending_level_graphics_export = true;
        }
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
                if response.clicked_by(egui::PointerButton::Primary) {
                    self.foreground_color = color;
                    selected_foreground = Some(color);
                }
                if response.clicked_by(egui::PointerButton::Secondary) {
                    self.background_color = color;
                    selected_background = Some(color);
                }
                if response.hovered() {
                    hovered_color = Some(color);
                }
            }
        });
        self.status.update_palette_hover(hovered_color);
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
        let commit_shortcut = take_graphics_save_shortcut(ui);
        if expanded && modified && !file_work_running && (commit_clicked || commit_shortcut) {
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
        if !self.pending_level_graphics_export {
            return;
        }
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
            self.pending_level_graphics_export = false;
            self.begin_level_graphics_batch(app, snapshot, special_world_passed);
        } else if cancelled || context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.pending_level_graphics_export = false;
        }
    }

    fn begin_level_graphics_batch(
        &mut self,
        app: &AppState,
        snapshot: &lm_app::ControllerSnapshot,
        special_world_passed: bool,
    ) {
        let Some(level) = app.current_level() else {
            self.error = Some("no active level is available for GFX extraction".into());
            return;
        };
        let source = match pristine_level_graphics_batch_source(
            snapshot,
            self.controller.as_ref(),
            level,
            special_world_passed,
        ) {
            Ok(source) => source,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_level_graphics_directory() else {
            return;
        };
        if let Err(error) = self.graphics_batch.start(source, directory) {
            self.error = Some(error);
        }
    }

    fn load(&mut self, snapshot: &lm_app::ControllerSnapshot, key: EditorKey) {
        match GraphicsController::decode_editable(
            snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        ) {
            Ok(controller) => {
                self.controller = Some(controller);
                self.selected_tile = 0;
                self.foreground_color = 1;
                self.background_color = 0;
                self.display_palette = GraphicsDisplayPalette::default();
                self.status = GraphicsEditorStatus::default();
                self.clipboard_paste_target = None;
                self.error = None;
            }
            Err(error) => {
                self.controller = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn tile_list(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        edits_enabled: bool,
        level_export_enabled: bool,
    ) {
        let Some(controller) = &self.controller else {
            return;
        };
        let tiles = &controller.graphics().tiles;
        let tile_count = tiles.len();
        self.selected_tile = self.selected_tile.min(tiles.len().saturating_sub(1));
        let page = tile_page_range(self.selected_tile, tile_count);
        let (page_start, page_end) = (page.start, page.end);
        let mut responses = Vec::with_capacity(page_end.saturating_sub(page_start));
        let mut selected_by_pointer = None;
        let selected_tile = controller.graphics().tiles.get(self.selected_tile).cloned();
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
                            match tile_pointer_action(ui, &response, index) {
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
                                    if edits_enabled && let Some(tile) = selected_tile.clone() {
                                        selected_paste = Some((index, tile));
                                    }
                                }
                                Some(TilePointerAction::PasteClipboard(index)) if edits_enabled => {
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
            apply_tile_keyboard_navigation(ui, &mut self.selected_tile, &responses, tile_count)
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
        self.status.update_tile_hover(
            &responses,
            page_start,
            ui.input(|input| input.modifiers),
            None,
        );
        if let Some(status) = navigation_status.or(palette_status) {
            self.status.set(status);
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
            self.pending_level_graphics_export = true;
        }
    }

    fn pixel_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        edits_enabled: bool,
    ) {
        let tile = self
            .controller
            .as_ref()
            .and_then(|controller| controller.graphics().tiles.get(self.selected_tile))
            .cloned();
        let Some(mut tile) = tile else {
            ui.label("No tiles in this graphics file.");
            return;
        };
        if let Some(direction) = self.pending_shift.take() {
            tile = tile.shifted_wrapping(direction);
            self.apply_tile(tile.clone());
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
            self.apply_tile(mapped);
            if let Some(current) = self
                .controller
                .as_ref()
                .and_then(|controller| controller.graphics().tiles.get(self.selected_tile))
            {
                tile = current.clone();
            }
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
            self.apply_tile(tile.clone());
        }
        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::splat(TILE_EDITOR_SIDE),
            egui::Sense::click_and_drag(),
        );
        self.status
            .update_pixel_editor_hover(response.hovered(), self.selected_tile);
        paint_tile(ui.painter(), rect, &tile, palette, self.display_palette);
        if let Some(action) =
            tile_pixel_pointer_action(&response, ui.input(|input| input.modifiers))
            && let Some(position) = response.interact_pointer_pos()
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
                    self.apply_tile(tile);
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

    fn apply_tile_at(&mut self, index: usize, tile: IndexedTile) -> bool {
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

    fn paste_tile(&mut self, text: &str) {
        let target = self
            .clipboard_paste_target
            .take()
            .unwrap_or(self.selected_tile);
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

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.error = None;
        self.pending_level_graphics_export = false;
        self.status = GraphicsEditorStatus::default();
        self.clipboard_paste_target = None;
    }
}

fn pristine_level_graphics_batch_source(
    snapshot: &lm_app::ControllerSnapshot,
    controller: Option<&GraphicsController>,
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
    let raw_4bpp_overrides = if slots.contains(&usize::from(active_slot)) {
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
        encoding: graphics_batch::GraphicsBatchEncoding::Decoded4Bpp,
        raw_4bpp_overrides,
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
            pristine_level_graphics_batch_source(&snapshot, Some(&controller), 0x105, false)
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
}
