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
    AppState, Command, NativeCustomOverworldSpriteController, NativeCustomOverworldSpriteEdit,
    OverworldController, OverworldControllerEdit, OverworldLayerId, ProfiledControllerSnapshot,
    SmwMainOverworldLayer2Controller,
};
use lm_graphics::{Palette, PaletteOwnership};
use lm_overworld::{
    OverworldEndpoint, OverworldPathDirection, OverworldPathLink, OverworldPathLinkTable,
    OverworldPathTarget,
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
    NativeSprites,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MapPaintTool {
    #[default]
    Select,
    Brush,
    Rectangle,
    Fill,
    NativeSprite,
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
    baseline_animation_options: [crate::overworld_editor_render::OverworldAnimationOptions; 7],
    native_appearances: Option<lm_render::NativeOverworldAppearancePair>,
    native_sprites: NativeCustomOverworldSpriteController,
    native_sprite_layout: lm_profile::SmwUsV1NativeCustomOverworldSpriteLayout,
}

struct NativeSpriteForm {
    map: usize,
    index: usize,
    id: String,
    x: String,
    y: String,
    screen: String,
    extra: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeSpriteDrag {
    map: usize,
    index: usize,
}

impl Default for NativeSpriteForm {
    fn default() -> Self {
        Self {
            map: 0,
            index: 0,
            id: "00".into(),
            x: "0000".into(),
            y: "0000".into(),
            screen: "00".into(),
            extra: String::new(),
        }
    }
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
    direction: OverworldPathDirection,
    one_way: bool,
    loaded: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum OverworldAnimationRate {
    Fps7_5,
    #[default]
    Fps15,
    Fps30,
    Fps60,
}

impl OverworldAnimationRate {
    const ALL: [Self; 4] = [Self::Fps7_5, Self::Fps15, Self::Fps30, Self::Fps60];

    const fn interval_seconds(self) -> f64 {
        match self {
            Self::Fps7_5 => 0.120,
            Self::Fps15 => 0.060,
            Self::Fps30 => 0.030,
            Self::Fps60 => 0.015,
        }
    }

    const fn substeps_per_tick(self) -> usize {
        match self {
            Self::Fps7_5 => 8,
            Self::Fps15 => 4,
            Self::Fps30 => 2,
            Self::Fps60 => 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Fps7_5 => "7.5 fps",
            Self::Fps15 => "15 fps",
            Self::Fps30 => "30 fps",
            Self::Fps60 => "60 fps",
        }
    }
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
    rendered_key: Option<(u64, usize, usize)>,
    texture: Option<egui::TextureHandle>,
    animation_graphics_texture: Option<egui::TextureHandle>,
    animation_preview_paused: bool,
    animation_preview_origin: Option<f64>,
    animation_preview_tick: usize,
    animation_preview_rate: OverworldAnimationRate,
    animation_preview_triggers: lm_graphics::ExAnimationTriggerPreviewState,
    animation_preview_events_passed: Vec<bool>,
    animation_preview_trigger_kind: usize,
    animation_preview_trigger_index: usize,
    animation_preview_event: usize,
    animation_option_map: usize,
    native_sprite: NativeSpriteForm,
    native_sprite_drag: Option<NativeSpriteDrag>,
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
    transfer_kind: Option<transfer::TransferKind>,
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
            let previous_direction = self.main_path.direction;
            ui.horizontal(|ui| {
                ui.label("Direction");
                egui::ComboBox::from_id_salt("playable-overworld-path-direction")
                    .selected_text(match self.main_path.direction {
                        OverworldPathDirection::Up => "Up",
                        OverworldPathDirection::Down => "Down",
                        OverworldPathDirection::Left => "Left",
                        OverworldPathDirection::Right => "Right",
                    })
                    .show_ui(ui, |ui| {
                        for (direction, label) in [
                            (OverworldPathDirection::Up, "Up"),
                            (OverworldPathDirection::Down, "Down"),
                            (OverworldPathDirection::Left, "Left"),
                            (OverworldPathDirection::Right, "Right"),
                        ] {
                            ui.selectable_value(&mut self.main_path.direction, direction, label);
                        }
                    });
                ui.checkbox(&mut self.main_path.one_way, "One-way (no return endpoint)");
            });
            if self.main_path.direction != previous_direction
                && let Err(error) = self.main_path.reorient_from(previous_direction)
            {
                self.main_path.direction = previous_direction;
                self.error = Some(error);
            }
            ui.small("Canvas route tools use Lunar Magic's Up, Down, Left, Right order and exact edge offsets.");
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
            self.update_animation_preview_clock(context);
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
        self.one_way = link.destination
            == (OverworldEndpoint {
                x: 0xffff,
                y: 0xffff,
                submap: 0xff,
            });
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
            destination: if self.one_way {
                OverworldEndpoint {
                    x: 0xffff,
                    y: 0xffff,
                    submap: 0xff,
                }
            } else {
                OverworldEndpoint {
                    x: level_editor_forms::parse_hex_u16(
                        &self.destination_x,
                        "route destination X",
                    )?,
                    y: level_editor_forms::parse_hex_u16(
                        &self.destination_y,
                        "route destination Y",
                    )?,
                    submap: level_editor_forms::parse_hex_u8(
                        &self.destination_submap,
                        "route destination submap",
                    )?,
                }
            },
            target: OverworldPathTarget {
                x_tile: level_editor_forms::parse_hex_u8(&self.target_x, "route target X")?,
                y_tile: level_editor_forms::parse_hex_u8(&self.target_y, "route target Y")?,
            },
        })
    }

    fn reorient_from(&mut self, previous: OverworldPathDirection) -> Result<(), String> {
        let source = OverworldEndpoint {
            x: level_editor_forms::parse_hex_u16(&self.source_x, "route source X")?,
            y: level_editor_forms::parse_hex_u16(&self.source_y, "route source Y")?,
            submap: level_editor_forms::parse_hex_u8(&self.source_submap, "route source submap")?,
        };
        let destination = if self.one_way {
            None
        } else {
            Some(OverworldEndpoint {
                x: level_editor_forms::parse_hex_u16(&self.destination_x, "route destination X")?,
                y: level_editor_forms::parse_hex_u16(&self.destination_y, "route destination Y")?,
                submap: level_editor_forms::parse_hex_u8(
                    &self.destination_submap,
                    "route destination submap",
                )?,
            })
        };
        let source = previous.reorient_directional_point(source, self.direction);
        self.source_x = format!("{:04X}", source.x);
        self.source_y = format!("{:04X}", source.y);
        self.source_submap = format!("{:02X}", source.submap);
        if let Some(destination) = destination {
            let destination = previous.reorient_directional_point(destination, self.direction);
            self.destination_x = format!("{:04X}", destination.x);
            self.destination_y = format!("{:04X}", destination.y);
            self.destination_submap = format!("{:02X}", destination.submap);
        }
        Ok(())
    }
}

fn path_form_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

impl RomOverworldEditor {
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let (
            stale,
            shape,
            slot,
            controller_revision,
            data,
            modes,
            ownership,
            animation_ownership,
            global_animation,
            native_summary,
        ) = {
            let workspace = self.workspace.as_ref()?;
            let map = usize::from(workspace.slot).min(6);
            (
                workspace.controller.revision() != revision,
                workspace.profiled.profile.overworld_shape,
                workspace.slot,
                workspace.controller.revision(),
                workspace.controller.data().clone(),
                workspace.profiled.profile.exanimation_double_size_modes,
                workspace.ownership.clone(),
                overworld_editor_render::overworld_animation_ownership(
                    &workspace.controller.data().animation,
                    workspace.assets.global_animation.as_ref(),
                    workspace.assets.animation_options[map],
                    workspace.assets.graphics.graphics.tiles.len(),
                    workspace.controller.data().palette.colors.len(),
                ),
                workspace.assets.global_animation.clone(),
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
        let previous_panel = self.panel;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Records, "Records");
            ui.selectable_value(&mut self.panel, Panel::Palette, "Palette");
            ui.selectable_value(&mut self.panel, Panel::Animation, "Animation");
            ui.selectable_value(&mut self.panel, Panel::NativeSprites, "Native sprites");
        });
        if previous_panel != self.panel && self.panel != Panel::NativeSprites {
            if self.paint_tool == MapPaintTool::NativeSprite {
                self.paint_tool = MapPaintTool::Select;
            }
        }
        let file = CompleteOverworldFile {
            source_slot: slot,
            shape,
            data,
        };
        let mut runtime_command = None;
        let edit = match self.panel {
            Panel::Records => self.records.show(ui, &file, controller_revision),
            Panel::Palette => {
                let edit = self.palette.show(
                    ui,
                    &file.data.palette,
                    &ownership,
                    &animation_ownership.palette,
                );
                if let Some(owner) = self.palette.take_navigation() {
                    self.panel = Panel::Animation;
                    self.animation.navigate(owner);
                }
                edit
            }
            Panel::Animation => {
                self.animation_preview_controls(ui, &file.data.animation);
                self.animation_file_controls(ui, stale, revision);
                runtime_command = self.animation_option_controls(ui, editing_blocked);
                self.animation_destination_controls(ui, &animation_ownership.graphics);
                self.animation.show(
                    ui,
                    &file.data.animation,
                    global_animation.as_ref(),
                    &modes,
                    controller_revision,
                )
            }
            Panel::NativeSprites => {
                self.native_sprite_controls(ui, editing_blocked);
                None
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
        if runtime_command.is_some() {
            return runtime_command;
        }
        self.commit_controls(ui, editing_blocked, revision)
    }

    fn native_sprite_controls(&mut self, ui: &mut egui::Ui, blocked: bool) {
        let counts = self.workspace.as_ref().map(|workspace| {
            std::array::from_fn::<_, 7, _>(|map| workspace.native_sprites.table().maps[map].len())
        });
        let Some(counts) = counts else { return };
        ui.heading("Native custom overworld sprite stream");
        ui.small("Seven map-local lists, variable record widths, and Lunar Magic's 24-sprite-per-map limit.");
        ui.add(egui::Slider::new(&mut self.native_sprite.map, 0..=6).text("Map"));
        let count = counts[self.native_sprite.map];
        ui.add(
            egui::Slider::new(&mut self.native_sprite.index, 0..=count)
                .text("Record / insertion index"),
        );
        egui::Grid::new("native-custom-overworld-sprite-form")
            .striped(true)
            .show(ui, |ui| {
                path_form_row(ui, "ID (hex)", &mut self.native_sprite.id);
                path_form_row(ui, "X pixels (hex)", &mut self.native_sprite.x);
                path_form_row(ui, "Y pixels (hex)", &mut self.native_sprite.y);
                path_form_row(ui, "Screen (hex)", &mut self.native_sprite.screen);
                path_form_row(ui, "Extension bytes (hex)", &mut self.native_sprite.extra);
            });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.native_sprite.index < count,
                    egui::Button::new("Load selected"),
                )
                .clicked()
            {
                self.load_native_sprite_form();
            }
            if ui.button("Use canvas selection").clicked() {
                match native_sprite_canvas_position(self.native_sprite.map, self.x, self.y) {
                    Some((x, y)) => {
                        self.native_sprite.x = format!("{x:04X}");
                        self.native_sprite.y = format!("{y:04X}");
                    }
                    None => {
                        self.error =
                            Some("the selected canvas cell is outside this map's plane".into())
                    }
                }
            }
        });
        if let Ok(id) = level_editor_forms::parse_hex_u8(&self.native_sprite.id, "native sprite ID")
            && let Some(required) = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.native_sprites.required_extra_len(id))
        {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "ID {id:02X} requires {required} extension byte(s)."
                ));
                if ui.button("Fill extension with zeroes").clicked() {
                    self.native_sprite.extra = std::iter::repeat_n("00", required)
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            });
        }
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!blocked, egui::Button::new("Insert"))
                .clicked()
            {
                edit = Some(
                    self.parse_native_sprite()
                        .map(|sprite| NativeCustomOverworldSpriteEdit::Insert {
                            map: self.native_sprite.map,
                            index: self.native_sprite.index,
                            sprite,
                        })
                        .map_err(|error| error.to_string()),
                );
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count,
                    egui::Button::new("Replace"),
                )
                .clicked()
            {
                edit = Some(
                    self.parse_native_sprite()
                        .map(|sprite| NativeCustomOverworldSpriteEdit::Replace {
                            map: self.native_sprite.map,
                            index: self.native_sprite.index,
                            sprite,
                        })
                        .map_err(|error| error.to_string()),
                );
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count,
                    egui::Button::new("Delete"),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::Remove {
                    map: self.native_sprite.map,
                    index: self.native_sprite.index,
                }));
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index > 0 && self.native_sprite.index < count,
                    egui::Button::new("Move up"),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::MoveBefore {
                    map: self.native_sprite.map,
                    from: self.native_sprite.index,
                    before: self.native_sprite.index - 1,
                }));
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count.saturating_sub(1),
                    egui::Button::new("Move down"),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::MoveBefore {
                    map: self.native_sprite.map,
                    from: self.native_sprite.index,
                    before: self.native_sprite.index + 2,
                }));
            }
        });
        if let Some(edit) = edit {
            match edit.and_then(|edit| {
                self.workspace
                    .as_mut()
                    .ok_or_else(|| String::from("workspace is closed"))?
                    .native_sprites
                    .apply_edits(&[edit])
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {
                    self.rendered_key = None;
                    self.texture = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.label(format!(
            "Map {}: {count}/24 records",
            self.native_sprite.map
        ));
    }

    fn load_native_sprite_form(&mut self) {
        let Some(sprite) = self.workspace.as_ref().and_then(|workspace| {
            workspace
                .native_sprites
                .table()
                .maps
                .get(self.native_sprite.map)?
                .get(self.native_sprite.index)
        }) else {
            return;
        };
        self.native_sprite.id = format!("{:02X}", sprite.id);
        self.native_sprite.x = format!("{:04X}", sprite.x);
        self.native_sprite.y = format!("{:04X}", sprite.y);
        self.native_sprite.screen = format!("{:02X}", sprite.screen);
        self.native_sprite.extra = sprite
            .extra
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
    }

    fn parse_native_sprite(&self) -> Result<lm_overworld::NativeCustomOverworldSprite, String> {
        let extra = self
            .native_sprite
            .extra
            .split(|character: char| character.is_ascii_whitespace() || character == ',')
            .filter(|value| !value.is_empty())
            .enumerate()
            .map(|(index, value)| {
                level_editor_forms::parse_hex_u8(value, &format!("extension byte {index}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(lm_overworld::NativeCustomOverworldSprite {
            id: level_editor_forms::parse_hex_u8(&self.native_sprite.id, "native sprite ID")?,
            x: level_editor_forms::parse_hex_u16(&self.native_sprite.x, "native sprite X")?,
            y: level_editor_forms::parse_hex_u16(&self.native_sprite.y, "native sprite Y")?,
            screen: level_editor_forms::parse_hex_u8(
                &self.native_sprite.screen,
                "native sprite screen",
            )?,
            extra,
        })
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

    fn animation_destination_controls(
        &mut self,
        ui: &mut egui::Ui,
        owners: &[Option<overworld_editor_render::OverworldAnimationOwner>],
    ) {
        ui.collapsing("Rendered graphics destinations", |ui| {
            ui.small(
                "Ctrl+Shift+click an attributed 8x8 tile to select its last-writing local or global ExAnimation record.",
            );
            let Some(texture) = self.animation_graphics_texture.clone() else {
                ui.label("The current animated graphics cache could not be rendered.");
                return;
            };
            let columns = 16;
            let rows = owners.len().div_ceil(columns);
            let native = texture.size_vec2();
            let width = (native.x * 2.0).min(ui.available_width()).max(native.x);
            let response = ui.add(
                egui::Image::new(&texture)
                    .fit_to_exact_size(egui::vec2(width, width * native.y / native.x))
                    .sense(egui::Sense::click()),
            );
            let pointed = response
                .hover_pos()
                .and_then(|position| {
                    overworld_editor_render::selected_tile(
                        response.rect,
                        position,
                        columns,
                        rows,
                    )
                })
                .and_then(|(x, y)| y.checked_mul(columns).and_then(|base| base.checked_add(x)));
            if let Some(index) = pointed {
                response.clone().on_hover_text(match owners.get(index).copied().flatten() {
                    Some(owner) => format!(
                        "Tile {index:03X}: {:?} ExAnimation record {:02X}",
                        owner.domain, owner.record
                    ),
                    None => format!("Tile {index:03X}: no ExAnimation owner"),
                });
                if response.clicked()
                    && let Some(owner) =
                        overworld_editor_render::ctrl_shift_animation_navigation(
                            ui.input(|input| input.modifiers),
                            owners.get(index).copied().flatten(),
                        )
                {
                    self.animation.navigate(owner);
                }
            }
        });
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
            if self.workspace.is_some() && self.panel == Panel::NativeSprites {
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::NativeSprite,
                    "Place/move native sprite",
                );
            }
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
        if self.paint_tool != MapPaintTool::NativeSprite {
            self.native_sprite_drag = None;
        }
        let mut action = None;
        let mut native_sprite_position = None;
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
                let canvas_pixel = overworld_editor_render::selected_tile(
                    response.rect,
                    position,
                    shape.width.saturating_mul(8),
                    shape.height.saturating_mul(8),
                )
                .unwrap_or((x.saturating_mul(8), y.saturating_mul(8)));
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
                    MapPaintTool::NativeSprite if !stale => {
                        if response.drag_started() {
                            if let Some(index) = self.native_sprite_hit_test(canvas_pixel) {
                                self.native_sprite.index = index;
                                self.load_native_sprite_form();
                                self.native_sprite_drag = Some(NativeSpriteDrag {
                                    map: self.native_sprite.map,
                                    index,
                                });
                            } else {
                                self.native_sprite_drag = None;
                            }
                        } else if response.drag_stopped() {
                            if let Some(drag) = self.native_sprite_drag.take() {
                                self.native_sprite.map = drag.map;
                                self.native_sprite.index = drag.index;
                                native_sprite_position = Some((x, y));
                            }
                        } else if response.clicked() {
                            if let Some(index) = self.native_sprite_hit_test(canvas_pixel) {
                                self.native_sprite.index = index;
                                self.load_native_sprite_form();
                            } else {
                                native_sprite_position = Some((x, y));
                            }
                        }
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
                            && let Some(endpoint) = route_directional_canvas_endpoint(
                                response.rect,
                                position,
                                submap,
                                self.main_path.direction,
                            )
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
                            && let Some(endpoint) = route_directional_canvas_endpoint(
                                response.rect,
                                position,
                                submap,
                                self.main_path.direction,
                            )
                        {
                            self.main_path.destination_x = format!("{:04X}", endpoint.x);
                            self.main_path.destination_y = format!("{:04X}", endpoint.y);
                            self.main_path.destination_submap = format!("{:02X}", endpoint.submap);
                            self.main_path.one_way = false;
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
                | MapPaintTool::NativeSprite
                | MapPaintTool::RouteSource
                | MapPaintTool::RouteDestination => {}
            }
        }
        if let Some(position) = native_sprite_position
            && let Err(error) = self.place_native_sprite_at_canvas(position)
        {
            self.error = Some(error);
        }
        if self.paint_tool == MapPaintTool::Brush && !ui.input(|input| input.pointer.primary_down())
        {
            self.paint_anchor = None;
        }
    }

    fn place_native_sprite_at_canvas(&mut self, position: (usize, usize)) -> Result<(), String> {
        let (x, y) = native_sprite_canvas_position(self.native_sprite.map, position.0, position.1)
            .ok_or("the clicked canvas cell is outside the selected native sprite map")?;
        self.native_sprite.x = format!("{x:04X}");
        self.native_sprite.y = format!("{y:04X}");
        let sprite = self.parse_native_sprite()?;
        let workspace = self.workspace.as_mut().ok_or("workspace is closed")?;
        let count = workspace.native_sprites.table().maps[self.native_sprite.map].len();
        let edit = native_sprite_canvas_edit(
            self.native_sprite.map,
            self.native_sprite.index,
            count,
            sprite,
            position,
        )?;
        workspace
            .native_sprites
            .apply_edits(&[edit])
            .map_err(|error| error.to_string())?;
        self.native_sprite.index = self.native_sprite.index.min(count);
        self.rendered_key = None;
        self.texture = None;
        Ok(())
    }

    fn native_sprite_hit_test(&self, point: (usize, usize)) -> Option<usize> {
        let workspace = self.workspace.as_ref()?;
        overworld_editor_render::native_custom_sprite_hit_test(
            workspace.native_appearances.as_ref(),
            workspace.native_sprites.table(),
            self.native_sprite.map,
            point,
        )
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
        let key = (
            workspace.controller.revision(),
            self.completed_reveals,
            self.animation_preview_tick
                .saturating_mul(self.animation_preview_rate.substeps_per_tick()),
        );
        if self.rendered_key == Some(key) {
            return;
        }
        let file = CompleteOverworldFile {
            source_slot: workspace.slot,
            shape: workspace.profiled.profile.overworld_shape,
            data: workspace.controller.data().clone(),
        };
        let preview = overworld_editor_render::OverworldExAnimationPreview {
            tick: self.animation_preview_tick,
            substeps_per_tick: self.animation_preview_rate.substeps_per_tick(),
            triggers: self.animation_preview_triggers.clone(),
            events_passed: self.animation_preview_events_passed.clone(),
        };
        match overworld_editor_render::render_texture_with_preview(
            context,
            &file,
            &workspace.assets,
            workspace.native_appearances.as_ref(),
            Some(workspace.native_sprites.table()),
            self.completed_reveals,
            Some(&preview),
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.animation_graphics_texture =
                    overworld_editor_render::render_exanimation_graphics_texture(
                        context,
                        &file,
                        &workspace.assets,
                        &preview,
                    )
                    .ok();
                self.rendered_key = Some(key);
            }
            Err(error) => {
                self.texture = None;
                self.animation_graphics_texture = None;
                self.rendered_key = Some(key);
                self.error = Some(format!("could not render native overworld: {error}"));
            }
        }
    }

    fn refresh_main_layer2_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.main_layer2_workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), 0, 0);
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
        let modified = self.workspace.as_ref().is_some_and(|value| {
            value.controller.is_modified()
                || value.native_sprites.is_modified()
                || value.assets.animation_options != value.baseline_animation_options
        });
        let payloads_modified = self
            .workspace
            .as_ref()
            .is_some_and(|value| value.controller.is_modified());
        let transfer_busy = self.transfer_busy();
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit all staged overworld changes"),
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
                payloads_modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
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

    fn animation_option_controls(
        &mut self,
        ui: &mut egui::Ui,
        editing_blocked: bool,
    ) -> Option<Command> {
        let Some(workspace) = self.workspace.as_mut() else {
            return None;
        };
        ui.separator();
        ui.heading("Per-map animation options");
        ui.add(
            egui::Slider::new(&mut self.animation_option_map, 0..=6)
                .text("Map (main, Yoshi, Vanilla, Forest, Valley, Special, Star)"),
        );
        let installed = workspace.assets.animation_options_runtime_installed;
        let layout_supported = workspace.assets.animation_options_layout_supported;
        let mut staged = workspace.controller.is_modified()
            || workspace.assets.animation_options != workspace.baseline_animation_options;
        let option = &mut workspace.assets.animation_options[self.animation_option_map];
        let before = *option;
        ui.add_enabled_ui(!editing_blocked && layout_supported, |ui| {
            ui.add_enabled_ui(installed, |ui| {
                for (label, feature) in [
                    (
                        "Original palette animation",
                        lm_graphics::ExAnimationFeature::PaletteAnimation,
                    ),
                    (
                        "Original animated tiles",
                        lm_graphics::ExAnimationFeature::VanillaAnimation,
                    ),
                    (
                        "Global ExAnimation",
                        lm_graphics::ExAnimationFeature::GlobalExAnimation,
                    ),
                    (
                        "This map's ExAnimation",
                        lm_graphics::ExAnimationFeature::LevelExAnimation,
                    ),
                ] {
                    let mut enabled = option.features.enabled(feature);
                    if ui.checkbox(&mut enabled, label).changed() {
                        option.features.set_enabled(feature, enabled);
                    }
                }
            });
            ui.checkbox(&mut option.original_lightning, "Original lightning");
        });
        if !layout_supported {
            ui.small("Per-map option operands are not authenticated for this ROM profile.");
        } else if !installed {
            ui.small(
                "The four feature switches require Lunar Magic's overworld animation runtime; original lightning is independently editable.",
            );
        }
        if *option != before {
            staged = true;
            self.rendered_key = None;
            self.texture = None;
        }
        if !installed
            && layout_supported
            && ui
                .add_enabled(
                    !editing_blocked && !staged,
                    egui::Button::new("Install overworld animation runtime"),
                )
                .on_hover_text(
                    "Install Lunar Magic's authenticated vanilla SMW-US runtime and seven-byte per-map option table as one undoable ROM transaction.",
                )
                .clicked()
        {
            match self.prepare_animation_runtime_install() {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        if !installed && staged {
            ui.small("Commit or discard staged changes before installing the runtime.");
        }
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
        self.reset_animation_preview();
    }

    fn update_animation_preview_clock(&mut self, context: &egui::Context) {
        if self.animation_preview_paused {
            return;
        }
        let seconds = context.input(|input| input.time);
        let origin = *self.animation_preview_origin.get_or_insert(seconds);
        self.animation_preview_tick =
            overworld_animation_preview_tick(seconds - origin, self.animation_preview_rate);
        context.request_repaint_after(std::time::Duration::from_secs_f64(
            self.animation_preview_rate.interval_seconds(),
        ));
    }

    fn reset_animation_preview(&mut self) {
        self.animation_preview_origin = None;
        self.animation_preview_tick = 0;
        self.animation_preview_triggers = lm_graphics::ExAnimationTriggerPreviewState::default();
        self.animation_preview_events_passed.clear();
        self.animation_preview_events_passed.resize(256, false);
        if let Some(animation) = self
            .workspace
            .as_ref()
            .map(|workspace| &workspace.controller.data().animation)
        {
            for index in 0..16 {
                if animation.trigger_mask & (1 << index) != 0 {
                    self.animation_preview_triggers.manual_frames[index] =
                        animation.trigger_values[index];
                    self.animation_preview_triggers.custom[index] =
                        animation.trigger_values[index] != 0;
                }
            }
        }
    }

    fn animation_preview_controls(
        &mut self,
        ui: &mut egui::Ui,
        animation: &lm_graphics::CompactExAnimation,
    ) {
        if self.animation_preview_events_passed.len() != 256 {
            self.animation_preview_events_passed.resize(256, false);
        }
        ui.group(|ui| {
            ui.label("Live overworld ExAnimation preview");
            ui.horizontal(|ui| {
                let label = if self.animation_preview_paused { "Play" } else { "Pause" };
                if ui.button(label).clicked() {
                    self.animation_preview_paused = !self.animation_preview_paused;
                    if self.animation_preview_paused {
                        self.animation_preview_origin = None;
                    } else {
                        let now = ui.input(|input| input.time);
                        self.animation_preview_origin = Some(
                            now - self.animation_preview_tick as f64
                                * self.animation_preview_rate.interval_seconds(),
                        );
                    }
                }
                if ui.button("Reset").clicked() {
                    self.reset_animation_preview();
                    self.rendered_key = None;
                }
                if ui
                    .add_enabled(self.animation_preview_paused, egui::Button::new("Step timer"))
                    .clicked()
                {
                    self.animation_preview_tick = self.animation_preview_tick.saturating_add(1);
                    self.rendered_key = None;
                }
                ui.monospace(format!(
                    "phase {:X}, tick {}",
                    self.animation_preview_tick
                        .saturating_mul(self.animation_preview_rate.substeps_per_tick())
                        & 7,
                    self.animation_preview_tick
                ));
                egui::ComboBox::from_id_salt("overworld-animation-preview-rate")
                    .selected_text(self.animation_preview_rate.label())
                    .show_ui(ui, |ui| {
                        for rate in OverworldAnimationRate::ALL {
                            if ui
                                .selectable_value(
                                    &mut self.animation_preview_rate,
                                    rate,
                                    rate.label(),
                                )
                                .changed()
                            {
                                self.animation_preview_origin = None;
                                self.animation_preview_tick = 0;
                                self.rendered_key = None;
                            }
                        }
                    });
            });
            ui.small(format!(
                "The selected native timer advances {} animation substep{} per callback.",
                self.animation_preview_rate.substeps_per_tick(),
                if self.animation_preview_rate.substeps_per_tick() == 1 {
                    ""
                } else {
                    "s"
                }
            ));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("overworld-preview-trigger-kind")
                    .selected_text(match self.animation_preview_trigger_kind {
                        0 => "Custom",
                        1 => "One Shot",
                        _ => "Manual Frame",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.animation_preview_trigger_kind, 0, "Custom");
                        ui.selectable_value(&mut self.animation_preview_trigger_kind, 1, "One Shot");
                        ui.selectable_value(&mut self.animation_preview_trigger_kind, 2, "Manual Frame");
                    });
                let maximum = if self.animation_preview_trigger_kind == 1 { 31 } else { 15 };
                self.animation_preview_trigger_index = self.animation_preview_trigger_index.min(maximum);
                ui.add(egui::DragValue::new(&mut self.animation_preview_trigger_index).range(0..=maximum).prefix("#"));
                let changed = match self.animation_preview_trigger_kind {
                    0 => ui.checkbox(
                        &mut self.animation_preview_triggers.custom[self.animation_preview_trigger_index],
                        "Active",
                    ).changed(),
                    1 => ui.checkbox(
                        &mut self.animation_preview_triggers.one_shot[self.animation_preview_trigger_index],
                        "Active",
                    ).changed(),
                    _ => ui.add(
                        egui::DragValue::new(
                            &mut self.animation_preview_triggers.manual_frames[self.animation_preview_trigger_index],
                        ).range(0..=u8::MAX).prefix("frame $"),
                    ).changed(),
                };
                if changed {
                    self.rendered_key = None;
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.animation_preview_event)
                        .range(0..=u8::MAX as usize)
                        .prefix("Event $"),
                );
                if ui
                    .checkbox(
                        &mut self.animation_preview_events_passed[self.animation_preview_event],
                        "Passed",
                    )
                    .changed()
                {
                    self.rendered_key = None;
                }
            });
            ui.small("Event Manual 8-F uses the event numbers stored by Trigger Init and these passed-event states.");
            if animation.records.is_empty() {
                ui.small("No custom overworld ExAnimation records are installed for this submap.");
            }
        });
    }
}

fn native_sprite_canvas_position(map: usize, x: usize, y: usize) -> Option<(u16, u16)> {
    if map >= 7 || y >= 64 {
        return None;
    }
    let local_x = if map == 0 {
        (x < 64).then_some(x)?
    } else if (64..128).contains(&x) {
        x - 64
    } else {
        return None;
    };
    Some((u16::try_from(local_x * 8).ok()?, u16::try_from(y * 8).ok()?))
}

fn native_sprite_canvas_edit(
    map: usize,
    selected: usize,
    count: usize,
    mut sprite: lm_overworld::NativeCustomOverworldSprite,
    position: (usize, usize),
) -> Result<NativeCustomOverworldSpriteEdit, String> {
    let (x, y) = native_sprite_canvas_position(map, position.0, position.1)
        .ok_or("the clicked canvas cell is outside the selected native sprite map")?;
    sprite.x = x;
    sprite.y = y;
    Ok(if selected < count {
        NativeCustomOverworldSpriteEdit::Replace {
            map,
            index: selected,
            sprite,
        }
    } else {
        NativeCustomOverworldSpriteEdit::Insert {
            map,
            index: count,
            sprite,
        }
    })
}

fn overworld_animation_preview_tick(seconds: f64, rate: OverworldAnimationRate) -> usize {
    if let Ok(tick) = std::env::var("LM_NATIVE_OVERWORLD_ANIMATION_TICK")
        && let Ok(tick) = tick.parse::<usize>()
    {
        return tick;
    }
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds / rate.interval_seconds()).floor() as u64;
    usize::try_from(ticks).unwrap_or(usize::MAX)
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

fn route_directional_canvas_endpoint(
    rect: egui::Rect,
    position: egui::Pos2,
    selected_submap: u8,
    direction: OverworldPathDirection,
) -> Option<OverworldEndpoint> {
    route_canvas_endpoint(rect, position, selected_submap)
        .map(|endpoint| direction.offset_directional_point(endpoint))
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
        MainPathLinkForm, NativeCustomOverworldSpriteEdit, OverworldAnimationRate,
        OverworldControllerEdit, OverworldEndpoint, OverworldLayerId, OverworldPathDirection,
        OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget, RomOverworldEditor,
        flood_fill_cells, grid_line, native_sprite_canvas_edit, native_sprite_canvas_position,
        overworld_animation_preview_tick, rectangle_cells, route_canvas_endpoint,
        route_directional_canvas_endpoint, route_endpoint_canvas_pixel, stroke_edits,
    };
    use crate::document_loader::BoundedRead;
    use crate::overworld_editor_render;
    use eframe::egui;

    #[test]
    fn native_sprite_form_preserves_variable_extensions_and_hex_coordinates() {
        let mut editor = RomOverworldEditor::default();
        editor.native_sprite.id = "2A".into();
        editor.native_sprite.x = "01F8".into();
        editor.native_sprite.y = "0100".into();
        editor.native_sprite.screen = "F8".into();
        editor.native_sprite.extra = "01, aB 7f".into();
        assert_eq!(
            editor.parse_native_sprite().unwrap(),
            lm_overworld::NativeCustomOverworldSprite {
                id: 0x2a,
                x: 0x1f8,
                y: 0x100,
                screen: 0xf8,
                extra: vec![1, 0xab, 0x7f],
            }
        );
    }

    #[test]
    fn native_sprite_canvas_selection_maps_main_and_shared_planes_locally() {
        assert_eq!(native_sprite_canvas_position(0, 63, 63), Some((504, 504)));
        assert_eq!(native_sprite_canvas_position(1, 64, 2), Some((0, 16)));
        assert_eq!(native_sprite_canvas_position(6, 127, 63), Some((504, 504)));
        assert_eq!(native_sprite_canvas_position(0, 64, 0), None);
        assert_eq!(native_sprite_canvas_position(2, 63, 0), None);
        assert_eq!(native_sprite_canvas_position(7, 64, 0), None);
    }

    #[test]
    fn native_sprite_canvas_tool_inserts_at_end_or_replaces_selected_record() {
        let sprite = lm_overworld::NativeCustomOverworldSprite {
            id: 3,
            x: 0,
            y: 0,
            screen: 8,
            extra: vec![0xaa],
        };
        assert!(matches!(
            native_sprite_canvas_edit(1, 4, 4, sprite.clone(), (70, 3)).unwrap(),
            NativeCustomOverworldSpriteEdit::Insert { map: 1, index: 4, sprite }
                if (sprite.x, sprite.y) == (48, 24)
        ));
        assert!(matches!(
            native_sprite_canvas_edit(0, 2, 4, sprite, (10, 5)).unwrap(),
            NativeCustomOverworldSpriteEdit::Replace { map: 0, index: 2, sprite }
                if (sprite.x, sprite.y) == (80, 40)
        ));
    }

    #[test]
    fn native_sprite_canvas_hit_test_selects_topmost_record_on_the_active_plane() {
        let sprite = |id, x, y| lm_overworld::NativeCustomOverworldSprite {
            id,
            x,
            y,
            screen: 0,
            extra: Vec::new(),
        };
        let table = lm_overworld::NativeCustomOverworldSpriteTable {
            maps: [
                vec![sprite(1, 80, 40), sprite(2, 80, 40)],
                vec![sprite(3, 48, 24)],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
        };

        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 0, (80, 40)),
            Some(1)
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 1, (560, 24)),
            Some(0)
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 0, (560, 24)),
            None
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 1, (80, 40)),
            None
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 7, (512, 0)),
            None
        );
    }

    #[test]
    fn overworld_animation_clock_uses_the_authenticated_native_preview_cadence() {
        assert_eq!(
            overworld_animation_preview_tick(f64::NAN, OverworldAnimationRate::Fps15),
            0
        );
        assert_eq!(
            overworld_animation_preview_tick(-1.0, OverworldAnimationRate::Fps15),
            0
        );
        assert_eq!(
            overworld_animation_preview_tick(0.0, OverworldAnimationRate::Fps15),
            0
        );
        for rate in OverworldAnimationRate::ALL {
            let interval = rate.interval_seconds();
            assert_eq!(overworld_animation_preview_tick(interval * 0.99, rate), 0);
            assert_eq!(overworld_animation_preview_tick(interval, rate), 1);
            let callbacks = 64 / rate.substeps_per_tick();
            assert_eq!(overworld_animation_preview_tick(0.960, rate), callbacks);
            assert_eq!(
                rate.substeps_per_tick() * overworld_animation_preview_tick(0.960, rate),
                64
            );
        }
    }

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

        form.set(OverworldPathLink {
            destination: OverworldEndpoint {
                x: 0xffff,
                y: 0xffff,
                submap: 0xff,
            },
            ..link
        });
        assert!(form.one_way);
        form.destination_x = "not parsed while one-way".into();
        assert_eq!(form.parse().unwrap().destination.x, 0xffff);
    }

    #[test]
    fn directional_route_canvas_clicks_apply_lunar_magic_edge_offsets() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 512.0));
        let position = egui::pos2(320.0, 40.0);
        let cases = [
            (OverworldPathDirection::Up, (328, 40)),
            (OverworldPathDirection::Down, (312, 40)),
            (OverworldPathDirection::Left, (320, 48)),
            (OverworldPathDirection::Right, (320, 32)),
        ];
        for (direction, (x, y)) in cases {
            assert_eq!(
                route_directional_canvas_endpoint(rect, position, 0, direction),
                Some(OverworldEndpoint { x, y, submap: 0 })
            );
        }
    }

    #[test]
    fn retained_lm363_left_to_up_one_way_route_transition_is_exact() {
        let fixture = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/overworld-path-direction/transition.tsv"
        );
        let fixture_field = |name: &str, column: usize| {
            fixture
                .lines()
                .find(|line| line.starts_with(name))
                .unwrap()
                .split('\t')
                .nth(column)
                .unwrap()
        };
        let record_hex = |column: usize| {
            fixture_field("interleaved_record_hex\t", column)
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(fixture_field("rejection_tile_type\t", 1), "00");
        assert_eq!(fixture_field("rejection_title\t", 1), "Wrong type of tile!");
        assert_eq!(
            fixture_field("rejection_body\t", 1),
            "An exit tile must be selected to use this."
        );
        assert_eq!(
            fixture_field("rejection_rom_sha256\t", 1),
            fixture_field("rejection_rom_sha256\t", 2)
        );
        let before = OverworldPathLink {
            source: OverworldEndpoint {
                x: 0x00d8,
                y: 0x00a0,
                submap: 0,
            },
            destination: OverworldEndpoint {
                x: 0x0058,
                y: 0x0150,
                submap: 2,
            },
            target: OverworldPathTarget {
                x_tile: 0x05,
                y_tile: 0x14,
            },
        };
        assert_eq!(
            &OverworldPathLinkTable {
                links: vec![before],
            }
            .encode_native_file()
            .unwrap()[12..],
            record_hex(1)
        );
        let mut form = MainPathLinkForm {
            index: 4,
            direction: OverworldPathDirection::Left,
            ..Default::default()
        };
        form.set(before);
        form.one_way = true;
        form.direction = OverworldPathDirection::Up;
        form.reorient_from(OverworldPathDirection::Left).unwrap();

        let after = form.parse().unwrap();
        assert_eq!(
            after,
            OverworldPathLink {
                source: OverworldEndpoint {
                    x: 0x00e0,
                    y: 0x0098,
                    submap: 0,
                },
                destination: OverworldEndpoint {
                    x: 0xffff,
                    y: 0xffff,
                    submap: 0xff,
                },
                target: before.target,
            }
        );
        assert_eq!(
            &OverworldPathLinkTable { links: vec![after] }
                .encode_native_file()
                .unwrap()[12..],
            record_hex(2)
        );
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
