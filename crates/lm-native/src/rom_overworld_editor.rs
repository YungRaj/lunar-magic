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
    ProfiledControllerSnapshot, SmwMainOverworldLayer2Controller,
};
use lm_graphics::{Palette, PaletteOwnership};
use lm_overworld::{
    OverworldEndpoint, OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget,
};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MapPaintTool {
    #[default]
    Select,
    Brush,
    Rectangle,
    Fill,
    RouteSource,
    RouteDestination,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct PendingOpen {
    profiled: ProfiledControllerSnapshot,
    slot: String,
    rom_path: Option<std::path::PathBuf>,
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
    native_appearances: Option<lm_render::NativeOverworldAppearancePair>,
}

struct MainLayer2Workspace {
    controller: SmwMainOverworldLayer2Controller,
    original_paths: OverworldPathLinkTable,
    paths: OverworldPathLinkTable,
    palette: Palette,
    assets: OverworldAssets,
}

#[derive(Default)]
struct MainPathLinkForm {
    index: usize,
    source_x: String,
    source_y: String,
    source_submap: String,
    destination_x: String,
    destination_y: String,
    destination_submap: String,
    target_x: String,
    target_y: String,
    loaded: Option<usize>,
}

#[derive(Default)]
pub(crate) struct RomOverworldEditor {
    workspace: Option<Workspace>,
    main_layer2_workspace: Option<MainLayer2Workspace>,
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
    paint_tool: MapPaintTool,
    paint_anchor: Option<(usize, usize)>,
    completed_reveals: usize,
    rendered_key: Option<(u64, usize)>,
    texture: Option<egui::TextureHandle>,
    map16_page: usize,
    map16_rendered_key: Option<(u64, usize)>,
    map16_texture: Option<egui::TextureHandle>,
    direct_tile_palette: usize,
    direct_tile_rendered_palette: Option<usize>,
    direct_tile_texture: Option<egui::TextureHandle>,
    main_path: MainPathLinkForm,
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
    fn main_layer2_contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let workspace = self.main_layer2_workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        let shape = CompleteOverworldShape {
            width: lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
            height: lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
            event_reveals: 0,
            endpoints: 0,
            messages: 0,
            sprites: 0,
            sprite_record_len: 0,
            palette_colors: workspace.palette.colors.len(),
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let paths_modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.paths != workspace.original_paths);
        ui.label("Gameplay-consumed SMW US main-map Layer 2 (128x64 tiles)");
        self.layer = 1;
        self.world_canvas(ui, shape, stale || paths_modified);
        self.main_layer2_tile_controls(ui, shape, stale || paths_modified);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.is_modified());
        if ui
            .add_enabled(
                modified && !paths_modified && !stale,
                egui::Button::new("Commit playable Layer 2 map"),
            )
            .clicked()
        {
            match self.prepare_main_layer2_commit() {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        ui.label(if modified {
            "Staged playable map changes"
        } else {
            "No staged map changes"
        });
        if paths_modified {
            ui.small("Commit or discard the staged route-link edit before changing terrain.");
        }
        ui.separator();
        if let Some(path_command) = self.main_path_link_controls(ui, stale, modified) {
            return Some(path_command);
        }
        None
    }

    fn main_path_link_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        terrain_modified: bool,
    ) -> Option<Command> {
        let path_count = self
            .main_layer2_workspace
            .as_ref()
            .map_or(0, |workspace| workspace.paths.links.len());
        let paths_modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.paths != workspace.original_paths);
        let mut command = None;
        ui.collapsing("Gameplay route links", |ui| {
            ui.label("Native source/destination endpoints and engine target bytes (hexadecimal).");
            ui.small(
                "Route tools use the left plane for submap 00. On the right shared submap sheet, \n+                 enter submap 01-06 first; a click retains that endpoint's submap ID.",
            );
            if path_count == 0 {
                ui.label("No gameplay route links are installed.");
                return;
            }
            let previous = self.main_path.index;
            ui.add(egui::Slider::new(&mut self.main_path.index, 0..=path_count - 1).text("Link"));
            if self.main_path.index != previous {
                self.load_main_path_link();
            }
            egui::Grid::new("playable-overworld-path-link-form")
                .striped(true)
                .show(ui, |ui| {
                    path_form_row(ui, "Source X", &mut self.main_path.source_x);
                    path_form_row(ui, "Source Y", &mut self.main_path.source_y);
                    path_form_row(ui, "Source submap", &mut self.main_path.source_submap);
                    path_form_row(ui, "Destination X", &mut self.main_path.destination_x);
                    path_form_row(ui, "Destination Y", &mut self.main_path.destination_y);
                    path_form_row(
                        ui,
                        "Destination submap",
                        &mut self.main_path.destination_submap,
                    );
                    path_form_row(ui, "Target X tile", &mut self.main_path.target_x);
                    path_form_row(ui, "Target Y tile", &mut self.main_path.target_y);
                });
            ui.horizontal(|ui| {
                if ui.button("Reload link").clicked() {
                    self.load_main_path_link();
                }
                if ui
                    .add_enabled(
                        !stale && !terrain_modified,
                        egui::Button::new("Apply route link"),
                    )
                    .clicked()
                    && let Err(error) = self.apply_main_path_link()
                {
                    self.error = Some(error);
                }
                if ui
                    .add_enabled(
                        paths_modified && !stale && !terrain_modified,
                        egui::Button::new("Commit route links"),
                    )
                    .clicked()
                    && let Some(workspace) = self.main_layer2_workspace.as_ref()
                {
                    command = Some(Command::ReplaceNativeOverworldPathLinks {
                        rev: workspace.controller.revision(),
                        table: Box::new(workspace.paths.clone()),
                    });
                }
            });
            if terrain_modified {
                ui.small("Commit or discard the staged terrain edit before changing route links.");
            } else if paths_modified {
                ui.small("Staged gameplay route changes");
            }
        });
        command
    }

    fn load_main_path_link(&mut self) {
        let Some(link) = self
            .main_layer2_workspace
            .as_ref()
            .and_then(|workspace| workspace.paths.links.get(self.main_path.index))
            .copied()
        else {
            self.main_path.loaded = None;
            return;
        };
        self.main_path.set(link);
    }

    fn apply_main_path_link(&mut self) -> Result<(), String> {
        if self.main_path.loaded != Some(self.main_path.index) {
            return Err("reload the selected route link before applying it".into());
        }
        let link = self.main_path.parse()?;
        let workspace = self
            .main_layer2_workspace
            .as_mut()
            .ok_or("playable overworld workspace is closed")?;
        let mut staged = workspace.paths.clone();
        staged.links[self.main_path.index] = link;
        staged.encode_planes().map_err(|error| error.to_string())?;
        workspace.paths = staged;
        Ok(())
    }

    fn main_layer2_tile_controls(
        &mut self,
        ui: &mut egui::Ui,
        shape: CompleteOverworldShape,
        stale: bool,
    ) {
        let old_selection = (self.x, self.y);
        ui.label("Layer 2 packed 8x8 tilemap");
        ui.add(egui::Slider::new(&mut self.x, 0..=shape.width.saturating_sub(1)).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0..=shape.height.saturating_sub(1)).text("Y"));
        if old_selection != (self.x, self.y) {
            self.paint_anchor = None;
            self.loaded = None;
            self.load_main_layer2_tile();
        }
        ui.horizontal(|ui| {
            ui.label("SNES tilemap word");
            ui.text_edit_singleline(&mut self.tile);
        });
        self.direct_tile_picker(ui);
        if ui
            .add_enabled(!stale, egui::Button::new("Apply layer tile"))
            .clicked()
        {
            match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
                Ok(tile) => self.apply(OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: self.x,
                    y: self.y,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn direct_tile_picker(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Visual 8x8 tile picker", |ui| {
            let previous_palette = self.direct_tile_palette;
            ui.add(egui::Slider::new(&mut self.direct_tile_palette, 0..=7).text("Palette row"));
            if previous_palette != self.direct_tile_palette {
                self.direct_tile_rendered_palette = None;
            }
            if self.direct_tile_rendered_palette != Some(self.direct_tile_palette) {
                self.direct_tile_texture =
                    self.main_layer2_workspace.as_ref().and_then(|workspace| {
                        overworld_editor_render::render_layer2_graphics_texture(
                            ui.ctx(),
                            &workspace.assets.graphics,
                            &workspace.palette,
                            self.direct_tile_palette,
                        )
                        .map_err(|error| self.error = Some(error))
                        .ok()
                    });
                self.direct_tile_rendered_palette = Some(self.direct_tile_palette);
            }
            let Some(texture) = self.direct_tile_texture.clone() else {
                ui.label("The current overworld graphics cannot be previewed.");
                return;
            };
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click()));
            let columns = 16;
            let rows = self.main_layer2_workspace.as_ref().map_or(0, |workspace| {
                workspace
                    .assets
                    .graphics
                    .graphics
                    .tiles
                    .len()
                    .div_ceil(columns)
            });
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
                && let Some((x, y)) =
                    overworld_editor_render::selected_tile(response.rect, position, columns, rows)
                && let Some(index) = y.checked_mul(columns).and_then(|base| base.checked_add(x))
                && index
                    < self.main_layer2_workspace.as_ref().map_or(0, |workspace| {
                        workspace.assets.graphics.graphics.tiles.len()
                    })
                && let Ok(word) = level_editor_forms::parse_hex_u16(&self.tile, "SNES tilemap word")
                && let Ok(tile_number) = u16::try_from(index)
            {
                let word = (word & !0x1fff) | tile_number | (self.direct_tile_palette as u16) << 10;
                self.tile = format!("{word:04X}");
            }
        });
    }

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
        if self.main_layer2_workspace.is_some() {
            self.load_main_layer2_tile();
            self.refresh_main_layer2_texture(context);
            self.refresh_map16_texture(context);
            egui::Window::new("ROM Playable Main Overworld Layer 2 Editor")
                .default_size([820.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.main_layer2_contents(ui, revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
}

impl MainPathLinkForm {
    fn set(&mut self, link: OverworldPathLink) {
        self.source_x = format!("{:04X}", link.source.x);
        self.source_y = format!("{:04X}", link.source.y);
        self.source_submap = format!("{:02X}", link.source.submap);
        self.destination_x = format!("{:04X}", link.destination.x);
        self.destination_y = format!("{:04X}", link.destination.y);
        self.destination_submap = format!("{:02X}", link.destination.submap);
        self.target_x = format!("{:02X}", link.target.x_tile);
        self.target_y = format!("{:02X}", link.target.y_tile);
        self.loaded = Some(self.index);
    }

    fn parse(&self) -> Result<OverworldPathLink, String> {
        Ok(OverworldPathLink {
            source: OverworldEndpoint {
                x: level_editor_forms::parse_hex_u16(&self.source_x, "route source X")?,
                y: level_editor_forms::parse_hex_u16(&self.source_y, "route source Y")?,
                submap: level_editor_forms::parse_hex_u8(
                    &self.source_submap,
                    "route source submap",
                )?,
            },
            destination: OverworldEndpoint {
                x: level_editor_forms::parse_hex_u16(&self.destination_x, "route destination X")?,
                y: level_editor_forms::parse_hex_u16(&self.destination_y, "route destination Y")?,
                submap: level_editor_forms::parse_hex_u8(
                    &self.destination_submap,
                    "route destination submap",
                )?,
            },
            target: OverworldPathTarget {
                x_tile: level_editor_forms::parse_hex_u8(&self.target_x, "route target X")?,
                y_tile: level_editor_forms::parse_hex_u8(&self.target_y, "route target Y")?,
            },
        })
    }
}

fn path_form_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

impl RomOverworldEditor {
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let (stale, shape, slot, controller_revision, data, modes, ownership, native_summary) = {
            let workspace = self.workspace.as_ref()?;
            (
                workspace.controller.revision() != revision,
                workspace.profiled.profile.overworld_shape,
                workspace.slot,
                workspace.controller.revision(),
                workspace.controller.data().clone(),
                workspace.profiled.profile.exanimation_double_size_modes,
                workspace.ownership.clone(),
                workspace.native_appearances.as_ref().map(|pair| {
                    (
                        pair.definitions.appearances.len(),
                        pair.definitions.tooltips.len(),
                        pair.sprite_map16.loaded_len(),
                    )
                }),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let transfer_busy = self.transfer_busy();
        let editing_blocked = stale || transfer_busy;
        if transfer_busy {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Complete-overworld file transfer is active; editing is temporarily disabled.",
            );
        }
        self.complete_file_controls(ui, stale, revision);
        if let Some((appearances, tooltips, map16_bytes)) = native_summary {
            ui.label(format!(
                "ROM-adjacent native sprite display: {appearances} appearances, {tooltips} tooltips, {map16_bytes} Sprite Map16 bytes"
            ));
        }
        self.world_canvas(ui, shape, editing_blocked);
        self.layer_tile_controls(ui, shape, editing_blocked);
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
                Ok(edit) if !editing_blocked => self.apply(edit),
                Ok(_) if transfer_busy => {
                    self.error = Some("overworld editing is disabled during file transfer".into());
                }
                Ok(_) => self.error = Some("stale overworld workspace cannot accept edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        self.commit_controls(ui, editing_blocked, revision)
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
            .assets()
            .map_or(0, |assets| assets.map16.set.pages.len());
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
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Select, "Select");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Brush, "Brush");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Rectangle, "Rectangle");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Fill, "Fill");
            if self.main_layer2_workspace.is_some() {
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::RouteSource,
                    "Set route source",
                );
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::RouteDestination,
                    "Set route destination",
                );
            }
        });
        let mut action = None;
        egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click_and_drag()));
            if (response.clicked()
                || response.dragged()
                || response.drag_started()
                || response.drag_stopped())
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
                match self.paint_tool {
                    MapPaintTool::Select => {
                        self.load_tile();
                        self.refresh_map16_texture(ui.ctx());
                    }
                    MapPaintTool::Brush if !stale => action = Some((MapPaintTool::Brush, (x, y))),
                    MapPaintTool::Fill if !stale && response.clicked() => {
                        action = Some((MapPaintTool::Fill, (x, y)));
                    }
                    MapPaintTool::Rectangle if !stale => {
                        if response.drag_started() {
                            self.paint_anchor = ui
                                .input(|input| input.pointer.press_origin())
                                .and_then(|origin| {
                                    overworld_editor_render::selected_tile(
                                        response.rect,
                                        origin,
                                        shape.width,
                                        shape.height,
                                    )
                                })
                                .or(Some((x, y)));
                        }
                        if response.drag_stopped() || response.clicked() {
                            action = Some((MapPaintTool::Rectangle, (x, y)));
                        }
                    }
                    MapPaintTool::RouteSource if !stale && response.clicked() => {
                        let submap = level_editor_forms::parse_hex_u8(
                            &self.main_path.source_submap,
                            "route source submap",
                        );
                        if let Ok(submap) = submap
                            && let Some(endpoint) =
                                route_canvas_endpoint(response.rect, position, submap)
                        {
                            self.main_path.source_x = format!("{:04X}", endpoint.x);
                            self.main_path.source_y = format!("{:04X}", endpoint.y);
                            self.main_path.source_submap = format!("{:02X}", endpoint.submap);
                        }
                    }
                    MapPaintTool::RouteDestination if !stale && response.clicked() => {
                        let submap = level_editor_forms::parse_hex_u8(
                            &self.main_path.destination_submap,
                            "route destination submap",
                        );
                        if let Ok(submap) = submap
                            && let Some(endpoint) =
                                route_canvas_endpoint(response.rect, position, submap)
                        {
                            self.main_path.destination_x = format!("{:04X}", endpoint.x);
                            self.main_path.destination_y = format!("{:04X}", endpoint.y);
                            self.main_path.destination_submap = format!("{:02X}", endpoint.submap);
                        }
                    }
                    _ => {}
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
            self.paint_main_route_overlay(ui, response.rect);
        });
        if let Some((tool, position)) = action {
            match tool {
                MapPaintTool::Brush => self.paint_to(position),
                MapPaintTool::Rectangle => self.paint_rectangle_to(position),
                MapPaintTool::Fill => self.fill_at(position),
                MapPaintTool::Select
                | MapPaintTool::RouteSource
                | MapPaintTool::RouteDestination => {}
            }
        }
        if self.paint_tool == MapPaintTool::Brush && !ui.input(|input| input.pointer.primary_down())
        {
            self.paint_anchor = None;
        }
    }

    fn paint_main_route_overlay(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self.main_layer2_workspace.is_none() {
            return;
        }
        let Ok(link) = self.main_path.parse() else {
            return;
        };
        let point = |endpoint: OverworldEndpoint| {
            route_endpoint_canvas_pixel(endpoint).map(|(x, y)| {
                rect.min
                    + egui::vec2(
                        f32::from(x) / 1024.0 * rect.width(),
                        f32::from(y) / 512.0 * rect.height(),
                    )
            })
        };
        let source = point(link.source);
        let destination = point(link.destination);
        if let (Some(source), Some(destination)) = (source, destination) {
            ui.painter().line_segment(
                [source, destination],
                egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            );
        }
        for (position, color) in [
            (source, egui::Color32::CYAN),
            (destination, egui::Color32::MAGENTA),
        ] {
            if let Some(position) = position {
                ui.painter().circle_filled(position, 5.0, color);
                ui.painter().circle_stroke(
                    position,
                    6.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::BLACK),
                );
            }
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
            workspace.native_appearances.as_ref(),
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

    fn refresh_main_layer2_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.main_layer2_workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), 0);
        if self.rendered_key == Some(key) {
            return;
        }
        match overworld_editor_render::render_layer_texture(
            context,
            workspace.controller.layer(),
            &workspace.palette,
            &workspace.assets,
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.rendered_key = Some(key);
            }
            Err(error) => {
                self.texture = None;
                self.rendered_key = Some(key);
                self.error = Some(format!("could not render playable main overworld: {error}"));
            }
        }
    }

    fn refresh_map16_texture(&mut self, context: &egui::Context) {
        let page_count = self
            .assets()
            .map_or(0, |assets| assets.map16.set.pages.len());
        self.map16_page = self.map16_page.min(page_count.saturating_sub(1));
        let revision = self.workspace.as_ref().map_or_else(
            || {
                self.main_layer2_workspace
                    .as_ref()
                    .map_or(0, |workspace| workspace.controller.revision())
            },
            |workspace| workspace.controller.revision(),
        );
        let key = (revision, self.map16_page);
        if self.map16_rendered_key == Some(key) {
            return;
        }
        let Some(assets) = self.assets() else {
            return;
        };
        let Some(page) = assets.map16.set.pages.get(self.map16_page) else {
            self.map16_texture = None;
            self.map16_rendered_key = Some(key);
            return;
        };
        let page = lm_level::Map16PageFile {
            source_page: u16::try_from(self.map16_page).unwrap_or_default(),
            page: page.clone(),
        };
        let palette = if let Some(workspace) = self.workspace.as_ref() {
            lm_graphics::PaletteInterchangeFile {
                source_palette: workspace.slot,
                palette: workspace.controller.data().palette.clone(),
            }
        } else if let Some(workspace) = self.main_layer2_workspace.as_ref() {
            lm_graphics::PaletteInterchangeFile {
                source_palette: 0,
                palette: workspace.palette.clone(),
            }
        } else {
            return;
        };
        match crate::map16_editor_render::render_texture(context, &page, &assets.graphics, &palette)
        {
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
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let cells = grid_line(self.paint_anchor.unwrap_or(position), position);
        self.paint_anchor = Some(position);
        let layer = self.layer_id();
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn paint_rectangle_to(&mut self, position: (usize, usize)) {
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let start = self.paint_anchor.take().unwrap_or(position);
        let cells = rectangle_cells(start, position);
        let layer = self.layer_id();
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn fill_at(&mut self, position: (usize, usize)) {
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let layer = self.layer_id();
        let (width, height, source) = if let Some(workspace) = self.workspace.as_ref() {
            let shape = workspace.profiled.profile.overworld_shape;
            let source = match layer {
                OverworldLayerId::Layer1 => &workspace.controller.data().layers.layer1.tiles,
                OverworldLayerId::Layer2 => &workspace.controller.data().layers.layer2.tiles,
            };
            (shape.width, shape.height, source.as_slice())
        } else if let Some(workspace) = self.main_layer2_workspace.as_ref() {
            if layer != OverworldLayerId::Layer2 {
                return;
            }
            let layer = workspace.controller.layer();
            (layer.width, layer.height, layer.tiles.as_slice())
        } else {
            return;
        };
        let cells = flood_fill_cells(width, height, source, position);
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn brush_tile(&mut self) -> Option<u16> {
        match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
            Ok(tile) => Some(tile),
            Err(error) => {
                self.error = Some(error);
                self.paint_anchor = None;
                None
            }
        }
    }

    fn current_tile(&self, layer: OverworldLayerId, x: usize, y: usize) -> Option<u16> {
        if let Some(workspace) = self.workspace.as_ref() {
            let shape = workspace.profiled.profile.overworld_shape;
            let index = y.checked_mul(shape.width)?.checked_add(x)?;
            return match layer {
                OverworldLayerId::Layer1 => {
                    workspace.controller.data().layers.layer1.tiles.get(index)
                }
                OverworldLayerId::Layer2 => {
                    workspace.controller.data().layers.layer2.tiles.get(index)
                }
            }
            .copied();
        }
        let workspace = self.main_layer2_workspace.as_ref()?;
        if layer != OverworldLayerId::Layer2 {
            return None;
        }
        workspace.controller.layer().tile(x, y).ok()
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
        let transfer_busy = self.transfer_busy();
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
        if self.transfer_busy() {
            self.error = Some("overworld editing is disabled during file transfer".into());
            return;
        }
        let result = if let Some(workspace) = self.workspace.as_mut() {
            workspace
                .controller
                .apply_edits(edits)
                .map_err(|error| error.to_string())
        } else if let Some(workspace) = self.main_layer2_workspace.as_mut() {
            workspace
                .controller
                .apply_edits(edits)
                .map_err(|error| error.to_string())
        } else {
            self.error = Some("overworld workspace is closed".into());
            return;
        };
        if let Err(error) = result {
            self.error = Some(error);
        } else {
            self.invalidate();
        }
    }

    fn transfer_busy(&self) -> bool {
        self.transfer_loader.is_running() || self.transfer_persistence.is_running()
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

    fn load_main_layer2_tile(&mut self) {
        let Some(workspace) = self.main_layer2_workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), 1, self.x, self.y);
        if self.loaded == Some(key) {
            return;
        }
        let layer = workspace.controller.layer();
        self.x = self.x.min(layer.width.saturating_sub(1));
        self.y = self.y.min(layer.height.saturating_sub(1));
        if let Ok(tile) = layer.tile(self.x, self.y) {
            self.tile = format!("{tile:04X}");
            self.direct_tile_palette = usize::from(lm_level::Subtile(tile).palette());
            let page = usize::from(tile) / lm_level::Map16Page::TILE_COUNT;
            if page != self.map16_page {
                self.map16_page = page;
                self.map16_rendered_key = None;
            }
        }
        self.loaded = Some((workspace.controller.revision(), 1, self.x, self.y));
    }

    fn assets(&self) -> Option<&OverworldAssets> {
        self.workspace
            .as_ref()
            .map(|workspace| &workspace.assets)
            .or_else(|| {
                self.main_layer2_workspace
                    .as_ref()
                    .map(|workspace| &workspace.assets)
            })
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

fn route_endpoint_canvas_pixel(endpoint: OverworldEndpoint) -> Option<(u16, u16)> {
    let plane_x = match endpoint.submap {
        0 => 0,
        1..=6 => 512,
        _ => return None,
    };
    Some((plane_x + (endpoint.x & 0x01ff), endpoint.y & 0x01ff))
}

fn route_canvas_endpoint(
    rect: egui::Rect,
    position: egui::Pos2,
    selected_submap: u8,
) -> Option<OverworldEndpoint> {
    let (x, y) = overworld_editor_render::selected_tile(rect, position, 128, 64)?;
    let (x, submap) = if x < 64 {
        (x, 0)
    } else if (1..=6).contains(&selected_submap) {
        (x - 64, selected_submap)
    } else {
        return None;
    };
    Some(OverworldEndpoint {
        x: u16::try_from(x.checked_mul(8)?).ok()?,
        y: u16::try_from(y.checked_mul(8)?).ok()?,
        submap,
    })
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

fn rectangle_cells(start: (usize, usize), end: (usize, usize)) -> Vec<(usize, usize)> {
    let minimum_x = start.0.min(end.0);
    let maximum_x = start.0.max(end.0);
    let minimum_y = start.1.min(end.1);
    let maximum_y = start.1.max(end.1);
    (minimum_y..=maximum_y)
        .flat_map(|y| (minimum_x..=maximum_x).map(move |x| (x, y)))
        .collect()
}

fn flood_fill_cells(
    width: usize,
    height: usize,
    tiles: &[u16],
    start: (usize, usize),
) -> Vec<(usize, usize)> {
    let Some(cell_count) = width.checked_mul(height) else {
        return Vec::new();
    };
    if width == 0
        || height == 0
        || tiles.len() != cell_count
        || start.0 >= width
        || start.1 >= height
    {
        return Vec::new();
    }
    let start_index = start.1 * width + start.0;
    let target = tiles[start_index];
    let mut visited = vec![false; cell_count];
    let mut pending = vec![start];
    let mut cells = Vec::new();
    while let Some((x, y)) = pending.pop() {
        let index = y * width + x;
        if visited[index] || tiles[index] != target {
            continue;
        }
        visited[index] = true;
        cells.push((x, y));
        if x > 0 {
            pending.push((x - 1, y));
        }
        if x + 1 < width {
            pending.push((x + 1, y));
        }
        if y > 0 {
            pending.push((x, y - 1));
        }
        if y + 1 < height {
            pending.push((x, y + 1));
        }
    }
    cells.sort_unstable_by_key(|&(x, y)| (y, x));
    cells
}

#[cfg(test)]
mod canvas_tests {
    use super::{
        MainPathLinkForm, OverworldControllerEdit, OverworldEndpoint, OverworldLayerId,
        OverworldPathLink, OverworldPathTarget, RomOverworldEditor, flood_fill_cells, grid_line,
        rectangle_cells, route_canvas_endpoint, route_endpoint_canvas_pixel, stroke_edits,
    };
    use crate::document_loader::BoundedRead;
    use eframe::egui;

    #[test]
    fn active_complete_transfer_rejects_direct_mutation_entry_point() {
        let mut editor = RomOverworldEditor::default();
        editor
            .transfer_loader
            .start(vec![BoundedRead::new(
                std::env::temp_dir().join(format!(
                    "lm-missing-overworld-transfer-{}",
                    std::process::id()
                )),
                1,
                "missing transfer fixture",
            )])
            .unwrap();
        assert!(editor.transfer_busy());
        editor.apply_many(&[OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer1,
            x: 0,
            y: 0,
            tile: 1,
        }]);
        assert_eq!(
            editor.error.as_deref(),
            Some("overworld editing is disabled during file transfer")
        );
    }

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

    #[test]
    fn rectangle_cells_are_normalized_and_row_major() {
        assert_eq!(
            rectangle_cells((3, 2), (1, 1)),
            vec![(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2)]
        );
    }

    #[test]
    fn flood_fill_is_four_connected_bounded_and_row_major() {
        let tiles = [1, 1, 9, 1, 9, 2, 2, 2, 2];
        assert_eq!(
            flood_fill_cells(3, 3, &tiles, (0, 0)),
            vec![(0, 0), (1, 0), (0, 1)]
        );
        assert_eq!(flood_fill_cells(3, 3, &tiles, (2, 0)), vec![(2, 0)]);
        assert!(flood_fill_cells(3, 3, &tiles[..8], (0, 0)).is_empty());
        assert!(flood_fill_cells(3, 3, &tiles, (3, 0)).is_empty());
    }

    #[test]
    fn integrated_route_form_round_trips_every_native_field() {
        let link = OverworldPathLink {
            source: OverworldEndpoint {
                x: 0x1234,
                y: 0x5678,
                submap: 0x9a,
            },
            destination: OverworldEndpoint {
                x: 0xbcde,
                y: 0xf012,
                submap: 0x34,
            },
            target: OverworldPathTarget {
                x_tile: 0x56,
                y_tile: 0x78,
            },
        };
        let mut form = MainPathLinkForm {
            index: 3,
            ..Default::default()
        };
        form.set(link);
        assert_eq!(form.loaded, Some(3));
        assert_eq!(form.parse().unwrap(), link);
        form.target_x = "100".into();
        assert!(form.parse().is_err());
    }

    #[test]
    fn route_points_map_main_and_shared_submap_runtime_planes() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 512.0));
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(320.0, 40.0), 6),
            Some(OverworldEndpoint {
                x: 320,
                y: 40,
                submap: 0,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(511.0, 511.0), 6),
            Some(OverworldEndpoint {
                x: 504,
                y: 504,
                submap: 0,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(512.0, 40.0), 4),
            Some(OverworldEndpoint {
                x: 0,
                y: 40,
                submap: 4,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(512.0, 40.0), 0),
            None
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x140,
                y: 0x28,
                submap: 0,
            }),
            Some((0x140, 0x28))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x48,
                y: 0x10,
                submap: 1,
            }),
            Some((0x248, 0x10))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x200,
                y: 0x200,
                submap: 3,
            }),
            Some((0x200, 0))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0,
                y: 0,
                submap: 7,
            }),
            None
        );
    }
}
