use crate::{
    dialogs,
    document_loader::DocumentLoader,
    graphics_painter::{
        GraphicsCharacterShortcut, GraphicsColorMapEditor, GraphicsDisplayPalette,
        GraphicsEditorStatus, GraphicsTileGrid, GraphicsTileTransform, PalettePointerAction,
        TILE_EDITOR_SIDE, TILE_GRID_COLUMNS, TilePixelPointerAction, TilePixelPointerCapture,
        TilePointerAction, apply_tile_keyboard_navigation, apply_tile_navigation,
        apply_tile_palette_keyboard, apply_tile_palette_step, color_selection_marker,
        graphics_navigation_controls, graphics_transform_controls, paint_tile, palette_color,
        palette_pointer_action, shortcut_transform, take_graphics_character_shortcut,
        take_graphics_refresh_shortcut, take_graphics_save_shortcut, take_tile_grid_shortcut,
        take_tile_shift, tile_button, tile_coordinate, tile_page_range, tile_pixel_pointer_action,
        tile_pointer_action,
    },
    native_clipboard,
};
use eframe::egui;
use lm_app::GraphicsDocumentController;
use lm_graphics::{PaletteInterchangeFile, TileShift};

mod document_io;
mod editing;

use document_io::decode_documents;
use editing::{apply_pixel, apply_tile, flip_tile, paste_tile, shift_tile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct GraphicsDocument {
    controller: GraphicsDocumentController,
    palette: PaletteInterchangeFile,
}

#[derive(Default)]
pub(crate) struct GraphicsEditor {
    document: Option<GraphicsDocument>,
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
    error: Option<String>,
    pending_close: Option<PendingClose>,
    save_worker: crate::persistence_worker::PersistenceWorker,
    loader: DocumentLoader,
}

impl GraphicsEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(graphics_path) = dialogs::choose_graphics_document() else {
            return;
        };
        let Some(palette_path) = dialogs::choose_palette_document() else {
            return;
        };
        if let Err(error) = self
            .loader
            .start(document_io::requests(graphics_path, palette_path))
        {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for graphics loading to finish before closing".into());
            return false;
        }
        if self.save_worker.is_running() {
            self.error = Some("wait for graphics persistence to finish before closing".into());
            return false;
        }
        let Some(document) = &self.document else {
            return true;
        };
        if !document.controller.is_modified() {
            self.document = None;
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
            match result.and_then(decode_documents) {
                Ok(document) => {
                    self.document = Some(document);
                    self.selected_tile = 0;
                    self.foreground_color = 1;
                    self.background_color = 0;
                    self.display_palette = GraphicsDisplayPalette::default();
                    self.status = GraphicsEditorStatus::default();
                    self.clipboard_paste_target = None;
                    self.pixel_pointer_capture = TilePixelPointerCapture::None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.poll_save(context);
        let mut quit_approved = false;
        if self.document.is_some() {
            egui::Window::new("Portable Graphics Editor")
                .default_size([760.0, 620.0])
                .show(context, |ui| self.contents(ui));
        }
        if let Some(pending) = self.pending_close {
            egui::Window::new("Unsaved graphics")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.label("Discard unsaved graphics changes?");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.pending_close = None;
                        }
                        if ui.button("Discard").clicked() {
                            self.document = None;
                            self.pending_close = None;
                            quit_approved = pending == PendingClose::Application;
                        }
                    });
                });
        }
        self.show_error(context);
        quit_approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        take_graphics_refresh_shortcut(ui);
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let save_requested = take_graphics_save_shortcut(ui);
        self.toolbar(ui, save_requested);
        if let Some(text) = pasted {
            self.paste_tile(&text);
        }
        let Some(document) = self.document.as_ref() else {
            return;
        };
        ui.separator();
        let rows = document.palette.palette.colors.len() / 16;
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
        let palette = document.palette.clone();
        self.color_picker(ui, &palette);
        ui.separator();
        ui.columns(2, |columns| {
            self.tile_list(&mut columns[0], &palette);
            self.pixel_editor(&mut columns[1], &palette);
        });
        self.status.show(ui);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, save_requested: bool) {
        let save_available = !self.save_worker.is_running();
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let controller = &mut document.controller;
        let revision = controller.revision();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(controller.can_undo(), egui::Button::new("Undo"))
                .clicked()
                && let Err(error) = controller.undo(revision)
            {
                self.error = Some(error.to_string());
            }
            if ui
                .add_enabled(controller.can_redo(), egui::Button::new("Redo"))
                .clicked()
                && let Err(error) = controller.redo(revision)
            {
                self.error = Some(error.to_string());
            }
            let save_clicked = ui
                .add_enabled(save_available, egui::Button::new("Save"))
                .clicked();
            if save_available && (save_clicked || save_requested) {
                document_io::begin_save(controller, &mut self.save_worker, &mut self.error);
            }
            if ui.button("Copy tile").clicked()
                && let Some(tile) = controller.value().graphics.tiles.get(self.selected_tile)
            {
                match native_clipboard::encode_graphics_tile(tile) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Paste tile").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            ui.label(if controller.is_modified() {
                "Modified"
            } else {
                "Saved"
            });
        });
    }

    fn color_picker(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let mut hovered_color = None;
        let mut selected_foreground = None;
        let mut selected_background = None;
        ui.horizontal_wrapped(|ui| {
            for color in 0_u8..16 {
                let fill = palette_color(palette, self.display_palette, color);
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
    }

    fn tile_list(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let Some(document) = &self.document else {
            return;
        };
        let tiles = &document.controller.value().graphics.tiles;
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("portable-graphics-tiles")
                .spacing([0.0, 0.0])
                .show(ui, |ui| {
                    for (offset, tile) in tiles[page_start..page_end].iter().enumerate() {
                        let index = page_start + offset;
                        let selected = index == self.selected_tile;
                        let response = tile_button(
                            ui,
                            tile,
                            palette,
                            self.display_palette,
                            selected,
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
                                if let Some(tile) = selected_tile.clone() {
                                    selected_paste = Some((index, tile));
                                }
                            }
                            Some(TilePointerAction::PasteClipboard(index)) => {
                                self.clipboard_paste_target = Some(index);
                                ui.ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
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
            && let Some(document) = self.document.as_mut()
            && apply_tile(&mut document.controller, index, tile, &mut self.error)
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
                true,
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
        if let Some(index) = selected_by_pointer {
            self.status.select_tile(index);
        }
        if let Some(status) = paste_status {
            self.status.set(status);
        }
        if copied {
            self.status.set("Copied tile to clipboard.");
        }
        self.pending_shift = take_tile_shift(ui, self.selected_tile, &responses, true);
        self.pending_character_shortcut =
            take_graphics_character_shortcut(ui, self.selected_tile, &responses);
        if let Some(status) =
            take_tile_grid_shortcut(ui, self.selected_tile, &responses, &mut self.tile_grid)
        {
            self.status.set(status);
        }
    }

    fn pixel_editor(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let Some(mut tile) = document
            .controller
            .value()
            .graphics
            .tiles
            .get(self.selected_tile)
            .cloned()
        else {
            ui.label("No graphics tiles");
            return;
        };
        if let Some(direction) = self.pending_shift.take() {
            shift_tile(
                &mut document.controller,
                self.selected_tile,
                direction,
                &mut self.error,
            );
            if let Some(current) = document
                .controller
                .value()
                .graphics
                .tiles
                .get(self.selected_tile)
            {
                tile = current.clone();
            }
        }
        let character_shortcut = self.pending_character_shortcut.take();
        if character_shortcut == Some(GraphicsCharacterShortcut::EditColorMap) {
            self.color_map.open_dialog();
        }
        ui.label(format!("Tile {:03X}", self.selected_tile));
        let clicked_mapping = self
            .color_map
            .show(ui, palette, self.display_palette, &tile, true);
        let mapped = character_shortcut
            .filter(|shortcut| *shortcut == GraphicsCharacterShortcut::ApplyColorMap)
            .and_then(|_| self.color_map.apply(&tile))
            .or(clicked_mapping);
        if let Some(mapped) = mapped {
            apply_tile(
                &mut document.controller,
                self.selected_tile,
                mapped,
                &mut self.error,
            );
            if let Some(current) = document
                .controller
                .value()
                .graphics
                .tiles
                .get(self.selected_tile)
            {
                tile = current.clone();
            }
        }
        let clicked_transform = graphics_transform_controls(ui, true);
        let transform = shortcut_transform(character_shortcut).or(clicked_transform);
        if let Some(transform) = transform {
            match transform {
                GraphicsTileTransform::RotateClockwise => {
                    apply_tile(
                        &mut document.controller,
                        self.selected_tile,
                        tile.rotated_clockwise(),
                        &mut self.error,
                    );
                }
                GraphicsTileTransform::FlipHorizontal => flip_tile(
                    &mut document.controller,
                    self.selected_tile,
                    true,
                    false,
                    &mut self.error,
                ),
                GraphicsTileTransform::FlipVertical => flip_tile(
                    &mut document.controller,
                    self.selected_tile,
                    false,
                    true,
                    &mut self.error,
                ),
            }
            if let Some(current) = document
                .controller
                .value()
                .graphics
                .tiles
                .get(self.selected_tile)
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
                TilePixelPointerAction::PaintForeground
                | TilePixelPointerAction::PaintBackground => {
                    let color = match action {
                        TilePixelPointerAction::PaintForeground => self.foreground_color,
                        TilePixelPointerAction::PaintBackground => self.background_color,
                        _ => unreachable!(),
                    };
                    apply_pixel(
                        &mut document.controller,
                        self.selected_tile,
                        x,
                        y,
                        color,
                        tile,
                        &mut self.error,
                    );
                }
                TilePixelPointerAction::PickForeground => {
                    self.foreground_color = tile.pixel(x, y).unwrap_or(0);
                    self.status.select_foreground_color(self.foreground_color);
                }
                TilePixelPointerAction::PickBackground => {
                    self.background_color = tile.pixel(x, y).unwrap_or(0);
                    self.status.select_background_color(self.background_color);
                }
            }
        }
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Graphics error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn poll_save(&mut self, context: &egui::Context) {
        let Some(completion) = self.save_worker.show(context) else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            self.error = Some("graphics save completed after its document was closed".into());
            return;
        };
        document_io::complete_save(&mut document.controller, completion, &mut self.error);
    }

    fn paste_tile(&mut self, text: &str) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let target = self
            .clipboard_paste_target
            .take()
            .unwrap_or(self.selected_tile);
        if paste_tile(&mut document.controller, target, text, &mut self.error) {
            self.selected_tile = target;
            self.status.set(format!(
                "Pasted tile from clipboard over tile 0x{target:X}."
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{
        GraphicsColorMapFilters, GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile,
    };

    fn controller() -> GraphicsDocumentController {
        let file = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT])],
            },
        };
        GraphicsDocumentController::decode("graphics.lmgfx".into(), &file.encode().unwrap())
            .unwrap()
    }

    #[test]
    fn pixel_edit_is_controller_revisioned_and_undoable() {
        let mut controller = controller();
        let tile = controller.value().graphics.tiles[0].clone();
        let mut error = None;
        apply_pixel(&mut controller, 0, 7, 6, 15, tile, &mut error);
        assert_eq!(error, None);
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().graphics.tiles[0].pixel(7, 6), Some(15));
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.value().graphics.tiles[0].pixel(7, 6), Some(0));
    }

    #[test]
    fn tile_flips_are_controller_revisioned_composable_and_undoable() {
        let file = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                    index.to_le_bytes()[0] & 0x0f
                }))],
            },
        };
        let mut controller =
            GraphicsDocumentController::decode("graphics.lmgfx".into(), &file.encode().unwrap())
                .unwrap();
        let original = controller.value().graphics.tiles[0].clone();
        let mut error = None;
        flip_tile(&mut controller, 0, true, false, &mut error);
        assert_eq!(error, None);
        assert_eq!(controller.revision(), 1);
        assert_eq!(
            controller.value().graphics.tiles[0],
            original.flipped(true, false)
        );
        flip_tile(&mut controller, 0, false, true, &mut error);
        assert_eq!(controller.revision(), 2);
        assert_eq!(
            controller.value().graphics.tiles[0],
            original.flipped(true, true)
        );
        assert!(controller.undo(2).unwrap());
        assert_eq!(
            controller.value().graphics.tiles[0],
            original.flipped(true, false)
        );
        assert!(controller.undo(3).unwrap());
        assert_eq!(controller.value().graphics.tiles[0], original);
    }

    #[test]
    fn wrapping_tile_shifts_are_controller_revisioned_and_undoable() {
        let file = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                    index.to_le_bytes()[0] & 0x0f
                }))],
            },
        };
        let mut controller =
            GraphicsDocumentController::decode("graphics.lmgfx".into(), &file.encode().unwrap())
                .unwrap();
        let original = controller.value().graphics.tiles[0].clone();
        let mut error = None;
        shift_tile(&mut controller, 0, TileShift::Left, &mut error);
        assert_eq!(error, None);
        assert_eq!(controller.revision(), 1);
        assert_eq!(
            controller.value().graphics.tiles[0],
            original.shifted_wrapping(TileShift::Left)
        );
        shift_tile(&mut controller, 0, TileShift::Down, &mut error);
        assert_eq!(controller.revision(), 2);
        assert_eq!(
            controller.value().graphics.tiles[0],
            original
                .shifted_wrapping(TileShift::Left)
                .shifted_wrapping(TileShift::Down)
        );
        assert!(controller.undo(2).unwrap());
        assert_eq!(
            controller.value().graphics.tiles[0],
            original.shifted_wrapping(TileShift::Left)
        );
        assert!(controller.undo(3).unwrap());
        assert_eq!(controller.value().graphics.tiles[0], original);
    }

    #[test]
    fn color_map_application_is_one_controller_revision_and_is_undoable() {
        let file = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                    index.to_le_bytes()[0] & 0x0f
                }))],
            },
        };
        let mut controller =
            GraphicsDocumentController::decode("graphics.lmgfx".into(), &file.encode().unwrap())
                .unwrap();
        let original = controller.value().graphics.tiles[0].clone();
        let mut filters = GraphicsColorMapFilters::default();
        filters.set_destination(9, 3, 12).unwrap();
        let mapped = filters.apply(9, &original).unwrap();
        let mut error = None;
        apply_tile(&mut controller, 0, mapped.clone(), &mut error);

        assert_eq!(error, None);
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().graphics.tiles[0], mapped);
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.value().graphics.tiles[0], original);
    }

    #[test]
    fn dirty_graphics_document_requires_close_confirmation() {
        let mut controller = controller();
        let tile = controller.value().graphics.tiles[0].clone();
        apply_pixel(&mut controller, 0, 0, 0, 1, tile, &mut None);
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: lm_graphics::Palette {
                colors: vec![lm_graphics::Bgr555(0); 16],
            },
        };
        let mut editor = GraphicsEditor {
            document: Some(GraphicsDocument {
                controller,
                palette,
            }),
            ..GraphicsEditor::default()
        };
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
        assert_eq!(editor.pending_close, Some(PendingClose::Application));
    }

    #[test]
    fn typed_tile_paste_is_revisioned_and_rejects_other_domains() {
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: lm_graphics::Palette {
                colors: vec![lm_graphics::Bgr555(0); 16],
            },
        };
        let mut editor = GraphicsEditor {
            document: Some(GraphicsDocument {
                controller: controller(),
                palette,
            }),
            ..GraphicsEditor::default()
        };
        let replacement = IndexedTile::new([9; IndexedTile::PIXEL_COUNT]);
        editor.paste_tile(&native_clipboard::encode_graphics_tile(&replacement).unwrap());
        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 1);
        assert_eq!(document.controller.value().graphics.tiles[0], replacement);
        assert_eq!(
            editor.status.text(),
            Some("Pasted tile from clipboard over tile 0x0.")
        );
        editor.clipboard_paste_target = Some(0);
        editor.paste_tile(&native_clipboard::encode_palette_color(lm_graphics::Bgr555(3)).unwrap());
        assert!(editor.error.is_some());
        assert_eq!(editor.clipboard_paste_target, None);
        assert_eq!(editor.document.as_ref().unwrap().controller.revision(), 1);
    }
}
