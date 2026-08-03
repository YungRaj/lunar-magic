use crate::{
    document_loader::DocumentLoader,
    level_editor_forms,
    overworld_editor_animation::OverworldAnimationPanel,
    overworld_editor_palette::OverworldPalettePanel,
    overworld_editor_records::OverworldRecordPanels,
    overworld_editor_render::{self, OverworldAssets},
};
use eframe::egui;
use lm_app::{
    AppState, Command, OverworldController, OverworldControllerEdit, OverworldLayerId,
    ProfiledControllerSnapshot,
};
use lm_graphics::PaletteOwnership;
use lm_project::{CompleteOverworldFile, CompleteOverworldShape};

mod commit;
mod lifecycle;
mod transfer;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Records,
    Palette,
    Animation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct PendingOpen {
    profiled: ProfiledControllerSnapshot,
    slot: String,
}

struct PendingLoad {
    open: PendingOpen,
    slot: u16,
}

struct Workspace {
    controller: OverworldController,
    profiled: ProfiledControllerSnapshot,
    slot: u16,
    image: lm_rom::RomImage,
    ownership: PaletteOwnership,
    assets: OverworldAssets,
}

#[derive(Default)]
pub(crate) struct RomOverworldEditor {
    workspace: Option<Workspace>,
    pending_open: Option<PendingOpen>,
    panel: Panel,
    records: OverworldRecordPanels,
    palette: OverworldPalettePanel,
    animation: OverworldAnimationPanel,
    layer: usize,
    x: usize,
    y: usize,
    tile: String,
    loaded: Option<(u64, usize, usize, usize)>,
    paint_enabled: bool,
    paint_anchor: Option<(usize, usize)>,
    completed_reveals: usize,
    rendered_key: Option<(u64, usize)>,
    texture: Option<egui::TextureHandle>,
    map16_page: usize,
    map16_rendered_key: Option<(u64, usize)>,
    map16_texture: Option<egui::TextureHandle>,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    transfer_loader: DocumentLoader,
    transfer_persistence: crate::persistence_worker::PersistenceWorker,
}

impl RomOverworldEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        self.poll_transfer_file_io(context, revision);
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, revision);
        }
        self.open_dialog(context);
        let mut command = match self.manifest_loader.show(context, revision) {
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
        };
        if self.workspace.is_some() {
            self.load_tile();
            self.refresh_texture(context);
            self.refresh_map16_texture(context);
            egui::Window::new("ROM Complete Overworld Editor")
                .default_size([820.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
}

impl RomOverworldEditor {
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let (stale, shape, slot, controller_revision, data, modes, ownership) = {
            let workspace = self.workspace.as_ref()?;
            (
                workspace.controller.revision() != revision,
                workspace.profiled.profile.overworld_shape,
                workspace.slot,
                workspace.controller.revision(),
                workspace.controller.data().clone(),
                workspace.profiled.profile.exanimation_double_size_modes,
                workspace.ownership.clone(),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        self.complete_file_controls(ui, stale, revision);
        self.world_canvas(ui, shape, stale);
        self.layer_tile_controls(ui, shape, stale);
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Records, "Records");
            ui.selectable_value(&mut self.panel, Panel::Palette, "Palette");
            ui.selectable_value(&mut self.panel, Panel::Animation, "Animation");
        });
        let file = CompleteOverworldFile {
            source_slot: slot,
            shape,
            data,
        };
        let edit = match self.panel {
            Panel::Records => self.records.show(ui, &file, controller_revision),
            Panel::Palette => self.palette.show(ui, &file.data.palette, &ownership),
            Panel::Animation => {
                self.animation
                    .show(ui, &file.data.animation, &modes, controller_revision)
            }
        };
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !stale => self.apply(edit),
                Ok(_) => self.error = Some("stale overworld workspace cannot accept edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        self.commit_controls(ui, stale, revision)
    }

    fn layer_tile_controls(
        &mut self,
        ui: &mut egui::Ui,
        shape: CompleteOverworldShape,
        stale: bool,
    ) {
        let old_selection = (self.layer, self.x, self.y);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.layer, 0, "Layer 1");
            ui.selectable_value(&mut self.layer, 1, "Layer 2");
        });
        ui.add(egui::Slider::new(&mut self.x, 0..=shape.width.saturating_sub(1)).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0..=shape.height.saturating_sub(1)).text("Y"));
        if old_selection != (self.layer, self.x, self.y) {
            self.paint_anchor = None;
            self.loaded = None;
            self.load_tile();
        }
        ui.horizontal(|ui| {
            ui.label("Map16 tile");
            ui.text_edit_singleline(&mut self.tile);
        });
        self.map16_picker(ui);
        if ui
            .add_enabled(!stale, egui::Button::new("Apply layer tile"))
            .clicked()
        {
            match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
                Ok(tile) => self.apply(OverworldControllerEdit::SetLayerTile {
                    layer: self.layer_id(),
                    x: self.x,
                    y: self.y,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn map16_picker(&mut self, ui: &mut egui::Ui) {
        let page_count = self
            .workspace
            .as_ref()
            .map_or(0, |workspace| workspace.assets.map16.set.pages.len());
        ui.collapsing("Visual Map16 tile picker", |ui| {
            let previous_page = self.map16_page;
            ui.add(
                egui::Slider::new(&mut self.map16_page, 0..=page_count.saturating_sub(1))
                    .text("Map16 page"),
            );
            if previous_page != self.map16_page {
                self.map16_rendered_key = None;
                self.refresh_map16_texture(ui.ctx());
            }
            let Some(texture) = self.map16_texture.clone() else {
                ui.label("This Map16 page cannot be previewed with the current overworld assets.");
                return;
            };
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click()));
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
                && let Some(index) =
                    crate::map16_editor_render::selected_tile(response.rect, position)
                && let Some(tile) = self
                    .map16_page
                    .checked_mul(lm_level::Map16Page::TILE_COUNT)
                    .and_then(|base| base.checked_add(index))
                    .and_then(|tile| u16::try_from(tile).ok())
            {
                self.tile = format!("{tile:04X}");
            }
            if let Ok(tile) = level_editor_forms::parse_hex_u16(&self.tile, "overworld tile")
                && usize::from(tile) / lm_level::Map16Page::TILE_COUNT == self.map16_page
            {
                let index = usize::from(tile) % lm_level::Map16Page::TILE_COUNT;
                let cell = response.rect.width() / 16.0;
                let column = f32::from(u8::try_from(index % 16).unwrap_or_default());
                let row = f32::from(u8::try_from(index / 16).unwrap_or_default());
                let minimum = response.rect.min + egui::vec2(column * cell, row * cell);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell)),
                    0.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
        });
    }

    fn world_canvas(&mut self, ui: &mut egui::Ui, shape: CompleteOverworldShape, stale: bool) {
        let reveal_count = self.workspace.as_ref().map_or(0, |workspace| {
            workspace.controller.data().event_reveals.entries.len()
        });
        if ui
            .add(
                egui::Slider::new(&mut self.completed_reveals, 0..=reveal_count)
                    .text("Completed event reveals"),
            )
            .changed()
        {
            self.rendered_key = None;
        }
        let Some(texture) = self.texture.clone() else {
            ui.label("Overworld preview unavailable; property editing remains available.");
            return;
        };
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.paint_enabled, "Paint selected Map16 tile");
            if self.paint_enabled {
                ui.label("Drag across the map to paint a continuous stroke.");
            }
        });
        let mut paint_position = None;
        egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click_and_drag()));
            if (response.clicked() || response.dragged())
                && let Some(position) = response.interact_pointer_pos()
                && let Some((x, y)) = overworld_editor_render::selected_tile(
                    response.rect,
                    position,
                    shape.width,
                    shape.height,
                )
            {
                self.x = x;
                self.y = y;
                self.loaded = None;
                if self.paint_enabled && !stale {
                    paint_position = Some((x, y));
                } else {
                    self.load_tile();
                    self.refresh_map16_texture(ui.ctx());
                }
            }
            if shape.width > 0 && shape.height > 0 {
                let width = f32::from(u16::try_from(shape.width).unwrap_or(1));
                let height = f32::from(u16::try_from(shape.height).unwrap_or(1));
                let selected_x = f32::from(u16::try_from(self.x).unwrap_or_default());
                let selected_y = f32::from(u16::try_from(self.y).unwrap_or_default());
                let cell_width = response.rect.width() / width;
                let cell_height = response.rect.height() / height;
                let minimum = response.rect.min
                    + egui::vec2(selected_x * cell_width, selected_y * cell_height);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(minimum, egui::vec2(cell_width, cell_height)),
                    0.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
        });
        if let Some(position) = paint_position {
            self.paint_to(position);
        }
        if !ui.input(|input| input.pointer.primary_down()) {
            self.paint_anchor = None;
        }
    }

    fn refresh_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), self.completed_reveals);
        if self.rendered_key == Some(key) {
            return;
        }
        let file = CompleteOverworldFile {
            source_slot: workspace.slot,
            shape: workspace.profiled.profile.overworld_shape,
            data: workspace.controller.data().clone(),
        };
        match overworld_editor_render::render_texture(
            context,
            &file,
            &workspace.assets,
            self.completed_reveals,
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.rendered_key = Some(key);
            }
            Err(error) => {
                self.texture = None;
                self.rendered_key = Some(key);
                self.error = Some(format!("could not render native overworld: {error}"));
            }
        }
    }

    fn refresh_map16_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let page_count = workspace.assets.map16.set.pages.len();
        self.map16_page = self.map16_page.min(page_count.saturating_sub(1));
        let key = (workspace.controller.revision(), self.map16_page);
        if self.map16_rendered_key == Some(key) {
            return;
        }
        let Some(page) = workspace.assets.map16.set.pages.get(self.map16_page) else {
            self.map16_texture = None;
            self.map16_rendered_key = Some(key);
            return;
        };
        let page = lm_level::Map16PageFile {
            source_page: u16::try_from(self.map16_page).unwrap_or_default(),
            page: page.clone(),
        };
        let palette = lm_graphics::PaletteInterchangeFile {
            source_palette: workspace.slot,
            palette: workspace.controller.data().palette.clone(),
        };
        match crate::map16_editor_render::render_texture(
            context,
            &page,
            &workspace.assets.graphics,
            &palette,
        ) {
            Ok(texture) => {
                self.map16_texture = Some(texture);
                self.map16_rendered_key = Some(key);
            }
            Err(_) => {
                self.map16_texture = None;
                self.map16_rendered_key = Some(key);
            }
        }
    }

    fn paint_to(&mut self, position: (usize, usize)) {
        let tile = match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
            Ok(tile) => tile,
            Err(error) => {
                self.error = Some(error);
                self.paint_anchor = None;
                return;
            }
        };
        let cells = grid_line(self.paint_anchor.unwrap_or(position), position);
        self.paint_anchor = Some(position);
        let layer = self.layer_id();
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn current_tile(&self, layer: OverworldLayerId, x: usize, y: usize) -> Option<u16> {
        let workspace = self.workspace.as_ref()?;
        let shape = workspace.profiled.profile.overworld_shape;
        let index = y.checked_mul(shape.width)?.checked_add(x)?;
        match layer {
            OverworldLayerId::Layer1 => workspace.controller.data().layers.layer1.tiles.get(index),
            OverworldLayerId::Layer2 => workspace.controller.data().layers.layer2.tiles.get(index),
        }
        .copied()
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        revision: u64,
    ) -> Option<Command> {
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
            .is_some_and(|value| value.controller.is_modified());
        let transfer_busy =
            self.transfer_loader.is_running() || self.transfer_persistence.is_running();
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit all nine overworld payloads"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit and reclaim all nine"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged overworld changes"
        } else {
            "No staged changes"
        });
        None
    }

    fn apply(&mut self, edit: OverworldControllerEdit) {
        self.apply_many(&[edit]);
    }

    fn apply_many(&mut self, edits: &[OverworldControllerEdit]) {
        if edits.is_empty() {
            return;
        }
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("overworld workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(edits) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn load_tile(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (workspace.controller.revision(), self.layer, self.x, self.y);
        if self.loaded == Some(key) {
            return;
        }
        let shape = workspace.profiled.profile.overworld_shape;
        self.x = self.x.min(shape.width.saturating_sub(1));
        self.y = self.y.min(shape.height.saturating_sub(1));
        let tiles = if self.layer == 0 {
            &workspace.controller.data().layers.layer1.tiles
        } else {
            &workspace.controller.data().layers.layer2.tiles
        };
        if let Some(tile) = tiles.get(self.y * shape.width + self.x) {
            self.tile = format!("{tile:04X}");
            let page = usize::from(*tile) / lm_level::Map16Page::TILE_COUNT;
            if page != self.map16_page {
                self.map16_page = page;
                self.map16_rendered_key = None;
            }
        }
        self.loaded = Some((workspace.controller.revision(), self.layer, self.x, self.y));
    }

    fn layer_id(&self) -> OverworldLayerId {
        if self.layer == 0 {
            OverworldLayerId::Layer1
        } else {
            OverworldLayerId::Layer2
        }
    }

    fn invalidate(&mut self) {
        self.loaded = None;
        self.rendered_key = None;
        self.map16_rendered_key = None;
        self.records.invalidate();
        self.animation.invalidate();
    }
}

fn grid_line(start: (usize, usize), end: (usize, usize)) -> Vec<(usize, usize)> {
    let (mut x, mut y) = (start.0 as i64, start.1 as i64);
    let (end_x, end_y) = (end.0 as i64, end.1 as i64);
    let dx = (end_x - x).abs();
    let step_x = if x < end_x { 1 } else { -1 };
    let dy = -(end_y - y).abs();
    let step_y = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    let mut cells = Vec::new();
    loop {
        cells.push((x as usize, y as usize));
        if x == end_x && y == end_y {
            break;
        }
        let doubled = error * 2;
        if doubled >= dy {
            error += dy;
            x += step_x;
        }
        if doubled <= dx {
            error += dx;
            y += step_y;
        }
    }
    cells
}

fn stroke_edits(
    layer: OverworldLayerId,
    cells: &[(usize, usize)],
    tile: u16,
    mut current_tile: impl FnMut(usize, usize) -> Option<u16>,
) -> Vec<OverworldControllerEdit> {
    cells
        .iter()
        .copied()
        .filter_map(|(x, y)| {
            (current_tile(x, y) != Some(tile)).then_some(OverworldControllerEdit::SetLayerTile {
                layer,
                x,
                y,
                tile,
            })
        })
        .collect()
}

#[cfg(test)]
mod canvas_tests {
    use super::{OverworldControllerEdit, OverworldLayerId, grid_line, stroke_edits};

    #[test]
    fn drag_strokes_cover_skipped_grid_cells_in_both_directions() {
        assert_eq!(
            grid_line((1, 2), (5, 2)),
            vec![(1, 2), (2, 2), (3, 2), (4, 2), (5, 2)]
        );
        assert_eq!(
            grid_line((4, 4), (1, 1)),
            vec![(4, 4), (3, 3), (2, 2), (1, 1)]
        );
        assert_eq!(grid_line((3, 7), (3, 7)), vec![(3, 7)]);
    }

    #[test]
    fn stroke_batch_preserves_order_and_omits_unchanged_cells() {
        let edits = stroke_edits(
            OverworldLayerId::Layer2,
            &[(2, 4), (3, 4), (4, 4)],
            0x1234,
            |x, _| (x == 3).then_some(0x1234),
        );
        assert_eq!(
            edits,
            vec![
                OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: 2,
                    y: 4,
                    tile: 0x1234,
                },
                OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: 4,
                    y: 4,
                    tile: 0x1234,
                },
            ]
        );
    }
}
