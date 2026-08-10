use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, LevelController, NativeLevelEdit, RomExpansionCommand,
    VanillaEntranceController,
};
use lm_level::{
    CustomTimeError, CustomTimeSettings, Layer1VerticalScrollMode, LegacyHeaderEdit,
    NativeSpriteRecordFields, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord,
    SecondaryExitTable, SeparateMidwayEntrance, SpriteLengthTable, SpriteToken,
};
use lm_project::LevelSaveOptions;
use lm_project::{Project, VanillaMainEntrance};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SnesPointer24, SupportedGame};
use std::collections::HashMap;

use crate::user_toolbar_images::{
    MainToolbarImageSet, OriginalCatalogAction, OriginalTiledImage, OriginalToolbarImages,
    tiled_surface_canvas_size,
};

const ROM_LEVEL_CANVAS_CELL: f32 = 12.0;
const ROM_LEVEL_CANVAS_MIN_ZOOM: u16 = 100;
const ROM_LEVEL_CANVAS_MAX_ZOOM: u16 = 5_000;
const ROM_LEVEL_CANVAS_ZOOM_STEP: u16 = 100;
const ROM_LEVEL_CANVAS_INITIAL_PREVIOUS_ZOOM: u16 = 200;
const ROM_LEVEL_CANVAS_ZOOM_MENU: [u16; 9] = [100, 125, 150, 175, 200, 300, 400, 600, 800];
const CATALOG_PREVIEW_LOGICAL_SIDE: f32 = 256.0;
const CATALOG_PREVIEW_ZOOM_MENU: [u16; 6] = [100, 200, 300, 400, 600, 800];
const LUNAR_MAGIC_ANIMATION_TICK_SECONDS: f64 = 0.06;
const NATIVE_LEVEL_MINOR_TILES: u16 = 27;
const VERTICAL_LEVEL_MINOR_TILES: u16 = 32;
const VANILLA_EMPTY_MAP16_TILE: u16 = 0x25;
const VANILLA_ENTRANCE_Y_LOW: [u8; 16] = [
    0x00, 0x30, 0x60, 0x80, 0xa0, 0xb0, 0xc0, 0xe0, 0x10, 0x30, 0x50, 0x60, 0x70, 0x90, 0x00, 0x00,
];
const VANILLA_ENTRANCE_Y_HIGH: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
];
const VANILLA_ENTRANCE_X_LOW: [u8; 8] = [0x10, 0x80, 0x00, 0xe0, 0x10, 0x70, 0x00, 0xe0];
const VANILLA_VERTICAL_ENTRANCE_X_HIGH: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01];
const VANILLA_ALTERNATE_VERTICAL_ENTRANCE_X_HIGH: [u8; 8] = [0; 8];
const VANILLA_LAYER2_VERTICAL_SCROLL: [u8; 16] = [3, 1, 1, 0, 0, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0];
const VANILLA_LAYER2_HORIZONTAL_SCROLL: [u8; 16] = [2, 2, 1, 0, 1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const VANILLA_INITIAL_LAYER1_Y: [u8; 4] = [0x00, 0x60, 0xc0, 0x00];
const VANILLA_INITIAL_LAYER2_Y: [u8; 4] = [0x60, 0x90, 0xc0, 0x00];
// At the default desktop window size this leaves the one-screen canvas pane close to 256:224,
// minimizing its centered side bezels while retaining a useful, fixed-width editing column.
const ROM_LEVEL_TOOL_PANEL_WIDTH: f32 = 530.0;
const STANDARD_SPRITE_MAX: u8 = 0xed;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LevelToolPanel {
    Settings,
    Layer2,
    Sprites,
    ScreenExits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EntranceOverlayToggle {
    All,
    Primary,
    Secondary,
    Midway,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EntranceOverlayVisibility {
    all: bool,
    primary: bool,
    secondary: bool,
    midway: bool,
}

impl Default for EntranceOverlayVisibility {
    fn default() -> Self {
        Self {
            all: true,
            primary: true,
            secondary: true,
            midway: true,
        }
    }
}

impl LevelToolPanel {
    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    level: u16,
    sprite_lengths_signature: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct HeaderForm {
    background_palette: u8,
    last_screen: u8,
    level_mode: u8,
    background_color: u8,
    sprite_tileset: u8,
    default_music_selector: u8,
    time_limit_selector: u8,
    custom_time_enabled: bool,
    custom_time_value: u16,
    force_time_reset: bool,
    sprite_palette: u8,
    foreground_palette: u8,
    object_tileset: u8,
    layer1_vertical_scroll: u8,
}

impl HeaderForm {
    fn from_controller(controller: &LevelController) -> Self {
        let header = controller.level().layer1.header;
        let vertical = lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical;
        let custom_time = controller.level().layer1.objects.custom_time(vertical);
        Self {
            background_palette: header.background_palette(),
            last_screen: header.last_screen(),
            level_mode: header.level_mode(),
            background_color: header.background_color(),
            sprite_tileset: header.sprite_tileset(),
            default_music_selector: header.default_music_selector(),
            time_limit_selector: header.time_limit_selector(),
            custom_time_enabled: custom_time.is_some(),
            custom_time_value: custom_time.map_or(300, CustomTimeSettings::value),
            force_time_reset: custom_time.is_some_and(CustomTimeSettings::force_reset),
            sprite_palette: header.sprite_palette(),
            foreground_palette: header.foreground_palette(),
            object_tileset: header.object_tileset(),
            layer1_vertical_scroll: header.layer1_vertical_scroll().raw(),
        }
    }

    fn edits(self) -> Result<Vec<NativeLevelEdit>, CustomTimeError> {
        let custom_time = self
            .custom_time_enabled
            .then(|| CustomTimeSettings::new(self.custom_time_value, self.force_time_reset))
            .transpose()?;
        Ok(vec![
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(
                self.background_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(self.last_screen)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(self.level_mode)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(self.background_color)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(self.sprite_tileset)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(
                self.default_music_selector,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::TimeLimitSelector(
                self.time_limit_selector,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(self.sprite_palette)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(
                self.foreground_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(self.object_tileset)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::Layer1VerticalScroll(
                Layer1VerticalScrollMode::from_raw(self.layer1_vertical_scroll),
            )),
            NativeLevelEdit::SetCustomTime(custom_time),
        ])
    }
}

#[derive(Clone, Debug, Default)]
struct ObjectForm {
    encoded: String,
    command_id: u8,
    parameter: u8,
    first_coordinate: u8,
    second_coordinate: u8,
    advances_screen: bool,
    screen_jump: Option<(lm_level::ScreenJumpEncoding, u16)>,
    screen_exit: Option<(u8, u16)>,
    extended_command27_size: Option<(u8, u8)>,
}

#[derive(Clone, Debug, Default)]
struct SpriteForm {
    header: u8,
    sprite_memory: u8,
    sprite_buoyancy_1: bool,
    sprite_buoyancy_2: bool,
    encoded: String,
    y_low: u8,
    extra_bits: u8,
    screen: u8,
    x: u8,
    sprite_number: u8,
    semantic_record: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntityPasteTarget {
    Object,
    Layer2Object,
    Sprite,
    DirectMap16Rectangle {
        key: EditorKey,
        controller_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanvasPlacementMode {
    Object,
    Sprite,
    Layer2Object,
    Layer2Tile,
}

const fn placement_mode_visible(
    mode: CanvasPlacementMode,
    visibility: crate::application::LevelViewVisibility,
) -> bool {
    match mode {
        CanvasPlacementMode::Object => visibility.layer1,
        CanvasPlacementMode::Sprite => visibility.sprites,
        CanvasPlacementMode::Layer2Object | CanvasPlacementMode::Layer2Tile => visibility.layer2,
    }
}

impl SpriteForm {
    fn from_token(header: u8, token: Option<&SpriteToken>) -> Self {
        let semantic_header = lm_level::NativeSpriteHeader::from_raw(header);
        let encoded = match token {
            Some(SpriteToken::Record(record)) => record
                .encoded
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            Some(SpriteToken::Screen(value)) => format!("yhigh {value:02X}"),
            Some(SpriteToken::Control(value)) => format!("control {value:02X}"),
            None => String::new(),
        };
        let fields = token
            .and_then(|token| match token {
                SpriteToken::Record(record) => record.native_fields().ok(),
                SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
            })
            .unwrap_or(NativeSpriteRecordFields {
                y_low: 0,
                extra_bits: 0,
                screen: 0,
                x: 0,
                sprite_number: 0,
            });
        Self {
            header,
            sprite_memory: semantic_header.memory(),
            sprite_buoyancy_1: semantic_header.buoyancy_1(),
            sprite_buoyancy_2: semantic_header.buoyancy_2(),
            encoded,
            y_low: fields.y_low,
            extra_bits: fields.extra_bits,
            screen: fields.screen,
            x: fields.x,
            sprite_number: fields.sprite_number,
            semantic_record: token.is_some_and(|token| matches!(token, SpriteToken::Record(_))),
        }
    }

    fn semantic_header(&self) -> Result<u8, String> {
        lm_level::NativeSpriteHeader::from_raw(self.header)
            .with_properties(
                self.sprite_memory,
                self.sprite_buoyancy_1,
                self.sprite_buoyancy_2,
            )
            .map(lm_level::NativeSpriteHeader::raw)
            .map_err(|error| error.to_string())
    }

    fn semantic_edit(
        &self,
        index: usize,
        token: Option<&SpriteToken>,
        _lengths: &SpriteLengthTable,
    ) -> Result<NativeLevelEdit, String> {
        let Some(SpriteToken::Record(_)) = token else {
            return Err("select a sprite record before applying semantic fields".into());
        };
        Ok(NativeLevelEdit::SetSpriteFields {
            index,
            fields: NativeSpriteRecordFields {
                y_low: self.y_low,
                extra_bits: self.extra_bits,
                screen: self.screen,
                x: self.x,
                sprite_number: self.sprite_number,
            },
        })
    }
}

impl ObjectForm {
    fn from_record(record: &ObjectRecord) -> Self {
        let coordinates = record.coordinate_nibbles();
        Self {
            encoded: crate::level_editor_forms::format_bytes(record.encoded()),
            command_id: record.command_id(),
            parameter: record.parameter(),
            first_coordinate: coordinates.first,
            second_coordinate: coordinates.second,
            advances_screen: record.advances_screen(),
            screen_jump: record
                .screen_jump()
                .map(|jump| (jump.encoding, jump.packed_target)),
            screen_exit: record
                .screen_exit()
                .map(|exit| (exit.screen, exit.destination_and_flags)),
            extended_command27_size: record.extended_command27_tile_size(),
        }
    }

    fn ordinary_record(&self) -> Result<ObjectRecord, String> {
        if self.command_id > 0x3f || self.first_coordinate > 0x0f || self.second_coordinate > 0x0f {
            return Err("object command or coordinate is out of range".into());
        }
        let first = self.first_coordinate
            | ((self.command_id & 0x30) << 1)
            | if self.advances_screen { 0x80 } else { 0 };
        let second = self.second_coordinate | ((self.command_id & 0x0f) << 4);
        ObjectRecord::new(vec![first, second, self.parameter]).map_err(|error| error.to_string())
    }

    fn raw_record(&self) -> Result<ObjectRecord, String> {
        let record = crate::level_editor_forms::parse_object(&self.encoded)?;
        let expected = lm_level::encoded_record_length(record.encoded())
            .ok_or_else(|| "raw object record has an incomplete native header".to_owned())?;
        if expected != record.encoded().len() {
            return Err(format!(
                "raw object record has {} bytes, but its native command encoding requires {expected}",
                record.encoded().len()
            ));
        }
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanvasEntitySelection {
    Layer1Object,
    Layer2Object,
    Sprite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanvasEntityShortcut {
    SelectAll,
    Insert,
    Duplicate,
    Remove,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ZOrderTraversal {
    Forward,
    Backward,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CanvasObjectGroupDrag {
    domain: CanvasEntitySelection,
    origin_major: i32,
    origin_minor: i32,
    secondary: bool,
}

#[derive(Clone, Copy, Debug)]
struct LevelCanvasGeometry {
    rect: egui::Rect,
    cell: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
}

#[derive(Default)]
pub(crate) struct VanillaLevelEditor {
    key: Option<EditorKey>,
    controller: Option<LevelController>,
    pending_expansion_commit: Option<LevelController>,
    entrance_controller: Option<VanillaEntranceController>,
    secondary_exits: Option<SecondaryExitTable>,
    secondary_exit_references: Option<Vec<bool>>,
    secondary_exits_revision: Option<u64>,
    secondary_exits_error: Option<String>,
    entrance_form: VanillaMainEntrance,
    midway_form: Option<SeparateMidwayEntrance>,
    midway_install_form: SeparateMidwayEntrance,
    entrance_overlay_visibility: EntranceOverlayVisibility,
    form: HeaderForm,
    selected_object: usize,
    selected_object_group: Vec<usize>,
    object_form: ObjectForm,
    dragging_object: Option<usize>,
    dragging_layer2_object: Option<usize>,
    resizing_object: Option<usize>,
    resizing_layer2_object: Option<usize>,
    object_catalog_filter: String,
    extended_object_catalog_filter: String,
    custom_object_catalog_filter: String,
    object_catalog_preview_icons: Option<bool>,
    object_catalog_compatible_only: Option<bool>,
    object_catalog_vertical_layout: Option<bool>,
    object_catalog_preview_area: Option<bool>,
    object_catalog_preview_zoom: Option<u16>,
    object_catalog_preview_selector: Option<lm_level::OscObjectSelector>,
    object_placement_template: Option<ObjectRecord>,
    selected_sprite: usize,
    selected_sprite_group: Vec<usize>,
    sprite_form: SpriteForm,
    dragging_sprite: Option<usize>,
    secondary_duplicate_drag: bool,
    object_group_drag: Option<CanvasObjectGroupDrag>,
    sprite_catalog_filter: String,
    custom_sprite_catalog_filter: String,
    sprite_catalog_preview_icons: Option<bool>,
    sprite_catalog_compatible_only: Option<bool>,
    sprite_catalog_vertical_layout: Option<bool>,
    sprite_catalog_preview_area: Option<bool>,
    sprite_catalog_preview_zoom: Option<u16>,
    sprite_catalog_preview_selector: Option<lm_level::SscSpriteSelector>,
    canvas_zoom_percent: Option<u16>,
    canvas_previous_zoom_percent: Option<u16>,
    zoom_filter: Option<bool>,
    zoom_popup_open: bool,
    animation_playing: Option<bool>,
    animation_last_wall_seconds: f64,
    animation_time_offset_seconds: f64,
    animation_frozen_seconds: f64,
    switch_view_state: lm_render::LunarMagicSwitchViewState,
    conditional_view_state: lm_render::LunarMagicConditionalViewState,
    exanimation_trigger_view_state: ExAnimationTriggerViewState,
    blue_pow_active: bool,
    silver_pow_active: bool,
    background_512_height: bool,
    translucent_overlays: bool,
    tools_panel_visible: Option<bool>,
    tool_panel_generations: [u64; 4],
    requested_tool_panel: Option<LevelToolPanel>,
    screen_exit_table_form: Option<[Option<u16>; 32]>,
    screen_exit_table_selected: Option<u8>,
    canvas_geometry: Option<LevelCanvasGeometry>,
    game_preview: Option<bool>,
    snes_viewport: Option<bool>,
    draw_selection_over_live: Option<bool>,
    preview_camera_major_offset: i16,
    preview_camera_minor_offset: i16,
    initial_vertical_scroll_tiles: Option<u16>,
    placement_mode: Option<CanvasPlacementMode>,
    canvas_entity_selection: Option<CanvasEntitySelection>,
    paste_target: Option<EntityPasteTarget>,
    pending_layer2_mode_reset: Option<HeaderForm>,
    error: Option<String>,
    map16_key: Option<(
        u64,
        u16,
        u8,
        u8,
        bool,
        bool,
        bool,
        bool,
        lm_render::LunarMagicConditionalViewState,
    )>,
    map16_texture: Option<egui::TextureHandle>,
    outline_texture: Option<egui::TextureHandle>,
    layer2_map16_texture: Option<egui::TextureHandle>,
    background_map16_texture: Option<egui::TextureHandle>,
    animated_map16_textures: Vec<egui::TextureHandle>,
    block_contents_textures: Vec<egui::TextureHandle>,
    animated_layer2_map16_textures: Vec<egui::TextureHandle>,
    animated_background_map16_textures: Vec<egui::TextureHandle>,
    animated_background_plane_textures: Vec<egui::TextureHandle>,
    shared_vanilla_background: bool,
    sprite_texture: Option<egui::TextureHandle>,
    animated_sprite_textures: Vec<egui::TextureHandle>,
    entrance_texture: Option<egui::TextureHandle>,
    sprite_tiles: Vec<lm_graphics::IndexedTile>,
    foreground_tiles: Vec<lm_graphics::IndexedTile>,
    layer3_tiles: Vec<lm_graphics::IndexedTile>,
    layer3_low_texture: Option<egui::TextureHandle>,
    layer3_high_texture: Option<egui::TextureHandle>,
    layer3_position: Option<(i16, i16)>,
    layer3_editor_row_offset: Option<i16>,
    layer3_between_background_and_foreground: bool,
    sprite_palette: Option<lm_graphics::Palette>,
    canvas_backdrop: Option<lm_graphics::Bgr555>,
    foreground_texture: Option<egui::TextureHandle>,
    map16_summary: Option<Map16Summary>,
    map16_error: Option<String>,
    standard_object_map: Option<lm_profile::SmwUsV1StandardObjectDefinitionMap>,
    selected_layer2_tile: usize,
    layer2_word: u16,
    selected_layer2_object: usize,
    selected_layer2_object_group: Vec<usize>,
    layer2_object_form: ObjectForm,
    layer2_object_placement_template: Option<ObjectRecord>,
    external_asset_revision: u64,
    external_sprite_textures:
        HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    layer1_z_order_bounds: HashMap<usize, egui::Rect>,
    layer2_z_order_bounds: HashMap<usize, egui::Rect>,
    sprite_z_order_bounds: HashMap<usize, egui::Rect>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExAnimationTriggerViewState {
    custom: [bool; 16],
    one_shot: [bool; 32],
    manual_frames: [u8; 16],
    selected_custom: u8,
    selected_one_shot: u8,
    selected_manual: u8,
}

impl ExAnimationTriggerViewState {
    fn select_custom(&mut self, delta: i8) {
        self.selected_custom = wrapping_index(self.selected_custom, delta, 16);
    }

    fn select_one_shot(&mut self, delta: i8) {
        self.selected_one_shot = wrapping_index(self.selected_one_shot, delta, 32);
    }

    fn select_manual(&mut self, delta: i8) {
        self.selected_manual = wrapping_index(self.selected_manual, delta, 16);
    }
}

fn wrapping_index(value: u8, delta: i8, modulus: u8) -> u8 {
    (i16::from(value) + i16::from(delta)).rem_euclid(i16::from(modulus)) as u8
}

impl VanillaLevelEditor {
    pub(crate) fn invalidate_graphics_preview(&mut self) {
        self.map16_key = None;
    }

    pub(crate) fn foreground_texture(&self) -> Option<&egui::TextureHandle> {
        self.foreground_texture.as_ref()
    }

    pub(crate) fn sprite_texture(&self) -> Option<&egui::TextureHandle> {
        self.sprite_texture.as_ref()
    }

    pub(crate) fn handles(app: &AppState) -> bool {
        app.revision_profile().is_none()
            && app.controller_snapshot().is_ok_and(|snapshot| {
                matches!(snapshot.mode, EditorMode::Level(_)) && is_supported(&snapshot)
            })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        app: &AppState,
        special_world_passed: bool,
        visibility: crate::application::LevelViewVisibility,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        external_asset_revision: u64,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        live_frame: Option<(egui::TextureId, [usize; 2])>,
        toolbar_images: &MainToolbarImageSet,
    ) -> Option<Command> {
        let snapshot = app.controller_snapshot().ok()?;
        let EditorMode::Level(level) = snapshot.mode else {
            self.clear();
            return None;
        };
        if !is_supported(&snapshot) || app.revision_profile().is_some() {
            self.clear();
            return None;
        }
        let key = EditorKey {
            revision: snapshot.revision,
            level,
            sprite_lengths_signature: ssc_sprite_lengths_signature(custom_sprites),
        };
        match self.take_pending_expansion_commit(&snapshot) {
            Ok(Some(command)) => return Some(command),
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
        if self.key != Some(key) {
            self.load(&snapshot, key, custom_sprites);
        }
        if self.external_asset_revision != external_asset_revision {
            self.external_asset_revision = external_asset_revision;
            self.external_sprite_textures.clear();
        }

        self.show_layer2_mode_reset_confirmation(ui.ctx());

        ui.heading(format!("Level {level:03X} — built-in SMW editor"));
        if self.outline_texture.is_none() {
            match crate::level_outline::atlas_image() {
                Ok(image) => {
                    self.outline_texture = Some(ui.ctx().load_texture(
                        "lunar-magic-level-outline-atlas",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.show_zoom_popup(ui.ctx());
        let Some(controller) = self.controller.as_ref() else {
            ui.colored_label(
                egui::Color32::RED,
                self.error.as_deref().unwrap_or("load failed"),
            );
            return None;
        };
        ui.label("Pristine SMW-US layout detected automatically.");
        if let Some(mode) = controller.normalized_reserved_level_mode() {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "Mode ${mode:02X} is reserved. Lunar Magic compatibility uses mode $00 instead."
                ),
            );
        }
        ui.separator();
        let object_count = controller.level().layer1.objects.records.len();
        let sprite_count = controller.level().sprites.tokens.len();
        let object_tileset = controller.level().layer1.header.object_tileset();
        let object_family = lm_profile::smw_us_v1_object_family(object_tileset);
        self.ensure_map16_assets(ui.ctx(), &snapshot, object_tileset, special_world_passed);
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} standard-object definitions (tileset {object_tileset:X})",
                object_family.display_name()
            ));
            let tools_visible = self.tools_panel_visible();
            if ui
                .button(if tools_visible {
                    "Hide tools"
                } else {
                    "Show tools"
                })
                .clicked()
            {
                self.tools_panel_visible = Some(!tools_visible);
            }
        });
        let workspace_size = ui.available_size();
        let tool_width = workspace_tool_width(workspace_size.x);
        let requested_tool_panel = self.requested_tool_panel.take();
        let mut pending_command = None;
        ui.horizontal_top(|ui| {
            if self.tools_panel_visible() {
                // `allocate_ui_with_layout` permits children to enlarge its requested rectangle.
                // Reserve an exact parent slot first, then paint a clipped child into it so no
                // expanded catalog can ever alter the canvas allocation beside it.
                let (tool_rect, _) = ui.allocate_exact_size(
                    egui::vec2(tool_width, workspace_size.y),
                    egui::Sense::hover(),
                );
                let mut tool_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("vanilla-level-tool-panel-fixed")
                        .max_rect(tool_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                tool_ui.set_width(tool_width);
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-level-tool-panel")
                    .max_width(tool_width)
                    .auto_shrink([false, false])
                    .show(&mut tool_ui, |ui| {
                        self.show_staged_history(ui);
                        egui::CollapsingHeader::new("Level and entrance settings")
                            .id_salt((
                                "vanilla-level-settings",
                                self.tool_panel_generations[LevelToolPanel::Settings.index()],
                            ))
                            .default_open(requested_tool_panel == Some(LevelToolPanel::Settings))
                            .show(ui, |ui| {
                                self.show_header_editor(ui, object_count, sprite_count);
                                if pending_command.is_none() {
                                    pending_command = self.show_entrance_editor(ui, level);
                                }
                            });
                        egui::CollapsingHeader::new("Screen exits")
                            .id_salt((
                                "vanilla-screen-exit-table",
                                self.tool_panel_generations[LevelToolPanel::ScreenExits.index()],
                            ))
                            .default_open(requested_tool_panel == Some(LevelToolPanel::ScreenExits))
                            .show(ui, |ui| self.show_screen_exit_table_editor(ui));
                        self.show_layer2_editor(
                            ui,
                            custom_objects,
                            custom_map16,
                            toolbar_images,
                            requested_tool_panel == Some(LevelToolPanel::Layer2),
                        );
                        egui::CollapsingHeader::new("Layer 1 objects")
                            .id_salt("vanilla-layer1-tools")
                            .default_open(true)
                            .show(ui, |ui| {
                                self.object_list(ui);
                                self.object_editor(
                                    ui,
                                    custom_objects,
                                    custom_map16,
                                    toolbar_images,
                                );
                            });
                        egui::CollapsingHeader::new("Enemies and sprites")
                            .id_salt((
                                "vanilla-sprite-tools",
                                self.tool_panel_generations[LevelToolPanel::Sprites.index()],
                            ))
                            .default_open(requested_tool_panel == Some(LevelToolPanel::Sprites))
                            .show(ui, |ui| {
                                self.sprite_list(ui);
                                self.sprite_editor(
                                    ui,
                                    custom_sprites,
                                    external_assets,
                                    custom_map16,
                                    toolbar_images,
                                );
                            });
                        self.show_map16_preview(ui, object_tileset);
                        if pending_command.is_none() {
                            pending_command = self.show_commit_controls(ui, &snapshot);
                        }
                    });
                ui.separator();
            }
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), workspace_size.y),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    self.object_canvas(
                        ui,
                        visibility,
                        custom_sprites,
                        external_assets,
                        custom_objects,
                        custom_map16,
                        live_frame,
                        toolbar_images,
                    );
                },
            );
        });
        pending_command
    }

    /// Reports the exact built-in editor commit shape used by Lunar Magic's optimized LMSW path.
    pub(crate) fn has_sprite_only_changes(&self) -> bool {
        self.controller.as_ref().is_some_and(|controller| {
            controller.sprites_are_modified()
                && !controller.layer1_is_modified()
                && !controller.layer2_is_modified()
        })
    }

    /// Serializes the staged sprite stream exactly as LMSW receives it: the one-byte level sprite
    /// header is omitted, while legacy/expanded framing and terminators remain intact.
    pub(crate) fn lmsw_sprite_payload(&self) -> Result<Vec<u8>, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| "the built-in level editor is not loaded".to_string())?;
        let encoded = controller
            .level()
            .sprites
            .encode_for_table(controller.sprite_lengths())
            .map_err(|error| error.to_string())?;
        encoded
            .get(1..)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| "serialized sprite stream omitted its header".to_string())
    }

    fn tools_panel_visible(&self) -> bool {
        self.tools_panel_visible.unwrap_or(true)
    }

    fn game_preview(&self) -> bool {
        self.game_preview.unwrap_or_else(|| {
            std::env::var("LM_NATIVE_PREVIEW_STYLE")
                .map_or(true, |style| !style.eq_ignore_ascii_case("editor"))
        })
    }

    fn snes_viewport(&self) -> bool {
        self.snes_viewport.unwrap_or(true)
    }

    fn draw_selection_over_live(&self) -> bool {
        self.draw_selection_over_live.unwrap_or(true)
    }

    fn show_commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &lm_app::ControllerSnapshot,
    ) -> Option<Command> {
        ui.separator();
        let expanded = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .is_ok_and(|image| image.logical_len() > 0x80_000);
        let relocation_needed = self.controller.as_ref().is_some_and(|controller| {
            controller.layer1_is_modified() || controller.layer2_is_modified()
        });
        if !expanded && relocation_needed {
            ui.label("Layer 1/2 relocation needs one expanded free-space bank.");
            if ui.button("Expand ROM to 1 MiB").clicked() {
                self.pending_expansion_commit = self.controller.clone();
                return Some(Command::ExpandRom(RomExpansionCommand {
                    expected_revision: snapshot.revision,
                    mapper: snapshot.identity.mapper,
                    target_logical_len: 0x10_0000,
                    fill: 0xff,
                    checksum_field: snapshot.identity.internal_header_offset + 0x1c,
                }));
            }
        }
        if ui
            .add_enabled(
                (expanded || !relocation_needed)
                    && self
                        .controller
                        .as_ref()
                        .is_some_and(LevelController::is_modified),
                egui::Button::new("Commit level changes to ROM"),
            )
            .clicked()
        {
            return match prepare_commit(
                self.controller
                    .as_ref()
                    .expect("controller presence checked above"),
                snapshot,
            ) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
        None
    }

    fn show_layer2_editor(
        &mut self,
        ui: &mut egui::Ui,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        toolbar_images: &MainToolbarImageSet,
        requested_open: bool,
    ) {
        let Some(layer2) = self
            .controller
            .as_ref()
            .and_then(LevelController::layer2)
            .cloned()
        else {
            return;
        };
        egui::CollapsingHeader::new("Layer 2")
            .id_salt((
                "vanilla-layer2-tools",
                self.tool_panel_generations[LevelToolPanel::Layer2.index()],
            ))
            .default_open(requested_open)
            .show(ui, |ui| match &layer2 {
                lm_level::NativeLayer2Data::Tilemap(bytes) => {
                    let count = bytes.len() / 2;
                    self.selected_layer2_tile =
                        self.selected_layer2_tile.min(count.saturating_sub(1));
                    ui.label(format!(
                        "Compressed 32×32 background tilemap · selected storage word {}",
                        self.selected_layer2_tile
                    ));
                    if layer2_tilemap_editable(self.shared_vanilla_background) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Map16 word");
                            ui.add(
                                egui::DragValue::new(&mut self.layer2_word)
                                    .hexadecimal(4, false, true),
                            );
                            if ui.button("Stage selected tile").clicked() {
                                let result = self
                                    .controller
                                    .as_mut()
                                    .expect("controller presence checked above")
                                    .apply_layer2_tilemap_words(&[(
                                        self.selected_layer2_tile,
                                        self.layer2_word,
                                    )]);
                                match result {
                                    Ok(()) => self.error = None,
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
                        });
                        ui.small(
                            "Choose “Paint Layer 2 tile” and click the canvas to write this word. \
                         Selection follows Lunar Magic's column-major two-plane storage.",
                        );
                    } else {
                        ui.small(
                        "This is a shared pristine SMW background. It remains read-only until the \
                         format-$103 Layer 2 runtime can be installed copy-on-write; editing the \
                         shared bank-$0C payload directly would change every level that uses it.",
                    );
                    }
                    self.show_layer2_tilemap_canvas(ui, bytes, toolbar_images);
                }
                lm_level::NativeLayer2Data::Objects(objects) => {
                    ui.label(format!(
                        "{} native Layer 2 object records are decoded and rendered.",
                        objects.objects.records.len()
                    ));
                    self.object_catalog(ui, custom_map16, true);
                    self.extended_object_catalog(ui, custom_map16, true);
                    self.custom_object_catalog(ui, custom_objects, custom_map16, true);
                    self.object_catalog_preview_area(ui, custom_objects, custom_map16, true);
                    self.show_layer2_object_editor(ui, &objects.objects.records, custom_objects);
                }
            });
    }

    fn show_layer2_tilemap_canvas(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
        toolbar_images: &MainToolbarImageSet,
    ) {
        let Some(texture) = self.animated_background_plane_textures.first().cloned() else {
            return;
        };
        let display_side = ui.available_width().min(256.0).max(128.0);
        let image_size = egui::vec2(display_side, display_side);
        let viewport_size = egui::vec2(ui.available_width().max(display_side), 280.0);
        ui.label("32×32 background canvas");
        egui::ScrollArea::both()
            .id_salt("vanilla-layer2-background-canvas")
            .max_height(viewport_size.y)
            .show(ui, |ui| {
                let canvas_size = tiled_surface_canvas_size(image_size, viewport_size);
                let (canvas_rect, response) =
                    ui.allocate_exact_size(canvas_size, egui::Sense::click());
                let image_rect = egui::Rect::from_min_size(canvas_rect.min, image_size);
                let painter = ui.painter_at(canvas_rect);
                painter.rect_filled(canvas_rect, 0.0, egui::Color32::BLACK);
                toolbar_images.paint_tiled_surface(
                    &painter,
                    OriginalTiledImage::BackgroundCanvas,
                    canvas_rect,
                    egui::pos2(image_rect.max.x, image_rect.min.y),
                );
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                let cell = display_side / 32.0;
                if let Some((x, y)) = layer2_canvas_coordinates(self.selected_layer2_tile) {
                    let selected = egui::Rect::from_min_size(
                        image_rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                        egui::vec2(cell, cell),
                    );
                    painter.rect_stroke(
                        selected,
                        0.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::YELLOW),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked()
                    && let Some(position) = response.interact_pointer_pos()
                    && let Some(index) = layer2_tile_at_canvas_position(position, image_rect, cell)
                    && let Some(word) = bytes.get(index * 2..index * 2 + 2)
                {
                    self.selected_layer2_tile = index;
                    self.layer2_word = u16::from_le_bytes([word[0], word[1]]);
                }
            });
    }

    #[allow(
        clippy::too_many_lines,
        reason = "keeps the complete Layer 2 object list, semantic fields, and ordered actions together"
    )]
    fn show_layer2_object_editor(
        &mut self,
        ui: &mut egui::Ui,
        records: &[ObjectRecord],
        custom_objects: Option<&lm_level::OscResolvedTable>,
    ) {
        self.selected_layer2_object = self
            .selected_layer2_object
            .min(records.len().saturating_sub(1));
        egui::ScrollArea::vertical()
            .id_salt("vanilla-layer2-object-list")
            .max_height(180.0)
            .show(ui, |ui| {
                for (index, record) in records.iter().enumerate() {
                    let encoded = record
                        .encoded()
                        .iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if ui
                        .selectable_label(
                            index == self.selected_layer2_object,
                            format!(
                                "{index:03}: command {:02X} · {encoded}",
                                record.command_id()
                            ),
                        )
                        .clicked()
                    {
                        self.selected_layer2_object = index;
                        self.layer2_object_form = ObjectForm::from_record(record);
                        self.layer2_object_placement_template = Some(record.clone());
                    }
                }
            });
        let resize_model = records
            .get(self.selected_layer2_object)
            .and_then(|record| self.active_object_resize_model(record, custom_objects));
        show_compact_object_fields(
            ui,
            "vanilla-layer2-object-fields",
            &mut self.layer2_object_form,
        );
        show_standard_object_resize_fields(ui, resize_model, &mut self.layer2_object_form);
        show_raw_object_record(
            ui,
            "vanilla-layer2-raw-object",
            &mut self.layer2_object_form,
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Place on canvas").clicked() {
                self.placement_mode = Some(CanvasPlacementMode::Layer2Object);
                self.error = None;
            }
            if ui.button("Insert after selection").clicked() {
                self.insert_layer2_object_after_selection(records.len());
            }
            let has_selection = self.selected_layer2_object < records.len();
            if ui
                .add_enabled(has_selection, egui::Button::new("Apply fields"))
                .clicked()
            {
                let edits = object_field_edits(
                    &self.layer2_object_form,
                    self.selected_layer2_object,
                    records.get(self.selected_layer2_object),
                );
                self.apply_layer2_object_result(edits);
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Apply raw record"))
                .clicked()
            {
                let edit = self.layer2_object_form.raw_record().map(|record| {
                    vec![ObjectEdit::Replace {
                        index: self.selected_layer2_object,
                        record,
                    }]
                });
                self.apply_layer2_object_result(edit);
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Remove object"))
                .clicked()
            {
                self.apply_layer2_object_result(Ok(vec![ObjectEdit::Remove {
                    index: self.selected_layer2_object,
                }]));
            }
            self.layer2_object_move_buttons(ui, records.len());
            if ui
                .add_enabled(has_selection, egui::Button::new("Copy"))
                .clicked()
                && let Some(record) = records.get(self.selected_layer2_object)
            {
                match crate::native_clipboard::encode_level_object(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Paste after selection").clicked() {
                self.paste_target = Some(EntityPasteTarget::Layer2Object);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if self.paste_target == Some(EntityPasteTarget::Layer2Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.paste_layer2_object(&text, records.len());
        }
    }

    fn apply_layer2_object_result(&mut self, edits: Result<Vec<ObjectEdit>, String>) {
        match edits {
            Ok(edits) => {
                let result = self
                    .controller
                    .as_mut()
                    .ok_or_else(|| "level controller is unavailable".to_owned())
                    .and_then(|controller| {
                        controller
                            .apply_layer2_object_edits(&edits)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => {
                        self.reload_layer2_object_form();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn insert_layer2_object_after_selection(&mut self, record_count: usize) {
        let insertion = object_insertion_index(self.selected_layer2_object, record_count);
        let result = self
            .layer2_object_record_for_placement()
            .and_then(|record| {
                self.controller
                    .as_mut()
                    .ok_or_else(|| "level controller is unavailable".to_owned())?
                    .apply_layer2_object_edits(&[ObjectEdit::Insert {
                        index: insertion,
                        record,
                    }])
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                self.selected_layer2_object = insertion;
                self.reload_layer2_object_form();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn reload_layer2_object_form(&mut self) {
        let records = self.controller.as_ref().and_then(|controller| {
            controller.layer2().and_then(|layer2| match layer2 {
                lm_level::NativeLayer2Data::Objects(objects) => Some(&objects.objects.records),
                lm_level::NativeLayer2Data::Tilemap(_) => None,
            })
        });
        let Some(records) = records else {
            self.selected_layer2_object = 0;
            self.layer2_object_form = ObjectForm::default();
            self.layer2_object_placement_template = None;
            return;
        };
        self.selected_layer2_object = self
            .selected_layer2_object
            .min(records.len().saturating_sub(1));
        if let Some(record) = records.get(self.selected_layer2_object).cloned() {
            self.layer2_object_form = ObjectForm::from_record(&record);
            self.layer2_object_placement_template = Some(record);
        } else {
            self.layer2_object_form = ObjectForm::default();
            self.layer2_object_placement_template = None;
        }
    }

    fn show_header_editor(&mut self, ui: &mut egui::Ui, objects: usize, sprites: usize) {
        ui.label(format!("{objects} objects, {sprites} sprite records"));
        egui::Grid::new("vanilla-level-header").show(ui, |ui| {
            header_row(ui, "Level mode", &mut self.form.level_mode, 31);
            header_row(
                ui,
                "Background palette",
                &mut self.form.background_palette,
                7,
            );
            header_row(ui, "Last screen", &mut self.form.last_screen, 31);
            header_row(ui, "Background color", &mut self.form.background_color, 7);
            header_row(ui, "Sprite tileset", &mut self.form.sprite_tileset, 15);
            header_row(
                ui,
                "Default music selector",
                &mut self.form.default_music_selector,
                7,
            );
            header_row(
                ui,
                "Time limit selector",
                &mut self.form.time_limit_selector,
                3,
            );
            ui.label("Custom time bypass");
            ui.checkbox(&mut self.form.custom_time_enabled, "Enabled");
            ui.end_row();
            ui.label("Custom time (hex)");
            ui.add_enabled(
                self.form.custom_time_enabled,
                egui::DragValue::new(&mut self.form.custom_time_value)
                    .range(0..=CustomTimeSettings::MAX_VALUE)
                    .hexadecimal(3, false, true),
            );
            ui.end_row();
            ui.label("Force time reset");
            ui.add_enabled(
                self.form.custom_time_enabled,
                egui::Checkbox::without_text(&mut self.form.force_time_reset),
            );
            ui.end_row();
            header_row(
                ui,
                "Foreground palette",
                &mut self.form.foreground_palette,
                7,
            );
            header_row(ui, "Sprite palette", &mut self.form.sprite_palette, 7);
            header_row(ui, "Object tileset", &mut self.form.object_tileset, 15);
            header_row(
                ui,
                "Layer 1 vertical scroll",
                &mut self.form.layer1_vertical_scroll,
                3,
            );
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Stage header changes").clicked() {
                let controller = self
                    .controller
                    .as_ref()
                    .expect("controller presence checked above");
                let source_mode = controller.level().layer1.header.level_mode();
                let target_mode = lm_level::lunar_magic_canonical_level_mode(self.form.level_mode);
                let resets_layer2 = mode_change_resets_layer2(
                    source_mode,
                    target_mode,
                    controller.layer2().is_some(),
                );
                if resets_layer2 {
                    self.pending_layer2_mode_reset = Some(self.form);
                    return;
                }
                let result = self
                    .form
                    .edits()
                    .map_err(|error| error.to_string())
                    .and_then(|edits| {
                        self.controller
                            .as_mut()
                            .expect("controller presence checked above")
                            .apply_edits(&edits)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Reset staged values").clicked() {
                self.form = HeaderForm::from_controller(
                    self.controller
                        .as_ref()
                        .expect("controller presence checked above"),
                );
                self.error = None;
            }
        });
    }

    fn show_layer2_mode_reset_confirmation(&mut self, context: &egui::Context) {
        let Some(form) = self.pending_layer2_mode_reset else {
            return;
        };
        let source_mode = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.header.level_mode()
        });
        let target_mode = lm_level::lunar_magic_canonical_level_mode(form.level_mode);
        egui::Window::new("Reset Layer 2 for level mode change?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!(
                    "Changing level mode ${source_mode:02X} to ${target_mode:02X} switches Layer 2 storage formats."
                ));
                ui.label(
                    "Lunar Magic clears the tilemap workspace when entering a tilemap-backed mode. Object-backed data remains available if you switch back before saving.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_layer2_mode_reset = None;
                    }
                    if ui.button("Reset Layer 2 and stage changes").clicked() {
                        let result = form.edits().map_err(|error| error.to_string()).and_then(
                            |edits| {
                                self.controller
                                    .as_mut()
                                    .ok_or_else(|| "level workspace is closed".to_owned())?
                                    .apply_edits_with_layer2_reset(&edits, true)
                                    .map_err(|error| error.to_string())
                            },
                        );
                        self.pending_layer2_mode_reset = None;
                        match result {
                            Ok(()) => {
                                self.error = None;
                                if let Some(controller) = &self.controller {
                                    self.form = HeaderForm::from_controller(controller);
                                }
                                self.selected_layer2_tile = 0;
                                self.layer2_word = self
                                    .controller
                                    .as_ref()
                                    .and_then(LevelController::layer2)
                                    .and_then(|layer2| match layer2 {
                                        lm_level::NativeLayer2Data::Tilemap(bytes) => bytes
                                            .get(..2)
                                            .map(|word| u16::from_le_bytes([word[0], word[1]])),
                                        lm_level::NativeLayer2Data::Objects(_) => None,
                                    })
                                    .unwrap_or_default();
                                self.reload_layer2_object_form();
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                });
            });
    }

    fn show_entrance_editor(&mut self, ui: &mut egui::Ui, level: u16) -> Option<Command> {
        ui.collapsing("Main entrance", |ui| {
            ui.label("Exact four-plane vanilla SMW entrance record.");
            egui::Grid::new("vanilla-main-entrance").show(ui, |ui| {
                header_row(ui, "Position", &mut self.entrance_form.position, u8::MAX);
                let mut layer2_scroll_table = self.entrance_form.position >> 4;
                ui.label("Layer 2 original scroll preset");
                if ui
                    .add(egui::DragValue::new(&mut layer2_scroll_table).range(0..=15))
                    .changed()
                {
                    self.entrance_form.position =
                        self.entrance_form.position & 0x0f | layer2_scroll_table << 4;
                }
                ui.end_row();
                header_row(
                    ui,
                    "Vertical settings",
                    &mut self.entrance_form.vertical_settings,
                    u8::MAX,
                );
                header_row(
                    ui,
                    "Screen / method",
                    &mut self.entrance_form.screen_and_method,
                    u8::MAX,
                );
                header_row(
                    ui,
                    "Level mode / screen",
                    &mut self.entrance_form.level_mode_and_screen,
                    u8::MAX,
                );
            });
            if let Some(midway) = &mut self.midway_form {
                ui.separator();
                ui.label("Installed separate midway entrance");
                egui::Grid::new("installed-midway-entrance").show(ui, |ui| {
                    header_row(ui, "Flags", &mut midway.flags, u8::MAX);
                    header_row(ui, "Position", &mut midway.position, u8::MAX);
                    header_row(
                        ui,
                        "Additional flags",
                        &mut midway.additional_flags,
                        u8::MAX,
                    );
                    header_row(ui, "High position", &mut midway.high_position, u8::MAX);
                });
            } else {
                ui.label("Separate-midway runtime is not installed. Initial values:");
                egui::Grid::new("new-midway-entrance").show(ui, |ui| {
                    header_row(ui, "Flags", &mut self.midway_install_form.flags, u8::MAX);
                    header_row(
                        ui,
                        "Position",
                        &mut self.midway_install_form.position,
                        u8::MAX,
                    );
                    header_row(
                        ui,
                        "Additional flags",
                        &mut self.midway_install_form.additional_flags,
                        u8::MAX,
                    );
                    header_row(
                        ui,
                        "High position",
                        &mut self.midway_install_form.high_position,
                        u8::MAX,
                    );
                });
            }
        });
        let controller = self.entrance_controller.as_mut()?;
        if self.midway_form.is_none() && ui.button("Install separate midway runtime").clicked() {
            return match controller.prepare_midway_install(self.midway_install_form) {
                Ok(prepared) => Some(prepared.into_command()),
                Err(error) => {
                    self.error = Some(error.to_string());
                    None
                }
            };
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Stage entrance fields").clicked() {
                controller.set_entrance(self.entrance_form);
                if let Some(midway) = self.midway_form {
                    controller.set_midway_entrance(midway);
                }
            }
            if ui.button("Reset entrance").clicked() {
                self.entrance_form = controller.entrance();
                self.midway_form = controller.midway_entrance();
            }
        });
        if ui
            .add_enabled(
                controller.is_modified(),
                egui::Button::new("Commit entrances to ROM"),
            )
            .clicked()
        {
            return match controller.prepare_commit(format!("Edit level {level:03X} entrances")) {
                Ok(prepared) => Some(prepared.into_command()),
                Err(error) => {
                    self.error = Some(error.to_string());
                    None
                }
            };
        }
        None
    }

    fn show_staged_history(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let can_undo = controller.can_undo();
        let can_redo = controller.can_redo();
        let modified = controller.is_modified();
        let mut undo = false;
        let mut redo = false;
        ui.horizontal_wrapped(|ui| {
            undo = ui
                .add_enabled(can_undo, egui::Button::new("Undo staged edit"))
                .clicked();
            redo = ui
                .add_enabled(can_redo, egui::Button::new("Redo staged edit"))
                .clicked();
            ui.label(if modified {
                "ROM has uncommitted level changes"
            } else {
                "Level matches the opened ROM"
            });
        });
        if undo || redo {
            let changed = if undo {
                self.controller
                    .as_mut()
                    .expect("controller presence checked above")
                    .undo()
            } else {
                self.controller
                    .as_mut()
                    .expect("controller presence checked above")
                    .redo()
            };
            if changed {
                self.refresh_forms_after_history();
                self.error = None;
            }
        }
    }

    fn show_screen_exit_table_editor(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = self.controller.as_ref() else {
            ui.label("The current level is unavailable.");
            return;
        };
        let current = screen_exit_table(&controller.level().layer1.objects.records);
        let selected_screen_exit = self.screen_exit_table_selected;
        let form = self.screen_exit_table_form.get_or_insert(current);
        ui.label(
            "Stage all 32 source screens together. Apply creates one level-editor Undo step; Reset discards this form only.",
        );
        egui::Grid::new("vanilla-screen-exit-table-grid")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Screen");
                ui.label("Present");
                ui.label("Destination / flags");
                ui.end_row();
                for (screen, entry) in form.iter_mut().enumerate() {
                    let selected = selected_screen_exit == Some(screen as u8);
                    let response = ui.selectable_label(selected, format!("{screen:02X}"));
                    if selected {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    let mut present = entry.is_some();
                    if ui.checkbox(&mut present, "").changed() {
                        *entry = present.then_some(0);
                    }
                    if let Some(value) = entry {
                        ui.add(egui::DragValue::new(value).hexadecimal(4, false, true));
                    } else {
                        ui.label("—");
                    }
                    ui.end_row();
                }
            });
        let dirty = *form != current;
        let mut apply = false;
        let mut reset = false;
        ui.horizontal_wrapped(|ui| {
            apply = ui
                .add_enabled(dirty, egui::Button::new("Apply all screen exits"))
                .clicked();
            reset = ui
                .add_enabled(dirty, egui::Button::new("Reset screen exits"))
                .clicked();
        });
        if reset {
            self.screen_exit_table_form = Some(current);
        } else if apply {
            let exits = *self
                .screen_exit_table_form
                .as_ref()
                .expect("the form was initialized above");
            let result = self
                .controller
                .as_mut()
                .expect("the controller was checked above")
                .apply_edits(&[NativeLevelEdit::Objects(vec![
                    ObjectEdit::ReplaceScreenExitTable { exits },
                ])]);
            match result {
                Ok(()) => {
                    let controller = self.controller.as_ref().expect("apply retained controller");
                    self.screen_exit_table_form = Some(screen_exit_table(
                        &controller.level().layer1.objects.records,
                    ));
                    self.reload_object_form();
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn refresh_forms_after_history(&mut self) {
        self.clear_z_order_bounds();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        self.screen_exit_table_form = Some(screen_exit_table(
            &controller.level().layer1.objects.records,
        ));
        self.form = HeaderForm::from_controller(controller);
        self.selected_object = self.selected_object.min(
            controller
                .level()
                .layer1
                .objects
                .records
                .len()
                .saturating_sub(1),
        );
        self.object_form = controller
            .level()
            .layer1
            .objects
            .records
            .get(self.selected_object)
            .map_or_else(ObjectForm::default, ObjectForm::from_record);
        self.selected_sprite = self
            .selected_sprite
            .min(controller.level().sprites.tokens.len().saturating_sub(1));
        self.sprite_form = SpriteForm::from_token(
            controller.level().sprites.header,
            controller.level().sprites.tokens.get(self.selected_sprite),
        );
        if let Some(lm_level::NativeLayer2Data::Tilemap(bytes)) = controller.layer2() {
            self.selected_layer2_tile = self
                .selected_layer2_tile
                .min((bytes.len() / 2).saturating_sub(1));
            if let Some(word) =
                bytes.get(self.selected_layer2_tile * 2..self.selected_layer2_tile * 2 + 2)
            {
                self.layer2_word = u16::from_le_bytes([word[0], word[1]]);
            }
        } else if let Some(lm_level::NativeLayer2Data::Objects(objects)) = controller.layer2() {
            self.selected_layer2_object = self
                .selected_layer2_object
                .min(objects.objects.records.len().saturating_sub(1));
            self.layer2_object_form = objects
                .objects
                .records
                .get(self.selected_layer2_object)
                .map_or_else(ObjectForm::default, ObjectForm::from_record);
            self.layer2_object_placement_template = objects
                .objects
                .records
                .get(self.selected_layer2_object)
                .cloned();
        }
        self.object_placement_template = controller
            .level()
            .layer1
            .objects
            .records
            .get(self.selected_object)
            .cloned();
        self.dragging_object = None;
        self.dragging_layer2_object = None;
        self.object_group_drag = None;
        self.selected_object_group.clear();
        self.selected_layer2_object_group.clear();
        self.selected_sprite_group.clear();
        self.resizing_object = None;
        self.resizing_layer2_object = None;
        self.external_sprite_textures.clear();
        self.dragging_sprite = None;
        self.secondary_duplicate_drag = false;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "keeps one failure-atomic level, Layer 2, entrance, and visual-asset snapshot load together"
    )]
    fn load(
        &mut self,
        snapshot: &lm_app::ControllerSnapshot,
        key: EditorKey,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
    ) {
        self.clear_z_order_bounds();
        self.screen_exit_table_form = None;
        self.screen_exit_table_selected = None;
        self.canvas_geometry = None;
        self.pending_layer2_mode_reset = None;
        self.canvas_entity_selection = None;
        if let Err(error) = validate_builtin_graphics_layout(snapshot) {
            self.controller = None;
            self.entrance_controller = None;
            self.error = Some(error);
            self.key = Some(key);
            return;
        }
        let sprite_lengths = match sprite_lengths_from_ssc(custom_sprites) {
            Ok(lengths) => lengths,
            Err(error) => {
                self.controller = None;
                self.error = Some(error);
                self.key = Some(key);
                return;
            }
        };
        let layer2_layout = match editor_layer2_layout(snapshot, key.level) {
            Ok(layout) => layout,
            Err(error) => {
                self.controller = None;
                self.error = Some(error);
                self.key = Some(key);
                return;
            }
        };
        let level_layout = match editor_level_layout(snapshot, key.level) {
            Ok(layout) => layout,
            Err(error) => {
                self.controller = None;
                self.entrance_controller = None;
                self.error = Some(error);
                self.key = Some(key);
                return;
            }
        };
        match LevelController::decode_with_layer2(
            snapshot,
            level_layout,
            layer2_layout,
            &sprite_lengths,
        ) {
            Ok(controller) => {
                let mut entrance_layout = lm_profile::smw_us_v1_vanilla_entrance_layout();
                entrance_layout.mapper = snapshot.identity.mapper;
                let entrance_error = match VanillaEntranceController::decode_with_midway(
                    snapshot,
                    entrance_layout,
                    lm_profile::smw_us_v1_separate_midway_locator(),
                ) {
                    Ok(entrance) => {
                        self.entrance_controller = Some(entrance);
                        None
                    }
                    Err(error) => {
                        self.entrance_controller = None;
                        Some(error.to_string())
                    }
                };
                self.entrance_form = self.entrance_controller.as_ref().map_or_else(
                    VanillaMainEntrance::default,
                    VanillaEntranceController::entrance,
                );
                self.initial_vertical_scroll_tiles =
                    visual_smoke_editor_scroll_row().or_else(|| {
                        (!lm_profile::smw_us_v1_level_mode(
                            controller.level().layer1.header.level_mode(),
                        )
                        .vertical)
                            .then(|| vanilla_horizontal_entrance_scroll_row(self.entrance_form))
                    });
                self.preview_camera_major_offset = visual_smoke_camera_offset("MAJOR");
                self.preview_camera_minor_offset = visual_smoke_camera_offset("MINOR");
                self.midway_form = self
                    .entrance_controller
                    .as_ref()
                    .and_then(VanillaEntranceController::midway_entrance);
                self.midway_install_form = SeparateMidwayEntrance::default();
                let editor_project = RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .map(Project::new)
                    .map_err(|error| error.to_string());
                if self.secondary_exits_revision != Some(snapshot.revision) {
                    match editor_project.as_ref() {
                        Ok(project) => match project.load_secondary_exit_table_detected(
                            lm_profile::smw_us_v1_secondary_exit_locator(),
                        ) {
                            Ok(loaded) => {
                                self.secondary_exits = Some(loaded.table);
                                match referenced_secondary_exit_slots(project) {
                                    Ok(references) => {
                                        self.secondary_exit_references = Some(references);
                                        self.secondary_exits_error = None;
                                    }
                                    Err(error) => {
                                        self.secondary_exit_references = None;
                                        self.secondary_exits_error = Some(error);
                                    }
                                }
                            }
                            Err(error) => {
                                self.secondary_exits = None;
                                self.secondary_exit_references = None;
                                self.secondary_exits_error = Some(error.to_string());
                            }
                        },
                        Err(error) => {
                            self.secondary_exits = None;
                            self.secondary_exit_references = None;
                            self.secondary_exits_error = Some(error.clone());
                        }
                    }
                    self.secondary_exits_revision = Some(snapshot.revision);
                }
                let secondary_exit_error = self.secondary_exits_error.clone();
                self.standard_object_map = editor_project.as_ref().ok().and_then(|project| {
                    lm_profile::load_smw_us_v1_standard_object_definition_map(&project.rom).ok()
                });
                self.shared_vanilla_background = editor_project
                    .as_ref()
                    .ok()
                    .and_then(|project| {
                        lm_profile::smw_us_v1_level_uses_shared_background(
                            &project.rom,
                            controller.level().number,
                        )
                        .ok()
                    })
                    .unwrap_or(false);
                self.selected_layer2_tile = 0;
                self.selected_layer2_object = 0;
                self.layer2_word = controller
                    .layer2()
                    .and_then(|layer2| match layer2 {
                        lm_level::NativeLayer2Data::Tilemap(bytes) => bytes
                            .get(..2)
                            .map(|word| u16::from_le_bytes([word[0], word[1]])),
                        lm_level::NativeLayer2Data::Objects(_) => None,
                    })
                    .unwrap_or_default();
                self.layer2_object_form = controller
                    .layer2()
                    .and_then(|layer2| match layer2 {
                        lm_level::NativeLayer2Data::Objects(objects) => {
                            objects.objects.records.first()
                        }
                        lm_level::NativeLayer2Data::Tilemap(_) => None,
                    })
                    .map_or_else(ObjectForm::default, ObjectForm::from_record);
                self.layer2_object_placement_template =
                    controller.layer2().and_then(|layer2| match layer2 {
                        lm_level::NativeLayer2Data::Objects(objects) => {
                            objects.objects.records.first().cloned()
                        }
                        lm_level::NativeLayer2Data::Tilemap(_) => None,
                    });
                self.form = HeaderForm::from_controller(&controller);
                self.selected_object = 0;
                self.object_form = controller
                    .level()
                    .layer1
                    .objects
                    .records
                    .first()
                    .map_or_else(ObjectForm::default, ObjectForm::from_record);
                self.object_placement_template =
                    controller.level().layer1.objects.records.first().cloned();
                self.selected_sprite = 0;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.first(),
                );
                self.controller = Some(controller);
                self.error = entrance_error.or(secondary_exit_error);
            }
            Err(error) => {
                self.controller = None;
                self.entrance_controller = None;
                self.secondary_exits = None;
                self.secondary_exit_references = None;
                self.secondary_exits_revision = None;
                self.secondary_exits_error = None;
                self.midway_form = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn clear(&mut self) {
        self.clear_z_order_bounds();
        self.key = None;
        self.controller = None;
        self.pending_expansion_commit = None;
        self.screen_exit_table_form = None;
        self.screen_exit_table_selected = None;
        self.canvas_geometry = None;
        self.entrance_controller = None;
        self.secondary_exits = None;
        self.secondary_exit_references = None;
        self.secondary_exits_revision = None;
        self.secondary_exits_error = None;
        self.midway_form = None;
        self.midway_install_form = SeparateMidwayEntrance::default();
        self.preview_camera_major_offset = 0;
        self.preview_camera_minor_offset = 0;
        self.pending_layer2_mode_reset = None;
        self.error = None;
        self.map16_key = None;
        self.map16_texture = None;
        self.layer2_map16_texture = None;
        self.background_map16_texture = None;
        self.animated_map16_textures.clear();
        self.block_contents_textures.clear();
        self.animated_layer2_map16_textures.clear();
        self.animated_background_map16_textures.clear();
        self.animated_background_plane_textures.clear();
        self.shared_vanilla_background = false;
        self.sprite_texture = None;
        self.animated_sprite_textures.clear();
        self.entrance_texture = None;
        self.sprite_tiles.clear();
        self.foreground_tiles.clear();
        self.layer3_tiles.clear();
        self.layer3_low_texture = None;
        self.layer3_high_texture = None;
        self.layer3_position = None;
        self.layer3_editor_row_offset = None;
        self.layer3_between_background_and_foreground = false;
        self.sprite_palette = None;
        self.canvas_backdrop = None;
        self.foreground_texture = None;
        self.map16_summary = None;
        self.map16_error = None;
        self.standard_object_map = None;
        self.object_placement_template = None;
        self.layer2_object_placement_template = None;
        self.paste_target = None;
        self.dragging_sprite = None;
        self.dragging_object = None;
        self.dragging_layer2_object = None;
        self.secondary_duplicate_drag = false;
        self.object_group_drag = None;
        self.selected_object_group.clear();
        self.selected_layer2_object_group.clear();
        self.selected_sprite_group.clear();
        self.resizing_object = None;
        self.resizing_layer2_object = None;
        self.canvas_entity_selection = None;
    }

    fn take_pending_expansion_commit(
        &mut self,
        snapshot: &lm_app::ControllerSnapshot,
    ) -> Result<Option<Command>, String> {
        let Some(mut staged) = self.pending_expansion_commit.take() else {
            return Ok(None);
        };
        staged
            .rebase_after_rom_expansion(snapshot)
            .map_err(|error| error.to_string())?;
        prepare_commit(&staged, snapshot).map(Some)
    }

    #[allow(clippy::too_many_lines)]
    fn show_map16_preview(&mut self, ui: &mut egui::Ui, object_tileset: u8) {
        egui::CollapsingHeader::new(format!(
            "Graphics reference (not a picker) — tileset {object_tileset:X}"
        ))
        .default_open(false)
        .show(ui, |ui| {
            ui.small(
                "Reference atlas only. Add level content with the visual Layer 1 object and Sprite catalogs above.",
            );
            let sprite_tileset = self.form.sprite_tileset;
            if let Some(summary) = self.map16_summary {
                let files = summary.foreground_files;
                let background_files = summary.background_files;
                let sprite_files = summary.sprite_files;
                let common = summary.common_tiles;
                let specific = summary.tileset_tiles;
                ui.label(format!(
                    "GFX{:02X}/GFX{:02X}/GFX{:02X}/GFX{:02X}; {common} common and {specific} tileset-specific definitions",
                    files[0], files[1], files[2], files[3]
                ));
                if background_files != files {
                    ui.label(format!(
                        "Background runtime: GFX{:02X}/GFX{:02X}/GFX{:02X}/GFX{:02X}",
                        background_files[0],
                        background_files[1],
                        background_files[2],
                        background_files[3]
                    ));
                }
                ui.label(format!(
                    "Sprite set {sprite_tileset:X}: SP1 GFX{:02X}, SP2 GFX{:02X}, SP3 GFX{:02X}, SP4 GFX{:02X}",
                    sprite_files[0], sprite_files[1], sprite_files[2], sprite_files[3]
                ));
            }
            if let Some(texture) = &self.map16_texture {
                egui::ScrollArea::horizontal()
                    .id_salt("vanilla-map16-preview")
                    .show(ui, |ui| {
                        ui.image(texture);
                    });
                ui.small(
                    "Decoded 4bpp pixels, level palette, and flip attributes; pink marks tiles outside the four foreground slots.",
                );
                if let Some(sprite_texture) = &self.sprite_texture {
                    ui.image(sprite_texture);
                    ui.small(
                        "Recovered SP1–SP4 graphics assignments, decoded with sprite palette row 8 and used by standard/custom sprite previews below.",
                    );
                }
            } else if let Some(error) = &self.map16_error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });
    }

    #[allow(clippy::too_many_lines)]
    fn ensure_map16_assets(
        &mut self,
        context: &egui::Context,
        snapshot: &lm_app::ControllerSnapshot,
        object_tileset: u8,
        special_world_passed: bool,
    ) {
        let sprite_tileset = self.form.sprite_tileset;
        let level = self.controller.as_ref().map_or(0, |controller| {
            u16::try_from(controller.level().number).unwrap_or(0)
        });
        let game_runtime = self.game_preview();
        let key = (
            snapshot.revision,
            level,
            object_tileset,
            sprite_tileset,
            game_runtime,
            special_world_passed,
            self.blue_pow_active,
            self.silver_pow_active,
            self.conditional_view_state,
        );
        if self.map16_key == Some(key) {
            return;
        }
        self.map16_texture = None;
        self.layer2_map16_texture = None;
        self.background_map16_texture = None;
        self.animated_map16_textures.clear();
        self.block_contents_textures.clear();
        self.animated_layer2_map16_textures.clear();
        self.animated_background_map16_textures.clear();
        self.animated_background_plane_textures.clear();
        self.sprite_texture = None;
        self.animated_sprite_textures.clear();
        self.entrance_texture = None;
        self.sprite_tiles.clear();
        self.foreground_tiles.clear();
        self.layer3_tiles.clear();
        self.layer3_low_texture = None;
        self.layer3_high_texture = None;
        self.layer3_position = None;
        self.layer3_editor_row_offset = None;
        self.layer3_between_background_and_foreground = false;
        self.sprite_palette = None;
        self.canvas_backdrop = None;
        self.external_sprite_textures.clear();
        self.foreground_texture = None;
        self.map16_summary = None;
        self.map16_error = None;
        match crate::vanilla_map16_preview::render_with_animation_view_state(
            snapshot.rom_bytes.clone(),
            level,
            self.controller
                .as_ref()
                .map_or_default(|controller| controller.level().layer1.header),
            game_runtime,
            special_world_passed,
            crate::vanilla_map16_preview::VanillaAnimationViewState {
                blue_pow_active: self.blue_pow_active,
                silver_pow_active: self.silver_pow_active,
                conditional: self.conditional_view_state,
            },
        ) {
            Ok(preview) => {
                let background_planes = self
                    .controller
                    .as_ref()
                    .and_then(|controller| controller.layer2())
                    .and_then(|layer2| match layer2 {
                        lm_level::NativeLayer2Data::Tilemap(bytes) => Some(
                            bytes
                                .chunks_exact(2)
                                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                                .collect::<Vec<_>>(),
                        ),
                        lm_level::NativeLayer2Data::Objects(_) => None,
                    })
                    .map(|tilemap| {
                        preview
                            .animated_background_images
                            .iter()
                            .map(|image| {
                                crate::vanilla_map16_preview::compose_native_map16_plane(
                                    image, &tilemap,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose();
                self.map16_summary = Some(Map16Summary {
                    foreground_files: preview.graphics_files,
                    background_files: preview.background_graphics_files,
                    sprite_files: preview.sprite_graphics_files,
                    common_tiles: preview.common_tiles,
                    tileset_tiles: preview.tileset_tiles,
                });
                self.map16_texture = Some(context.load_texture(
                    format!("vanilla-map16-{object_tileset:X}-{}", snapshot.revision),
                    preview.image,
                    egui::TextureOptions::NEAREST,
                ));
                self.layer2_map16_texture = Some(context.load_texture(
                    format!(
                        "vanilla-layer2-map16-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.layer2_image,
                    egui::TextureOptions::NEAREST,
                ));
                self.background_map16_texture = Some(context.load_texture(
                    format!(
                        "vanilla-background-map16-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.background_image,
                    egui::TextureOptions::NEAREST,
                ));
                self.animated_map16_textures = load_animation_textures(
                    context,
                    &format!("vanilla-map16-{object_tileset:X}-{}", snapshot.revision),
                    preview.animated_images,
                );
                self.block_contents_textures = load_animation_textures(
                    context,
                    &format!(
                        "vanilla-block-contents-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.block_contents_images,
                );
                self.animated_layer2_map16_textures = load_animation_textures(
                    context,
                    &format!(
                        "vanilla-layer2-map16-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.animated_layer2_images,
                );
                self.animated_background_map16_textures = load_animation_textures(
                    context,
                    &format!(
                        "vanilla-background-map16-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.animated_background_images,
                );
                match background_planes {
                    Ok(Some(images)) => {
                        self.animated_background_plane_textures = load_animation_textures(
                            context,
                            &format!(
                                "vanilla-background-plane-{object_tileset:X}-{}",
                                snapshot.revision
                            ),
                            images,
                        );
                    }
                    Ok(None) => {}
                    Err(error) => self.map16_error = Some(error),
                }
                self.sprite_texture = Some(context.load_texture(
                    format!(
                        "vanilla-sprite-gfx-{sprite_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.sprite_image,
                    egui::TextureOptions::NEAREST,
                ));
                self.animated_sprite_textures = load_animation_textures(
                    context,
                    &format!(
                        "vanilla-sprite-gfx-{sprite_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.animated_sprite_images,
                );
                self.entrance_texture = Some(context.load_texture(
                    format!("vanilla-entrance-{}", snapshot.revision),
                    preview.entrance_image,
                    egui::TextureOptions::NEAREST,
                ));
                self.foreground_texture = Some(context.load_texture(
                    format!(
                        "vanilla-foreground-gfx-{object_tileset:X}-{}",
                        snapshot.revision
                    ),
                    preview.foreground_image,
                    egui::TextureOptions::NEAREST,
                ));
                self.sprite_tiles = preview.sprite_tiles;
                self.foreground_tiles = preview.foreground_tiles;
                self.layer3_tiles = preview.layer3_tiles;
                self.layer3_low_texture = preview.layer3_low_image.map(|image| {
                    context.load_texture(
                        format!("vanilla-layer3-low-{level:03X}-{}", snapshot.revision),
                        image,
                        egui::TextureOptions::NEAREST,
                    )
                });
                self.layer3_high_texture = preview.layer3_high_image.map(|image| {
                    context.load_texture(
                        format!("vanilla-layer3-high-{level:03X}-{}", snapshot.revision),
                        image,
                        egui::TextureOptions::NEAREST,
                    )
                });
                self.layer3_position = preview.layer3_position;
                self.layer3_editor_row_offset = preview.layer3_editor_row_offset;
                self.layer3_between_background_and_foreground =
                    preview.layer3_between_background_and_foreground;
                self.canvas_backdrop = Some(preview.backdrop);
                self.sprite_palette = Some(preview.palette);
            }
            Err(error) => self.map16_error = Some(error),
        }
        self.map16_key = Some(key);
    }

    fn object_list(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = &self.controller else {
            return;
        };
        let family =
            lm_profile::smw_us_v1_object_family(controller.level().layer1.header.object_tileset());
        let count = controller.level().layer1.objects.records.len();
        egui::CollapsingHeader::new(format!(
            "Existing objects ({count}) — {}",
            family.display_name()
        ))
        .id_salt("vanilla-existing-layer1-objects")
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for (index, record) in
                        controller.level().layer1.objects.records.iter().enumerate()
                    {
                        let encoded = record
                            .encoded()
                            .iter()
                            .map(|byte| format!("{byte:02X}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if ui
                            .selectable_label(
                                index == self.selected_object,
                                format!(
                                    "{index:03}: command {:02X} · {encoded}",
                                    record.command_id()
                                ),
                            )
                            .clicked()
                        {
                            self.selected_object = index;
                            self.object_form = ObjectForm::from_record(record);
                            self.object_placement_template = Some(record.clone());
                        }
                    }
                });
        });
    }

    #[allow(clippy::too_many_lines)]
    fn object_canvas(
        &mut self,
        ui: &mut egui::Ui,
        visibility: crate::application::LevelViewVisibility,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        live_frame: Option<(egui::TextureId, [usize; 2])>,
        toolbar_images: &MainToolbarImageSet,
    ) {
        let CanvasModel {
            layer1_records: records,
            layer1_placements: placements,
            layer2_records,
            layer2_placements,
            layer2_tilemap,
            sprite_placements,
        } = self.canvas_model();
        let vertical = self.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        let level_mode = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.header.level_mode()
        });
        let object_tileset = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.header.object_tileset()
        });
        let animation_seconds = self.animation_seconds(ui.input(|input| input.time));
        let map16_animation_phase = map16_animation_phase(animation_seconds);
        let animation_phase = sprite_animation_phase(animation_seconds);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
        ensure_remapped_placement_textures(
            ui.ctx(),
            &mut self.external_sprite_textures,
            custom_sprites,
            SpriteRasterAssets {
                external: external_assets,
                foreground_tiles: &self.foreground_tiles,
                layer3_tiles: &self.layer3_tiles,
                vanilla_tiles: &self.sprite_tiles,
                vanilla_palette: self.sprite_palette.as_ref(),
            },
            custom_map16,
            &sprite_placements,
        );
        if sprite_placements
            .iter()
            .any(|placement| placement.sprite_number == 0xa6)
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(125));
        }
        let visible_objects = layer2_placements
            .iter()
            .chain(&placements)
            .copied()
            .collect::<Vec<_>>();
        let mut major_tiles = canvas_major_tiles(&visible_objects, &sprite_placements);
        let mode_major_tiles =
            u16::from(lm_profile::smw_us_v1_level_mode(level_mode).editor_major_screens)
                .saturating_mul(16);
        major_tiles = major_tiles.max(mode_major_tiles);
        // A horizontal SMW screen is 16×27 tiles. Byte 0 bit $10 places objects in its lower
        // 11-tile region; parameter nibbles describe command geometry and are not canvas bounds.
        let mut minor_tiles = if vertical {
            VERTICAL_LEVEL_MINOR_TILES
        } else {
            NATIVE_LEVEL_MINOR_TILES
        };
        for (records, placements) in [
            (records.as_slice(), placements.as_slice()),
            (layer2_records.as_slice(), layer2_placements.as_slice()),
        ] {
            let resize_models = self.active_object_resize_models(records, custom_objects);
            let (extended_major, extended_minor) =
                extended_command27_canvas_extent(records, placements, &resize_models, vertical);
            major_tiles = major_tiles.max(extended_major);
            minor_tiles = minor_tiles.max(extended_minor);
            if let Some(handler_map) = self.active_standard_object_handler_map()
                && let Some((rendered_major, rendered_minor)) =
                    rendered_standard_object_canvas_extent(records, handler_map, vertical)
            {
                major_tiles = major_tiles.max(rendered_major);
                minor_tiles = minor_tiles.max(rendered_minor);
            }
        }
        if !layer2_tilemap.is_empty() {
            // The SNES background plane is 32×32 and wraps while the camera traverses the level.
            // It guarantees one screen-pair along the level's major axis, but its off-viewport
            // second half must not double the editable minor axis of an ordinary level.
            major_tiles = major_tiles.max(32);
        }
        let game_preview = self.game_preview();
        let snes_viewport = game_preview && self.snes_viewport();
        self.show_canvas_tools(ui, major_tiles, minor_tiles, vertical, live_frame.is_some());
        if self.placement_mode.is_some() {
            ui.label("Click a canvas tile to place the values from the matching editor below.");
        } else {
            ui.label(
                "Select or drag an object/enemy; Insert places the active template at the pointer, right-click duplicates there, and Delete removes the selection.",
            );
        }
        let mut toolbar_shortcut = None;
        if let Some(selection) = self.canvas_entity_selection {
            let description = match selection {
                CanvasEntitySelection::Layer1Object => {
                    format!("Selected Layer 1 object {}", self.selected_object)
                }
                CanvasEntitySelection::Layer2Object => {
                    format!("Selected Layer 2 object {}", self.selected_layer2_object)
                }
                CanvasEntitySelection::Sprite => {
                    format!("Selected sprite token {}", self.selected_sprite)
                }
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(egui::Color32::YELLOW, description);
                if ui.button("Duplicate selected").clicked() {
                    toolbar_shortcut = Some(CanvasEntityShortcut::Duplicate);
                }
                if ui.button("Delete selected").clicked() {
                    toolbar_shortcut = Some(CanvasEntityShortcut::Remove);
                }
            });
        }
        if let Some(shortcut) = toolbar_shortcut {
            self.apply_canvas_entity_shortcut(shortcut);
        }
        let canvas_available = ui.available_size();
        let cell = if snes_viewport {
            fitted_snes_viewport_cell(canvas_available, self.canvas_zoom_percent())
        } else {
            self.canvas_cell()
        };
        let world_size = rom_canvas_size(major_tiles, minor_tiles, vertical, cell);
        let canvas_size = if is_boss_battle_level_mode(level_mode) {
            // Lunar Magic keeps its 512-row diagnostic plane but exposes a
            // 656-pixel-wide editor DIB, repeating the plane horizontally.
            egui::vec2(656.0, 512.0)
        } else if snes_viewport {
            // The editing surface follows the pane itself on every native window resize. Keep
            // square SNES pixels and reveal the surplus edge of the level on the non-limiting
            // axis instead of leaving a fixed-size 256×224 rectangle letterboxed inside it.
            canvas_available
        } else {
            tiled_surface_canvas_size(world_size, canvas_available)
        };
        let audit_scroll = visual_smoke_editor_scroll_column().is_some()
            || visual_smoke_editor_scroll_row().is_some();
        let scroll_id = if audit_scroll {
            "vanilla-rom-level-canvas-audit"
        } else {
            "vanilla-rom-level-canvas"
        };
        let mut scroll_area = egui::ScrollArea::both()
            .id_salt(scroll_id)
            .max_height(ui.available_height().max(160.0))
            .auto_shrink([false, false]);
        let requested_horizontal_scroll =
            visual_smoke_editor_scroll_column().map(|column| f32::from(column) * cell);
        let requested_vertical_scroll = visual_smoke_editor_scroll_row()
            .or(self.initial_vertical_scroll_tiles)
            .map(|row| f32::from(row) * cell);
        if !snes_viewport && let Some(offset) = requested_horizontal_scroll {
            scroll_area = scroll_area.horizontal_scroll_offset(offset);
        }
        if !snes_viewport && let Some(offset) = requested_vertical_scroll {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
        let mut paint_canvas = |ui: &mut egui::Ui| {
            let (rect, response) =
                ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let paint_rect = if snes_viewport {
                let (origin_x, origin_y) =
                    self.game_preview_camera_origin(major_tiles, minor_tiles, vertical);
                let rendered_viewport = egui::vec2(16.0 * cell, 14.0 * cell);
                let centered_crop = egui::vec2(
                    ((rendered_viewport.x - rect.width()) * 0.5).max(0.0),
                    ((rendered_viewport.y - rect.height()) * 0.5).max(0.0),
                );
                egui::Rect::from_min_size(
                    rect.min
                        - egui::vec2(f32::from(origin_x) * cell, f32::from(origin_y) * cell)
                        - centered_crop,
                    world_size,
                )
            } else {
                egui::Rect::from_min_size(rect.min, world_size)
            };
            self.canvas_geometry = Some(LevelCanvasGeometry {
                rect: paint_rect,
                cell,
                major_tiles,
                minor_tiles,
                vertical,
            });
            if is_boss_battle_level_mode(level_mode) {
                paint_boss_battle_diagnostic(&painter, rect);
            } else {
                painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
                if !snes_viewport {
                    let tiled_origin = if vertical {
                        egui::pos2(paint_rect.max.x, paint_rect.min.y)
                    } else {
                        egui::pos2(paint_rect.min.x, paint_rect.max.y)
                    };
                    toolbar_images.paint_tiled_surface(
                        &painter,
                        OriginalTiledImage::LevelCanvas,
                        rect,
                        tiled_origin,
                    );
                }
                self.paint_object_canvas(
                    &painter,
                    &response,
                    paint_rect,
                    cell,
                    major_tiles,
                    minor_tiles,
                    vertical,
                    level_mode,
                    object_tileset,
                    map16_animation_phase,
                    animation_phase,
                    visibility,
                    &layer2_records,
                    &layer2_placements,
                    &layer2_tilemap,
                    &records,
                    &placements,
                    &sprite_placements,
                    custom_sprites,
                    custom_objects,
                    custom_map16,
                );
                if snes_viewport && let Some((texture, size)) = live_frame {
                    let live_rect = live_frame_rect(rect, size, cell);
                    painter.image(
                        texture,
                        live_rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    if self.draw_selection_over_live() {
                        self.paint_selection_over_live_frame(
                            &painter,
                            paint_rect,
                            cell,
                            major_tiles,
                            minor_tiles,
                            vertical,
                            level_mode,
                            map16_animation_phase,
                            animation_phase,
                            visibility,
                            &layer2_records,
                            &layer2_placements,
                            &records,
                            &placements,
                            &sprite_placements,
                            custom_sprites,
                            custom_objects,
                            custom_map16,
                        );
                    }
                }
            }
        };
        if snes_viewport {
            // Paint directly so scroll-area frame padding cannot become a visible bezel.
            paint_canvas(ui);
            return;
        }
        let scroll_output = scroll_area.show(ui, paint_canvas);
        if !snes_viewport && let Some(requested) = requested_vertical_scroll {
            let target = clamped_scroll_offset(
                requested,
                scroll_output.content_size.y,
                scroll_output.inner_rect.height(),
            );
            if (scroll_output.state.offset.y - target).abs() < 0.5 {
                self.initial_vertical_scroll_tiles = None;
            }
        }
        draw_canvas_caption(ui, vertical);
    }

    fn show_canvas_tools(
        &mut self,
        ui: &mut egui::Ui,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
        live_frame_available: bool,
    ) {
        // Keep the controls to one stable row. Wrapping made a horizontal window resize add or
        // remove toolbar rows, so the canvas height jumped independently of the window and looked
        // as though it was not following the native resize.
        ui.scope(|ui| {
            // macOS-style floating scrollbars are painted over their contents. In this compact
            // toolbar that put the horizontal scrollbar directly on top of buttons whenever the
            // window was narrow. Use a solid, permanently reserved scrollbar row here so controls
            // remain clickable and the canvas below retains a stable height while resizing.
            ui.style_mut().spacing.scroll.floating = false;
            egui::ScrollArea::horizontal()
                .id_salt("vanilla-level-canvas-tools")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut game_preview = self.game_preview();
                        if ui.toggle_value(&mut game_preview, "Game pixels").changed() {
                            self.game_preview = Some(game_preview);
                        }
                        if game_preview {
                            let mut snes_viewport = self.snes_viewport();
                            if ui
                                .toggle_value(&mut snes_viewport, "256×224 viewport")
                                .changed()
                            {
                                self.snes_viewport = Some(snes_viewport);
                            }
                            if snes_viewport {
                                if live_frame_available {
                                    let mut draw_selection = self.draw_selection_over_live();
                                    if ui
                                        .toggle_value(&mut draw_selection, "Selection over game")
                                        .on_hover_text(
                                            "Draw selected object and sprite tiles over the live emulator frame",
                                        )
                                        .changed()
                                    {
                                        self.draw_selection_over_live = Some(draw_selection);
                                    }
                                }
                                self.show_preview_camera_tools(
                                    ui,
                                    major_tiles,
                                    minor_tiles,
                                    vertical,
                                );
                            }
                        }
                        ui.separator();
                        ui.label("Canvas tool:");
                        ui.selectable_value(&mut self.placement_mode, None, "Select / move");
                        ui.selectable_value(
                            &mut self.placement_mode,
                            Some(CanvasPlacementMode::Object),
                            "Place object",
                        );
                        ui.selectable_value(
                            &mut self.placement_mode,
                            Some(CanvasPlacementMode::Sprite),
                            "Place sprite",
                        );
                        if matches!(
                            self.controller.as_ref().and_then(LevelController::layer2),
                            Some(lm_level::NativeLayer2Data::Tilemap(_))
                        ) && layer2_tilemap_editable(self.shared_vanilla_background)
                        {
                            ui.selectable_value(
                                &mut self.placement_mode,
                                Some(CanvasPlacementMode::Layer2Tile),
                                "Paint Layer 2 tile",
                            );
                        } else if matches!(
                            self.controller.as_ref().and_then(LevelController::layer2),
                            Some(lm_level::NativeLayer2Data::Objects(_))
                        ) {
                            ui.selectable_value(
                                &mut self.placement_mode,
                                Some(CanvasPlacementMode::Layer2Object),
                                "Place Layer 2 object",
                            );
                        }
                        ui.separator();
                        ui.label("Zoom:");
                        let mut zoom = self.canvas_zoom_percent();
                        if ui.small_button("−").clicked() {
                            zoom = zoom.saturating_sub(ROM_LEVEL_CANVAS_ZOOM_STEP);
                        }
                        let slider = egui::Slider::new(
                            &mut zoom,
                            ROM_LEVEL_CANVAS_MIN_ZOOM..=ROM_LEVEL_CANVAS_MAX_ZOOM,
                        )
                        .suffix("%")
                        .step_by(f64::from(ROM_LEVEL_CANVAS_ZOOM_STEP));
                        ui.add(slider);
                        if ui.small_button("Reset").clicked() {
                            zoom = 100;
                        }
                        if ui.small_button("+").clicked() {
                            zoom = zoom.saturating_add(ROM_LEVEL_CANVAS_ZOOM_STEP);
                        }
                        let zoom = clamp_canvas_zoom(zoom);
                        self.canvas_zoom_percent = Some(zoom);
                        if zoom != 100 {
                            self.canvas_previous_zoom_percent = Some(zoom);
                        }
                    })
                });
        });
    }

    fn show_preview_camera_tools(
        &mut self,
        ui: &mut egui::Ui,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
    ) {
        ui.separator();
        ui.label("Camera:");
        for (label, major_delta, minor_delta) in [
            ("Screen −", -16, 0),
            ("←", -1, 0),
            ("→", 1, 0),
            ("Screen +", 16, 0),
            ("↑", 0, -1),
            ("↓", 0, 1),
        ] {
            if ui.small_button(label).clicked() {
                self.preview_camera_major_offset =
                    self.preview_camera_major_offset.saturating_add(major_delta);
                self.preview_camera_minor_offset =
                    self.preview_camera_minor_offset.saturating_add(minor_delta);
            }
        }
        if ui.small_button("Entrance").clicked() {
            self.preview_camera_major_offset = 0;
            self.preview_camera_minor_offset = 0;
        }
        let (x, y) = self.game_preview_camera_origin(major_tiles, minor_tiles, vertical);
        ui.monospace(format!("({x},{y})"));
    }

    fn canvas_zoom_percent(&self) -> u16 {
        clamp_canvas_zoom(self.canvas_zoom_percent.unwrap_or(100))
    }

    fn zoom_filter(&self) -> bool {
        self.zoom_filter.unwrap_or(true)
    }

    pub(crate) fn toolbar_zoom_popup(&mut self) {
        self.zoom_popup_open = true;
    }

    pub(crate) fn toolbar_open_tool_panel(&mut self, panel: LevelToolPanel) {
        self.tools_panel_visible = Some(true);
        let generation = &mut self.tool_panel_generations[panel.index()];
        *generation = generation.wrapping_add(1);
        self.requested_tool_panel = Some(panel);
    }

    /// Mirrors Lunar Magic command `$26FF`: resolve the current mouse cell against the level
    /// canvas, preselect that source screen, and open the same complete editor as `$2523`.
    pub(crate) fn toolbar_open_screen_exit_at_pointer(&mut self, context: &egui::Context) -> bool {
        let Some(position) = context.pointer_hover_pos() else {
            return false;
        };
        let Some(geometry) = self.canvas_geometry else {
            return false;
        };
        let Some(screen) = screen_at_canvas_position(position, geometry) else {
            return false;
        };
        self.screen_exit_table_selected = Some(screen);
        self.toolbar_open_tool_panel(LevelToolPanel::ScreenExits);
        true
    }

    /// Mirrors Lunar Magic 3.63's four entrance-view commands. The aggregate command owns an
    /// independent check state (`DAT_005e7b0e`) and writes all three renderer flags; changing an
    /// individual flag does not recompute that aggregate state.
    pub(crate) fn toolbar_toggle_entrance_overlay(&mut self, toggle: EntranceOverlayToggle) {
        match toggle {
            EntranceOverlayToggle::All => {
                let visible = !self.entrance_overlay_visibility.all;
                self.entrance_overlay_visibility = EntranceOverlayVisibility {
                    all: visible,
                    primary: visible,
                    secondary: visible,
                    midway: visible,
                };
            }
            EntranceOverlayToggle::Primary => {
                self.entrance_overlay_visibility.primary =
                    !self.entrance_overlay_visibility.primary;
            }
            EntranceOverlayToggle::Secondary => {
                self.entrance_overlay_visibility.secondary =
                    !self.entrance_overlay_visibility.secondary;
            }
            EntranceOverlayToggle::Midway => {
                self.entrance_overlay_visibility.midway = !self.entrance_overlay_visibility.midway;
            }
        }
    }

    pub(crate) fn toolbar_place_object(&mut self) {
        self.tools_panel_visible = Some(true);
        self.placement_mode = Some(CanvasPlacementMode::Object);
        self.error = None;
    }

    pub(crate) fn toolbar_place_sprite(&mut self) {
        self.tools_panel_visible = Some(true);
        self.placement_mode = Some(CanvasPlacementMode::Sprite);
        self.error = None;
    }

    pub(crate) fn toolbar_select_all(&mut self) {
        self.apply_canvas_entity_shortcut(CanvasEntityShortcut::SelectAll);
    }

    pub(crate) fn toolbar_delete_selection(&mut self) {
        self.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
    }

    pub(crate) fn toolbar_delete_all(&mut self) {
        self.toolbar_select_all();
        self.toolbar_delete_selection();
    }

    pub(crate) fn toolbar_escape(&mut self) {
        self.placement_mode = None;
        self.canvas_entity_selection = None;
        self.selected_object_group.clear();
        self.selected_layer2_object_group.clear();
        self.selected_sprite_group.clear();
        self.dragging_object = None;
        self.dragging_layer2_object = None;
        self.dragging_sprite = None;
        self.resizing_object = None;
        self.resizing_layer2_object = None;
        self.object_group_drag = None;
        self.secondary_duplicate_drag = false;
        self.error = None;
    }

    pub(crate) fn toolbar_edit_layer1(&mut self) {
        self.tools_panel_visible = Some(true);
        self.placement_mode = None;
        let selected = self.controller.as_ref().and_then(|controller| {
            controller
                .level()
                .layer1
                .objects
                .native_placements()
                .into_iter()
                .map(|placement| placement.record_index)
                .find(|index| *index == self.selected_object)
                .or_else(|| {
                    controller
                        .level()
                        .layer1
                        .objects
                        .native_placements()
                        .first()
                        .map(|placement| placement.record_index)
                })
        });
        self.selected_object_group.clear();
        if let Some(selected) = selected {
            self.selected_object = selected;
            self.selected_object_group.push(selected);
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
            self.reload_object_form();
        } else {
            self.canvas_entity_selection = None;
        }
        self.error = None;
    }

    pub(crate) fn toolbar_edit_layer2(&mut self) {
        self.tools_panel_visible = Some(true);
        self.placement_mode = None;
        let selected = self.controller.as_ref().and_then(|controller| {
            let lm_level::NativeLayer2Data::Objects(layer2) = controller.layer2()? else {
                return None;
            };
            layer2
                .objects
                .native_placements()
                .into_iter()
                .map(|placement| placement.record_index)
                .find(|index| *index == self.selected_layer2_object)
                .or_else(|| {
                    layer2
                        .objects
                        .native_placements()
                        .first()
                        .map(|placement| placement.record_index)
                })
        });
        self.selected_layer2_object_group.clear();
        if let Some(selected) = selected {
            self.selected_layer2_object = selected;
            self.selected_layer2_object_group.push(selected);
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
            self.reload_layer2_object_form();
        } else {
            self.canvas_entity_selection = None;
        }
        self.error = None;
    }

    pub(crate) fn toolbar_edit_sprites(&mut self) {
        self.tools_panel_visible = Some(true);
        self.placement_mode = None;
        let selected = self.controller.as_ref().and_then(|controller| {
            controller
                .level()
                .sprites
                .native_placements()
                .into_iter()
                .map(|placement| placement.token_index)
                .find(|index| *index == self.selected_sprite)
                .or_else(|| {
                    controller
                        .level()
                        .sprites
                        .native_placements()
                        .first()
                        .map(|placement| placement.token_index)
                })
        });
        self.selected_sprite_group.clear();
        if let Some(selected) = selected {
            self.selected_sprite = selected;
            self.selected_sprite_group.push(selected);
            self.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
            if let Some(controller) = self.controller.as_ref() {
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
            }
        } else {
            self.canvas_entity_selection = None;
        }
        self.error = None;
    }

    pub(crate) fn toolbar_copy_selection(&mut self) -> Result<String, String> {
        let selection = self
            .canvas_entity_selection
            .ok_or_else(|| "Copy requires an object or sprite canvas selection".to_owned())?;
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| "level controller is unavailable".to_owned())?;
        match selection {
            CanvasEntitySelection::Layer1Object => {
                let indexes = selected_indexes(&self.selected_object_group, self.selected_object);
                let records = indexes
                    .into_iter()
                    .map(|index| {
                        controller
                            .level()
                            .layer1
                            .objects
                            .records
                            .get(index)
                            .cloned()
                            .ok_or_else(|| {
                                format!("selected Layer 1 object {index} is unavailable")
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                crate::native_clipboard::encode_level_objects(&records)
            }
            CanvasEntitySelection::Layer2Object => {
                let lm_level::NativeLayer2Data::Objects(layer2) = controller
                    .layer2()
                    .ok_or_else(|| "the current level has no Layer 2 data".to_owned())?
                else {
                    return Err("the current level does not use object-backed Layer 2".into());
                };
                let indexes = selected_indexes(
                    &self.selected_layer2_object_group,
                    self.selected_layer2_object,
                );
                let records = indexes
                    .into_iter()
                    .map(|index| {
                        layer2.objects.records.get(index).cloned().ok_or_else(|| {
                            format!("selected Layer 2 object {index} is unavailable")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                crate::native_clipboard::encode_level_objects(&records)
            }
            CanvasEntitySelection::Sprite => {
                let indexes = selected_indexes(&self.selected_sprite_group, self.selected_sprite);
                let records = indexes
                    .into_iter()
                    .map(|index| match controller.level().sprites.tokens.get(index) {
                        Some(SpriteToken::Record(record)) => Ok(record.clone()),
                        Some(SpriteToken::Screen(_) | SpriteToken::Control(_)) => Err(format!(
                            "selected sprite token {index} is not a sprite record"
                        )),
                        None => Err(format!("selected sprite token {index} is unavailable")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                crate::native_clipboard::encode_level_sprites(&records)
            }
        }
    }

    pub(crate) fn toolbar_cut_selection(&mut self) -> Result<String, String> {
        let text = self.toolbar_copy_selection()?;
        self.toolbar_delete_selection();
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        Ok(text)
    }

    pub(crate) fn toolbar_request_paste(&mut self, context: &egui::Context) {
        self.paste_target = match self.canvas_entity_selection {
            Some(CanvasEntitySelection::Layer1Object) => Some(EntityPasteTarget::Object),
            Some(CanvasEntitySelection::Layer2Object) => Some(EntityPasteTarget::Layer2Object),
            Some(CanvasEntitySelection::Sprite) => Some(EntityPasteTarget::Sprite),
            None => {
                self.error =
                    Some("Paste requires an active object or sprite editing domain".into());
                return;
            }
        };
        self.error = None;
        context.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }

    /// Moves the active selection in screen-space tiles. Vertical levels swap the native stream's
    /// major/minor axes, but Lunar Magic's coordinate commands remain X/Y oriented.
    pub(crate) fn toolbar_nudge_selection(&mut self, x_delta: i32, y_delta: i32) {
        let Some(domain) = self.canvas_entity_selection else {
            self.error =
                Some("Coordinate adjustment requires an object or sprite selection".into());
            return;
        };
        let Some(controller) = self.controller.as_ref() else {
            self.error = Some("level controller is unavailable".into());
            return;
        };
        let vertical =
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                .vertical;
        let (major_delta, minor_delta) = screen_nudge_delta(vertical, x_delta, y_delta);
        if major_delta == 0 && minor_delta == 0 {
            return;
        }
        match domain {
            CanvasEntitySelection::Layer1Object => {
                let selected = selected_indexes(&self.selected_object_group, self.selected_object);
                self.relocate_selected_objects(domain, selected, major_delta, minor_delta);
            }
            CanvasEntitySelection::Layer2Object => {
                let selected = selected_indexes(
                    &self.selected_layer2_object_group,
                    self.selected_layer2_object,
                );
                self.relocate_selected_objects(domain, selected, major_delta, minor_delta);
            }
            CanvasEntitySelection::Sprite => {
                let selected = selected_indexes(&self.selected_sprite_group, self.selected_sprite);
                let controller = self
                    .controller
                    .as_mut()
                    .expect("controller presence checked above");
                let mut predicted = controller.level().sprites.clone();
                let moved = match predicted.relocate_record_group(
                    &selected,
                    major_delta,
                    minor_delta,
                    vertical,
                    controller.sprite_lengths(),
                ) {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                let result = controller.apply_edits(&[NativeLevelEdit::RelocateSpriteGroup {
                    selected: selected.clone(),
                    major_delta,
                    minor_delta,
                }]);
                match result {
                    Ok(()) => {
                        self.selected_sprite_group = moved;
                        self.selected_sprite = self.selected_sprite_group[0];
                        if let Some(controller) = self.controller.as_ref() {
                            self.sprite_form = SpriteForm::from_token(
                                controller.level().sprites.header,
                                controller.level().sprites.tokens.get(self.selected_sprite),
                            );
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
    }

    fn relocate_selected_objects(
        &mut self,
        domain: CanvasEntitySelection,
        selected: Vec<usize>,
        major_delta: i32,
        minor_delta: i32,
    ) {
        let edit = ObjectEdit::RelocateOrdinaryGroup {
            selected: selected.clone(),
            major_delta,
            minor_delta,
        };
        let controller = self
            .controller
            .as_mut()
            .expect("toolbar nudge requires a loaded controller");
        let moved = match domain {
            CanvasEntitySelection::Layer1Object => {
                let mut predicted = controller.level().layer1.objects.clone();
                predicted.relocate_ordinary_object_group(&selected, major_delta, minor_delta)
            }
            CanvasEntitySelection::Layer2Object => {
                let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
                    self.error =
                        Some("the current level does not use object-backed Layer 2".into());
                    return;
                };
                let mut predicted = layer2.objects.clone();
                predicted.relocate_ordinary_object_group(&selected, major_delta, minor_delta)
            }
            CanvasEntitySelection::Sprite => unreachable!("sprite nudge has a dedicated edit"),
        };
        let moved = match moved {
            Ok(moved) => moved,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let result = match domain {
            CanvasEntitySelection::Layer1Object => {
                controller.apply_edits(&[NativeLevelEdit::Objects(vec![edit])])
            }
            CanvasEntitySelection::Layer2Object => controller.apply_layer2_object_edits(&[edit]),
            CanvasEntitySelection::Sprite => unreachable!("sprite nudge has a dedicated edit"),
        };
        match result {
            Ok(()) => {
                match domain {
                    CanvasEntitySelection::Layer1Object => {
                        self.selected_object_group = moved;
                        self.selected_object = self.selected_object_group[0];
                        self.reload_object_form();
                    }
                    CanvasEntitySelection::Layer2Object => {
                        self.selected_layer2_object_group = moved;
                        self.selected_layer2_object = self.selected_layer2_object_group[0];
                        self.reload_layer2_object_form();
                    }
                    CanvasEntitySelection::Sprite => unreachable!(),
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Applies Lunar Magic's legacy Increase/Decrease Z Order operation: one stable creation-order
    /// step, independent of overlap. The newer forward/back commands use different overlap-aware
    /// semantics and intentionally do not route here.
    pub(crate) fn toolbar_z_order_step(&mut self, increase: bool) {
        let Some(domain) = self.canvas_entity_selection else {
            self.error = Some("Z-order adjustment requires an object or sprite selection".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("level controller is unavailable".into());
            return;
        };
        match domain {
            CanvasEntitySelection::Layer1Object => {
                let selected = selected_indexes(&self.selected_object_group, self.selected_object);
                let mut predicted = controller.level().layer1.objects.clone();
                let moved = predicted.adjust_ordinary_object_z_order(&selected, increase);
                let moved = match moved {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
                    ObjectEdit::AdjustOrdinaryZOrder { selected, increase },
                ])]) {
                    Ok(()) => {
                        self.selected_object_group = moved;
                        self.selected_object = self.selected_object_group[0];
                        self.reload_object_form();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            CanvasEntitySelection::Layer2Object => {
                let selected = selected_indexes(
                    &self.selected_layer2_object_group,
                    self.selected_layer2_object,
                );
                let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
                    self.error =
                        Some("the current level does not use object-backed Layer 2".into());
                    return;
                };
                let mut predicted = layer2.objects.clone();
                let moved = match predicted.adjust_ordinary_object_z_order(&selected, increase) {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                match controller.apply_layer2_object_edits(&[ObjectEdit::AdjustOrdinaryZOrder {
                    selected,
                    increase,
                }]) {
                    Ok(()) => {
                        self.selected_layer2_object_group = moved;
                        self.selected_layer2_object = self.selected_layer2_object_group[0];
                        self.reload_layer2_object_form();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            CanvasEntitySelection::Sprite => {
                let selected = selected_indexes(&self.selected_sprite_group, self.selected_sprite);
                let vertical =
                    lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                        .vertical;
                let mut predicted = controller.level().sprites.clone();
                let moved = match predicted.adjust_record_z_order(&selected, increase, vertical) {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                match controller
                    .apply_edits(&[NativeLevelEdit::AdjustSpriteZOrder { selected, increase }])
                {
                    Ok(()) => {
                        self.selected_sprite_group = moved;
                        self.selected_sprite = self.selected_sprite_group[0];
                        self.sprite_form = SpriteForm::from_token(
                            controller.level().sprites.header,
                            controller.level().sprites.tokens.get(self.selected_sprite),
                        );
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
    }

    pub(crate) fn toolbar_overlap_z_order(&mut self, traversal: ZOrderTraversal) {
        let Some(domain) = self.canvas_entity_selection else {
            self.error = Some("Z-order adjustment requires an object or sprite selection".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("level controller is unavailable".into());
            return;
        };
        match domain {
            CanvasEntitySelection::Layer1Object | CanvasEntitySelection::Layer2Object => {
                let (selected, bounds, stream) = match domain {
                    CanvasEntitySelection::Layer1Object => (
                        selected_indexes(&self.selected_object_group, self.selected_object),
                        &self.layer1_z_order_bounds,
                        &controller.level().layer1.objects,
                    ),
                    CanvasEntitySelection::Layer2Object => {
                        let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2()
                        else {
                            self.error =
                                Some("the current level does not use object-backed Layer 2".into());
                            return;
                        };
                        (
                            selected_indexes(
                                &self.selected_layer2_object_group,
                                self.selected_layer2_object,
                            ),
                            &self.layer2_z_order_bounds,
                            &layer2.objects,
                        )
                    }
                    CanvasEntitySelection::Sprite => unreachable!(),
                };
                let order = stream
                    .native_placements()
                    .into_iter()
                    .map(|placement| placement.record_index)
                    .collect::<Vec<_>>();
                let reordered = match overlap_z_order_permutation(
                    &order,
                    &selected,
                    bounds,
                    traversal,
                    |_, _| true,
                ) {
                    Ok(reordered) => reordered,
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                };
                if reordered == order {
                    self.error = None;
                    return;
                }
                let mut predicted = stream.clone();
                let moved = match predicted.reorder_ordinary_objects(&reordered, &selected) {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                let edit = ObjectEdit::ReorderOrdinaryZOrder {
                    order: reordered,
                    selected,
                };
                let result = match domain {
                    CanvasEntitySelection::Layer1Object => {
                        controller.apply_edits(&[NativeLevelEdit::Objects(vec![edit])])
                    }
                    CanvasEntitySelection::Layer2Object => {
                        controller.apply_layer2_object_edits(&[edit])
                    }
                    CanvasEntitySelection::Sprite => unreachable!(),
                };
                match result {
                    Ok(()) => {
                        match domain {
                            CanvasEntitySelection::Layer1Object => {
                                self.selected_object_group = moved;
                                self.selected_object = self.selected_object_group[0];
                                self.reload_object_form();
                            }
                            CanvasEntitySelection::Layer2Object => {
                                self.selected_layer2_object_group = moved;
                                self.selected_layer2_object = self.selected_layer2_object_group[0];
                                self.reload_layer2_object_form();
                            }
                            CanvasEntitySelection::Sprite => unreachable!(),
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            CanvasEntitySelection::Sprite => {
                let selected = selected_indexes(&self.selected_sprite_group, self.selected_sprite);
                let vertical =
                    lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                        .vertical;
                let expanded = controller.level().sprites.expanded;
                let placements = controller.level().sprites.native_placements();
                let groups = placements
                    .iter()
                    .map(|placement| {
                        (
                            placement.token_index,
                            (
                                placement.screen,
                                if expanded { placement.minor / 32 } else { 0 },
                                if expanded && vertical {
                                    placement.minor & 0x0f
                                } else {
                                    0
                                },
                            ),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                let order = placements
                    .iter()
                    .map(|placement| placement.token_index)
                    .collect::<Vec<_>>();
                let reordered = match overlap_z_order_permutation(
                    &order,
                    &selected,
                    &self.sprite_z_order_bounds,
                    traversal,
                    |left, right| groups.get(left) == groups.get(right),
                ) {
                    Ok(reordered) => reordered,
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                };
                if reordered == order {
                    self.error = None;
                    return;
                }
                let mut predicted = controller.level().sprites.clone();
                let moved =
                    match predicted.reorder_records_for_z_order(&reordered, &selected, vertical) {
                        Ok(moved) => moved,
                        Err(error) => {
                            self.error = Some(error.to_string());
                            return;
                        }
                    };
                match controller.apply_edits(&[NativeLevelEdit::ReorderSpriteZOrder {
                    order: reordered,
                    selected,
                }]) {
                    Ok(()) => {
                        self.selected_sprite_group = moved;
                        self.selected_sprite = self.selected_sprite_group[0];
                        self.sprite_form = SpriteForm::from_token(
                            controller.level().sprites.header,
                            controller.level().sprites.tokens.get(self.selected_sprite),
                        );
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
    }

    pub(crate) fn toolbar_zoom_filter_toggle(&mut self) {
        self.zoom_filter = Some(!self.zoom_filter());
        self.invalidate_graphics_preview();
    }

    fn toolbar_zoom_set(&mut self, percent: u16) {
        let percent = clamp_canvas_zoom(percent);
        self.canvas_zoom_percent = Some(percent);
        if percent != 100 {
            self.canvas_previous_zoom_percent = Some(percent);
        }
    }

    fn show_zoom_popup(&mut self, context: &egui::Context) {
        if !self.zoom_popup_open {
            return;
        }
        let mut open = true;
        let mut selected = None;
        let mut delta = 0_i16;
        let mut toggle_filter = false;
        let filter_enabled = self.zoom_filter();
        egui::Window::new("Zoom")
            .id(egui::Id::new("lunar-magic-level-zoom-popup"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                for percent in ROM_LEVEL_CANVAS_ZOOM_MENU {
                    if ui
                        .selectable_label(
                            self.canvas_zoom_percent() == percent,
                            format!("{percent}%"),
                        )
                        .clicked()
                    {
                        selected = Some(percent);
                    }
                }
                ui.separator();
                if ui.button("Zoom in").clicked() {
                    delta = i16::try_from(ROM_LEVEL_CANVAS_ZOOM_STEP).unwrap_or(100);
                }
                if ui.button("Zoom out").clicked() {
                    delta = -i16::try_from(ROM_LEVEL_CANVAS_ZOOM_STEP).unwrap_or(100);
                }
                ui.separator();
                let mut filter = filter_enabled;
                if ui.checkbox(&mut filter, "Zoom Filter").clicked() {
                    toggle_filter = true;
                }
            });
        if let Some(percent) = selected {
            self.toolbar_zoom_set(percent);
            open = false;
        } else if delta != 0 {
            self.toolbar_zoom_adjust(delta);
            open = false;
        }
        if toggle_filter {
            self.toolbar_zoom_filter_toggle();
            open = false;
        }
        self.zoom_popup_open = open;
    }

    pub(crate) fn toolbar_zoom_toggle(&mut self) {
        let current = self.canvas_zoom_percent();
        if current == 100 {
            self.canvas_zoom_percent = Some(clamp_canvas_zoom(
                self.canvas_previous_zoom_percent
                    .unwrap_or(ROM_LEVEL_CANVAS_INITIAL_PREVIOUS_ZOOM),
            ));
        } else {
            self.canvas_previous_zoom_percent = Some(current);
            self.canvas_zoom_percent = Some(100);
        }
    }

    pub(crate) fn toolbar_zoom_default(&mut self) {
        let current = self.canvas_zoom_percent();
        if current != 100 {
            self.canvas_previous_zoom_percent = Some(current);
        }
        self.canvas_zoom_percent = Some(100);
    }

    pub(crate) fn toolbar_zoom_adjust(&mut self, delta: i16) {
        let current = i32::from(self.canvas_zoom_percent());
        let next = current.saturating_add(i32::from(delta)).clamp(
            i32::from(ROM_LEVEL_CANVAS_MIN_ZOOM),
            i32::from(ROM_LEVEL_CANVAS_MAX_ZOOM),
        ) as u16;
        self.canvas_zoom_percent = Some(next);
        if next != 100 {
            self.canvas_previous_zoom_percent = Some(next);
        }
    }

    pub(crate) fn animation_playing(&self) -> bool {
        self.animation_playing.unwrap_or(true)
    }

    fn animation_seconds(&mut self, wall_seconds: f64) -> f64 {
        self.animation_last_wall_seconds = wall_seconds;
        if self.animation_playing() {
            wall_seconds + self.animation_time_offset_seconds
        } else {
            self.animation_frozen_seconds
        }
    }

    pub(crate) fn toolbar_animation_toggle(&mut self) {
        if self.animation_playing() {
            self.animation_frozen_seconds =
                self.animation_last_wall_seconds + self.animation_time_offset_seconds;
            self.animation_playing = Some(false);
        } else {
            self.animation_time_offset_seconds =
                self.animation_frozen_seconds - self.animation_last_wall_seconds;
            self.animation_playing = Some(true);
        }
    }

    pub(crate) fn toolbar_animation_step(&mut self) {
        if self.animation_playing() {
            self.animation_time_offset_seconds += LUNAR_MAGIC_ANIMATION_TICK_SECONDS;
        } else {
            self.animation_frozen_seconds += LUNAR_MAGIC_ANIMATION_TICK_SECONDS;
        }
    }

    pub(crate) fn toolbar_animation_reset(&mut self) {
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_switch_view_toggle(&mut self, switch: u8) {
        match switch {
            0 => self.switch_view_state.green = !self.switch_view_state.green,
            1 => self.switch_view_state.yellow = !self.switch_view_state.yellow,
            2 => self.switch_view_state.blue = !self.switch_view_state.blue,
            3 => self.switch_view_state.red = !self.switch_view_state.red,
            _ => unreachable!("the toolbar exposes exactly four switch-state commands"),
        }
    }

    pub(crate) fn toolbar_silver_pow_toggle(&mut self) {
        self.silver_pow_active = !self.silver_pow_active;
    }

    pub(crate) fn toolbar_blue_pow_toggle(&mut self) {
        self.blue_pow_active = !self.blue_pow_active;
    }

    pub(crate) fn toolbar_invisible_pow_objects_toggle(&mut self) {
        self.conditional_view_state.invisible_pow_objects =
            !self.conditional_view_state.invisible_pow_objects;
    }

    pub(crate) fn toolbar_other_invisible_objects_toggle(&mut self) {
        self.conditional_view_state.other_invisible_objects =
            !self.conditional_view_state.other_invisible_objects;
    }

    pub(crate) fn toolbar_on_off_switch_toggle(&mut self) {
        self.conditional_view_state.on_off_switch_on =
            !self.conditional_view_state.on_off_switch_on;
    }

    pub(crate) fn toolbar_conditional_direct_map16_toggle(&mut self) {
        self.conditional_view_state.conditional_direct_map16 =
            !self.conditional_view_state.conditional_direct_map16;
    }

    pub(crate) fn toolbar_block_contents_toggle(&mut self) {
        self.conditional_view_state.block_contents = !self.conditional_view_state.block_contents;
    }

    pub(crate) fn toolbar_block_exits_toggle(&mut self) {
        self.conditional_view_state.block_exits = !self.conditional_view_state.block_exits;
    }

    pub(crate) fn toolbar_have_star_toggle(&mut self) {
        self.conditional_view_state.have_star = !self.conditional_view_state.have_star;
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_time_100_toggle(&mut self) {
        self.conditional_view_state.time_100 = !self.conditional_view_state.time_100;
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_five_yoshi_coins_toggle(&mut self) {
        self.conditional_view_state.five_yoshi_coins =
            !self.conditional_view_state.five_yoshi_coins;
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_custom_trigger_toggle(&mut self, trigger: u8) {
        let state = &mut self.exanimation_trigger_view_state.custom[usize::from(trigger & 0x0f)];
        *state = !*state;
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_one_shot_trigger_toggle(&mut self, trigger: u8) {
        let state = &mut self.exanimation_trigger_view_state.one_shot[usize::from(trigger & 0x1f)];
        *state = !*state;
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_manual_trigger_adjust(&mut self, trigger: u8, delta: i8) {
        let frame =
            &mut self.exanimation_trigger_view_state.manual_frames[usize::from(trigger & 0x0f)];
        *frame = frame.wrapping_add_signed(delta);
        self.invalidate_graphics_preview();
    }

    pub(crate) fn toolbar_trigger_selection_adjust(&mut self, family: u8, delta: i8) {
        match family {
            0 => self.exanimation_trigger_view_state.select_custom(delta),
            1 => self.exanimation_trigger_view_state.select_one_shot(delta),
            2 => self.exanimation_trigger_view_state.select_manual(delta),
            _ => unreachable!("the toolbar exposes exactly three trigger selectors"),
        }
    }

    pub(crate) fn toolbar_current_trigger_action(&mut self, family: u8, delta: i8) {
        match family {
            0 => self
                .toolbar_custom_trigger_toggle(self.exanimation_trigger_view_state.selected_custom),
            1 => self.toolbar_one_shot_trigger_toggle(
                self.exanimation_trigger_view_state.selected_one_shot,
            ),
            2 => self.toolbar_manual_trigger_adjust(
                self.exanimation_trigger_view_state.selected_manual,
                delta,
            ),
            _ => unreachable!("the toolbar exposes exactly three current-trigger actions"),
        }
    }

    pub(crate) fn toolbar_background_512_height_toggle(&mut self) {
        self.background_512_height = !self.background_512_height;
    }

    pub(crate) fn toolbar_translucent_overlays_toggle(&mut self) {
        self.translucent_overlays = !self.translucent_overlays;
    }

    fn game_preview_camera_origin(
        &self,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
    ) -> (u16, u16) {
        offset_game_preview_origin(
            game_preview_origin(self.entrance_form, major_tiles, minor_tiles, vertical),
            self.preview_camera_major_offset,
            self.preview_camera_minor_offset,
            major_tiles,
            minor_tiles,
            vertical,
        )
    }

    fn canvas_cell(&self) -> f32 {
        let base = if self.game_preview() {
            16.0
        } else {
            visual_smoke_editor_cell().unwrap_or(ROM_LEVEL_CANVAS_CELL)
        };
        base * f32::from(self.canvas_zoom_percent()) / 100.0
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn paint_selection_over_live_frame(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        cell: f32,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
        level_mode: u8,
        map16_animation_phase: u8,
        animation_phase: u8,
        visibility: crate::application::LevelViewVisibility,
        layer2_records: &[ObjectRecord],
        layer2_placements: &[lm_level::NativeObjectPlacement],
        records: &[ObjectRecord],
        placements: &[lm_level::NativeObjectPlacement],
        sprite_placements: &[lm_level::NativeSpritePlacement],
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let animation_index = usize::from(map16_animation_phase);
        let variant_start = animation_index * 4;
        let map16_variants = self
            .animated_map16_textures
            .get(variant_start..variant_start + 4);
        let map16_texture = map16_variants
            .and_then(|textures| textures.first())
            .or(self.map16_texture.as_ref());
        let layer2_variants = self
            .animated_layer2_map16_textures
            .get(variant_start..variant_start + 4);
        let layer2_texture = layer2_variants
            .and_then(|textures| textures.first())
            .or(self.layer2_map16_texture.as_ref())
            .or(map16_texture);
        let block_contents = self.block_contents_textures.get(animation_index);
        let object_minor_tiles = native_object_cache_minor_tiles(minor_tiles, vertical);
        let camera = self.game_preview_camera_origin(major_tiles, minor_tiles, vertical);
        let layer1_camera = (i32::from(camera.0) * 16, i32::from(camera.1) * 16);
        let layer2_camera = vanilla_layer2_camera_pixels(self.entrance_form, layer1_camera);
        let layer2_target = rect.translate(egui::vec2(
            screen_pixels_f32(layer1_camera.0 - layer2_camera.0) * cell / 16.0,
            screen_pixels_f32(layer1_camera.1 - layer2_camera.1) * cell / 16.0,
        ));

        if visibility.layer1
            && self.canvas_entity_selection == Some(CanvasEntitySelection::Layer1Object)
        {
            let selected = selected_object_placements(
                placements,
                &self.selected_object_group,
                self.selected_object,
            );
            let bounds = self.draw_object_artwork(
                painter,
                rect,
                cell,
                major_tiles,
                object_minor_tiles,
                vertical,
                records,
                &selected,
                custom_objects,
                custom_map16,
                map16_texture,
                map16_variants,
                block_contents,
                visibility.surface_outline,
                visibility.line_guide_outline,
            );
            draw_object_placement_markers(
                painter,
                None,
                rect,
                vertical,
                records,
                &selected,
                &self.selected_object_group,
                self.selected_object,
                map16_texture,
                &bounds,
                &self.active_object_resize_models(records, custom_objects),
                cell,
                false,
                true,
            );
        }
        if visibility.layer2
            && self.canvas_entity_selection == Some(CanvasEntitySelection::Layer2Object)
        {
            let selected = selected_object_placements(
                layer2_placements,
                &self.selected_layer2_object_group,
                self.selected_layer2_object,
            );
            let bounds = self.draw_object_artwork(
                painter,
                layer2_target,
                cell,
                major_tiles,
                object_minor_tiles,
                vertical,
                layer2_records,
                &selected,
                custom_objects,
                custom_map16,
                layer2_texture,
                layer2_variants,
                block_contents,
                visibility.surface_outline,
                visibility.line_guide_outline,
            );
            draw_object_placement_markers(
                painter,
                None,
                layer2_target,
                vertical,
                layer2_records,
                &selected,
                &self.selected_layer2_object_group,
                self.selected_layer2_object,
                layer2_texture,
                &bounds,
                &self.active_object_resize_models(layer2_records, custom_objects),
                cell,
                false,
                true,
            );
        }
        if visibility.sprites && self.canvas_entity_selection == Some(CanvasEntitySelection::Sprite)
        {
            let _ = draw_sprite_placements(SpritePlacementDraw {
                painter,
                overlay_painter: painter,
                target: rect,
                cell_size: cell,
                texture: self.sprite_texture.as_ref(),
                animated_texture: self
                    .animated_sprite_textures
                    .get(usize::from(animation_phase))
                    .or(self.sprite_texture.as_ref()),
                placements: sprite_placements,
                cursor: None,
                selected_group: &self.selected_sprite_group,
                selected: self.selected_sprite,
                vertical,
                level_mode,
                sprite_tileset: self.form.sprite_tileset,
                sprite_memory_index: self
                    .controller
                    .as_ref()
                    .map_or(0, |controller| controller.level().sprites.header & 0x3f),
                animation_phase,
                silver_pow_active: self.silver_pow_active,
                custom_sprites,
                custom_map16,
                external_textures: &self.external_sprite_textures,
                editor_overlays: false,
                selection_visible: true,
                selected_only: true,
            });
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn paint_object_canvas(
        &mut self,
        painter: &egui::Painter,
        response: &egui::Response,
        rect: egui::Rect,
        cell: f32,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
        level_mode: u8,
        object_tileset: u8,
        map16_animation_phase: u8,
        animation_phase: u8,
        visibility: crate::application::LevelViewVisibility,
        layer2_records: &[ObjectRecord],
        layer2_placements: &[lm_level::NativeObjectPlacement],
        layer2_tilemap: &[u16],
        records: &[ObjectRecord],
        placements: &[lm_level::NativeObjectPlacement],
        sprite_placements: &[lm_level::NativeSpritePlacement],
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        painter.rect_filled(rect, 0.0, canvas_background_color(self.canvas_backdrop));
        let animation_phase_index = usize::from(map16_animation_phase);
        let map16_variant_start = animation_phase_index * 4;
        let map16_texture_variants = self
            .animated_map16_textures
            .get(map16_variant_start..map16_variant_start + 4);
        let map16_texture = map16_texture_variants
            .and_then(|textures| textures.first())
            .or(self.map16_texture.as_ref());
        let block_contents_texture = self.block_contents_textures.get(animation_phase_index);
        let layer2_map16_texture_variants = self
            .animated_layer2_map16_textures
            .get(map16_variant_start..map16_variant_start + 4);
        let layer2_map16_texture = layer2_map16_texture_variants
            .and_then(|textures| textures.first())
            .or(self.layer2_map16_texture.as_ref())
            .or(map16_texture);
        let background_map16_texture = self
            .animated_background_map16_textures
            .get(animation_phase_index)
            .or(self.background_map16_texture.as_ref());
        let background_plane_texture = self
            .animated_background_plane_textures
            .get(animation_phase_index);
        let game_camera = (self.game_preview() && self.snes_viewport())
            .then(|| self.game_preview_camera_origin(major_tiles, minor_tiles, vertical));
        let layer3_position = self.layer3_position.map(|(x, y)| {
            if game_camera.is_some() {
                (x, y)
            } else {
                self.layer3_editor_row_offset
                    .map_or((x, y), |row| (x, row * 16))
            }
        });
        let layer3_camera = game_camera.or_else(|| {
            // Lunar Magic also composites ordinary static Layer 3 backgrounds in its full
            // editor canvas. Some effects need a recovered editor-row override, but the
            // absence of that override does not mean the decoded plane is inactive.
            (!self.game_preview() && layer3_position.is_some()).then(|| {
                (
                    visual_smoke_editor_scroll_column().unwrap_or_default(),
                    visual_smoke_editor_scroll_row()
                        .or(self.initial_vertical_scroll_tiles)
                        .unwrap_or_default(),
                )
            })
        });
        let layer2_target = game_camera.map_or(rect, |camera| {
            let layer1_x = i32::from(camera.0) * 16;
            let layer1_y = i32::from(camera.1) * 16;
            let (layer2_x, layer2_y) =
                vanilla_layer2_camera_pixels(self.entrance_form, (layer1_x, layer1_y));
            rect.translate(egui::vec2(
                screen_pixels_f32(layer1_x - layer2_x) * cell / 16.0,
                screen_pixels_f32(layer1_y - layer2_y) * cell / 16.0,
            ))
        });
        if visibility.layer3
            && !self.layer3_between_background_and_foreground
            && let (Some(texture), Some(position), Some(camera)) = (
                self.layer3_low_texture.as_ref(),
                layer3_position,
                layer3_camera,
            )
        {
            // Low-priority BG3 pixels sit behind BG2 in ordinary level compositing.
            draw_layer3_editor_or_viewport(
                painter,
                rect,
                cell,
                texture,
                position,
                camera,
                major_tiles,
                minor_tiles,
                vertical,
                game_camera.is_some(),
            );
        }
        if visibility.layer2 && visual_smoke_editor_layer2() {
            draw_layer2_tilemap(
                painter,
                if self.shared_vanilla_background {
                    rect
                } else {
                    layer2_target
                },
                cell,
                layer2_tilemap,
                map16_texture,
                map16_texture_variants,
                background_map16_texture,
                self.shared_vanilla_background
                    .then_some(())
                    .and(background_plane_texture),
                self.foreground_texture.as_ref(),
                custom_map16,
                object_tileset,
                self.entrance_form,
                major_tiles,
                minor_tiles,
                vertical,
                game_camera,
                self.background_512_height,
                self.outline_texture.as_ref(),
                visibility.surface_outline,
                visibility.line_guide_outline,
            );
        }
        if visibility.layer3
            && self.layer3_between_background_and_foreground
            && let (Some(position), Some(camera)) = (layer3_position, layer3_camera)
        {
            for texture in [
                self.layer3_low_texture.as_ref(),
                self.layer3_high_texture.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                draw_layer3_editor_or_viewport(
                    painter,
                    rect,
                    cell,
                    texture,
                    position,
                    camera,
                    major_tiles,
                    minor_tiles,
                    vertical,
                    game_camera.is_some(),
                );
            }
        }
        // The object cache uses SMW's 0x1B0-byte 16×27 screen pages. The 32×32 Layer 2 plane may
        // enlarge the visible canvas, but its final five rows are not object-cache coordinates.
        let object_minor_tiles = native_object_cache_minor_tiles(minor_tiles, vertical);
        let layer2_artwork_bounds = if visibility.layer2 {
            self.draw_object_artwork(
                painter,
                layer2_target,
                cell,
                major_tiles,
                object_minor_tiles,
                vertical,
                layer2_records,
                layer2_placements,
                custom_objects,
                custom_map16,
                layer2_map16_texture,
                layer2_map16_texture_variants,
                block_contents_texture,
                visibility.surface_outline,
                visibility.line_guide_outline,
            )
        } else {
            HashMap::new()
        };
        let game_preview = self.game_preview();
        let editor_overlays = !game_preview && visual_smoke_editor_overlays();
        let mut overlay_painter = painter.clone();
        overlay_painter.set_opacity(overlay_opacity(self.translucent_overlays));
        let layer1_artwork_bounds = if visibility.layer1 && visual_smoke_editor_layer1() {
            self.draw_object_artwork(
                painter,
                rect,
                cell,
                major_tiles,
                object_minor_tiles,
                vertical,
                records,
                placements,
                custom_objects,
                custom_map16,
                map16_texture,
                map16_texture_variants,
                block_contents_texture,
                visibility.surface_outline,
                visibility.line_guide_outline,
            )
        } else {
            HashMap::new()
        };
        self.layer1_z_order_bounds =
            object_interactive_bounds(rect, vertical, placements, &layer1_artwork_bounds, cell);
        self.layer2_z_order_bounds = object_interactive_bounds(
            rect,
            vertical,
            layer2_placements,
            &layer2_artwork_bounds,
            cell,
        );
        if visibility.layer3
            && game_camera.is_none()
            && !self.layer3_between_background_and_foreground
            && let (Some(texture), Some(position), Some(camera)) = (
                self.layer3_high_texture.as_ref(),
                layer3_position,
                layer3_camera,
            )
        {
            // Lunar Magic's editor composites every level-art layer before its sprite-preview
            // nodes. Runtime priority remains relevant to the game viewport, but applying BG3
            // high priority here hides previews that cross foreground effects (for example the
            // dolphins at level $002's water line).
            draw_layer3_editor_or_viewport(
                painter,
                rect,
                cell,
                texture,
                position,
                camera,
                major_tiles,
                minor_tiles,
                vertical,
                false,
            );
        }
        if !game_preview && self.conditional_view_state.block_exits {
            if visibility.layer2 {
                draw_block_exit_warnings(
                    painter,
                    layer2_target,
                    cell,
                    major_tiles,
                    object_minor_tiles,
                    vertical,
                    layer2_records,
                    layer2_placements,
                    self.active_standard_object_handler_map(),
                    self.conditional_view_state,
                    level_mode,
                    object_tileset,
                );
            }
            if visibility.layer1 && visual_smoke_editor_layer1() {
                draw_block_exit_warnings(
                    painter,
                    rect,
                    cell,
                    major_tiles,
                    object_minor_tiles,
                    vertical,
                    records,
                    placements,
                    self.active_standard_object_handler_map(),
                    self.conditional_view_state,
                    level_mode,
                    object_tileset,
                );
            }
        }
        let layer2_resize_models = self.active_object_resize_models(layer2_records, custom_objects);
        let layer1_resize_models = self.active_object_resize_models(records, custom_objects);
        let hit_layer2 = visibility
            .layer2
            .then(|| {
                draw_object_placement_markers(
                    &overlay_painter,
                    response.interact_pointer_pos(),
                    rect,
                    vertical,
                    layer2_records,
                    layer2_placements,
                    &self.selected_layer2_object_group,
                    self.selected_layer2_object,
                    map16_texture,
                    &layer2_artwork_bounds,
                    &layer2_resize_models,
                    cell,
                    editor_overlays,
                    matches!(
                        self.canvas_entity_selection,
                        Some(CanvasEntitySelection::Layer2Object)
                    ),
                )
            })
            .unwrap_or_default();
        let hit = visibility
            .layer1
            .then(|| {
                draw_object_placement_markers(
                    &overlay_painter,
                    response.interact_pointer_pos(),
                    rect,
                    vertical,
                    records,
                    placements,
                    &self.selected_object_group,
                    self.selected_object,
                    map16_texture,
                    &layer1_artwork_bounds,
                    &layer1_resize_models,
                    cell,
                    editor_overlays,
                    matches!(
                        self.canvas_entity_selection,
                        Some(CanvasEntitySelection::Layer1Object)
                    ),
                )
            })
            .unwrap_or_default();
        let sprite_limit = visual_smoke_editor_sprite_limit()
            .unwrap_or(sprite_placements.len())
            .min(sprite_placements.len());
        let sprite_draw = (visibility.sprites && visual_smoke_editor_sprites()).then(|| {
            draw_sprite_placements(SpritePlacementDraw {
                painter,
                overlay_painter: &overlay_painter,
                target: rect,
                cell_size: cell,
                texture: self.sprite_texture.as_ref(),
                animated_texture: self
                    .animated_sprite_textures
                    .get(usize::from(animation_phase))
                    .or(self.sprite_texture.as_ref()),
                placements: &sprite_placements[..sprite_limit],
                cursor: response.interact_pointer_pos(),
                selected_group: &self.selected_sprite_group,
                selected: self.selected_sprite,
                vertical,
                level_mode,
                sprite_tileset: self.form.sprite_tileset,
                sprite_memory_index: self
                    .controller
                    .as_ref()
                    .map_or(0, |controller| controller.level().sprites.header & 0x3f),
                animation_phase,
                silver_pow_active: self.silver_pow_active,
                custom_sprites,
                custom_map16,
                external_textures: &self.external_sprite_textures,
                editor_overlays,
                selection_visible: matches!(
                    self.canvas_entity_selection,
                    Some(CanvasEntitySelection::Sprite)
                ),
                selected_only: false,
            })
        });
        let hit_sprite = sprite_draw.as_ref().and_then(|result| result.hit);
        self.sprite_z_order_bounds = sprite_draw.map_or_else(HashMap::new, |result| result.bounds);
        if visibility.layer3
            && game_camera.is_some()
            && !self.layer3_between_background_and_foreground
            && let (Some(texture), Some(position), Some(camera)) = (
                self.layer3_high_texture.as_ref(),
                layer3_position,
                layer3_camera,
            )
        {
            draw_layer3_editor_or_viewport(
                painter,
                rect,
                cell,
                texture,
                position,
                camera,
                major_tiles,
                minor_tiles,
                vertical,
                game_camera.is_some(),
            );
        }
        // Paint the editor grid after the level artwork. Drawing opaque grid lines underneath
        // transparent Map16 pixels turns SMW's solid backdrop into a misleading checkerboard.
        if editor_overlays {
            if tile_grid_visible(editor_overlays, visibility) {
                draw_object_grid(
                    &overlay_painter,
                    rect,
                    cell,
                    major_tiles,
                    minor_tiles,
                    vertical,
                );
            }
            if visibility.screen_overlay == crate::application::LevelScreenOverlay::ScreenGrid {
                draw_level_screen_grid(
                    &overlay_painter,
                    rect,
                    cell,
                    major_tiles,
                    minor_tiles,
                    vertical,
                );
            }
            if visibility.screen_overlay == crate::application::LevelScreenOverlay::ScreenExits {
                draw_level_screen_exit_annotations(
                    &overlay_painter,
                    rect,
                    cell,
                    major_tiles,
                    minor_tiles,
                    vertical,
                    records,
                    self.secondary_exits.as_ref(),
                );
            }
            if visibility.screen_overlay == crate::application::LevelScreenOverlay::BoundaryGuide {
                draw_level_boundary_guide(
                    &overlay_painter,
                    rect,
                    cell,
                    level_mode,
                    self.game_preview_camera_origin(major_tiles, minor_tiles, vertical),
                );
            }
            let alternate_vertical_layout =
                lm_profile::smw_us_v1_level_mode(level_mode).alternate_layer_layout;
            let level = self.controller.as_ref().map_or(0, |controller| {
                u16::try_from(controller.level().number).unwrap_or(0)
            });
            let entrances_overlap = self.entrance_form.level_mode_and_screen & 0x1f
                == self.entrance_form.screen_and_method >> 4;
            if self.entrance_overlay_visibility.primary {
                draw_primary_entrance_label(
                    &overlay_painter,
                    rect,
                    cell,
                    level,
                    self.entrance_form,
                    vertical,
                    alternate_vertical_layout,
                    entrances_overlap && self.entrance_overlay_visibility.midway,
                );
                draw_primary_entrance_position_warning(
                    &overlay_painter,
                    rect,
                    cell,
                    self.entrance_form,
                    vertical,
                );
            }
            if self.entrance_overlay_visibility.secondary {
                draw_secondary_entrances(
                    &overlay_painter,
                    rect,
                    cell,
                    level,
                    self.secondary_exits.as_ref(),
                    self.secondary_exit_references.as_deref(),
                    self.entrance_texture.as_ref(),
                    vertical,
                    alternate_vertical_layout,
                );
            }
            if self.entrance_overlay_visibility.midway && !entrances_overlap {
                draw_midway_entrance(
                    &overlay_painter,
                    rect,
                    cell,
                    self.entrance_texture.as_ref(),
                    self.entrance_form,
                    vertical,
                    alternate_vertical_layout,
                );
            }
            if self.entrance_overlay_visibility.primary
                && let Some(texture) = self.entrance_texture.as_ref()
            {
                draw_primary_entrance_marker(
                    &overlay_painter,
                    rect,
                    cell,
                    texture,
                    self.entrance_form,
                    vertical,
                    alternate_vertical_layout,
                );
            }
        }
        self.handle_canvas_interaction(
            response,
            hit.body,
            hit.resize,
            hit_layer2.body,
            hit_layer2.resize,
            hit_sprite,
            layer2_records,
            records,
            rect,
            cell,
            vertical,
            visibility,
        );
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn handle_canvas_interaction(
        &mut self,
        response: &egui::Response,
        hit_object: Option<usize>,
        hit_object_resize: Option<usize>,
        hit_layer2_object: Option<usize>,
        hit_layer2_resize: Option<usize>,
        hit_sprite: Option<usize>,
        layer2_records: &[ObjectRecord],
        records: &[ObjectRecord],
        rect: egui::Rect,
        cell: f32,
        vertical: bool,
        visibility: crate::application::LevelViewVisibility,
    ) {
        // Select on the physical press, not only on egui's synthesized click at release. A
        // click-and-drag response may cease to qualify as `clicked()` after even a tiny amount of
        // pointer motion (especially through remote-desktop clients), which previously made the
        // canvas appear completely inert. The same press also becomes the anchor if egui later
        // promotes the gesture to a drag.
        let (primary_pressed, additive_selection) = response.ctx.input(|input| {
            (
                response.hovered() && input.pointer.button_pressed(egui::PointerButton::Primary),
                input.modifiers.ctrl,
            )
        });
        let selection_pressed = primary_pressed;
        if selection_pressed {
            response.request_focus();
        }
        let (duplicate_at_pointer, secondary_released) = response.ctx.input(|input| {
            (
                response.hovered()
                    && input.pointer.button_pressed(egui::PointerButton::Secondary)
                    && !input.modifiers.any(),
                input
                    .pointer
                    .button_released(egui::PointerButton::Secondary),
            )
        });
        if duplicate_at_pointer
            && self.placement_mode.is_none()
            && let Some(position) = response.interact_pointer_pos()
        {
            response.request_focus();
            let visible = match self.canvas_entity_selection {
                Some(CanvasEntitySelection::Layer1Object) => visibility.layer1,
                Some(CanvasEntitySelection::Layer2Object) => visibility.layer2,
                Some(CanvasEntitySelection::Sprite) => visibility.sprites,
                None => false,
            };
            if visible {
                self.begin_secondary_duplicate_drag(position, rect, cell, vertical);
                return;
            }
        }
        if secondary_released && self.secondary_duplicate_drag {
            let position = response
                .interact_pointer_pos()
                .or_else(|| response.ctx.pointer_interact_pos());
            self.finish_secondary_duplicate_drag(position, rect, cell, vertical);
            return;
        }
        if response.clicked()
            && let Some(mode) = self.placement_mode
            && placement_mode_visible(mode, visibility)
            && let Some(position) = response.interact_pointer_pos()
        {
            match mode {
                CanvasPlacementMode::Object => {
                    self.place_object_at_canvas(position, rect, cell, vertical);
                }
                CanvasPlacementMode::Sprite => {
                    self.place_sprite_at_canvas(position, rect, cell, vertical);
                }
                CanvasPlacementMode::Layer2Object => {
                    self.place_layer2_object_at_canvas(position, rect, cell, vertical);
                }
                CanvasPlacementMode::Layer2Tile => {
                    self.paint_layer2_tile_at_canvas(position, rect, cell);
                }
            }
            return;
        }
        if selection_pressed
            && self.placement_mode.is_none()
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            if self.update_canvas_object_group(
                CanvasEntitySelection::Layer1Object,
                index,
                additive_selection,
            ) {
                self.selected_object = index;
                self.object_form = ObjectForm::from_record(record);
                self.object_placement_template = Some(record.clone());
            }
        }
        if selection_pressed
            && self.placement_mode.is_none()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_object
            && let Some(record) = layer2_records.get(index)
        {
            if self.update_canvas_object_group(
                CanvasEntitySelection::Layer2Object,
                index,
                additive_selection,
            ) {
                self.selected_layer2_object = index;
                self.layer2_object_form = ObjectForm::from_record(record);
                self.layer2_object_placement_template = Some(record.clone());
            }
        }
        if response.clicked()
            && hit_object.is_none()
            && hit_layer2_object.is_none()
            && hit_sprite.is_none()
            && visibility.layer2
            && let Some(position) = response.interact_pointer_pos()
            && let Some(index) = layer2_tile_at_canvas_position(position, rect, cell)
            && let Some(lm_level::NativeLayer2Data::Tilemap(bytes)) =
                self.controller.as_ref().and_then(LevelController::layer2)
            && let Some(word) = bytes.get(index * 2..index * 2 + 2)
        {
            self.selected_layer2_tile = index;
            self.layer2_word = u16::from_le_bytes([word[0], word[1]]);
        }
        if selection_pressed
            && self.placement_mode.is_none()
            && let Some(index) = hit_sprite
            && let Some(controller) = &self.controller
        {
            let header = controller.level().sprites.header;
            let token = controller.level().sprites.tokens.get(index).cloned();
            if self.update_canvas_sprite_group(index, additive_selection) {
                self.selected_sprite = index;
                self.sprite_form = SpriteForm::from_token(header, token.as_ref());
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && !additive_selection
            && let Some(index) = hit_sprite
        {
            if self.selected_sprite_group.len() > 1
                && self.selected_sprite_group.contains(&index)
                && let Some(position) = response.interact_pointer_pos()
                && let Some((origin_major, origin_minor)) =
                    object_native_position_at_canvas(position, rect, cell, vertical)
            {
                self.object_group_drag = Some(CanvasObjectGroupDrag {
                    domain: CanvasEntitySelection::Sprite,
                    origin_major,
                    origin_minor,
                    secondary: false,
                });
            } else {
                self.dragging_sprite = Some(index);
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && !additive_selection
            && hit_sprite.is_none()
            && let Some(index) = hit_object_resize
        {
            self.resizing_object = Some(index);
            self.selected_object = index;
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
            if let Some(record) = records.get(index) {
                self.object_form = ObjectForm::from_record(record);
                self.object_placement_template = Some(record.clone());
            }
        } else if response.drag_started_by(egui::PointerButton::Primary)
            && !additive_selection
            && hit_sprite.is_none()
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            if self.selected_object_group.len() > 1
                && self.selected_object_group.contains(&index)
                && let Some(position) = response.interact_pointer_pos()
                && let Some((origin_major, origin_minor)) =
                    object_native_position_at_canvas(position, rect, cell, vertical)
            {
                self.object_group_drag = Some(CanvasObjectGroupDrag {
                    domain: CanvasEntitySelection::Layer1Object,
                    origin_major,
                    origin_minor,
                    secondary: false,
                });
            } else {
                self.dragging_object = Some(index);
            }
            self.selected_object = index;
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
            self.object_form = ObjectForm::from_record(record);
            self.object_placement_template = Some(record.clone());
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && !additive_selection
            && hit_sprite.is_none()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_resize
        {
            self.resizing_layer2_object = Some(index);
            self.selected_layer2_object = index;
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
            if let Some(record) = layer2_records.get(index) {
                self.layer2_object_form = ObjectForm::from_record(record);
                self.layer2_object_placement_template = Some(record.clone());
            }
        } else if response.drag_started_by(egui::PointerButton::Primary)
            && !additive_selection
            && hit_sprite.is_none()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_object
            && let Some(record) = layer2_records.get(index)
        {
            if self.selected_layer2_object_group.len() > 1
                && self.selected_layer2_object_group.contains(&index)
                && let Some(position) = response.interact_pointer_pos()
                && let Some((origin_major, origin_minor)) =
                    object_native_position_at_canvas(position, rect, cell, vertical)
            {
                self.object_group_drag = Some(CanvasObjectGroupDrag {
                    domain: CanvasEntitySelection::Layer2Object,
                    origin_major,
                    origin_minor,
                    secondary: false,
                });
            } else {
                self.dragging_layer2_object = Some(index);
            }
            self.selected_layer2_object = index;
            self.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
            self.layer2_object_form = ObjectForm::from_record(record);
            self.layer2_object_placement_template = Some(record.clone());
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if self.object_group_drag.is_some_and(|drag| !drag.secondary) {
                self.finish_object_group_drag(
                    response.interact_pointer_pos(),
                    rect,
                    cell,
                    vertical,
                );
            } else {
                self.finish_canvas_entity_drag(
                    response.interact_pointer_pos(),
                    rect,
                    cell,
                    vertical,
                );
            }
        }
        if self.dragging_object.is_none()
            && self.dragging_layer2_object.is_none()
            && self.dragging_sprite.is_none()
            && self.object_group_drag.is_none()
            && self.resizing_object.is_none()
            && self.resizing_layer2_object.is_none()
            && let Some(shortcut) = canvas_entity_shortcut(response)
        {
            if shortcut == CanvasEntityShortcut::Insert {
                if let Some(position) = response
                    .interact_pointer_pos()
                    .or_else(|| response.ctx.pointer_hover_pos())
                {
                    self.apply_canvas_insert_shortcut(position, rect, cell, vertical);
                } else {
                    self.error =
                        Some("Insert requires a pointer position on the level canvas".into());
                }
            } else {
                self.apply_canvas_entity_shortcut(shortcut);
            }
        }
    }

    fn apply_canvas_insert_shortcut(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        match self.canvas_entity_selection {
            Some(CanvasEntitySelection::Layer1Object) => {
                self.place_object_at_canvas(position, canvas, cell, vertical);
            }
            Some(CanvasEntitySelection::Layer2Object) => {
                self.place_layer2_object_at_canvas(position, canvas, cell, vertical);
            }
            Some(CanvasEntitySelection::Sprite) => {
                self.place_sprite_at_canvas(position, canvas, cell, vertical);
            }
            None => {}
        }
    }

    fn update_canvas_object_group(
        &mut self,
        domain: CanvasEntitySelection,
        index: usize,
        additive: bool,
    ) -> bool {
        debug_assert!(matches!(
            domain,
            CanvasEntitySelection::Layer1Object | CanvasEntitySelection::Layer2Object
        ));
        if !additive
            && self.canvas_entity_selection == Some(domain)
            && match domain {
                CanvasEntitySelection::Layer1Object => {
                    self.selected_object_group.len() > 1
                        && self.selected_object_group.contains(&index)
                }
                CanvasEntitySelection::Layer2Object => {
                    self.selected_layer2_object_group.len() > 1
                        && self.selected_layer2_object_group.contains(&index)
                }
                CanvasEntitySelection::Sprite => unreachable!(),
            }
        {
            return true;
        }
        if !additive || self.canvas_entity_selection != Some(domain) {
            self.selected_object_group.clear();
            self.selected_layer2_object_group.clear();
            self.selected_sprite_group.clear();
            match domain {
                CanvasEntitySelection::Layer1Object => self.selected_object_group.push(index),
                CanvasEntitySelection::Layer2Object => {
                    self.selected_layer2_object_group.push(index);
                }
                CanvasEntitySelection::Sprite => unreachable!(),
            }
            self.canvas_entity_selection = Some(domain);
            return true;
        }

        let group = match domain {
            CanvasEntitySelection::Layer1Object => &mut self.selected_object_group,
            CanvasEntitySelection::Layer2Object => &mut self.selected_layer2_object_group,
            CanvasEntitySelection::Sprite => unreachable!(),
        };
        if let Some(position) = group.iter().position(|selected| *selected == index) {
            group.remove(position);
            let replacement = group.last().copied();
            if let Some(replacement) = replacement {
                match domain {
                    CanvasEntitySelection::Layer1Object => {
                        self.selected_object = replacement;
                        self.reload_object_form();
                    }
                    CanvasEntitySelection::Layer2Object => {
                        self.selected_layer2_object = replacement;
                        self.reload_layer2_object_form();
                    }
                    CanvasEntitySelection::Sprite => unreachable!(),
                }
            } else {
                self.canvas_entity_selection = None;
            }
            false
        } else {
            group.push(index);
            true
        }
    }

    fn update_canvas_sprite_group(&mut self, index: usize, additive: bool) -> bool {
        if !additive
            && self.canvas_entity_selection == Some(CanvasEntitySelection::Sprite)
            && self.selected_sprite_group.len() > 1
            && self.selected_sprite_group.contains(&index)
        {
            return true;
        }
        if !additive || self.canvas_entity_selection != Some(CanvasEntitySelection::Sprite) {
            self.selected_object_group.clear();
            self.selected_layer2_object_group.clear();
            self.selected_sprite_group.clear();
            self.selected_sprite_group.push(index);
            self.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
            return true;
        }
        if let Some(position) = self
            .selected_sprite_group
            .iter()
            .position(|selected| *selected == index)
        {
            self.selected_sprite_group.remove(position);
            if let Some(replacement) = self.selected_sprite_group.last().copied() {
                self.selected_sprite = replacement;
                if let Some(controller) = &self.controller {
                    self.sprite_form = SpriteForm::from_token(
                        controller.level().sprites.header,
                        controller.level().sprites.tokens.get(replacement),
                    );
                }
            } else {
                self.canvas_entity_selection = None;
            }
            false
        } else {
            self.selected_sprite_group.push(index);
            true
        }
    }

    fn apply_canvas_entity_shortcut(&mut self, shortcut: CanvasEntityShortcut) {
        let Some(selection) = self.canvas_entity_selection else {
            return;
        };
        match (selection, shortcut) {
            (_, CanvasEntityShortcut::Insert) => {}
            (selection, CanvasEntityShortcut::SelectAll) => {
                let Some(controller) = self.controller.as_ref() else {
                    return;
                };
                match selection {
                    CanvasEntitySelection::Layer1Object => {
                        self.selected_object_group = controller
                            .level()
                            .layer1
                            .objects
                            .native_placements()
                            .into_iter()
                            .map(|placement| placement.record_index)
                            .collect();
                        if let Some(selected) = self.selected_object_group.first().copied() {
                            self.selected_object = selected;
                            self.reload_object_form();
                        }
                    }
                    CanvasEntitySelection::Layer2Object => {
                        let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2()
                        else {
                            return;
                        };
                        self.selected_layer2_object_group = layer2
                            .objects
                            .native_placements()
                            .into_iter()
                            .map(|placement| placement.record_index)
                            .collect();
                        if let Some(selected) = self.selected_layer2_object_group.first().copied() {
                            self.selected_layer2_object = selected;
                            self.reload_layer2_object_form();
                        }
                    }
                    CanvasEntitySelection::Sprite => {
                        self.selected_sprite_group = controller
                            .level()
                            .sprites
                            .native_placements()
                            .into_iter()
                            .map(|placement| placement.token_index)
                            .collect();
                        if let Some(selected) = self.selected_sprite_group.first().copied() {
                            self.selected_sprite = selected;
                            self.sprite_form = SpriteForm::from_token(
                                controller.level().sprites.header,
                                controller.level().sprites.tokens.get(selected),
                            );
                        }
                    }
                }
                self.error = None;
            }
            (CanvasEntitySelection::Layer1Object, CanvasEntityShortcut::Duplicate) => {
                if self.selected_object_group.len() > 1 {
                    let selected = self.selected_object_group.clone();
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    let mut predicted = controller.level().layer1.objects.clone();
                    let clones = match predicted.duplicate_ordinary_object_group(&selected, 0, 0) {
                        Ok(clones) => clones,
                        Err(error) => {
                            self.error = Some(error.to_string());
                            return;
                        }
                    };
                    self.apply_object_result(Ok(NativeLevelEdit::Objects(vec![
                        ObjectEdit::DuplicateOrdinaryGroup {
                            selected,
                            major_delta: 0,
                            minor_delta: 0,
                        },
                    ])));
                    if self.error.is_none() {
                        self.selected_object_group = clones;
                        self.selected_object = self.selected_object_group[0];
                        self.reload_object_form();
                    }
                    return;
                }
                let Some(record) = self.controller.as_ref().and_then(|controller| {
                    controller
                        .level()
                        .layer1
                        .objects
                        .records
                        .get(self.selected_object)
                        .cloned()
                }) else {
                    return;
                };
                let index = self.selected_object.saturating_add(1);
                let previous = self.selected_object;
                self.selected_object = index;
                self.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                    index,
                    record,
                }])));
                if self.error.is_some() {
                    self.selected_object = previous;
                    self.reload_object_form();
                }
            }
            (CanvasEntitySelection::Layer1Object, CanvasEntityShortcut::Remove) => {
                if self.selected_object_group.len() > 1 {
                    let mut indexes = self.selected_object_group.clone();
                    indexes.sort_unstable_by(|left, right| right.cmp(left));
                    self.apply_object_result(Ok(NativeLevelEdit::Objects(
                        indexes
                            .into_iter()
                            .map(|index| ObjectEdit::Remove { index })
                            .collect(),
                    )));
                    if self.error.is_none() {
                        self.selected_object_group.clear();
                        self.canvas_entity_selection = None;
                    }
                    return;
                }
                self.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
                    index: self.selected_object,
                }])));
                if self.error.is_none() {
                    self.canvas_entity_selection = None;
                }
            }
            (CanvasEntitySelection::Layer2Object, CanvasEntityShortcut::Duplicate) => {
                if self.selected_layer2_object_group.len() > 1 {
                    let selected = self.selected_layer2_object_group.clone();
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2()
                    else {
                        return;
                    };
                    let mut predicted = layer2.objects.clone();
                    let clones = match predicted.duplicate_ordinary_object_group(&selected, 0, 0) {
                        Ok(clones) => clones,
                        Err(error) => {
                            self.error = Some(error.to_string());
                            return;
                        }
                    };
                    match controller.apply_layer2_object_edits(&[
                        ObjectEdit::DuplicateOrdinaryGroup {
                            selected,
                            major_delta: 0,
                            minor_delta: 0,
                        },
                    ]) {
                        Ok(()) => {
                            self.selected_layer2_object_group = clones;
                            self.selected_layer2_object = self.selected_layer2_object_group[0];
                            self.reload_layer2_object_form();
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    return;
                }
                let Some(record) = self.controller.as_ref().and_then(|controller| {
                    let lm_level::NativeLayer2Data::Objects(layer2) = controller.layer2()? else {
                        return None;
                    };
                    layer2
                        .objects
                        .records
                        .get(self.selected_layer2_object)
                        .cloned()
                }) else {
                    return;
                };
                let index = self.selected_layer2_object.saturating_add(1);
                let Some(controller) = self.controller.as_mut() else {
                    return;
                };
                match controller.apply_layer2_object_edits(&[ObjectEdit::Insert { index, record }])
                {
                    Ok(()) => {
                        self.selected_layer2_object = index;
                        self.reload_layer2_object_form();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            (CanvasEntitySelection::Layer2Object, CanvasEntityShortcut::Remove) => {
                if self.selected_layer2_object_group.len() > 1 {
                    let mut indexes = self.selected_layer2_object_group.clone();
                    indexes.sort_unstable_by(|left, right| right.cmp(left));
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    let edits: Vec<_> = indexes
                        .into_iter()
                        .map(|index| ObjectEdit::Remove { index })
                        .collect();
                    match controller.apply_layer2_object_edits(&edits) {
                        Ok(()) => {
                            self.selected_layer2_object_group.clear();
                            self.canvas_entity_selection = None;
                            self.reload_layer2_object_form();
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    return;
                }
                let index = self.selected_layer2_object;
                let Some(controller) = self.controller.as_mut() else {
                    return;
                };
                match controller.apply_layer2_object_edits(&[ObjectEdit::Remove { index }]) {
                    Ok(()) => {
                        self.reload_layer2_object_form();
                        self.canvas_entity_selection = None;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            (CanvasEntitySelection::Sprite, CanvasEntityShortcut::Duplicate) => {
                if self.selected_sprite_group.len() > 1 {
                    let selected = self.selected_sprite_group.clone();
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    let vertical = lm_profile::smw_us_v1_level_mode(
                        controller.level().layer1.header.level_mode(),
                    )
                    .vertical;
                    let mut predicted = controller.level().sprites.clone();
                    let clones = match predicted.duplicate_record_group(
                        &selected,
                        0,
                        0,
                        vertical,
                        controller.sprite_lengths(),
                    ) {
                        Ok(clones) => clones,
                        Err(error) => {
                            self.error = Some(error.to_string());
                            return;
                        }
                    };
                    match controller.apply_edits(&[NativeLevelEdit::DuplicateSpriteGroup {
                        selected,
                        major_delta: 0,
                        minor_delta: 0,
                    }]) {
                        Ok(()) => {
                            self.selected_sprite_group = clones;
                            self.selected_sprite = self.selected_sprite_group[0];
                            self.sprite_form = SpriteForm::from_token(
                                controller.level().sprites.header,
                                controller.level().sprites.tokens.get(self.selected_sprite),
                            );
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    return;
                }
                let Some(token) = self.controller.as_ref().and_then(|controller| {
                    controller
                        .level()
                        .sprites
                        .tokens
                        .get(self.selected_sprite)
                        .cloned()
                }) else {
                    return;
                };
                self.insert_canvas_sprite_token(token);
            }
            (CanvasEntitySelection::Sprite, CanvasEntityShortcut::Remove) => {
                if self.selected_sprite_group.len() > 1 {
                    let mut indexes = self.selected_sprite_group.clone();
                    indexes.sort_unstable_by(|left, right| right.cmp(left));
                    let edits: Vec<_> = indexes
                        .into_iter()
                        .map(|index| NativeLevelEdit::RemoveSprite { index })
                        .collect();
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    match controller.apply_edits(&edits) {
                        Ok(()) => {
                            self.selected_sprite_group.clear();
                            self.selected_sprite = self
                                .selected_sprite
                                .min(controller.level().sprites.tokens.len().saturating_sub(1));
                            self.sprite_form = SpriteForm::from_token(
                                controller.level().sprites.header,
                                controller.level().sprites.tokens.get(self.selected_sprite),
                            );
                            self.canvas_entity_selection = None;
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    return;
                }
                let index = self.selected_sprite;
                let Some(controller) = self.controller.as_mut() else {
                    return;
                };
                match controller.apply_edits(&[NativeLevelEdit::RemoveSprite { index }]) {
                    Ok(()) => {
                        self.selected_sprite = self
                            .selected_sprite
                            .min(controller.level().sprites.tokens.len().saturating_sub(1));
                        self.sprite_form = SpriteForm::from_token(
                            controller.level().sprites.header,
                            controller.level().sprites.tokens.get(self.selected_sprite),
                        );
                        self.selected_sprite_group.clear();
                        self.canvas_entity_selection = None;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        }
    }

    fn begin_secondary_duplicate_drag(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let selection = self.canvas_entity_selection;
        self.duplicate_canvas_selection_at(position, canvas, cell, vertical);
        if self.error.is_some() {
            return;
        }
        match selection {
            Some(CanvasEntitySelection::Layer1Object) => {
                if self.selected_object_group.len() > 1 {
                    let Some((origin_major, origin_minor)) =
                        object_native_position_at_canvas(position, canvas, cell, vertical)
                    else {
                        return;
                    };
                    self.object_group_drag = Some(CanvasObjectGroupDrag {
                        domain: CanvasEntitySelection::Layer1Object,
                        origin_major,
                        origin_minor,
                        secondary: true,
                    });
                } else {
                    self.dragging_object = Some(self.selected_object);
                }
            }
            Some(CanvasEntitySelection::Layer2Object) => {
                if self.selected_layer2_object_group.len() > 1 {
                    let Some((origin_major, origin_minor)) =
                        object_native_position_at_canvas(position, canvas, cell, vertical)
                    else {
                        return;
                    };
                    self.object_group_drag = Some(CanvasObjectGroupDrag {
                        domain: CanvasEntitySelection::Layer2Object,
                        origin_major,
                        origin_minor,
                        secondary: true,
                    });
                } else {
                    self.dragging_layer2_object = Some(self.selected_layer2_object);
                }
            }
            Some(CanvasEntitySelection::Sprite) => {
                if self.selected_sprite_group.len() > 1 {
                    let Some((origin_major, origin_minor)) =
                        object_native_position_at_canvas(position, canvas, cell, vertical)
                    else {
                        return;
                    };
                    self.object_group_drag = Some(CanvasObjectGroupDrag {
                        domain: CanvasEntitySelection::Sprite,
                        origin_major,
                        origin_minor,
                        secondary: true,
                    });
                } else {
                    self.dragging_sprite = Some(self.selected_sprite);
                }
            }
            None => return,
        }
        self.secondary_duplicate_drag = true;
    }

    fn finish_canvas_entity_drag(
        &mut self,
        position: Option<egui::Pos2>,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        if let (Some(index), Some(position)) = (self.resizing_object.take(), position) {
            self.resize_object_to_canvas(index, position, canvas, cell, vertical);
        } else if let (Some(index), Some(position)) = (self.resizing_layer2_object.take(), position)
        {
            self.resize_layer2_object_to_canvas(index, position, canvas, cell, vertical);
        } else if let (Some(index), Some(position)) = (self.dragging_sprite.take(), position) {
            self.move_sprite_to_canvas(index, position, canvas, cell, vertical);
        } else if let (Some(index), Some(position)) = (self.dragging_object.take(), position) {
            self.move_object_to_canvas(index, position, canvas, cell, vertical);
        } else if let (Some(index), Some(position)) = (self.dragging_layer2_object.take(), position)
        {
            self.move_layer2_object_to_canvas(index, position, canvas, cell, vertical);
        } else if position.is_none() {
            self.dragging_sprite = None;
            self.dragging_object = None;
            self.dragging_layer2_object = None;
            self.resizing_object = None;
            self.resizing_layer2_object = None;
        }
    }

    fn finish_secondary_duplicate_drag(
        &mut self,
        position: Option<egui::Pos2>,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        if self.object_group_drag.is_some_and(|drag| drag.secondary) {
            self.finish_object_group_drag(position, canvas, cell, vertical);
        } else {
            self.finish_canvas_entity_drag(position, canvas, cell, vertical);
        }
        self.secondary_duplicate_drag = false;
    }

    fn clear_z_order_bounds(&mut self) {
        self.layer1_z_order_bounds.clear();
        self.layer2_z_order_bounds.clear();
        self.sprite_z_order_bounds.clear();
    }

    /// Reproduces Lunar Magic's unmodified right-click placement boundary for the one selected
    /// native entity. The original clones the selection before relocating it, so the source stays
    /// in place and the newly inserted record becomes the active selection.
    fn duplicate_canvas_selection_at(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        match self.canvas_entity_selection {
            Some(CanvasEntitySelection::Layer1Object) => {
                if self.selected_object_group.len() > 1 {
                    self.duplicate_object_group_at(
                        CanvasEntitySelection::Layer1Object,
                        position,
                        canvas,
                        cell,
                        vertical,
                    );
                } else {
                    self.place_object_at_canvas(position, canvas, cell, vertical);
                }
            }
            Some(CanvasEntitySelection::Layer2Object) => {
                if self.selected_layer2_object_group.len() > 1 {
                    self.duplicate_object_group_at(
                        CanvasEntitySelection::Layer2Object,
                        position,
                        canvas,
                        cell,
                        vertical,
                    );
                } else {
                    self.place_layer2_object_at_canvas(position, canvas, cell, vertical);
                }
            }
            Some(CanvasEntitySelection::Sprite) => {
                if self.selected_sprite_group.len() > 1 {
                    self.duplicate_sprite_group_at(position, canvas, cell, vertical);
                } else {
                    self.place_sprite_at_canvas(position, canvas, cell, vertical);
                }
            }
            None => {}
        }
    }

    fn duplicate_object_group_at(
        &mut self,
        domain: CanvasEntitySelection,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((target_major, target_minor)) =
            object_native_position_at_canvas(position, canvas, cell, vertical)
        else {
            self.error = Some("object group placement is outside the native level space".into());
            return;
        };
        let selected = match domain {
            CanvasEntitySelection::Layer1Object => self.selected_object_group.clone(),
            CanvasEntitySelection::Layer2Object => self.selected_layer2_object_group.clone(),
            CanvasEntitySelection::Sprite => return,
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let stream = match domain {
            CanvasEntitySelection::Layer1Object => &controller.level().layer1.objects,
            CanvasEntitySelection::Layer2Object => {
                let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
                    self.error =
                        Some("the current level does not use object-backed Layer 2".into());
                    return;
                };
                &layer2.objects
            }
            CanvasEntitySelection::Sprite => return,
        };
        let Some((anchor_major, anchor_minor)) = object_group_anchor(stream, &selected) else {
            self.error = Some("selected object group contains no visible native placement".into());
            return;
        };
        let requested_major = target_major - anchor_major;
        let requested_minor = target_minor - anchor_minor;
        let positions: Vec<_> = stream
            .native_placements()
            .into_iter()
            .filter(|placement| selected.contains(&placement.record_index))
            .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
            .collect();
        let Some((major_delta, minor_delta)) = nearest_valid_group_delta(
            &positions,
            requested_major,
            requested_minor,
            512,
            i32::from(level_minor_tile_limit(vertical)),
        ) else {
            self.error = Some("no nonzero shared object displacement remains in bounds".into());
            return;
        };
        let mut predicted = stream.clone();
        let cloned =
            match predicted.duplicate_ordinary_object_group(&selected, major_delta, minor_delta) {
                Ok(cloned) => cloned,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
        let edit = ObjectEdit::DuplicateOrdinaryGroup {
            selected,
            major_delta,
            minor_delta,
        };
        let result = match domain {
            CanvasEntitySelection::Layer1Object => {
                controller.apply_edits(&[NativeLevelEdit::Objects(vec![edit])])
            }
            CanvasEntitySelection::Layer2Object => controller.apply_layer2_object_edits(&[edit]),
            CanvasEntitySelection::Sprite => return,
        };
        match result {
            Ok(()) => {
                match domain {
                    CanvasEntitySelection::Layer1Object => {
                        self.selected_object_group = cloned;
                        self.selected_object = self.selected_object_group[0];
                        self.reload_object_form();
                    }
                    CanvasEntitySelection::Layer2Object => {
                        self.selected_layer2_object_group = cloned;
                        self.selected_layer2_object = self.selected_layer2_object_group[0];
                        self.reload_layer2_object_form();
                    }
                    CanvasEntitySelection::Sprite => return,
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn duplicate_sprite_group_at(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((target_major, target_minor)) =
            object_native_position_at_canvas(position, canvas, cell, vertical)
        else {
            self.error = Some("sprite group placement is outside the native level space".into());
            return;
        };
        let selected = self.selected_sprite_group.clone();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let stream = &controller.level().sprites;
        let Some((anchor_major, anchor_minor)) = sprite_group_anchor(stream, &selected) else {
            self.error = Some("selected sprite group contains no visible native placement".into());
            return;
        };
        let requested_major = target_major - anchor_major;
        let requested_minor = target_minor - anchor_minor;
        let positions: Vec<_> = stream
            .native_placements()
            .into_iter()
            .filter(|placement| selected.contains(&placement.token_index))
            .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
            .collect();
        let Some((major_delta, minor_delta)) = nearest_valid_group_delta(
            &positions,
            requested_major,
            requested_minor,
            512,
            i32::from(level_minor_tile_limit(vertical)),
        ) else {
            self.error = Some("no nonzero shared sprite displacement remains in bounds".into());
            return;
        };
        let mut predicted = stream.clone();
        let cloned = match predicted.duplicate_record_group(
            &selected,
            major_delta,
            minor_delta,
            vertical,
            controller.sprite_lengths(),
        ) {
            Ok(cloned) => cloned,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::DuplicateSpriteGroup {
            selected,
            major_delta,
            minor_delta,
        }]) {
            Ok(()) => {
                self.selected_sprite_group = cloned;
                self.selected_sprite = self.selected_sprite_group[0];
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(self.selected_sprite),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn finish_object_group_drag(
        &mut self,
        position: Option<egui::Pos2>,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some(drag) = self.object_group_drag.take() else {
            return;
        };
        let Some((target_major, target_minor)) = position.and_then(|position| {
            object_native_position_at_canvas(position, canvas, cell, vertical)
        }) else {
            return;
        };
        let requested_major = target_major - drag.origin_major;
        let requested_minor = target_minor - drag.origin_minor;
        if requested_major == 0 && requested_minor == 0 {
            return;
        }
        let selected = match drag.domain {
            CanvasEntitySelection::Layer1Object => self.selected_object_group.clone(),
            CanvasEntitySelection::Layer2Object => self.selected_layer2_object_group.clone(),
            CanvasEntitySelection::Sprite => self.selected_sprite_group.clone(),
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let stream = match drag.domain {
            CanvasEntitySelection::Layer1Object => &controller.level().layer1.objects,
            CanvasEntitySelection::Layer2Object => {
                let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
                    self.error =
                        Some("the current level does not use object-backed Layer 2".into());
                    return;
                };
                &layer2.objects
            }
            CanvasEntitySelection::Sprite => {
                let positions: Vec<_> = controller
                    .level()
                    .sprites
                    .native_placements()
                    .into_iter()
                    .filter(|placement| selected.contains(&placement.token_index))
                    .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
                    .collect();
                let Some((major_delta, minor_delta)) = nearest_valid_group_delta(
                    &positions,
                    requested_major,
                    requested_minor,
                    512,
                    i32::from(level_minor_tile_limit(vertical)),
                ) else {
                    return;
                };
                let mut predicted = controller.level().sprites.clone();
                let moved = match predicted.relocate_record_group(
                    &selected,
                    major_delta,
                    minor_delta,
                    vertical,
                    controller.sprite_lengths(),
                ) {
                    Ok(moved) => moved,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                };
                match controller.apply_edits(&[NativeLevelEdit::RelocateSpriteGroup {
                    selected,
                    major_delta,
                    minor_delta,
                }]) {
                    Ok(()) => {
                        self.selected_sprite_group = moved;
                        self.selected_sprite = self.selected_sprite_group[0];
                        self.sprite_form = SpriteForm::from_token(
                            controller.level().sprites.header,
                            controller.level().sprites.tokens.get(self.selected_sprite),
                        );
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
                return;
            }
        };
        let positions: Vec<_> = stream
            .native_placements()
            .into_iter()
            .filter(|placement| selected.contains(&placement.record_index))
            .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
            .collect();
        let Some((major_delta, minor_delta)) = nearest_valid_group_delta(
            &positions,
            requested_major,
            requested_minor,
            512,
            i32::from(level_minor_tile_limit(vertical)),
        ) else {
            return;
        };
        let mut predicted = stream.clone();
        let moved =
            match predicted.relocate_ordinary_object_group(&selected, major_delta, minor_delta) {
                Ok(moved) => moved,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
        let edit = ObjectEdit::RelocateOrdinaryGroup {
            selected,
            major_delta,
            minor_delta,
        };
        let result = match drag.domain {
            CanvasEntitySelection::Layer1Object => {
                controller.apply_edits(&[NativeLevelEdit::Objects(vec![edit])])
            }
            CanvasEntitySelection::Layer2Object => controller.apply_layer2_object_edits(&[edit]),
            CanvasEntitySelection::Sprite => return,
        };
        match result {
            Ok(()) => {
                match drag.domain {
                    CanvasEntitySelection::Layer1Object => {
                        self.selected_object_group = moved;
                        self.selected_object = self.selected_object_group[0];
                        self.reload_object_form();
                    }
                    CanvasEntitySelection::Layer2Object => {
                        self.selected_layer2_object_group = moved;
                        self.selected_layer2_object = self.selected_layer2_object_group[0];
                        self.reload_layer2_object_form();
                    }
                    CanvasEntitySelection::Sprite => return,
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn insert_canvas_sprite_token(&mut self, token: SpriteToken) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let index = self.selected_sprite.saturating_add(1);
        let selected = if controller.level().sprites.expanded {
            index
        } else {
            let mut predicted = controller.level().sprites.clone();
            if let Err(error) = predicted.insert(index, token.clone()) {
                self.error = Some(error.to_string());
                return;
            }
            match predicted.sort_legacy_records_by_screen(index) {
                Ok(selected) => selected,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::InsertSprite { index, token }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.selected_sprite_group.clear();
                self.selected_sprite_group.push(selected);
                self.selected_object_group.clear();
                self.selected_layer2_object_group.clear();
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn paint_layer2_tile_at_canvas(&mut self, position: egui::Pos2, canvas: egui::Rect, cell: f32) {
        if !layer2_tilemap_editable(self.shared_vanilla_background) {
            self.error = Some(
                "shared pristine backgrounds require a copy-on-write Layer 2 runtime installation"
                    .into(),
            );
            return;
        }
        let Some(index) = layer2_tile_at_canvas_position(position, canvas, cell) else {
            self.error = Some("Layer 2 tile lies outside the native 32×32 background".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_layer2_tilemap_words(&[(index, self.layer2_word)]) {
            Ok(()) => {
                self.selected_layer2_tile = index;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn place_object_at_canvas(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((screen, coordinates, perpendicular_high)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error = Some("object placement is outside the native 16×512-tile space".into());
            return;
        };
        let record = match self.object_record_for_placement() {
            Ok(record) if record.is_positioned_object() => record,
            Ok(_) => {
                self.error = Some(
                    "canvas placement requires a standard or extended object, not a command-zero control"
                        .into(),
                );
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut predicted = controller.level().layer1.objects.clone();
        let selected = match predicted.insert_ordinary_object_at_position(
            record.clone(),
            screen,
            coordinates,
            perpendicular_high,
        ) {
            Ok(selected) => selected,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
            ObjectEdit::InsertOrdinaryAtPosition {
                record,
                screen,
                coordinates,
                perpendicular_high,
            },
        ])]) {
            Ok(()) => {
                self.selected_object = selected;
                self.selected_object_group.clear();
                self.selected_object_group.push(selected);
                self.selected_layer2_object_group.clear();
                self.reload_object_form();
                self.placement_mode = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn place_layer2_object_at_canvas(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((screen, coordinates, perpendicular_high)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error =
                Some("Layer 2 object placement is outside the native 16×512-tile space".into());
            return;
        };
        let record = match self.layer2_object_record_for_placement() {
            Ok(record) if record.is_positioned_object() => record,
            Ok(_) => {
                self.error = Some(
                    "canvas placement requires a standard or extended Layer 2 object, not a command-zero control"
                        .into(),
                );
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
            self.error = Some("the current level does not use object-backed Layer 2".into());
            return;
        };
        let mut predicted = layer2.objects.clone();
        let selected = match predicted.insert_ordinary_object_at_position(
            record.clone(),
            screen,
            coordinates,
            perpendicular_high,
        ) {
            Ok(selected) => selected,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_layer2_object_edits(&[ObjectEdit::InsertOrdinaryAtPosition {
            record: record.clone(),
            screen,
            coordinates,
            perpendicular_high,
        }]) {
            Ok(()) => {
                self.selected_layer2_object = selected;
                self.selected_layer2_object_group.clear();
                self.selected_layer2_object_group.push(selected);
                self.selected_object_group.clear();
                if let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() {
                    self.layer2_object_form =
                        ObjectForm::from_record(&layer2.objects.records[selected]);
                }
                self.layer2_object_placement_template = Some(record);
                self.placement_mode = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn layer2_object_record_for_placement(&self) -> Result<ObjectRecord, String> {
        if let Some(mut record) = self.layer2_object_placement_template.clone()
            && record.command_id() == self.layer2_object_form.command_id
            && record.parameter() == self.layer2_object_form.parameter
        {
            record
                .set_coordinate_nibbles(ObjectCoordinateNibbles {
                    first: self.layer2_object_form.first_coordinate,
                    second: self.layer2_object_form.second_coordinate,
                })
                .map_err(|error| error.to_string())?;
            record
                .set_advances_screen(self.layer2_object_form.advances_screen)
                .map_err(|error| error.to_string())?;
            return Ok(record);
        }
        self.layer2_object_form.ordinary_record()
    }

    fn object_record_for_placement(&self) -> Result<ObjectRecord, String> {
        if let Some(mut record) = self.object_placement_template.clone()
            && record.command_id() == self.object_form.command_id
            && record.parameter() == self.object_form.parameter
        {
            record
                .set_coordinate_nibbles(ObjectCoordinateNibbles {
                    first: self.object_form.first_coordinate,
                    second: self.object_form.second_coordinate,
                })
                .map_err(|error| error.to_string())?;
            record
                .set_advances_screen(self.object_form.advances_screen)
                .map_err(|error| error.to_string())?;
            return Ok(record);
        }
        self.object_form.ordinary_record()
    }

    fn place_sprite_at_canvas(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some(fields) = sprite_fields_at_canvas_position(
            position,
            canvas,
            cell,
            vertical,
            NativeSpriteRecordFields {
                y_low: self.sprite_form.y_low,
                extra_bits: self.sprite_form.extra_bits,
                screen: self.sprite_form.screen,
                x: self.sprite_form.x,
                sprite_number: self.sprite_form.sprite_number,
            },
        ) else {
            self.error = Some("sprite placement is outside the native 32×512-tile space".into());
            return;
        };
        let record = match crate::native_level_document_form::parse_sprite_token(
            &self.sprite_form.encoded,
        ) {
            Ok(SpriteToken::Record(record)) => record,
            Ok(_) => {
                self.error =
                    Some("canvas placement requires a sprite record, not a control".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut predicted = controller.level().sprites.clone();
        let selected = match predicted.place_record_at_position(
            record.clone(),
            fields.screen,
            fields.x,
            u16::from(fields.y_low),
            vertical,
            controller.sprite_lengths(),
        ) {
            Ok(selected) => selected,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::PlaceSpriteAtPosition {
            record,
            screen: fields.screen,
            x: fields.x,
            y: u16::from(fields.y_low),
        }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.selected_sprite_group.clear();
                self.selected_sprite_group.push(selected);
                self.selected_object_group.clear();
                self.selected_layer2_object_group.clear();
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.placement_mode = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn move_object_to_canvas(
        &mut self,
        index: usize,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let has_placement = self.controller.as_ref().is_some_and(|controller| {
            controller
                .level()
                .layer1
                .objects
                .native_placements()
                .into_iter()
                .any(|placement| placement.record_index == index)
        });
        if !has_placement {
            self.error = Some("selected object has no visible native placement".into());
            return;
        }
        let Some((screen, coordinates, perpendicular_high)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error = Some("object drag ended outside the native 16×512-tile space".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut predicted = controller.level().layer1.objects.clone();
        let new_index = match predicted.relocate_ordinary_object_position(
            index,
            screen,
            coordinates,
            perpendicular_high,
        ) {
            Ok(index) => index,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
            ObjectEdit::RelocateOrdinaryPosition {
                index,
                screen,
                coordinates,
                perpendicular_high,
            },
        ])]) {
            Ok(()) => {
                self.selected_object = new_index;
                self.selected_object_group.clear();
                self.selected_object_group.push(new_index);
                self.reload_object_form();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn resize_object_to_canvas(
        &mut self,
        index: usize,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let result = self.controller.as_ref().and_then(|controller| {
            let record = controller.level().layer1.objects.records.get(index)?;
            let placement = controller
                .level()
                .layer1
                .objects
                .native_placements()
                .into_iter()
                .find(|placement| placement.record_index == index)?;
            let model = self.active_object_resize_model(record, None)?;
            Some(resized_standard_object_record_at_canvas_position(
                record, placement, model, position, canvas, cell, vertical,
            ))
        });
        let Some(result) = result else {
            self.error = Some("selected object has no authenticated resize handle".into());
            return;
        };
        let record = match result {
            Ok(record) => record,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index,
            record,
        }])]) {
            Ok(()) => {
                self.selected_object = index;
                self.reload_object_form();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn move_layer2_object_to_canvas(
        &mut self,
        index: usize,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((screen, coordinates, perpendicular_high)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error =
                Some("Layer 2 object drag ended outside the native 16×512-tile space".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() else {
            self.error = Some("the current level does not use object-backed Layer 2".into());
            return;
        };
        if !layer2
            .objects
            .native_placements()
            .iter()
            .any(|placement| placement.record_index == index)
        {
            self.error = Some("selected Layer 2 object has no visible native placement".into());
            return;
        }
        let mut predicted = layer2.objects.clone();
        let new_index = match predicted.relocate_ordinary_object_position(
            index,
            screen,
            coordinates,
            perpendicular_high,
        ) {
            Ok(index) => index,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_layer2_object_edits(&[ObjectEdit::RelocateOrdinaryPosition {
            index,
            screen,
            coordinates,
            perpendicular_high,
        }]) {
            Ok(()) => {
                self.selected_layer2_object = new_index;
                self.selected_layer2_object_group.clear();
                self.selected_layer2_object_group.push(new_index);
                if let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() {
                    let record = &layer2.objects.records[new_index];
                    self.layer2_object_form = ObjectForm::from_record(record);
                    self.layer2_object_placement_template = Some(record.clone());
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn resize_layer2_object_to_canvas(
        &mut self,
        index: usize,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let result = self.controller.as_ref().and_then(|controller| {
            let lm_level::NativeLayer2Data::Objects(layer2) = controller.layer2()? else {
                return None;
            };
            let record = layer2.objects.records.get(index)?;
            let placement = layer2
                .objects
                .native_placements()
                .into_iter()
                .find(|placement| placement.record_index == index)?;
            let model = self.active_object_resize_model(record, None)?;
            Some(resized_standard_object_record_at_canvas_position(
                record, placement, model, position, canvas, cell, vertical,
            ))
        });
        let Some(result) = result else {
            self.error = Some("selected Layer 2 object has no authenticated resize handle".into());
            return;
        };
        let record = match result {
            Ok(record) => record,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_layer2_object_edits(&[ObjectEdit::Replace { index, record }]) {
            Ok(()) => {
                self.selected_layer2_object = index;
                if let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2() {
                    let record = &layer2.objects.records[index];
                    self.layer2_object_form = ObjectForm::from_record(record);
                    self.layer2_object_placement_template = Some(record.clone());
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn move_sprite_to_canvas(
        &mut self,
        index: usize,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some(fields) = sprite_fields_at_canvas_position(
            position,
            canvas,
            cell,
            vertical,
            NativeSpriteRecordFields {
                y_low: self.sprite_form.y_low,
                extra_bits: self.sprite_form.extra_bits,
                screen: self.sprite_form.screen,
                x: self.sprite_form.x,
                sprite_number: self.sprite_form.sprite_number,
            },
        ) else {
            self.error = Some("sprite drag ended outside the native 32×512 tile space".into());
            return;
        };
        self.selected_sprite = index;
        self.sprite_form.y_low = fields.y_low;
        self.sprite_form.screen = fields.screen;
        self.sprite_form.x = fields.x;
        self.apply_dragged_sprite_fields(index);
    }

    fn apply_dragged_sprite_fields(&mut self, index: usize) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let screen = self.sprite_form.screen;
        let x = self.sprite_form.x;
        let y = u16::from(self.sprite_form.y_low);
        let vertical =
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                .vertical;
        let mut predicted = controller.level().sprites.clone();
        let selected = match predicted.relocate_record_position(
            index,
            screen,
            x,
            y,
            vertical,
            controller.sprite_lengths(),
        ) {
            Ok(selected) => selected,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::RelocateSpritePosition {
            selected: index,
            screen,
            x,
            y,
        }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.selected_sprite_group.clear();
                self.selected_sprite_group.push(selected);
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_object_artwork(
        &self,
        painter: &egui::Painter,
        target: egui::Rect,
        cell_size: f32,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
        records: &[ObjectRecord],
        placements: &[lm_level::NativeObjectPlacement],
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        texture: Option<&egui::TextureHandle>,
        texture_variants: Option<&[egui::TextureHandle]>,
        block_contents_texture: Option<&egui::TextureHandle>,
        surface_outline: bool,
        line_guide_outline: bool,
    ) -> HashMap<usize, egui::Rect> {
        let Some(texture) = texture else {
            return HashMap::new();
        };
        draw_ordered_object_tiles(
            painter,
            OrderedObjectDraw {
                texture,
                target,
                cell_size,
                major_tiles,
                minor_tiles,
                vertical,
                records,
                placements,
                handler_map: self.active_standard_object_handler_map(),
                metadata: custom_objects,
                variant: self.active_object_family_index(),
                object_tileset: self.controller.as_ref().map_or(0, |controller| {
                    controller.level().layer1.header.object_tileset()
                }),
                level_mode: self.controller.as_ref().map_or(0, |controller| {
                    controller.level().layer1.header.level_mode()
                }),
                custom_map16,
                foreground_texture: self.foreground_texture.as_ref(),
                texture_variants,
                block_contents_texture,
                outline_texture: self.outline_texture.as_ref(),
                surface_outline,
                line_guide_outline,
                switch_view_state: self.switch_view_state,
                conditional_view_state: self.conditional_view_state,
                blue_pow_active: self.blue_pow_active,
            },
        )
    }

    fn active_standard_object_handler_map(&self) -> Option<&[u8; 64]> {
        let family_index = self.active_object_family_index();
        self.standard_object_map
            .as_ref()?
            .family(usize::from(family_index))
    }

    fn active_object_resize_model(
        &self,
        record: &ObjectRecord,
        custom_objects: Option<&lm_level::OscResolvedTable>,
    ) -> Option<lm_render::StandardObjectResizeModel> {
        if custom_objects.is_some_and(|metadata| {
            metadata
                .default_display(
                    record.command_id(),
                    record.parameter(),
                    self.active_object_family_index(),
                )
                .is_some()
        }) {
            return None;
        }
        let handler_map = self.active_standard_object_handler_map()?;
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).ok()?;
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).ok()?;
        definitions.mapped_resize_model(record, handler_map)
    }

    fn active_object_resize_models(
        &self,
        records: &[ObjectRecord],
        custom_objects: Option<&lm_level::OscResolvedTable>,
    ) -> HashMap<usize, lm_render::StandardObjectResizeModel> {
        let Some(handler_map) = self.active_standard_object_handler_map() else {
            return HashMap::new();
        };
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        if lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).is_err()
            || lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).is_err()
        {
            return HashMap::new();
        }
        let variant = self.active_object_family_index();
        records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                if custom_objects.is_some_and(|metadata| {
                    metadata
                        .default_display(record.command_id(), record.parameter(), variant)
                        .is_some()
                }) {
                    return None;
                }
                let model = definitions.mapped_resize_model(record, handler_map)?;
                (model != lm_render::StandardObjectResizeModel::Fixed).then_some((index, model))
            })
            .collect()
    }

    fn active_object_family_index(&self) -> u8 {
        let tileset = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.header.object_tileset()
        });
        match lm_profile::smw_us_v1_object_family(tileset) {
            lm_profile::VanillaObjectFamily::Normal => 0,
            lm_profile::VanillaObjectFamily::Castle => 1,
            lm_profile::VanillaObjectFamily::Rope => 2,
            lm_profile::VanillaObjectFamily::Underground => 3,
            lm_profile::VanillaObjectFamily::GhostHouse => 4,
        }
    }

    fn canvas_model(&self) -> CanvasModel {
        self.controller
            .as_ref()
            .map(|controller| {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                        .vertical;
                let layer2_objects = match controller.layer2() {
                    Some(lm_level::NativeLayer2Data::Objects(objects)) => Some(objects),
                    Some(lm_level::NativeLayer2Data::Tilemap(_)) | None => None,
                };
                let layer2_tilemap = match controller.layer2() {
                    Some(lm_level::NativeLayer2Data::Tilemap(bytes)) => bytes
                        .chunks_exact(2)
                        .map(|word| u16::from_le_bytes([word[0], word[1]]))
                        .collect(),
                    Some(lm_level::NativeLayer2Data::Objects(_)) | None => Vec::new(),
                };
                CanvasModel {
                    layer1_records: controller.level().layer1.objects.records.clone(),
                    layer1_placements: controller
                        .level()
                        .layer1
                        .objects
                        .native_placements_for_orientation(vertical),
                    layer2_records: layer2_objects
                        .map_or_else(Vec::new, |layer2| layer2.objects.records.clone()),
                    layer2_placements: layer2_objects.map_or_else(Vec::new, |layer2| {
                        layer2.objects.native_placements_for_orientation(vertical)
                    }),
                    layer2_tilemap,
                    sprite_placements: controller.level().sprites.native_placements(),
                }
            })
            .unwrap_or_default()
    }

    fn object_editor(
        &mut self,
        ui: &mut egui::Ui,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        toolbar_images: &MainToolbarImageSet,
    ) {
        let record_count = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.objects.records.len()
        });
        let has_selection = self.selected_object < record_count;
        let resize_model = self
            .controller
            .as_ref()
            .and_then(|controller| {
                controller
                    .level()
                    .layer1
                    .objects
                    .records
                    .get(self.selected_object)
            })
            .and_then(|record| self.active_object_resize_model(record, custom_objects));
        if has_selection {
            ui.label(format!("Object {}", self.selected_object));
        } else {
            ui.label("No selected object.");
        }
        self.catalog_presentation_toolbar(
            ui,
            toolbar_images,
            OriginalToolbarImages::AddObject,
            true,
        );
        self.object_catalog(ui, custom_map16, false);
        self.extended_object_catalog(ui, custom_map16, false);
        self.custom_object_catalog(ui, custom_objects, custom_map16, false);
        self.object_catalog_preview_area(ui, custom_objects, custom_map16, false);
        if let Some((screen, destination_and_flags)) = &mut self.object_form.screen_exit {
            ui.label("Native screen-exit object");
            egui::Grid::new("vanilla-screen-exit-fields").show(ui, |ui| {
                ui.label("Source screen");
                ui.add(egui::DragValue::new(screen).range(0..=0x1f));
                ui.end_row();
                ui.label("Destination / flags");
                ui.add(
                    egui::DragValue::new(destination_and_flags)
                        .range(0..=u16::MAX)
                        .hexadecimal(4, false, true),
                );
                ui.end_row();
            });
            ui.small(
                "Lunar Magic always sets flag 0400. Resulting values below 1000 use the compact four-byte form; higher flag values use the five-byte extended form.",
            );
        } else if let Some((encoding, target)) = &mut self.object_form.screen_jump {
            ui.label(format!(
                "Screen-jump control ({})",
                match encoding {
                    lm_level::ScreenJumpEncoding::FirstLow => "low byte first",
                    lm_level::ScreenJumpEncoding::FirstHigh => "high byte first",
                }
            ));
            let (mut first, mut second) = screen_jump_components(*encoding, *target);
            egui::Grid::new("vanilla-screen-jump-fields").show(ui, |ui| {
                ui.label("First encoded component");
                ui.add(
                    egui::DragValue::new(&mut first)
                        .range(0..=0x1f)
                        .hexadecimal(2, false, true),
                );
                ui.end_row();
                ui.label("Second encoded component");
                ui.add(
                    egui::DragValue::new(&mut second)
                        .range(0..=0x0f)
                        .hexadecimal(2, false, true),
                );
                ui.end_row();
            });
            *target = pack_screen_jump_components(*encoding, first, second);
            ui.small(screen_jump_resolution_label(*encoding, *target));
        } else {
            egui::Grid::new("vanilla-object-fields").show(ui, |ui| {
                header_row(ui, "Command", &mut self.object_form.command_id, 0x3f);
                header_row(ui, "Parameter", &mut self.object_form.parameter, 0xff);
                header_row(
                    ui,
                    "Coordinate A",
                    &mut self.object_form.first_coordinate,
                    0x0f,
                );
                header_row(
                    ui,
                    "Coordinate B",
                    &mut self.object_form.second_coordinate,
                    0x0f,
                );
                ui.label("Advance screen");
                ui.checkbox(&mut self.object_form.advances_screen, "");
                ui.end_row();
            });
            show_standard_object_resize_fields(ui, resize_model, &mut self.object_form);
        }
        show_raw_object_record(ui, "vanilla-layer1-raw-object", &mut self.object_form);
        self.object_action_buttons(ui, record_count, has_selection);
        self.handle_object_paste(ui, record_count);
    }

    fn catalog_presentation_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        images: &MainToolbarImageSet,
        kind: OriginalToolbarImages,
        objects: bool,
    ) {
        let (mut previews, mut compatible_only, mut vertical, mut preview_area, mut zoom) =
            if objects {
                (
                    self.object_catalog_preview_icons.unwrap_or(true),
                    self.object_catalog_compatible_only.unwrap_or(false),
                    self.object_catalog_vertical_layout.unwrap_or(false),
                    self.object_catalog_preview_area.unwrap_or(true),
                    self.object_catalog_preview_zoom.unwrap_or(100),
                )
            } else {
                (
                    self.sprite_catalog_preview_icons.unwrap_or(true),
                    self.sprite_catalog_compatible_only.unwrap_or(false),
                    self.sprite_catalog_vertical_layout.unwrap_or(false),
                    self.sprite_catalog_preview_area.unwrap_or(true),
                    self.sprite_catalog_preview_zoom.unwrap_or(100),
                )
            };
        ui.horizontal(|ui| {
            if images
                .original_catalog_button(
                    ui,
                    kind,
                    OriginalCatalogAction::PreviewIcons,
                    "Show preview icons in list",
                    previews,
                )
                .clicked()
            {
                previews = !previews;
            }
            if images
                .original_catalog_button(
                    ui,
                    kind,
                    OriginalCatalogAction::CompatibleGraphicsOnly,
                    if objects {
                        "Hide objects without the correct BG1/FG3 graphics"
                    } else {
                        "Hide sprites without the correct SP3/SP4 graphics"
                    },
                    compatible_only,
                )
                .clicked()
            {
                compatible_only = !compatible_only;
            }
            ui.add_enabled_ui(preview_area, |ui| {
                if images
                    .original_catalog_button(
                        ui,
                        kind,
                        OriginalCatalogAction::VerticalLayout,
                        "Use vertical layout",
                        vertical,
                    )
                    .clicked()
                {
                    vertical = !vertical;
                }
                let zoom_response = images.original_catalog_button(
                    ui,
                    kind,
                    OriginalCatalogAction::Zoom,
                    &format!("Preview zoom: {zoom}%"),
                    zoom != 100,
                );
                let popup_id = ui.make_persistent_id(if objects {
                    "object-catalog-preview-zoom"
                } else {
                    "sprite-catalog-preview-zoom"
                });
                if zoom_response.clicked() {
                    ui.memory_mut(|memory| memory.toggle_popup(popup_id));
                }
                egui::popup::popup_below_widget(
                    ui,
                    popup_id,
                    &zoom_response,
                    egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        for preset in CATALOG_PREVIEW_ZOOM_MENU {
                            if ui
                                .selectable_label(zoom == preset, format!("{preset}%"))
                                .clicked()
                            {
                                zoom = preset;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Zoom out").clicked() {
                            zoom = change_catalog_preview_zoom(zoom, -100);
                        }
                        if ui.button("Zoom in").clicked() {
                            zoom = change_catalog_preview_zoom(zoom, 100);
                        }
                        if ui.button("Default 100%").clicked() {
                            zoom = 100;
                            ui.close_menu();
                        }
                    },
                );
            });
            if images
                .original_catalog_button(
                    ui,
                    kind,
                    OriginalCatalogAction::PreviewArea,
                    "Show preview area",
                    preview_area,
                )
                .clicked()
            {
                preview_area = !preview_area;
            }
        });
        if objects {
            self.object_catalog_preview_icons = Some(previews);
            self.object_catalog_compatible_only = Some(compatible_only);
            self.object_catalog_vertical_layout = Some(vertical);
            self.object_catalog_preview_area = Some(preview_area);
            self.object_catalog_preview_zoom = Some(zoom);
        } else {
            self.sprite_catalog_preview_icons = Some(previews);
            self.sprite_catalog_compatible_only = Some(compatible_only);
            self.sprite_catalog_vertical_layout = Some(vertical);
            self.sprite_catalog_preview_area = Some(preview_area);
            self.sprite_catalog_preview_zoom = Some(zoom);
        }
    }

    fn object_catalog_preview_area(
        &self,
        ui: &mut egui::Ui,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        layer2: bool,
    ) {
        if !self.object_catalog_preview_area.unwrap_or(true) {
            return;
        }
        let zoom = self
            .object_catalog_preview_zoom
            .unwrap_or(100)
            .clamp(100, 5_000);
        ui.separator();
        ui.label(format!("Object preview · {zoom}%"));
        let side = catalog_preview_side(zoom);
        egui::ScrollArea::both()
            .id_salt("vanilla-object-catalog-preview-area")
            .max_height(320.0)
            .show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                draw_catalog_background(&painter, rect, false);
                let scale = f32::from(zoom) / 100.0;
                if let Some(object) = self
                    .object_catalog_preview_selector
                    .and_then(|selector| custom_objects.and_then(|table| table.get(selector)))
                    && let Some(parts) =
                        lm_render::render_resolved_lunar_magic_custom_object(object)
                {
                    draw_fitted_custom_object_preview(
                        &painter,
                        self.map16_texture.as_ref(),
                        self.foreground_texture.as_ref(),
                        custom_map16,
                        rect,
                        &parts,
                        scale,
                    );
                    return;
                }

                let record = if layer2 {
                    self.layer2_object_placement_template
                        .clone()
                        .or_else(|| self.layer2_object_form.ordinary_record().ok())
                } else {
                    self.object_placement_template
                        .clone()
                        .or_else(|| self.object_form.ordinary_record().ok())
                };
                let object_tileset = self.controller.as_ref().map_or(0, |controller| {
                    controller.level().layer1.header.object_tileset()
                });
                let Some(mut definitions) = standard_object_definitions_for_tileset(object_tileset)
                else {
                    draw_catalog_preview_unavailable(&painter, rect);
                    return;
                };
                if definitions
                    .apply_lunar_magic_switch_view_state(self.switch_view_state)
                    .is_err()
                {
                    draw_catalog_preview_unavailable(&painter, rect);
                    return;
                }
                let Some(tiles) = record.as_ref().and_then(|record| {
                    self.active_standard_object_handler_map()
                        .and_then(|handlers| {
                            object_catalog_record_tiles(record, handlers, &definitions)
                        })
                }) else {
                    draw_catalog_preview_unavailable(&painter, rect);
                    return;
                };
                draw_fitted_object_catalog_preview(
                    &painter,
                    self.map16_texture.as_ref(),
                    self.foreground_texture.as_ref(),
                    custom_map16,
                    rect,
                    &tiles,
                    16.0 * scale,
                );
            });
    }

    fn handle_object_paste(&mut self, ui: &egui::Ui, record_count: usize) {
        if self.paste_target == Some(EntityPasteTarget::Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.paste_object(&text, record_count);
        }
        if let Some(EntityPasteTarget::DirectMap16Rectangle {
            key,
            controller_revision,
        }) = self.paste_target
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.stage_direct_map16_rectangle(&text, key, controller_revision);
        }
    }

    fn object_catalog(
        &mut self,
        ui: &mut egui::Ui,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        layer2: bool,
    ) {
        egui::CollapsingHeader::new("Add structures and platforms")
            .id_salt(if layer2 {
                "vanilla-layer2-standard-object-catalog"
            } else {
                "vanilla-standard-object-catalog"
            })
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hex filter");
                    ui.text_edit_singleline(&mut self.object_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.object_catalog_filter.clear();
                    }
                });
                ui.label("Choose a tileset-resolved object, then click its destination tile.");
                let object_tileset = self.controller.as_ref().map_or(0, |controller| {
                    controller.level().layer1.header.object_tileset()
                });
                let commands = filter_standard_object_catalog_for_graphics(
                    object_catalog_commands(&self.object_catalog_filter),
                    self.object_catalog_compatible_only.unwrap_or(false),
                    self.active_object_family_index(),
                    object_tileset,
                    self.map16_summary.map(|summary| summary.foreground_files),
                );
                let texture = self.map16_texture.clone();
                let foreground_texture = self.foreground_texture.clone();
                let handler_map = self.active_standard_object_handler_map().copied();
                let Some(handler_map) = handler_map else {
                    ui.label("The active standard-object handler map is unavailable.");
                    return;
                };
                let Some(mut definitions) = standard_object_definitions() else {
                    ui.label("The recovered standard-object definitions are unavailable.");
                    return;
                };
                if definitions
                    .apply_lunar_magic_switch_view_state(self.switch_view_state)
                    .is_err()
                {
                    ui.label("The switch-state object previews are unavailable.");
                    return;
                }
                let selected_command = if layer2 {
                    self.layer2_object_form.command_id
                } else {
                    self.object_form.command_id
                };
                let preview_icons = self.object_catalog_preview_icons.unwrap_or(true);
                let vertical_layout = self.object_catalog_vertical_layout.unwrap_or(false);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt(if layer2 {
                        "vanilla-layer2-standard-object-catalog-scroll"
                    } else {
                        "vanilla-standard-object-catalog-scroll"
                    })
                    .max_height(280.0)
                    .show(ui, |ui| {
                        catalog_entry_layout(ui, vertical_layout, |ui| {
                            for command in commands {
                                let response = if preview_icons {
                                    draw_object_catalog_entry(
                                        ui,
                                        texture.as_ref(),
                                        foreground_texture.as_ref(),
                                        custom_map16,
                                        command,
                                        &handler_map,
                                        &definitions,
                                        command == selected_command,
                                    )
                                } else {
                                    ui.selectable_label(
                                        command == selected_command,
                                        format!("Standard object ${command:02X}"),
                                    )
                                };
                                if response.clicked() {
                                    chosen = Some(command);
                                }
                            }
                        });
                    });
                if let Some(command) = chosen {
                    self.select_standard_object_from_catalog(command, layer2);
                }
            });
    }

    fn select_standard_object_from_catalog(&mut self, command: u8, layer2: bool) {
        self.object_catalog_preview_selector = None;
        let form = if layer2 {
            &mut self.layer2_object_form
        } else {
            &mut self.object_form
        };
        form.command_id = command;
        form.parameter = 0;
        form.advances_screen = false;
        if layer2 {
            self.layer2_object_placement_template = None;
            self.placement_mode = Some(CanvasPlacementMode::Layer2Object);
        } else {
            self.object_placement_template = None;
            self.placement_mode = Some(CanvasPlacementMode::Object);
        }
        self.error = None;
    }

    fn custom_object_catalog(
        &mut self,
        ui: &mut egui::Ui,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        layer2: bool,
    ) {
        let Some(custom_objects) = custom_objects else {
            return;
        };
        egui::CollapsingHeader::new("Add custom OSC object visually")
            .id_salt(if layer2 {
                "vanilla-layer2-custom-object-catalog"
            } else {
                "vanilla-custom-object-catalog"
            })
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hex/name filter");
                    ui.text_edit_singleline(&mut self.custom_object_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.custom_object_catalog_filter.clear();
                    }
                });
                let variant = self.active_object_family_index();
                let entries = custom_object_catalog_entries(
                    custom_objects,
                    variant,
                    &self.custom_object_catalog_filter,
                );
                let map16_texture = self.map16_texture.clone();
                let foreground_texture = self.foreground_texture.clone();
                let preview_icons = self.object_catalog_preview_icons.unwrap_or(true);
                let vertical_layout = self.object_catalog_vertical_layout.unwrap_or(false);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt(if layer2 {
                        "vanilla-layer2-custom-object-catalog-scroll"
                    } else {
                        "vanilla-custom-object-catalog-scroll"
                    })
                    .max_height(280.0)
                    .show(ui, |ui| {
                        catalog_entry_layout(ui, vertical_layout, |ui| {
                            for entry in entries {
                                let response = if preview_icons {
                                    draw_custom_object_catalog_entry(
                                        ui,
                                        map16_texture.as_ref(),
                                        foreground_texture.as_ref(),
                                        custom_map16,
                                        entry,
                                    )
                                } else {
                                    ui.selectable_label(
                                        false,
                                        format!(
                                            "${:02X}/${:02X} {}",
                                            entry.selector.object_type,
                                            entry.selector.parameter,
                                            entry.description.as_deref().unwrap_or("custom object")
                                        ),
                                    )
                                };
                                if response.clicked() {
                                    chosen = Some(entry.selector);
                                }
                            }
                        });
                    });
                if let Some(selector) = chosen {
                    self.select_custom_object_from_catalog(selector, layer2);
                }
            });
    }

    fn select_custom_object_from_catalog(
        &mut self,
        selector: lm_level::OscObjectSelector,
        layer2: bool,
    ) {
        match custom_object_native_record(selector) {
            Ok(record) if layer2 => {
                self.object_catalog_preview_selector = Some(selector);
                self.layer2_object_form = ObjectForm::from_record(&record);
                self.layer2_object_placement_template = Some(record);
                self.placement_mode = Some(CanvasPlacementMode::Layer2Object);
                self.error = None;
            }
            Ok(record) => {
                self.object_catalog_preview_selector = Some(selector);
                self.object_form = ObjectForm::from_record(&record);
                self.object_placement_template = Some(record);
                self.placement_mode = Some(CanvasPlacementMode::Object);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn extended_object_catalog(
        &mut self,
        ui: &mut egui::Ui,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        layer2: bool,
    ) {
        egui::CollapsingHeader::new("Add blocks, coins, doors, and small objects")
            .id_salt(if layer2 {
                "vanilla-layer2-extended-object-catalog"
            } else {
                "vanilla-extended-object-catalog"
            })
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hex filter");
                    ui.text_edit_singleline(&mut self.extended_object_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.extended_object_catalog_filter.clear();
                    }
                });
                ui.label(
                    "Choose a tileset-resolved extended object, then click its destination tile.",
                );
                let object_tileset = self.controller.as_ref().map_or(0, |controller| {
                    controller.level().layer1.header.object_tileset()
                });
                let Some(mut definitions) = standard_object_definitions_for_tileset(object_tileset)
                else {
                    ui.label("The recovered extended-object definitions are unavailable.");
                    return;
                };
                if definitions
                    .apply_lunar_magic_switch_view_state(self.switch_view_state)
                    .is_err()
                {
                    ui.label("The switch-state object previews are unavailable.");
                    return;
                }
                let selectors = filter_extended_object_catalog_for_graphics(
                    extended_object_catalog_selectors(
                        &definitions,
                        &self.extended_object_catalog_filter,
                    ),
                    self.object_catalog_compatible_only.unwrap_or(false),
                    object_tileset,
                    self.map16_summary.map(|summary| summary.foreground_files),
                );
                let texture = self.map16_texture.clone();
                let foreground_texture = self.foreground_texture.clone();
                let handler_map = self.active_standard_object_handler_map().copied();
                let Some(handler_map) = handler_map else {
                    ui.label("The active standard-object handler map is unavailable.");
                    return;
                };
                let selected = if layer2 {
                    &self.layer2_object_form
                } else {
                    &self.object_form
                };
                let selected = (selected.command_id, selected.parameter);
                let preview_icons = self.object_catalog_preview_icons.unwrap_or(true);
                let vertical_layout = self.object_catalog_vertical_layout.unwrap_or(false);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt(if layer2 {
                        "vanilla-layer2-extended-object-catalog-scroll"
                    } else {
                        "vanilla-extended-object-catalog-scroll"
                    })
                    .max_height(280.0)
                    .show(ui, |ui| {
                        catalog_entry_layout(ui, vertical_layout, |ui| {
                            for selector in selectors {
                                let response = if preview_icons {
                                    draw_extended_object_catalog_entry(
                                        ui,
                                        texture.as_ref(),
                                        foreground_texture.as_ref(),
                                        custom_map16,
                                        selector,
                                        &handler_map,
                                        &definitions,
                                        selected == (0, selector),
                                    )
                                } else {
                                    ui.selectable_label(
                                        selected == (0, selector),
                                        format!("Extended object $00/${selector:02X}"),
                                    )
                                };
                                if response.clicked() {
                                    chosen = Some(selector);
                                }
                            }
                        });
                    });
                if let Some(selector) = chosen {
                    self.select_extended_object_from_catalog(selector, layer2);
                }
            });
    }

    fn select_extended_object_from_catalog(&mut self, selector: u8, layer2: bool) {
        self.object_catalog_preview_selector = None;
        let record = ObjectRecord::new(vec![0, 0, selector])
            .expect("extended catalog selectors always encode three-byte objects");
        if layer2 {
            self.layer2_object_form = ObjectForm::from_record(&record);
            self.layer2_object_placement_template = Some(record);
            self.placement_mode = Some(CanvasPlacementMode::Layer2Object);
        } else {
            self.object_form = ObjectForm::from_record(&record);
            self.object_placement_template = Some(record);
            self.placement_mode = Some(CanvasPlacementMode::Object);
        }
        self.error = None;
    }

    fn object_action_buttons(
        &mut self,
        ui: &mut egui::Ui,
        record_count: usize,
        has_selection: bool,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Insert after selection").clicked() {
                self.insert_object_after_selection(record_count);
            }
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(if self.object_form.screen_jump.is_some() {
                        "Apply screen jump"
                    } else if self.object_form.screen_exit.is_some() {
                        "Apply screen exit"
                    } else {
                        "Apply object fields"
                    }),
                )
                .clicked()
            {
                let edits = match self.selected_object_field_edits() {
                    Ok(edits) => edits,
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                };
                if let Some(controller) = self.controller.as_mut() {
                    match controller.apply_edits(&[NativeLevelEdit::Objects(edits)]) {
                        Ok(()) => {
                            self.reload_object_form();
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Apply raw record"))
                .clicked()
            {
                let edit = self.object_form.raw_record().map(|record| {
                    NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                        index: self.selected_object,
                        record,
                    }])
                });
                self.apply_object_result(edit);
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Remove object"))
                .clicked()
            {
                self.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
                    index: self.selected_object,
                }])));
            }
            self.object_move_buttons(ui, record_count);
            if ui
                .add_enabled(has_selection, egui::Button::new("Copy"))
                .clicked()
            {
                self.copy_object(ui);
            }
            if ui.button("Paste after selection").clicked() {
                self.paste_target = Some(EntityPasteTarget::Object);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui.button("Paste Map16 rectangle for placement").clicked() {
                match (self.key, self.controller.as_ref()) {
                    (Some(key), Some(controller)) => {
                        self.paste_target = Some(EntityPasteTarget::DirectMap16Rectangle {
                            key,
                            controller_revision: controller.revision(),
                        });
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    }
                    _ => self.error = Some("level controller is unavailable".into()),
                }
            }
        });
    }

    fn stage_direct_map16_rectangle(
        &mut self,
        text: &str,
        requested_key: EditorKey,
        requested_controller_revision: u64,
    ) {
        let request_is_current = self.key == Some(requested_key)
            && self
                .controller
                .as_ref()
                .is_some_and(|controller| controller.revision() == requested_controller_revision);
        if !request_is_current {
            self.error = Some(
                "Map16 rectangle paste was discarded because the level changed while reading the clipboard"
                    .into(),
            );
            return;
        }
        match direct_map16_rectangle_from_clipboard(text) {
            Ok(record) => {
                self.object_form = ObjectForm::from_record(&record);
                self.object_placement_template = Some(record);
                self.placement_mode = Some(CanvasPlacementMode::Object);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn selected_object_field_edits(&self) -> Result<Vec<ObjectEdit>, String> {
        let current = self.controller.as_ref().and_then(|controller| {
            controller
                .level()
                .layer1
                .objects
                .records
                .get(self.selected_object)
        });
        object_field_edits(&self.object_form, self.selected_object, current)
    }

    fn insert_object_after_selection(&mut self, record_count: usize) {
        let insertion = object_insertion_index(self.selected_object, record_count);
        let edit = self.object_record_for_placement().map(|record| {
            NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index: insertion,
                record,
            }])
        });
        let previous_selection = self.selected_object;
        self.selected_object = insertion;
        self.apply_object_result(edit);
        if self.error.is_some() {
            self.selected_object = previous_selection;
        }
    }

    fn apply_object_result(&mut self, edit: Result<NativeLevelEdit, String>) {
        match edit {
            Ok(edit) => {
                let Some(controller) = self.controller.as_mut() else {
                    self.error = Some("level controller is unavailable".into());
                    return;
                };
                match controller.apply_edits(&[edit]) {
                    Ok(()) => {
                        self.reload_object_form();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn reload_object_form(&mut self) {
        let records = self
            .controller
            .as_ref()
            .map(|controller| &controller.level().layer1.objects.records);
        let Some(records) = records else {
            self.selected_object = 0;
            self.object_form = ObjectForm::default();
            self.object_placement_template = None;
            return;
        };
        self.selected_object = self.selected_object.min(records.len().saturating_sub(1));
        if let Some(record) = records.get(self.selected_object).cloned() {
            self.object_form = ObjectForm::from_record(&record);
            self.object_placement_template = Some(record);
        } else {
            self.object_form = ObjectForm::default();
            self.object_placement_template = None;
        }
    }

    fn sprite_list(&mut self, ui: &mut egui::Ui) {
        let animation_phase =
            sprite_animation_phase(self.animation_seconds(ui.input(|input| input.time)));
        let Some(controller) = &self.controller else {
            return;
        };
        let header = controller.level().sprites.header;
        let tokens = controller.level().sprites.tokens.clone();
        let count = tokens.len();
        let placements = controller.level().sprites.native_placements();
        let texture = self.sprite_texture.clone();
        let animated_texture = self
            .animated_sprite_textures
            .get(usize::from(animation_phase))
            .cloned()
            .or_else(|| texture.clone());
        let level_header = &controller.level().layer1.header;
        let vertical = lm_profile::smw_us_v1_level_mode(level_header.level_mode()).vertical;
        let level_mode = level_header.level_mode();
        let sprite_tileset = self.form.sprite_tileset;
        egui::CollapsingHeader::new(format!("Edit existing enemies and sprites ({count})"))
            .id_salt("vanilla-existing-sprites")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Choose a picture, then click the canvas to place a copy in this level.");
                let selected_placement = placements
                    .iter()
                    .find(|placement| placement.token_index == self.selected_sprite);
                let selected_text = selected_placement.map_or_else(
                    || "Choose an existing sprite…".to_owned(),
                    |placement| {
                        format!(
                            "${:02X} · record {} · screen {}",
                            placement.sprite_number, placement.token_index, placement.screen
                        )
                    },
                );
                let mut chosen = None;
                egui::ComboBox::from_id_salt("vanilla-existing-sprite-picture-picker")
                    .selected_text(selected_text)
                    .width(ui.available_width().min(320.0))
                    .show_ui(ui, |ui| {
                        ui.set_min_width(280.0);
                        egui::ScrollArea::vertical()
                            .id_salt("vanilla-existing-sprite-picture-list")
                            .max_height(280.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for placement in &placements {
                                        let index = placement.token_index;
                                        let Some(token) = tokens.get(index) else {
                                            continue;
                                        };
                                        let form = SpriteForm::from_token(header, Some(token));
                                        let mut mode = sprite_catalog_preview_mode(
                                            &form,
                                            vertical,
                                            level_mode,
                                            sprite_tileset,
                                        );
                                        mode.alternate_display = self.silver_pow_active;
                                        let response = draw_sprite_catalog_entry(
                                            ui,
                                            texture.as_ref(),
                                            animated_texture.as_ref(),
                                            placement.sprite_number,
                                            mode,
                                            index == self.selected_sprite,
                                        )
                                        .on_hover_text(format!(
                                            "Existing record {index}\nScreen {}, tile ${:X},{}\nSelect to place a copy",
                                            placement.screen,
                                            placement.major & 0x0f,
                                            placement.minor
                                        ));
                                        if response.clicked() {
                                            chosen = Some((index, form));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            });
                    });
                if let Some((index, form)) = chosen {
                    self.selected_sprite = index;
                    self.sprite_form = form;
                    self.placement_mode = Some(CanvasPlacementMode::Sprite);
                    self.error = None;
                }
                if self.placement_mode == Some(CanvasPlacementMode::Sprite) {
                    ui.label("Placement active: click a destination tile on the canvas.");
                }
                egui::CollapsingHeader::new("Raw stream records and control commands")
                    .id_salt("vanilla-existing-sprite-raw-list")
                    .show(ui, |ui| {
                        for (index, token) in tokens.iter().enumerate() {
                            let text = SpriteForm::from_token(header, Some(token)).encoded;
                            if ui
                                .selectable_label(
                                    index == self.selected_sprite,
                                    format!("{index:03}: {text}"),
                                )
                                .clicked()
                            {
                                self.selected_sprite = index;
                                self.sprite_form = SpriteForm::from_token(header, Some(token));
                            }
                        }
                    });
            });
    }

    fn sprite_editor(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
        toolbar_images: &MainToolbarImageSet,
    ) {
        let token_count = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.level().sprites.tokens.len());
        ui.label("Enemies and sprites stored in this level");
        self.catalog_presentation_toolbar(
            ui,
            toolbar_images,
            OriginalToolbarImages::AddSprite,
            false,
        );
        self.sprite_catalog(ui);
        self.custom_sprite_catalog(ui, custom_sprites, external_assets, custom_map16);
        self.sprite_catalog_preview_area(ui, custom_sprites, external_assets, custom_map16);
        self.sprite_form_controls(ui);
        sprite_save_constraint(ui, self.controller.as_ref());
        self.sprite_editor_actions(ui, token_count);
        if self.paste_target == Some(EntityPasteTarget::Sprite)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.paste_sprite(&text, token_count);
        }
    }

    fn sprite_editor_actions(&mut self, ui: &mut egui::Ui, token_count: usize) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Stage sprite header").clicked()
                && let Some(controller) = self.controller.as_mut()
            {
                let result = self.sprite_form.semantic_header().and_then(|header| {
                    controller
                        .apply_edits(&[NativeLevelEdit::SetSpriteHeader(header)])
                        .map_err(|error| error.to_string())
                });
                match result {
                    Ok(()) => {
                        let level = controller.level();
                        self.sprite_form = SpriteForm::from_token(
                            level.sprites.header,
                            level.sprites.tokens.get(self.selected_sprite),
                        );
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Insert after selection").clicked() {
                self.insert_sprite(token_count);
            }
            if ui
                .add_enabled(
                    self.selected_sprite < token_count,
                    egui::Button::new("Replace record"),
                )
                .clicked()
            {
                let edit = crate::native_level_document_form::parse_sprite_token(
                    &self.sprite_form.encoded,
                )
                .map(|token| NativeLevelEdit::ReplaceSprite {
                    index: self.selected_sprite,
                    token,
                });
                self.apply_sprite_result(edit);
            }
            if ui
                .add_enabled(
                    self.selected_sprite < token_count && self.sprite_form.semantic_record,
                    egui::Button::new("Apply sprite fields"),
                )
                .clicked()
            {
                self.apply_sprite_semantic_fields();
            }
            if ui
                .add_enabled(
                    self.selected_sprite < token_count,
                    egui::Button::new("Remove sprite"),
                )
                .clicked()
                && let Some(controller) = self.controller.as_mut()
            {
                match controller.apply_edits(&[NativeLevelEdit::RemoveSprite {
                    index: self.selected_sprite,
                }]) {
                    Ok(()) => {
                        self.selected_sprite =
                            self.selected_sprite.min(token_count.saturating_sub(2));
                        self.sprite_form = SpriteForm::from_token(
                            controller.level().sprites.header,
                            controller.level().sprites.tokens.get(self.selected_sprite),
                        );
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            self.sprite_move_buttons(ui, token_count);
            let selected_record = self
                .controller
                .as_ref()
                .and_then(|controller| controller.level().sprites.tokens.get(self.selected_sprite))
                .and_then(|token| match token {
                    SpriteToken::Record(record) => Some(record),
                    SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
                });
            if ui
                .add_enabled(selected_record.is_some(), egui::Button::new("Copy record"))
                .clicked()
                && let Some(record) = selected_record
            {
                match crate::native_clipboard::encode_level_sprite(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Paste record after selection").clicked() {
                self.paste_target = Some(EntityPasteTarget::Sprite);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
    }

    fn sprite_catalog(&mut self, ui: &mut egui::Ui) {
        let animation_phase =
            sprite_animation_phase(self.animation_seconds(ui.input(|input| input.time)));
        egui::CollapsingHeader::new("Add new enemies and sprites")
            .id_salt("vanilla-standard-sprite-catalog")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hex filter");
                    ui.text_edit_singleline(&mut self.sprite_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.sprite_catalog_filter.clear();
                    }
                });
                ui.label(
                    "Choose a recovered standard-sprite preview, then click its destination tile.",
                );
                let ids = filter_standard_sprite_catalog_for_graphics(
                    sprite_catalog_ids(&self.sprite_catalog_filter),
                    self.sprite_catalog_compatible_only.unwrap_or(false),
                    self.form.sprite_tileset,
                    self.map16_summary.map(|summary| summary.sprite_files),
                );
                let texture = self.sprite_texture.clone();
                let animated_texture = self
                    .animated_sprite_textures
                    .get(usize::from(animation_phase))
                    .cloned()
                    .or_else(|| texture.clone());
                let (vertical, level_mode) =
                    self.controller.as_ref().map_or((false, 0), |controller| {
                        let header = &controller.level().layer1.header;
                        (
                            lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical,
                            header.level_mode(),
                        )
                    });
                let mut mode = sprite_catalog_preview_mode(
                    &self.sprite_form,
                    vertical,
                    level_mode,
                    self.form.sprite_tileset,
                );
                mode.alternate_display = self.silver_pow_active;
                let preview_icons = self.sprite_catalog_preview_icons.unwrap_or(true);
                let vertical_layout = self.sprite_catalog_vertical_layout.unwrap_or(false);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-standard-sprite-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        catalog_entry_layout(ui, vertical_layout, |ui| {
                            for id in ids {
                                let response = if preview_icons {
                                    draw_sprite_catalog_entry(
                                        ui,
                                        texture.as_ref(),
                                        animated_texture.as_ref(),
                                        id,
                                        mode,
                                        id == self.sprite_form.sprite_number,
                                    )
                                } else {
                                    ui.selectable_label(
                                        id == self.sprite_form.sprite_number,
                                        format!("Standard sprite ${id:02X}"),
                                    )
                                };
                                if response.clicked() {
                                    chosen = Some(id);
                                }
                            }
                        });
                    });
                if let Some(id) = chosen {
                    self.choose_standard_sprite(id);
                }
            });
    }

    fn custom_sprite_catalog(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let animation_phase =
            sprite_animation_phase(self.animation_seconds(ui.input(|input| input.time)));
        let Some(custom_sprites) = custom_sprites else {
            return;
        };
        egui::CollapsingHeader::new("Add custom enemies and sprites")
            .id_salt("vanilla-custom-sprite-catalog")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hex/name filter");
                    ui.text_edit_singleline(&mut self.custom_sprite_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.custom_sprite_catalog_filter.clear();
                    }
                });
                let entries = custom_sprite_catalog_entries(
                    custom_sprites,
                    &self.custom_sprite_catalog_filter,
                );
                let texture = self.sprite_texture.clone();
                let animated_texture = self
                    .animated_sprite_textures
                    .get(usize::from(animation_phase))
                    .cloned()
                    .or_else(|| texture.clone());
                let preview_icons = self.sprite_catalog_preview_icons.unwrap_or(true);
                let vertical_layout = self.sprite_catalog_vertical_layout.unwrap_or(false);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-custom-sprite-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        catalog_entry_layout(ui, vertical_layout, |ui| {
                            for entry in entries {
                                let response = if preview_icons {
                                    let atlas_parts =
                                        lm_render::render_atlas_lunar_magic_custom_sprite_with(
                                            custom_sprites,
                                            entry,
                                            |index| external_sprite_definition(custom_map16, index),
                                        );
                                    let external_parts =
                                        lm_render::render_remapped_lunar_magic_custom_sprite_with(
                                            custom_sprites,
                                            entry,
                                            |index| external_sprite_definition(custom_map16, index),
                                        );
                                    if let Some(parts) = external_parts.as_deref() {
                                        ensure_remapped_part_textures(
                                            ui.ctx(),
                                            &mut self.external_sprite_textures,
                                            parts,
                                            SpriteRasterAssets {
                                                external: external_assets,
                                                foreground_tiles: &self.foreground_tiles,
                                                layer3_tiles: &self.layer3_tiles,
                                                vanilla_tiles: &self.sprite_tiles,
                                                vanilla_palette: self.sprite_palette.as_ref(),
                                            },
                                        );
                                    }
                                    draw_custom_sprite_catalog_entry(
                                        ui,
                                        texture.as_ref(),
                                        animated_texture.as_ref(),
                                        entry,
                                        atlas_parts.as_deref(),
                                        external_parts.as_deref(),
                                        &self.external_sprite_textures,
                                    )
                                } else {
                                    ui.selectable_label(
                                        false,
                                        format!(
                                            "${:02X} · E{} {}",
                                            entry.selector.sprite_number,
                                            entry.selector.extra_bits,
                                            entry.description.as_deref().unwrap_or("custom sprite")
                                        ),
                                    )
                                };
                                if response.clicked() {
                                    chosen = Some(entry.selector);
                                }
                            }
                        });
                    });
                if let Some(selector) = chosen {
                    self.choose_custom_sprite(selector);
                }
            });
    }

    fn sprite_catalog_preview_area(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        if !self.sprite_catalog_preview_area.unwrap_or(true) {
            return;
        }
        let zoom = self
            .sprite_catalog_preview_zoom
            .unwrap_or(100)
            .clamp(100, 5_000);
        let scale = f32::from(zoom) / 100.0;
        let side = catalog_preview_side(zoom);
        let animation_phase =
            sprite_animation_phase(self.animation_seconds(ui.input(|input| input.time)));
        let texture = self.sprite_texture.clone();
        let animated_texture = self
            .animated_sprite_textures
            .get(usize::from(animation_phase))
            .cloned()
            .or_else(|| texture.clone());
        let custom = self
            .sprite_catalog_preview_selector
            .and_then(|selector| custom_sprites.and_then(|table| table.get(selector)));
        let (atlas_parts, external_parts) = custom.map_or((None, None), |entry| {
            (
                lm_render::render_atlas_lunar_magic_custom_sprite_with(
                    custom_sprites.expect("custom entry requires its table"),
                    entry,
                    |index| external_sprite_definition(custom_map16, index),
                ),
                lm_render::render_remapped_lunar_magic_custom_sprite_with(
                    custom_sprites.expect("custom entry requires its table"),
                    entry,
                    |index| external_sprite_definition(custom_map16, index),
                ),
            )
        });
        if let Some(parts) = external_parts.as_deref() {
            ensure_remapped_part_textures(
                ui.ctx(),
                &mut self.external_sprite_textures,
                parts,
                SpriteRasterAssets {
                    external: external_assets,
                    foreground_tiles: &self.foreground_tiles,
                    layer3_tiles: &self.layer3_tiles,
                    vanilla_tiles: &self.sprite_tiles,
                    vanilla_palette: self.sprite_palette.as_ref(),
                },
            );
        }

        ui.separator();
        ui.label(format!("Sprite preview · {zoom}%"));
        egui::ScrollArea::both()
            .id_salt("vanilla-sprite-catalog-preview-area")
            .max_height(320.0)
            .show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                draw_catalog_background(&painter, rect, false);
                if custom.is_some()
                    && let (Some(texture), Some(parts)) = (texture.as_ref(), atlas_parts.as_deref())
                {
                    draw_fitted_sprite_catalog_preview(
                        &painter,
                        texture,
                        animated_texture.as_ref(),
                        None,
                        rect,
                        parts,
                        scale,
                    );
                    return;
                }
                if custom.is_some()
                    && let Some(parts) = external_parts.as_deref()
                    && parts
                        .iter()
                        .all(|part| self.external_sprite_textures.contains_key(part))
                {
                    draw_fitted_external_sprite_catalog_preview(
                        &painter,
                        rect,
                        parts,
                        &self.external_sprite_textures,
                        scale,
                    );
                    return;
                }

                let Some(texture) = texture.as_ref() else {
                    draw_catalog_preview_unavailable(&painter, rect);
                    return;
                };
                let (vertical, level_mode) =
                    self.controller.as_ref().map_or((false, 0), |controller| {
                        let header = &controller.level().layer1.header;
                        (
                            lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical,
                            header.level_mode(),
                        )
                    });
                let mut mode = sprite_catalog_preview_mode(
                    &self.sprite_form,
                    vertical,
                    level_mode,
                    self.form.sprite_tileset,
                );
                mode.alternate_display = self.silver_pow_active;
                let Some(parts) = lm_render::render_lunar_magic_standard_sprite_with_mode(
                    self.sprite_form.sprite_number,
                    mode,
                ) else {
                    draw_catalog_preview_unavailable(&painter, rect);
                    return;
                };
                draw_fitted_sprite_catalog_preview(
                    &painter,
                    texture,
                    animated_texture.as_ref(),
                    Some(self.sprite_form.sprite_number),
                    rect,
                    &parts,
                    scale,
                );
            });
    }

    fn choose_custom_sprite(&mut self, selector: lm_level::SscSpriteSelector) {
        let Some(controller) = self.controller.as_ref() else {
            self.error = Some("native level controller is unavailable".into());
            return;
        };
        let fields = NativeSpriteRecordFields {
            y_low: self.sprite_form.y_low,
            extra_bits: selector.extra_bits,
            screen: self.sprite_form.screen,
            x: self.sprite_form.x,
            sprite_number: selector.sprite_number,
        };
        match custom_sprite_token(fields, controller.sprite_lengths()) {
            Ok(token) => {
                self.sprite_catalog_preview_selector = Some(selector);
                self.sprite_form = SpriteForm::from_token(self.sprite_form.header, Some(&token));
                self.placement_mode = Some(CanvasPlacementMode::Sprite);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn choose_standard_sprite(&mut self, sprite_number: u8) {
        self.sprite_catalog_preview_selector = None;
        let fields = NativeSpriteRecordFields {
            y_low: self.sprite_form.y_low,
            extra_bits: self.sprite_form.extra_bits,
            screen: self.sprite_form.screen,
            x: self.sprite_form.x,
            sprite_number,
        };
        let lengths = self
            .controller
            .as_ref()
            .map_or_else(SpriteLengthTable::standard, |controller| {
                controller.sprite_lengths().clone()
            });
        match standard_sprite_token(fields, &lengths) {
            Ok(token) => {
                self.sprite_form = SpriteForm::from_token(self.sprite_form.header, Some(&token));
                self.placement_mode = Some(CanvasPlacementMode::Sprite);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn sprite_form_controls(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("vanilla-sprite-fields").show(ui, |ui| {
            header_row(
                ui,
                "Sprite memory",
                &mut self.sprite_form.sprite_memory,
                lm_level::NativeSpriteHeader::MAX_MEMORY,
            );
            ui.label("Sprite buoyancy 1");
            ui.checkbox(
                &mut self.sprite_form.sprite_buoyancy_1,
                "Water/lava interaction",
            );
            ui.end_row();
            ui.label("Sprite buoyancy 2");
            ui.checkbox(
                &mut self.sprite_form.sprite_buoyancy_2,
                "Water/lava; disable Layer 2/3 interaction",
            );
            ui.end_row();
            ui.label("Record bytes");
            ui.text_edit_singleline(&mut self.sprite_form.encoded);
            ui.end_row();
            header_row(
                ui,
                "Sprite number",
                &mut self.sprite_form.sprite_number,
                0xff,
            );
            header_row(ui, "Screen", &mut self.sprite_form.screen, 0x1f);
            header_row(ui, "X", &mut self.sprite_form.x, 0x0f);
            header_row(ui, "Y (low 5 bits)", &mut self.sprite_form.y_low, 0x1f);
            header_row(ui, "Extra bits", &mut self.sprite_form.extra_bits, 3);
        });
    }

    fn apply_sprite_semantic_fields(&mut self) {
        let edit = self.controller.as_ref().map_or_else(
            || Err("native level controller is unavailable".into()),
            |controller| {
                self.sprite_form.semantic_edit(
                    self.selected_sprite,
                    controller.level().sprites.tokens.get(self.selected_sprite),
                    controller.sprite_lengths(),
                )
            },
        );
        let edit = match edit {
            Ok(edit) => edit,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("native level controller is unavailable".into());
            return;
        };
        let NativeLevelEdit::SetSpriteFields { index, fields } = edit else {
            unreachable!("semantic sprite edit is always typed fields");
        };
        let vertical =
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                .vertical;
        let mut predicted = controller.level().sprites.clone();
        let selected =
            match predicted.set_record_fields(index, fields, vertical, controller.sprite_lengths())
            {
                Ok(selected) => selected,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
        match controller.apply_edits(&[NativeLevelEdit::SetSpriteFields { index, fields }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn apply_sprite_result(&mut self, edit: Result<NativeLevelEdit, String>) {
        match edit {
            Ok(edit) => {
                if let Some(controller) = self.controller.as_mut() {
                    let selected = if !controller.level().sprites.expanded
                        && let NativeLevelEdit::ReplaceSprite { index, token } = &edit
                    {
                        let mut predicted = controller.level().sprites.clone();
                        predicted.tokens[*index] = token.clone();
                        match predicted.sort_legacy_records_by_screen(*index) {
                            Ok(selected) => selected,
                            Err(error) => {
                                self.error = Some(error.to_string());
                                return;
                            }
                        }
                    } else {
                        self.selected_sprite
                    };
                    match controller.apply_edits(&[edit]) {
                        Ok(()) => {
                            self.selected_sprite = selected;
                            self.sprite_form = SpriteForm::from_token(
                                controller.level().sprites.header,
                                controller.level().sprites.tokens.get(selected),
                            );
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn insert_sprite(&mut self, token_count: usize) {
        let token = match crate::native_level_document_form::parse_sprite_token(
            &self.sprite_form.encoded,
        ) {
            Ok(token) => token,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let index = sprite_insertion_index(self.selected_sprite, token_count);
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let selected = if controller.level().sprites.expanded {
            index
        } else {
            let mut predicted = controller.level().sprites.clone();
            if let Err(error) = predicted.insert(index, token.clone()) {
                self.error = Some(error.to_string());
                return;
            }
            match predicted.sort_legacy_records_by_screen(index) {
                Ok(selected) => selected,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::InsertSprite { index, token }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn copy_object(&mut self, ui: &egui::Ui) {
        let Some(record) = self.controller.as_ref().and_then(|controller| {
            controller
                .level()
                .layer1
                .objects
                .records
                .get(self.selected_object)
        }) else {
            return;
        };
        match crate::native_clipboard::encode_level_object(record) {
            Ok(text) => ui.ctx().copy_text(text),
            Err(error) => self.error = Some(error),
        }
    }

    fn paste_object(&mut self, text: &str, record_count: usize) {
        let index = object_insertion_index(self.selected_object, record_count);
        let records = match crate::native_clipboard::decode_level_objects(text) {
            Ok(records) if !records.is_empty() => records,
            Ok(_) => {
                self.error = Some("level-object paste requires at least one object".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let inserted = index..index + records.len();
        let edits = records
            .into_iter()
            .enumerate()
            .map(|(offset, record)| ObjectEdit::Insert {
                index: index + offset,
                record,
            })
            .collect();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(edits)]) {
            Ok(()) => {
                self.selected_object = index;
                self.selected_object_group = inserted.collect();
                self.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
                self.reload_object_form();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn paste_layer2_object(&mut self, text: &str, record_count: usize) {
        let index = object_insertion_index(self.selected_layer2_object, record_count);
        let records = match crate::native_clipboard::decode_level_objects(text) {
            Ok(records) if !records.is_empty() => records,
            Ok(_) => {
                self.error = Some("Layer 2 object paste requires at least one object".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let inserted = index..index + records.len();
        let edits = records
            .into_iter()
            .enumerate()
            .map(|(offset, record)| ObjectEdit::Insert {
                index: index + offset,
                record,
            })
            .collect::<Vec<_>>();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_layer2_object_edits(&edits) {
            Ok(()) => {
                self.selected_layer2_object = index;
                self.selected_layer2_object_group = inserted.collect();
                self.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
                if let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2()
                    && let Some(record) = layer2.objects.records.get(index)
                {
                    self.layer2_object_form = ObjectForm::from_record(record);
                    self.layer2_object_placement_template = Some(record.clone());
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn paste_sprite(&mut self, text: &str, token_count: usize) {
        let index = sprite_insertion_index(self.selected_sprite, token_count);
        let records = match crate::native_clipboard::decode_level_sprites(text) {
            Ok(records) if !records.is_empty() => records,
            Ok(_) => {
                self.error = Some("level-sprite paste requires at least one sprite".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let last_inserted = index + records.len() - 1;
        let mut edits = records
            .into_iter()
            .enumerate()
            .map(|(offset, record)| NativeLevelEdit::InsertSprite {
                index: index + offset,
                token: SpriteToken::Record(record),
            })
            .collect::<Vec<_>>();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let selected = if controller.level().sprites.expanded {
            last_inserted
        } else {
            let mut predicted = controller.level().sprites.clone();
            for edit in &edits {
                let NativeLevelEdit::InsertSprite { index, token } = edit else {
                    unreachable!("pasted sprite edits are insertions");
                };
                if let Err(error) = predicted.insert(*index, token.clone()) {
                    self.error = Some(error.to_string());
                    return;
                }
            }
            let selected = match predicted.sort_legacy_records_by_screen(last_inserted) {
                Ok(selected) => selected,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
            edits.push(NativeLevelEdit::SortLegacySpritesByScreen {
                selected: last_inserted,
            });
            selected
        };
        match controller.apply_edits(&edits) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.selected_sprite_group.clear();
                self.selected_sprite_group.push(selected);
                self.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(selected),
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn move_object(&mut self, record_count: usize, down: bool) {
        let Some((before, selected)) =
            move_before_indexes(self.selected_object, record_count, down)
        else {
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::MoveBefore {
            from: self.selected_object,
            before,
        }])]) {
            Ok(()) => {
                self.selected_object = selected;
                self.reload_object_form();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn object_move_buttons(&mut self, ui: &mut egui::Ui, record_count: usize) {
        if ui
            .add_enabled(self.selected_object > 0, egui::Button::new("Move up"))
            .clicked()
        {
            self.move_object(record_count, false);
        }
        if ui
            .add_enabled(
                self.selected_object.saturating_add(1) < record_count,
                egui::Button::new("Move down"),
            )
            .clicked()
        {
            self.move_object(record_count, true);
        }
    }

    fn move_layer2_object(&mut self, record_count: usize, down: bool) {
        let Some((before, selected)) =
            move_before_indexes(self.selected_layer2_object, record_count, down)
        else {
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_layer2_object_edits(&[ObjectEdit::MoveBefore {
            from: self.selected_layer2_object,
            before,
        }]) {
            Ok(()) => {
                self.selected_layer2_object = selected;
                if let Some(lm_level::NativeLayer2Data::Objects(layer2)) = controller.layer2()
                    && let Some(record) = layer2.objects.records.get(selected)
                {
                    self.layer2_object_form = ObjectForm::from_record(record);
                    self.layer2_object_placement_template = Some(record.clone());
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn layer2_object_move_buttons(&mut self, ui: &mut egui::Ui, record_count: usize) {
        if ui
            .add_enabled(
                self.selected_layer2_object > 0,
                egui::Button::new("Move up"),
            )
            .clicked()
        {
            self.move_layer2_object(record_count, false);
        }
        if ui
            .add_enabled(
                self.selected_layer2_object.saturating_add(1) < record_count,
                egui::Button::new("Move down"),
            )
            .clicked()
        {
            self.move_layer2_object(record_count, true);
        }
    }

    fn move_sprite(&mut self, token_count: usize, down: bool) {
        let Some((before, selected)) = move_before_indexes(self.selected_sprite, token_count, down)
        else {
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[NativeLevelEdit::MoveSpriteBefore {
            from: self.selected_sprite,
            before,
        }]) {
            Ok(()) => {
                self.selected_sprite = selected;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn sprite_move_buttons(&mut self, ui: &mut egui::Ui, token_count: usize) {
        if ui
            .add_enabled(self.selected_sprite > 0, egui::Button::new("Move up"))
            .clicked()
        {
            self.move_sprite(token_count, false);
        }
        if ui
            .add_enabled(
                self.selected_sprite.saturating_add(1) < token_count,
                egui::Button::new("Move down"),
            )
            .clicked()
        {
            self.move_sprite(token_count, true);
        }
    }
}

fn canvas_entity_shortcut(response: &egui::Response) -> Option<CanvasEntityShortcut> {
    if !response.has_focus() {
        return None;
    }
    response.ctx.input_mut(|input| {
        let modifiers = input.modifiers;
        if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd {
            return input
                .consume_key(modifiers, egui::Key::A)
                .then_some(CanvasEntityShortcut::SelectAll);
        }
        if modifiers.any() {
            return None;
        }
        if input.consume_key(modifiers, egui::Key::Insert) {
            Some(CanvasEntityShortcut::Insert)
        } else if input.consume_key(modifiers, egui::Key::Delete)
            || input.consume_key(modifiers, egui::Key::Backspace)
        {
            Some(CanvasEntityShortcut::Remove)
        } else {
            None
        }
    })
}

fn mode_change_resets_layer2(source_mode: u8, target_mode: u8, layer2_loaded: bool) -> bool {
    layer2_loaded
        && lm_level::level_mode_layer2_storage(source_mode)
            != lm_level::level_mode_layer2_storage(target_mode)
}

const fn layer2_tilemap_editable(shared_vanilla_background: bool) -> bool {
    !shared_vanilla_background
}

const fn sprite_insertion_index(selected: usize, token_count: usize) -> usize {
    if selected < token_count {
        selected + 1
    } else {
        token_count
    }
}

fn selected_indexes(group: &[usize], fallback: usize) -> Vec<usize> {
    let mut indexes = if group.is_empty() {
        vec![fallback]
    } else {
        group.to_vec()
    };
    indexes.sort_unstable();
    indexes.dedup();
    indexes
}

fn screen_nudge_delta(vertical: bool, x_delta: i32, y_delta: i32) -> (i32, i32) {
    if vertical {
        (y_delta, x_delta)
    } else {
        (x_delta, y_delta)
    }
}

fn overlap_z_order_permutation(
    order: &[usize],
    selected: &[usize],
    bounds: &HashMap<usize, egui::Rect>,
    traversal: ZOrderTraversal,
    can_cross: impl Fn(&usize, &usize) -> bool,
) -> Result<Vec<usize>, String> {
    if selected.is_empty() {
        return Err("Z-order adjustment requires a nonempty selection".into());
    }
    let selected_set = selected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for identity in selected_set.iter().copied() {
        if !order.contains(&identity) {
            return Err(format!("selected Z-order record {identity} is unavailable"));
        }
        if !bounds.contains_key(&identity) {
            return Err("render the active canvas before changing overlap-aware Z order".into());
        }
    }
    let increase = matches!(traversal, ZOrderTraversal::Forward | ZOrderTraversal::Front);
    let farthest = matches!(traversal, ZOrderTraversal::Front | ZOrderTraversal::Back);
    let selected_iteration = if increase {
        selected.iter().rev().copied().collect::<Vec<_>>()
    } else {
        selected.to_vec()
    };
    let mut reordered = order.to_vec();
    for identity in selected_iteration {
        let Some(position) = reordered.iter().position(|value| *value == identity) else {
            continue;
        };
        let source_bounds = bounds[&identity];
        let candidates = if increase {
            reordered[position + 1..]
                .iter()
                .copied()
                .collect::<Vec<_>>()
        } else {
            reordered[..position]
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>()
        };
        let mut matches = candidates.into_iter().filter(|candidate| {
            !selected_set.contains(candidate)
                && can_cross(&identity, candidate)
                && bounds.get(candidate).is_some_and(|candidate_bounds| {
                    strict_rect_overlap(source_bounds, *candidate_bounds)
                })
        });
        let target = if farthest {
            matches.last()
        } else {
            matches.next()
        };
        let Some(target) = target else {
            continue;
        };
        reordered.remove(position);
        let target_position = reordered
            .iter()
            .position(|value| *value == target)
            .expect("overlap target remains after removing a selected identity");
        reordered.insert(
            if increase {
                target_position + 1
            } else {
                target_position
            },
            identity,
        );
    }
    Ok(reordered)
}

fn strict_rect_overlap(left: egui::Rect, right: egui::Rect) -> bool {
    left.min.x < right.max.x
        && right.min.x < left.max.x
        && left.min.y < right.max.y
        && right.min.y < left.max.y
}

fn object_insertion_index(selected: usize, record_count: usize) -> usize {
    selected.saturating_add(1).min(record_count)
}

fn pasted_object_edit(text: &str, index: usize) -> Result<NativeLevelEdit, String> {
    crate::native_clipboard::decode_level_object(text)
        .map(|record| NativeLevelEdit::Objects(vec![ObjectEdit::Insert { index, record }]))
}

fn direct_map16_rectangle_from_clipboard(text: &str) -> Result<ObjectRecord, String> {
    let rectangle = crate::native_clipboard::decode_native_map16_rectangle(text)?;
    let source_tile = u16::try_from(rectangle.source_index)
        .map_err(|_| "Map16 rectangle source lies outside the Direct Map16 namespace".to_owned())?;
    let width = u8::try_from(rectangle.width).map_err(|_| {
        "Map16 rectangle width is not representable by a Direct Map16 object".to_owned()
    })?;
    let height = u8::try_from(rectangle.height).map_err(|_| {
        "Map16 rectangle height is not representable by a Direct Map16 object".to_owned()
    })?;
    ObjectRecord::direct_map16_rectangle(source_tile, width, height)
        .map_err(|error| error.to_string())
}

fn pasted_sprite_edit(text: &str, index: usize) -> Result<NativeLevelEdit, String> {
    crate::native_clipboard::decode_level_sprite(text).map(|record| NativeLevelEdit::InsertSprite {
        index,
        token: SpriteToken::Record(record),
    })
}

const fn move_before_indexes(selected: usize, count: usize, down: bool) -> Option<(usize, usize)> {
    if down {
        if selected.saturating_add(1) < count {
            Some((selected + 2, selected + 1))
        } else {
            None
        }
    } else if selected > 0 && selected < count {
        Some((selected - 1, selected - 1))
    } else {
        None
    }
}

fn draw_object_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    cell: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
) {
    let (columns, rows) = if vertical {
        (minor_tiles, major_tiles)
    } else {
        (major_tiles, minor_tiles)
    };
    for column in 0..=columns {
        draw_grid_line(painter, rect, cell, column, true);
    }
    for row in 0..=rows {
        draw_grid_line(painter, rect, cell, row, false);
    }
}

fn tile_grid_visible(
    editor_overlays: bool,
    visibility: crate::application::LevelViewVisibility,
) -> bool {
    editor_overlays && visibility.tile_grid
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LevelScreenGridRegion {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    label: String,
}

fn level_screen_grid_regions(
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
) -> Vec<LevelScreenGridRegion> {
    let screens = major_tiles.div_ceil(16);
    let secondary_regions = minor_tiles.div_ceil(16);
    let mut regions = Vec::with_capacity(usize::from(screens * secondary_regions));
    for screen in 0..screens {
        for secondary in 0..secondary_regions {
            let secondary_start = secondary * 16;
            let secondary_size = (minor_tiles - secondary_start).min(16);
            let (x, y, width, height, side) = if vertical {
                (
                    secondary_start,
                    screen * 16,
                    secondary_size,
                    (major_tiles - screen * 16).min(16),
                    if secondary == 0 { "Left" } else { "Right" },
                )
            } else {
                (
                    screen * 16,
                    secondary_start,
                    (major_tiles - screen * 16).min(16),
                    secondary_size,
                    if secondary == 0 { "Top" } else { "Bottom" },
                )
            };
            regions.push(LevelScreenGridRegion {
                x,
                y,
                width,
                height,
                label: format!("{screen:02X} : {side}"),
            });
        }
    }
    regions
}

fn draw_level_screen_grid(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
) {
    let outline = egui::Color32::from_rgb(0, 160, 0);
    for region in level_screen_grid_regions(major_tiles, minor_tiles, vertical) {
        let min = canvas.min + egui::vec2(f32::from(region.x) * cell, f32::from(region.y) * cell);
        let size = egui::vec2(
            f32::from(region.width) * cell,
            f32::from(region.height) * cell,
        );
        let rect = egui::Rect::from_min_size(min, size).intersect(canvas);
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.5_f32, outline),
            egui::StrokeKind::Inside,
        );
        let label_at = rect.min + egui::vec2(4.0, 4.0);
        painter.text(
            label_at,
            egui::Align2::LEFT_TOP,
            region.label,
            egui::FontId::monospace((cell * 0.75).clamp(8.0, 16.0)),
            egui::Color32::WHITE,
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LevelScreenExitAnnotation {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    label: String,
}

fn screen_exit_table(records: &[ObjectRecord]) -> [Option<u16>; 32] {
    let mut exits = [None; 32];
    for exit in records.iter().filter_map(ObjectRecord::screen_exit) {
        exits[usize::from(exit.screen)] = Some(exit.destination_and_flags);
    }
    exits
}

fn screen_at_canvas_position(position: egui::Pos2, geometry: LevelCanvasGeometry) -> Option<u8> {
    if !geometry.rect.contains(position) || !geometry.cell.is_finite() || geometry.cell <= 0.0 {
        return None;
    }
    let column = ((position.x - geometry.rect.left()) / geometry.cell).floor();
    let row = ((position.y - geometry.rect.top()) / geometry.cell).floor();
    if column < 0.0 || row < 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (major, minor) = if geometry.vertical {
        (row as u16, column as u16)
    } else {
        (column as u16, row as u16)
    };
    if major >= geometry.major_tiles || minor >= geometry.minor_tiles {
        return None;
    }
    u8::try_from(major / 16).ok().filter(|screen| *screen < 32)
}

fn level_screen_exit_annotations(
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    records: &[ObjectRecord],
    secondary_exits: Option<&SecondaryExitTable>,
) -> Vec<LevelScreenExitAnnotation> {
    let mut exits = [None; 32];
    for exit in records.iter().filter_map(ObjectRecord::screen_exit) {
        exits[usize::from(exit.screen)] = Some(exit.destination_and_flags);
    }
    let screens = major_tiles.div_ceil(16).min(32);
    (0..screens)
        .map(|screen| {
            let (x, y, width, height) = if vertical {
                (
                    0,
                    screen * 16,
                    minor_tiles,
                    (major_tiles - screen * 16).min(16),
                )
            } else {
                (
                    screen * 16,
                    0,
                    (major_tiles - screen * 16).min(16),
                    minor_tiles,
                )
            };
            let label = exits[usize::from(screen)].map_or_else(
                || format!("{screen:02X}"),
                |destination_and_flags| {
                    screen_exit_annotation_label(screen, destination_and_flags, secondary_exits)
                },
            );
            LevelScreenExitAnnotation {
                x,
                y,
                width,
                height,
                label,
            }
        })
        .collect()
}

fn screen_exit_annotation_label(
    screen: u16,
    destination_and_flags: u16,
    secondary_exits: Option<&SecondaryExitTable>,
) -> String {
    let destination = (destination_and_flags >> 3) & 0x1e00 | destination_and_flags & 0x01ff;
    if destination_and_flags & 0x0200 != 0 {
        let resolved = secondary_exits
            .and_then(|table| table.entries.get(usize::from(destination)))
            .copied();
        if resolved.is_some_and(|exit| exit.x_and_overworld_flags & 0x80 != 0) {
            format!("{screen:02X} : Secondary Exit {destination:X} to OV")
        } else if let Some(exit) = resolved {
            format!(
                "{screen:02X} : Secondary Exit {destination:X} to {:X}",
                exit.destination_level
            )
        } else {
            format!("{screen:02X} : Secondary Exit {destination:X}")
        }
    } else if destination_and_flags & 0x0800 != 0 {
        format!("{screen:02X} : Midway Exit to Level {destination:X}")
    } else {
        format!("{screen:02X} : Exit to Level {destination:X}")
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_level_screen_exit_annotations(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    records: &[ObjectRecord],
    secondary_exits: Option<&SecondaryExitTable>,
) {
    let outline = egui::Color32::from_rgb(255, 0, 0);
    for annotation in
        level_screen_exit_annotations(major_tiles, minor_tiles, vertical, records, secondary_exits)
    {
        let min = canvas.min
            + egui::vec2(
                f32::from(annotation.x) * cell,
                f32::from(annotation.y) * cell,
            );
        let rect = egui::Rect::from_min_size(
            min,
            egui::vec2(
                f32::from(annotation.width) * cell,
                f32::from(annotation.height) * cell,
            ),
        )
        .intersect(canvas);
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.5_f32, outline),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.min + egui::vec2(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            annotation.label,
            egui::FontId::monospace((cell * 0.75).clamp(8.0, 16.0)),
            egui::Color32::WHITE,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LevelBoundaryGuideGeometry {
    x_tiles: f32,
    y_tiles: f32,
    width_tiles: f32,
    height_tiles: f32,
}

fn level_boundary_guide_geometry(level_mode: u8, camera: (u16, u16)) -> LevelBoundaryGuideGeometry {
    let mode = lm_profile::smw_us_v1_level_mode(level_mode);
    let (width_pixels, height_pixels) = match (mode.alternate_layer_layout, mode.vertical) {
        (true, true) => (448.0_f32, 224.0_f32),
        (true, false) => (352.0_f32, 232.0_f32),
        _ => (256.0_f32, 232.0_f32),
    };
    LevelBoundaryGuideGeometry {
        x_tiles: f32::from(camera.0),
        y_tiles: f32::from(camera.1),
        width_tiles: width_pixels / 16.0,
        height_tiles: height_pixels / 16.0,
    }
}

fn draw_level_boundary_guide(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell: f32,
    level_mode: u8,
    camera: (u16, u16),
) {
    let geometry = level_boundary_guide_geometry(level_mode, camera);
    let margin = cell / 4.0;
    let min = canvas.min
        + egui::vec2(
            geometry.x_tiles * cell - margin,
            geometry.y_tiles * cell - margin,
        );
    let size = egui::vec2(
        geometry.width_tiles * cell + margin * 2.0,
        geometry.height_tiles * cell + margin * 2.0,
    );
    painter.rect_stroke(
        egui::Rect::from_min_size(min, size).intersect(canvas),
        0.0,
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 224, 224)),
        egui::StrokeKind::Inside,
    );
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

fn draw_grid_line(painter: &egui::Painter, rect: egui::Rect, cell: f32, index: u16, column: bool) {
    let coordinate = f32::from(index) * cell;
    let stroke = grid_line_stroke(index);
    let points = if column {
        let x = rect.left() + coordinate;
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())]
    } else {
        let y = rect.top() + coordinate;
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)]
    };
    painter.line_segment(points, stroke);
}

fn grid_line_stroke(index: u16) -> egui::Stroke {
    if index % 16 == 0 {
        egui::Stroke::new(
            1.5_f32,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 72),
        )
    } else {
        egui::Stroke::new(
            0.5_f32,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 24),
        )
    }
}

fn canvas_major_tiles(
    objects: &[lm_level::NativeObjectPlacement],
    sprites: &[lm_level::NativeSpritePlacement],
) -> u16 {
    let object_end = objects
        .iter()
        .map(|placement| {
            placement
                .major
                .saturating_add(u16::from(placement.major_span))
        })
        .max()
        .unwrap_or(16);
    let sprite_end = sprites
        .iter()
        .map(|placement| placement.major.saturating_add(1))
        .max()
        .unwrap_or(16);
    object_end.max(sprite_end).clamp(16, 512)
}

fn object_stream_major_tiles(records: &[ObjectRecord]) -> u16 {
    let stream = lm_level::ObjectStream {
        records: records.to_vec(),
    };
    let furthest_screen = stream
        .native_placements()
        .into_iter()
        .map(|placement| placement.screen)
        .chain(
            records
                .iter()
                .filter_map(ObjectRecord::screen_jump)
                .map(lm_level::ObjectScreenJump::resolved_screen),
        )
        .max()
        .unwrap_or(0)
        .min(31);
    furthest_screen.saturating_add(1).saturating_mul(16)
}

fn extended_command27_canvas_extent(
    records: &[ObjectRecord],
    placements: &[lm_level::NativeObjectPlacement],
    resize_models: &HashMap<usize, lm_render::StandardObjectResizeModel>,
    vertical: bool,
) -> (u16, u16) {
    let (major, minor) = placements
        .iter()
        .filter_map(|placement| {
            (resize_models.get(&placement.record_index)
                == Some(&lm_render::StandardObjectResizeModel::ExtendedCommand27Axes))
            .then_some(())?;
            let record = records.get(placement.record_index)?;
            let (width, height) = record.extended_command27_tile_size()?;
            let (x, y) = placement.tile_coordinates(vertical);
            let x_end = x.saturating_add(u16::from(width));
            let y_end = y.saturating_add(u16::from(height));
            Some(if vertical {
                (y_end, x_end)
            } else {
                (x_end, y_end)
            })
        })
        .fold((16, 16), |(major, minor), (next_major, next_minor)| {
            (major.max(next_major), minor.max(next_minor))
        });
    (major.clamp(16, 512), minor.clamp(16, 32))
}

fn rendered_standard_object_canvas_extent(
    records: &[ObjectRecord],
    handler_map: &[u8; 64],
    vertical: bool,
) -> Option<(u16, u16)> {
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).ok()?;
    lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).ok()?;
    let layout = lm_render::NativeLevelMap16Layout {
        width: if vertical {
            usize::from(VERTICAL_LEVEL_MINOR_TILES)
        } else {
            512
        },
        height: if vertical {
            512
        } else {
            usize::from(NATIVE_LEVEL_MINOR_TILES)
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical,
    };
    let stream = lm_level::ObjectStream {
        records: records.to_vec(),
    };
    let report = lm_render::render_mapped_standard_object_stream(
        &stream,
        &definitions,
        handler_map,
        layout,
        VANILLA_EMPTY_MAP16_TILE,
    )
    .ok()?;
    let mut major_end = 16_u16;
    let mut minor_end = 16_u16;
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            if !report.cache.was_written(index) {
                continue;
            }
            let (major, minor) = if vertical { (y, x) } else { (x, y) };
            major_end = major_end.max(u16::try_from(major + 1).ok()?);
            minor_end = minor_end.max(u16::try_from(minor + 1).ok()?);
        }
    }
    Some((
        major_end
            .min(object_stream_major_tiles(records))
            .clamp(16, 512),
        minor_end.clamp(16, 32),
    ))
}

fn clamp_canvas_zoom(zoom: u16) -> u16 {
    zoom.clamp(ROM_LEVEL_CANVAS_MIN_ZOOM, ROM_LEVEL_CANVAS_MAX_ZOOM)
}

fn change_catalog_preview_zoom(current: u16, delta: i16) -> u16 {
    i32::from(current)
        .saturating_add(i32::from(delta))
        .clamp(100, 5_000) as u16
}

fn catalog_preview_side(zoom: u16) -> f32 {
    CATALOG_PREVIEW_LOGICAL_SIDE * f32::from(zoom.clamp(100, 5_000)) / 100.0
}

fn rom_canvas_size(major_tiles: u16, minor_tiles: u16, vertical: bool, cell: f32) -> egui::Vec2 {
    let major = f32::from(major_tiles) * cell;
    let minor = f32::from(minor_tiles) * cell;
    if vertical {
        egui::vec2(minor, major)
    } else {
        egui::vec2(major, minor)
    }
}

fn object_catalog_commands(filter: &str) -> Vec<u8> {
    let filter = filter.trim().to_ascii_uppercase();
    (1..=0x3f)
        .filter(|command| filter.is_empty() || format!("{command:02X}").contains(&filter))
        .collect()
}

fn filter_standard_object_catalog_for_graphics(
    commands: Vec<u8>,
    compatible_only: bool,
    family: u8,
    object_tileset: u8,
    foreground_files: Option<[usize; 4]>,
) -> Vec<u8> {
    if !compatible_only {
        return commands;
    }
    let Some(foreground_files) = foreground_files else {
        return commands;
    };
    commands
        .into_iter()
        .filter(|&command| {
            crate::catalog_graphics_compatibility::standard_object_is_graphics_compatible(
                command,
                family,
                object_tileset,
                foreground_files,
            )
        })
        .collect()
}

fn filter_extended_object_catalog_for_graphics(
    selectors: Vec<u8>,
    compatible_only: bool,
    object_tileset: u8,
    foreground_files: Option<[usize; 4]>,
) -> Vec<u8> {
    if !compatible_only {
        return selectors;
    }
    let Some(foreground_files) = foreground_files else {
        return selectors;
    };
    selectors
        .into_iter()
        .filter(|&selector| {
            crate::catalog_graphics_compatibility::extended_object_is_graphics_compatible(
                selector,
                object_tileset,
                foreground_files,
            )
        })
        .collect()
}

fn extended_object_catalog_selectors(
    definitions: &lm_render::StandardObjectDefinitionSet,
    filter: &str,
) -> Vec<u8> {
    let filter = filter.trim().to_ascii_uppercase();
    (4..=u8::MAX)
        .filter(|selector| definitions.get_extended(*selector).is_some())
        .filter(|selector| filter.is_empty() || format!("{selector:02X}").contains(&filter))
        .collect()
}

fn custom_object_catalog_entries<'a>(
    table: &'a lm_level::OscResolvedTable,
    variant: u8,
    filter: &str,
) -> Vec<&'a lm_level::OscResolvedObject> {
    let filter = filter.trim().to_ascii_lowercase();
    let mut entries = Vec::new();
    for object in table.objects() {
        let selector = object.selector;
        if selector.object_type == 0 || selector.variant != variant || object.display.is_none() {
            continue;
        }
        if entries.iter().any(|entry: &&lm_level::OscResolvedObject| {
            entry.selector.object_type == selector.object_type
                && entry.selector.parameter == selector.parameter
        }) {
            continue;
        }
        let label = format!("{:02x}/{:02x}", selector.object_type, selector.parameter);
        let description_matches = object
            .description
            .as_deref()
            .is_some_and(|description| description.to_ascii_lowercase().contains(&filter));
        if filter.is_empty() || label.contains(&filter) || description_matches {
            entries.push(object);
        }
    }
    entries
}

fn custom_object_native_record(
    selector: lm_level::OscObjectSelector,
) -> Result<ObjectRecord, String> {
    if selector.object_type == 0 || selector.object_type > 0x3f {
        return Err(format!(
            "OSC object type {:02X} is not an ordinary placeable command",
            selector.object_type
        ));
    }
    let command = selector.object_type;
    let mut encoded = vec![
        (command & 0x30) << 1,
        (command & 0x0f) << 4,
        selector.parameter,
        0,
        0,
        0,
        0,
        0,
    ];
    let length = lm_level::encoded_record_length(&encoded)
        .ok_or_else(|| "OSC object has no representable native record shape".to_string())?;
    if !(3..=8).contains(&length) {
        return Err(format!(
            "OSC object requires unsupported native record length {length}"
        ));
    }
    encoded.truncate(length);
    ObjectRecord::new(encoded).map_err(|error| error.to_string())
}

fn draw_custom_object_catalog_entry(
    ui: &mut egui::Ui,
    map16_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    object: &lm_level::OscResolvedObject,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(78.0, 70.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    draw_catalog_background(&painter, rect, false);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 22.0),
    );
    let parts = lm_render::render_resolved_lunar_magic_custom_object(object);
    if let Some(parts) = parts {
        draw_fitted_custom_object_preview(
            &painter,
            map16_texture,
            foreground_texture,
            custom_map16,
            preview_rect,
            &parts,
            1.0,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        format!(
            "${:02X}/${:02X}",
            object.selector.object_type, object.selector.parameter
        ),
        egui::FontId::monospace(9.0),
        egui::Color32::WHITE,
    );
    response.on_hover_text(format!(
        "{}\nObject ${:02X}, parameter ${:02X}, variant {}",
        object.description.as_deref().unwrap_or("custom OSC object"),
        object.selector.object_type,
        object.selector.parameter,
        object.selector.variant
    ))
}

fn draw_fitted_custom_object_preview(
    painter: &egui::Painter,
    map16_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    target: egui::Rect,
    parts: &[lm_render::CustomObjectPreviewTile],
    max_scale: f32,
) {
    let min_x = parts.iter().map(|part| part.x).min().unwrap_or(0);
    let min_y = parts.iter().map(|part| part.y).min().unwrap_or(0);
    let max_x = parts
        .iter()
        .map(|part| part.x.saturating_add(16))
        .max()
        .unwrap_or(16);
    let max_y = parts
        .iter()
        .map(|part| part.y.saturating_add(16))
        .max()
        .unwrap_or(16);
    let width = f32::from(max_x.saturating_sub(min_x).max(1));
    let height = f32::from(max_y.saturating_sub(min_y).max(1));
    let scale = (target.width() / width)
        .min(target.height() / height)
        .min(max_scale);
    let origin = target.center() - egui::vec2(width * scale, height * scale) / 2.0;
    for part in parts {
        let position = origin
            + egui::vec2(
                f32::from(part.x.saturating_sub(min_x)) * scale,
                f32::from(part.y.saturating_sub(min_y)) * scale,
            );
        let tile_rect = egui::Rect::from_min_size(position, egui::vec2(16.0 * scale, 16.0 * scale));
        match map16_paint_source(part.tile, custom_map16) {
            Map16PaintSource::Base(tile) => {
                if let Some(texture) = map16_texture {
                    draw_map16_atlas_tile(painter, texture, tile_rect, tile);
                } else {
                    draw_unresolved_map16_paint(painter, tile_rect, part.tile);
                }
            }
            Map16PaintSource::Custom(definition) => {
                if let Some(texture) = foreground_texture {
                    draw_custom_map16_tile(painter, texture, tile_rect, definition);
                } else {
                    draw_unresolved_map16_paint(painter, tile_rect, part.tile);
                }
            }
            Map16PaintSource::Unresolved => {
                draw_unresolved_map16_paint(painter, tile_rect, part.tile);
            }
        }
    }
}

fn standard_object_definitions() -> Option<lm_render::StandardObjectDefinitionSet> {
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).ok()?;
    lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).ok()?;
    Some(definitions)
}

fn standard_object_definitions_for_tileset(
    object_tileset: u8,
) -> Option<lm_render::StandardObjectDefinitionSet> {
    let mut definitions = standard_object_definitions()?;
    lm_render::install_lunar_magic_tileset_extended_objects(&mut definitions, object_tileset)
        .ok()?;
    Some(definitions)
}

fn draw_object_catalog_entry(
    ui: &mut egui::Ui,
    map16_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    command: u8,
    handler_map: &[u8; 64],
    definitions: &lm_render::StandardObjectDefinitionSet,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(62.0, 62.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    draw_catalog_background(&painter, rect, selected);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 15.0),
    );
    if let Some(tiles) = object_catalog_tiles(command, handler_map, definitions) {
        draw_fitted_object_catalog_preview(
            &painter,
            map16_texture,
            foreground_texture,
            custom_map16,
            preview_rect,
            &tiles,
            16.0,
        );
    } else {
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{command:02X}"),
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_BLUE,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 7.0),
        egui::Align2::CENTER_CENTER,
        format!("{command:02X}"),
        egui::FontId::monospace(10.0),
        egui::Color32::WHITE,
    );
    response.on_hover_text(format!("Standard object ${command:02X}"))
}

fn draw_extended_object_catalog_entry(
    ui: &mut egui::Ui,
    map16_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    selector: u8,
    handler_map: &[u8; 64],
    definitions: &lm_render::StandardObjectDefinitionSet,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(62.0, 62.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    draw_catalog_background(&painter, rect, selected);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 15.0),
    );
    let record = ObjectRecord::new(vec![0, 0, selector])
        .expect("extended catalog selectors always encode three-byte objects");
    if let Some(tiles) = object_catalog_record_tiles(&record, handler_map, definitions) {
        draw_fitted_object_catalog_preview(
            &painter,
            map16_texture,
            foreground_texture,
            custom_map16,
            preview_rect,
            &tiles,
            16.0,
        );
    } else {
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{selector:02X}"),
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_BLUE,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 7.0),
        egui::Align2::CENTER_CENTER,
        format!("{selector:02X}"),
        egui::FontId::monospace(10.0),
        egui::Color32::WHITE,
    );
    response.on_hover_text(format!("Extended object ${selector:02X}"))
}

fn draw_catalog_background(painter: &egui::Painter, rect: egui::Rect, selected: bool) {
    painter.rect_filled(
        rect,
        3.0,
        if selected {
            egui::Color32::from_rgb(65, 72, 45)
        } else {
            egui::Color32::from_gray(28)
        },
    );
    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(
            if selected { 2.0_f32 } else { 1.0_f32 },
            if selected {
                egui::Color32::YELLOW
            } else {
                egui::Color32::from_gray(70)
            },
        ),
        egui::StrokeKind::Inside,
    );
}

fn draw_catalog_preview_unavailable(painter: &egui::Painter, rect: egui::Rect) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "preview unavailable",
        egui::FontId::monospace(12.0),
        egui::Color32::GRAY,
    );
}

fn catalog_entry_layout(
    ui: &mut egui::Ui,
    vertical: bool,
    add_entries: impl FnOnce(&mut egui::Ui),
) {
    if vertical {
        ui.vertical(add_entries);
    } else {
        ui.horizontal_wrapped(add_entries);
    }
}

fn object_catalog_tiles(
    command: u8,
    handler_map: &[u8; 64],
    definitions: &lm_render::StandardObjectDefinitionSet,
) -> Option<Vec<(usize, usize, u16)>> {
    let record = ObjectForm {
        command_id: command,
        ..ObjectForm::default()
    }
    .ordinary_record()
    .ok()?;
    object_catalog_record_tiles(&record, handler_map, definitions)
}

fn object_catalog_record_tiles(
    record: &ObjectRecord,
    handler_map: &[u8; 64],
    definitions: &lm_render::StandardObjectDefinitionSet,
) -> Option<Vec<(usize, usize, u16)>> {
    let layout = lm_render::NativeLevelMap16Layout {
        width: 16,
        height: 16,
        page_stride: 0x1b0,
        base_cell: 0,
        vertical: false,
    };
    let report = lm_render::render_mapped_standard_object_stream(
        &lm_level::ObjectStream {
            records: vec![record.clone()],
        },
        definitions,
        handler_map,
        layout,
        u16::MAX,
    )
    .ok()?;
    (report.rendered_objects == 1)
        .then(|| {
            let mut tiles = Vec::new();
            for y in 0..layout.height {
                for x in 0..layout.width {
                    let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
                    let tile = report.cache.cells()[index];
                    if tile != u16::MAX {
                        tiles.push((x, y, tile));
                    }
                }
            }
            tiles
        })
        .filter(|tiles| !tiles.is_empty())
}

fn draw_fitted_object_catalog_preview(
    painter: &egui::Painter,
    map16_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    target: egui::Rect,
    tiles: &[(usize, usize, u16)],
    max_cell: f32,
) {
    let Some(min_x) = tiles.iter().map(|(x, _, _)| *x).min() else {
        return;
    };
    let min_y = tiles.iter().map(|(_, y, _)| *y).min().unwrap_or(0);
    let max_x = tiles.iter().map(|(x, _, _)| *x).max().unwrap_or(min_x);
    let max_y = tiles.iter().map(|(_, y, _)| *y).max().unwrap_or(min_y);
    let width = f32::from(u16::try_from(max_x - min_x + 1).expect("catalog width is at most 16"));
    let height = f32::from(u16::try_from(max_y - min_y + 1).expect("catalog height is at most 16"));
    let cell = (target.width() / width)
        .min(target.height() / height)
        .min(max_cell);
    let origin = target.center() - egui::vec2(width * cell, height * cell) / 2.0;
    for &(x, y, tile) in tiles {
        let relative_x = u16::try_from(x - min_x).expect("catalog x is at most 15");
        let relative_y = u16::try_from(y - min_y).expect("catalog y is at most 15");
        let position =
            origin + egui::vec2(f32::from(relative_x) * cell, f32::from(relative_y) * cell);
        let tile_rect = egui::Rect::from_min_size(position, egui::vec2(cell, cell));
        match map16_paint_source(tile, custom_map16) {
            Map16PaintSource::Base(tile) => {
                if let Some(texture) = map16_texture {
                    draw_map16_atlas_tile(painter, texture, tile_rect, tile);
                } else {
                    draw_unresolved_map16_paint(painter, tile_rect, tile);
                }
            }
            Map16PaintSource::Custom(definition) => {
                if let Some(texture) = foreground_texture {
                    draw_custom_map16_tile(painter, texture, tile_rect, definition);
                } else {
                    draw_unresolved_map16_paint(painter, tile_rect, tile);
                }
            }
            Map16PaintSource::Unresolved => {
                draw_unresolved_map16_paint(painter, tile_rect, tile);
            }
        }
    }
}

fn sprite_catalog_ids(filter: &str) -> Vec<u8> {
    let filter = filter.trim().to_ascii_uppercase();
    (0..=STANDARD_SPRITE_MAX)
        .filter(|id| filter.is_empty() || format!("{id:02X}").contains(&filter))
        .collect()
}

fn filter_standard_sprite_catalog_for_graphics(
    mut ids: Vec<u8>,
    enabled: bool,
    graphics_mode: u8,
    sprite_files: Option<[usize; 4]>,
) -> Vec<u8> {
    if enabled && let Some(sprite_files) = sprite_files {
        ids.retain(|sprite| {
            crate::catalog_graphics_compatibility::standard_sprite_is_graphics_compatible(
                *sprite,
                graphics_mode,
                sprite_files,
            )
        });
    }
    ids
}

fn custom_sprite_catalog_entries<'a>(
    table: &'a lm_level::SscResolvedTable,
    filter: &str,
) -> Vec<&'a lm_level::SscResolvedSprite> {
    let filter = filter.trim().to_ascii_lowercase();
    let mut entries = Vec::new();
    for sprite in table.sprites() {
        let selector = sprite.selector;
        if selector.alternate || sprite.display.is_none() {
            continue;
        }
        if entries.iter().any(|entry: &&lm_level::SscResolvedSprite| {
            entry.selector.sprite_number == selector.sprite_number
                && entry.selector.extra_bits == selector.extra_bits
        }) {
            continue;
        }
        let label = format!("{:02x}/{:x}", selector.sprite_number, selector.extra_bits);
        let description_matches = sprite
            .description
            .as_deref()
            .is_some_and(|description| description.to_ascii_lowercase().contains(&filter));
        if filter.is_empty() || label.contains(&filter) || description_matches {
            entries.push(sprite);
        }
    }
    entries
}

fn ssc_sprite_lengths_signature(table: Option<&lm_level::SscResolvedTable>) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    if let Some(table) = table {
        for sprite in table.sprites() {
            for byte in [
                sprite.selector.sprite_number,
                sprite.selector.extra_bits,
                sprite.selector.record_length.unwrap_or(0),
                u8::from(sprite.selector.alternate),
            ] {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(PRIME);
            }
        }
    }
    hash
}

fn sprite_lengths_from_ssc(
    table: Option<&lm_level::SscResolvedTable>,
) -> Result<SpriteLengthTable, String> {
    let mut lengths = SpriteLengthTable::standard();
    let mut assigned = [None; SpriteLengthTable::ENCODED_LEN];
    let Some(table) = table else {
        return Ok(lengths);
    };
    for sprite in table.sprites() {
        let selector = sprite.selector;
        let Some(record_length) = selector.record_length else {
            continue;
        };
        let index = usize::from(selector.extra_bits) * 256 + usize::from(selector.sprite_number);
        let Some(slot) = assigned.get_mut(index) else {
            return Err(format!(
                "SSC sprite {:02X} uses invalid extra-bit table {}",
                selector.sprite_number, selector.extra_bits
            ));
        };
        if let Some(previous) = *slot
            && previous != record_length
        {
            return Err(format!(
                "SSC sprite {:02X} table {} declares conflicting record lengths {} and {}",
                selector.sprite_number, selector.extra_bits, previous, record_length
            ));
        }
        *slot = Some(record_length);
        lengths
            .set(selector.extra_bits, selector.sprite_number, record_length)
            .map_err(|error| error.to_string())?;
    }
    Ok(lengths)
}

fn standard_sprite_token(
    fields: NativeSpriteRecordFields,
    lengths: &SpriteLengthTable,
) -> Result<SpriteToken, String> {
    let mut record = lm_level::SpriteRecord {
        encoded: vec![0, 0, fields.sprite_number],
    };
    record
        .set_native_fields(fields, lengths)
        .map_err(|error| error.to_string())?;
    Ok(SpriteToken::Record(record))
}

fn custom_sprite_token(
    fields: NativeSpriteRecordFields,
    lengths: &SpriteLengthTable,
) -> Result<SpriteToken, String> {
    let first = packed_sprite_first(fields);
    let record_length = lengths
        .record_len(&[first, 0, fields.sprite_number])
        .ok_or_else(|| "custom sprite has no valid native record length".to_string())?;
    let mut record = lm_level::SpriteRecord {
        encoded: vec![0; record_length],
    };
    record.encoded[2] = fields.sprite_number;
    record
        .set_native_fields(fields, lengths)
        .map_err(|error| error.to_string())?;
    Ok(SpriteToken::Record(record))
}

const fn packed_sprite_first(fields: NativeSpriteRecordFields) -> u8 {
    (fields.y_low & 0x0f) << 4
        | (fields.extra_bits & 3) << 2
        | (fields.screen >> 4) << 1
        | fields.y_low >> 4
}

fn sprite_catalog_preview_mode(
    form: &SpriteForm,
    vertical: bool,
    level_mode: u8,
    sprite_tileset: u8,
) -> lm_render::StandardSpritePreviewMode {
    let placement_first = (form.y_low & 0x0f) << 4 | (form.x & 0x0f);
    lm_render::StandardSpritePreviewMode {
        placement_first,
        placement_major: u16::from(form.screen)
            .saturating_mul(16)
            .saturating_add(u16::from(form.x)),
        placement_minor: u16::from(form.y_low),
        level_mode,
        sprite_graphics_mode: sprite_tileset,
        placement_preview_mode: true,
        level_orientation: if vertical {
            lm_render::StandardLevelOrientation::Vertical
        } else {
            lm_render::StandardLevelOrientation::Horizontal
        },
        ..lm_render::StandardSpritePreviewMode::default()
    }
}

fn draw_custom_sprite_catalog_entry(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    animated_texture: Option<&egui::TextureHandle>,
    sprite: &lm_level::SscResolvedSprite,
    parts: Option<&[lm_render::StandardSpritePreviewTile]>,
    external_parts: Option<&[lm_render::RemappedCustomSpritePreviewTile]>,
    external_textures: &HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(78.0, 70.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    draw_catalog_background(&painter, rect, false);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 22.0),
    );
    if let (Some(texture), Some(parts)) = (texture, parts) {
        draw_fitted_sprite_catalog_preview(
            &painter,
            texture,
            animated_texture,
            None,
            preview_rect,
            parts,
            1.0,
        );
    } else if let Some(parts) = external_parts
        && parts
            .iter()
            .all(|part| external_textures.contains_key(part))
    {
        draw_fitted_external_sprite_catalog_preview(
            &painter,
            preview_rect,
            parts,
            external_textures,
            1.0,
        );
    } else {
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{:02X}", sprite.selector.sprite_number),
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_RED,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        format!(
            "${:02X} · E{}",
            sprite.selector.sprite_number, sprite.selector.extra_bits
        ),
        egui::FontId::monospace(9.0),
        egui::Color32::WHITE,
    );
    let description = sprite.description.as_deref().unwrap_or("custom SSC sprite");
    response.on_hover_text(format!(
        "{description}\nSprite ${:02X}, extra bits {}, record length {}",
        sprite.selector.sprite_number,
        sprite.selector.extra_bits,
        sprite
            .selector
            .record_length
            .map_or_else(|| "default".into(), |length| length.to_string())
    ))
}

fn draw_fitted_external_sprite_catalog_preview(
    painter: &egui::Painter,
    target: egui::Rect,
    parts: &[lm_render::RemappedCustomSpritePreviewTile],
    textures: &HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    max_scale: f32,
) {
    let min_x = parts.iter().map(|part| part.x).min().unwrap_or(0);
    let min_y = parts.iter().map(|part| part.y).min().unwrap_or(0);
    let max_x = parts
        .iter()
        .map(|part| part.x.saturating_add(16))
        .max()
        .unwrap_or(16);
    let max_y = parts
        .iter()
        .map(|part| part.y.saturating_add(16))
        .max()
        .unwrap_or(16);
    let width = f32::from(max_x.saturating_sub(min_x).max(1));
    let height = f32::from(max_y.saturating_sub(min_y).max(1));
    let scale = (target.width() / width)
        .min(target.height() / height)
        .min(max_scale);
    let origin = target.center() - egui::vec2(width * scale, height * scale) / 2.0;
    for part in parts {
        let Some(texture) = textures.get(part) else {
            continue;
        };
        let position = origin
            + egui::vec2(
                f32::from(part.x.saturating_sub(min_x)) * scale,
                f32::from(part.y.saturating_sub(min_y)) * scale,
            );
        draw_external_sprite_part(
            painter,
            texture,
            egui::Rect::from_min_size(position, egui::vec2(16.0 * scale, 16.0 * scale)),
        );
    }
}

fn draw_sprite_catalog_entry(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    animated_texture: Option<&egui::TextureHandle>,
    sprite_number: u8,
    mode: lm_render::StandardSpritePreviewMode,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(62.0, 62.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    draw_catalog_background(&painter, rect, selected);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 15.0),
    );
    let parts = lm_render::render_lunar_magic_standard_sprite_with_mode(sprite_number, mode);
    if let (Some(texture), Some(parts)) = (texture, parts) {
        draw_fitted_sprite_catalog_preview(
            &painter,
            texture,
            animated_texture,
            Some(sprite_number),
            preview_rect,
            &parts,
            1.0,
        );
    } else if lm_render::lunar_magic_standard_sprite_preview_source(sprite_number)
        == lm_render::StandardSpritePreviewSource::NativeEmpty
    {
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            "native\nempty",
            egui::FontId::monospace(9.0),
            egui::Color32::GRAY,
        );
    } else {
        painter.text(
            preview_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{sprite_number:02X}"),
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_RED,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 7.0),
        egui::Align2::CENTER_CENTER,
        format!("{sprite_number:02X}"),
        egui::FontId::monospace(10.0),
        egui::Color32::WHITE,
    );
    let source = lm_render::lunar_magic_standard_sprite_preview_source(sprite_number);
    response.on_hover_text(format!(
        "Standard sprite ${sprite_number:02X}\nPreview source: {source:?}"
    ))
}

fn draw_fitted_sprite_catalog_preview(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    animated_texture: Option<&egui::TextureHandle>,
    standard_sprite_number: Option<u8>,
    target: egui::Rect,
    parts: &[lm_render::StandardSpritePreviewTile],
    max_scale: f32,
) {
    let min_x = parts.iter().map(|part| part.x).min().unwrap_or(0);
    let min_y = parts.iter().map(|part| part.y).min().unwrap_or(0);
    let max_x = parts
        .iter()
        .map(|part| part.x.saturating_add(16))
        .max()
        .unwrap_or(16);
    let max_y = parts
        .iter()
        .map(|part| part.y.saturating_add(16))
        .max()
        .unwrap_or(16);
    let width = f32::from(max_x.saturating_sub(min_x).max(1));
    let height = f32::from(max_y.saturating_sub(min_y).max(1));
    let scale = (target.width() / width)
        .min(target.height() / height)
        .min(max_scale);
    let origin = target.center() - egui::vec2(width * scale, height * scale) / 2.0;
    for part in parts {
        let position = origin
            + egui::vec2(
                f32::from(part.x.saturating_sub(min_x)) * scale,
                f32::from(part.y.saturating_sub(min_y)) * scale,
            );
        draw_sprite_preview_definition_tinted(
            painter,
            texture,
            animated_texture,
            egui::Rect::from_min_size(position, egui::vec2(16.0 * scale, 16.0 * scale)),
            part.subtiles,
            sprite_preview_source_tint(standard_sprite_number, part.definition_index),
        );
    }
}

fn sprite_preview_source_tint(
    standard_sprite_number: Option<u8>,
    definition_index: u16,
) -> egui::Color32 {
    standard_sprite_number.map_or(egui::Color32::WHITE, |sprite_number| {
        standard_sprite_preview_tint(sprite_number, definition_index)
    })
}

fn sprite_fields_at_canvas_position(
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
    mut fields: NativeSpriteRecordFields,
) -> Option<NativeSpriteRecordFields> {
    if !canvas.contains(position) || !cell.is_finite() || cell <= 0.0 {
        return None;
    }
    let column = ((position.x - canvas.left()) / cell).floor();
    let row = ((position.y - canvas.top()) / cell).floor();
    if !(0.0..=f32::from(u16::MAX)).contains(&column) || !(0.0..=f32::from(u16::MAX)).contains(&row)
    {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (major, minor) = if vertical {
        (row as u16, column as u16)
    } else {
        (column as u16, row as u16)
    };
    if major >= 0x200 || minor >= level_minor_tile_limit(vertical) {
        return None;
    }
    fields.screen = u8::try_from(major / 16).ok()?;
    fields.x = u8::try_from(major % 16).ok()?;
    fields.y_low = u8::try_from(minor).ok()?;
    Some(fields)
}

/// Reproduces Lunar Magic's horizontal-level entrance scroll calculation in
/// `FinalizeLoadedLevelEditorState`: the entrance Y table selects the 16-tile page, with a
/// three-tile correction only for low positions `$E0` and `$F0`.
fn vanilla_horizontal_entrance_scroll_row(entrance: VanillaMainEntrance) -> u16 {
    let index = usize::from(entrance.position & 0x0f);
    let low = VANILLA_ENTRANCE_Y_LOW[index];
    let mut row = u16::from(VANILLA_ENTRANCE_Y_HIGH[index]) * 16;
    if low >> 4 > 0x0d {
        row += 3;
    }
    row
}

/// Returns Lunar Magic's label anchor in level pixels for the ordinary main entrance.
///
/// The two coordinate tables are the live arrays used by
/// `DrawPrimaryOrMidwayEntranceLabel` at `00452920`. The entrance pose changes the horizontal
/// label clearance from 18 to 24 pixels; the marker itself is rendered immediately to its left.
pub(crate) fn horizontal_primary_entrance_label_pixels(
    entrance: VanillaMainEntrance,
) -> (u16, u16) {
    horizontal_entrance_label_pixels(entrance, u16::from(entrance.level_mode_and_screen & 0x1f))
}

fn horizontal_entrance_label_pixels(entrance: VanillaMainEntrance, screen: u16) -> (u16, u16) {
    let screen = screen * 0x100;
    let x_setting = usize::from(entrance.vertical_settings & 7);
    let y_setting = usize::from(entrance.position & 0x0f);
    let x = screen + u16::from(VANILLA_ENTRANCE_X_LOW[x_setting]);
    let y = u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
        + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]);
    let pose = entrance.vertical_settings >> 3 & 7;
    let label_clearance = if pose < 3 || pose == 5 { 18 } else { 24 };
    (x.saturating_add(label_clearance), y)
}

pub(crate) fn horizontal_primary_entrance_marker_pixels(
    entrance: VanillaMainEntrance,
) -> (u16, u16) {
    horizontal_entrance_marker_pixels(entrance, u16::from(entrance.level_mode_and_screen & 0x1f))
}

fn horizontal_entrance_marker_pixels(entrance: VanillaMainEntrance, screen: u16) -> (u16, u16) {
    let screen = screen * 0x100;
    let x_setting = usize::from(entrance.vertical_settings & 7);
    let y_setting = usize::from(entrance.position & 0x0f);
    (
        screen + u16::from(VANILLA_ENTRANCE_X_LOW[x_setting]),
        u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
            + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]),
    )
}

pub(crate) fn vertical_primary_entrance_marker_pixels(
    entrance: VanillaMainEntrance,
    alternate_layout: bool,
) -> (u16, u16) {
    let y_setting = usize::from(entrance.position & 0x0f);
    let screen = if entrance.level_mode_and_screen & 0x20 != 0 {
        u16::from(entrance.level_mode_and_screen & 0x1f)
    } else {
        // With vertical entrance positioning disabled, Lunar Magic retains the legacy entrance
        // table's page and emits the "Turn on vertical entrance positioning" warning.
        u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting])
    };
    vertical_entrance_marker_pixels(entrance, screen, alternate_layout)
}

/// Returns Lunar Magic's marker and label anchors for a secondary entrance targeting the
/// current level. This follows `DrawSecondaryEntranceLabels` at `$00452D10`, including its
/// ordinary horizontal/vertical tables and the expanded-coordinate flag-$40 path.
pub(crate) fn secondary_entrance_marker_and_label_pixels(
    exit: lm_level::SecondaryExit,
    vertical: bool,
    alternate_vertical_layout: bool,
) -> ((u16, u16), (u16, u16)) {
    let position = usize::from(exit.position_and_method & 0x0f);
    let vertical_position = usize::from(exit.y & 7);
    let (marker_x, marker_y) = if exit.destination_flags & 0x40 == 0 {
        if vertical {
            let x_high = if alternate_vertical_layout {
                VANILLA_ALTERNATE_VERTICAL_ENTRANCE_X_HIGH[vertical_position]
            } else {
                VANILLA_VERTICAL_ENTRANCE_X_HIGH[vertical_position]
            };
            (
                u16::from(x_high) * 0x100 + u16::from(VANILLA_ENTRANCE_X_LOW[vertical_position]),
                u16::from(exit.screen) * 0x100 + u16::from(VANILLA_ENTRANCE_Y_LOW[position]),
            )
        } else {
            (
                u16::from(exit.screen) * 0x100
                    + u16::from(VANILLA_ENTRANCE_X_LOW[vertical_position]),
                u16::from(VANILLA_ENTRANCE_Y_HIGH[position]) * 0x100
                    + u16::from(VANILLA_ENTRANCE_Y_LOW[position]),
            )
        }
    } else {
        let packed_x = exit.x_and_overworld_flags | exit.x;
        let mut x =
            (u16::from(exit.position_and_method & 0x0f) + u16::from(packed_x & 0x3f) * 0x10) * 0x10;
        let mut y = (u16::from(exit.destination_flags >> 1 & 0x18) + u16::from(exit.y)) * 0x10;
        if vertical {
            x = u16::from(exit.screen) * 0x100 + (x & 0xf0);
        } else {
            y = u16::from(exit.screen) * 0x100 + (y & 0xf0);
        }
        (y, x)
    };
    let pose = exit.destination_flags & 7;
    let mut clearance = if pose < 3 || pose == 5 { 18 } else { 24 };
    if exit.additional_flags & 0x40 != 0 && pose == 6 {
        clearance += 10;
    }
    (
        (marker_x, marker_y),
        (marker_x.saturating_add(clearance), marker_y),
    )
}

fn referenced_secondary_exit_slots(project: &Project) -> Result<Vec<bool>, String> {
    let mut referenced = vec![false; SecondaryExitTable::ENTRY_COUNT];
    for level_number in 0..0x200 {
        let level = project
            .load_level_slot(
                level_number,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .map_err(|error| error.to_string())?;
        for record in &level.layer1.objects.records {
            let Some(exit) = record.screen_exit() else {
                continue;
            };
            let index = usize::from(
                exit.destination_and_flags & 0x00ff | (exit.destination_and_flags & 0x0200) >> 1,
            );
            referenced[index] = true;
        }
    }
    Ok(referenced)
}

fn secondary_entrance_destination(index: usize, exit: lm_level::SecondaryExit) -> u16 {
    if index < 0x200 {
        exit.destination_level & 0x00ff | u16::try_from(index & 0x100).unwrap_or(0)
    } else {
        exit.destination_level
    }
}

fn secondary_entrance_is_empty(exit: lm_level::SecondaryExit) -> bool {
    exit.position_and_method == 0
        && exit.screen == 0
        && exit.x == 0
        && exit.y == 0
        && exit.destination_flags == 0
        && exit.x_and_overworld_flags == 0
        && exit.additional_flags == 0
}

fn vertical_entrance_marker_pixels(
    entrance: VanillaMainEntrance,
    screen: u16,
    alternate_layout: bool,
) -> (u16, u16) {
    let x_setting = usize::from(entrance.vertical_settings & 7);
    let y_setting = usize::from(entrance.position & 0x0f);
    let x_high = if alternate_layout {
        VANILLA_ALTERNATE_VERTICAL_ENTRANCE_X_HIGH[x_setting]
    } else {
        VANILLA_VERTICAL_ENTRANCE_X_HIGH[x_setting]
    };
    (
        u16::from(x_high) * 0x100 + u16::from(VANILLA_ENTRANCE_X_LOW[x_setting]),
        screen * 0x100 + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]),
    )
}

/// Returns the vanilla midway entrance marker. In an untouched SMW ROM, midway entrances share
/// the main entrance's X/Y and pose settings; the high nibble of `$05:D7A1` selects their screen.
/// This is the `DAT_00600246 >> 4` path in Lunar Magic 3.63's
/// `DrawPrimaryOrMidwayEntranceLabel` at `00452920`.
pub(crate) fn midway_entrance_marker_pixels(
    entrance: VanillaMainEntrance,
    vertical: bool,
    alternate_vertical_layout: bool,
) -> (u16, u16) {
    let screen = u16::from(entrance.screen_and_method >> 4);
    if vertical {
        vertical_entrance_marker_pixels(entrance, screen, alternate_vertical_layout)
    } else {
        horizontal_entrance_marker_pixels(entrance, screen)
    }
}

pub(crate) fn midway_entrance_label_pixels(
    entrance: VanillaMainEntrance,
    vertical: bool,
    alternate_vertical_layout: bool,
) -> (u16, u16) {
    let screen = u16::from(entrance.screen_and_method >> 4);
    let (x, y) = if vertical {
        vertical_entrance_marker_pixels(entrance, screen, alternate_vertical_layout)
    } else {
        horizontal_entrance_marker_pixels(entrance, screen)
    };
    let pose = entrance.vertical_settings >> 3 & 7;
    let label_clearance = if pose < 3 || pose == 5 { 18 } else { 24 };
    (x.saturating_add(label_clearance), y)
}

pub(crate) fn vertical_primary_entrance_label_pixels(
    entrance: VanillaMainEntrance,
    alternate_layout: bool,
) -> (u16, u16) {
    let (x, y) = vertical_primary_entrance_marker_pixels(entrance, alternate_layout);
    let pose = entrance.vertical_settings >> 3 & 7;
    let label_clearance = if pose < 3 || pose == 5 { 18 } else { 24 };
    (x.saturating_add(label_clearance), y)
}

fn draw_primary_entrance_marker(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    entrance: VanillaMainEntrance,
    vertical: bool,
    alternate_vertical_layout: bool,
) {
    let (x, y) = if vertical {
        vertical_primary_entrance_marker_pixels(entrance, alternate_vertical_layout)
    } else {
        horizontal_primary_entrance_marker_pixels(entrance)
    };
    let scale = cell_size / 16.0;
    let target = egui::Rect::from_min_size(
        canvas.min + egui::vec2(f32::from(x) * scale, f32::from(y.saturating_add(2)) * scale),
        egui::vec2(16.0 * scale, 32.0 * scale),
    );
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

fn draw_primary_entrance_label(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    level: u16,
    entrance: VanillaMainEntrance,
    vertical: bool,
    alternate_vertical_layout: bool,
    shared_with_midway: bool,
) {
    let (x, y) = if vertical {
        vertical_primary_entrance_label_pixels(entrance, alternate_vertical_layout)
    } else {
        horizontal_primary_entrance_label_pixels(entrance)
    };
    let scale = cell_size / 16.0;
    let position = canvas.min + egui::vec2(f32::from(x) * scale, f32::from(y) * scale);
    let font_size = (cell_size * 0.625).max(7.0);
    let galley = painter.layout_no_wrap(
        format!(
            "{}Entrance to level {level:X}",
            if shared_with_midway { ">" } else { "" }
        ),
        egui::FontId::monospace(font_size),
        egui::Color32::WHITE,
    );
    let background =
        egui::Rect::from_min_size(position, galley.size()).expand2(egui::vec2(1.0, 0.0));
    painter.rect_filled(
        background,
        0.0,
        egui::Color32::from_rgba_unmultiplied(0, 116, 44, 220),
    );
    painter.galley(position, galley, egui::Color32::WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_secondary_entrances(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    level: u16,
    exits: Option<&SecondaryExitTable>,
    referenced: Option<&[bool]>,
    texture: Option<&egui::TextureHandle>,
    vertical: bool,
    alternate_vertical_layout: bool,
) {
    let (Some(exits), Some(referenced)) = (exits, referenced) else {
        return;
    };
    for (index, exit) in visible_secondary_entrances(level, exits, referenced) {
        let (marker, label) =
            secondary_entrance_marker_and_label_pixels(exit, vertical, alternate_vertical_layout);
        if let Some(texture) = texture {
            draw_entrance_marker_at(painter, canvas, cell_size, texture, marker);
        }
        draw_entrance_label_at(
            painter,
            canvas,
            cell_size,
            label,
            format!("Secondary Entrance #{index:03X}"),
        );
    }
}

fn visible_secondary_entrances(
    level: u16,
    exits: &SecondaryExitTable,
    referenced: &[bool],
) -> Vec<(usize, lm_level::SecondaryExit)> {
    exits
        .entries
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, exit)| {
            secondary_entrance_destination(*index, *exit) == level
                && exit.x_and_overworld_flags & 0x80 == 0
                && referenced.get(*index).copied().unwrap_or(false)
                && (!matches!(level, 0x000 | 0x100) || !secondary_entrance_is_empty(*exit))
        })
        .collect()
}

fn draw_midway_entrance(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    texture: Option<&egui::TextureHandle>,
    entrance: VanillaMainEntrance,
    vertical: bool,
    alternate_vertical_layout: bool,
) {
    let marker = midway_entrance_marker_pixels(entrance, vertical, alternate_vertical_layout);
    if let Some(texture) = texture {
        draw_entrance_marker_at(painter, canvas, cell_size, texture, marker);
    }
    draw_entrance_label_at(
        painter,
        canvas,
        cell_size,
        midway_entrance_label_pixels(entrance, vertical, alternate_vertical_layout),
        "Midway Entrance".into(),
    );
}

fn draw_entrance_marker_at(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    (x, y): (u16, u16),
) {
    let scale = cell_size / 16.0;
    let target = egui::Rect::from_min_size(
        canvas.min + egui::vec2(f32::from(x) * scale, f32::from(y.saturating_add(2)) * scale),
        egui::vec2(16.0 * scale, 32.0 * scale),
    );
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

fn draw_entrance_label_at(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    (x, y): (u16, u16),
    text: String,
) {
    let scale = cell_size / 16.0;
    let position = canvas.min + egui::vec2(f32::from(x) * scale, f32::from(y) * scale);
    let font_size = (cell_size * 0.625).max(7.0);
    let galley = painter.layout_no_wrap(
        text,
        egui::FontId::monospace(font_size),
        egui::Color32::WHITE,
    );
    let background =
        egui::Rect::from_min_size(position, galley.size()).expand2(egui::vec2(1.0, 0.0));
    painter.rect_filled(
        background,
        0.0,
        egui::Color32::from_rgba_unmultiplied(0, 116, 44, 220),
    );
    painter.galley(position, galley, egui::Color32::WHITE);
}

fn draw_primary_entrance_position_warning(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell_size: f32,
    entrance: VanillaMainEntrance,
    vertical: bool,
) {
    // RenderConfiguredLevelEntrance at 004cc660 emits this warning when a vertical level still
    // has the entrance-positioning bit disabled. Its first label starts one tile right and
    // twenty pixels below the entrance's world anchor.
    if !vertical || entrance.level_mode_and_screen & 0x20 != 0 {
        return;
    }
    let y_setting = usize::from(entrance.position & 0x0f);
    let marker_y = u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
        + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]);
    let scale = cell_size / 16.0;
    let position =
        canvas.min + egui::vec2(32.0 * scale, f32::from(marker_y.saturating_add(20)) * scale);
    let font_size = (cell_size * 0.625).max(7.0);
    let line_height = font_size + 1.0;
    for (line, offset) in [
        ("Warning:Turn on vertical", 0.0),
        ("entrance positioning!!", line_height),
    ] {
        let line_position = position + egui::vec2(0.0, offset);
        let galley = painter.layout_no_wrap(
            line.to_owned(),
            egui::FontId::monospace(font_size),
            egui::Color32::WHITE,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(line_position, galley.size()),
            0.0,
            egui::Color32::from_rgb(0, 0, 128),
        );
        painter.galley(line_position, galley, egui::Color32::WHITE);
    }
}

const fn presented_sprite_minor(placement: lm_level::NativeSpritePlacement) -> u16 {
    placement.minor
}

const fn presented_sprite_tile_coordinates(
    placement: lm_level::NativeSpritePlacement,
    vertical: bool,
) -> (u16, u16) {
    if vertical {
        (presented_sprite_minor(placement), placement.major)
    } else {
        (placement.major, presented_sprite_minor(placement))
    }
}

fn object_placement_at_canvas_position(
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
) -> Option<(u16, ObjectCoordinateNibbles, bool)> {
    if !canvas.contains(position) || !cell.is_finite() || cell <= 0.0 {
        return None;
    }
    let column = ((position.x - canvas.left()) / cell).floor();
    let row = ((position.y - canvas.top()) / cell).floor();
    if !(0.0..=f32::from(u16::MAX)).contains(&column) || !(0.0..=f32::from(u16::MAX)).contains(&row)
    {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (major, minor) = if vertical {
        (row as u16, column as u16)
    } else {
        (column as u16, row as u16)
    };
    let screen = major / 16;
    if screen >= 32 || minor >= level_minor_tile_limit(vertical) {
        return None;
    }
    let (first, second) = if vertical {
        (
            u8::try_from(major % 16).ok()?,
            u8::try_from(minor % 16).ok()?,
        )
    } else {
        (
            u8::try_from(minor % 16).ok()?,
            u8::try_from(major % 16).ok()?,
        )
    };
    Some((
        screen,
        ObjectCoordinateNibbles { first, second },
        minor >= 16,
    ))
}

fn object_native_position_at_canvas(
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
) -> Option<(i32, i32)> {
    if !canvas.contains(position) || !cell.is_finite() || cell <= 0.0 {
        return None;
    }
    let column = ((position.x - canvas.left()) / cell).floor();
    let row = ((position.y - canvas.top()) / cell).floor();
    let (major, minor) = if vertical {
        (row, column)
    } else {
        (column, row)
    };
    if !(0.0..512.0).contains(&major)
        || !(0.0..f32::from(level_minor_tile_limit(vertical))).contains(&minor)
    {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some((major as i32, minor as i32))
}

fn object_group_anchor(stream: &lm_level::ObjectStream, selected: &[usize]) -> Option<(i32, i32)> {
    stream
        .native_placements()
        .into_iter()
        .filter(|placement| selected.contains(&placement.record_index))
        .min_by_key(|placement| u16::from(placement.minor).saturating_add(placement.major))
        .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
}

fn sprite_group_anchor(
    stream: &lm_level::NativeSpriteStream,
    selected: &[usize],
) -> Option<(i32, i32)> {
    stream
        .native_placements()
        .into_iter()
        .filter(|placement| selected.contains(&placement.token_index))
        .min_by_key(|placement| placement.major.saturating_add(placement.minor))
        .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
}

fn nearest_valid_group_delta(
    positions: &[(i32, i32)],
    requested_major: i32,
    requested_minor: i32,
    major_limit: i32,
    minor_limit: i32,
) -> Option<(i32, i32)> {
    let valid = |(major, minor): (i32, i32), major_delta: i32, minor_delta: i32| {
        (0..major_limit).contains(&(major + major_delta))
            && (0..minor_limit).contains(&(minor + minor_delta))
    };
    if positions
        .iter()
        .copied()
        .all(|position| valid(position, requested_major, requested_minor))
    {
        return Some((requested_major, requested_minor));
    }

    let mut major_delta = requested_major;
    let mut minor_delta = requested_minor;
    loop {
        let offending = positions
            .iter()
            .copied()
            .find(|position| !valid(*position, major_delta, minor_delta))?;
        let major_step = if major_delta < 0 { 1 } else { -1 };
        let minor_step = if minor_delta < 0 { 1 } else { -1 };
        let mut candidate_minor = minor_delta;
        let mut corrected = None;
        while candidate_minor != minor_step {
            let mut candidate_major = major_delta;
            while candidate_major != major_step {
                if (candidate_major != 0 || candidate_minor != 0)
                    && valid(offending, candidate_major, candidate_minor)
                {
                    corrected = Some((candidate_major, candidate_minor));
                    break;
                }
                candidate_major += major_step;
            }
            if corrected.is_some() {
                break;
            }
            candidate_minor += minor_step;
        }
        let corrected = corrected?;
        major_delta = corrected.0;
        minor_delta = corrected.1;
        if positions
            .iter()
            .copied()
            .all(|position| valid(position, major_delta, minor_delta))
        {
            return Some((major_delta, minor_delta));
        }
    }
}

fn layer2_tile_at_canvas_position(
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
) -> Option<usize> {
    if !canvas.contains(position) || !cell.is_finite() || cell <= 0.0 {
        return None;
    }
    let x = ((position.x - canvas.left()) / cell).floor();
    let y = ((position.y - canvas.top()) / cell).floor();
    if !(0.0..32.0).contains(&x) || !(0.0..32.0).contains(&y) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    lm_level::native_layer2_tilemap_index(x as usize, y as usize)
}

fn layer2_canvas_coordinates(index: usize) -> Option<(usize, usize)> {
    if index >= 32 * 32 {
        return None;
    }
    let plane = index / (16 * 32);
    let within_plane = index % (16 * 32);
    Some((plane * 16 + within_plane % 16, within_plane / 16))
}

fn draw_map16_atlas_tile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    tile: u16,
) {
    draw_map16_atlas_tile_tinted(painter, texture, target, tile, egui::Color32::WHITE);
}

fn draw_map16_atlas_tile_for_tileset(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    tile: u16,
    object_tileset: u8,
) {
    // RenderMap16TileToPixelBuffer @ $0044EAF0 applies the half-alpha path only
    // to tiles $027-$02A in the underground object family. The same Map16 IDs
    // are ordinary opaque artwork in other families.
    draw_map16_atlas_tile_tinted(
        painter,
        texture,
        target,
        tile,
        vanilla_map16_atlas_tint(object_tileset, tile),
    );
}

const fn vanilla_map16_atlas_tint(object_tileset: u8, tile: u16) -> egui::Color32 {
    if object_tileset == 4 && tile >= 0x027 && tile <= 0x02a {
        egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
    } else {
        egui::Color32::WHITE
    }
}

fn draw_map16_atlas_tile_tinted(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    tile: u16,
    tint: egui::Color32,
) {
    painter.image(
        texture.id(),
        target,
        map16_atlas_uv(tile, false, false),
        tint,
    );
}

fn draw_map16_atlas_word(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    word: u16,
    tint: egui::Color32,
) {
    painter.image(
        texture.id(),
        target,
        map16_atlas_uv(word & 0x3fff, word & 0x4000 != 0, word & 0x8000 != 0),
        tint,
    );
}

fn map16_atlas_uv(tile: u16, x_flip: bool, y_flip: bool) -> egui::Rect {
    let column = f32::from(tile % 32);
    let row = f32::from(tile / 32);
    let (left, right) = if x_flip {
        ((column + 1.0) / 32.0, column / 32.0)
    } else {
        (column / 32.0, (column + 1.0) / 32.0)
    };
    let (top, bottom) = if y_flip {
        ((row + 1.0) / 16.0, row / 16.0)
    } else {
        (row / 16.0, (row + 1.0) / 16.0)
    };
    egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom))
}

fn canvas_background_color(backdrop: Option<lm_graphics::Bgr555>) -> egui::Color32 {
    backdrop.map_or_else(
        || egui::Color32::from_gray(20),
        |color| {
            let color = color.to_rgb8();
            egui::Color32::from_rgb(color.red, color.green, color.blue)
        },
    )
}

#[derive(Default)]
struct CanvasModel {
    layer1_records: Vec<ObjectRecord>,
    layer1_placements: Vec<lm_level::NativeObjectPlacement>,
    layer2_records: Vec<ObjectRecord>,
    layer2_placements: Vec<lm_level::NativeObjectPlacement>,
    layer2_tilemap: Vec<u16>,
    sprite_placements: Vec<lm_level::NativeSpritePlacement>,
}

#[derive(Clone, Copy)]
struct Map16Summary {
    foreground_files: [usize; 4],
    background_files: [usize; 4],
    sprite_files: [usize; 4],
    common_tiles: usize,
    tileset_tiles: usize,
}

#[allow(clippy::too_many_arguments)]
fn draw_layer2_tilemap(
    painter: &egui::Painter,
    target: egui::Rect,
    cell_size: f32,
    tilemap: &[u16],
    map16_texture: Option<&egui::TextureHandle>,
    map16_texture_variants: Option<&[egui::TextureHandle]>,
    background_map16_texture: Option<&egui::TextureHandle>,
    background_plane_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    object_tileset: u8,
    entrance: VanillaMainEntrance,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    game_camera: Option<(u16, u16)>,
    background_512_height: bool,
    outline_texture: Option<&egui::TextureHandle>,
    surface_outline: bool,
    line_guide_outline: bool,
) {
    if let (Some(texture), Some(camera)) = (background_plane_texture, game_camera) {
        draw_wrapped_background_viewport(
            painter,
            target,
            cell_size,
            texture,
            entrance,
            camera,
            background_512_height,
        );
        return;
    }
    let (columns, rows) = if vertical {
        (usize::from(minor_tiles), usize::from(major_tiles))
    } else {
        (usize::from(major_tiles), usize::from(minor_tiles))
    };
    for y in 0..rows {
        for x in 0..columns {
            let shared_background = background_map16_texture.is_some();
            let (background_x, background_y) = if shared_background {
                game_camera.map_or_else(
                    || vanilla_shared_background_coordinates(x, y, entrance),
                    |camera| {
                        vanilla_game_background_coordinates(
                            x,
                            y,
                            entrance,
                            camera,
                            background_512_height,
                        )
                    },
                )
            } else {
                (x, y)
            };
            let Some(&word) =
                presented_layer2_tilemap_index(background_x, background_y, shared_background)
                    .and_then(|index| tilemap.get(index))
            else {
                continue;
            };
            let tile = word & 0x3fff;
            let x_offset = native_canvas_tile_offset(x, cell_size);
            let y_offset = native_canvas_tile_offset(y, cell_size);
            let cell = egui::Rect::from_min_size(
                target.min + egui::vec2(x_offset, y_offset),
                egui::vec2(cell_size, cell_size),
            );
            let definition = match custom_map16 {
                Some(lm_app::NativeMap16SidecarDocument::M16(sidecar)) => {
                    sidecar.tile(usize::from(tile))
                }
                Some(lm_app::NativeMap16SidecarDocument::S16(_)) | None => None,
            };
            if let (Some(definition), Some(texture)) = (definition, foreground_texture) {
                draw_custom_map16_tile_with_outer_flips(
                    painter,
                    texture,
                    cell,
                    definition,
                    word & 0x4000 != 0,
                    word & 0x8000 != 0,
                );
            } else if background_map16_texture.is_some()
                && tile < 0x200
                && let Some(texture) = background_map16_texture
            {
                draw_map16_atlas_word(painter, texture, cell, word, egui::Color32::WHITE);
            } else if tile < 0x200
                && let Some(texture) = map16_texture_variants
                    .and_then(|textures| textures.get(map16_screen_variant(x, y, vertical)))
                    .or(map16_texture)
            {
                draw_map16_atlas_word(
                    painter,
                    texture,
                    cell,
                    word,
                    vanilla_map16_atlas_tint(object_tileset, tile),
                );
            }
            draw_map16_outline_tile(
                painter,
                outline_texture,
                cell,
                tile,
                object_tileset,
                custom_map16,
                surface_outline,
                line_guide_outline,
            );
        }
    }
}

fn native_canvas_tile_offset(tile: usize, cell_size: f32) -> f32 {
    f32::from(u16::try_from(tile).expect("native level canvas coordinate fits u16")) * cell_size
}

fn draw_wrapped_background_viewport(
    painter: &egui::Painter,
    world: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    entrance: VanillaMainEntrance,
    camera: (u16, u16),
    background_512_height: bool,
) {
    const PLANE_WIDTH_PIXELS: i32 = 512;
    const VIEW_WIDTH: i32 = 256;
    const VIEW_HEIGHT: i32 = 224;
    let layer1_camera = (i32::from(camera.0) * 16, i32::from(camera.1) * 16);
    let (source_x, source_y) = vanilla_layer2_camera_pixels(entrance, layer1_camera);
    let plane_height_pixels = background_plane_height_pixels(background_512_height);
    let viewport = egui::Rect::from_min_size(
        world.min
            + egui::vec2(
                f32::from(camera.0) * cell_size,
                f32::from(camera.1) * cell_size,
            ),
        egui::vec2(
            f32::from(u8::try_from(VIEW_WIDTH / 16).unwrap()) * cell_size,
            f32::from(u8::try_from(VIEW_HEIGHT / 16).unwrap()) * cell_size,
        ),
    );
    let mut output_y = 0;
    while output_y < VIEW_HEIGHT {
        let plane_y = (source_y + output_y).rem_euclid(plane_height_pixels);
        let rows = (plane_height_pixels - plane_y).min(VIEW_HEIGHT - output_y);
        let mut output_x = 0;
        while output_x < VIEW_WIDTH {
            let plane_x = (source_x + output_x).rem_euclid(PLANE_WIDTH_PIXELS);
            let columns = (PLANE_WIDTH_PIXELS - plane_x).min(VIEW_WIDTH - output_x);
            let target = egui::Rect::from_min_size(
                viewport.min
                    + egui::vec2(
                        screen_pixels_f32(output_x) * cell_size / 16.0,
                        screen_pixels_f32(output_y) * cell_size / 16.0,
                    ),
                egui::vec2(
                    screen_pixels_f32(columns) * cell_size / 16.0,
                    screen_pixels_f32(rows) * cell_size / 16.0,
                ),
            );
            let uv = egui::Rect::from_min_max(
                egui::pos2(
                    screen_pixels_f32(plane_x) / 512.0,
                    screen_pixels_f32(plane_y) / 512.0,
                ),
                egui::pos2(
                    screen_pixels_f32(plane_x + columns) / 512.0,
                    screen_pixels_f32(plane_y + rows) / 512.0,
                ),
            );
            painter.image(texture.id(), target, uv, egui::Color32::WHITE);
            output_x += columns;
        }
        output_y += rows;
    }
}

const fn background_plane_height_pixels(background_512_height: bool) -> i32 {
    if background_512_height { 512 } else { 432 }
}

const fn overlay_opacity(translucent: bool) -> f32 {
    if translucent { 0.5 } else { 1.0 }
}

#[allow(clippy::too_many_arguments)]
fn draw_layer3_editor_or_viewport(
    painter: &egui::Painter,
    world: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    position: (i16, i16),
    camera: (u16, u16),
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    single_viewport: bool,
) {
    if single_viewport {
        draw_wrapped_layer3_region(
            painter,
            world,
            cell_size,
            texture,
            position,
            camera,
            (256, 224),
        );
        return;
    }
    let (width, height) = if vertical {
        (minor_tiles, major_tiles)
    } else {
        (major_tiles, minor_tiles)
    };
    let pixel_scale = cell_size / 16.0;
    let world_width = i32::from(width) * 16;
    let world_height = i32::from(height) * 16;
    // RenderLayer3TilemapCellAtCoordinates @ $004502C0 masks coordinates into Lunar Magic's
    // active BG3 plane while traversing vertical editor worlds. Horizontal editors retain the
    // native single Y origin; normalizing a negative position backward by 512 pixels leaks the
    // plane's tail into the top of levels such as $127.
    let target_y_origins = layer3_plane_y_origins(position.1, world_height, vertical);
    for target_y in target_y_origins {
        for target_x in repeating_layer3_plane_origins(position.0, world_width) {
            let target = egui::Rect::from_min_size(
                world.min
                    + egui::vec2(
                        screen_pixels_f32(target_x) * pixel_scale,
                        screen_pixels_f32(target_y) * pixel_scale,
                    ),
                egui::vec2(512.0 * pixel_scale, 512.0 * pixel_scale),
            );
            painter.image(
                texture.id(),
                target,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
}

fn repeating_layer3_plane_origins(position: i16, world_extent: i32) -> Vec<i32> {
    const PLANE_PIXELS: i32 = 512;
    let mut origin = -i32::from(position);
    while origin > 0 {
        origin -= PLANE_PIXELS;
    }
    while origin + PLANE_PIXELS <= 0 {
        origin += PLANE_PIXELS;
    }
    let mut origins = Vec::new();
    while origin < world_extent {
        origins.push(origin);
        origin += PLANE_PIXELS;
    }
    origins
}

fn layer3_plane_y_origins(position: i16, world_extent: i32, vertical: bool) -> Vec<i32> {
    if vertical {
        repeating_layer3_plane_origins(position, world_extent)
    } else {
        vec![-i32::from(position)]
    }
}

fn draw_wrapped_layer3_region(
    painter: &egui::Painter,
    world: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    position: (i16, i16),
    camera: (u16, u16),
    view_pixels: (i32, i32),
) {
    const PLANE_PIXELS: i32 = 512;
    let viewport = egui::Rect::from_min_size(
        world.min
            + egui::vec2(
                f32::from(camera.0) * cell_size,
                f32::from(camera.1) * cell_size,
            ),
        egui::vec2(
            screen_pixels_f32(view_pixels.0) * cell_size / 16.0,
            screen_pixels_f32(view_pixels.1) * cell_size / 16.0,
        ),
    );
    let pixel_scale = cell_size / 16.0;
    let source_x = i32::from(position.0).rem_euclid(PLANE_PIXELS);
    let source_y = i32::from(position.1).rem_euclid(PLANE_PIXELS);
    let mut output_y = 0;
    while output_y < view_pixels.1 {
        let plane_y = (source_y + output_y).rem_euclid(PLANE_PIXELS);
        let rows = (PLANE_PIXELS - plane_y).min(view_pixels.1 - output_y);
        let mut output_x = 0;
        while output_x < view_pixels.0 {
            let plane_x = (source_x + output_x).rem_euclid(PLANE_PIXELS);
            let columns = (PLANE_PIXELS - plane_x).min(view_pixels.0 - output_x);
            let pixels = |value| f32::from(u16::try_from(value).unwrap_or_default());
            let plane_extent = pixels(PLANE_PIXELS);
            let target = egui::Rect::from_min_size(
                viewport.min
                    + egui::vec2(
                        pixels(output_x) * pixel_scale,
                        pixels(output_y) * pixel_scale,
                    ),
                egui::vec2(pixels(columns) * pixel_scale, pixels(rows) * pixel_scale),
            );
            let uv = egui::Rect::from_min_max(
                egui::pos2(
                    pixels(plane_x) / plane_extent,
                    pixels(plane_y) / plane_extent,
                ),
                egui::pos2(
                    pixels(plane_x + columns) / plane_extent,
                    pixels(plane_y + rows) / plane_extent,
                ),
            );
            painter.image(texture.id(), target, uv, egui::Color32::WHITE);
            output_x += columns;
        }
        output_y += rows;
    }
}

fn vanilla_game_background_coordinates(
    layer1_x: usize,
    layer1_y: usize,
    entrance: VanillaMainEntrance,
    camera: (u16, u16),
    background_512_height: bool,
) -> (usize, usize) {
    let setting = usize::from(entrance.position >> 4);
    let camera_x = usize::from(camera.0);
    let camera_y = usize::from(camera.1);
    let initial_layer1_y =
        usize::from(VANILLA_INITIAL_LAYER1_Y[usize::from((entrance.screen_and_method >> 2) & 3)])
            / 16;
    let initial_layer2_y =
        usize::from(VANILLA_INITIAL_LAYER2_Y[usize::from(entrance.screen_and_method & 3)]) / 16;
    let layer2_camera_x = scale_layer2_camera(camera_x, VANILLA_LAYER2_HORIZONTAL_SCROLL[setting]);
    let vertical_scroll = VANILLA_LAYER2_VERTICAL_SCROLL[setting];
    let layer2_camera_y = i64::try_from(initial_layer2_y).unwrap_or_default()
        + i64::try_from(scale_layer2_camera(camera_y, vertical_scroll)).unwrap_or_default()
        - i64::try_from(scale_layer2_camera(initial_layer1_y, vertical_scroll)).unwrap_or_default();

    let source_x = i64::try_from(layer2_camera_x).unwrap_or_default()
        + i64::try_from(layer1_x).unwrap_or_default()
        - i64::try_from(camera_x).unwrap_or_default();
    let source_y = layer2_camera_y + i64::try_from(layer1_y).unwrap_or_default()
        - i64::try_from(camera_y).unwrap_or_default();
    (
        usize::try_from(source_x.rem_euclid(32)).unwrap_or_default(),
        usize::try_from(source_y.rem_euclid(if background_512_height { 32 } else { 27 }))
            .unwrap_or_default(),
    )
}

fn vanilla_layer2_camera_pixels(
    entrance: VanillaMainEntrance,
    layer1_camera: (i32, i32),
) -> (i32, i32) {
    let setting = usize::from(entrance.position >> 4);
    let horizontal = VANILLA_LAYER2_HORIZONTAL_SCROLL[setting];
    let layer2_x = match horizontal {
        0 => 0,
        1 => layer1_camera.0,
        _ => layer1_camera.0 / 2,
    };
    let vertical = VANILLA_LAYER2_VERTICAL_SCROLL[setting];
    let initial_layer1 =
        i32::from(VANILLA_INITIAL_LAYER1_Y[usize::from((entrance.screen_and_method >> 2) & 3)]);
    let initial_layer2 =
        i32::from(VANILLA_INITIAL_LAYER2_Y[usize::from(entrance.screen_and_method & 3)]);
    let layer2_y = match vertical {
        0 => initial_layer2,
        1 => initial_layer2 - initial_layer1 + layer1_camera.1,
        2 => initial_layer2 - initial_layer1 / 2 + layer1_camera.1 / 2,
        // GameMode11_LoadSublevel initializes setting 3 relative to L1/8,
        // while HandleStandardLevelCameraScroll subsequently applies L1/32.
        _ => initial_layer2 - initial_layer1 / 8 + layer1_camera.1 / 32,
    };
    (layer2_x, layer2_y)
}

fn screen_pixels_f32(value: i32) -> f32 {
    f32::from(
        i16::try_from(value)
            .expect("SMW layer planes and level-camera coordinates fit signed 16-bit pixels"),
    )
}

const fn scale_layer2_camera(position: usize, scroll_setting: u8) -> usize {
    match scroll_setting {
        0 => 0,
        1 => position,
        2 => position / 2,
        _ => position / 32,
    }
}

fn presented_layer2_tilemap_index(x: usize, y: usize, _shared_background: bool) -> Option<usize> {
    lm_level::native_layer2_tilemap_index(x % 32, y % 32)
}

fn vanilla_shared_background_coordinates(
    layer1_x: usize,
    layer1_y: usize,
    _entrance: VanillaMainEntrance,
) -> (usize, usize) {
    // Lunar Magic's active editor compositor indexes the materialized 32×32 background plane
    // directly. Entrance-relative scroll rates belong only to the separate game-camera preview.
    (layer1_x, layer1_y)
}

const fn native_object_cache_minor_tiles(canvas_minor_tiles: u16, vertical: bool) -> u16 {
    let native_minor_tiles = level_minor_tile_limit(vertical);
    if canvas_minor_tiles < native_minor_tiles {
        canvas_minor_tiles
    } else {
        native_minor_tiles
    }
}

const fn level_minor_tile_limit(vertical: bool) -> u16 {
    if vertical {
        VERTICAL_LEVEL_MINOR_TILES
    } else {
        NATIVE_LEVEL_MINOR_TILES
    }
}

#[derive(Clone, Copy)]
struct OrderedObjectDraw<'a> {
    texture: &'a egui::TextureHandle,
    texture_variants: Option<&'a [egui::TextureHandle]>,
    block_contents_texture: Option<&'a egui::TextureHandle>,
    target: egui::Rect,
    cell_size: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    records: &'a [ObjectRecord],
    placements: &'a [lm_level::NativeObjectPlacement],
    handler_map: Option<&'a [u8; 64]>,
    metadata: Option<&'a lm_level::OscResolvedTable>,
    variant: u8,
    object_tileset: u8,
    level_mode: u8,
    custom_map16: Option<&'a lm_app::NativeMap16SidecarDocument>,
    foreground_texture: Option<&'a egui::TextureHandle>,
    outline_texture: Option<&'a egui::TextureHandle>,
    surface_outline: bool,
    line_guide_outline: bool,
    switch_view_state: lm_render::LunarMagicSwitchViewState,
    conditional_view_state: lm_render::LunarMagicConditionalViewState,
    blue_pow_active: bool,
}

fn draw_canvas_caption(ui: &mut egui::Ui, vertical: bool) {
    ui.label(format!(
        "Screen-aware {} layout: recovered object and sprite artwork; red markers identify missing custom displays, while native empty handlers remain artwork-free; stronger lines mark screen boundaries.",
        if vertical { "vertical" } else { "horizontal" }
    ));
}

#[allow(clippy::too_many_lines)]
fn draw_ordered_object_tiles(
    painter: &egui::Painter,
    request: OrderedObjectDraw<'_>,
) -> HashMap<usize, egui::Rect> {
    let record_limit = visual_smoke_editor_object_limit()
        .unwrap_or(request.records.len())
        .min(request.records.len());
    let mut artwork_bounds = HashMap::new();
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    if lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).is_err()
        || lm_render::install_lunar_magic_tileset_extended_objects(
            &mut definitions,
            request.object_tileset,
        )
        .is_err()
        || lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).is_err()
        || definitions
            .apply_lunar_magic_switch_view_state(request.switch_view_state)
            .is_err()
    {
        return artwork_bounds;
    }
    let layout = lm_render::NativeLevelMap16Layout {
        width: if request.vertical {
            usize::from(request.minor_tiles)
        } else {
            usize::from(request.major_tiles)
        },
        height: if request.vertical {
            usize::from(request.major_tiles)
        } else {
            usize::from(request.minor_tiles)
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical: request.vertical,
    };
    let has_custom_displays = request.placements.iter().any(|placement| {
        request
            .records
            .get(placement.record_index)
            .is_some_and(|record| {
                request.metadata.is_some_and(|metadata| {
                    resolved_custom_object_parts(record, metadata, request.variant).is_some()
                })
            })
    });
    let has_direct_map16 = request.records[..record_limit]
        .iter()
        .any(|record| record.direct_map16_fields().is_some());
    let shared_standard_cache = if has_custom_displays || has_direct_map16 {
        false
    } else {
        request.handler_map.is_some_and(|handler_map| {
            let stream = lm_level::ObjectStream {
                records: request.records[..record_limit].to_vec(),
            };
            lm_render::render_mapped_standard_object_stream(
                &stream,
                &definitions,
                handler_map,
                layout,
                VANILLA_EMPTY_MAP16_TILE,
            )
            .is_ok_and(|report| {
                draw_standard_object_cache(painter, request, layout, &report.cache);
                true
            })
        })
    };
    for placement in request.placements {
        if placement.record_index >= record_limit {
            continue;
        }
        let Some(record) = request.records.get(placement.record_index) else {
            continue;
        };
        if let Some(parts) = request
            .metadata
            .and_then(|metadata| resolved_custom_object_parts(record, metadata, request.variant))
        {
            draw_custom_object_parts(painter, request, *placement, &parts);
            let encoded = encoded_object_rect(
                request.target,
                *placement,
                request.vertical,
                request.cell_size,
            );
            let (tile_x, tile_y) = placement.tile_coordinates(request.vertical);
            let origin = request.target.min
                + egui::vec2(
                    f32::from(tile_x) * request.cell_size,
                    f32::from(tile_y) * request.cell_size,
                );
            artwork_bounds.insert(
                placement.record_index,
                custom_object_display_rect(encoded, origin, &parts, request.cell_size),
            );
            continue;
        }
        let Some(handler_map) = request.handler_map else {
            continue;
        };
        let Ok(Some(cache)) = lm_render::render_mapped_standard_object_placement_with_view_state(
            record,
            *placement,
            &definitions,
            handler_map,
            layout,
            VANILLA_EMPTY_MAP16_TILE,
            request.conditional_view_state,
        ) else {
            continue;
        };
        if !shared_standard_cache {
            draw_standard_object_cache(painter, request, layout, &cache);
        }
        artwork_bounds.insert(
            placement.record_index,
            standard_object_cache_display_rect(
                encoded_object_rect(
                    request.target,
                    *placement,
                    request.vertical,
                    request.cell_size,
                ),
                request.target,
                layout,
                &cache,
                request.cell_size,
            ),
        );
    }
    artwork_bounds
}

fn draw_standard_object_cache(
    painter: &egui::Painter,
    request: OrderedObjectDraw<'_>,
    layout: lm_render::NativeLevelMap16Layout,
    cache: &lm_render::NativeLevelMap16Cache,
) {
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            let Some(&tile) = cache.cells().get(index) else {
                continue;
            };
            if !cache.was_written(index) {
                continue;
            }
            let Ok(tile_x) = u16::try_from(x) else {
                continue;
            };
            let Ok(tile_y) = u16::try_from(y) else {
                continue;
            };
            let tile_rect = egui::Rect::from_min_size(
                request.target.min
                    + egui::vec2(
                        f32::from(tile_x) * request.cell_size,
                        f32::from(tile_y) * request.cell_size,
                    ),
                egui::vec2(request.cell_size, request.cell_size),
            );
            match map16_paint_source(tile, request.custom_map16) {
                Map16PaintSource::Base(tile) => {
                    draw_map16_atlas_tile_for_tileset(
                        painter,
                        map16_texture_for_cell(request, x, y),
                        tile_rect,
                        tile,
                        request.object_tileset,
                    );
                }
                Map16PaintSource::Custom(definition) => {
                    if let Some(texture) = request.foreground_texture {
                        draw_custom_map16_tile(painter, texture, tile_rect, definition);
                    } else {
                        draw_unresolved_map16_paint(painter, tile_rect, tile);
                    }
                }
                Map16PaintSource::Unresolved => {
                    draw_unresolved_map16_paint(painter, tile_rect, tile);
                }
            }
            if request.conditional_view_state.block_contents
                && let Some(texture) = request.block_contents_texture
            {
                let mapping = lm_level::lunar_magic_block_contents_mapping(
                    tile & 0x3fff,
                    index,
                    lm_level::DscDisplayContext {
                        first_feature_enabled: request.blue_pow_active,
                        first_feature_suppressed: false,
                        second_feature_enabled: request.conditional_view_state.on_off_switch_on,
                    },
                    request.level_mode,
                );
                if mapping != 0 {
                    draw_block_contents_overlay(painter, texture, tile_rect, mapping);
                }
            }
            draw_map16_outline(painter, request, tile_rect, tile);
        }
    }
}

fn draw_block_contents_overlay(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    mapping: u16,
) {
    let alpha = block_contents_overlay_alpha(mapping);
    let tint = egui::Color32::from_rgba_premultiplied(alpha - 1, alpha - 1, alpha - 1, alpha);
    let tile = mapping & 0x3fff;
    let column = f32::from(tile % 32);
    let row = f32::from(tile / 32);
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(
            egui::pos2(column / 32.0, row / 32.0),
            egui::pos2((column + 1.0) / 32.0, (row + 1.0) / 32.0),
        ),
        tint,
    );
}

const fn block_contents_overlay_alpha(mapping: u16) -> u8 {
    if mapping & 0x8000 != 0 { 128 } else { 192 }
}

#[allow(clippy::too_many_arguments)]
fn draw_block_exit_warnings(
    painter: &egui::Painter,
    target: egui::Rect,
    cell_size: f32,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    records: &[ObjectRecord],
    placements: &[lm_level::NativeObjectPlacement],
    handler_map: Option<&[u8; 64]>,
    view_state: lm_render::LunarMagicConditionalViewState,
    level_mode: u8,
    object_tileset: u8,
) {
    let Some(handler_map) = handler_map else {
        return;
    };
    let layout = lm_render::NativeLevelMap16Layout {
        width: if vertical {
            usize::from(minor_tiles)
        } else {
            usize::from(major_tiles)
        },
        height: if vertical {
            usize::from(major_tiles)
        } else {
            usize::from(minor_tiles)
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical,
    };
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    if lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).is_err()
        || lm_render::install_lunar_magic_tileset_extended_objects(&mut definitions, object_tileset)
            .is_err()
        || lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).is_err()
    {
        return;
    }
    let mut cache = lm_render::NativeLevelMap16Cache::filled(VANILLA_EMPTY_MAP16_TILE);
    for placement in placements {
        let Some(record) = records.get(placement.record_index) else {
            continue;
        };
        let Ok(Some(rendered)) = lm_render::render_mapped_standard_object_placement_with_view_state(
            record,
            *placement,
            &definitions,
            handler_map,
            layout,
            VANILLA_EMPTY_MAP16_TILE,
            view_state,
        ) else {
            continue;
        };
        cache.overlay_written_cells(&rendered);
    }
    for (x, y) in block_exit_warning_cells(&cache, layout, level_mode) {
        let rect = egui::Rect::from_min_size(
            target.min
                + egui::vec2(
                    f32::from(u16::try_from(x).expect("native level width fits u16")) * cell_size,
                    f32::from(u16::try_from(y).expect("native level height fits u16")) * cell_size,
                ),
            egui::vec2(cell_size, cell_size),
        );
        draw_block_exit_outline(painter, rect);
    }
}

fn block_exit_warning_cells(
    cache: &lm_render::NativeLevelMap16Cache,
    layout: lm_render::NativeLevelMap16Layout,
    level_mode: u8,
) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            if cache.was_written(index)
                && cache.cells().get(index).is_some_and(|tile| {
                    lm_level::lunar_magic_block_exit_marker(tile & 0x3fff, level_mode)
                })
            {
                cells.push((x, y));
            }
        }
    }
    cells
}

fn draw_block_exit_outline(painter: &egui::Painter, rect: egui::Rect) {
    // DrawEditorSelectionOutline @ $00450B30 receives outer color 0 and inner color $FF0000.
    // For a 16×16 warning cell it writes black lines at offsets 0, 3, 12, and 15 and red lines at
    // offsets 1, 2, 13, and 14 on both axes. Scale those logical pixels with the canvas cell.
    let pixel = rect.width().min(rect.height()) / 16.0;
    for (offset, color) in block_exit_outline_stripes() {
        let offset = f32::from(offset) * pixel;
        let vertical = egui::Rect::from_min_size(
            rect.min + egui::vec2(offset, 0.0),
            egui::vec2(pixel, rect.height()),
        );
        let horizontal = egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, offset),
            egui::vec2(rect.width(), pixel),
        );
        painter.rect_filled(vertical, 0.0, color);
        painter.rect_filled(horizontal, 0.0, color);
    }
}

const fn block_exit_outline_stripes() -> [(u8, egui::Color32); 8] {
    let black = egui::Color32::BLACK;
    let red = egui::Color32::from_rgb(255, 0, 0);
    [
        (0, black),
        (3, black),
        (12, black),
        (15, black),
        (1, red),
        (2, red),
        (13, red),
        (14, red),
    ]
}

fn draw_map16_outline(
    painter: &egui::Painter,
    request: OrderedObjectDraw<'_>,
    target: egui::Rect,
    tile: u16,
) {
    draw_map16_outline_tile(
        painter,
        request.outline_texture,
        target,
        tile,
        request.object_tileset,
        request.custom_map16,
        request.surface_outline,
        request.line_guide_outline,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_map16_outline_tile(
    painter: &egui::Painter,
    texture: Option<&egui::TextureHandle>,
    target: egui::Rect,
    tile: u16,
    object_tileset: u8,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    surface_outline: bool,
    line_guide_outline: bool,
) {
    let Some(texture) = texture else { return };
    let Some(glyph) = crate::level_outline::glyph_for_tile(
        tile,
        object_tileset,
        custom_map16,
        surface_outline,
        line_guide_outline,
    ) else {
        return;
    };
    const GLYPHS: f32 = 113.0;
    let left = f32::from(glyph) / GLYPHS;
    let right = (f32::from(glyph) + 1.0) / GLYPHS;
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::pos2(left, 0.0), egui::pos2(right, 1.0)),
        egui::Color32::WHITE,
    );
}

fn map16_texture_for_cell<'a>(
    request: OrderedObjectDraw<'a>,
    x: usize,
    y: usize,
) -> &'a egui::TextureHandle {
    request
        .texture_variants
        .and_then(|textures| textures.get(map16_screen_variant(x, y, request.vertical)))
        .unwrap_or(request.texture)
}

const fn map16_screen_variant(x: usize, y: usize, vertical: bool) -> usize {
    let major = if vertical { y } else { x };
    (major >> 4) & 3
}

fn resolved_custom_object_parts(
    record: &ObjectRecord,
    metadata: &lm_level::OscResolvedTable,
    variant: u8,
) -> Option<Vec<lm_render::CustomObjectPreviewTile>> {
    let object = metadata.default_display(record.command_id(), record.parameter(), variant)?;
    lm_render::render_resolved_lunar_magic_custom_object(object)
}

fn draw_custom_object_parts(
    painter: &egui::Painter,
    request: OrderedObjectDraw<'_>,
    placement: lm_level::NativeObjectPlacement,
    parts: &[lm_render::CustomObjectPreviewTile],
) {
    let (tile_x, tile_y) = placement.tile_coordinates(request.vertical);
    let origin = request.target.min
        + egui::vec2(
            f32::from(tile_x) * request.cell_size,
            f32::from(tile_y) * request.cell_size,
        );
    for part in parts {
        let offset = egui::vec2(
            f32::from(part.x) * request.cell_size / 16.0,
            f32::from(part.y) * request.cell_size / 16.0,
        );
        let target = egui::Rect::from_min_size(
            origin + offset,
            egui::vec2(request.cell_size, request.cell_size),
        );
        match map16_paint_source(part.tile, request.custom_map16) {
            Map16PaintSource::Base(tile) => {
                let x = usize::try_from(
                    i32::from(tile_x)
                        .saturating_add(i32::from(part.x) / 16)
                        .max(0),
                )
                .unwrap_or_default();
                let y = usize::try_from(
                    i32::from(tile_y)
                        .saturating_add(i32::from(part.y) / 16)
                        .max(0),
                )
                .unwrap_or_default();
                draw_map16_atlas_tile_for_tileset(
                    painter,
                    map16_texture_for_cell(request, x, y),
                    target,
                    tile,
                    request.object_tileset,
                );
            }
            Map16PaintSource::Custom(definition) => {
                if let Some(texture) = request.foreground_texture {
                    draw_custom_map16_tile(painter, texture, target, definition);
                } else {
                    draw_unresolved_map16_paint(painter, target, part.tile);
                }
            }
            Map16PaintSource::Unresolved => {
                draw_unresolved_map16_paint(painter, target, part.tile);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16PaintSource {
    Base(u16),
    Custom(lm_level::Map16Tile),
    Unresolved,
}

fn map16_paint_source(
    tile: u16,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
) -> Map16PaintSource {
    if tile < 0x200 {
        return Map16PaintSource::Base(tile);
    }
    let Some(lm_app::NativeMap16SidecarDocument::M16(sidecar)) = custom_map16 else {
        return Map16PaintSource::Unresolved;
    };
    sidecar
        .tile(usize::from(tile))
        .map_or(Map16PaintSource::Unresolved, Map16PaintSource::Custom)
}

fn draw_unresolved_map16_paint(painter: &egui::Painter, target: egui::Rect, tile: u16) {
    painter.rect_filled(
        target.shrink(1.0),
        1.0,
        egui::Color32::from_rgb(220, 70, 70),
    );
    painter.text(
        target.center(),
        egui::Align2::CENTER_CENTER,
        unresolved_map16_label(tile),
        egui::FontId::monospace(6.0),
        egui::Color32::WHITE,
    );
}

fn unresolved_map16_label(tile: u16) -> String {
    format!("{tile:04X}")
}

pub(crate) fn draw_custom_map16_tile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    definition: lm_level::Map16Tile,
) {
    draw_custom_map16_tile_with_outer_flips(painter, texture, target, definition, false, false);
}

fn draw_custom_map16_tile_with_outer_flips(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    definition: lm_level::Map16Tile,
    outer_x_flip: bool,
    outer_y_flip: bool,
) {
    let half = target.size() / 2.0;
    for (visual_quadrant, subtile) in map16_visual_subtiles(definition, outer_x_flip, outer_y_flip)
        .into_iter()
        .enumerate()
    {
        let output_x = visual_quadrant % 2;
        let output_y = visual_quadrant / 2;
        let position = target.min
            + egui::vec2(
                if output_x == 0 { 0.0 } else { half.x },
                if output_y == 0 { 0.0 } else { half.y },
            );
        let quadrant = egui::Rect::from_min_size(position, half);
        draw_foreground_subtile(painter, texture, quadrant, subtile);
    }
}

fn map16_visual_subtiles(
    definition: lm_level::Map16Tile,
    outer_x_flip: bool,
    outer_y_flip: bool,
) -> [lm_level::Subtile; 4] {
    let source = [
        definition.top_left,
        definition.top_right,
        definition.bottom_left,
        definition.bottom_right,
    ];
    std::array::from_fn(|visual_quadrant| {
        let output_x = visual_quadrant % 2;
        let output_y = visual_quadrant / 2;
        let source_x = output_x ^ usize::from(outer_x_flip);
        let source_y = output_y ^ usize::from(outer_y_flip);
        let mut subtile = source[source_y * 2 + source_x];
        subtile.0 ^= if outer_x_flip { 0x4000 } else { 0 };
        subtile.0 ^= if outer_y_flip { 0x8000 } else { 0 };
        subtile
    })
}

fn draw_foreground_subtile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    subtile: lm_level::Subtile,
) {
    const COLUMNS: f32 = 32.0;
    const ROWS: f32 = 128.0;
    let tile = subtile.tile_number();
    let column = f32::from(tile % 32);
    let row = f32::from(subtile.palette()) * 16.0 + f32::from(tile / 32);
    let (left, right) = if subtile.x_flip() {
        ((column + 1.0) / COLUMNS, column / COLUMNS)
    } else {
        (column / COLUMNS, (column + 1.0) / COLUMNS)
    };
    let (top, bottom) = if subtile.y_flip() {
        ((row + 1.0) / ROWS, row / ROWS)
    } else {
        (row / ROWS, (row + 1.0) / ROWS)
    };
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)),
        egui::Color32::WHITE,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_object_placement_markers(
    painter: &egui::Painter,
    cursor: Option<egui::Pos2>,
    canvas: egui::Rect,
    vertical: bool,
    records: &[ObjectRecord],
    placements: &[lm_level::NativeObjectPlacement],
    selected_group: &[usize],
    selected: usize,
    map16_texture: Option<&egui::TextureHandle>,
    artwork_bounds: &HashMap<usize, egui::Rect>,
    resize_models: &HashMap<usize, lm_render::StandardObjectResizeModel>,
    cell: f32,
    editor_overlays: bool,
    selection_visible: bool,
) -> ObjectPlacementHits {
    let mut hits = ObjectPlacementHits::default();
    for placement in placements {
        let index = placement.record_index;
        let Some(record) = records.get(index) else {
            continue;
        };
        let artwork_rect = artwork_bounds.get(&index).copied();
        let object_rect =
            artwork_rect.unwrap_or_else(|| encoded_object_rect(canvas, *placement, vertical, cell));
        let selected_visible = (selected_group.contains(&index)
            || selected_group.is_empty() && index == selected)
            && (editor_overlays || selection_visible);
        if editor_overlays || selected_visible {
            draw_object_marker(
                painter,
                map16_texture,
                object_rect,
                record,
                selected_visible,
                artwork_rect.is_some(),
            );
        }
        if editor_overlays
            && index == selected
            && let Some(&model) = resize_models.get(&index)
            && let Some(handle) =
                standard_object_resize_handle(canvas, *placement, record, model, vertical, cell)
        {
            painter.rect_filled(handle, 1.0, egui::Color32::YELLOW);
            painter.rect_stroke(
                handle,
                1.0,
                egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                egui::StrokeKind::Inside,
            );
            if cursor.is_some_and(|position| handle.contains(position)) {
                hits.resize = Some(index);
            }
        }
        if cursor.is_some_and(|position| object_rect.contains(position)) {
            hits.body = Some(index);
        }
    }
    hits
}

fn object_interactive_bounds(
    canvas: egui::Rect,
    vertical: bool,
    placements: &[lm_level::NativeObjectPlacement],
    artwork_bounds: &HashMap<usize, egui::Rect>,
    cell: f32,
) -> HashMap<usize, egui::Rect> {
    placements
        .iter()
        .map(|placement| {
            (
                placement.record_index,
                artwork_bounds
                    .get(&placement.record_index)
                    .copied()
                    .unwrap_or_else(|| encoded_object_rect(canvas, *placement, vertical, cell)),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ObjectPlacementHits {
    body: Option<usize>,
    resize: Option<usize>,
}

fn standard_object_resize_handle(
    canvas: egui::Rect,
    placement: lm_level::NativeObjectPlacement,
    record: &ObjectRecord,
    model: lm_render::StandardObjectResizeModel,
    vertical: bool,
    cell: f32,
) -> Option<egui::Rect> {
    use lm_render::StandardObjectResizeModel::{
        ExtendedCommand27Axes, Fixed, MajorByte, MajorNibble, MinorByte, MinorNibble,
        ParameterNibbles, SwappedParameterNibbles,
    };
    let encoded =
        authenticated_standard_object_rect(canvas, placement, record, model, vertical, cell)?;
    let center = match model {
        ParameterNibbles | SwappedParameterNibbles | ExtendedCommand27Axes => encoded.max,
        MajorNibble | MajorByte { .. } if vertical => {
            egui::pos2(encoded.center().x, encoded.bottom())
        }
        MajorNibble | MajorByte { .. } => egui::pos2(encoded.right(), encoded.center().y),
        MinorNibble { .. } | MinorByte { .. } if vertical => {
            egui::pos2(encoded.right(), encoded.center().y)
        }
        MinorNibble { .. } | MinorByte { .. } => egui::pos2(encoded.center().x, encoded.bottom()),
        Fixed => return None,
    };
    Some(egui::Rect::from_center_size(center, egui::vec2(8.0, 8.0)))
}

fn authenticated_standard_object_rect(
    canvas: egui::Rect,
    placement: lm_level::NativeObjectPlacement,
    record: &ObjectRecord,
    model: lm_render::StandardObjectResizeModel,
    vertical: bool,
    cell: f32,
) -> Option<egui::Rect> {
    use lm_render::StandardObjectResizeModel::{
        ExtendedCommand27Axes, Fixed, MajorByte, MajorNibble, MinorByte, MinorNibble,
        ParameterNibbles, SwappedParameterNibbles,
    };
    if model == ExtendedCommand27Axes {
        let (width, height) = record.extended_command27_tile_size()?;
        let (tile_x, tile_y) = placement.tile_coordinates(vertical);
        return Some(egui::Rect::from_min_size(
            canvas.min + egui::vec2(f32::from(tile_x) * cell, f32::from(tile_y) * cell),
            egui::vec2(f32::from(width) * cell, f32::from(height) * cell),
        ));
    }
    let (major_span, minor_span) = match model {
        ParameterNibbles => (
            u16::from(placement.major_span),
            u16::from(placement.minor_span),
        ),
        SwappedParameterNibbles => (
            u16::from(placement.minor_span),
            u16::from(placement.major_span),
        ),
        MajorNibble => (u16::from(placement.major_span), 1),
        MajorByte { fixed_minor_tiles } => (
            u16::from(record.parameter()) + 1,
            u16::from(fixed_minor_tiles),
        ),
        MinorNibble { fixed_major_tiles } => (
            u16::from(fixed_major_tiles),
            u16::from(placement.minor_span),
        ),
        MinorByte { fixed_major_tiles } => (
            u16::from(fixed_major_tiles),
            u16::from(record.parameter()) + 1,
        ),
        ExtendedCommand27Axes => unreachable!("handled above"),
        Fixed => return None,
    };
    let (tile_x, tile_y) = placement.tile_coordinates(vertical);
    let position = canvas.min + egui::vec2(f32::from(tile_x) * cell, f32::from(tile_y) * cell);
    let (tile_width, tile_height) = if vertical {
        (minor_span, major_span)
    } else {
        (major_span, minor_span)
    };
    Some(egui::Rect::from_min_size(
        position,
        egui::vec2(f32::from(tile_width) * cell, f32::from(tile_height) * cell),
    ))
}

fn encoded_object_rect(
    canvas: egui::Rect,
    placement: lm_level::NativeObjectPlacement,
    vertical: bool,
    cell: f32,
) -> egui::Rect {
    let (tile_x, tile_y) = placement.tile_coordinates(vertical);
    let position = canvas.min + egui::vec2(f32::from(tile_x) * cell, f32::from(tile_y) * cell);
    let (tile_width, tile_height) = if vertical {
        (placement.minor_span, placement.major_span)
    } else {
        (placement.major_span, placement.minor_span)
    };
    egui::Rect::from_min_size(
        position,
        egui::vec2(
            (f32::from(tile_width) * cell).max(8.0),
            (f32::from(tile_height) * cell).max(8.0),
        ),
    )
}

fn standard_object_cache_display_rect(
    encoded_rect: egui::Rect,
    canvas: egui::Rect,
    layout: lm_render::NativeLevelMap16Layout,
    cache: &lm_render::NativeLevelMap16Cache,
    cell: f32,
) -> egui::Rect {
    let mut bounds = encoded_rect;
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            if !cache.was_written(index) {
                continue;
            }
            let Ok(x) = u16::try_from(x) else {
                continue;
            };
            let Ok(y) = u16::try_from(y) else {
                continue;
            };
            bounds = bounds.union(egui::Rect::from_min_size(
                canvas.min + egui::vec2(f32::from(x) * cell, f32::from(y) * cell),
                egui::vec2(cell, cell),
            ));
        }
    }
    bounds
}

fn custom_object_display_rect(
    encoded_rect: egui::Rect,
    origin: egui::Pos2,
    parts: &[lm_render::CustomObjectPreviewTile],
    cell: f32,
) -> egui::Rect {
    parts.iter().fold(encoded_rect, |bounds, part| {
        let part_min = origin
            + egui::vec2(
                f32::from(part.x) * cell / 16.0,
                f32::from(part.y) * cell / 16.0,
            );
        bounds.union(egui::Rect::from_min_size(part_min, egui::vec2(cell, cell)))
    })
}

fn draw_object_marker(
    painter: &egui::Painter,
    texture: Option<&egui::TextureHandle>,
    target: egui::Rect,
    record: &ObjectRecord,
    selected: bool,
    artwork_rendered: bool,
) {
    if let (Some(tile), Some(texture)) = (marker_fallback_tile(record, artwork_rendered), texture) {
        draw_map16_atlas_tile(painter, texture, target.shrink(1.0), tile);
    } else if !artwork_rendered {
        painter.rect_filled(
            target.shrink(1.0),
            1.0,
            egui::Color32::from_rgb(80, 170, 230),
        );
        painter.text(
            target.center(),
            egui::Align2::CENTER_CENTER,
            format!("{:X}", record.command_id()),
            egui::FontId::monospace(8.0),
            egui::Color32::BLACK,
        );
    }
    if selected {
        painter.rect_stroke(
            target,
            1.0,
            egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
            egui::StrokeKind::Inside,
        );
    }
}

fn marker_fallback_tile(record: &ObjectRecord, artwork_rendered: bool) -> Option<u16> {
    (!artwork_rendered && record.command_id() == 0)
        .then(|| lm_render::lunar_magic_shared_extended_object_tile(record.parameter()))
        .flatten()
}

const fn is_boss_battle_level_mode(level_mode: u8) -> bool {
    matches!(level_mode & 0x1f, 0x09 | 0x0b | 0x10)
}

fn paint_boss_battle_diagnostic(painter: &egui::Painter, target: egui::Rect) {
    const PLANE_PIXELS: usize = 512;
    const MESSAGE: &str = "CANNOT RENDER : This is a boss battle level!";
    let rows = target.height().ceil().max(1.0) as usize;
    for row in 0..rows {
        let red = boss_battle_diagnostic_red(row);
        let minimum = target.min + egui::vec2(0.0, row as f32);
        painter.rect_filled(
            egui::Rect::from_min_size(minimum, egui::vec2(target.width(), 1.0)),
            0.0,
            egui::Color32::from_rgb(red, 0, 0),
        );
    }
    let mut x = target.min.x + 106.0;
    while x < target.max.x {
        painter.text(
            egui::pos2(x, target.min.y + 256.0),
            egui::Align2::LEFT_CENTER,
            MESSAGE,
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
        x += PLANE_PIXELS as f32;
    }
}

const fn boss_battle_diagnostic_red(row: usize) -> u8 {
    let plane_y = row % 512;
    if plane_y < 256 {
        plane_y as u8
    } else {
        (511 - plane_y) as u8
    }
}

#[derive(Clone, Copy)]
struct SpritePlacementDraw<'a> {
    painter: &'a egui::Painter,
    overlay_painter: &'a egui::Painter,
    target: egui::Rect,
    cell_size: f32,
    texture: Option<&'a egui::TextureHandle>,
    animated_texture: Option<&'a egui::TextureHandle>,
    placements: &'a [lm_level::NativeSpritePlacement],
    cursor: Option<egui::Pos2>,
    selected_group: &'a [usize],
    selected: usize,
    vertical: bool,
    level_mode: u8,
    sprite_tileset: u8,
    sprite_memory_index: u8,
    animation_phase: u8,
    silver_pow_active: bool,
    custom_sprites: Option<&'a lm_level::SscResolvedTable>,
    custom_map16: Option<&'a lm_app::NativeMap16SidecarDocument>,
    external_textures: &'a HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    editor_overlays: bool,
    selection_visible: bool,
    selected_only: bool,
}

#[allow(clippy::too_many_lines)]
struct SpritePlacementDrawResult {
    hit: Option<usize>,
    bounds: HashMap<usize, egui::Rect>,
}

fn draw_sprite_placements(request: SpritePlacementDraw<'_>) -> SpritePlacementDrawResult {
    let SpritePlacementDraw {
        painter,
        overlay_painter,
        target,
        cell_size,
        texture,
        animated_texture,
        placements,
        cursor,
        selected_group,
        selected,
        vertical,
        level_mode,
        sprite_tileset,
        sprite_memory_index,
        animation_phase,
        silver_pow_active,
        custom_sprites,
        custom_map16,
        external_textures,
        editor_overlays,
        selection_visible,
        selected_only,
    } = request;
    let mut hit = None;
    let mut bounds = HashMap::with_capacity(placements.len());
    let mut standard_8a_count = 0_u8;
    for placement in placements {
        let (tile_x, tile_y) = presented_sprite_tile_coordinates(*placement, vertical);
        let center = target.min
            + egui::vec2(
                (f32::from(tile_x) + 0.5) * cell_size,
                (f32::from(tile_y) + 0.5) * cell_size,
            );
        let marker = egui::Rect::from_center_size(
            center,
            egui::vec2(cell_size.max(9.0), cell_size.max(9.0)),
        );
        let custom_display = custom_sprites.and_then(|table| {
            table
                .default_display(placement.sprite_number, placement.extra_bits)
                .map(|sprite| (table, sprite))
        });
        let custom_preview = custom_display.and_then(|(table, sprite)| {
            lm_render::render_atlas_lunar_magic_custom_sprite_with(table, sprite, |index| {
                external_sprite_definition(custom_map16, index)
            })
        });
        let external_preview = custom_display.and_then(|(table, sprite)| {
            lm_render::render_remapped_lunar_magic_custom_sprite_with(table, sprite, |index| {
                external_sprite_definition(custom_map16, index)
            })
        });
        let uses_standard = custom_display.is_none();
        let preview = if uses_standard {
            lm_render::render_lunar_magic_standard_sprite_with_mode(placement.sprite_number, {
                let mut mode = standard_sprite_preview_mode(
                    placement,
                    vertical,
                    level_mode,
                    sprite_tileset,
                    sprite_memory_index,
                    animation_phase,
                    standard_8a_count,
                );
                mode.alternate_display = silver_pow_active;
                mode
            })
        } else {
            custom_preview
        };
        if uses_standard && placement.sprite_number == 0x8a {
            standard_8a_count = standard_8a_count.saturating_add(1);
        }
        let is_selected = selected_group.contains(&placement.token_index)
            || selected_group.is_empty() && placement.token_index == selected;
        if selected_only && !is_selected {
            continue;
        }
        let interactive_rect = resolved_sprite_preview_bounds(
            marker,
            preview.as_deref(),
            external_preview.as_deref(),
            cell_size,
        );
        bounds.insert(placement.token_index, interactive_rect);
        if let (Some(texture), Some(parts)) = (texture, preview.as_deref()) {
            for part in parts {
                draw_sprite_preview_definition_tinted(
                    painter,
                    texture,
                    animated_texture,
                    wrap_horizontal_sprite_preview_rect(
                        sprite_preview_part_rect(marker, part.x, part.y, cell_size),
                        target,
                        cell_size,
                        vertical,
                    ),
                    part.subtiles,
                    // Lunar Magic draws $E1's ghost definition at 50% opacity while keeping
                    // its separate $114 star overlay opaque. egui tint colors are premultiplied;
                    // white-with-alpha would gamma-adjust this to roughly 75% opacity.
                    if uses_standard
                        && level_mode == 0x0c
                        && matches!(placement.sprite_number, 0x38..=0x39)
                    {
                        egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
                    } else {
                        sprite_preview_source_tint(
                            uses_standard.then_some(placement.sprite_number),
                            part.definition_index,
                        )
                    },
                );
            }
        } else if let Some(parts) = external_preview.as_deref()
            && parts
                .iter()
                .all(|part| external_textures.contains_key(part))
        {
            for part in parts {
                let texture = &external_textures[part];
                draw_external_sprite_part(
                    painter,
                    texture,
                    sprite_preview_part_rect(marker, part.x, part.y, cell_size),
                );
            }
        } else if editor_overlays
            && should_draw_unresolved_sprite_marker(uses_standard, placement.sprite_number)
        {
            overlay_painter.rect_filled(
                marker,
                marker.width() / 2.0,
                if is_selected {
                    egui::Color32::LIGHT_RED
                } else {
                    egui::Color32::from_rgb(220, 70, 70)
                },
            );
            overlay_painter.text(
                marker.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:02X}", placement.sprite_number),
                egui::FontId::monospace(7.0),
                egui::Color32::WHITE,
            );
        }
        if (editor_overlays || selection_visible) && is_selected {
            overlay_painter.rect_stroke(
                interactive_rect,
                marker.width() / 2.0,
                egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                egui::StrokeKind::Inside,
            );
        }
        if cursor.is_some_and(|position| interactive_rect.contains(position)) {
            hit = Some(placement.token_index);
        }
    }
    SpritePlacementDrawResult { hit, bounds }
}

fn sprite_preview_part_rect(marker: egui::Rect, x: i16, y: i16, cell_size: f32) -> egui::Rect {
    marker.translate(egui::vec2(
        f32::from(x) * cell_size / 16.0,
        f32::from(y) * cell_size / 16.0,
    ))
}

fn wrap_horizontal_sprite_preview_rect(
    mut part: egui::Rect,
    canvas: egui::Rect,
    cell_size: f32,
    vertical: bool,
) -> egui::Rect {
    if vertical || canvas.height() <= 0.0 || cell_size <= 0.0 {
        return part;
    }
    while part.min.y >= canvas.max.y {
        part = part.translate(egui::vec2(16.0 * cell_size, -canvas.height()));
    }
    part
}

fn sprite_preview_bounds(
    marker: egui::Rect,
    offsets: impl IntoIterator<Item = (i16, i16)>,
    cell_size: f32,
) -> egui::Rect {
    offsets.into_iter().fold(marker, |bounds, (x, y)| {
        bounds.union(sprite_preview_part_rect(marker, x, y, cell_size))
    })
}

fn resolved_sprite_preview_bounds(
    marker: egui::Rect,
    atlas_parts: Option<&[lm_render::StandardSpritePreviewTile]>,
    remapped_parts: Option<&[lm_render::RemappedCustomSpritePreviewTile]>,
    cell_size: f32,
) -> egui::Rect {
    if let Some(parts) = atlas_parts {
        sprite_preview_bounds(marker, parts.iter().map(|part| (part.x, part.y)), cell_size)
    } else if let Some(parts) = remapped_parts {
        sprite_preview_bounds(marker, parts.iter().map(|part| (part.x, part.y)), cell_size)
    } else {
        marker
    }
}

fn should_draw_unresolved_sprite_marker(uses_standard: bool, sprite_number: u8) -> bool {
    !uses_standard
        || lm_render::lunar_magic_standard_sprite_preview_source(sprite_number)
            != lm_render::StandardSpritePreviewSource::NativeEmpty
}

pub(crate) fn standard_sprite_preview_mode(
    placement: &lm_level::NativeSpritePlacement,
    vertical: bool,
    level_mode: u8,
    sprite_tileset: u8,
    sprite_memory_index: u8,
    animation_phase: u8,
    sprite_8a_sequence_index: u8,
) -> lm_render::StandardSpritePreviewMode {
    lm_render::StandardSpritePreviewMode {
        placement_first: placement.packed_display_position(),
        serialized_first_byte: placement.first_byte,
        placement_major: placement.major,
        placement_minor: placement.minor,
        level_mode,
        extra_bits: placement.extra_bits,
        sprite_graphics_mode: sprite_tileset,
        wide_context: if sprite_memory_index == 1 {
            lm_render::StandardSpriteWideContext::ValidLong64
        } else {
            lm_render::StandardSpriteWideContext::ValidShort
        },
        animation_phase,
        sprite_8a_sequence_index,
        level_orientation: if vertical {
            lm_render::StandardLevelOrientation::Vertical
        } else {
            lm_render::StandardLevelOrientation::Horizontal
        },
        ..lm_render::StandardSpritePreviewMode::default()
    }
}

fn sprite_animation_phase(seconds: f64) -> u8 {
    if let Ok(phase) = std::env::var("LM_NATIVE_ANIMATION_PHASE")
        && let Ok(phase) = phase.parse::<u8>()
        && phase < 8
    {
        return phase / 2;
    }
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds * 8.0).floor() as u64;
    u8::try_from(ticks & 3).expect("two-bit animation phase")
}

fn map16_animation_phase(seconds: f64) -> u8 {
    if let Ok(phase) = std::env::var("LM_NATIVE_ANIMATION_PHASE")
        && let Ok(phase) = phase.parse::<u8>()
        && phase < 8
    {
        return phase;
    }
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds * (1000.0 / 60.0)).floor() as u64;
    u8::try_from(ticks & 7).expect("three-bit animation phase")
}

fn external_sprite_definition(
    document: Option<&lm_app::NativeMap16SidecarDocument>,
    index: u16,
) -> Option<[u16; 4]> {
    let lm_app::NativeMap16SidecarDocument::M16(sidecar) = document? else {
        return None;
    };
    let tile = sidecar.tile(usize::from(index))?;
    Some([
        tile.top_left.0,
        tile.top_right.0,
        tile.bottom_left.0,
        tile.bottom_right.0,
    ])
}

#[cfg(test)]
type ObjectDrawLayer<'a> = (&'a [ObjectRecord], &'a [lm_level::NativeObjectPlacement]);

#[cfg(test)]
fn object_draw_layers<'a>(
    layer2_records: &'a [ObjectRecord],
    layer2_placements: &'a [lm_level::NativeObjectPlacement],
    layer1_records: &'a [ObjectRecord],
    layer1_placements: &'a [lm_level::NativeObjectPlacement],
) -> [ObjectDrawLayer<'a>; 2] {
    [
        (layer2_records, layer2_placements),
        (layer1_records, layer1_placements),
    ]
}

#[derive(Clone, Copy)]
struct SpriteRasterAssets<'a> {
    external: &'a lm_graphics::ExternalSpriteAssets,
    foreground_tiles: &'a [lm_graphics::IndexedTile],
    layer3_tiles: &'a [lm_graphics::IndexedTile],
    vanilla_tiles: &'a [lm_graphics::IndexedTile],
    vanilla_palette: Option<&'a lm_graphics::Palette>,
}

fn ensure_remapped_placement_textures(
    context: &egui::Context,
    textures: &mut HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    custom_sprites: Option<&lm_level::SscResolvedTable>,
    assets: SpriteRasterAssets<'_>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    placements: &[lm_level::NativeSpritePlacement],
) {
    let Some(table) = custom_sprites else {
        return;
    };
    for placement in placements {
        let Some(sprite) = table.default_display(placement.sprite_number, placement.extra_bits)
        else {
            continue;
        };
        let Some(parts) =
            lm_render::render_remapped_lunar_magic_custom_sprite_with(table, sprite, |index| {
                external_sprite_definition(custom_map16, index)
            })
        else {
            continue;
        };
        ensure_remapped_part_textures(context, textures, &parts, assets);
    }
}

fn ensure_remapped_part_textures(
    context: &egui::Context,
    textures: &mut HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    parts: &[lm_render::RemappedCustomSpritePreviewTile],
    assets: SpriteRasterAssets<'_>,
) {
    for part in parts {
        if textures.contains_key(part) {
            continue;
        }
        let Some(canvas) = lm_render::raster_remapped_custom_sprite_tile_with(
            part,
            |global_tile| resolve_ssc_graphics_tile(assets, global_tile),
            |source, palette, color| match source {
                Some(source) => assets.external.palette_color(source, palette, color),
                None => ordinary_ssc_palette_color(
                    assets.vanilla_palette?,
                    part.graphics_base,
                    palette,
                    color,
                ),
            },
        ) else {
            continue;
        };
        let rgba = canvas
            .pixels()
            .iter()
            .flat_map(|pixel| [pixel.red, pixel.green, pixel.blue, pixel.alpha])
            .collect::<Vec<_>>();
        let image =
            egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
        let texture = context.load_texture(
            format!(
                "ssc-external-{:04X}-{:04X}-{:04X}",
                part.definition_index,
                part.graphics_base,
                part.palette_source.unwrap_or(0)
            ),
            image,
            egui::TextureOptions::NEAREST,
        );
        textures.insert(*part, texture);
    }
}

fn resolve_ssc_graphics_tile(
    assets: SpriteRasterAssets<'_>,
    global_tile: u16,
) -> Option<&lm_graphics::IndexedTile> {
    if global_tile >= lm_graphics::EXTERNAL_SPRITE_GRAPHICS_BASE_TILE {
        return assets.external.graphics_tile(global_tile);
    }
    if let Some(layer3_tile) = global_tile.checked_sub(0x900)
        && layer3_tile < 0x400
    {
        return assets.layer3_tiles.get(usize::from(layer3_tile));
    }
    if let Some(sprite_tile) = global_tile.checked_sub(0x400)
        && sprite_tile < 0x400
    {
        return assets.vanilla_tiles.get(usize::from(sprite_tile));
    }
    assets.foreground_tiles.get(usize::from(global_tile))
}

fn ordinary_ssc_palette_color(
    palette: &lm_graphics::Palette,
    graphics_base: u16,
    subtile_palette: u8,
    color: u8,
) -> Option<lm_graphics::Rgb8> {
    if subtile_palette > 7 || !(1..=15).contains(&color) {
        return None;
    }
    let base_row = usize::from(graphics_base != 0) * 8;
    let row = base_row.checked_add(usize::from(subtile_palette))?;
    palette
        .colors
        .get(row.checked_mul(16)?.checked_add(usize::from(color))?)
        .copied()
        .map(lm_graphics::Bgr555::to_rgb8)
}

fn draw_external_sprite_part(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
) {
    painter.image(
        texture.id(),
        target,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

pub(crate) fn draw_sprite_preview_definition(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    subtiles: [u16; 4],
) {
    draw_sprite_preview_definition_tinted(
        painter,
        texture,
        None,
        target,
        subtiles,
        egui::Color32::WHITE,
    );
}

fn draw_sprite_preview_definition_tinted(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    animated_texture: Option<&egui::TextureHandle>,
    target: egui::Rect,
    subtiles: [u16; 4],
    tint: egui::Color32,
) {
    for (quadrant, word) in subtiles.into_iter().enumerate() {
        let half = target.size() / 2.0;
        let (x, y) = sprite_definition_quadrant_position(quadrant);
        let minimum = target.min + egui::vec2(f32::from(x) * half.x, f32::from(y) * half.y);
        draw_sprite_atlas_subtile(
            painter,
            sprite_preview_texture(texture, animated_texture, word),
            egui::Rect::from_min_size(minimum, half),
            word,
            tint,
        );
    }
}

fn sprite_preview_texture<'a>(
    texture: &'a egui::TextureHandle,
    animated_texture: Option<&'a egui::TextureHandle>,
    word: u16,
) -> &'a egui::TextureHandle {
    // Lunar Magic's decoded animation cache supplies sprite tile page 2. Page 0
    // remains backed by the four ordinary SP slots even when its low tile number
    // overlaps an animation destination. Applying the animated atlas globally
    // turns definition $1CB's page-0 gray blocks into animation-group-$12 lines.
    if sprite_preview_uses_animated_page(word) {
        animated_texture.unwrap_or(texture)
    } else {
        texture
    }
}

const fn sprite_preview_uses_animated_page(word: u16) -> bool {
    word & 0x0200 != 0
}

const fn standard_sprite_preview_tint(sprite_number: u8, definition_index: u16) -> egui::Color32 {
    if (sprite_number == 0xe1 && definition_index == 0x1b8)
        || (sprite_number == 0x90 && definition_index >= 0x1c0 && definition_index <= 0x1f3)
    {
        egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
    } else {
        egui::Color32::WHITE
    }
}

fn sprite_definition_quadrant_position(quadrant: usize) -> (u16, u16) {
    (
        u16::try_from(quadrant / 2).expect("quadrant x fits u16"),
        u16::try_from(quadrant % 2).expect("quadrant y fits u16"),
    )
}

fn draw_sprite_atlas_subtile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    word: u16,
    tint: egui::Color32,
) {
    // Bit $200 selects Lunar Magic's separately materialized animated sprite page. The caller
    // has already selected that page's texture, so its UV address is the remaining nine-bit
    // tile index. Keeping the page bit here incorrectly advances into the next palette band.
    let tile = usize::from(word & 0x01ff);
    let palette = usize::from((word >> 10) & 7);
    let slot = tile / 128;
    let within_slot = tile % 128;
    let column = slot % 2 * 16 + within_slot % 16;
    let row = palette * 16 + slot / 2 * 8 + within_slot / 16;
    let column = u16::try_from(column).expect("sprite atlas has 32 columns");
    let row = u16::try_from(row).expect("sprite atlas has 128 rows");
    let mut minimum = egui::pos2(f32::from(column) / 32.0, f32::from(row) / 128.0);
    let mut maximum = egui::pos2(f32::from(column + 1) / 32.0, f32::from(row + 1) / 128.0);
    if word & 0x4000 != 0 {
        std::mem::swap(&mut minimum.x, &mut maximum.x);
    }
    if word & 0x8000 != 0 {
        std::mem::swap(&mut minimum.y, &mut maximum.y);
    }
    let uv = egui::Rect::from_min_max(minimum, maximum);
    painter.image(texture.id(), target, uv, tint);
}

fn header_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=maximum));
    ui.end_row();
}

fn sprite_save_constraint(ui: &mut egui::Ui, controller: Option<&LevelController>) {
    let Some(controller) = controller else {
        return;
    };
    match controller.sprite_encoded_lengths() {
        Ok((original, staged)) if staged > original => {
            ui.small(format!(
                "Sprite stream: {original} → {staged} bytes. Commit will allocate a RATS-owned copy in the original shared bank, update only this level's low pointer, and preserve the old unowned bytes."
            ));
        }
        Ok((original, staged)) => {
            ui.small(format!(
                "Sprite stream: {original} → {staged} bytes. Commit can replace this level's exclusive shared-bank stream in place and repairs the checksum."
            ));
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Sprite stream cannot be serialized: {error}"),
            );
        }
    }
}

fn show_compact_object_fields(ui: &mut egui::Ui, id: &str, form: &mut ObjectForm) {
    if form.screen_exit.is_some() || form.screen_jump.is_some() {
        ui.small("Select an ordinary Layer 2 object to edit semantic fields.");
        return;
    }
    egui::Grid::new(id).show(ui, |ui| {
        header_row(ui, "Command", &mut form.command_id, 0x3f);
        header_row(ui, "Parameter", &mut form.parameter, 0xff);
        header_row(ui, "Coordinate A", &mut form.first_coordinate, 0x0f);
        header_row(ui, "Coordinate B", &mut form.second_coordinate, 0x0f);
        ui.label("Advance screen");
        ui.checkbox(&mut form.advances_screen, "");
        ui.end_row();
    });
}

fn show_raw_object_record(ui: &mut egui::Ui, id: &str, form: &mut ObjectForm) {
    egui::CollapsingHeader::new("Raw native object record")
        .id_salt(id)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Bytes");
                ui.text_edit_singleline(&mut form.encoded)
                    .on_hover_text("Three to eight hexadecimal bytes separated by whitespace");
            });
            ui.small(
                "Apply raw record preserves and exposes command-specific extension bytes. \
                 The encoded command must declare exactly the supplied native record length.",
            );
        });
}

#[allow(clippy::too_many_lines)]
fn show_standard_object_resize_fields(
    ui: &mut egui::Ui,
    model: Option<lm_render::StandardObjectResizeModel>,
    form: &mut ObjectForm,
) {
    let Some(model) = model else {
        return;
    };
    ui.group(|ui| {
        ui.label("Authenticated object size");
        egui::Grid::new(("standard-object-resize", ui.id())).show(ui, |ui| match model {
            lm_render::StandardObjectResizeModel::ParameterNibbles => {
                let mut major = (form.parameter >> 4) + 1;
                ui.label("Major-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut major).range(1..=16))
                    .changed()
                {
                    form.parameter = set_standard_object_major_tiles(model, form.parameter, major)
                        .expect("bounded major-axis control");
                }
                ui.end_row();
                let mut minor = (form.parameter & 0x0f) + 1;
                ui.label("Minor-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut minor).range(1..=16))
                    .changed()
                {
                    form.parameter =
                        set_standard_object_minor_tiles(model, form.parameter, u16::from(minor))
                            .expect("bounded minor-axis control");
                }
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::SwappedParameterNibbles => {
                let mut major = (form.parameter & 0x0f) + 1;
                ui.label("Major-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut major).range(1..=16))
                    .changed()
                {
                    form.parameter = set_standard_object_major_tiles(model, form.parameter, major)
                        .expect("bounded swapped major-axis control");
                }
                ui.end_row();
                let mut minor = (form.parameter >> 4) + 1;
                ui.label("Minor-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut minor).range(1..=16))
                    .changed()
                {
                    form.parameter =
                        set_standard_object_minor_tiles(model, form.parameter, u16::from(minor))
                            .expect("bounded swapped minor-axis control");
                }
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::MajorNibble => {
                let mut major = (form.parameter >> 4) + 1;
                ui.label("Major-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut major).range(1..=16))
                    .changed()
                {
                    form.parameter = set_standard_object_major_tiles(model, form.parameter, major)
                        .expect("bounded major-axis control");
                }
                ui.end_row();
                ui.label("Minor-axis tiles");
                ui.label("1 (fixed)");
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::MinorNibble { fixed_major_tiles } => {
                ui.label("Major-axis tiles");
                ui.label(format!("{fixed_major_tiles} (fixed)"));
                ui.end_row();
                let mut minor = (form.parameter & 0x0f) + 1;
                ui.label("Minor-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut minor).range(1..=16))
                    .changed()
                {
                    form.parameter =
                        set_standard_object_minor_tiles(model, form.parameter, u16::from(minor))
                            .expect("bounded minor-axis control");
                }
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::MajorByte { fixed_minor_tiles } => {
                let mut major = u16::from(form.parameter) + 1;
                ui.label("Major-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut major).range(1..=256))
                    .changed()
                {
                    form.parameter = set_standard_object_major_byte_tiles(model, major)
                        .expect("bounded full-byte major-axis control");
                }
                ui.end_row();
                ui.label("Minor-axis tiles");
                ui.label(format!("{fixed_minor_tiles} (fixed)"));
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::MinorByte { fixed_major_tiles } => {
                ui.label("Major-axis tiles");
                ui.label(format!("{fixed_major_tiles} (fixed)"));
                ui.end_row();
                let mut minor = u16::from(form.parameter) + 1;
                ui.label("Minor-axis tiles");
                if ui
                    .add(egui::DragValue::new(&mut minor).range(1..=256))
                    .changed()
                {
                    form.parameter = set_standard_object_minor_tiles(model, form.parameter, minor)
                        .expect("bounded full-byte minor-axis control");
                }
                ui.end_row();
            }
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes => {
                let (mut horizontal, mut vertical) =
                    form.extended_command27_size.unwrap_or((1, 1));
                ui.label("Horizontal tiles");
                ui.add(egui::DragValue::new(&mut horizontal).range(1..=128));
                ui.end_row();
                ui.label("Vertical tiles");
                ui.add(egui::DragValue::new(&mut vertical).range(1..=128));
                ui.end_row();
                form.extended_command27_size = Some((horizontal, vertical));
            }
            lm_render::StandardObjectResizeModel::Fixed => {
                ui.label("Size");
                ui.label("fixed by active tileset handler");
                ui.end_row();
            }
        });
        ui.small(
            "Size controls update only the authenticated native fields; use Apply object fields to commit.",
        );
    });
}

fn set_standard_object_major_tiles(
    model: lm_render::StandardObjectResizeModel,
    parameter: u8,
    tiles: u8,
) -> Result<u8, String> {
    if !(1..=16).contains(&tiles) {
        return Err("major-axis object size must be 1–16 tiles".into());
    }
    match model {
        lm_render::StandardObjectResizeModel::ParameterNibbles
        | lm_render::StandardObjectResizeModel::MajorNibble => {
            Ok(((tiles - 1) << 4) | (parameter & 0x0f))
        }
        lm_render::StandardObjectResizeModel::SwappedParameterNibbles => {
            Ok((parameter & 0xf0) | (tiles - 1))
        }
        _ => Err("active object handler does not encode a resizable major axis".into()),
    }
}

fn set_standard_object_minor_tiles(
    model: lm_render::StandardObjectResizeModel,
    parameter: u8,
    tiles: u16,
) -> Result<u8, String> {
    match model {
        lm_render::StandardObjectResizeModel::ParameterNibbles
        | lm_render::StandardObjectResizeModel::MinorNibble { .. } => {
            let tiles = u8::try_from(tiles)
                .ok()
                .filter(|tiles| (1..=16).contains(tiles))
                .ok_or_else(|| "minor-axis nibble size must be 1–16 tiles".to_owned())?;
            Ok((parameter & 0xf0) | (tiles - 1))
        }
        lm_render::StandardObjectResizeModel::MinorByte { .. } => {
            let encoded = tiles
                .checked_sub(1)
                .and_then(|tiles| u8::try_from(tiles).ok())
                .ok_or_else(|| "minor-axis byte size must be 1–256 tiles".to_owned())?;
            Ok(encoded)
        }
        lm_render::StandardObjectResizeModel::SwappedParameterNibbles => {
            let tiles = u8::try_from(tiles)
                .ok()
                .filter(|tiles| (1..=16).contains(tiles))
                .ok_or_else(|| "minor-axis nibble size must be 1–16 tiles".to_owned())?;
            Ok(((tiles - 1) << 4) | (parameter & 0x0f))
        }
        _ => Err("active object handler does not encode a resizable minor axis".into()),
    }
}

fn set_standard_object_major_byte_tiles(
    model: lm_render::StandardObjectResizeModel,
    tiles: u16,
) -> Result<u8, String> {
    if !matches!(
        model,
        lm_render::StandardObjectResizeModel::MajorByte { .. }
    ) {
        return Err("active object handler does not encode a full-byte major axis".into());
    }
    tiles
        .checked_sub(1)
        .and_then(|tiles| u8::try_from(tiles).ok())
        .ok_or_else(|| "major-axis byte size must be 1–256 tiles".to_owned())
}

#[allow(clippy::too_many_arguments)]
fn resized_standard_object_record_at_canvas_position(
    record: &ObjectRecord,
    placement: lm_level::NativeObjectPlacement,
    model: lm_render::StandardObjectResizeModel,
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
) -> Result<ObjectRecord, String> {
    let mut resized = record.clone();
    if model == lm_render::StandardObjectResizeModel::ExtendedCommand27Axes {
        if cell <= 0.0 || !canvas.contains(position) {
            return Err("object resize ended outside the native level canvas".into());
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "validated finite canvas coordinates are intentionally quantized to tile indexes"
        )]
        let target_x = ((position.x - canvas.left()) / cell).floor() as i32;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "validated finite canvas coordinates are intentionally quantized to tile indexes"
        )]
        let target_y = ((position.y - canvas.top()) / cell).floor() as i32;
        let (origin_x, origin_y) = placement.tile_coordinates(vertical);
        let width = target_x - i32::from(origin_x) + 1;
        let height = target_y - i32::from(origin_y) + 1;
        if width < 1 || height < 1 {
            return Err("object resize handle cannot move before the object origin".into());
        }
        resized
            .set_extended_command27_tile_size(
                u8::try_from(width).map_err(|_| "horizontal object size is too large")?,
                u8::try_from(height).map_err(|_| "vertical object size is too large")?,
            )
            .map_err(|error| error.to_string())?;
        return Ok(resized);
    }
    let parameter = resized_standard_object_parameter_at_canvas_position(
        record, placement, model, position, canvas, cell, vertical,
    )?;
    resized
        .set_parameter(parameter)
        .map_err(|error| error.to_string())?;
    Ok(resized)
}

#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    reason = "validated finite canvas coordinates are intentionally quantized to tile indexes"
)]
fn resized_standard_object_parameter_at_canvas_position(
    record: &ObjectRecord,
    placement: lm_level::NativeObjectPlacement,
    model: lm_render::StandardObjectResizeModel,
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
) -> Result<u8, String> {
    if cell <= 0.0 || !canvas.contains(position) {
        return Err("object resize ended outside the native level canvas".into());
    }
    let target_x = ((position.x - canvas.left()) / cell).floor() as i32;
    let target_y = ((position.y - canvas.top()) / cell).floor() as i32;
    let (origin_x, origin_y) = placement.tile_coordinates(vertical);
    let width = target_x - i32::from(origin_x) + 1;
    let height = target_y - i32::from(origin_y) + 1;
    let (major, minor) = if vertical {
        (height, width)
    } else {
        (width, height)
    };
    let mut parameter = record.parameter();
    match model {
        lm_render::StandardObjectResizeModel::ParameterNibbles
        | lm_render::StandardObjectResizeModel::SwappedParameterNibbles => {
            if major < 1 || minor < 1 {
                return Err("object resize handle cannot move before the object origin".into());
            }
            parameter = set_standard_object_major_tiles(
                model,
                parameter,
                u8::try_from(major).map_err(|_| "major-axis object size is too large")?,
            )?;
            set_standard_object_minor_tiles(
                model,
                parameter,
                u16::try_from(minor).map_err(|_| "minor-axis object size is too large")?,
            )
        }
        lm_render::StandardObjectResizeModel::MajorNibble => {
            if major < 1 {
                return Err(
                    "major-axis object resize handle cannot move before the object origin".into(),
                );
            }
            set_standard_object_major_tiles(
                model,
                parameter,
                u8::try_from(major).map_err(|_| "major-axis object size is too large")?,
            )
        }
        lm_render::StandardObjectResizeModel::MajorByte { .. } => {
            if major < 1 {
                return Err(
                    "major-axis object resize handle cannot move before the object origin".into(),
                );
            }
            set_standard_object_major_byte_tiles(
                model,
                u16::try_from(major).map_err(|_| "major-axis object size is too large")?,
            )
        }
        lm_render::StandardObjectResizeModel::MinorNibble { .. }
        | lm_render::StandardObjectResizeModel::MinorByte { .. } => {
            if minor < 1 {
                return Err(
                    "minor-axis object resize handle cannot move before the object origin".into(),
                );
            }
            set_standard_object_minor_tiles(
                model,
                parameter,
                u16::try_from(minor).map_err(|_| "minor-axis object size is too large")?,
            )
        }
        lm_render::StandardObjectResizeModel::ExtendedCommand27Axes => {
            Err("extended command $27 sizes are edited through their two native fields".into())
        }
        lm_render::StandardObjectResizeModel::Fixed => {
            Err("active object handler has a fixed size".into())
        }
    }
}

fn editor_layer2_layout(
    snapshot: &lm_app::ControllerSnapshot,
    level: u16,
) -> Result<Option<lm_project::LevelLayer2RomLayout>, String> {
    let rom =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    lm_profile::smw_us_v1_level_layer2_layout(&rom, usize::from(level))
        .map_err(|error| error.to_string())
}

fn editor_level_layout(
    snapshot: &lm_app::ControllerSnapshot,
    level: u16,
) -> Result<lm_project::LevelRomLayout, String> {
    let rom =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
    layout.sprites =
        lm_profile::smw_us_v1_sprite_pointer_table(&rom).map_err(|error| error.to_string())?;
    let sprite_pointer = layout
        .sprites
        .read_snes_pointer(&rom, usize::from(level))
        .map_err(|error| error.to_string())?;
    let sprite_offset = sprite_pointer
        .to_pc(layout.mapper)
        .map_err(|error| error.to_string())?;
    let header = rom
        .read(sprite_offset, 1)
        .map_err(|error| error.to_string())?[0];
    layout.expanded_sprites = lm_level::NativeSpriteStream::header_uses_expanded_framing(header);
    Ok(layout)
}

fn workspace_tool_width(available_width: f32) -> f32 {
    ROM_LEVEL_TOOL_PANEL_WIDTH.min((available_width * 0.49).max(280.0))
}

fn load_animation_textures(
    context: &egui::Context,
    name_prefix: &str,
    images: Vec<egui::ColorImage>,
) -> Vec<egui::TextureHandle> {
    images
        .into_iter()
        .enumerate()
        .map(|(phase, image)| {
            context.load_texture(
                format!("{name_prefix}-phase-{phase}"),
                image,
                egui::TextureOptions::NEAREST,
            )
        })
        .collect()
}

fn clamped_scroll_offset(requested: f32, content_extent: f32, viewport_extent: f32) -> f32 {
    requested.clamp(0.0, (content_extent - viewport_extent).max(0.0))
}

fn fitted_snes_viewport_cell(available: egui::Vec2, zoom_percent: u16) -> f32 {
    const TILE_PIXELS: f32 = 16.0;
    const VIEWPORT_WIDTH: f32 = 256.0;
    const VIEWPORT_HEIGHT: f32 = 224.0;
    let horizontal_scale = available.x.max(1.0) / VIEWPORT_WIDTH;
    // `available` is measured after the toolbar and instruction row have been laid out, so it is
    // already the canvas pane's complete inner rectangle. Reserving caption space here would
    // count that row twice and leave an avoidable border, especially after maximizing the window.
    let vertical_scale = available.y.max(1.0) / VIEWPORT_HEIGHT;
    // Cover the complete responsive canvas. The non-matching edge is clipped symmetrically by
    // the viewport painter, preventing a narrow or short window from exposing unrendered space.
    let fitted_pixel_scale = horizontal_scale.max(vertical_scale).max(1.0 / TILE_PIXELS);
    let zoom_steps = f32::from(clamp_canvas_zoom(zoom_percent)) / 100.0;
    (fitted_pixel_scale * zoom_steps).max(1.0 / TILE_PIXELS) * TILE_PIXELS
}

fn live_frame_rect(canvas: egui::Rect, size: [usize; 2], cell: f32) -> egui::Rect {
    let pixels_per_source_pixel = cell / 16.0;
    egui::Rect::from_center_size(
        canvas.center(),
        egui::vec2(size[0] as f32, size[1] as f32) * pixels_per_source_pixel,
    )
}

fn selected_object_placements(
    placements: &[lm_level::NativeObjectPlacement],
    selected_group: &[usize],
    selected: usize,
) -> Vec<lm_level::NativeObjectPlacement> {
    placements
        .iter()
        .copied()
        .filter(|placement| {
            selected_group.contains(&placement.record_index)
                || selected_group.is_empty() && placement.record_index == selected
        })
        .collect()
}

fn game_preview_origin(
    entrance: VanillaMainEntrance,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
) -> (u16, u16) {
    let entrance_screen = u16::from(entrance.level_mode_and_screen & 0x1f);
    if vertical {
        let initial_y =
            u16::from(VANILLA_INITIAL_LAYER1_Y[usize::from((entrance.screen_and_method >> 2) & 3)])
                / 16;
        let y = entrance_screen
            .saturating_mul(16)
            .saturating_add(initial_y)
            .min(major_tiles.saturating_sub(14));
        (0, y)
    } else {
        let x = entrance_screen
            .saturating_mul(16)
            .min(major_tiles.saturating_sub(16));
        let y =
            u16::from(VANILLA_INITIAL_LAYER1_Y[usize::from((entrance.screen_and_method >> 2) & 3)])
                / 16;
        let y = y.min(minor_tiles.saturating_sub(14));
        (x, y)
    }
}

fn offset_game_preview_origin(
    entrance_origin: (u16, u16),
    major_offset: i16,
    minor_offset: i16,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
) -> (u16, u16) {
    let (x_offset, y_offset) = if vertical {
        (minor_offset, major_offset)
    } else {
        (major_offset, minor_offset)
    };
    let max_x = if vertical {
        minor_tiles.saturating_sub(16)
    } else {
        major_tiles.saturating_sub(16)
    };
    let max_y = if vertical {
        major_tiles.saturating_sub(14)
    } else {
        minor_tiles.saturating_sub(14)
    };
    (
        offset_camera_coordinate(entrance_origin.0, x_offset, max_x),
        offset_camera_coordinate(entrance_origin.1, y_offset, max_y),
    )
}

fn offset_camera_coordinate(origin: u16, offset: i16, maximum: u16) -> u16 {
    let clamped = i32::from(origin)
        .saturating_add(i32::from(offset))
        .clamp(0, i32::from(maximum));
    u16::try_from(clamped).expect("camera coordinate was clamped to a u16 bound")
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_camera_offset(axis: &str) -> i16 {
    std::env::var(format!("LM_NATIVE_PREVIEW_CAMERA_{axis}"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_scroll_row() -> Option<u16> {
    std::env::var("LM_NATIVE_EDITOR_SCROLL_ROW")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_scroll_column() -> Option<u16> {
    std::env::var("LM_NATIVE_EDITOR_SCROLL_COLUMN")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_scroll_column() -> Option<u16> {
    None
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_scroll_row() -> Option<u16> {
    None
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_overlays() -> bool {
    std::env::var("LM_NATIVE_EDITOR_OVERLAYS").map_or(true, |value| value != "0")
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_layer2() -> bool {
    std::env::var("LM_NATIVE_EDITOR_LAYER2").map_or(true, |value| value != "0")
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_layer1() -> bool {
    std::env::var("LM_NATIVE_EDITOR_LAYER1").map_or(true, |value| value != "0")
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_sprites() -> bool {
    std::env::var("LM_NATIVE_EDITOR_SPRITES").map_or(true, |value| value != "0")
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_sprite_limit() -> Option<usize> {
    std::env::var("LM_NATIVE_EDITOR_SPRITE_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_object_limit() -> Option<usize> {
    std::env::var("LM_NATIVE_EDITOR_OBJECT_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_object_limit() -> Option<usize> {
    None
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_layer1() -> bool {
    true
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_sprites() -> bool {
    true
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_sprite_limit() -> Option<usize> {
    None
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_layer2() -> bool {
    true
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_editor_cell() -> Option<f32> {
    std::env::var("LM_NATIVE_EDITOR_CELL")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| *value > 0.0 && value.is_finite())
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_cell() -> Option<f32> {
    None
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_editor_overlays() -> bool {
    true
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_camera_offset(_axis: &str) -> i16 {
    0
}

fn object_field_edits(
    form: &ObjectForm,
    index: usize,
    current: Option<&ObjectRecord>,
) -> Result<Vec<ObjectEdit>, String> {
    if let Some((screen, destination_and_flags)) = form.screen_exit {
        let mut record = current
            .cloned()
            .ok_or_else(|| "selected screen-exit object no longer exists".to_owned())?;
        record
            .set_screen_exit(screen, destination_and_flags)
            .map_err(|error| error.to_string())?;
        return Ok(vec![ObjectEdit::Replace { index, record }]);
    }
    if let Some((_, packed_target)) = form.screen_jump {
        return Ok(vec![ObjectEdit::SetScreenJumpTarget {
            index,
            packed_target,
        }]);
    }
    if let Some((horizontal, vertical)) = form.extended_command27_size {
        let mut record = current
            .cloned()
            .ok_or_else(|| "selected extended command $27 object no longer exists".to_owned())?;
        record
            .set_command_id(form.command_id)
            .and_then(|()| record.set_parameter(form.parameter))
            .and_then(|()| {
                record.set_coordinate_nibbles(ObjectCoordinateNibbles {
                    first: form.first_coordinate,
                    second: form.second_coordinate,
                })
            })
            .and_then(|()| record.set_advances_screen(form.advances_screen))
            .and_then(|()| record.set_extended_command27_tile_size(horizontal, vertical))
            .map_err(|error| error.to_string())?;
        return Ok(vec![ObjectEdit::Replace { index, record }]);
    }
    Ok(vec![
        ObjectEdit::SetCommandId {
            index,
            command_id: form.command_id,
        },
        ObjectEdit::SetParameter {
            index,
            parameter: form.parameter,
        },
        ObjectEdit::SetCoordinateNibbles {
            index,
            coordinates: ObjectCoordinateNibbles {
                first: form.first_coordinate,
                second: form.second_coordinate,
            },
        },
        ObjectEdit::SetAdvancesScreen {
            index,
            advances: form.advances_screen,
        },
    ])
}

#[cfg(test)]
fn selected_object_form(records: &[ObjectRecord], selected: usize) -> Option<ObjectForm> {
    records.get(selected).map(ObjectForm::from_record)
}

const fn screen_jump_components(
    encoding: lm_level::ScreenJumpEncoding,
    packed_target: u16,
) -> (u8, u8) {
    let [low, high] = packed_target.to_le_bytes();
    match encoding {
        lm_level::ScreenJumpEncoding::FirstLow => (low & 0x1f, high & 0x0f),
        lm_level::ScreenJumpEncoding::FirstHigh => (high & 0x1f, low & 0x0f),
    }
}

const fn pack_screen_jump_components(
    encoding: lm_level::ScreenJumpEncoding,
    first: u8,
    second: u8,
) -> u16 {
    match encoding {
        lm_level::ScreenJumpEncoding::FirstLow => u16::from_le_bytes([first & 0x1f, second & 0x0f]),
        lm_level::ScreenJumpEncoding::FirstHigh => {
            u16::from_le_bytes([second & 0x0f, first & 0x1f])
        }
    }
}

fn screen_jump_resolution_label(
    encoding: lm_level::ScreenJumpEncoding,
    packed_target: u16,
) -> String {
    let resolved = lm_level::ObjectScreenJump {
        encoding,
        packed_target,
    }
    .resolved_screen();
    if resolved <= 0x1f {
        format!(
            "Packed target: {packed_target:04X}. Resolved screen: {resolved:02X}. The original low/high ordering is preserved."
        )
    } else {
        format!(
            "Packed target: {packed_target:04X}. Resolved screen: {resolved:02X}, outside 00-1F; the raw value is retained losslessly but does not contribute to automatic extent."
        )
    }
}

fn is_supported(snapshot: &lm_app::ControllerSnapshot) -> bool {
    snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
}

fn validate_builtin_graphics_layout(snapshot: &lm_app::ControllerSnapshot) -> Result<(), String> {
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let project = lm_project::Project::new(image);
    for file in 0..0x32 {
        project
            .load_decompressed_graphics_file(file, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .map_err(|error| {
                format!(
                    "the built-in editor requires the pristine SMW-US graphics pointer layout, \
                     but GFX{file:02X} could not be decoded: {error}. Install a matching audited \
                     revision profile for this modified ROM"
                )
            })?;
    }
    let special_layouts =
        lm_profile::smw_us_v1_special_graphics_layouts(&project.rom).map_err(|error| {
            format!(
                "the built-in editor could not authenticate the SMW-US special-graphics startup \
                 layout: {error}. Install a matching audited revision profile for this modified ROM"
            )
        })?;
    for (name, layout) in [
        ("GFX33", special_layouts.gfx33),
        ("GFX32", special_layouts.gfx32),
    ] {
        project
            .load_decompressed_graphics_file(0, layout)
            .map_err(|error| {
                format!(
                    "the built-in editor resolved the SMW-US special-graphics startup layout, but \
                     {name} could not be decoded: {error}. Install a matching audited revision \
                     profile for this modified ROM"
                )
            })?;
    }
    Ok(())
}

fn prepare_commit(
    controller: &LevelController,
    snapshot: &lm_app::ControllerSnapshot,
) -> Result<Command, String> {
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let logical_len = image.logical_len();
    if logical_len <= 0x80_000 && controller.layer1_is_modified() {
        return Err("expand the ROM before committing level changes".into());
    }
    let fill_bytes = if logical_len > 0x80_000 {
        vec![0x00, 0xff]
    } else {
        vec![0xff]
    };
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let layer2_layout =
        lm_profile::smw_us_v1_layer2_layout(&image).map_err(|error| error.to_string())?;
    let allocation = AllocationPolicy {
        search: logical_len.min(0x80_000)..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: fill_bytes.clone(),
        protected: vec![
            ProtectedRange(
                layout.layer1.offset
                    ..layout.layer1.offset + layout.layer1.entries * layout.layer1.stride,
            ),
            ProtectedRange(
                snapshot.identity.internal_header_offset
                    ..snapshot.identity.internal_header_offset + 0x40,
            ),
            ProtectedRange(
                layer2_layout.pointers.offset
                    ..layer2_layout.pointers.offset
                        + layer2_layout.pointers.entries * layer2_layout.pointers.stride,
            ),
        ],
    };
    let sprite_bank = pristine_sprite_bank_range(&image, layout)?;
    let level_options = LevelSaveOptions {
        layer1_allocation: allocation.clone(),
        sprite_allocation: AllocationPolicy {
            search: sprite_bank,
            bank_size: Some(0x8000),
            fill_bytes,
            protected: allocation.protected.clone(),
        },
        previous_layer1: None,
        previous_sprites: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if controller.layer2_is_modified() {
        controller.prepare_commit_with_layer2(
            format!("Edit pristine SMW level {:03X}", controller.level().number),
            &level_options,
            &lm_project::LevelLayer2SaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            true,
        )
    } else {
        controller.prepare_commit_with_shared_bank_sprite_relocation(
            format!("Edit pristine SMW level {:03X}", controller.level().number),
            &level_options,
        )
    }
    .map(lm_app::PreparedRomCommit::into_command)
    .map_err(|error| error.to_string())
}

fn pristine_sprite_bank_range(
    image: &RomImage,
    layout: lm_project::LevelRomLayout,
) -> Result<std::ops::Range<usize>, String> {
    let lm_project::SpritePointerTable::SplitSharedBank { bank_offset, .. } = layout.sprites else {
        return Err("pristine sprite layout does not use a shared bank".into());
    };
    let bank = *image
        .logical_bytes()
        .get(bank_offset)
        .ok_or_else(|| "shared sprite bank byte lies outside the ROM".to_owned())?;
    let first = SnesPointer24::new((u32::from(bank) << 16) | 0x8000)
        .map_err(|error| error.to_string())?
        .to_pc(layout.mapper)
        .map_err(|error| error.to_string())?;
    let end = first
        .checked_add(0x8000)
        .ok_or_else(|| "shared sprite bank range overflows".to_owned())?;
    if end > image.logical_len() {
        return Err("shared sprite bank is not fully present in the ROM".into());
    }
    Ok(first..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_and_sprite_catalog_presentation_state_is_independent() {
        let mut editor = VanillaLevelEditor::default();
        assert!(editor.object_catalog_preview_icons.unwrap_or(true));
        assert!(!editor.object_catalog_compatible_only.unwrap_or(false));
        assert!(!editor.object_catalog_vertical_layout.unwrap_or(false));
        assert!(editor.object_catalog_preview_area.unwrap_or(true));
        assert_eq!(editor.object_catalog_preview_zoom.unwrap_or(100), 100);
        assert!(editor.sprite_catalog_preview_icons.unwrap_or(true));
        assert!(!editor.sprite_catalog_compatible_only.unwrap_or(false));
        assert!(!editor.sprite_catalog_vertical_layout.unwrap_or(false));
        assert!(editor.sprite_catalog_preview_area.unwrap_or(true));
        assert_eq!(editor.sprite_catalog_preview_zoom.unwrap_or(100), 100);

        editor.object_catalog_preview_icons = Some(false);
        editor.object_catalog_compatible_only = Some(true);
        editor.object_catalog_vertical_layout = Some(true);
        editor.object_catalog_preview_area = Some(false);
        editor.object_catalog_preview_zoom = Some(600);
        assert!(!editor.object_catalog_preview_icons.unwrap_or(true));
        assert!(editor.object_catalog_compatible_only.unwrap_or(false));
        assert!(editor.object_catalog_vertical_layout.unwrap_or(false));
        assert!(!editor.object_catalog_preview_area.unwrap_or(true));
        assert_eq!(editor.object_catalog_preview_zoom.unwrap_or(100), 600);
        assert!(editor.sprite_catalog_preview_icons.unwrap_or(true));
        assert!(!editor.sprite_catalog_compatible_only.unwrap_or(false));
        assert!(!editor.sprite_catalog_vertical_layout.unwrap_or(false));
        assert!(editor.sprite_catalog_preview_area.unwrap_or(true));
        assert_eq!(editor.sprite_catalog_preview_zoom.unwrap_or(100), 100);
    }

    #[test]
    fn catalog_preview_zoom_matches_recovered_presets_bounds_and_canvas_size() {
        assert_eq!(CATALOG_PREVIEW_ZOOM_MENU, [100, 200, 300, 400, 600, 800]);
        assert_eq!(change_catalog_preview_zoom(100, -100), 100);
        assert_eq!(change_catalog_preview_zoom(100, 100), 200);
        assert_eq!(change_catalog_preview_zoom(4_950, 100), 5_000);
        assert_eq!(change_catalog_preview_zoom(5_000, 100), 5_000);
        assert_eq!(catalog_preview_side(100), 256.0);
        assert_eq!(catalog_preview_side(200), 512.0);
        assert_eq!(catalog_preview_side(5_000), 12_800.0);
    }

    #[test]
    fn object_graphics_filters_are_gated_and_require_loaded_assets() {
        let commands = vec![0x01, 0x39];
        assert_eq!(
            filter_standard_object_catalog_for_graphics(
                commands.clone(),
                false,
                0,
                1,
                Some([0, 1, 2, 0x15]),
            ),
            commands
        );
        assert_eq!(
            filter_standard_object_catalog_for_graphics(commands.clone(), true, 0, 1, None,),
            commands
        );
        assert_eq!(
            filter_standard_object_catalog_for_graphics(
                commands,
                true,
                0,
                1,
                Some([0, 1, 2, 0x15]),
            ),
            vec![0x01]
        );

        let selectors = vec![0x04, 0x7f];
        assert_eq!(
            filter_extended_object_catalog_for_graphics(
                selectors.clone(),
                false,
                4,
                Some([0, 1, 2, 0x1a]),
            ),
            selectors
        );
        assert_eq!(
            filter_extended_object_catalog_for_graphics(selectors, true, 4, Some([0, 1, 2, 0x1a]),),
            vec![0x04]
        );
    }

    #[test]
    fn standard_sprite_graphics_filter_is_gated_and_requires_loaded_assets() {
        let ids = vec![0x00, 0x0d, 0x1b];
        assert_eq!(
            filter_standard_sprite_catalog_for_graphics(ids.clone(), false, 0, Some([0, 1, 2, 3])),
            ids
        );
        assert_eq!(
            filter_standard_sprite_catalog_for_graphics(ids.clone(), true, 0, None),
            ids
        );
        assert_eq!(
            filter_standard_sprite_catalog_for_graphics(ids, true, 0, Some([0, 1, 7, 2])),
            vec![0x00, 0x0d]
        );
    }

    #[test]
    fn staged_level_edit_rebases_across_expansion_then_commits_and_reopens() {
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(source).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = LevelController::decode(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let baseline = controller.level().layer1.objects.records.len();
        let record = controller.level().layer1.objects.records[1].clone();
        let sprite_baseline = controller.level().sprites.tokens.len();
        let sprite = controller.level().sprites.tokens[1].clone();
        controller
            .apply_edits(&[
                NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                    index: baseline,
                    record,
                }]),
                NativeLevelEdit::InsertSprite {
                    index: sprite_baseline,
                    token: sprite,
                },
            ])
            .unwrap();
        let expected = controller.level().clone();

        app.dispatch(Command::ExpandRom(RomExpansionCommand {
            expected_revision: snapshot.revision,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor {
            pending_expansion_commit: Some(controller),
            ..VanillaLevelEditor::default()
        };
        let commit = editor
            .take_pending_expansion_commit(&expanded)
            .unwrap()
            .expect("expanded ROM should produce the pending level commit");
        assert!(editor.pending_expansion_commit.is_none());
        app.dispatch(commit).unwrap();

        if let Ok(path) = std::env::var("LM_NATIVE_EXPANSION_ARTIFACT") {
            std::fs::write(path, app.project().unwrap().save_snapshot()).unwrap();
        }

        let reopened = app.controller_snapshot().unwrap();
        let reopened = LevelController::decode(
            &reopened,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        assert_eq!(reopened.level(), &expected);
        assert_eq!(reopened.level().layer1.objects.records.len(), baseline + 1);
        assert_eq!(reopened.level().sprites.tokens.len(), sprite_baseline + 1);
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x10_0000);
    }

    #[test]
    fn horizontal_sprite_preview_rect_wraps_to_the_following_screen() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(2048.0, 432.0));
        let part = egui::Rect::from_min_size(egui::pos2(1773.0, 453.0), egui::vec2(16.0, 16.0));
        assert_eq!(
            wrap_horizontal_sprite_preview_rect(part, canvas, 16.0, false).min,
            egui::pos2(2029.0, 21.0)
        );
        assert_eq!(
            wrap_horizontal_sprite_preview_rect(part, canvas, 16.0, true),
            part
        );
    }

    #[test]
    fn level_mode_confirmation_is_required_only_across_loaded_layer2_storage_classes() {
        assert!(mode_change_resets_layer2(2, 0, true));
        assert!(mode_change_resets_layer2(0, 1, true));
        assert!(!mode_change_resets_layer2(0, 10, true));
        assert!(!mode_change_resets_layer2(2, 3, true));
        assert!(!mode_change_resets_layer2(2, 0, false));
    }

    fn prepare_lunar_magic_restore_files(root: &std::path::Path, directory: &std::path::Path) {
        let restore = directory.join("sysLMRestore");
        std::fs::create_dir(&restore).unwrap();
        std::fs::copy(
            root.join("sysLMRestore/smwOrig.smc"),
            restore.join("smwOrig.smc"),
        )
        .unwrap();
        std::fs::copy(
            root.join("sysLMRestore/Super Mario World (USA).lrp"),
            restore.join("Super Mario World (USA).lrp"),
        )
        .unwrap();
    }

    fn export_level_with_lunar_magic(
        root: &std::path::Path,
        rom_path: &std::path::Path,
        mwl_path: &std::path::Path,
        level: u16,
    ) {
        let wine_path = |path: &std::path::Path| {
            let rendered = path.display().to_string().replace('/', r"\");
            format!(r"Z:\{}", rendered.trim_start_matches('\\'))
        };
        let executable = root.join("lm363/Lunar Magic.exe");
        assert!(executable.is_file(), "missing {}", executable.display());
        let output = std::process::Command::new("wine")
            .env("WINEDEBUG", "-all")
            .arg(executable)
            .arg("-ExportLevel")
            .arg(wine_path(rom_path))
            .arg(wine_path(mwl_path))
            .arg(format!("{level:03X}"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Lunar Magic export stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn resize_test_placement() -> lm_level::NativeObjectPlacement {
        lm_level::NativeObjectPlacement {
            record_index: 0,
            screen: 0,
            major: 10,
            minor: 3,
            major_span: 2,
            minor_span: 2,
        }
    }

    #[test]
    fn hidden_canvas_domains_reject_their_placement_tools() {
        let visibility = crate::application::LevelViewVisibility {
            layer1: false,
            layer2: true,
            layer3: false,
            sprites: false,
            tile_grid: false,
            surface_outline: false,
            line_guide_outline: false,
            screen_overlay: crate::application::LevelScreenOverlay::None,
        };
        assert!(!placement_mode_visible(
            CanvasPlacementMode::Object,
            visibility
        ));
        assert!(!placement_mode_visible(
            CanvasPlacementMode::Sprite,
            visibility
        ));
        assert!(placement_mode_visible(
            CanvasPlacementMode::Layer2Object,
            visibility
        ));
        assert!(placement_mode_visible(
            CanvasPlacementMode::Layer2Tile,
            visibility
        ));
    }

    #[test]
    fn authenticated_add_object_and_sprite_toolbar_routes_select_the_integrated_placer() {
        let mut editor = VanillaLevelEditor {
            tools_panel_visible: Some(false),
            error: Some("old error".into()),
            ..VanillaLevelEditor::default()
        };
        editor.toolbar_place_object();
        assert_eq!(editor.tools_panel_visible, Some(true));
        assert_eq!(editor.placement_mode, Some(CanvasPlacementMode::Object));
        assert_eq!(editor.error, None);

        editor.tools_panel_visible = Some(false);
        editor.error = Some("old error".into());
        editor.toolbar_place_sprite();
        assert_eq!(editor.tools_panel_visible, Some(true));
        assert_eq!(editor.placement_mode, Some(CanvasPlacementMode::Sprite));
        assert_eq!(editor.error, None);

        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.selected_sprite_group = vec![1, 2];
        editor.dragging_sprite = Some(1);
        editor.toolbar_escape();
        assert_eq!(editor.placement_mode, None);
        assert_eq!(editor.canvas_entity_selection, None);
        assert!(editor.selected_sprite_group.is_empty());
        assert_eq!(editor.dragging_sprite, None);
    }

    #[test]
    fn authenticated_level_editor_commands_reopen_the_matching_integrated_tool_panel() {
        let mut editor = VanillaLevelEditor {
            tools_panel_visible: Some(false),
            ..VanillaLevelEditor::default()
        };
        for (generation, panel) in [
            (1, LevelToolPanel::Layer2),
            (1, LevelToolPanel::Sprites),
            (1, LevelToolPanel::Settings),
            (1, LevelToolPanel::ScreenExits),
            (2, LevelToolPanel::Layer2),
        ] {
            editor.toolbar_open_tool_panel(panel);
            assert_eq!(editor.tools_panel_visible, Some(true));
            assert_eq!(editor.requested_tool_panel, Some(panel));
            assert_eq!(editor.tool_panel_generations[panel.index()], generation);
            editor.tools_panel_visible = Some(false);
        }
    }

    #[test]
    fn map16_screen_variants_follow_the_level_major_axis() {
        assert_eq!(map16_screen_variant(0, 63, false), 0);
        assert_eq!(map16_screen_variant(15, 63, false), 0);
        assert_eq!(map16_screen_variant(16, 63, false), 1);
        assert_eq!(map16_screen_variant(63, 0, false), 3);
        assert_eq!(map16_screen_variant(64, 0, false), 0);

        assert_eq!(map16_screen_variant(63, 0, true), 0);
        assert_eq!(map16_screen_variant(63, 15, true), 0);
        assert_eq!(map16_screen_variant(63, 16, true), 1);
        assert_eq!(map16_screen_variant(0, 63, true), 3);
        assert_eq!(map16_screen_variant(0, 64, true), 0);
    }

    #[test]
    fn translucent_map16_tint_is_underground_family_specific() {
        let translucent = egui::Color32::from_rgba_premultiplied(127, 127, 127, 128);
        assert_eq!(vanilla_map16_atlas_tint(4, 0x027), translucent);
        assert_eq!(vanilla_map16_atlas_tint(4, 0x02a), translucent);

        assert_eq!(vanilla_map16_atlas_tint(0, 0x028), egui::Color32::WHITE);
        assert_eq!(vanilla_map16_atlas_tint(4, 0x026), egui::Color32::WHITE);
        assert_eq!(vanilla_map16_atlas_tint(4, 0x02b), egui::Color32::WHITE);
    }

    #[test]
    fn canvas_resize_encodes_both_axes_in_horizontal_and_vertical_levels() {
        let record = ObjectRecord::new(vec![0x00, 0x10, 0x00]).unwrap();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 256.0));
        let horizontal = resized_standard_object_parameter_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::ParameterNibbles,
            egui::pos2(14.5 * 16.0, 7.5 * 16.0),
            canvas,
            16.0,
            false,
        )
        .unwrap();
        let vertical = resized_standard_object_parameter_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::ParameterNibbles,
            egui::pos2(7.5 * 16.0, 14.5 * 16.0),
            canvas,
            16.0,
            true,
        )
        .unwrap();
        assert_eq!(horizontal, 0x44);
        assert_eq!(vertical, 0x44);
    }

    #[test]
    fn canvas_resize_preserves_fixed_axis_bits_and_rejects_invalid_drags() {
        let record = ObjectRecord::new(vec![0x00, 0x10, 0xa3]).unwrap();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 256.0));
        let major = resized_standard_object_parameter_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::MajorNibble,
            egui::pos2(14.5 * 16.0, 2.5 * 16.0),
            canvas,
            16.0,
            false,
        )
        .unwrap();
        let minor = resized_standard_object_parameter_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::MinorNibble {
                fixed_major_tiles: 1,
            },
            egui::pos2(10.5 * 16.0, 7.5 * 16.0),
            canvas,
            16.0,
            false,
        )
        .unwrap();
        let minor_with_ignored_major_position =
            resized_standard_object_parameter_at_canvas_position(
                &record,
                resize_test_placement(),
                lm_render::StandardObjectResizeModel::MinorNibble {
                    fixed_major_tiles: 1,
                },
                egui::pos2(9.5 * 16.0, 7.5 * 16.0),
                canvas,
                16.0,
                false,
            )
            .unwrap();
        assert_eq!(major, 0x43);
        assert_eq!(minor, 0xa4);
        assert_eq!(minor_with_ignored_major_position, 0xa4);
        assert!(
            resized_standard_object_parameter_at_canvas_position(
                &record,
                resize_test_placement(),
                lm_render::StandardObjectResizeModel::MajorNibble,
                egui::pos2(9.5 * 16.0, 3.5 * 16.0),
                canvas,
                16.0,
                false,
            )
            .is_err()
        );
        assert!(
            resized_standard_object_parameter_at_canvas_position(
                &record,
                resize_test_placement(),
                lm_render::StandardObjectResizeModel::Fixed,
                egui::pos2(10.5 * 16.0, 3.5 * 16.0),
                canvas,
                16.0,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn extended_command27_canvas_resize_uses_physical_axes_in_both_level_orientations() {
        let record =
            ObjectRecord::new(vec![0x40, 0x70, 0x84, 0xc3, 0xaa, 0xbb, 0x06, 0xdd]).unwrap();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0));
        let horizontal = resized_standard_object_record_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
            egui::pos2(14.5 * 16.0, 8.5 * 16.0),
            canvas,
            16.0,
            false,
        )
        .unwrap();
        let vertical = resized_standard_object_record_at_canvas_position(
            &record,
            resize_test_placement(),
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
            egui::pos2(7.5 * 16.0, 15.5 * 16.0),
            canvas,
            16.0,
            true,
        )
        .unwrap();
        assert_eq!(horizontal.extended_command27_tile_size(), Some((5, 6)));
        assert_eq!(vertical.extended_command27_tile_size(), Some((5, 6)));
        for index in [3, 4, 5, 7] {
            assert_eq!(horizontal.encoded()[index], record.encoded()[index]);
            assert_eq!(vertical.encoded()[index], record.encoded()[index]);
        }
    }

    #[test]
    fn extended_command27_canvas_extent_and_handle_follow_physical_bounds() {
        let record = ObjectRecord::new(vec![0x40, 0x70, 29, 0xc0, 0, 0, 19]).unwrap();
        let placement = resize_test_placement();
        let records = [record.clone()];
        let placements = [placement];
        let models = HashMap::from([(
            0,
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
        )]);
        assert_eq!(
            extended_command27_canvas_extent(&records, &placements, &models, false),
            (40, 23)
        );
        assert_eq!(
            extended_command27_canvas_extent(&records, &placements, &models, true),
            (30, 32)
        );
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0));
        let horizontal = standard_object_resize_handle(
            canvas,
            placement,
            &record,
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
            false,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        let vertical = standard_object_resize_handle(
            canvas,
            placement,
            &record,
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
            true,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        assert_eq!(
            horizontal.center(),
            egui::pos2(40.0 * ROM_LEVEL_CANVAS_CELL, 23.0 * ROM_LEVEL_CANVAS_CELL)
        );
        assert_eq!(
            vertical.center(),
            egui::pos2(33.0 * ROM_LEVEL_CANVAS_CELL, 30.0 * ROM_LEVEL_CANVAS_CELL)
        );
    }

    #[test]
    fn resize_handles_follow_the_encoded_axis_for_level_orientation() {
        let record = ObjectRecord::new(vec![0x00, 0x10, 0xa3]).unwrap();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 256.0));
        let horizontal = standard_object_resize_handle(
            canvas,
            resize_test_placement(),
            &record,
            lm_render::StandardObjectResizeModel::MajorNibble,
            false,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        let vertical = standard_object_resize_handle(
            canvas,
            resize_test_placement(),
            &record,
            lm_render::StandardObjectResizeModel::MajorNibble,
            true,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        assert_eq!(
            horizontal.center(),
            egui::pos2(12.0 * ROM_LEVEL_CANVAS_CELL, 3.5 * ROM_LEVEL_CANVAS_CELL)
        );
        assert_eq!(
            vertical.center(),
            egui::pos2(3.5 * ROM_LEVEL_CANVAS_CELL, 12.0 * ROM_LEVEL_CANVAS_CELL)
        );
        assert!(
            standard_object_resize_handle(
                canvas,
                resize_test_placement(),
                &record,
                lm_render::StandardObjectResizeModel::Fixed,
                false,
                ROM_LEVEL_CANVAS_CELL,
            )
            .is_none()
        );
    }

    #[test]
    fn shared_pristine_backgrounds_require_copy_on_write_before_editing() {
        assert!(layer2_tilemap_editable(false));
        assert!(!layer2_tilemap_editable(true));
    }

    #[test]
    fn canvas_grid_is_translucent_and_emphasizes_screen_boundaries() {
        let ordinary = grid_line_stroke(1);
        let boundary = grid_line_stroke(16);

        assert!(ordinary.color.a() < boundary.color.a());
        assert!(ordinary.color.a() < 64);
        assert!(boundary.color.a() < 128);
        assert!(ordinary.width < boundary.width);
    }

    #[test]
    fn sprite_preview_definitions_use_native_column_major_quadrants() {
        assert_eq!(sprite_definition_quadrant_position(0), (0, 0));
        assert_eq!(sprite_definition_quadrant_position(1), (0, 1));
        assert_eq!(sprite_definition_quadrant_position(2), (1, 0));
        assert_eq!(sprite_definition_quadrant_position(3), (1, 1));
    }

    #[test]
    fn full_byte_minor_resize_handle_uses_all_256_lengths() {
        let record = ObjectRecord::new(vec![0x00, 0x10, 0x1f]).unwrap();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0));
        let model = lm_render::StandardObjectResizeModel::MinorByte {
            fixed_major_tiles: 3,
        };
        let horizontal = authenticated_standard_object_rect(
            canvas,
            resize_test_placement(),
            &record,
            model,
            false,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        let vertical = authenticated_standard_object_rect(
            canvas,
            resize_test_placement(),
            &record,
            model,
            true,
            ROM_LEVEL_CANVAS_CELL,
        )
        .unwrap();
        assert_eq!(
            horizontal.size(),
            egui::vec2(3.0 * ROM_LEVEL_CANVAS_CELL, 32.0 * ROM_LEVEL_CANVAS_CELL)
        );
        assert_eq!(
            vertical.size(),
            egui::vec2(32.0 * ROM_LEVEL_CANVAS_CELL, 3.0 * ROM_LEVEL_CANVAS_CELL)
        );
    }

    #[test]
    fn layer2_canvas_hit_testing_matches_native_two_plane_storage() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 512.0));
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(8.0, 8.0), canvas, 16.0),
            Some(0)
        );
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(24.0, 8.0), canvas, 16.0),
            Some(1)
        );
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(504.0, 504.0), canvas, 16.0),
            Some(1023)
        );
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(513.0, 8.0), canvas, 16.0),
            None
        );
        for index in [0, 15, 16, 511, 512, 527, 528, 1023] {
            let (x, y) = layer2_canvas_coordinates(index).unwrap();
            assert_eq!(lm_level::native_layer2_tilemap_index(x, y), Some(index));
        }
        assert_eq!(layer2_canvas_coordinates(1024), None);
    }

    #[test]
    fn game_pixel_mode_keeps_object_canvas_hit_testing_active_without_editor_overlays() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 432.0));
        let placement = resize_test_placement();
        let record = ObjectRecord::new(vec![0x00, 0x10, 0x00]).unwrap();
        let object_rect = encoded_object_rect(canvas, placement, false, ROM_LEVEL_CANVAS_CELL);
        let context = egui::Context::default();
        let hits = draw_object_placement_markers(
            &context.debug_painter(),
            Some(object_rect.center()),
            canvas,
            false,
            std::slice::from_ref(&record),
            std::slice::from_ref(&placement),
            &[],
            0,
            None,
            &HashMap::new(),
            &HashMap::new(),
            ROM_LEVEL_CANVAS_CELL,
            false,
            false,
        );
        assert_eq!(hits.body, Some(0));
        assert_eq!(hits.resize, None);
    }

    #[test]
    fn layer3_editor_plane_repeats_across_complete_scrolled_world() {
        assert_eq!(
            repeating_layer3_plane_origins(0, 1_280),
            vec![0, 512, 1_024]
        );
        assert_eq!(
            repeating_layer3_plane_origins(-64, 1_024),
            vec![-448, 64, 576]
        );
        assert_eq!(
            repeating_layer3_plane_origins(112, 1_024),
            vec![-112, 400, 912]
        );
        assert_eq!(layer3_plane_y_origins(-48, 432, false), vec![48]);
        assert_eq!(
            layer3_plane_y_origins(-48, 1_024, true),
            vec![-464, 48, 560]
        );
    }

    #[test]
    fn pristine_level_105_opens_its_shared_vanilla_background() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert!(editor.controller.is_some(), "{:?}", editor.error);
        let layer2 = editor.controller.as_ref().unwrap().layer2();
        assert!(matches!(
            layer2,
            Some(lm_level::NativeLayer2Data::Tilemap(bytes))
                if bytes.len() == lm_level::NATIVE_LAYER2_TILEMAP_LEN
        ));
        let model = editor.canvas_model();
        assert!(
            model
                .layer1_placements
                .iter()
                .any(|placement| placement.minor >= 16)
        );
        assert!(
            model
                .layer1_placements
                .iter()
                .all(|placement| u16::from(placement.minor) < NATIVE_LEVEL_MINOR_TILES)
        );
        assert_eq!(NATIVE_LEVEL_MINOR_TILES, 27);
        assert!(editor.error.is_none());
    }

    #[test]
    fn builtin_editor_rejects_a_modified_graphics_pointer_layout() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        validate_builtin_graphics_layout(&snapshot).unwrap();

        let mut modified = snapshot.clone();
        let image = RomImage::from_bytes(modified.rom_bytes.clone()).unwrap();
        let prefix_len = modified.rom_bytes.len() - image.logical_len();
        modified.rom_bytes[prefix_len + 0x3992] ^= 0xff;
        let error = validate_builtin_graphics_layout(&modified).unwrap_err();
        assert!(error.contains("pristine SMW-US graphics pointer layout"));
        assert!(error.contains("audited revision profile"));
    }

    #[test]
    fn builtin_editor_resolves_lunar_magic_per_level_sprite_banks() {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let pristine_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let low_offset = pristine_layout.sprites.low_or_contiguous_table().offset;
        let low_word = u16::from_le_bytes([bytes[low_offset], bytes[low_offset + 1]]);
        let installed_bank = 0x10_u8;
        let installed_pointer =
            SnesPointer24::new((u32::from(installed_bank) << 16) | u32::from(low_word)).unwrap();
        let installed_offset = installed_pointer.to_pc(Mapper::LoRom).unwrap();
        bytes.resize(0x10_0000, 0xff);
        assert!(installed_offset + 5 <= bytes.len());
        bytes[lm_profile::SMW_US_V1_LEVEL_SPRITE_POINTER_HOOK_OFFSET] = 0x22;
        bytes[lm_profile::SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_TABLE_OFFSET] = installed_bank;
        bytes[installed_offset..installed_offset + 5]
            .copy_from_slice(&[0x00, 0x60, 0x00, 0x47, 0xff]);

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let resolved = editor_level_layout(&snapshot, 0).unwrap();
        assert!(matches!(
            resolved.sprites,
            lm_project::SpritePointerTable::SplitBankTable { .. }
        ));

        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let placements = editor.canvas_model().sprite_placements;
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].sprite_number, 0x47);
        assert_eq!((placements[0].major, placements[0].minor), (0, 6));
    }

    #[test]
    fn builtin_editor_detects_sprite_framing_from_each_stream_header() {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let legacy_offset = layout
            .sprites
            .read_snes_pointer(&image, 0)
            .unwrap()
            .to_pc(Mapper::LoRom)
            .unwrap();
        let expanded_offset = layout
            .sprites
            .read_snes_pointer(&image, 1)
            .unwrap()
            .to_pc(Mapper::LoRom)
            .unwrap();
        assert_ne!(legacy_offset, expanded_offset);

        bytes[legacy_offset..legacy_offset + 5].copy_from_slice(&[0x00, 0x60, 0x00, 0x47, 0xff]);
        bytes[expanded_offset..expanded_offset + 8]
            .copy_from_slice(&[0x20, 0xff, 0x02, 0x60, 0x00, 0x47, 0xff, 0xfe]);

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();

        app.dispatch(Command::SelectLevel(0)).unwrap();
        let legacy_snapshot = app.controller_snapshot().unwrap();
        let legacy_layout = editor_level_layout(&legacy_snapshot, 0).unwrap();
        assert!(!legacy_layout.expanded_sprites);
        let legacy = LevelController::decode(
            &legacy_snapshot,
            legacy_layout,
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        assert!(!legacy.level().sprites.expanded);

        app.dispatch(Command::SelectLevel(1)).unwrap();
        let expanded_snapshot = app.controller_snapshot().unwrap();
        let expanded_layout = editor_level_layout(&expanded_snapshot, 1).unwrap();
        assert!(expanded_layout.expanded_sprites);
        let baseline = expanded_snapshot.rom_bytes.clone();
        let mut expanded = LevelController::decode(
            &expanded_snapshot,
            expanded_layout,
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        assert!(expanded.level().sprites.expanded);
        assert!(matches!(
            expanded.level().sprites.tokens[0],
            SpriteToken::Screen(2)
        ));
        expanded
            .apply_edits(&[NativeLevelEdit::RelocateExpandedSprite {
                selected: 1,
                screen: 0,
                x: 6,
                y: 3 * 32 + 5,
            }])
            .unwrap();
        app.dispatch(prepare_commit(&expanded, &expanded_snapshot).unwrap())
            .unwrap();

        let reopened_snapshot = app.controller_snapshot().unwrap();
        let reopened_layout = editor_level_layout(&reopened_snapshot, 1).unwrap();
        assert!(reopened_layout.expanded_sprites);
        let reopened = LevelController::decode(
            &reopened_snapshot,
            reopened_layout,
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let placement = reopened.level().sprites.native_placements()[0];
        assert_eq!((placement.major, placement.minor), (6, 101));
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.controller_snapshot().unwrap().rom_bytes, baseline);
    }

    #[test]
    fn expanded_zero_fill_is_available_for_layer1_growth() {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        bytes.resize(0x10_0000, 0x00);
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut controller =
            LevelController::decode(&snapshot, layout, &SpriteLengthTable::standard()).unwrap();
        let index = controller.level().layer1.objects.records.len();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index,
                record: ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
            }])])
            .unwrap();
        app.dispatch(prepare_commit(&controller, &snapshot).unwrap())
            .unwrap();
        let pointer = layout
            .layer1
            .read_snes_pointer(&app.project().unwrap().rom, 0x105)
            .unwrap();
        let offset = pointer.to_pc(Mapper::LoRom).unwrap();
        assert!(offset >= 0x80_008);
        assert_eq!(
            lm_rats::parse_at(app.project().unwrap().rom.logical_bytes(), offset - 8)
                .unwrap()
                .payload
                .start,
            offset
        );
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic-modified SMW-US ROM"]
    fn external_lunar_magic_rom_sprite_edit_saves_reopens_and_undoes() {
        let path = std::env::var_os("LM_MODIFIED_LEVEL_ROM")
            .expect("LM_MODIFIED_LEVEL_ROM must name the modified ROM");
        let source = std::fs::read(path).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::SelectLevel(0x102)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let baseline = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let baseline_image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        let mut baseline_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        baseline_layout.sprites =
            lm_profile::smw_us_v1_sprite_pointer_table(&baseline_image).unwrap();
        let baseline_pointer = baseline_layout
            .sprites
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let baseline_layer1_pointer = baseline_layout
            .layer1
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let neighboring_pointer = baseline_layout
            .sprites
            .read_snes_pointer(&baseline_image, 0x101)
            .unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x102,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let controller = editor.controller.as_ref().unwrap();
        let baseline_sprites = controller.level().sprites.clone();
        let index = controller
            .level()
            .sprites
            .tokens
            .iter()
            .enumerate()
            .find_map(|(index, token)| match token {
                lm_level::SpriteToken::Record(_) => Some(index),
                _ => None,
            })
            .expect("level 102 must contain an ordinary sprite");
        editor.selected_sprite = index;
        editor.sprite_form = SpriteForm::from_token(
            controller.level().sprites.header,
            controller.level().sprites.tokens.get(index),
        );
        editor.sprite_form.screen = editor.sprite_form.screen.saturating_add(1).min(0x1f);
        editor.apply_sprite_semantic_fields();
        assert_eq!(editor.error, None);
        let sorted_index = editor.selected_sprite;
        assert_ne!(sorted_index, index);
        let original_token_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .tokens
            .len();
        editor.sprite_form.sprite_number ^= 0x01;
        editor.placement_mode = Some(CanvasPlacementMode::Sprite);
        editor.place_sprite_at_canvas(
            egui::pos2(36.5, 8.5),
            egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
            ),
            1.0,
            false,
        );
        assert_eq!(editor.error, None);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .tokens
                .len(),
            original_token_count + 1
        );
        let expected_sprites = editor.controller.as_ref().unwrap().level().sprites.clone();
        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let image = app.project().unwrap().rom.clone();
        let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
        layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        assert_ne!(
            layout.sprites.read_snes_pointer(&image, 0x102).unwrap(),
            baseline_pointer
        );
        assert_eq!(
            layout.layer1.read_snes_pointer(&image, 0x102).unwrap(),
            baseline_layer1_pointer
        );
        assert_eq!(
            layout.sprites.read_snes_pointer(&image, 0x101).unwrap(),
            neighboring_pointer
        );
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(0x102, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(reopened.sprites, expected_sprites);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let temporary = tempfile::Builder::new()
            .prefix("lm-modified-level-save-wine-")
            .tempdir()
            .unwrap();
        let directory = temporary.path();
        let baseline_rom_path = directory.join("baseline modified ROM.smc");
        let baseline_mwl_path = directory.join("baseline level 102.mwl");
        let rom_path = directory.join("rust saved modified ROM.smc");
        let mwl_path = directory.join("rust saved level 102.mwl");
        std::fs::write(&baseline_rom_path, &source).unwrap();
        std::fs::write(&rom_path, app.project().unwrap().rom.as_file_bytes()).unwrap();
        prepare_lunar_magic_restore_files(&root, directory);
        export_level_with_lunar_magic(&root, &baseline_rom_path, &baseline_mwl_path, 0x102);
        export_level_with_lunar_magic(&root, &rom_path, &mwl_path, 0x102);
        let baseline_exported = lm_project::MwlNativeLevel::decode(
            &lm_level::MwlFile::decode(&std::fs::read(&baseline_mwl_path).unwrap()).unwrap(),
            &SpriteLengthTable::standard(),
            32,
            &[false; 256],
        )
        .unwrap();
        let exported = lm_project::MwlNativeLevel::decode(
            &lm_level::MwlFile::decode(&std::fs::read(&mwl_path).unwrap()).unwrap(),
            &SpriteLengthTable::standard(),
            32,
            &[false; 256],
        )
        .unwrap();
        assert_eq!(exported.layer1, baseline_exported.layer1);
        assert_eq!(baseline_exported.sprites, baseline_sprites);
        assert_eq!(exported.sprites, expected_sprites);
        assert!(
            lm_rom::detect_identity(
                &RomImage::from_bytes(std::fs::read(&rom_path).unwrap()).unwrap()
            )
            .unwrap()
            .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), baseline);
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic-modified SMW-US ROM"]
    fn external_lunar_magic_rom_object_insertion_grows_only_layer1_and_undoes() {
        let path = std::env::var_os("LM_MODIFIED_LEVEL_ROM")
            .expect("LM_MODIFIED_LEVEL_ROM must name the modified ROM");
        let source = std::fs::read(path).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::SelectLevel(0x102)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let baseline = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let baseline_image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        let mut baseline_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        baseline_layout.sprites =
            lm_profile::smw_us_v1_sprite_pointer_table(&baseline_image).unwrap();
        let baseline_layer1_pointer = baseline_layout
            .layer1
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let neighboring_layer1_pointer = baseline_layout
            .layer1
            .read_snes_pointer(&baseline_image, 0x101)
            .unwrap();
        let baseline_sprite_pointer = baseline_layout
            .sprites
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x102,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert_eq!(editor.error, None);
        let placement = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .next()
            .expect("level 102 must contain an ordinary object");
        let record = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records[placement.record_index]
            .clone();
        editor.selected_object = placement.record_index;
        editor.object_form = ObjectForm::from_record(&record);
        editor.object_placement_template = Some(record);
        let inserted_template = editor.object_record_for_placement().unwrap();
        let original_record_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records
            .len();
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        editor.placement_mode = Some(CanvasPlacementMode::Object);
        editor.place_object_at_canvas(egui::pos2(36.5, 8.5), canvas, 1.0, false);
        assert_eq!(editor.error, None);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records
                .len(),
            original_record_count + 1
        );
        let extended_template = ObjectRecord::new(vec![0, 0, 0x10]).unwrap();
        editor.object_form = ObjectForm::from_record(&extended_template);
        editor.object_placement_template = Some(extended_template.clone());
        let extended_position = egui::pos2(52.5, 10.5);
        let (extended_screen, extended_coordinates, extended_perpendicular_high) =
            object_placement_at_canvas_position(extended_position, canvas, 1.0, false).unwrap();
        editor.placement_mode = Some(CanvasPlacementMode::Object);
        editor.place_object_at_canvas(extended_position, canvas, 1.0, false);
        assert_eq!(editor.error, None);
        assert_eq!(editor.object_form.command_id, 0);
        assert_eq!(editor.object_form.parameter, 0x10);
        editor.object_form.parameter = 0x11;
        let extended_edits = editor.selected_object_field_edits().unwrap();
        editor
            .controller
            .as_mut()
            .unwrap()
            .apply_edits(&[NativeLevelEdit::Objects(extended_edits)])
            .unwrap();
        editor.reload_object_form();
        assert_eq!(editor.object_form.parameter, 0x11);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records
                .len(),
            original_record_count + 2
        );
        let expected_rust_layer1 = editor.controller.as_ref().unwrap().level().layer1.clone();
        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();

        let image = app.project().unwrap().rom.clone();
        let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
        layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        assert_ne!(
            layout.layer1.read_snes_pointer(&image, 0x102).unwrap(),
            baseline_layer1_pointer
        );
        assert_eq!(
            layout.layer1.read_snes_pointer(&image, 0x101).unwrap(),
            neighboring_layer1_pointer
        );
        assert_eq!(
            layout.sprites.read_snes_pointer(&image, 0x102).unwrap(),
            baseline_sprite_pointer
        );
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(0x102, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(reopened.layer1, expected_rust_layer1);

        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let temporary = tempfile::Builder::new()
            .prefix("lm-modified-level-object-growth-wine-")
            .tempdir()
            .unwrap();
        let directory = temporary.path();
        let baseline_rom_path = directory.join("baseline modified ROM.smc");
        let baseline_mwl_path = directory.join("baseline level 102.mwl");
        let rom_path = directory.join("rust object-grown modified ROM.smc");
        let mwl_path = directory.join("rust object-grown level 102.mwl");
        std::fs::write(&baseline_rom_path, &source).unwrap();
        std::fs::write(&rom_path, app.project().unwrap().rom.as_file_bytes()).unwrap();
        prepare_lunar_magic_restore_files(&root, directory);
        export_level_with_lunar_magic(&root, &baseline_rom_path, &baseline_mwl_path, 0x102);
        export_level_with_lunar_magic(&root, &rom_path, &mwl_path, 0x102);
        let baseline_exported = lm_project::MwlNativeLevel::decode(
            &lm_level::MwlFile::decode(&std::fs::read(&baseline_mwl_path).unwrap()).unwrap(),
            &SpriteLengthTable::standard(),
            32,
            &[false; 256],
        )
        .unwrap();
        let exported = lm_project::MwlNativeLevel::decode(
            &lm_level::MwlFile::decode(&std::fs::read(&mwl_path).unwrap()).unwrap(),
            &SpriteLengthTable::standard(),
            32,
            &[false; 256],
        )
        .unwrap();
        let mut expected_exported_layer1 = baseline_exported.layer1.clone();
        expected_exported_layer1
            .objects
            .insert_ordinary_object_at_position(
                inserted_template,
                2,
                ObjectCoordinateNibbles {
                    first: 8,
                    second: 4,
                },
                false,
            )
            .unwrap();
        let mut expected_extended = extended_template;
        expected_extended.set_parameter(0x11).unwrap();
        expected_exported_layer1
            .objects
            .insert_ordinary_object_at_position(
                expected_extended,
                extended_screen,
                extended_coordinates,
                extended_perpendicular_high,
            )
            .unwrap();
        assert_eq!(exported.layer1, expected_exported_layer1);
        assert_eq!(exported.sprites, baseline_exported.sprites);
        assert!(
            lm_rom::detect_identity(
                &RomImage::from_bytes(std::fs::read(&rom_path).unwrap()).unwrap()
            )
            .unwrap()
            .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), baseline);
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic-modified SMW-US ROM"]
    #[allow(
        clippy::too_many_lines,
        reason = "one external gate retains the complete object move/delete and Lunar Magic oracle boundary"
    )]
    fn external_lunar_magic_rom_object_move_and_delete_round_trip_exactly() {
        let path = std::env::var_os("LM_MODIFIED_LEVEL_ROM")
            .expect("LM_MODIFIED_LEVEL_ROM must name the modified ROM");
        let source = std::fs::read(path).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::SelectLevel(0x102)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let baseline = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let baseline_image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        let mut baseline_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        baseline_layout.sprites =
            lm_profile::smw_us_v1_sprite_pointer_table(&baseline_image).unwrap();
        let baseline_layer1_pointer = baseline_layout
            .layer1
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let neighboring_layer1_pointer = baseline_layout
            .layer1
            .read_snes_pointer(&baseline_image, 0x101)
            .unwrap();
        let baseline_sprite_pointer = baseline_layout
            .sprites
            .read_snes_pointer(&baseline_image, 0x102)
            .unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x102,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert_eq!(editor.error, None);
        let ordinary_indexes = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .filter(|placement| {
                editor
                    .controller
                    .as_ref()
                    .unwrap()
                    .level()
                    .layer1
                    .objects
                    .records[placement.record_index]
                    .command_id()
                    != 0
            })
            .map(|placement| placement.record_index)
            .collect::<Vec<_>>();
        assert!(
            ordinary_indexes.len() >= 2,
            "level 102 must contain two movable ordinary objects"
        );
        let moved_original_index = ordinary_indexes[0];
        let vertical = editor.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        let target_position = egui::pos2(52.5, 9.5);
        let (target_screen, target_coordinates, target_perpendicular_high) =
            object_placement_at_canvas_position(target_position, canvas, 1.0, vertical).unwrap();
        editor.move_object_to_canvas(moved_original_index, target_position, canvas, 1.0, vertical);
        assert_eq!(editor.error, None);
        let moved_index = editor.selected_object;
        let deleted_index = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .map(|placement| placement.record_index)
            .find(|&index| {
                index != moved_index
                    && editor
                        .controller
                        .as_ref()
                        .unwrap()
                        .level()
                        .layer1
                        .objects
                        .records[index]
                        .command_id()
                        != 0
            })
            .expect("a second ordinary object must survive relocation");
        editor.selected_object = deleted_index;
        editor.reload_object_form();
        editor.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
            index: deleted_index,
        }])));
        assert_eq!(editor.error, None);
        let expected_rust_layer1 = editor.controller.as_ref().unwrap().level().layer1.clone();
        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();

        let image = app.project().unwrap().rom.clone();
        let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
        layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        let relocated_layer1_pointer = layout.layer1.read_snes_pointer(&image, 0x102).unwrap();
        assert_ne!(relocated_layer1_pointer, baseline_layer1_pointer);
        let relocated_layer1_offset = relocated_layer1_pointer.to_pc(layout.mapper).unwrap();
        assert!(
            lm_rats::parse_at(
                image.logical_bytes(),
                relocated_layer1_offset - lm_rats::HEADER_LEN
            )
            .is_ok()
        );
        assert_eq!(
            layout.layer1.read_snes_pointer(&image, 0x101).unwrap(),
            neighboring_layer1_pointer
        );
        assert_eq!(
            layout.sprites.read_snes_pointer(&image, 0x102).unwrap(),
            baseline_sprite_pointer
        );
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(0x102, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(reopened.layer1, expected_rust_layer1);

        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let temporary = tempfile::Builder::new()
            .prefix("lm-modified-level-object-lifecycle-wine-")
            .tempdir()
            .unwrap();
        let directory = temporary.path();
        let baseline_rom_path = directory.join("baseline modified ROM.smc");
        let baseline_mwl_path = directory.join("baseline object lifecycle.mwl");
        let rom_path = directory.join("rust object lifecycle ROM.smc");
        let mwl_path = directory.join("rust object lifecycle.mwl");
        std::fs::write(&baseline_rom_path, &source).unwrap();
        std::fs::write(&rom_path, app.project().unwrap().rom.as_file_bytes()).unwrap();
        prepare_lunar_magic_restore_files(&root, directory);
        export_level_with_lunar_magic(&root, &baseline_rom_path, &baseline_mwl_path, 0x102);
        export_level_with_lunar_magic(&root, &rom_path, &mwl_path, 0x102);
        let decode_export = |path: &std::path::Path| {
            lm_project::MwlNativeLevel::decode(
                &lm_level::MwlFile::decode(&std::fs::read(path).unwrap()).unwrap(),
                &SpriteLengthTable::standard(),
                32,
                &[false; 256],
            )
            .unwrap()
        };
        let baseline_exported = decode_export(&baseline_mwl_path);
        let exported = decode_export(&mwl_path);
        let mut expected_exported_layer1 = baseline_exported.layer1.clone();
        let expected_moved_index = expected_exported_layer1
            .objects
            .relocate_ordinary_object_position(
                moved_original_index,
                target_screen,
                target_coordinates,
                target_perpendicular_high,
            )
            .unwrap();
        assert_eq!(expected_moved_index, moved_index);
        expected_exported_layer1
            .objects
            .apply_edits(&[ObjectEdit::Remove {
                index: deleted_index,
            }])
            .unwrap();
        assert_eq!(exported.layer1, expected_exported_layer1);
        assert_eq!(exported.layer2, baseline_exported.layer2);
        assert_eq!(exported.sprites, baseline_exported.sprites);
        assert!(
            lm_rom::detect_identity(
                &RomImage::from_bytes(std::fs::read(&rom_path).unwrap()).unwrap()
            )
            .unwrap()
            .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), baseline);
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic-modified SMW-US ROM"]
    #[allow(
        clippy::too_many_lines,
        reason = "one external gate retains the complete Layer 2 save and Lunar Magic oracle boundary"
    )]
    fn external_lunar_magic_rom_layer2_object_insertion_is_isolated_and_undoes() {
        let path = std::env::var_os("LM_MODIFIED_LEVEL_ROM")
            .expect("LM_MODIFIED_LEVEL_ROM must name the modified ROM");
        let source = std::fs::read(path).unwrap();
        let source_image = RomImage::from_bytes(source.clone()).unwrap();
        let source_project = lm_project::Project::new(source_image.clone());
        let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let source_layer2_layout = lm_profile::smw_us_v1_layer2_layout(&source_image).unwrap();
        let lengths = SpriteLengthTable::standard();
        let (level, template) = (0..0x200)
            .find_map(|level| {
                let slot = source_project
                    .load_level_slot(level, level_layout, &lengths)
                    .ok()?;
                let layer2 = source_project
                    .load_level_layer2(level, slot.layer1.header.level_mode(), source_layer2_layout)
                    .ok()?;
                let lm_level::NativeLayer2Data::Objects(objects) = layer2 else {
                    return None;
                };
                let template = objects
                    .objects
                    .records
                    .iter()
                    .find(|record| record.command_id() != 0)?
                    .clone();
                Some((u16::try_from(level).ok()?, template))
            })
            .expect("modified ROM must retain an object-backed Layer 2 level");
        let level_index = usize::from(level);
        let baseline_layer2_pointer = source_layer2_layout
            .pointers
            .read_snes_pointer(&source_image, level_index)
            .unwrap();
        let neighboring_level = if level_index == 0 { 1 } else { level_index - 1 };
        let neighboring_layer2_pointer = source_layer2_layout
            .pointers
            .read_snes_pointer(&source_image, neighboring_level)
            .unwrap();
        let baseline_layer1_pointer = level_layout
            .layer1
            .read_snes_pointer(&source_image, level_index)
            .unwrap();
        let baseline_sprite_pointer = level_layout
            .sprites
            .read_snes_pointer(&source_image, level_index)
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::SelectLevel(level)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let baseline = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert_eq!(editor.error, None);
        editor.layer2_object_form = ObjectForm::from_record(&template);
        editor.layer2_object_placement_template = Some(template.clone());
        let vertical = editor.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        let (inserted_screen, inserted_coordinates, inserted_perpendicular_high) =
            object_placement_at_canvas_position(egui::pos2(36.5, 8.5), canvas, 1.0, vertical)
                .unwrap();
        editor.placement_mode = Some(CanvasPlacementMode::Layer2Object);
        editor.place_layer2_object_at_canvas(egui::pos2(36.5, 8.5), canvas, 1.0, vertical);
        assert_eq!(editor.error, None);
        let expected_layer2 = editor
            .controller
            .as_ref()
            .unwrap()
            .layer2()
            .unwrap()
            .clone();
        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();

        let image = app.project().unwrap().rom.clone();
        let layer2_layout = lm_profile::smw_us_v1_layer2_layout(&image).unwrap();
        let relocated_layer2_pointer = layer2_layout
            .pointers
            .read_snes_pointer(&image, level_index)
            .unwrap();
        assert_ne!(relocated_layer2_pointer, baseline_layer2_pointer);
        let relocated_layer2_offset = relocated_layer2_pointer
            .to_pc(layer2_layout.mapper)
            .unwrap();
        assert_eq!(
            lm_rats::parse_at(
                image.logical_bytes(),
                relocated_layer2_offset - lm_rats::HEADER_LEN
            )
            .unwrap()
            .payload
            .start,
            relocated_layer2_offset
        );
        assert_eq!(
            layer2_layout
                .pointers
                .read_snes_pointer(&image, neighboring_level)
                .unwrap(),
            neighboring_layer2_pointer
        );
        assert_eq!(
            level_layout
                .layer1
                .read_snes_pointer(&image, level_index)
                .unwrap(),
            baseline_layer1_pointer
        );
        assert_eq!(
            level_layout
                .sprites
                .read_snes_pointer(&image, level_index)
                .unwrap(),
            baseline_sprite_pointer
        );
        let reopened_slot = app
            .project()
            .unwrap()
            .load_level_slot(level_index, level_layout, &lengths)
            .unwrap();
        let reopened_layer2 = app
            .project()
            .unwrap()
            .load_level_layer2(
                level_index,
                reopened_slot.layer1.header.level_mode(),
                layer2_layout,
            )
            .unwrap();
        assert_eq!(reopened_layer2, expected_layer2);

        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let temporary = tempfile::Builder::new()
            .prefix("lm-modified-level-layer2-growth-wine-")
            .tempdir()
            .unwrap();
        let directory = temporary.path();
        let baseline_rom_path = directory.join("baseline modified ROM.smc");
        let baseline_mwl_path = directory.join("baseline object Layer 2.mwl");
        let rom_path = directory.join("rust object Layer 2 ROM.smc");
        let mwl_path = directory.join("rust object Layer 2.mwl");
        std::fs::write(&baseline_rom_path, &source).unwrap();
        std::fs::write(&rom_path, app.project().unwrap().rom.as_file_bytes()).unwrap();
        prepare_lunar_magic_restore_files(&root, directory);
        export_level_with_lunar_magic(&root, &baseline_rom_path, &baseline_mwl_path, level);
        export_level_with_lunar_magic(&root, &rom_path, &mwl_path, level);
        let decode_export = |path: &std::path::Path| {
            lm_project::MwlNativeLevel::decode(
                &lm_level::MwlFile::decode(&std::fs::read(path).unwrap()).unwrap(),
                &lengths,
                32,
                &[false; 256],
            )
            .unwrap()
        };
        let baseline_exported = decode_export(&baseline_mwl_path);
        let exported = decode_export(&mwl_path);
        let mut expected_exported_layer2 = baseline_exported.layer2.clone();
        let lm_level::NativeLayer2Data::Objects(expected_objects) = &mut expected_exported_layer2
        else {
            panic!("selected oracle level must export object-backed Layer 2");
        };
        expected_objects
            .objects
            .insert_ordinary_object_at_position(
                template,
                inserted_screen,
                inserted_coordinates,
                inserted_perpendicular_high,
            )
            .unwrap();
        assert_eq!(exported.layer1, baseline_exported.layer1);
        assert_eq!(exported.sprites, baseline_exported.sprites);
        assert_eq!(exported.layer2, expected_exported_layer2);
        assert!(
            lm_rom::detect_identity(
                &RomImage::from_bytes(std::fs::read(&rom_path).unwrap()).unwrap()
            )
            .unwrap()
            .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), baseline);
    }

    #[test]
    fn every_pristine_level_materializes_its_builtin_render_assets() {
        let bytes = std::sync::Arc::new(crate::test_support::pristine_smw_us_rom_bytes());
        std::thread::scope(|scope| {
            for worker in 0_u16..8 {
                let bytes = bytes.clone();
                scope.spawn(move || {
                    let mut app = AppState::default();
                    app.load_rom(bytes.as_ref().clone()).unwrap();
                    for level in (worker..0x200).step_by(8) {
                        app.dispatch(Command::SelectLevel(level)).unwrap();
                        let snapshot = app.controller_snapshot().unwrap();
                        let layer2_layout = editor_layer2_layout(&snapshot, level).unwrap();
                        let controller = LevelController::decode_with_layer2(
                            &snapshot,
                            lm_profile::smw_us_v1_vanilla_level_layout(),
                            layer2_layout,
                            &lm_level::SpriteLengthTable::standard(),
                        )
                        .unwrap_or_else(|error| panic!("level ${level:03X} model failed: {error}"));
                        crate::vanilla_map16_preview::render(
                            snapshot.rom_bytes,
                            level,
                            controller.level().layer1.header,
                            false,
                            false,
                        )
                        .unwrap_or_else(|error| {
                            panic!("level ${level:03X} assets failed: {error}")
                        });
                    }
                });
            }
        });
    }

    #[test]
    fn diagnostic_write_pristine_512_renderer_manifest_when_requested() {
        use std::fmt::Write as _;

        let Ok(output) = std::env::var("LM_PRISTINE_512_RENDER_MANIFEST") else {
            return;
        };
        fn fnv1a(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
        }

        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let lengths = SpriteLengthTable::standard();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut manifest = String::from(
            "slot\tnative_renderable\tmode\tvertical\tobject_tileset\tsprite_tileset\tobjects\tobject_cells\tobject_hash\tsprites\tresolved_sprites\tnative_empty_sprites\tunresolved_sprites\tsprite_ids\n",
        );
        let mut native_renderable = 0_usize;

        for slot in 0_u16..0x200 {
            let level = project
                .load_level_slot(usize::from(slot), layout, &lengths)
                .unwrap_or_else(|error| panic!("level ${slot:03X} failed: {error}"));
            let header = level.layer1.header;
            let mode = lm_profile::smw_us_v1_level_mode(header.level_mode());
            let vertical = mode.vertical;
            let family = match lm_profile::smw_us_v1_object_family(header.object_tileset()) {
                lm_profile::VanillaObjectFamily::Normal => 0,
                lm_profile::VanillaObjectFamily::Castle => 1,
                lm_profile::VanillaObjectFamily::Rope => 2,
                lm_profile::VanillaObjectFamily::Underground => 3,
                lm_profile::VanillaObjectFamily::GhostHouse => 4,
            };
            let object_layout = lm_render::NativeLevelMap16Layout {
                width: if vertical { 27 } else { 512 },
                height: if vertical { 512 } else { 27 },
                page_stride: 0x1b0,
                base_cell: 0,
                vertical,
            };
            let report = lm_render::render_mapped_standard_object_stream(
                &level.layer1.objects,
                &definitions,
                definition_map.family(family).unwrap(),
                object_layout,
                VANILLA_EMPTY_MAP16_TILE,
            )
            .unwrap_or_else(|error| panic!("level ${slot:03X} objects failed: {error}"));
            assert!(report.missing_commands.is_empty(), "level ${slot:03X}");
            assert!(
                report.missing_extended_objects.is_empty(),
                "level ${slot:03X}"
            );

            let placements = level.sprites.native_placements();
            let mut resolved = 0_usize;
            let mut native_empty = 0_usize;
            let mut unresolved = 0_usize;
            let mut ids = std::collections::BTreeSet::new();
            let mut sequence_8a = 0_u8;
            for placement in &placements {
                ids.insert(placement.sprite_number);
                let preview = lm_render::render_lunar_magic_standard_sprite_with_mode(
                    placement.sprite_number,
                    standard_sprite_preview_mode(
                        placement,
                        vertical,
                        header.level_mode(),
                        header.sprite_tileset(),
                        level.sprites.header & 0x3f,
                        0,
                        sequence_8a,
                    ),
                );
                if placement.sprite_number == 0x8a {
                    sequence_8a = sequence_8a.saturating_add(1);
                }
                if preview.is_some() {
                    resolved += 1;
                } else if lm_render::lunar_magic_standard_sprite_preview_source(
                    placement.sprite_number,
                ) == lm_render::StandardSpritePreviewSource::NativeEmpty
                {
                    native_empty += 1;
                } else {
                    unresolved += 1;
                }
            }
            let id_text = ids
                .iter()
                .map(|id| format!("{id:02X}"))
                .collect::<Vec<_>>()
                .join(",");
            let renders_in_lunar_magic = !level.layer1.objects.records.is_empty();
            native_renderable += usize::from(renders_in_lunar_magic);
            writeln!(
                manifest,
                "{slot:03X}\t{}\t{:02X}\t{}\t{:02X}\t{:02X}\t{}\t{}\t{:016X}\t{}\t{}\t{}\t{}\t{}",
                u8::from(renders_in_lunar_magic),
                header.level_mode(),
                u8::from(vertical),
                header.object_tileset(),
                header.sprite_tileset(),
                level.layer1.objects.records.len(),
                report.painted_cells.len(),
                fnv1a(&report.cache.encode()),
                placements.len(),
                resolved,
                native_empty,
                unresolved,
                id_text,
            )
            .unwrap();
        }
        assert_eq!(native_renderable, 488);
        std::fs::write(output, manifest).unwrap();
    }

    #[test]
    fn window_workspace_reserves_the_majority_for_the_canvas() {
        for width in [720.0_f32, 1_100.0, 1_600.0, 3_200.0] {
            let tools = workspace_tool_width(width);
            assert!(tools >= 280.0);
            assert!(tools <= ROM_LEVEL_TOOL_PANEL_WIDTH);
            assert!(width - tools > width * 0.50);
        }
    }

    #[test]
    fn level_tools_default_to_the_exact_game_viewport_and_can_yield_the_complete_workspace() {
        let mut editor = VanillaLevelEditor::default();
        assert!(editor.tools_panel_visible());
        assert!(editor.game_preview());
        assert!(editor.snes_viewport());
        editor.tools_panel_visible = Some(false);
        assert!(!editor.tools_panel_visible());
        editor.tools_panel_visible = Some(true);
        assert!(editor.tools_panel_visible());
        editor.snes_viewport = Some(false);
        assert!(!editor.snes_viewport());
    }

    #[test]
    fn entrance_scroll_is_clamped_to_the_measured_canvas_viewport() {
        for (requested, content, viewport, expected) in [
            (256.0, 432.0, 224.0, 208.0),
            (128.0, 432.0, 224.0, 128.0),
            (256.0, 432.0, 720.0, 0.0),
        ] {
            assert!(
                (clamped_scroll_offset(requested, content, viewport) - expected).abs()
                    < f32::EPSILON
            );
        }
    }

    #[test]
    fn rendered_extended_object_is_not_repainted_as_a_stretched_marker() {
        let record = ObjectRecord::new(vec![0x10, 0x01, 0x41]).unwrap();
        assert!(marker_fallback_tile(&record, false).is_some());
        assert_eq!(marker_fallback_tile(&record, true), None);
    }

    #[test]
    fn canvas_accepts_positioned_extended_objects_but_rejects_command_zero_controls() {
        assert!(ObjectRecord::is_positioned_object(
            &ObjectRecord::new(vec![0, 0, 4]).unwrap()
        ));
        assert!(ObjectRecord::is_positioned_object(
            &ObjectRecord::new(vec![0, 0x10, 0]).unwrap()
        ));
        for parameter in 0..=3 {
            assert!(!ObjectRecord::is_positioned_object(
                &ObjectRecord::new(vec![0, 0, parameter]).unwrap()
            ));
        }
    }

    #[test]
    fn extended_catalog_filters_previews_and_places_the_selected_object() {
        let definitions = standard_object_definitions_for_tileset(0).unwrap();
        assert_eq!(
            extended_object_catalog_selectors(&definitions, "17"),
            [0x17]
        );
        assert!(
            extended_object_catalog_selectors(&definitions, "")
                .iter()
                .all(|selector| *selector >= 4)
        );
        let record = ObjectRecord::new(vec![0, 0, 0x17]).unwrap();
        let tiles = object_catalog_record_tiles(&record, &[0; 64], &definitions).unwrap();
        assert_eq!(tiles, [(0, 0, 0x12d)]);
        let alternate_definitions = standard_object_definitions_for_tileset(1).unwrap();
        assert_ne!(
            object_catalog_record_tiles(&record, &[0; 64], &alternate_definitions).unwrap(),
            tiles,
            "selector $17 preview must follow the active object-tileset substitution"
        );

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let before = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .len();
        editor.select_extended_object_from_catalog(0x17, false);
        assert_eq!(editor.object_form.command_id, 0);
        assert_eq!(editor.object_form.parameter, 0x17);
        assert_eq!(editor.placement_mode, Some(CanvasPlacementMode::Object));
        editor.place_object_at_canvas(
            egui::pos2(36.5, 8.5),
            egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
            ),
            1.0,
            false,
        );
        assert_eq!(editor.error, None);
        let records = &editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records;
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .native_placements()
                .len(),
            before + 1
        );
        assert_eq!(records[editor.selected_object].command_id(), 0);
        assert_eq!(records[editor.selected_object].parameter(), 0x17);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one real-ROM workflow proves placement, drag, typed paste, ordering, and rejection"
    )]
    fn primary_canvas_places_and_drags_object_backed_layer2() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let project = lm_project::Project::new(image);
        let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let layer2_layout = lm_profile::smw_us_v1_layer2_layout(&project.rom).unwrap();
        let lengths = SpriteLengthTable::standard();
        let (level, template) = (0..0x200)
            .find_map(|level| {
                let slot = project
                    .load_level_slot(level, level_layout, &lengths)
                    .ok()?;
                let layer2 = project
                    .load_level_layer2(level, slot.layer1.header.level_mode(), layer2_layout)
                    .ok()?;
                let lm_level::NativeLayer2Data::Objects(objects) = layer2 else {
                    return None;
                };
                let template = objects
                    .objects
                    .records
                    .iter()
                    .find(|record| record.command_id() != 0)?
                    .clone();
                Some((u16::try_from(level).ok()?, template))
            })
            .expect("pristine SMW must contain an object-backed Layer 2 level");

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(level)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        editor.layer2_object_form = ObjectForm::from_record(&template);
        editor.layer2_object_placement_template = Some(template.clone());
        let shortcut_baseline = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.clone(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Duplicate);
        assert_eq!(
            match editor.controller.as_ref().unwrap().layer2().unwrap() {
                lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
                lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
            },
            shortcut_baseline.records.len() + 1
        );
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
        assert_eq!(
            match editor.controller.as_ref().unwrap().layer2().unwrap() {
                lm_level::NativeLayer2Data::Objects(objects) => &objects.objects,
                lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
            },
            &shortcut_baseline
        );
        assert_eq!(editor.canvas_entity_selection, None);
        editor.layer2_object_form = ObjectForm::from_record(&template);
        editor.layer2_object_placement_template = Some(template.clone());
        let before_button_insert = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        let expected_button_selection =
            object_insertion_index(editor.selected_layer2_object, before_button_insert);
        editor.insert_layer2_object_after_selection(before_button_insert);
        assert_eq!(editor.selected_layer2_object, expected_button_selection);
        assert_eq!(
            editor.layer2_object_form.encoded,
            crate::level_editor_forms::format_bytes(template.encoded())
        );
        let before = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => {
                assert_eq!(objects.objects.records.len(), before_button_insert + 1);
                objects.objects.records.len()
            }
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(6144.0, 384.0));
        let vertical = editor.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer2Object);
        let duplicate_position =
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 2.5, ROM_LEVEL_CANVAS_CELL * 3.5);
        editor.begin_secondary_duplicate_drag(
            duplicate_position,
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert!(editor.secondary_duplicate_drag);
        assert_eq!(
            editor.dragging_layer2_object,
            Some(editor.selected_layer2_object)
        );
        editor.finish_secondary_duplicate_drag(
            Some(duplicate_position),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert!(!editor.secondary_duplicate_drag);
        let selected = editor.selected_layer2_object;
        let after_insert = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(after_insert, before + 1);
        let drag_target = if vertical {
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 4.5, ROM_LEVEL_CANVAS_CELL * 18.5)
        } else {
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 18.5, ROM_LEVEL_CANVAS_CELL * 4.5)
        };
        editor.move_layer2_object_to_canvas(
            selected,
            drag_target,
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert!(editor.controller.as_ref().unwrap().layer2_is_modified());
        assert!(editor.error.is_none());

        let clipboard = crate::native_clipboard::encode_level_object(&template).unwrap();
        let before_paste = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        editor.paste_layer2_object(&clipboard, before_paste);
        let pasted_index = editor.selected_layer2_object;
        let pasted = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => {
                assert_eq!(objects.objects.records.len(), before_paste + 1);
                objects.objects.records[pasted_index].clone()
            }
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(pasted, template);
        editor.move_layer2_object(before_paste + 1, false);
        assert_eq!(editor.selected_layer2_object, pasted_index - 1);
        let reordered = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => {
                objects.objects.records[editor.selected_layer2_object].clone()
            }
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(reordered, template);
        let mut replacement = reordered.clone();
        replacement
            .set_parameter(replacement.parameter().wrapping_add(1))
            .unwrap();
        editor.apply_layer2_object_result(Ok(vec![ObjectEdit::Replace {
            index: editor.selected_layer2_object,
            record: replacement.clone(),
        }]));
        assert_eq!(
            editor.layer2_object_form.encoded,
            crate::level_editor_forms::format_bytes(replacement.encoded())
        );
        assert_eq!(
            editor.layer2_object_placement_template.as_ref(),
            Some(&replacement)
        );

        let removed = editor.selected_layer2_object;
        editor.apply_layer2_object_result(Ok(vec![ObjectEdit::Remove { index: removed }]));
        let records_after_remove = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => &objects.objects.records,
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(records_after_remove.len(), before_paste);
        assert_eq!(
            editor.layer2_object_form.encoded,
            crate::level_editor_forms::format_bytes(
                records_after_remove[editor.selected_layer2_object].encoded()
            )
        );
        let count_before_invalid = records_after_remove.len();
        editor.paste_layer2_object("not a typed object", count_before_invalid);
        let count_after_invalid = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(count_after_invalid, count_before_invalid);
        assert!(editor.error.is_some());

        editor.select_extended_object_from_catalog(0x10, true);
        assert_eq!(editor.layer2_object_form.command_id, 0);
        assert_eq!(editor.layer2_object_form.parameter, 0x10);
        assert_eq!(
            editor.placement_mode,
            Some(CanvasPlacementMode::Layer2Object)
        );
        editor.place_layer2_object_at_canvas(
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 34.5, ROM_LEVEL_CANVAS_CELL * 5.5),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert_eq!(editor.error, None);
        let records_after_extended = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => &objects.objects.records,
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(records_after_extended.len(), count_before_invalid + 1);
        assert_eq!(
            records_after_extended[editor.selected_layer2_object].command_id(),
            0
        );
        assert_eq!(
            records_after_extended[editor.selected_layer2_object].parameter(),
            0x10
        );

        let count_after_extended = records_after_extended
            .iter()
            .filter(|record| record.is_positioned_object())
            .count();
        editor.select_standard_object_from_catalog(1, true);
        assert_eq!(editor.layer2_object_form.command_id, 1);
        assert_eq!(editor.layer2_object_form.parameter, 0);
        assert!(editor.layer2_object_placement_template.is_none());
        assert_eq!(
            editor.placement_mode,
            Some(CanvasPlacementMode::Layer2Object)
        );
        editor.place_layer2_object_at_canvas(
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 50.5, ROM_LEVEL_CANVAS_CELL * 6.5),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert_eq!(editor.error, None);
        let records_after_standard = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => &objects.objects.records,
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(
            records_after_standard
                .iter()
                .filter(|record| record.is_positioned_object())
                .count(),
            count_after_extended + 1,
            "catalog placement adds one positioned standard object even if transition controls canonicalize"
        );
        assert_eq!(
            records_after_standard[editor.selected_layer2_object].command_id(),
            1
        );

        let count_after_standard = records_after_standard
            .iter()
            .filter(|record| record.is_positioned_object())
            .count();
        let custom_sidecar = lm_level::OscSidecar::decode(b"22\t2\t13\t0,0,10\n").unwrap();
        let custom_selector =
            lm_level::OscResolvedTable::from_sidecar(&custom_sidecar).objects()[0].selector;
        editor.select_custom_object_from_catalog(custom_selector, true);
        assert_eq!(
            editor.object_catalog_preview_selector,
            Some(custom_selector)
        );
        let custom_template = editor.layer2_object_placement_template.as_ref().unwrap();
        assert_eq!(custom_template.command_id(), 0x22);
        assert_eq!(custom_template.parameter(), 2);
        assert_eq!(custom_template.encoded(), &[0x40, 0x20, 2, 0]);
        assert_eq!(
            editor.placement_mode,
            Some(CanvasPlacementMode::Layer2Object)
        );
        editor.place_layer2_object_at_canvas(
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 66.5, ROM_LEVEL_CANVAS_CELL * 7.5),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert_eq!(editor.error, None);
        let records_after_custom = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => &objects.objects.records,
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(
            records_after_custom
                .iter()
                .filter(|record| record.is_positioned_object())
                .count(),
            count_after_standard + 1
        );
        let placed_custom = &records_after_custom[editor.selected_layer2_object];
        assert_eq!(placed_custom.command_id(), 0x22);
        assert_eq!(placed_custom.parameter(), 2);
        assert_eq!(placed_custom.encoded()[3], 0);

        let staged_layer2 = editor
            .controller
            .as_ref()
            .unwrap()
            .layer2()
            .unwrap()
            .clone();
        let command = prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap();
        app.dispatch(command).unwrap();
        let reopened_slot = app
            .project()
            .unwrap()
            .load_level_slot(
                usize::from(level),
                level_layout,
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let reopened_layer2 = app
            .project()
            .unwrap()
            .load_level_layer2(
                usize::from(level),
                reopened_slot.layer1.header.level_mode(),
                layer2_layout,
            )
            .unwrap();
        assert_eq!(reopened_layer2, staged_layer2);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn pristine_switch_palace_level_opens_its_shared_vanilla_background() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x1bc)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x1bc,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        assert!(editor.controller.is_some(), "{:?}", editor.error);
        assert!(editor.shared_vanilla_background);
    }

    #[test]
    fn diagnostic_print_pristine_level_sprite_placements_when_requested() {
        let Ok(level) = std::env::var("LM_DIAGNOSTIC_LEVEL_SPRITES") else {
            return;
        };
        let level = u16::from_str_radix(level.trim_start_matches("0x"), 16).unwrap();
        let bytes = std::env::var("LM_DIAGNOSTIC_ROM")
            .map(|path| std::fs::read(path).unwrap())
            .unwrap_or_else(|_| crate::test_support::pristine_smw_us_rom_bytes());
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(level)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        for placement in editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements()
        {
            let token =
                &editor.controller.as_ref().unwrap().level().sprites.tokens[placement.token_index];
            eprintln!("{placement:?} token={token:?}");
        }
    }

    #[test]
    fn diagnostic_print_pristine_level_object_placements_when_requested() {
        let Ok(level) = std::env::var("LM_DIAGNOSTIC_LEVEL_OBJECTS") else {
            return;
        };
        let level = u16::from_str_radix(level.trim_start_matches("0x"), 16).unwrap();
        let bytes = std::env::var("LM_DIAGNOSTIC_ROM")
            .map(|path| std::fs::read(path).unwrap())
            .unwrap_or_else(|_| crate::test_support::pristine_smw_us_rom_bytes());
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(level)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let loaded = editor.controller.as_ref().unwrap().level();
        let vertical = lm_profile::smw_us_v1_level_mode(loaded.layer1.header.level_mode()).vertical;
        eprintln!(
            "header={:02X?} sprite-tileset={:02X} object-tileset={:02X}",
            loaded.layer1.header.encoded(),
            loaded.layer1.header.sprite_tileset(),
            loaded.layer1.header.object_tileset(),
        );
        for placement in loaded
            .layer1
            .objects
            .native_placements_for_orientation(vertical)
        {
            let record = &loaded.layer1.objects.records[placement.record_index];
            let (x, y) = placement.tile_coordinates(vertical);
            eprintln!(
                "object {} @({x},{y}) command={:02X} parameter={:02X} bytes={:02X?}",
                placement.record_index,
                record.command_id(),
                record.parameter(),
                record.encoded(),
            );
        }
    }

    #[test]
    fn boss_battle_modes_use_lunar_magics_symmetric_red_diagnostic() {
        for mode in [0x09, 0x0b, 0x10, 0x29, 0x2b, 0x30] {
            assert!(is_boss_battle_level_mode(mode));
        }
        for mode in [0x00, 0x0a, 0x0c, 0x0d, 0x11, 0x1e] {
            assert!(!is_boss_battle_level_mode(mode));
        }
        assert_eq!(boss_battle_diagnostic_red(0), 0);
        assert_eq!(boss_battle_diagnostic_red(1), 1);
        assert_eq!(boss_battle_diagnostic_red(255), 255);
        assert_eq!(boss_battle_diagnostic_red(256), 255);
        assert_eq!(boss_battle_diagnostic_red(511), 0);
        assert_eq!(boss_battle_diagnostic_red(512), 0);
    }

    #[test]
    fn animated_sprite_cache_is_scoped_to_tile_page_two() {
        assert!(!sprite_preview_uses_animated_page(0x0440));
        assert!(sprite_preview_uses_animated_page(0x0648));
        assert!(sprite_preview_uses_animated_page(0x8248));
    }

    #[test]
    fn unresolved_sprite_markers_preserve_lunar_magics_native_empty_handlers() {
        // Dispatch $29 is a real built-in handler at $004C4D10. Only the three entries routed
        // to Lunar Magic's default empty handler suppress an unresolved marker.
        for sprite_number in [0xee, 0xf0, 0xf1] {
            assert!(!should_draw_unresolved_sprite_marker(true, sprite_number));
        }
        assert!(should_draw_unresolved_sprite_marker(true, 0x00));
        assert!(should_draw_unresolved_sprite_marker(true, 0x30));
        assert!(should_draw_unresolved_sprite_marker(true, 0xf6));
        assert!(should_draw_unresolved_sprite_marker(false, 0xee));
    }

    #[test]
    fn translucent_standard_sprite_definitions_are_scoped_to_native_handlers() {
        assert_eq!(
            standard_sprite_preview_tint(0xe1, 0x1b8),
            egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
        );
        assert_eq!(
            standard_sprite_preview_tint(0xe1, 0x114),
            egui::Color32::WHITE
        );
        assert_eq!(
            standard_sprite_preview_tint(0xe0, 0x1b8),
            egui::Color32::WHITE
        );
        assert_eq!(
            standard_sprite_preview_tint(0x90, 0x1c0),
            egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
        );
        assert_eq!(
            standard_sprite_preview_tint(0x90, 0x1f3),
            egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
        );
        assert_eq!(
            standard_sprite_preview_tint(0x90, 0x1bf),
            egui::Color32::WHITE
        );
        assert_eq!(
            sprite_preview_source_tint(Some(0xe1), 0x1b8),
            egui::Color32::from_rgba_premultiplied(127, 127, 127, 128)
        );
        assert_eq!(
            sprite_preview_source_tint(None, 0x1b8),
            egui::Color32::WHITE
        );
    }

    #[test]
    fn object_form_constructs_native_three_byte_record() {
        let form = ObjectForm {
            encoded: String::new(),
            command_id: 0x31,
            parameter: 0x42,
            first_coordinate: 5,
            second_coordinate: 6,
            advances_screen: true,
            screen_jump: None,
            screen_exit: None,
            extended_command27_size: None,
        };
        let record = form.ordinary_record().unwrap();
        assert_eq!(record.encoded(), &[0xe5, 0x16, 0x42]);
        assert_eq!(ObjectForm::from_record(&record).command_id, 0x31);
    }

    #[test]
    fn raw_object_form_round_trips_every_native_extension_shape() {
        for bytes in [
            vec![0x01, 0x10, 0x20],
            vec![0x40, 0x20, 0x80, 0x99],
            vec![0x40, 0x70, 0x01, 0x00, 0xaa],
            vec![0x40, 0x70, 0x01, 0x80, 0xaa, 0xbb],
            vec![0x40, 0x70, 0x01, 0xc0, 0xaa, 0xbb, 0x02],
            vec![0x40, 0x70, 0x81, 0xc0, 0xaa, 0xbb, 0x02, 0xcc],
        ] {
            let record = ObjectRecord::new(bytes.clone()).unwrap();
            let form = ObjectForm::from_record(&record);
            assert_eq!(form.raw_record().unwrap().encoded(), bytes);
        }
    }

    #[test]
    fn raw_object_form_rejects_declared_length_mismatch_atomically() {
        let record =
            ObjectRecord::new(vec![0x40, 0x70, 0x81, 0xc0, 0xaa, 0xbb, 0x02, 0xcc]).unwrap();
        let mut form = ObjectForm::from_record(&record);
        form.encoded = "40 70 81 C0 AA BB 02".into();
        assert!(form.raw_record().is_err());
        assert_eq!(
            record.encoded(),
            &[0x40, 0x70, 0x81, 0xc0, 0xaa, 0xbb, 0x02, 0xcc]
        );
        form.encoded = "40 70 01 C0 AA BB 02 CC".into();
        assert!(form.raw_record().is_err());
        form.encoded = "GG 70 01".into();
        assert!(form.raw_record().is_err());
    }

    #[test]
    fn object_form_rejects_out_of_range_values() {
        assert!(
            ObjectForm {
                command_id: 0x40,
                ..ObjectForm::default()
            }
            .ordinary_record()
            .is_err()
        );
    }

    #[test]
    fn object_form_recognizes_both_native_screen_jump_encodings() {
        let low_first = ObjectRecord::new(vec![0x0d, 0x0c, 1]).unwrap();
        assert_eq!(
            ObjectForm::from_record(&low_first).screen_jump,
            Some((lm_level::ScreenJumpEncoding::FirstLow, 0x0c0d))
        );
        let high_first = ObjectRecord::new(vec![0x1c, 0x0d, 3]).unwrap();
        assert_eq!(
            ObjectForm::from_record(&high_first).screen_jump,
            Some((lm_level::ScreenJumpEncoding::FirstHigh, 0x1c0d))
        );
        assert_eq!(
            ObjectForm::from_record(&ObjectRecord::new(vec![0, 0, 0]).unwrap()).screen_jump,
            None
        );
    }

    #[test]
    fn screen_jump_components_bound_every_valid_value_and_preserve_encoding_order() {
        use lm_level::ScreenJumpEncoding::{FirstHigh, FirstLow};

        for (encoding, packed, components) in [
            (FirstLow, 0x0f1f, (0x1f, 0x0f)),
            (FirstHigh, 0x1f0f, (0x1f, 0x0f)),
            (FirstLow, 0x0c0d, (0x0d, 0x0c)),
            (FirstHigh, 0x1c0d, (0x1c, 0x0d)),
        ] {
            assert_eq!(screen_jump_components(encoding, packed), components);
            assert_eq!(
                pack_screen_jump_components(encoding, components.0, components.1),
                packed
            );
        }

        assert_eq!(pack_screen_jump_components(FirstLow, 0xff, 0xff), 0x0f1f);
        assert_eq!(pack_screen_jump_components(FirstHigh, 0xff, 0xff), 0x1f0f);
    }

    #[test]
    fn screen_jump_resolution_label_distinguishes_native_and_out_of_range_targets() {
        use lm_level::ScreenJumpEncoding::FirstLow;

        assert!(screen_jump_resolution_label(FirstLow, 0x0305).contains("Resolved screen: 08"));
        let out_of_range = screen_jump_resolution_label(FirstLow, 0x0f1f);
        assert!(out_of_range.contains("Resolved screen: 30"));
        assert!(out_of_range.contains("outside 00-1F"));
    }

    #[test]
    fn applied_screen_jump_form_reloads_the_original_encoding_and_exact_target() {
        for (source, requested) in [(vec![0x0d, 0x0c, 1], 0x0f1f), (vec![0x1c, 0x0d, 3], 0x1f0f)] {
            let source = ObjectRecord::new(source).unwrap();
            let encoding = source.screen_jump().unwrap().encoding;
            let mut form = ObjectForm::from_record(&source);
            form.screen_jump = Some((encoding, requested));
            let edits = object_field_edits(&form, 0, Some(&source)).unwrap();
            let [
                ObjectEdit::SetScreenJumpTarget {
                    index: 0,
                    packed_target,
                },
            ] = edits.as_slice()
            else {
                panic!("screen-jump form must stage one semantic edit");
            };
            assert_eq!(*packed_target, requested);
            let mut staged = source.clone();
            staged.set_screen_jump_target(*packed_target).unwrap();
            let refreshed = selected_object_form(std::slice::from_ref(&staged), 0).unwrap();
            assert_eq!(refreshed.screen_jump, Some((encoding, requested)));
        }
    }

    #[test]
    fn object_form_recognizes_native_screen_exit_fields() {
        let compact = ObjectRecord::new(vec![0x85, 0x0a, 0, 0x34]).unwrap();
        assert_eq!(
            ObjectForm::from_record(&compact).screen_exit,
            Some((5, 0x0a34))
        );
        let extended = ObjectRecord::new(vec![0x1f, 0, 2, 0xde, 0xbc]).unwrap();
        assert_eq!(
            ObjectForm::from_record(&extended).screen_exit,
            Some((0x1f, 0xbcde))
        );
    }

    #[test]
    fn applied_screen_exit_form_reloads_canonical_flag_and_encoding_shape() {
        let source = ObjectRecord::new(vec![0x85, 0x0a, 0, 0x34]).unwrap();
        for (requested, expected, encoded_len) in [(0, 0x0400, 4), (0x1000, 0x1400, 5)] {
            let mut form = ObjectForm::from_record(&source);
            form.screen_exit = Some((0x1f, requested));
            let edits = object_field_edits(&form, 0, Some(&source)).unwrap();
            let [ObjectEdit::Replace { index: 0, record }] = edits.as_slice() else {
                panic!("screen-exit form must stage one replacement");
            };
            assert_eq!(record.encoded().len(), encoded_len);
            assert_eq!(record.encoded()[0] & 0x80, 0x80);
            let refreshed = selected_object_form(std::slice::from_ref(record), 0).unwrap();
            assert_eq!(refreshed.screen_exit, Some((0x1f, expected)));
            assert_eq!(
                refreshed.encoded,
                crate::level_editor_forms::format_bytes(record.encoded())
            );
        }

        let mut invalid = ObjectForm::from_record(&source);
        invalid.screen_exit = Some((0x20, 0));
        assert!(object_field_edits(&invalid, 0, Some(&source)).is_err());
        assert_eq!(source.encoded(), &[0x85, 0x0a, 0, 0x34]);
    }

    #[test]
    fn authenticated_object_resize_fields_preserve_unowned_parameter_bits() {
        use lm_render::StandardObjectResizeModel::{
            Fixed, MajorNibble, MinorByte, MinorNibble, ParameterNibbles,
        };

        assert_eq!(
            set_standard_object_major_tiles(ParameterNibbles, 0xab, 4).unwrap(),
            0x3b
        );
        assert_eq!(
            set_standard_object_minor_tiles(ParameterNibbles, 0xab, 5).unwrap(),
            0xa4
        );
        assert_eq!(
            set_standard_object_major_tiles(MajorNibble, 0x2d, 16).unwrap(),
            0xfd
        );
        assert_eq!(
            set_standard_object_minor_tiles(
                MinorNibble {
                    fixed_major_tiles: 2,
                },
                0xc8,
                1,
            )
            .unwrap(),
            0xc0
        );
        assert_eq!(
            set_standard_object_minor_tiles(
                MinorByte {
                    fixed_major_tiles: 3,
                },
                0,
                256,
            )
            .unwrap(),
            0xff
        );
        assert!(set_standard_object_major_tiles(Fixed, 0x55, 2).is_err());
        assert!(set_standard_object_minor_tiles(MajorNibble, 0x55, 2).is_err());
        assert!(set_standard_object_major_tiles(ParameterNibbles, 0, 0).is_err());
        assert!(set_standard_object_minor_tiles(ParameterNibbles, 0, 17).is_err());
        assert!(
            set_standard_object_minor_tiles(
                MinorByte {
                    fixed_major_tiles: 3,
                },
                0,
                257,
            )
            .is_err()
        );
    }

    #[test]
    fn extended_command27_form_replaces_only_recovered_size_and_semantic_fields() {
        let record =
            ObjectRecord::new(vec![0x41, 0x72, 0x84, 0xc3, 0xaa, 0xbb, 0x06, 0xdd]).unwrap();
        let mut form = ObjectForm::from_record(&record);
        assert_eq!(form.extended_command27_size, Some((5, 7)));
        form.first_coordinate = 3;
        form.second_coordinate = 4;
        form.extended_command27_size = Some((128, 64));
        let edits = object_field_edits(&form, 2, Some(&record)).unwrap();
        let [
            ObjectEdit::Replace {
                index,
                record: replacement,
            },
        ] = edits.as_slice()
        else {
            panic!("extended command $27 must remain one lossless replacement");
        };
        assert_eq!(*index, 2);
        assert_eq!(
            replacement.encoded(),
            &[0x43, 0x74, 0xff, 0xc3, 0xaa, 0xbb, 0x3f, 0xdd]
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "keeps the raw form, semantic resize, canvas resize, ROM reopen, and undo assertions in one end-to-end fixture"
    )]
    fn extended_command27_form_and_canvas_commit_reopen_and_undo_in_pristine_rom() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(source).unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = LevelController::decode(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let index = controller.level().layer1.objects.records.len();
        let record =
            ObjectRecord::new(vec![0x41, 0x72, 0x84, 0xc3, 0xaa, 0xbb, 0x06, 0xdd]).unwrap();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index,
                record: record.clone(),
            }])])
            .unwrap();
        let mut raw_form = ObjectForm::from_record(&record);
        raw_form.encoded = "41 72 84 C3 11 22 06 EE".into();
        let raw_record = raw_form.raw_record().unwrap();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                index,
                record: raw_record.clone(),
            }])])
            .unwrap();
        let mut form = ObjectForm::from_record(&raw_record);
        form.extended_command27_size = Some((128, 64));
        controller
            .apply_edits(&[NativeLevelEdit::Objects(
                object_field_edits(&form, index, Some(&raw_record)).unwrap(),
            )])
            .unwrap();
        let current = controller.level().layer1.objects.records[index].clone();
        let placement = controller
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .find(|placement| placement.record_index == index)
            .unwrap();
        let vertical =
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode())
                .vertical;
        let (origin_x, origin_y) = placement.tile_coordinates(vertical);
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            rom_canvas_size(512, 32, vertical, ROM_LEVEL_CANVAS_CELL),
        );
        let resized = resized_standard_object_record_at_canvas_position(
            &current,
            placement,
            lm_render::StandardObjectResizeModel::ExtendedCommand27Axes,
            egui::pos2(
                (f32::from(origin_x) + 4.5) * ROM_LEVEL_CANVAS_CELL,
                (f32::from(origin_y) + 5.5) * ROM_LEVEL_CANVAS_CELL,
            ),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        )
        .unwrap();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                index,
                record: resized,
            }])])
            .unwrap();
        let expected = controller.level().layer1.objects.records[index].clone();
        assert_eq!(expected.extended_command27_tile_size(), Some((5, 6)));
        assert_eq!(
            expected.encoded(),
            &[0x41, 0x72, 0x84, 0xc3, 0x11, 0x22, 0x05, 0xee]
        );
        app.dispatch(prepare_commit(&controller, &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert_eq!(reopened.layer1.objects.records[index], expected);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "keeps clipboard conversion, revision rejection, canvas placement, ROM reopen, and undo in one application-backed fixture"
    )]
    fn direct_map16_clipboard_rectangle_places_reopens_and_undoes_in_pristine_rom() {
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(source).unwrap();
        app.dispatch(Command::ExpandRom(RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let key = EditorKey {
            revision: snapshot.revision,
            level: 0x105,
            sprite_lengths_signature: ssc_sprite_lengths_signature(None),
        };
        let mut editor = VanillaLevelEditor::default();
        editor.load(&snapshot, key, None);
        let rectangle = lm_app::NativeMap16Clipboard::from_rectangle(
            0x4000,
            3,
            2,
            vec![lm_level::Map16Tile::default(); 6],
        )
        .unwrap();
        let text = crate::native_clipboard::encode_native_map16_rectangle(&rectangle).unwrap();
        let revision = editor.controller.as_ref().unwrap().revision();
        let original_template = editor.object_placement_template.clone();

        editor.stage_direct_map16_rectangle(&text, key, revision + 1);
        assert_eq!(editor.object_placement_template, original_template);
        assert!(editor.error.as_deref().unwrap().contains("level changed"));

        editor.stage_direct_map16_rectangle(&text, key, revision);
        let staged = editor.object_placement_template.as_ref().unwrap().clone();
        assert_eq!(staged.encoded(), &[0x40, 0x90, 2, 0xc0, 0, 0x12, 1]);
        assert_eq!(
            staged.direct_map16_fields(),
            Some(lm_level::DirectMap16Rectangle {
                source_tile: 0x4000,
                pattern_width: 3,
                pattern_height: 2,
                output_width: 3,
                output_height: 2,
            })
        );
        assert_eq!(editor.placement_mode, Some(CanvasPlacementMode::Object));

        let vertical = lm_profile::smw_us_v1_level_mode(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .header
                .level_mode(),
        )
        .vertical;
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            rom_canvas_size(512, 32, vertical, ROM_LEVEL_CANVAS_CELL),
        );
        editor.place_object_at_canvas(
            egui::pos2(2.5 * ROM_LEVEL_CANVAS_CELL, 3.5 * ROM_LEVEL_CANVAS_CELL),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let placed = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records[editor.selected_object]
            .clone();
        assert_eq!(placed.direct_map16_fields(), staged.direct_map16_fields());
        assert_ne!(placed.coordinate_nibbles(), staged.coordinate_nibbles());

        let placement = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements_for_orientation(vertical)
            .into_iter()
            .find(|placement| placement.record_index == editor.selected_object)
            .unwrap();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let render_layout = lm_render::NativeLevelMap16Layout {
            width: if vertical { 32 } else { 512 },
            height: if vertical { 512 } else { 32 },
            page_stride: 0x1b0,
            base_cell: 0,
            vertical,
        };
        let preview = lm_render::render_mapped_standard_object_placement(
            &placed,
            placement,
            &definitions,
            editor.active_standard_object_handler_map().unwrap(),
            render_layout,
            VANILLA_EMPTY_MAP16_TILE,
        )
        .unwrap()
        .unwrap();
        let (placed_x, placed_y) = placement.tile_coordinates(vertical);
        assert_eq!(
            preview
                .get(render_layout, usize::from(placed_x), usize::from(placed_y))
                .unwrap(),
            0x4000
        );
        assert_eq!(
            preview
                .get(
                    render_layout,
                    usize::from(placed_x) + 2,
                    usize::from(placed_y) + 1,
                )
                .unwrap(),
            0x4012
        );

        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert!(reopened.layer1.objects.records.contains(&placed));
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn direct_map16_clipboard_conversion_rejects_source_outside_object_namespace() {
        let rectangle = lm_app::NativeMap16Clipboard::from_rectangle(
            0x8000,
            1,
            1,
            vec![lm_level::Map16Tile::default()],
        )
        .unwrap();
        let text = crate::native_clipboard::encode_native_map16_rectangle(&rectangle).unwrap();
        assert!(
            direct_map16_rectangle_from_clipboard(&text)
                .unwrap_err()
                .contains("InvalidDirectMap16Source")
        );
    }

    #[test]
    fn custom_sprite_definitions_use_only_the_m16_domain() {
        let mut bytes = vec![0; lm_level::M16Sidecar::ENCODED_LEN];
        bytes[8..16].copy_from_slice(&[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44]);
        let m16 =
            lm_app::NativeMap16SidecarDocument::M16(lm_level::M16Sidecar::decode(&bytes).unwrap());
        let s16 =
            lm_app::NativeMap16SidecarDocument::S16(lm_level::S16Sidecar::decode(&bytes).unwrap());

        assert_eq!(
            external_sprite_definition(Some(&m16), 1),
            Some([0x1111, 0x2222, 0x3333, 0x4444])
        );
        assert_eq!(external_sprite_definition(Some(&s16), 1), None);
        assert_eq!(external_sprite_definition(Some(&m16), 0x400), None);
    }

    #[test]
    fn map16_paints_distinguish_base_external_and_unresolved_tiles() {
        let bytes = vec![0; lm_level::M16Sidecar::ENCODED_LEN];
        let m16 =
            lm_app::NativeMap16SidecarDocument::M16(lm_level::M16Sidecar::decode(&bytes).unwrap());
        let s16 =
            lm_app::NativeMap16SidecarDocument::S16(lm_level::S16Sidecar::decode(&bytes).unwrap());
        assert_eq!(
            map16_paint_source(0x1ff, None),
            Map16PaintSource::Base(0x1ff)
        );
        assert_eq!(
            map16_paint_source(0x200, Some(&m16)),
            Map16PaintSource::Custom(lm_level::Map16Tile::default())
        );
        assert_eq!(
            map16_paint_source(0x200, None),
            Map16PaintSource::Unresolved
        );
        assert_eq!(
            map16_paint_source(0x200, Some(&s16)),
            Map16PaintSource::Unresolved
        );
        assert_eq!(
            map16_paint_source(0x400, Some(&m16)),
            Map16PaintSource::Unresolved
        );
        assert_eq!(
            map16_paint_source(0x4001, Some(&m16)),
            Map16PaintSource::Unresolved
        );
        assert_eq!(unresolved_map16_label(0x4001), "4001");
    }

    #[test]
    fn custom_object_selection_bounds_include_negative_and_extended_display_parts() {
        let origin = egui::pos2(100.0, 100.0);
        let encoded = egui::Rect::from_min_size(
            origin,
            egui::vec2(ROM_LEVEL_CANVAS_CELL, ROM_LEVEL_CANVAS_CELL),
        );
        let parts = [
            lm_render::CustomObjectPreviewTile {
                tile: 0x123,
                x: -8,
                y: 4,
            },
            lm_render::CustomObjectPreviewTile {
                tile: 0x124,
                x: 32,
                y: -16,
            },
        ];

        assert_eq!(
            custom_object_display_rect(encoded, origin, &parts, ROM_LEVEL_CANVAS_CELL),
            egui::Rect::from_min_max(egui::pos2(94.0, 88.0), egui::pos2(136.0, 115.0))
        );
        assert_eq!(
            custom_object_display_rect(encoded, origin, &[], ROM_LEVEL_CANVAS_CELL),
            encoded
        );
    }

    #[test]
    fn pristine_standard_object_artwork_expands_interactive_footprints() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0 * ROM_LEVEL_CANVAS_CELL, 16.0 * ROM_LEVEL_CANVAS_CELL),
        );
        let mut expanded = None;

        'levels: for level_number in 0..0x200 {
            let Ok(level) =
                project.load_level_slot(level_number, level_layout, &SpriteLengthTable::standard())
            else {
                continue;
            };
            let header = level.layer1.header;
            let vertical = lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical;
            let variant = match lm_profile::smw_us_v1_object_family(header.object_tileset()) {
                lm_profile::VanillaObjectFamily::Normal => 0,
                lm_profile::VanillaObjectFamily::Castle => 1,
                lm_profile::VanillaObjectFamily::Rope => 2,
                lm_profile::VanillaObjectFamily::Underground => 3,
                lm_profile::VanillaObjectFamily::GhostHouse => 4,
            };
            let handler_map = definition_map.family(variant).unwrap();
            let layout = lm_render::NativeLevelMap16Layout {
                width: if vertical { 16 } else { 512 },
                height: if vertical { 512 } else { 16 },
                page_stride: 0x1b0,
                base_cell: 0,
                vertical,
            };
            for placement in level.layer1.objects.native_placements() {
                let record = &level.layer1.objects.records[placement.record_index];
                let encoded =
                    encoded_object_rect(canvas, placement, vertical, ROM_LEVEL_CANVAS_CELL);
                let Ok(Some(cache)) = lm_render::render_mapped_standard_object_placement(
                    record,
                    placement,
                    &definitions,
                    handler_map,
                    layout,
                    u16::MAX,
                ) else {
                    continue;
                };
                let rendered = standard_object_cache_display_rect(
                    encoded,
                    canvas,
                    layout,
                    &cache,
                    ROM_LEVEL_CANVAS_CELL,
                );
                if rendered != encoded {
                    expanded = Some((level_number, record.command_id(), encoded, rendered));
                    break 'levels;
                }
            }
        }

        let (level, command, encoded, rendered) =
            expanded.expect("pristine SMW must contain artwork beyond a generic parameter span");
        assert!(
            rendered.contains(encoded.min) && rendered.contains(encoded.max),
            "level {level:03X} command {command:02X} lost its encoded footprint"
        );
    }

    #[test]
    fn every_pristine_level_object_has_authenticated_builtin_artwork() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut loaded_levels = 0;
        let mut missing = Vec::new();

        for level_number in 0..0x200 {
            let level = project
                .load_level_slot(level_number, level_layout, &SpriteLengthTable::standard())
                .unwrap_or_else(|error| panic!("level {level_number:03X} failed to load: {error}"));
            loaded_levels += 1;
            let header = level.layer1.header;
            let vertical = lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical;
            let family = match lm_profile::smw_us_v1_object_family(header.object_tileset()) {
                lm_profile::VanillaObjectFamily::Normal => 0,
                lm_profile::VanillaObjectFamily::Castle => 1,
                lm_profile::VanillaObjectFamily::Rope => 2,
                lm_profile::VanillaObjectFamily::Underground => 3,
                lm_profile::VanillaObjectFamily::GhostHouse => 4,
            };
            let handler_map = definition_map.family(family).unwrap();
            let layout = lm_render::NativeLevelMap16Layout {
                width: if vertical { 27 } else { 512 },
                height: if vertical { 512 } else { 27 },
                page_stride: 0x1b0,
                base_cell: 0,
                vertical,
            };
            for placement in level
                .layer1
                .objects
                .native_placements_for_orientation(vertical)
            {
                let record = &level.layer1.objects.records[placement.record_index];
                if record.command_id() == 0 && record.parameter() < 4 {
                    continue;
                }
                match lm_render::render_mapped_standard_object_placement(
                    record,
                    placement,
                    &definitions,
                    handler_map,
                    layout,
                    VANILLA_EMPTY_MAP16_TILE,
                ) {
                    Ok(Some(_))
                    | Err(lm_render::StandardObjectRenderError::Cache(
                        lm_render::NativeLevelMap16CacheError::CellOutOfRange(_),
                    )) => {}
                    Ok(None) => {
                        missing.push((level_number, record.command_id(), record.parameter()));
                    }
                    Err(error) => panic!(
                        "level {level_number:03X} record {} failed to render: {error}",
                        placement.record_index
                    ),
                }
            }
        }

        assert_eq!(loaded_levels, 0x200);
        missing.sort_unstable();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "pristine levels with missing built-in artwork: {missing:02X?}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn pristine_level_105_has_authenticated_artwork_for_every_renderable_object() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let family = match lm_profile::smw_us_v1_object_family(level.layer1.header.object_tileset())
        {
            lm_profile::VanillaObjectFamily::Normal => 0,
            lm_profile::VanillaObjectFamily::Castle => 1,
            lm_profile::VanillaObjectFamily::Rope => 2,
            lm_profile::VanillaObjectFamily::Underground => 3,
            lm_profile::VanillaObjectFamily::GhostHouse => 4,
        };
        let handler_map = definition_map.family(family).unwrap();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let layout = lm_render::NativeLevelMap16Layout {
            width: 512,
            height: 32,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let placements = level.layer1.objects.native_placements();
        assert_eq!(placements[0].tile_coordinates(false), (0, 24));
        assert_eq!(placements[1].tile_coordinates(false), (4, 23));
        assert_eq!(handler_map[0x3f], 17);
        let ground = lm_render::render_mapped_standard_object_placement(
            &level.layer1.objects.records[placements[0].record_index],
            placements[0],
            &definitions,
            handler_map,
            layout,
            u16::MAX,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ground.get(layout, 0, 24).unwrap(), 0x100);
        assert_eq!(ground.get(layout, 10, 24).unwrap(), 0x100);
        assert_eq!(ground.get(layout, 0, 25).unwrap(), 0x03f);
        let first_bush = lm_render::render_mapped_standard_object_placement(
            &level.layer1.objects.records[placements[1].record_index],
            placements[1],
            &definitions,
            handler_map,
            layout,
            u16::MAX,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            [
                first_bush.get(layout, 4, 23).unwrap(),
                first_bush.get(layout, 5, 23).unwrap(),
                first_bush.get(layout, 6, 23).unwrap(),
            ],
            [0x073, 0x074, 0x079]
        );
        assert_eq!(first_bush.get(layout, 4, 24).unwrap(), u16::MAX);
        let missing = placements
            .into_iter()
            .filter_map(|placement| {
                let record = &level.layer1.objects.records[placement.record_index];
                if record.command_id() == 0 && record.parameter() < 4 {
                    return None;
                }
                match lm_render::render_mapped_standard_object_placement(
                    record,
                    placement,
                    &definitions,
                    handler_map,
                    layout,
                    u16::MAX,
                ) {
                    Ok(Some(_))
                    | Err(lm_render::StandardObjectRenderError::Cache(
                        lm_render::NativeLevelMap16CacheError::CellOutOfRange(_),
                    )) => None,
                    Ok(None) => Some((
                        placement.record_index,
                        record.command_id(),
                        handler_map[usize::from(record.command_id())],
                        format!("missing at {placement:?}, bytes={:02X?}", record.encoded()),
                    )),
                    Err(error) => Some((
                        placement.record_index,
                        record.command_id(),
                        handler_map[usize::from(record.command_id())],
                        format!("{error} at {placement:?}, bytes={:02X?}", record.encoded()),
                    )),
                }
            })
            .collect::<Vec<_>>();
        assert!(missing.is_empty(), "missing mapped artwork: {missing:02X?}");
    }

    #[test]
    fn pristine_level_107_uses_lunar_magic_object_axes_without_cache_overflow() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                0x107,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let handler_map = definition_map.family(4).unwrap();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let layout = lm_render::NativeLevelMap16Layout {
            width: 512,
            height: 27,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let placements = level.layer1.objects.native_placements();
        for placement in placements.iter().copied() {
            let record = &level.layer1.objects.records[placement.record_index];
            lm_render::render_mapped_standard_object_placement(
                record,
                placement,
                &definitions,
                handler_map,
                layout,
                VANILLA_EMPTY_MAP16_TILE,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "record {} command {:02X} handler {} failed: {error}",
                    placement.record_index,
                    record.command_id(),
                    handler_map[usize::from(record.command_id())]
                )
            });
        }

        let render = |index: usize| {
            let placement = placements[index];
            lm_render::render_mapped_standard_object_placement(
                &level.layer1.objects.records[placement.record_index],
                placement,
                &definitions,
                handler_map,
                layout,
                VANILLA_EMPTY_MAP16_TILE,
            )
            .unwrap()
            .unwrap()
        };
        let ground = render(0);
        assert_eq!(ground.get(layout, 0, 24).unwrap(), 0x10a);
        assert_eq!(ground.get(layout, 42, 24).unwrap(), 0x10c);
        assert_eq!(ground.get(layout, 1, 25).unwrap(), 0x078);
        assert_eq!(ground.get(layout, 1, 26).unwrap(), 0x079);

        let wide_rectangle = render(2);
        assert_eq!(wide_rectangle.get(layout, 26, 20).unwrap(), 0x15e);
        assert_eq!(wide_rectangle.get(layout, 38, 20).unwrap(), 0x15e);
        assert_eq!(
            wide_rectangle.get(layout, 26, 21).unwrap(),
            VANILLA_EMPTY_MAP16_TILE
        );

        let capped_run = render(6);
        assert_eq!(capped_run.get(layout, 42, 21).unwrap(), 0x10a);
        assert_eq!(capped_run.get(layout, 47, 21).unwrap(), 0x10b);
        assert_eq!(capped_run.get(layout, 48, 21).unwrap(), 0x10c);
    }

    #[test]
    fn diagnostic_pristine_level_matches_lunar_magic_map16_cache_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_MAP16_CACHE"),
        ) else {
            return;
        };
        let slot = usize::from_str_radix(&slot, 16).unwrap();
        let live =
            lm_render::NativeLevelMap16Cache::decode(&std::fs::read(cache_path).unwrap()).unwrap();
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                slot,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let vertical = lm_profile::smw_us_v1_level_mode(level.layer1.header.level_mode()).vertical;
        let secondary_base_cell =
            lm_profile::smw_us_v1_secondary_layer_cache_base_cell(level.layer1.header.level_mode());
        if std::env::var_os("LM_DUMP_OBJECTS").is_some() {
            eprintln!(
                "header={:02X?} mode={:02X} background-palette={} foreground-palette={} sprite-palette={} background-color={} object-tileset={}",
                level.layer1.header.encoded(),
                level.layer1.header.level_mode(),
                level.layer1.header.background_palette(),
                level.layer1.header.foreground_palette(),
                level.layer1.header.sprite_palette(),
                level.layer1.header.background_color(),
                level.layer1.header.object_tileset(),
            );
            eprintln!(
                "raw object records: {}",
                level
                    .layer1
                    .objects
                    .records
                    .iter()
                    .enumerate()
                    .map(|(index, record)| format!("{index}:{:02X?}", record.encoded()))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            for placement in level
                .layer1
                .objects
                .native_placements_for_orientation(vertical)
            {
                let record = &level.layer1.objects.records[placement.record_index];
                let (x, y) = placement.tile_coordinates(vertical);
                eprintln!(
                    "object {} @({x},{y}) command={:02X} parameter={:02X} bytes={:02X?}",
                    placement.record_index,
                    record.command_id(),
                    record.parameter(),
                    record.encoded()
                );
            }
            eprintln!(
                "raw sprite tokens: {}",
                level
                    .sprites
                    .tokens
                    .iter()
                    .enumerate()
                    .map(|(index, token)| format!("{index}:{token:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            for placement in level.sprites.native_placements() {
                eprintln!(
                    "sprite {} @{:?} number={:02X} first={:02X} extra-bits={}",
                    placement.token_index,
                    placement.tile_coordinates(vertical),
                    placement.sprite_number,
                    placement.first_byte,
                    placement.extra_bits,
                );
            }
        }
        let major_tiles = usize::from(
            object_stream_major_tiles(&level.layer1.objects.records).max(
                u16::from(
                    lm_profile::smw_us_v1_level_mode(level.layer1.header.level_mode())
                        .editor_major_screens,
                )
                .saturating_mul(16),
            ),
        );
        let layout = lm_render::NativeLevelMap16Layout {
            width: if vertical { 32 } else { major_tiles },
            height: if vertical { major_tiles } else { 27 },
            page_stride: 0x1b0,
            base_cell: 0,
            vertical,
        };
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_tileset_extended_objects(
            &mut definitions,
            level.layer1.header.object_tileset(),
        )
        .unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let family = match lm_profile::smw_us_v1_object_family(level.layer1.header.object_tileset())
        {
            lm_profile::VanillaObjectFamily::Normal => 0,
            lm_profile::VanillaObjectFamily::Castle => 1,
            lm_profile::VanillaObjectFamily::Rope => 2,
            lm_profile::VanillaObjectFamily::Underground => 3,
            lm_profile::VanillaObjectFamily::GhostHouse => 4,
        };
        let handler_map = definition_map.family(family).unwrap();
        if std::env::var_os("LM_DUMP_OBJECTS").is_some() {
            eprintln!(
                "handler map: {}",
                handler_map
                    .iter()
                    .enumerate()
                    .map(|(command, handler)| format!("{command:02X}:{handler}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        let rendered = lm_render::render_mapped_standard_object_stream(
            &level.layer1.objects,
            &definitions,
            handler_map,
            layout,
            VANILLA_EMPTY_MAP16_TILE,
        )
        .unwrap()
        .cache;
        let layer2_rendered = project
            .load_level_layer2(
                slot,
                level.layer1.header.level_mode(),
                lm_profile::smw_us_v1_layer2_layout(&project.rom).unwrap(),
            )
            .ok()
            .and_then(|layer2| {
                let lm_level::NativeLayer2Data::Objects(layer2) = layer2 else {
                    return None;
                };
                if std::env::var_os("LM_DUMP_OBJECTS").is_some() {
                    eprintln!(
                        "raw Layer 2 object records: {}",
                        layer2
                            .objects
                            .records
                            .iter()
                            .enumerate()
                            .map(|(index, record)| { format!("{index}:{:02X?}", record.encoded()) })
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                    for placement in layer2.objects.native_placements() {
                        eprintln!(
                            "Layer 2 object {} horizontal={:?} swapped={:?}",
                            placement.record_index,
                            placement.tile_coordinates(false),
                            placement.tile_coordinates(true),
                        );
                    }
                }
                let layer2_layout = lm_render::NativeLevelMap16Layout {
                    base_cell: secondary_base_cell,
                    ..layout
                };
                Some((
                    lm_render::render_mapped_standard_object_stream(
                        &layer2.objects,
                        &definitions,
                        handler_map,
                        layer2_layout,
                        VANILLA_EMPTY_MAP16_TILE,
                    )
                    .unwrap()
                    .cache,
                    layer2.objects,
                ))
            });
        if let Some((layer2, _)) = layer2_rendered.as_ref() {
            let layer2_layout = lm_render::NativeLevelMap16Layout {
                base_cell: secondary_base_cell,
                ..layout
            };
            let layer2_mismatches = (0..layout.width)
                .flat_map(|x| (0..layout.height).map(move |y| (x, y)))
                .filter_map(|(x, y)| {
                    let actual = layer2.get(layer2_layout, x, y).unwrap();
                    let expected = live.get(layer2_layout, x, y).unwrap();
                    (actual != expected).then_some((x, y, actual, expected))
                })
                .collect::<Vec<_>>();
            eprintln!(
                "level {slot:03X} Layer 2 cache mismatches {} / {}",
                layer2_mismatches.len(),
                layout.width * layout.height,
            );
            for (x, y, actual, expected) in layer2_mismatches.iter().take(100) {
                eprintln!("L2 x={x:03} y={y:03} rust={actual:03X} wine={expected:03X}");
            }
            assert!(layer2_mismatches.is_empty());
        }
        let mut mismatches = Vec::new();
        for x in 0..layout.width {
            for y in 0..layout.height {
                let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
                let actual = if rendered.was_written(index) {
                    rendered.get(layout, x, y).unwrap()
                } else {
                    layer2_rendered
                        .as_ref()
                        .map_or(VANILLA_EMPTY_MAP16_TILE, |(layer2, _)| {
                            layer2
                                .get(
                                    lm_render::NativeLevelMap16Layout {
                                        base_cell: secondary_base_cell,
                                        ..layout
                                    },
                                    x,
                                    y,
                                )
                                .unwrap()
                        })
                };
                let expected = live.get(layout, x, y).unwrap();
                if actual != expected {
                    mismatches.push((x, y, actual, expected));
                }
            }
        }
        eprintln!(
            "level {slot:03X}, vertical={vertical}, tileset={}, mismatches {} / {}",
            level.layer1.header.object_tileset(),
            mismatches.len(),
            layout.width * layout.height,
        );
        if !mismatches.is_empty() {
            let mut owners = vec![None; layout.width * layout.height];
            let mut previous = lm_render::NativeLevelMap16Cache::filled(VANILLA_EMPTY_MAP16_TILE);
            for end in 1..=level.layer1.objects.records.len() {
                let prefix = lm_level::ObjectStream {
                    records: level.layer1.objects.records[..end].to_vec(),
                };
                let next = lm_render::render_mapped_standard_object_stream(
                    &prefix,
                    &definitions,
                    handler_map,
                    layout,
                    VANILLA_EMPTY_MAP16_TILE,
                )
                .unwrap()
                .cache;
                for x in 0..layout.width {
                    for y in 0..layout.height {
                        if next.get(layout, x, y).unwrap() != previous.get(layout, x, y).unwrap() {
                            owners[y * layout.width + x] = Some(end - 1);
                        }
                    }
                }
                previous = next;
            }
            let mut layer2_owners = vec![None; layout.width * layout.height];
            if let Some((_, layer2_objects)) = layer2_rendered.as_ref() {
                let layer2_layout = lm_render::NativeLevelMap16Layout {
                    base_cell: secondary_base_cell,
                    ..layout
                };
                let mut previous =
                    lm_render::NativeLevelMap16Cache::filled(VANILLA_EMPTY_MAP16_TILE);
                for end in 1..=layer2_objects.records.len() {
                    let prefix = lm_level::ObjectStream {
                        records: layer2_objects.records[..end].to_vec(),
                    };
                    let next = lm_render::render_mapped_standard_object_stream(
                        &prefix,
                        &definitions,
                        handler_map,
                        layer2_layout,
                        VANILLA_EMPTY_MAP16_TILE,
                    )
                    .unwrap()
                    .cache;
                    for x in 0..layout.width {
                        for y in 0..layout.height {
                            if next.get(layout, x, y).unwrap()
                                != previous.get(layout, x, y).unwrap()
                            {
                                layer2_owners[y * layout.width + x] = Some(end - 1);
                            }
                        }
                    }
                    previous = next;
                }
            }
            for &(x, y, actual, expected) in mismatches.iter().take(100) {
                let owner = owners[y * layout.width + x].map_or_else(
                    || {
                        layer2_owners[y * layout.width + x].map_or_else(
                            || "unwritten".to_owned(),
                            |owner| {
                                let (_, layer2_objects) = layer2_rendered.as_ref().unwrap();
                                let record = &layer2_objects.records[owner];
                                let placement = layer2_objects
                                    .native_placements()
                                    .into_iter()
                                    .find(|placement| placement.record_index == owner)
                                    .unwrap();
                                let (placement_x, placement_y) =
                                    placement.tile_coordinates(vertical);
                                format!(
                                    "L2:{owner}@({placement_x},{placement_y}): command={:02X} handler={} parameter={:02X}",
                                    record.command_id(),
                                    handler_map[usize::from(record.command_id())],
                                    record.parameter(),
                                )
                            },
                        )
                    },
                    |owner| {
                        let record = &level.layer1.objects.records[owner];
                        let placement = level
                            .layer1
                            .objects
                            .native_placements()
                            .into_iter()
                            .find(|placement| placement.record_index == owner)
                            .unwrap();
                        let (placement_x, placement_y) = placement.tile_coordinates(vertical);
                        format!(
                            "{owner}@({placement_x},{placement_y}): command={:02X} handler={} parameter={:02X}",
                            record.command_id(),
                            handler_map[usize::from(record.command_id())],
                            record.parameter(),
                        )
                    },
                );
                eprintln!("x={x:03} y={y:03} rust={actual:03X} wine={expected:03X} owner={owner}");
            }
        }
        assert!(mismatches.is_empty());
    }

    #[test]
    fn diagnostic_pristine_level_matches_lunar_magic_layer2_cache_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_LAYER2_CACHE"),
        ) else {
            return;
        };
        let slot = u16::from_str_radix(&slot, 16).unwrap();
        let live = std::fs::read(cache_path).unwrap();
        assert_eq!(live.len(), lm_level::NATIVE_LAYER2_TILEMAP_LEN);

        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(slot)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut layout = editor_layer2_layout(&snapshot, slot).unwrap();
        if let (Some(layout), Ok(offset)) = (
            layout.as_mut(),
            std::env::var("LM_LEVEL_LAYER2_POINTER_OFFSET"),
        ) {
            layout.pointers.offset =
                usize::from_str_radix(offset.trim_start_matches("0x"), 16).unwrap();
        }
        let controller = LevelController::decode_with_layer2(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            layout,
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let lm_level::NativeLayer2Data::Tilemap(rust) =
            controller.layer2().expect("level must have Layer 2 data")
        else {
            panic!("level uses object-backed Layer 2");
        };
        eprintln!(
            "header={:02X?} mode={:02X}",
            controller.level().layer1.header.encoded(),
            controller.level().layer1.header.level_mode()
        );
        if let Ok(path) = std::env::var("LM_DUMP_RUST_LAYER2") {
            std::fs::write(path, rust).unwrap();
        }
        let mismatches = rust
            .chunks_exact(2)
            .zip(live.chunks_exact(2))
            .enumerate()
            .filter(|(_, (rust, live))| rust != live)
            .map(|(index, (rust, live))| {
                (
                    index,
                    u16::from_le_bytes([rust[0], rust[1]]),
                    u16::from_le_bytes([live[0], live[1]]),
                )
            })
            .collect::<Vec<_>>();
        let rust_words = rust
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect::<Vec<_>>();
        let live_words = live
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect::<Vec<_>>();
        let best_rotation = (0..live_words.len())
            .map(|rotation| {
                let differences = rust_words
                    .iter()
                    .enumerate()
                    .filter(|(index, word)| {
                        **word != live_words[(index + rotation) % live_words.len()]
                    })
                    .count();
                (differences, rotation)
            })
            .min()
            .unwrap();
        eprintln!(
            "level {slot:03X}: {} Layer 2 mismatches / {} cells; best rotation={best_rotation:?}; first={:?}",
            mismatches.len(),
            live.len() / 2,
            mismatches.iter().take(16).collect::<Vec<_>>()
        );
        assert!(mismatches.is_empty());
    }

    #[test]
    fn pristine_level_102_matches_live_snes_map16_rows_around_high_tide() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                0x102,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let layout = lm_render::NativeLevelMap16Layout {
            width: 512,
            height: 27,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let report = lm_render::render_mapped_standard_object_stream(
            &level.layer1.objects,
            &definitions,
            definition_map.family(2).unwrap(),
            layout,
            VANILLA_EMPTY_MAP16_TILE,
        )
        .unwrap();
        let expected = [
            [
                0x025, 0x025, 0x073, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x075,
                0x025, 0x025, 0x025, 0x025,
            ],
            [
                0x107, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108, 0x108,
                0x108, 0x108, 0x108, 0x109,
            ],
            [
                0x025, 0x073, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074, 0x074,
                0x074, 0x074, 0x075, 0x025,
            ],
        ];
        for (row_offset, expected_row) in expected.into_iter().enumerate() {
            let y = 18 + row_offset;
            for (x, tile) in expected_row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.get(layout, x, y).unwrap(),
                    tile,
                    "live Snes9x Map16 mismatch at ({x}, {y})"
                );
            }
        }
        for y in 21..27 {
            for (x, tile) in expected[2].into_iter().enumerate() {
                assert_eq!(
                    report.cache.get(layout, x, y).unwrap(),
                    tile,
                    "live Snes9x Map16 mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn pristine_level_108_matches_live_snes_vertical_slope_rows() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                0x108,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let layout = lm_render::NativeLevelMap16Layout {
            width: 32,
            height: 512,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: true,
        };
        let report = lm_render::render_mapped_standard_object_stream(
            &level.layer1.objects,
            &definitions,
            definition_map.family(3).unwrap(),
            layout,
            VANILLA_EMPTY_MAP16_TILE,
        )
        .unwrap();
        assert_eq!(report.rendered_objects, 11);
        assert!(report.missing_commands.is_empty());
        assert!(report.missing_extended_objects.is_empty());
        assert_eq!(
            rendered_standard_object_canvas_extent(
                &level.layer1.objects.records,
                definition_map.family(3).unwrap(),
                true,
            ),
            Some((43, 16))
        );
        let live_nonblank = [
            (10, 14, 0x1ca),
            (11, 14, 0x1cc),
            (2, 15, 0x1aa),
            (3, 15, 0x1af),
            (4, 15, 0x196),
            (5, 15, 0x19b),
            (6, 15, 0x1a0),
            (7, 15, 0x1a5),
            (8, 15, 0x1ca),
            (9, 15, 0x1cc),
            (10, 15, 0x1cb),
            (11, 15, 0x1cd),
            (2, 16, 0x1e2),
            (3, 16, 0x1e4),
            (4, 16, 0x1de),
            (5, 16, 0x1e6),
            (6, 16, 0x1e6),
            (7, 16, 0x1e0),
            (8, 16, 0x1cb),
            (9, 16, 0x1cd),
            (10, 16, 0x1f1),
            (11, 16, 0x1f2),
            (8, 17, 0x1f1),
            (9, 17, 0x1f2),
        ];
        for y in 14..=17 {
            for x in 0..27 {
                let expected = live_nonblank
                    .iter()
                    .find_map(|&(live_x, live_y, tile)| {
                        (live_x == x && live_y == y).then_some(tile)
                    })
                    .unwrap_or(VANILLA_EMPTY_MAP16_TILE);
                assert_eq!(
                    report.cache.get(layout, x, y).unwrap(),
                    expected,
                    "live Snes9x vertical Map16 mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn pristine_level_109_clips_vertical_plane_edge_artwork() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                0x109,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let vertical = true;
        let handler_map = definition_map.family(3).unwrap();
        assert_eq!(
            rendered_standard_object_canvas_extent(
                &level.layer1.objects.records,
                handler_map,
                true,
            ),
            Some((112, 32))
        );
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let layout = lm_render::NativeLevelMap16Layout {
            width: 32,
            height: 512,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical,
        };
        let report = lm_render::render_mapped_standard_object_stream(
            &level.layer1.objects,
            &definitions,
            handler_map,
            layout,
            VANILLA_EMPTY_MAP16_TILE,
        )
        .unwrap();
        assert_eq!(report.rendered_objects, 159);
        assert!(report.missing_commands.is_empty());
        assert!(report.missing_extended_objects.is_empty());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn pristine_standard_object_resize_commits_reopens_and_undoes() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(source.clone()).unwrap();
        let definition_map =
            lm_profile::load_smw_us_v1_standard_object_definition_map(&image).unwrap();
        let project = lm_project::Project::new(image);
        let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut candidate = None;

        'levels: for level_number in 0..0x200 {
            let Ok(level) =
                project.load_level_slot(level_number, level_layout, &SpriteLengthTable::standard())
            else {
                continue;
            };
            let family =
                match lm_profile::smw_us_v1_object_family(level.layer1.header.object_tileset()) {
                    lm_profile::VanillaObjectFamily::Normal => 0,
                    lm_profile::VanillaObjectFamily::Castle => 1,
                    lm_profile::VanillaObjectFamily::Rope => 2,
                    lm_profile::VanillaObjectFamily::Underground => 3,
                    lm_profile::VanillaObjectFamily::GhostHouse => 4,
                };
            let handler_map = definition_map.family(family).unwrap();
            for (index, record) in level.layer1.objects.records.iter().enumerate() {
                let Some(model) = definitions.mapped_resize_model(record, handler_map) else {
                    continue;
                };
                let parameter = record.parameter();
                let resized = match model {
                    lm_render::StandardObjectResizeModel::ParameterNibbles
                    | lm_render::StandardObjectResizeModel::MajorNibble => {
                        let current = (parameter >> 4) + 1;
                        let next = if current == 16 { 15 } else { current + 1 };
                        set_standard_object_major_tiles(model, parameter, next).unwrap()
                    }
                    lm_render::StandardObjectResizeModel::SwappedParameterNibbles => {
                        let current = (parameter & 0x0f) + 1;
                        let next = if current == 16 { 15 } else { current + 1 };
                        set_standard_object_major_tiles(model, parameter, next).unwrap()
                    }
                    lm_render::StandardObjectResizeModel::MinorNibble { .. } => {
                        let current = (parameter & 0x0f) + 1;
                        let next = if current == 16 { 15 } else { current + 1 };
                        set_standard_object_minor_tiles(model, parameter, u16::from(next)).unwrap()
                    }
                    lm_render::StandardObjectResizeModel::MinorByte { .. } => {
                        let current = u16::from(parameter) + 1;
                        let next = if current == 256 { 255 } else { current + 1 };
                        set_standard_object_minor_tiles(model, parameter, next).unwrap()
                    }
                    lm_render::StandardObjectResizeModel::MajorByte { .. } => {
                        let current = u16::from(parameter) + 1;
                        let next = if current == 256 { 255 } else { current + 1 };
                        set_standard_object_major_byte_tiles(model, next).unwrap()
                    }
                    lm_render::StandardObjectResizeModel::ExtendedCommand27Axes
                    | lm_render::StandardObjectResizeModel::Fixed => continue,
                };
                if resized != parameter {
                    candidate = Some((u16::try_from(level_number).unwrap(), index, resized));
                    break 'levels;
                }
            }
        }

        let (level_number, index, resized) =
            candidate.expect("pristine SMW must contain a resizable standard object");
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(level_number)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller =
            LevelController::decode(&snapshot, level_layout, &SpriteLengthTable::standard())
                .unwrap();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::SetParameter {
                index,
                parameter: resized,
            }])])
            .unwrap();
        app.dispatch(prepare_commit(&controller, &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                usize::from(level_number),
                level_layout,
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert_eq!(reopened.layer1.objects.records[index].parameter(), resized);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn sprite_preview_geometry_scales_offsets_and_unions_complete_artwork() {
        let marker = egui::Rect::from_min_size(
            egui::pos2(100.0, 100.0),
            egui::vec2(ROM_LEVEL_CANVAS_CELL, ROM_LEVEL_CANVAS_CELL),
        );
        assert_eq!(
            sprite_preview_part_rect(marker, 16, -8, ROM_LEVEL_CANVAS_CELL),
            marker.translate(egui::vec2(12.0, -6.0))
        );
        assert_eq!(
            sprite_preview_bounds(marker, [(-8, 4), (32, -16)], ROM_LEVEL_CANVAS_CELL),
            egui::Rect::from_min_max(egui::pos2(94.0, 88.0), egui::pos2(136.0, 115.0))
        );
        assert_eq!(
            sprite_preview_bounds(marker, [], ROM_LEVEL_CANVAS_CELL),
            marker
        );
        let unresolved = [
            lm_render::RemappedCustomSpritePreviewTile {
                definition_index: 0x20,
                subtiles: [0; 4],
                graphics_base: 0x2000,
                palette_source: Some(1),
                x: -8,
                y: 4,
            },
            lm_render::RemappedCustomSpritePreviewTile {
                definition_index: 0x21,
                subtiles: [0; 4],
                graphics_base: 0x2000,
                palette_source: Some(1),
                x: 32,
                y: -16,
            },
        ];
        assert_eq!(
            resolved_sprite_preview_bounds(marker, None, Some(&unresolved), ROM_LEVEL_CANVAS_CELL),
            egui::Rect::from_min_max(egui::pos2(94.0, 88.0), egui::pos2(136.0, 115.0))
        );
    }

    #[test]
    fn sprite_insertion_follows_selection_or_appends_to_an_empty_stream() {
        assert_eq!(sprite_insertion_index(0, 0), 0);
        assert_eq!(sprite_insertion_index(0, 3), 1);
        assert_eq!(sprite_insertion_index(2, 3), 3);
        assert_eq!(sprite_insertion_index(99, 3), 3);
    }

    #[test]
    fn rom_canvas_zoom_scales_and_swaps_orientation_axes() {
        assert_eq!(
            rom_canvas_size(32, 16, false, 12.0),
            egui::vec2(384.0, 192.0)
        );
        assert_eq!(
            rom_canvas_size(32, 16, true, 24.0),
            egui::vec2(384.0, 768.0)
        );
        assert_eq!(
            rom_canvas_size(512, 32, false, 6.0),
            egui::vec2(3072.0, 192.0)
        );
        assert_eq!(clamp_canvas_zoom(0), ROM_LEVEL_CANVAS_MIN_ZOOM);
        assert_eq!(clamp_canvas_zoom(275), 275);
        assert_eq!(clamp_canvas_zoom(u16::MAX), ROM_LEVEL_CANVAS_MAX_ZOOM);
    }

    #[test]
    fn tile_grid_requires_both_the_original_display_flag_and_editor_overlays() {
        let mut visibility = crate::application::LevelViewVisibility::default();
        assert!(!tile_grid_visible(true, visibility));
        visibility.tile_grid = true;
        assert!(tile_grid_visible(true, visibility));
        assert!(!tile_grid_visible(false, visibility));
    }

    #[test]
    fn screen_grid_regions_match_horizontal_top_bottom_and_vertical_left_right() {
        let horizontal = level_screen_grid_regions(32, 27, false);
        assert_eq!(horizontal.len(), 4);
        assert_eq!(
            horizontal[0],
            LevelScreenGridRegion {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
                label: "00 : Top".into(),
            }
        );
        assert_eq!(horizontal[1].y, 16);
        assert_eq!(horizontal[1].height, 11);
        assert_eq!(horizontal[1].label, "00 : Bottom");
        assert_eq!(horizontal[2].x, 16);
        assert_eq!(horizontal[2].label, "01 : Top");

        let vertical = level_screen_grid_regions(32, 32, true);
        assert_eq!(vertical.len(), 4);
        assert_eq!(vertical[0].label, "00 : Left");
        assert_eq!(vertical[1].x, 16);
        assert_eq!(vertical[1].label, "00 : Right");
        assert_eq!(vertical[2].y, 16);
        assert_eq!(vertical[2].label, "01 : Left");
    }

    #[test]
    fn screen_exit_annotations_resolve_direct_midway_secondary_and_overworld_targets() {
        let direct = ObjectRecord::new(vec![0x00, 0x05, 0x00, 0x23]).unwrap();
        let secondary = ObjectRecord::new(vec![0x01, 0x07, 0x00, 0x01]).unwrap();
        let midway = ObjectRecord::new(vec![0x02, 0x0d, 0x00, 0x23]).unwrap();
        let overworld = ObjectRecord::new(vec![0x03, 0x07, 0x00, 0x02]).unwrap();
        let mut table = SecondaryExitTable {
            entries: vec![lm_level::SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT],
        };
        table.entries[0x101].destination_level = 0x1ab;
        table.entries[0x102].x_and_overworld_flags = 0x80;
        let annotations = level_screen_exit_annotations(
            64,
            27,
            false,
            &[direct, secondary, midway, overworld],
            Some(&table),
        );
        assert_eq!(annotations[0].label, "00 : Exit to Level 123");
        assert_eq!(annotations[1].label, "01 : Secondary Exit 101 to 1AB");
        assert_eq!(annotations[2].label, "02 : Midway Exit to Level 123");
        assert_eq!(annotations[3].label, "03 : Secondary Exit 102 to OV");
        assert_eq!((annotations[3].x, annotations[3].height), (48, 27));
    }

    #[test]
    fn complete_screen_exit_form_uses_last_duplicate_for_each_of_all_32_screens() {
        let records = vec![
            ObjectRecord::new(vec![0x00, 0x05, 0, 0x11]).unwrap(),
            ObjectRecord::new(vec![0x1f, 0x06, 0, 0x22]).unwrap(),
            ObjectRecord::new(vec![0x80, 0x07, 0, 0x33]).unwrap(),
        ];
        let exits = screen_exit_table(&records);
        assert_eq!(exits[0], Some(0x0733));
        assert_eq!(exits[0x1f], Some(0x0622));
        assert_eq!(exits.iter().filter(|entry| entry.is_some()).count(), 2);
    }

    #[test]
    fn mouse_screen_exit_command_maps_canvas_cells_in_both_level_orientations() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 320.0));
        let horizontal = LevelCanvasGeometry {
            rect,
            cell: 10.0,
            major_tiles: 64,
            minor_tiles: 27,
            vertical: false,
        };
        assert_eq!(
            screen_at_canvas_position(egui::pos2(10.0 + 35.0 * 10.0, 25.0), horizontal),
            Some(2)
        );
        assert_eq!(
            screen_at_canvas_position(egui::pos2(15.0, 20.0 + 27.5 * 10.0), horizontal),
            None
        );

        let vertical_rect =
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(320.0, 640.0));
        let vertical = LevelCanvasGeometry {
            rect: vertical_rect,
            cell: 10.0,
            major_tiles: 64,
            minor_tiles: 32,
            vertical: true,
        };
        assert_eq!(
            screen_at_canvas_position(egui::pos2(15.0, 20.0 + 51.0 * 10.0), vertical),
            Some(3)
        );
        assert_eq!(
            screen_at_canvas_position(
                egui::pos2(vertical_rect.right() + 1.0, vertical_rect.top()),
                vertical
            ),
            None
        );
    }

    #[test]
    fn authenticated_mouse_edit_screen_exit_preselects_and_opens_the_complete_table() {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput {
            events: vec![egui::Event::PointerMoved(egui::pos2(335.0, 25.0))],
            ..egui::RawInput::default()
        });
        let mut editor = VanillaLevelEditor {
            tools_panel_visible: Some(false),
            canvas_geometry: Some(LevelCanvasGeometry {
                rect: egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 270.0)),
                cell: 10.0,
                major_tiles: 64,
                minor_tiles: 27,
                vertical: false,
            }),
            ..VanillaLevelEditor::default()
        };
        assert!(editor.toolbar_open_screen_exit_at_pointer(&context));
        assert_eq!(editor.screen_exit_table_selected, Some(2));
        assert_eq!(
            editor.requested_tool_panel,
            Some(LevelToolPanel::ScreenExits)
        );
        assert_eq!(editor.tools_panel_visible, Some(true));
        let _ = context.end_pass();
    }

    #[test]
    fn last_screen_exit_record_wins_and_vertical_regions_follow_screen_rows() {
        let first = ObjectRecord::new(vec![0x01, 0x04, 0x00, 0x01]).unwrap();
        let last = ObjectRecord::new(vec![0x01, 0x04, 0x00, 0x02]).unwrap();
        let annotations = level_screen_exit_annotations(32, 32, true, &[first, last], None);
        assert_eq!(annotations[1].label, "01 : Exit to Level 2");
        assert_eq!(
            (
                annotations[1].x,
                annotations[1].y,
                annotations[1].width,
                annotations[1].height,
            ),
            (0, 16, 32, 16)
        );
    }

    #[test]
    fn boundary_guide_uses_recovered_mode_dimensions_and_camera_anchor() {
        assert_eq!(
            level_boundary_guide_geometry(0, (3, 4)),
            LevelBoundaryGuideGeometry {
                x_tiles: 3.0,
                y_tiles: 4.0,
                width_tiles: 16.0,
                height_tiles: 14.5,
            }
        );
        assert_eq!(
            level_boundary_guide_geometry(5, (7, 8)),
            LevelBoundaryGuideGeometry {
                x_tiles: 7.0,
                y_tiles: 8.0,
                width_tiles: 22.0,
                height_tiles: 14.5,
            }
        );
        assert_eq!(
            level_boundary_guide_geometry(7, (9, 10)),
            LevelBoundaryGuideGeometry {
                x_tiles: 9.0,
                y_tiles: 10.0,
                width_tiles: 28.0,
                height_tiles: 14.0,
            }
        );
    }

    #[test]
    fn toolbar_zoom_matches_lunar_magic_range_steps_and_previous_toggle() {
        let mut editor = VanillaLevelEditor::default();
        assert_eq!(
            ROM_LEVEL_CANVAS_ZOOM_MENU,
            [100, 125, 150, 175, 200, 300, 400, 600, 800]
        );
        assert_eq!(editor.canvas_zoom_percent(), 100);
        assert!(editor.zoom_filter());
        editor.toolbar_zoom_popup();
        assert!(editor.zoom_popup_open);
        editor.toolbar_zoom_filter_toggle();
        assert!(!editor.zoom_filter());
        editor.toolbar_zoom_set(125);
        assert_eq!(editor.canvas_zoom_percent(), 125);
        editor.toolbar_zoom_set(200);
        editor.toolbar_zoom_default();
        editor.toolbar_zoom_toggle();
        assert_eq!(editor.canvas_zoom_percent(), 200);
        editor.toolbar_zoom_adjust(100);
        assert_eq!(editor.canvas_zoom_percent(), 300);
        editor.toolbar_zoom_default();
        assert_eq!(editor.canvas_zoom_percent(), 100);
        editor.toolbar_zoom_toggle();
        assert_eq!(editor.canvas_zoom_percent(), 300);
        editor.toolbar_zoom_adjust(-10_000);
        assert_eq!(editor.canvas_zoom_percent(), 100);
        editor.toolbar_zoom_adjust(10_000);
        assert_eq!(editor.canvas_zoom_percent(), 5_000);
    }

    #[test]
    fn toolbar_animation_commands_pause_step_reload_and_resume_one_shared_clock() {
        let mut editor = VanillaLevelEditor::default();
        assert!(editor.animation_playing());
        assert!((editor.animation_seconds(1.0) - 1.0).abs() < f64::EPSILON);

        editor.toolbar_animation_toggle();
        assert!(!editor.animation_playing());
        assert!((editor.animation_seconds(10.0) - 1.0).abs() < f64::EPSILON);
        editor.toolbar_animation_step();
        assert!(
            (editor.animation_seconds(20.0) - (1.0 + LUNAR_MAGIC_ANIMATION_TICK_SECONDS)).abs()
                < f64::EPSILON
        );

        editor.toolbar_animation_reset();
        assert!(
            (editor.animation_seconds(30.0) - (1.0 + LUNAR_MAGIC_ANIMATION_TICK_SECONDS)).abs()
                < f64::EPSILON
        );
        editor.toolbar_animation_toggle();
        assert!(editor.animation_playing());
        assert!(
            (editor.animation_seconds(30.5) - (1.5 + LUNAR_MAGIC_ANIMATION_TICK_SECONDS)).abs()
                < 1.0e-9
        );
    }

    #[test]
    fn toolbar_switch_commands_toggle_four_independent_default_on_states() {
        let mut editor = VanillaLevelEditor::default();
        assert_eq!(
            editor.switch_view_state,
            lm_render::LunarMagicSwitchViewState::default()
        );
        for switch in 0..4 {
            editor.toolbar_switch_view_toggle(switch);
        }
        assert_eq!(
            editor.switch_view_state,
            lm_render::LunarMagicSwitchViewState {
                green: false,
                yellow: false,
                blue: false,
                red: false,
            }
        );
        editor.toolbar_switch_view_toggle(2);
        assert!(editor.switch_view_state.blue);
        assert!(!editor.switch_view_state.green);
        assert!(!editor.switch_view_state.yellow);
        assert!(!editor.switch_view_state.red);
    }

    #[test]
    fn toolbar_silver_pow_toggles_default_off_standard_sprite_substitution() {
        let mut editor = VanillaLevelEditor::default();
        assert!(!editor.silver_pow_active);
        editor.toolbar_silver_pow_toggle();
        assert!(editor.silver_pow_active);
        let mut mode = lm_render::StandardSpritePreviewMode::default();
        mode.alternate_display = editor.silver_pow_active;
        let preview = lm_render::render_lunar_magic_standard_sprite_with_mode(0x0c, mode).unwrap();
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].definition_index, 0x115);
        assert_eq!((preview[0].x, preview[0].y), (0, 1));
    }

    #[test]
    fn toolbar_blue_pow_toggles_default_off_animation_view_state() {
        let mut editor = VanillaLevelEditor::default();
        assert!(!editor.blue_pow_active);
        editor.toolbar_blue_pow_toggle();
        assert!(editor.blue_pow_active);
    }

    #[test]
    fn conditional_object_toolbar_states_default_on_and_toggle_independently() {
        let mut editor = VanillaLevelEditor::default();
        assert_eq!(
            editor.conditional_view_state,
            lm_render::LunarMagicConditionalViewState::default()
        );
        editor.toolbar_invisible_pow_objects_toggle();
        editor.toolbar_other_invisible_objects_toggle();
        editor.toolbar_on_off_switch_toggle();
        editor.toolbar_conditional_direct_map16_toggle();
        editor.toolbar_block_contents_toggle();
        editor.toolbar_block_exits_toggle();
        editor.toolbar_have_star_toggle();
        editor.toolbar_time_100_toggle();
        editor.toolbar_five_yoshi_coins_toggle();
        assert_eq!(
            editor.conditional_view_state,
            lm_render::LunarMagicConditionalViewState {
                invisible_pow_objects: false,
                other_invisible_objects: false,
                on_off_switch_on: false,
                conditional_direct_map16: false,
                block_contents: true,
                block_exits: true,
                have_star: true,
                time_100: true,
                five_yoshi_coins: true,
            }
        );
        assert_eq!(block_contents_overlay_alpha(0x4104), 192);
        assert_eq!(block_contents_overlay_alpha(0x80b8), 128);
        assert_eq!(
            block_exit_outline_stripes(),
            [
                (0, egui::Color32::BLACK),
                (3, egui::Color32::BLACK),
                (12, egui::Color32::BLACK),
                (15, egui::Color32::BLACK),
                (1, egui::Color32::RED),
                (2, egui::Color32::RED),
                (13, egui::Color32::RED),
                (14, egui::Color32::RED),
            ]
        );
    }

    #[test]
    fn exanimation_trigger_toolbar_states_match_native_ranges_and_wrapping() {
        let mut editor = VanillaLevelEditor::default();
        assert_eq!(
            editor.exanimation_trigger_view_state,
            ExAnimationTriggerViewState::default()
        );

        editor.toolbar_custom_trigger_toggle(0x0a);
        editor.toolbar_one_shot_trigger_toggle(0x1f);
        editor.toolbar_manual_trigger_adjust(0x0f, -1);
        assert!(editor.exanimation_trigger_view_state.custom[0x0a]);
        assert!(editor.exanimation_trigger_view_state.one_shot[0x1f]);
        assert_eq!(
            editor.exanimation_trigger_view_state.manual_frames[0x0f],
            0xff
        );

        editor.toolbar_trigger_selection_adjust(0, -1);
        editor.toolbar_trigger_selection_adjust(1, -1);
        editor.toolbar_trigger_selection_adjust(2, -1);
        assert_eq!(editor.exanimation_trigger_view_state.selected_custom, 0x0f);
        assert_eq!(
            editor.exanimation_trigger_view_state.selected_one_shot,
            0x1f
        );
        assert_eq!(editor.exanimation_trigger_view_state.selected_manual, 0x0f);

        editor.toolbar_current_trigger_action(0, 0);
        editor.toolbar_current_trigger_action(1, 0);
        editor.toolbar_current_trigger_action(2, 1);
        assert!(editor.exanimation_trigger_view_state.custom[0x0f]);
        assert!(!editor.exanimation_trigger_view_state.one_shot[0x1f]);
        assert_eq!(editor.exanimation_trigger_view_state.manual_frames[0x0f], 0);
    }

    #[test]
    fn block_exit_warnings_use_the_final_written_cell_instead_of_write_history() {
        let layout = lm_render::NativeLevelMap16Layout {
            width: 2,
            height: 1,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let mut cache = lm_render::NativeLevelMap16Cache::filled(VANILLA_EMPTY_MAP16_TILE);
        cache.set(layout, 0, 0, 0x1f).unwrap();
        cache.set(layout, 0, 0, 0x21).unwrap();
        cache.set(layout, 1, 0, 0x27).unwrap();
        assert_eq!(block_exit_warning_cells(&cache, layout, 0), [(1, 0)]);
    }

    #[test]
    fn toolbar_512_height_background_switches_vertical_wrap_from_27_to_32_tiles() {
        let mut editor = VanillaLevelEditor::default();
        assert!(!editor.background_512_height);
        assert_eq!(background_plane_height_pixels(false), 27 * 16);
        editor.toolbar_background_512_height_toggle();
        assert!(editor.background_512_height);
        assert_eq!(background_plane_height_pixels(true), 32 * 16);

        let entrance = VanillaMainEntrance {
            position: 0x0b,
            screen_and_method: 0x03,
            ..VanillaMainEntrance::default()
        };
        assert_ne!(
            vanilla_game_background_coordinates(0, 31, entrance, (0, 0), false).1,
            vanilla_game_background_coordinates(0, 31, entrance, (0, 0), true).1,
        );
    }

    #[test]
    fn toolbar_translucent_switch_applies_half_opacity_only_to_editor_overlays() {
        let mut editor = VanillaLevelEditor::default();
        assert!(!editor.translucent_overlays);
        assert_eq!(overlay_opacity(editor.translucent_overlays), 1.0);
        editor.toolbar_translucent_overlays_toggle();
        assert!(editor.translucent_overlays);
        assert_eq!(overlay_opacity(editor.translucent_overlays), 0.5);
    }

    #[test]
    fn one_snes_screen_fills_the_available_canvas_pane_and_preserves_zoom() {
        for (available, zoom, expected) in [
            (egui::vec2(800.0, 600.0), 100, 50.0),
            (egui::vec2(800.0, 600.0), 200, 100.0),
            (egui::vec2(256.0, 252.0), 100, 18.0),
            (egui::vec2(1_200.0, 1_000.0), 125, 93.75),
            (egui::vec2(128.0, 140.0), 100, 10.0),
        ] {
            assert!((fitted_snes_viewport_cell(available, zoom) - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn fitted_snes_viewport_sizes_exactly_one_256_by_224_screen() {
        let cell = fitted_snes_viewport_cell(egui::vec2(800.0, 600.0), 100);
        let canvas = egui::vec2(16.0 * cell, 14.0 * cell);
        assert!((canvas.x - 800.0).abs() < 0.001);
        assert!((canvas.y - 700.0).abs() < 0.001);
        // Cover mode must reach both pane edges; the mismatched axis is centered and clipped.
        assert!(canvas.x >= 800.0);
        assert!(canvas.y >= 600.0);
    }

    #[test]
    fn live_frame_uses_the_same_centered_cover_geometry_as_game_pixels() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let cell = fitted_snes_viewport_cell(canvas.size(), 100);
        let frame = live_frame_rect(canvas, [256, 224], cell);
        assert_eq!(frame.width(), 800.0);
        assert_eq!(frame.height(), 700.0);
        assert_eq!(frame.center(), canvas.center());
        assert_eq!(frame.min.y, -50.0);
        assert_eq!(frame.max.y, 650.0);
    }

    #[test]
    fn live_frame_selection_overlay_defaults_on_and_retains_an_explicit_choice() {
        let mut editor = VanillaLevelEditor::default();
        assert!(editor.draw_selection_over_live());
        editor.draw_selection_over_live = Some(false);
        assert!(!editor.draw_selection_over_live());
    }

    #[test]
    fn live_frame_selection_filter_keeps_only_the_active_object_group() {
        let placement = |record_index| lm_level::NativeObjectPlacement {
            record_index,
            screen: 0,
            major: 0,
            minor: 0,
            major_span: 1,
            minor_span: 1,
        };
        let placements = [placement(2), placement(5), placement(8)];
        let grouped = selected_object_placements(&placements, &[2, 8], 5);
        assert_eq!(
            grouped
                .iter()
                .map(|placement| placement.record_index)
                .collect::<Vec<_>>(),
            vec![2, 8]
        );
        let singular = selected_object_placements(&placements, &[], 5);
        assert_eq!(singular.len(), 1);
        assert_eq!(singular[0].record_index, 5);
    }

    #[test]
    fn snes_screen_fit_recomputes_for_window_resize_and_full_screen() {
        let windowed = fitted_snes_viewport_cell(egui::vec2(640.0, 480.0), 100);
        let resized = fitted_snes_viewport_cell(egui::vec2(960.0, 720.0), 100);
        let full_screen = fitted_snes_viewport_cell(egui::vec2(1_920.0, 1_080.0), 100);
        assert!(resized > windowed);
        assert!(full_screen > resized);
        for cell in [windowed, resized, full_screen] {
            assert!((16.0 * cell / (14.0 * cell) - 256.0 / 224.0).abs() < 0.000_001);
        }
    }

    #[test]
    fn snes_screen_fit_responds_to_horizontal_only_resize() {
        let narrow = fitted_snes_viewport_cell(egui::vec2(640.0, 480.0), 100);
        let wide = fitted_snes_viewport_cell(egui::vec2(800.0, 480.0), 100);
        assert_eq!(narrow, 40.0);
        assert_eq!(wide, 50.0);
    }

    #[test]
    fn native_sprite_subscreen_coordinates_preserve_the_encoded_fifth_bit() {
        let sprites = [lm_level::NativeSpritePlacement {
            token_index: 0,
            first_byte: 1,
            screen: 0,
            major: 3,
            minor: 31,
            sprite_number: 1,
            extra_bits: 0,
        }];
        assert_eq!(NATIVE_LEVEL_MINOR_TILES, 27);
        assert_eq!(
            presented_sprite_tile_coordinates(sprites[0], false),
            (3, 31)
        );
        assert_eq!(presented_sprite_tile_coordinates(sprites[0], true), (31, 3));
    }

    #[test]
    fn vanilla_horizontal_entrance_scroll_matches_lunar_magic_tables() {
        let mut entrance = VanillaMainEntrance {
            position: 0x0b,
            ..VanillaMainEntrance::default()
        };
        assert_eq!(vanilla_horizontal_entrance_scroll_row(entrance), 16);

        entrance.position = 0x07;
        assert_eq!(vanilla_horizontal_entrance_scroll_row(entrance), 3);

        entrance.position = 0x0d;
        assert_eq!(vanilla_horizontal_entrance_scroll_row(entrance), 16);
    }

    #[test]
    fn level_105_editor_background_uses_native_canvas_coordinates() {
        let entrance = VanillaMainEntrance {
            position: 0x5b,
            screen_and_method: 0x9a,
            ..VanillaMainEntrance::default()
        };
        assert_eq!(
            vanilla_shared_background_coordinates(0, 16, entrance),
            (0, 16)
        );
        assert_eq!(
            vanilla_shared_background_coordinates(20, 16, entrance),
            (20, 16)
        );
    }

    #[test]
    fn background_draw_offsets_preserve_columns_beyond_u8_range() {
        assert_eq!(native_canvas_tile_offset(0, 16.0), 0.0);
        assert_eq!(native_canvas_tile_offset(255, 16.0), 4_080.0);
        assert_eq!(native_canvas_tile_offset(256, 16.0), 4_096.0);
        assert_eq!(native_canvas_tile_offset(511, 16.0), 8_176.0);
    }

    #[test]
    fn level_106_primary_entrance_label_matches_lunar_magic_pixel_anchor() {
        let entrance = VanillaMainEntrance {
            position: 0x5b,
            vertical_settings: 0,
            screen_and_method: 0x9a,
            level_mode_and_screen: 0,
        };
        assert_eq!(
            horizontal_primary_entrance_label_pixels(entrance),
            (0x22, 0x160)
        );
    }

    #[test]
    fn level_108_vertical_primary_entrance_matches_lunar_magic_world_anchor() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let project = lm_project::Project::new(image);
        let entrance = project
            .load_vanilla_main_entrance(0x108, lm_profile::smw_us_v1_vanilla_entrance_layout())
            .unwrap();
        assert_eq!(
            entrance,
            VanillaMainEntrance {
                position: 0x0b,
                vertical_settings: 0,
                screen_and_method: 0x0a,
                level_mode_and_screen: 0,
            }
        );
        assert_eq!(
            vertical_primary_entrance_marker_pixels(entrance, false),
            (0x10, 0x160)
        );
        assert_eq!(
            vertical_primary_entrance_label_pixels(entrance, false),
            (0x22, 0x160)
        );
        assert_eq!(entrance.screen_and_method & 1, 0);
    }

    #[test]
    fn level_1d9_midway_entrance_uses_the_vanilla_midway_screen_nibble() {
        let entrance = VanillaMainEntrance {
            position: 0x09,
            vertical_settings: 0x03,
            screen_and_method: 0x0a,
            level_mode_and_screen: 0x03,
        };
        assert_eq!(
            horizontal_primary_entrance_marker_pixels(entrance),
            (0x3e0, 0x130)
        );
        assert_eq!(
            midway_entrance_marker_pixels(entrance, false, false),
            (0xe0, 0x130)
        );
        assert_eq!(
            midway_entrance_label_pixels(entrance, false, false),
            (0xf2, 0x130)
        );
    }

    #[test]
    fn entrance_toolbar_flags_match_lunar_magics_independent_and_aggregate_states() {
        let mut editor = VanillaLevelEditor::default();
        assert_eq!(
            editor.entrance_overlay_visibility,
            EntranceOverlayVisibility {
                all: true,
                primary: true,
                secondary: true,
                midway: true,
            }
        );

        editor.toolbar_toggle_entrance_overlay(EntranceOverlayToggle::Secondary);
        assert!(editor.entrance_overlay_visibility.all);
        assert!(editor.entrance_overlay_visibility.primary);
        assert!(!editor.entrance_overlay_visibility.secondary);
        assert!(editor.entrance_overlay_visibility.midway);

        editor.toolbar_toggle_entrance_overlay(EntranceOverlayToggle::All);
        assert_eq!(
            editor.entrance_overlay_visibility,
            EntranceOverlayVisibility {
                all: false,
                primary: false,
                secondary: false,
                midway: false,
            }
        );
        editor.toolbar_toggle_entrance_overlay(EntranceOverlayToggle::Primary);
        assert!(!editor.entrance_overlay_visibility.all);
        assert!(editor.entrance_overlay_visibility.primary);
        assert!(!editor.entrance_overlay_visibility.secondary);
        assert!(!editor.entrance_overlay_visibility.midway);
        editor.toolbar_toggle_entrance_overlay(EntranceOverlayToggle::All);
        assert_eq!(
            editor.entrance_overlay_visibility,
            EntranceOverlayVisibility {
                all: true,
                primary: true,
                secondary: true,
                midway: true,
            }
        );
    }

    #[test]
    fn live_secondary_entrance_filter_uses_screen_exit_references_and_slot_high_bit() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let project = Project::new(image);
        let referenced = referenced_secondary_exit_slots(&project).unwrap();
        assert_eq!(referenced.len(), SecondaryExitTable::ENTRY_COUNT);
        assert!(referenced[0x0bf]);
        assert!(referenced[0x1be]);
        let exits = project
            .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        assert_eq!(
            secondary_entrance_destination(0x1be, exits.entries[0x1be]),
            0x102
        );
        let visible = visible_secondary_entrances(0x102, &exits, &referenced);
        assert!(visible.iter().any(|(index, _)| *index == 0x1be));
        assert!(visible.iter().all(|(index, exit)| {
            referenced[*index]
                && exit.x_and_overworld_flags & 0x80 == 0
                && secondary_entrance_destination(*index, *exit) == 0x102
        }));
    }

    #[test]
    fn level_01d_secondary_entrance_uses_five_screen_bits_and_native_label_clearance() {
        let exit = lm_level::SecondaryExit {
            destination_level: 0x01d,
            position_and_method: 2,
            screen: 0x0a,
            x: 0,
            y: 3,
            destination_flags: 3,
            x_and_overworld_flags: 0,
            additional_flags: 0,
        };
        assert_eq!(
            secondary_entrance_marker_and_label_pixels(exit, false, false),
            ((0x0ae0, 0x60), (0x0af8, 0x60))
        );
    }

    #[test]
    fn level_1ce_vertical_primary_and_midway_use_their_configured_screens() {
        let entrance = VanillaMainEntrance {
            position: 0x73,
            vertical_settings: 0xf8,
            screen_and_method: 0x03,
            level_mode_and_screen: 0x64,
        };
        assert_eq!(
            vertical_primary_entrance_marker_pixels(entrance, false),
            (0x10, 0x480)
        );
        assert_eq!(
            vertical_primary_entrance_label_pixels(entrance, false),
            (0x28, 0x480)
        );
        assert_eq!(
            midway_entrance_marker_pixels(entrance, true, false),
            (0x10, 0x80)
        );
        assert_eq!(
            midway_entrance_label_pixels(entrance, true, false),
            (0x28, 0x80)
        );
    }

    #[test]
    fn level_105_game_preview_uses_initial_smw_camera_positions() {
        let entrance = VanillaMainEntrance {
            position: 0x5b,
            screen_and_method: 0x9a,
            ..VanillaMainEntrance::default()
        };
        assert_eq!(game_preview_origin(entrance, 512, 27, false), (0, 12));
        assert_eq!(
            vanilla_game_background_coordinates(0, 12, entrance, (0, 12), false),
            (0, 12)
        );
        assert_eq!(
            vanilla_game_background_coordinates(15, 25, entrance, (0, 12), false),
            (15, 25)
        );
    }

    #[test]
    fn cookie_mountain_background_uses_half_speed_horizontal_parallax() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = lm_project::Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let entrance = project
            .load_vanilla_main_entrance(1, lm_profile::smw_us_v1_vanilla_entrance_layout())
            .unwrap();
        assert_eq!(
            entrance,
            VanillaMainEntrance {
                position: 0x5b,
                screen_and_method: 0x9a,
                ..VanillaMainEntrance::default()
            }
        );
        assert_eq!(
            vanilla_layer2_camera_pixels(entrance, (15 * 16, 12 * 16)),
            (120, 192)
        );
        assert_eq!(
            vanilla_layer2_camera_pixels(entrance, (30 * 16, 12 * 16)),
            (240, 192)
        );
    }

    #[test]
    fn preview_camera_offsets_follow_orientation_and_clamp_to_level_bounds() {
        assert_eq!(
            offset_game_preview_origin((0, 12), 15, -20, 512, 27, false),
            (15, 0)
        );
        assert_eq!(
            offset_game_preview_origin((496, 13), 99, 99, 512, 27, false),
            (496, 13)
        );
        assert_eq!(
            offset_game_preview_origin((0, 12), 16, 4, 512, 27, true),
            (4, 28)
        );
        assert_eq!(
            offset_game_preview_origin((0, 498), 16, -4, 512, 27, true),
            (0, 498)
        );
    }

    #[test]
    fn vertical_game_preview_uses_the_recovered_initial_camera_row() {
        let entrance = VanillaMainEntrance {
            screen_and_method: 0x0a,
            ..VanillaMainEntrance::default()
        };
        assert_eq!(game_preview_origin(entrance, 512, 27, true), (0, 12));
        assert_eq!(vanilla_layer2_camera_pixels(entrance, (0, 0xc0)), (0, 0xae));
    }

    #[test]
    fn rom_canvas_major_extent_is_bounded_to_native_screen_space() {
        let sprites = [lm_level::NativeSpritePlacement {
            token_index: 0,
            first_byte: 0,
            screen: 31,
            major: 511,
            minor: 0,
            sprite_number: 1,
            extra_bits: 0,
        }];
        assert_eq!(canvas_major_tiles(&[], &sprites), 512);
        let out_of_model = [lm_level::NativeSpritePlacement {
            major: u16::MAX,
            ..sprites[0]
        }];
        assert_eq!(canvas_major_tiles(&[], &out_of_model), 512);
    }

    #[test]
    fn object_stream_extent_ends_at_its_furthest_encoded_screen() {
        let first_screen = [ObjectRecord::new(vec![0x01, 0x12, 0]).unwrap()];
        assert_eq!(object_stream_major_tiles(&first_screen), 16);

        let jumped = [
            first_screen[0].clone(),
            ObjectRecord::new(vec![0x03, 0x00, 1]).unwrap(),
            ObjectRecord::new(vec![0x04, 0x15, 0]).unwrap(),
        ];
        assert_eq!(object_stream_major_tiles(&jumped), 64);
    }

    #[test]
    fn standard_sprite_catalog_is_complete_and_hex_filterable() {
        let all = sprite_catalog_ids("");
        assert_eq!(all.len(), usize::from(STANDARD_SPRITE_MAX) + 1);
        assert_eq!(all.first(), Some(&0));
        assert_eq!(all.last(), Some(&STANDARD_SPRITE_MAX));
        let with_a = sprite_catalog_ids("a");
        assert_eq!(with_a.len(), 30);
        assert!(with_a.contains(&0x0a));
        assert!(with_a.contains(&0xa0));
        assert!(with_a.contains(&0xaf));
        assert!(with_a.contains(&0xea));
        assert_eq!(sprite_catalog_ids("ED"), vec![0xed]);
        assert!(sprite_catalog_ids("not hex").is_empty());
    }

    #[test]
    fn standard_object_catalog_covers_every_noncontrol_command_and_filters_hex() {
        let all = object_catalog_commands("");
        assert_eq!(all.len(), 0x3f);
        assert_eq!(all.first(), Some(&1));
        assert_eq!(all.last(), Some(&0x3f));
        assert_eq!(object_catalog_commands("3F"), vec![0x3f]);
        assert!(object_catalog_commands("not hex").is_empty());
    }

    #[test]
    fn standard_object_catalog_uses_the_pristine_tileset_handler_map() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let map = lm_profile::load_smw_us_v1_standard_object_definition_map(&rom).unwrap();
        let definitions = standard_object_definitions().unwrap();
        let family = map.family(0).unwrap();
        let rendered = (1..=0x3f)
            .filter(|&command| object_catalog_tiles(command, family, &definitions).is_some())
            .count();
        assert_eq!(
            rendered, 63,
            "normal-family authenticated artwork coverage changed"
        );
    }

    #[test]
    fn custom_object_catalog_selects_active_variant_and_filters_descriptions() {
        let sidecar = lm_level::OscSidecar::decode(
            b"10\t2\t11\tCustom Pipe\n10\t2\t13\t0,0,10\n10\t2\t23\t0,0,11\n11\t3\t2\t0,0,12\n",
        )
        .unwrap();
        let resolved = lm_level::OscResolvedTable::from_sidecar(&sidecar);
        let variant_one = custom_object_catalog_entries(&resolved, 1, "");
        assert_eq!(variant_one.len(), 2);
        assert_eq!(variant_one[0].selector.object_type, 0x10);
        assert_eq!(variant_one[1].selector.object_type, 0x11);
        assert_eq!(
            custom_object_catalog_entries(&resolved, 1, "pipe")[0]
                .selector
                .parameter,
            2
        );
        assert_eq!(
            custom_object_catalog_entries(&resolved, 2, "")[0]
                .display
                .as_ref()
                .unwrap()[0]
                .tile,
            0x11
        );
    }

    #[test]
    fn custom_object_catalog_materializes_native_command_specific_shapes() {
        let sidecar = lm_level::OscSidecar::decode(
            b"22\t2\t13\t0,0,10\n2D\t3\t13\t0,0,11\n27\t4\t13\t0,0,12\n",
        )
        .unwrap();
        let resolved = lm_level::OscResolvedTable::from_sidecar(&sidecar);
        let records = resolved
            .objects()
            .iter()
            .map(|object| custom_object_native_record(object.selector).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records[0].encoded(), &[0x40, 0x20, 2, 0]);
        assert_eq!(records[1].encoded(), &[0x40, 0xd0, 3, 0, 0]);
        assert_eq!(records[2].encoded(), &[0x40, 0x70, 4, 0, 0]);
    }

    #[test]
    fn osc_display_presence_overrides_builtin_artwork_even_when_empty() {
        let displayed =
            lm_level::OscSidecar::decode(b"10\t2\t13\t\n10\t3\t11\tDescription only\n").unwrap();
        let resolved = lm_level::OscResolvedTable::from_sidecar(&displayed);
        let display_record = custom_object_native_record(
            resolved
                .objects()
                .iter()
                .find(|object| object.selector.parameter == 2)
                .unwrap()
                .selector,
        )
        .unwrap();
        let description_record = custom_object_native_record(
            resolved
                .objects()
                .iter()
                .find(|object| object.selector.parameter == 3)
                .unwrap()
                .selector,
        )
        .unwrap();
        assert_eq!(
            resolved_custom_object_parts(&display_record, &resolved, 1),
            Some(Vec::new())
        );
        assert_eq!(
            resolved_custom_object_parts(&description_record, &resolved, 1),
            None
        );
    }

    #[test]
    fn osc_custom_object_inserts_commits_reopens_and_retains_display() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let sidecar = lm_level::OscSidecar::decode(b"10\t2\t2\t0,0,10;16,0,11\n").unwrap();
        let resolved = lm_level::OscResolvedTable::from_sidecar(&sidecar);
        let object = resolved.objects().first().unwrap();
        let record = custom_object_native_record(object.selector).unwrap();
        assert_eq!(
            resolved_custom_object_parts(&record, &resolved, object.selector.variant)
                .unwrap()
                .len(),
            2
        );

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let record_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records
            .len();
        editor.selected_object = record_count.saturating_sub(1);
        editor.object_form = ObjectForm::from_record(&record);
        editor.object_placement_template = Some(record.clone());
        editor.insert_object_after_selection(record_count);
        let insertion = record_count;
        assert_eq!(editor.selected_object, insertion);
        assert_eq!(
            editor.object_form.encoded,
            crate::level_editor_forms::format_bytes(record.encoded())
        );
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records[insertion],
            record
        );

        let clipboard = crate::native_clipboard::encode_level_object(&record).unwrap();
        editor.paste_object(&clipboard, record_count + 1);
        let pasted = editor.selected_object;
        assert_eq!(pasted, insertion + 1);
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));
        assert_eq!(editor.object_record_for_placement().unwrap(), record);
        editor.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
            index: pasted,
        }])));
        assert_eq!(editor.selected_object, insertion);
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));
        editor.move_object(record_count + 1, false);
        assert_eq!(editor.selected_object, insertion - 1);
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));
        editor.move_object(record_count + 1, true);
        assert_eq!(editor.selected_object, insertion);
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));

        let mut replacement = record.clone();
        replacement
            .set_coordinate_nibbles(ObjectCoordinateNibbles {
                first: 7,
                second: 8,
            })
            .unwrap();
        editor.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: insertion,
            record: replacement.clone(),
        }])));
        assert_eq!(
            editor.object_form.encoded,
            crate::level_editor_forms::format_bytes(replacement.encoded())
        );
        assert_eq!(
            editor.object_placement_template.as_ref(),
            Some(&replacement)
        );
        assert!(editor.controller.as_mut().unwrap().undo());
        editor.reload_object_form();

        editor.apply_object_result(Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
            index: insertion,
        }])));
        assert_eq!(editor.selected_object, record_count.saturating_sub(1));
        let selected_after_remove = &editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records[editor.selected_object];
        assert_eq!(
            editor.object_form.encoded,
            crate::level_editor_forms::format_bytes(selected_after_remove.encoded())
        );
        assert!(editor.controller.as_mut().unwrap().undo());
        editor.selected_object = insertion;
        editor.reload_object_form();
        assert_eq!(editor.object_placement_template.as_ref(), Some(&record));

        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(0x105, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(reopened.layer1.objects.records[insertion], record);
        let reopened_parts = resolved_custom_object_parts(
            &reopened.layer1.objects.records[insertion],
            &resolved,
            object.selector.variant,
        )
        .unwrap();
        assert_eq!(reopened_parts.len(), 2);
        let origin = egui::pos2(64.0, 32.0);
        let encoded = egui::Rect::from_min_size(
            origin,
            egui::vec2(ROM_LEVEL_CANVAS_CELL, ROM_LEVEL_CANVAS_CELL),
        );
        assert_eq!(
            custom_object_display_rect(encoded, origin, &reopened_parts, ROM_LEVEL_CANVAS_CELL,),
            egui::Rect::from_min_size(
                origin,
                egui::vec2(ROM_LEVEL_CANVAS_CELL * 2.0, ROM_LEVEL_CANVAS_CELL)
            )
        );
    }

    #[test]
    fn custom_object_placement_retains_required_extension_bytes() {
        let record = ObjectRecord::new(vec![0x40, 0x20, 2, 0xaa]).unwrap();
        let mut editor = VanillaLevelEditor {
            object_form: ObjectForm::from_record(&record),
            object_placement_template: Some(record),
            ..VanillaLevelEditor::default()
        };
        editor.object_form.first_coordinate = 5;
        editor.object_form.second_coordinate = 6;
        let placed = editor.object_record_for_placement().unwrap();
        assert_eq!(placed.encoded(), &[0x45, 0x26, 2, 0xaa]);
    }

    #[test]
    fn layer2_custom_object_placement_retains_required_extension_bytes() {
        let record = ObjectRecord::new(vec![0x40, 0xd0, 3, 0xaa, 0xbb]).unwrap();
        let mut editor = VanillaLevelEditor {
            layer2_object_form: ObjectForm::from_record(&record),
            layer2_object_placement_template: Some(record),
            ..VanillaLevelEditor::default()
        };
        editor.layer2_object_form.first_coordinate = 7;
        editor.layer2_object_form.second_coordinate = 8;
        let placed = editor.layer2_object_record_for_placement().unwrap();
        assert_eq!(placed.encoded(), &[0x47, 0xd8, 3, 0xaa, 0xbb]);
    }

    #[test]
    fn catalog_selection_constructs_a_valid_native_sprite_record() {
        let fields = NativeSpriteRecordFields {
            y_low: 0x1d,
            extra_bits: 2,
            screen: 0x1e,
            x: 3,
            sprite_number: 0xa6,
        };
        let token = standard_sprite_token(fields, &SpriteLengthTable::standard()).unwrap();
        let SpriteToken::Record(record) = token else {
            panic!("catalog always constructs an ordinary record");
        };
        assert_eq!(record.encoded, vec![0xdb, 0x3e, 0xa6]);
        assert_eq!(record.native_fields().unwrap(), fields);
    }

    #[test]
    fn canvas_entity_shortcuts_duplicate_and_remove_objects_and_sprites_exactly() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );

        let original_objects = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .clone();
        editor.selected_object = 1;
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Duplicate);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records
                .len(),
            original_objects.records.len() + 1
        );
        assert_eq!(editor.selected_object, 2);
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
        assert_eq!(
            editor.controller.as_ref().unwrap().level().layer1.objects,
            original_objects
        );
        assert_eq!(editor.canvas_entity_selection, None);

        let original_sprites = editor.controller.as_ref().unwrap().level().sprites.clone();
        editor.selected_sprite = original_sprites
            .tokens
            .iter()
            .position(|token| matches!(token, SpriteToken::Record(_)))
            .unwrap();
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Duplicate);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .tokens
                .len(),
            original_sprites.tokens.len() + 1
        );
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
        assert_eq!(
            editor.controller.as_ref().unwrap().level().sprites,
            original_sprites
        );
        assert_eq!(editor.canvas_entity_selection, None);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one workflow proves both original right-click object and sprite routes"
    )]
    fn right_click_duplication_repositions_objects_and_sprites_without_removing_sources() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );

        let object_placement = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .next()
            .unwrap();
        let source_object = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records[object_placement.record_index]
            .clone();
        let object_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .len();
        editor.selected_object = object_placement.record_index;
        editor.object_form = ObjectForm::from_record(&source_object);
        editor.object_placement_template = Some(source_object.clone());
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        let object_target = egui::pos2(36.5, 8.5);
        let (screen, coordinates, perpendicular_high) =
            object_placement_at_canvas_position(object_target, canvas, 1.0, false).unwrap();
        editor.begin_secondary_duplicate_drag(object_target, canvas, 1.0, false);
        assert!(editor.secondary_duplicate_drag);
        assert_eq!(editor.dragging_object, Some(editor.selected_object));
        editor.finish_secondary_duplicate_drag(Some(object_target), canvas, 1.0, false);
        assert!(!editor.secondary_duplicate_drag);
        let objects = &editor.controller.as_ref().unwrap().level().layer1.objects;
        assert_eq!(objects.native_placements().len(), object_count + 1);
        assert!(objects.records.contains(&source_object));
        assert!(objects.native_placements().into_iter().any(|placement| {
            placement.record_index == editor.selected_object
                && placement.screen == screen
                && placement.major
                    == screen
                        .saturating_mul(16)
                        .saturating_add(u16::from(coordinates.second))
                && placement.minor == coordinates.first | if perpendicular_high { 0x10 } else { 0 }
        }));

        let source_sprite_index = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .tokens
            .iter()
            .position(|token| matches!(token, SpriteToken::Record(_)))
            .unwrap();
        let source_sprite =
            editor.controller.as_ref().unwrap().level().sprites.tokens[source_sprite_index].clone();
        let sprite_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .tokens
            .len();
        editor.selected_sprite = source_sprite_index;
        editor.sprite_form = SpriteForm::from_token(
            editor.controller.as_ref().unwrap().level().sprites.header,
            Some(&source_sprite),
        );
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        let sprite_target = egui::pos2(69.5, 7.5);
        let expected_fields = sprite_fields_at_canvas_position(
            sprite_target,
            canvas,
            1.0,
            false,
            NativeSpriteRecordFields {
                y_low: editor.sprite_form.y_low,
                extra_bits: editor.sprite_form.extra_bits,
                screen: editor.sprite_form.screen,
                x: editor.sprite_form.x,
                sprite_number: editor.sprite_form.sprite_number,
            },
        )
        .unwrap();
        editor.begin_secondary_duplicate_drag(sprite_target, canvas, 1.0, false);
        assert!(editor.secondary_duplicate_drag);
        assert_eq!(editor.dragging_sprite, Some(editor.selected_sprite));
        editor.finish_secondary_duplicate_drag(Some(sprite_target), canvas, 1.0, false);
        assert!(!editor.secondary_duplicate_drag);
        let sprites = &editor.controller.as_ref().unwrap().level().sprites;
        assert_eq!(sprites.tokens.len(), sprite_count + 1);
        assert!(sprites.tokens.contains(&source_sprite));
        let SpriteToken::Record(inserted) = &sprites.tokens[editor.selected_sprite] else {
            panic!("right-click duplication must select the inserted sprite record");
        };
        assert_eq!(inserted.native_fields().unwrap(), expected_fields);
    }

    #[test]
    fn ctrl_object_selection_toggles_members_and_keeps_layer_domains_exclusive() {
        let mut editor = VanillaLevelEditor::default();
        assert!(editor.update_canvas_object_group(CanvasEntitySelection::Layer1Object, 3, false,));
        assert!(editor.update_canvas_object_group(CanvasEntitySelection::Layer1Object, 7, true,));
        assert_eq!(editor.selected_object_group, vec![3, 7]);

        // An unmodified press on a selected member preserves the group so that the following
        // physical drag moves the complete selection rather than silently collapsing it.
        assert!(editor.update_canvas_object_group(CanvasEntitySelection::Layer1Object, 3, false,));
        assert_eq!(editor.selected_object_group, vec![3, 7]);

        assert!(!editor.update_canvas_object_group(CanvasEntitySelection::Layer1Object, 3, true,));
        assert_eq!(editor.selected_object_group, vec![7]);
        assert!(editor.update_canvas_object_group(CanvasEntitySelection::Layer2Object, 11, true,));
        assert!(editor.selected_object_group.is_empty());
        assert_eq!(editor.selected_layer2_object_group, vec![11]);
        assert_eq!(
            editor.canvas_entity_selection,
            Some(CanvasEntitySelection::Layer2Object)
        );
        assert!(editor.update_canvas_sprite_group(5, true));
        assert!(editor.selected_object_group.is_empty());
        assert!(editor.selected_layer2_object_group.is_empty());
        assert_eq!(editor.selected_sprite_group, vec![5]);
        assert!(editor.update_canvas_sprite_group(9, true));
        assert_eq!(editor.selected_sprite_group, vec![5, 9]);
        assert!(!editor.update_canvas_sprite_group(5, true));
        assert_eq!(editor.selected_sprite_group, vec![9]);
    }

    #[test]
    fn nearest_valid_group_delta_matches_lunar_magic_search_and_restart_order() {
        assert_eq!(
            nearest_valid_group_delta(&[(510, 0), (0, 25)], 5, 5, 512, 27),
            Some((1, 1))
        );
        assert_eq!(
            nearest_valid_group_delta(&[(10, 10)], 2, 3, 512, 27),
            Some((2, 3))
        );
        assert_eq!(
            nearest_valid_group_delta(&[(0, 0)], 0, 0, 512, 27),
            Some((0, 0))
        );
        assert_eq!(nearest_valid_group_delta(&[(0, 0)], -5, 0, 512, 27), None);
        assert_eq!(nearest_valid_group_delta(&[], 1, 1, 512, 27), Some((1, 1)));
    }

    #[test]
    fn right_drag_duplicates_and_moves_a_complete_object_group_atomically() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        let original = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .clone();
        let placements = original.native_placements();
        assert!(placements.len() >= 2);
        let selected = vec![placements[0].record_index, placements[1].record_index];
        let source_positions: Vec<_> = selected
            .iter()
            .map(|index| {
                let placement = placements
                    .iter()
                    .find(|placement| placement.record_index == *index)
                    .unwrap();
                (i32::from(placement.major), i32::from(placement.minor))
            })
            .collect();
        let (anchor_major, anchor_minor) = object_group_anchor(&original, &selected).unwrap();
        let initial_major_delta = if source_positions.iter().all(|(major, _)| *major < 500) {
            2
        } else {
            -2
        };
        let initial_minor_delta = if source_positions.iter().all(|(_, minor)| *minor < 24) {
            1
        } else {
            -1
        };
        let duplicate_position = egui::pos2(
            (anchor_major + initial_major_delta) as f32 + 0.5,
            (anchor_minor + initial_minor_delta) as f32 + 0.5,
        );
        editor.selected_object_group = selected.clone();
        editor.selected_object = selected[0];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.begin_secondary_duplicate_drag(duplicate_position, canvas, 1.0, false);
        assert!(editor.secondary_duplicate_drag);
        assert_eq!(editor.selected_object_group.len(), selected.len());
        assert!(editor.dragging_object.is_none());
        assert!(editor.object_group_drag.is_some_and(|drag| drag.secondary));

        let release_position = duplicate_position + egui::vec2(1.0, 1.0);
        editor.finish_secondary_duplicate_drag(Some(release_position), canvas, 1.0, false);
        assert!(!editor.secondary_duplicate_drag);
        assert!(editor.object_group_drag.is_none());
        assert!(editor.error.is_none(), "{:?}", editor.error);

        let updated = &editor.controller.as_ref().unwrap().level().layer1.objects;
        assert_eq!(
            updated.native_placements().len(),
            original.native_placements().len() + selected.len()
        );
        for (ordinal, selected_clone) in editor.selected_object_group.iter().enumerate() {
            let clone = updated
                .native_placements()
                .into_iter()
                .find(|placement| placement.record_index == *selected_clone)
                .unwrap();
            assert_eq!(
                (i32::from(clone.major), i32::from(clone.minor)),
                (
                    source_positions[ordinal].0 + initial_major_delta + 1,
                    source_positions[ordinal].1 + initial_minor_delta + 1,
                )
            );
        }
        let updated_placements = updated.native_placements();
        for (ordinal, source_index) in selected.iter().enumerate() {
            let source_record = &original.records[*source_index];
            assert!(updated_placements.iter().any(|placement| {
                (i32::from(placement.major), i32::from(placement.minor))
                    == source_positions[ordinal]
                    && updated.records[placement.record_index].command_id()
                        == source_record.command_id()
                    && updated.records[placement.record_index].parameter()
                        == source_record.parameter()
            }));
        }
    }

    #[test]
    fn right_drag_duplicates_and_moves_a_complete_sprite_group_atomically() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        let original = editor.controller.as_ref().unwrap().level().sprites.clone();
        let placements = original.native_placements();
        assert!(placements.len() >= 2);
        let selected = vec![placements[0].token_index, placements[1].token_index];
        let source_positions: Vec<_> = selected
            .iter()
            .map(|index| {
                let placement = placements
                    .iter()
                    .find(|placement| placement.token_index == *index)
                    .unwrap();
                (
                    i32::from(placement.major),
                    i32::from(placement.minor),
                    placement.sprite_number,
                )
            })
            .collect();
        let (anchor_major, anchor_minor) = sprite_group_anchor(&original, &selected).unwrap();
        let drag_major_delta = if source_positions.iter().all(|(major, _, _)| *major < 511) {
            1
        } else {
            -1
        };
        let drag_minor_delta = if source_positions.iter().all(|(_, minor, _)| *minor < 26) {
            1
        } else {
            -1
        };
        let duplicate_position = egui::pos2(anchor_major as f32 + 0.5, anchor_minor as f32 + 0.5);
        editor.selected_sprite_group = selected.clone();
        editor.selected_sprite = selected[0];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.begin_secondary_duplicate_drag(duplicate_position, canvas, 1.0, false);
        assert!(editor.secondary_duplicate_drag);
        assert_eq!(editor.selected_sprite_group.len(), selected.len());
        assert!(editor.dragging_sprite.is_none());
        assert!(
            editor
                .object_group_drag
                .is_some_and(|drag| drag.domain == CanvasEntitySelection::Sprite && drag.secondary)
        );

        editor.finish_secondary_duplicate_drag(
            Some(duplicate_position + egui::vec2(drag_major_delta as f32, drag_minor_delta as f32)),
            canvas,
            1.0,
            false,
        );
        assert!(!editor.secondary_duplicate_drag);
        assert!(editor.object_group_drag.is_none());
        assert!(editor.error.is_none(), "{:?}", editor.error);

        let updated = &editor.controller.as_ref().unwrap().level().sprites;
        assert_eq!(
            updated.native_placements().len(),
            placements.len() + selected.len()
        );
        for (ordinal, clone_index) in editor.selected_sprite_group.iter().enumerate() {
            let clone = updated
                .native_placements()
                .into_iter()
                .find(|placement| placement.token_index == *clone_index)
                .unwrap();
            assert_eq!(
                (
                    i32::from(clone.major),
                    i32::from(clone.minor),
                    clone.sprite_number,
                ),
                (
                    source_positions[ordinal].0 + drag_major_delta,
                    source_positions[ordinal].1 + drag_minor_delta,
                    source_positions[ordinal].2,
                )
            );
        }
        for source in source_positions {
            assert!(updated.native_placements().iter().any(|placement| {
                (
                    i32::from(placement.major),
                    i32::from(placement.minor),
                    placement.sprite_number,
                ) == source
            }));
        }
    }

    #[test]
    fn sprite_group_drag_falls_back_to_the_nearest_shared_in_bounds_delta() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let placements = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements();
        let selected = vec![placements[0].token_index, placements[1].token_index];
        let source_positions: Vec<_> = placements
            .iter()
            .take(2)
            .map(|placement| (i32::from(placement.major), i32::from(placement.minor)))
            .collect();
        let (anchor_major, anchor_minor) = sprite_group_anchor(
            &editor.controller.as_ref().unwrap().level().sprites,
            &selected,
        )
        .unwrap();
        let expected_delta = 511
            - source_positions
                .iter()
                .map(|position| position.0)
                .max()
                .unwrap();
        assert!(expected_delta < 511 - anchor_major);
        editor.selected_sprite_group = selected;
        editor.selected_sprite = editor.selected_sprite_group[0];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.object_group_drag = Some(CanvasObjectGroupDrag {
            domain: CanvasEntitySelection::Sprite,
            origin_major: anchor_major,
            origin_minor: anchor_minor,
            secondary: false,
        });
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        editor.finish_object_group_drag(
            Some(egui::pos2(511.5, anchor_minor as f32 + 0.5)),
            canvas,
            1.0,
            false,
        );
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let moved = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements();
        for (ordinal, selected) in editor.selected_sprite_group.iter().enumerate() {
            let placement = moved
                .iter()
                .find(|placement| placement.token_index == *selected)
                .unwrap();
            assert_eq!(
                i32::from(placement.major),
                source_positions[ordinal].0 + expected_delta
            );
            assert_eq!(i32::from(placement.minor), source_positions[ordinal].1);
        }
        assert_eq!(
            editor
                .selected_sprite_group
                .iter()
                .filter_map(|selected| moved
                    .iter()
                    .find(|placement| placement.token_index == *selected))
                .map(|placement| placement.major)
                .max(),
            Some(511)
        );
    }

    #[test]
    fn object_select_all_shortcut_excludes_controls_and_group_delete_is_atomic() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let original = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .clone();
        let expected = original
            .native_placements()
            .into_iter()
            .map(|placement| placement.record_index)
            .collect::<Vec<_>>();
        assert!(expected.len() > 1);
        editor.toolbar_edit_layer1();
        assert_eq!(
            editor.canvas_entity_selection,
            Some(CanvasEntitySelection::Layer1Object)
        );
        editor.toolbar_select_all();
        assert_eq!(editor.selected_object_group, expected);
        assert!(
            editor
                .selected_object_group
                .iter()
                .all(|index| original.records[*index].is_positioned_object())
        );

        editor.toolbar_delete_selection();
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let remaining = &editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records;
        assert!(
            remaining
                .iter()
                .all(|record| !record.is_positioned_object())
        );
        assert!(editor.selected_object_group.is_empty());
        assert_eq!(editor.canvas_entity_selection, None);

        editor.toolbar_edit_sprites();
        assert_eq!(
            editor.canvas_entity_selection,
            Some(CanvasEntitySelection::Sprite)
        );
        assert_eq!(editor.selected_sprite_group, vec![editor.selected_sprite]);
        editor.toolbar_edit_layer2();
        assert_eq!(editor.canvas_entity_selection, None);
    }

    #[test]
    fn object_group_shortcuts_duplicate_and_delete_the_complete_selection() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let original = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .clone();
        let placements = original.native_placements();
        editor.selected_object_group = vec![placements[0].record_index, placements[1].record_index];
        editor.selected_object = editor.selected_object_group[0];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);

        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Duplicate);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        assert_eq!(editor.selected_object_group.len(), 2);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .native_placements()
                .len(),
            placements.len() + 2
        );
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let restored = &editor.controller.as_ref().unwrap().level().layer1.objects;
        let semantic = |stream: &lm_level::ObjectStream| {
            let mut placements = stream
                .native_placements()
                .into_iter()
                .map(|placement| {
                    let record = &stream.records[placement.record_index];
                    (
                        placement.major,
                        placement.minor,
                        record.command_id(),
                        record.parameter(),
                    )
                })
                .collect::<Vec<_>>();
            placements.sort_unstable();
            placements
        };
        assert_eq!(semantic(restored), semantic(&original));
        assert!(editor.selected_object_group.is_empty());
        assert_eq!(editor.canvas_entity_selection, None);
    }

    #[test]
    fn insert_shortcut_places_one_active_object_template_at_the_pointer() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let original = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .clone();
        let placements = original.native_placements();
        editor.selected_object_group = vec![placements[0].record_index, placements[1].record_index];
        editor.selected_object = editor.selected_object_group[0];
        editor.object_form = ObjectForm::from_record(&original.records[editor.selected_object]);
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0, f32::from(NATIVE_LEVEL_MINOR_TILES)),
        );
        editor.apply_canvas_insert_shortcut(egui::pos2(10.5, 10.5), canvas, 1.0, false);

        assert!(editor.error.is_none(), "{:?}", editor.error);
        let updated = &editor.controller.as_ref().unwrap().level().layer1.objects;
        assert_eq!(
            updated.native_placements().len(),
            original.native_placements().len() + 1
        );
        let inserted = updated
            .native_placements()
            .into_iter()
            .find(|placement| placement.record_index == editor.selected_object)
            .unwrap();
        assert_eq!((inserted.major, inserted.minor), (10, 10));
        assert_eq!(editor.selected_object_group, vec![editor.selected_object]);
    }

    #[test]
    fn sprite_group_shortcuts_duplicate_and_delete_the_complete_selection() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let original = editor.controller.as_ref().unwrap().level().sprites.clone();
        let placements = original.native_placements();
        editor.selected_sprite_group = vec![placements[0].token_index, placements[1].token_index];
        editor.selected_sprite = editor.selected_sprite_group[0];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);

        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Duplicate);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        assert_eq!(editor.selected_sprite_group.len(), 2);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .native_placements()
                .len(),
            placements.len() + 2
        );
        editor.apply_canvas_entity_shortcut(CanvasEntityShortcut::Remove);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        assert_eq!(
            editor.controller.as_ref().unwrap().level().sprites,
            original
        );
        assert!(editor.selected_sprite_group.is_empty());
        assert_eq!(editor.canvas_entity_selection, None);
    }

    #[test]
    fn toolbar_coordinate_commands_nudge_objects_and_sprites_through_staged_history() {
        assert_eq!(screen_nudge_delta(false, 1, -2), (1, -2));
        assert_eq!(screen_nudge_delta(true, 1, -2), (-2, 1));

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );

        let original_level = editor.controller.as_ref().unwrap().level().clone();
        let object = original_level
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .find(|placement| placement.major < 511)
            .unwrap();
        editor.selected_object = object.record_index;
        editor.selected_object_group = vec![object.record_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.toolbar_nudge_selection(1, 0);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let moved_object = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements()
            .into_iter()
            .find(|placement| {
                editor
                    .selected_object_group
                    .contains(&placement.record_index)
            })
            .unwrap();
        assert_eq!(moved_object.major, object.major + 1);
        assert_eq!(moved_object.minor, object.minor);
        assert!(editor.controller.as_mut().unwrap().undo());
        assert_eq!(editor.controller.as_ref().unwrap().level(), &original_level);

        let sprite = original_level
            .sprites
            .native_placements()
            .into_iter()
            .find(|placement| placement.minor > 0)
            .unwrap();
        editor.selected_sprite = sprite.token_index;
        editor.selected_sprite_group = vec![sprite.token_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.toolbar_nudge_selection(0, -1);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let moved_sprite = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements()
            .into_iter()
            .find(|placement| {
                editor
                    .selected_sprite_group
                    .contains(&placement.token_index)
            })
            .unwrap();
        assert_eq!(moved_sprite.major, sprite.major);
        assert_eq!(moved_sprite.minor + 1, sprite.minor);

        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let reopened_sprite = reopened
            .sprites
            .native_placements()
            .into_iter()
            .find(|placement| {
                placement.major == moved_sprite.major
                    && placement.minor == moved_sprite.minor
                    && placement.sprite_number == moved_sprite.sprite_number
            });
        assert!(reopened_sprite.is_some());
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn toolbar_z_order_commands_preserve_positions_commit_reopen_and_undo() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );

        let original_level = editor.controller.as_ref().unwrap().level().clone();
        let object_placements = original_level.layer1.objects.native_placements();
        let object_pair = object_placements
            .windows(2)
            .find(|pair| {
                original_level.layer1.objects.records[pair[0].record_index]
                    != original_level.layer1.objects.records[pair[1].record_index]
            })
            .unwrap();
        let first_object =
            original_level.layer1.objects.records[object_pair[0].record_index].clone();
        let second_object =
            original_level.layer1.objects.records[object_pair[1].record_index].clone();
        let mut original_object_positions = object_placements
            .iter()
            .map(|placement| (placement.major, placement.minor))
            .collect::<Vec<_>>();
        original_object_positions.sort_unstable();
        editor.selected_object = object_pair[0].record_index;
        editor.selected_object_group = vec![object_pair[0].record_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.toolbar_z_order_step(true);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let reordered_objects = &editor.controller.as_ref().unwrap().level().layer1.objects;
        assert!(
            reordered_objects
                .records
                .iter()
                .position(|record| record == &first_object)
                .unwrap()
                > reordered_objects
                    .records
                    .iter()
                    .position(|record| record == &second_object)
                    .unwrap()
        );
        let mut reordered_object_positions = reordered_objects
            .native_placements()
            .iter()
            .map(|placement| (placement.major, placement.minor))
            .collect::<Vec<_>>();
        reordered_object_positions.sort_unstable();
        assert_eq!(reordered_object_positions, original_object_positions);
        assert!(editor.controller.as_mut().unwrap().undo());
        assert_eq!(editor.controller.as_ref().unwrap().level(), &original_level);

        let source_ordinal = object_placements
            .iter()
            .position(|placement| placement.record_index == object_pair[0].record_index)
            .unwrap();
        let overlap_target = object_placements[source_ordinal + 2];
        let overlap_target_object =
            original_level.layer1.objects.records[overlap_target.record_index].clone();
        editor.layer1_z_order_bounds = object_placements
            .iter()
            .enumerate()
            .map(|(ordinal, placement)| {
                let x = ordinal as f32 * 32.0;
                (
                    placement.record_index,
                    egui::Rect::from_min_max(egui::pos2(x, 0.0), egui::pos2(x + 10.0, 10.0)),
                )
            })
            .collect();
        let shared_bounds = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(10.0, 10.0));
        editor
            .layer1_z_order_bounds
            .insert(object_pair[0].record_index, shared_bounds);
        editor
            .layer1_z_order_bounds
            .insert(overlap_target.record_index, shared_bounds);
        editor.selected_object = object_pair[0].record_index;
        editor.selected_object_group = vec![object_pair[0].record_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.toolbar_overlap_z_order(ZOrderTraversal::Forward);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let overlap_reordered = &editor.controller.as_ref().unwrap().level().layer1.objects;
        assert!(
            overlap_reordered
                .records
                .iter()
                .position(|record| record == &first_object)
                .unwrap()
                > overlap_reordered
                    .records
                    .iter()
                    .position(|record| record == &overlap_target_object)
                    .unwrap()
        );

        let sprite_placements = original_level.sprites.native_placements();
        let sprite_pair = sprite_placements
            .windows(2)
            .find(|pair| {
                pair[0].screen == pair[1].screen
                    && original_level.sprites.tokens[pair[0].token_index]
                        != original_level.sprites.tokens[pair[1].token_index]
            })
            .unwrap();
        let first_sprite = original_level.sprites.tokens[sprite_pair[0].token_index].clone();
        let second_sprite = original_level.sprites.tokens[sprite_pair[1].token_index].clone();
        editor.selected_sprite = sprite_pair[0].token_index;
        editor.selected_sprite_group = vec![sprite_pair[0].token_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Sprite);
        editor.toolbar_z_order_step(true);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let reordered_sprites = &editor.controller.as_ref().unwrap().level().sprites;
        assert!(
            reordered_sprites
                .tokens
                .iter()
                .position(|token| token == &first_sprite)
                .unwrap()
                > reordered_sprites
                    .tokens
                    .iter()
                    .position(|token| token == &second_sprite)
                    .unwrap()
        );

        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert!(
            reopened
                .layer1
                .objects
                .records
                .iter()
                .position(|record| record == &first_object)
                .unwrap()
                > reopened
                    .layer1
                    .objects
                    .records
                    .iter()
                    .position(|record| record == &overlap_target_object)
                    .unwrap()
        );
        assert!(
            reopened
                .sprites
                .tokens
                .iter()
                .position(|token| token == &first_sprite)
                .unwrap()
                > reopened
                    .sprites
                    .tokens
                    .iter()
                    .position(|token| token == &second_sprite)
                    .unwrap()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn ssc_record_lengths_drive_native_sprite_framing() {
        let sidecar =
            lm_level::SscSidecar::decode(b"10\t50002\t0,0,10\n11\t60012\t0,0,11\n").unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let lengths = sprite_lengths_from_ssc(Some(&resolved)).unwrap();
        assert_eq!(lengths.record_len(&[0, 0, 0x10]), Some(5));
        assert_eq!(lengths.record_len(&[4, 0, 0x11]), Some(6));
        assert_eq!(lengths.record_len(&[8, 0, 0x12]), Some(3));
        assert_ne!(
            ssc_sprite_lengths_signature(Some(&resolved)),
            ssc_sprite_lengths_signature(None)
        );
    }

    #[test]
    fn conflicting_ssc_record_lengths_fail_before_level_decode() {
        let sidecar =
            lm_level::SscSidecar::decode(b"10\t40002\t0,0,10\n10\t50003\t0,0,10\n").unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let error = sprite_lengths_from_ssc(Some(&resolved)).unwrap_err();
        assert!(error.contains("conflicting record lengths 4 and 5"));
    }

    #[test]
    fn semantic_custom_sprite_edit_preserves_declared_extension_width() {
        let sidecar = lm_level::SscSidecar::decode(b"10\t50002\t0,0,10\n").unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let lengths = sprite_lengths_from_ssc(Some(&resolved)).unwrap();
        let token = SpriteToken::Record(lm_level::SpriteRecord {
            encoded: vec![0, 0, 0x10, 0xaa, 0xbb],
        });
        let mut form = SpriteForm::from_token(0, Some(&token));
        form.x = 7;
        let NativeLevelEdit::SetSpriteFields { index, fields } =
            form.semantic_edit(0, Some(&token), &lengths).unwrap()
        else {
            panic!("semantic edit must emit typed fields");
        };
        let mut stream = lm_level::NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![token],
        };
        let selected = stream
            .set_record_fields(index, fields, false, &lengths)
            .unwrap();
        let SpriteToken::Record(record) = &stream.tokens[selected] else {
            unreachable!();
        };
        assert_eq!(&record.encoded[3..], &[0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap().x, 7);
    }

    #[test]
    fn mixed_width_ssc_catalog_sprites_commit_reopen_and_undo_exactly() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let sidecar = lm_level::SscSidecar::decode(
            b"F0\t40002\t0,0,10\nF0\t50012\t0,0,11\nF0\t60022\t0,0,12\nF0\t70032\t0,0,13\n",
        )
        .unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let lengths = sprite_lengths_from_ssc(Some(&resolved)).unwrap();

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::ExpandRom(lm_app::RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::LoRom,
            target_logical_len: 0x10_0000,
            fill: 0xff,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        let expanded_baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(Some(&resolved)),
            },
            Some(&resolved),
        );
        assert!(editor.error.is_none(), "{:?}", editor.error);
        let initial_token_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .tokens
            .len();
        editor.selected_sprite = initial_token_count.saturating_sub(1);
        let mut expected = Vec::new();
        for (case, sprite) in resolved.sprites().iter().enumerate() {
            let declared_length = case + 4;
            editor.choose_custom_sprite(sprite.selector);
            assert_eq!(
                editor.sprite_catalog_preview_selector,
                Some(sprite.selector)
            );
            let SpriteToken::Record(chosen) =
                crate::native_level_document_form::parse_sprite_token(&editor.sprite_form.encoded)
                    .unwrap()
            else {
                panic!("SSC catalog selection must construct an ordinary sprite record");
            };
            assert_eq!(chosen.encoded.len(), declared_length);
            assert_eq!(chosen.native_fields().unwrap().sprite_number, 0xf0);
            assert_eq!(
                chosen.native_fields().unwrap().extra_bits,
                u8::try_from(case).unwrap()
            );
            assert!(chosen.encoded[3..].iter().all(|byte| *byte == 0));

            let token_count = initial_token_count + case;
            editor.insert_sprite(token_count);
            editor.sprite_form.x = 5 + u8::try_from(case).unwrap();
            editor.sprite_form.y_low = 0x18 + u8::try_from(case).unwrap();
            editor.apply_sprite_semantic_fields();
            let inserted = editor.selected_sprite;
            let SpriteToken::Record(staged) =
                &editor.controller.as_ref().unwrap().level().sprites.tokens[inserted]
            else {
                panic!("SSC insertion must remain an ordinary sprite record");
            };
            assert_eq!(staged.encoded.len(), declared_length);
            assert!(staged.encoded[3..].iter().all(|byte| *byte == 0));
            assert_eq!(
                editor.sprite_form.encoded,
                crate::level_editor_forms::format_bytes(&staged.encoded)
            );
            expected.push(staged.encoded.clone());
        }

        app.dispatch(prepare_commit(editor.controller.as_ref().unwrap(), &snapshot).unwrap())
            .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lengths,
            )
            .unwrap();
        for encoded in expected {
            assert!(reopened.sprites.tokens.iter().any(|token| {
                matches!(token, SpriteToken::Record(record) if record.encoded == encoded)
            }));
        }
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_bytes(),
            expanded_baseline
        );
    }

    #[test]
    fn raw_sprite_replacement_reloads_committed_semantic_fields() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = LevelController::decode(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let selected = controller
            .level()
            .sprites
            .tokens
            .iter()
            .position(|token| matches!(token, SpriteToken::Record(_)))
            .unwrap();
        let replacement = SpriteToken::Record(lm_level::SpriteRecord {
            encoded: vec![0xdb, 0x3e, 0x55],
        });
        let mut editor = VanillaLevelEditor {
            controller: Some(controller),
            selected_sprite: selected,
            sprite_form: SpriteForm {
                encoded: "stale raw text".into(),
                semantic_record: false,
                ..SpriteForm::default()
            },
            ..VanillaLevelEditor::default()
        };

        editor.apply_sprite_result(Ok(NativeLevelEdit::ReplaceSprite {
            index: selected,
            token: replacement,
        }));

        assert_eq!(editor.error, None);
        assert_eq!(editor.sprite_form.encoded, "DB 3E 55");
        assert_eq!(
            (
                editor.sprite_form.y_low,
                editor.sprite_form.extra_bits,
                editor.sprite_form.screen,
                editor.sprite_form.x,
                editor.sprite_form.sprite_number,
                editor.sprite_form.semantic_record,
            ),
            (0x1d, 2, 0x1e, 3, 0x55, true)
        );
    }

    #[test]
    fn semantic_legacy_sprite_position_edit_sorts_and_tracks_the_selected_record() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = LevelController::decode(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let selected = controller
            .level()
            .sprites
            .tokens
            .iter()
            .position(|token| matches!(token, SpriteToken::Record(_)))
            .unwrap();
        let mut editor = VanillaLevelEditor {
            sprite_form: SpriteForm::from_token(
                controller.level().sprites.header,
                controller.level().sprites.tokens.get(selected),
            ),
            controller: Some(controller),
            selected_sprite: selected,
            ..VanillaLevelEditor::default()
        };
        editor.sprite_form.screen = 0x1f;
        let edit = editor
            .sprite_form
            .semantic_edit(
                selected,
                editor
                    .controller
                    .as_ref()
                    .unwrap()
                    .level()
                    .sprites
                    .tokens
                    .get(selected),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let NativeLevelEdit::SetSpriteFields { index, fields } = edit else {
            unreachable!();
        };
        let mut expected = editor.controller.as_ref().unwrap().level().sprites.clone();
        let expected_selected = expected
            .set_record_fields(index, fields, false, &SpriteLengthTable::standard())
            .unwrap();

        editor.apply_sprite_semantic_fields();

        assert_eq!(editor.error, None);
        assert_eq!(editor.selected_sprite, expected_selected);
        assert_eq!(
            editor.controller.as_ref().unwrap().level().sprites,
            expected
        );
        assert_eq!(editor.sprite_form.screen, 0x1f);
        assert_eq!(
            crate::native_level_document_form::parse_sprite_token(&editor.sprite_form.encoded)
                .unwrap(),
            expected.tokens[expected_selected]
        );
    }

    #[test]
    fn expanded_sprite_canvas_edits_rebuild_controls_commit_reopen_and_undo() {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let original = lm_project::Project::new(image)
            .load_level_slot(0x105, layout, &SpriteLengthTable::standard())
            .unwrap();
        let record = original
            .sprites
            .tokens
            .iter()
            .find_map(|token| match token {
                SpriteToken::Record(record) => Some(record.clone()),
                SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
            })
            .unwrap();
        let expanded = lm_level::NativeSpriteStream {
            header: original.sprites.header,
            expanded: true,
            tokens: vec![SpriteToken::Screen(2), SpriteToken::Record(record)],
        };
        let encoded = expanded
            .encode_for_table(&SpriteLengthTable::standard())
            .unwrap();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let sprite_offset = layout
            .sprites
            .read_snes_pointer(&image, 0x105)
            .unwrap()
            .to_pc(Mapper::LoRom)
            .unwrap();
        bytes[sprite_offset..sprite_offset + encoded.len()].copy_from_slice(&encoded);
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        layout.expanded_sprites = true;

        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let baseline = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let controller =
            LevelController::decode(&snapshot, layout, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(controller.level().sprites.tokens, expanded.tokens);
        let mut editor = VanillaLevelEditor {
            controller: Some(controller),
            selected_sprite: 1,
            sprite_form: SpriteForm::from_token(expanded.header, expanded.tokens.get(1)),
            ..VanillaLevelEditor::default()
        };
        let cell = 8.0;
        let canvas = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(512.0 * cell, f32::from(NATIVE_LEVEL_MINOR_TILES) * cell),
        );
        editor
            .controller
            .as_mut()
            .unwrap()
            .apply_edits(&[NativeLevelEdit::InsertSprite {
                index: 1,
                token: SpriteToken::Control(0x90),
            }])
            .unwrap();
        assert!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .tokens
                .iter()
                .all(|token| !matches!(token, SpriteToken::Control(_)))
        );
        editor.move_sprite_to_canvas(1, egui::pos2(52.5 * cell, 6.5 * cell), canvas, cell, false);

        assert_eq!(editor.error, None);
        assert_eq!(editor.selected_sprite, 0);
        assert!(matches!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .tokens
                .as_slice(),
            [SpriteToken::Record(_)]
        ));
        let placement = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements()[0];
        assert_eq!(
            (placement.screen, placement.major, placement.minor),
            (3, 52, 6)
        );
        assert_eq!(editor.sprite_form.screen, 3);
        assert_eq!(editor.sprite_form.x, 4);
        assert_eq!(editor.sprite_form.y_low, 6);
        assert!(!editor.controller.as_ref().unwrap().level().sprites.expanded);
        assert!(!lm_level::NativeSpriteStream::header_uses_expanded_framing(
            editor.controller.as_ref().unwrap().level().sprites.header
        ));
        editor.placement_mode = Some(CanvasPlacementMode::Sprite);
        editor.place_sprite_at_canvas(egui::pos2(69.5 * cell, 7.5 * cell), canvas, cell, false);
        assert_eq!(editor.error, None);
        assert_eq!(editor.selected_sprite, 1);
        assert_eq!(editor.placement_mode, None);
        let placements = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements();
        assert_eq!(placements.len(), 2);
        assert!(!editor.controller.as_ref().unwrap().level().sprites.expanded);
        assert_eq!(
            (
                placements[1].screen,
                placements[1].major,
                placements[1].minor
            ),
            (4, 69, 7)
        );
        assert!(editor.has_sprite_only_changes());
        let expected_lmsw = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .encode_for_table(&SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(editor.lmsw_sprite_payload().unwrap(), expected_lmsw[1..]);

        let options = LevelSaveOptions {
            layer1_allocation: AllocationPolicy {
                search: 0x40_000..0x80_000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![],
            },
            sprite_allocation: AllocationPolicy {
                search: pristine_sprite_bank_range(
                    &RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap(),
                    layout,
                )
                .unwrap(),
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![],
            },
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let command = editor
            .controller
            .as_ref()
            .unwrap()
            .prepare_commit_with_shared_bank_sprite_relocation("Move expanded sprite", &options)
            .unwrap()
            .into_command();
        app.dispatch(command).unwrap();
        layout.expanded_sprites = editor.controller.as_ref().unwrap().level().sprites.expanded;
        let reopened = app
            .project()
            .unwrap()
            .load_level_slot(0x105, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(
            reopened.sprites,
            editor.controller.as_ref().unwrap().level().sprites
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), baseline);
    }

    #[test]
    fn custom_sprite_catalog_deduplicates_defaults_and_filters_descriptions() {
        let sidecar = lm_level::SscSidecar::decode(
            b"10\t50000\tCustom Koopa\n10\t50002\t0,0,10\n10\t50003\t0,0,11\n11\t60012\t0,0,12\n",
        )
        .unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let all = custom_sprite_catalog_entries(&resolved, "");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].selector.sprite_number, 0x10);
        assert_eq!(all[1].selector.extra_bits, 1);
        assert_eq!(
            custom_sprite_catalog_entries(&resolved, "koopa")[0]
                .selector
                .sprite_number,
            0x10
        );
        assert_eq!(
            custom_sprite_catalog_entries(&resolved, "11/1")[0]
                .selector
                .sprite_number,
            0x11
        );
    }

    #[test]
    fn custom_catalog_selection_materializes_declared_zero_filled_extension() {
        let sidecar = lm_level::SscSidecar::decode(b"10\t50002\t0,0,10\n").unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let lengths = sprite_lengths_from_ssc(Some(&resolved)).unwrap();
        let fields = NativeSpriteRecordFields {
            y_low: 4,
            extra_bits: 0,
            screen: 3,
            x: 2,
            sprite_number: 0x10,
        };
        let SpriteToken::Record(record) = custom_sprite_token(fields, &lengths).unwrap() else {
            panic!("custom catalog always materializes an ordinary record");
        };
        assert_eq!(record.encoded, vec![0x40, 0x23, 0x10, 0, 0]);
        assert_eq!(record.native_fields().unwrap(), fields);
    }

    #[test]
    fn catalog_preview_uses_current_packed_position_and_orientation() {
        let form = SpriteForm {
            y_low: 0x11,
            extra_bits: 2,
            screen: 0x1e,
            x: 0x0f,
            ..SpriteForm::default()
        };
        let mode = sprite_catalog_preview_mode(&form, true, 7, 5);
        assert_eq!(mode.placement_first, 0x1f);
        assert_ne!(
            mode.placement_first,
            packed_sprite_first(NativeSpriteRecordFields {
                y_low: form.y_low,
                extra_bits: form.extra_bits,
                screen: form.screen,
                x: form.x,
                sprite_number: form.sprite_number,
            })
        );
        assert_eq!(mode.placement_major, 0x1ef);
        assert_eq!(mode.placement_minor, 0x11);
        assert_eq!(mode.level_mode, 7);
        assert_eq!(mode.sprite_graphics_mode, 5);
        assert_eq!(
            mode.level_orientation,
            lm_render::StandardLevelOrientation::Vertical
        );
    }

    #[test]
    fn pristine_sprite_header_form_preserves_expanded_framing() {
        let mut form = SpriteForm::from_token(0x20, None);
        form.sprite_memory = 0x12;
        form.sprite_buoyancy_1 = true;
        assert_eq!(form.semantic_header().unwrap(), 0xb2);

        form.sprite_memory = 0x13;
        assert_eq!(
            form.semantic_header().unwrap_err(),
            "sprite memory setting must be in 00..=12, got 13"
        );
    }

    #[test]
    fn sprite_form_edits_packed_native_fields_without_raw_bytes() {
        let token = SpriteToken::Record(lm_level::SpriteRecord {
            encoded: vec![0x9a, 0xc7, 0x42],
        });
        let mut form = SpriteForm::from_token(7, Some(&token));
        assert_eq!(
            (
                form.y_low,
                form.extra_bits,
                form.screen,
                form.x,
                form.sprite_number,
            ),
            (9, 2, 23, 12, 0x42)
        );
        form.y_low = 0x1d;
        form.screen = 0x1e;
        form.x = 3;
        form.sprite_number = 0x55;
        assert_eq!(
            form.semantic_edit(4, Some(&token), &SpriteLengthTable::standard())
                .unwrap(),
            NativeLevelEdit::SetSpriteFields {
                index: 4,
                fields: NativeSpriteRecordFields {
                    y_low: 0x1d,
                    extra_bits: 2,
                    screen: 0x1e,
                    x: 3,
                    sprite_number: 0x55,
                },
            }
        );
    }

    #[test]
    fn sprite_form_rejects_semantic_edits_for_control_tokens() {
        let token = SpriteToken::Screen(7);
        let form = SpriteForm::from_token(0, Some(&token));
        assert!(!form.semantic_record);
        assert!(
            form.semantic_edit(0, Some(&token), &SpriteLengthTable::standard())
                .is_err()
        );
    }

    #[test]
    fn standard_sprite_preview_receives_level_orientation_and_mode() {
        let placement = lm_level::NativeSpritePlacement {
            token_index: 3,
            first_byte: 0x91,
            screen: 2,
            major: 0x24,
            minor: 9,
            sprite_number: 0xe5,
            extra_bits: 1,
        };
        let horizontal = standard_sprite_preview_mode(&placement, false, 3, 5, 1, 2, 4);
        assert_eq!(horizontal.placement_first, 0x94);
        assert_eq!(horizontal.placement_major, 0x24);
        assert_eq!(horizontal.placement_minor, 9);
        assert_eq!(horizontal.level_mode, 3);
        assert_eq!(horizontal.sprite_graphics_mode, 5);
        assert_eq!(horizontal.animation_phase, 2);
        assert_eq!(horizontal.sprite_8a_sequence_index, 4);
        assert_eq!(
            horizontal.wide_context,
            lm_render::StandardSpriteWideContext::ValidLong64
        );
        assert_eq!(
            horizontal.level_orientation,
            lm_render::StandardLevelOrientation::Horizontal
        );
        let vertical = standard_sprite_preview_mode(&placement, true, 7, 6, 0, 1, 2);
        assert_eq!(vertical.level_mode, 7);
        assert_eq!(vertical.sprite_graphics_mode, 6);
        assert_eq!(vertical.animation_phase, 1);
        assert_eq!(vertical.sprite_8a_sequence_index, 2);
        assert_eq!(
            vertical.wide_context,
            lm_render::StandardSpriteWideContext::ValidShort
        );
        assert_eq!(
            vertical.level_orientation,
            lm_render::StandardLevelOrientation::Vertical
        );
    }

    #[test]
    fn game_pixel_mode_keeps_sprite_canvas_hit_testing_active_without_editor_overlays() {
        let placement = lm_level::NativeSpritePlacement {
            token_index: 7,
            first_byte: 0,
            screen: 0,
            major: 4,
            minor: 6,
            sprite_number: 0x0f,
            extra_bits: 0,
        };
        let target = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 432.0));
        let (tile_x, tile_y) = presented_sprite_tile_coordinates(placement, false);
        let cursor = target.min
            + egui::vec2(
                (f32::from(tile_x) + 0.5) * ROM_LEVEL_CANVAS_CELL,
                (f32::from(tile_y) + 0.5) * ROM_LEVEL_CANVAS_CELL,
            );
        let context = egui::Context::default();
        let mut hit = None;
        let mut selected_only_hit = None;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                hit = draw_sprite_placements(SpritePlacementDraw {
                    painter: ui.painter(),
                    overlay_painter: ui.painter(),
                    target,
                    cell_size: ROM_LEVEL_CANVAS_CELL,
                    texture: None,
                    animated_texture: None,
                    placements: std::slice::from_ref(&placement),
                    cursor: Some(cursor),
                    selected_group: &[],
                    selected: 0,
                    vertical: false,
                    level_mode: 0,
                    sprite_tileset: 0,
                    sprite_memory_index: 0,
                    animation_phase: 0,
                    silver_pow_active: false,
                    custom_sprites: None,
                    custom_map16: None,
                    external_textures: &HashMap::new(),
                    editor_overlays: false,
                    selection_visible: false,
                    selected_only: false,
                })
                .hit;
                selected_only_hit = draw_sprite_placements(SpritePlacementDraw {
                    painter: ui.painter(),
                    overlay_painter: ui.painter(),
                    target,
                    cell_size: ROM_LEVEL_CANVAS_CELL,
                    texture: None,
                    animated_texture: None,
                    placements: std::slice::from_ref(&placement),
                    cursor: Some(cursor),
                    selected_group: &[],
                    selected: 99,
                    vertical: false,
                    level_mode: 0,
                    sprite_tileset: 0,
                    sprite_memory_index: 0,
                    animation_phase: 0,
                    silver_pow_active: false,
                    custom_sprites: None,
                    custom_map16: None,
                    external_textures: &HashMap::new(),
                    editor_overlays: false,
                    selection_visible: true,
                    selected_only: true,
                })
                .hit;
            });
        });
        assert_eq!(hit, Some(7));
        assert_eq!(selected_only_hit, None);
    }

    #[test]
    fn sprite_animation_clock_is_bounded_and_deterministic() {
        assert_eq!(sprite_animation_phase(f64::NAN), 0);
        assert_eq!(sprite_animation_phase(-1.0), 0);
        assert_eq!(sprite_animation_phase(0.0), 0);
        assert_eq!(sprite_animation_phase(0.124), 0);
        assert_eq!(sprite_animation_phase(0.125), 1);
        assert_eq!(sprite_animation_phase(0.375), 3);
        assert_eq!(sprite_animation_phase(0.5), 0);
    }

    #[test]
    fn map16_animation_clock_tracks_lunar_magics_sixty_millisecond_timer() {
        assert_eq!(map16_animation_phase(f64::NAN), 0);
        assert_eq!(map16_animation_phase(-1.0), 0);
        assert_eq!(map16_animation_phase(0.0), 0);
        assert_eq!(map16_animation_phase(0.059), 0);
        assert_eq!(map16_animation_phase(0.061), 1);
        assert_eq!(map16_animation_phase(0.421), 7);
        assert_eq!(map16_animation_phase(0.481), 0);
    }

    #[test]
    fn canvas_sprite_drag_maps_both_orientations_to_native_fields() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(512.0, 256.0));
        let original = NativeSpriteRecordFields {
            y_low: 1,
            extra_bits: 2,
            screen: 3,
            x: 4,
            sprite_number: 0x55,
        };
        let horizontal = sprite_fields_at_canvas_position(
            egui::pos2(10.0 + 35.5 * 8.0, 20.0 + 12.5 * 8.0),
            canvas,
            8.0,
            false,
            original,
        )
        .unwrap();
        assert_eq!(
            horizontal,
            NativeSpriteRecordFields {
                y_low: 12,
                extra_bits: 2,
                screen: 2,
                x: 3,
                sprite_number: 0x55,
            }
        );

        let vertical = sprite_fields_at_canvas_position(
            egui::pos2(10.0 + 7.5 * 8.0, 20.0 + 31.5 * 8.0),
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(256.0, 512.0)),
            8.0,
            true,
            original,
        )
        .unwrap();
        assert_eq!(vertical.y_low, 7);
        assert_eq!(vertical.screen, 1);
        assert_eq!(vertical.x, 15);
        assert_eq!(vertical.extra_bits, 2);
        assert_eq!(vertical.sprite_number, 0x55);

        let lower_subscreen = sprite_fields_at_canvas_position(
            egui::pos2(10.0 + 35.5 * 8.0, 20.0 + 6.5 * 8.0),
            canvas,
            8.0,
            false,
            NativeSpriteRecordFields {
                y_low: 0x11,
                ..original
            },
        )
        .unwrap();
        assert_eq!(lower_subscreen.y_low, 6);
    }

    #[test]
    fn canvas_sprite_drag_rejects_outside_and_unrepresentable_positions() {
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 600.0));
        let fields = NativeSpriteRecordFields {
            y_low: 0,
            extra_bits: 0,
            screen: 0,
            x: 0,
            sprite_number: 1,
        };
        assert!(
            sprite_fields_at_canvas_position(egui::pos2(-1.0, 1.0), canvas, 1.0, false, fields,)
                .is_none()
        );
        assert!(
            sprite_fields_at_canvas_position(egui::pos2(1.0, 27.0), canvas, 1.0, false, fields,)
                .is_none()
        );
        assert!(
            sprite_fields_at_canvas_position(egui::pos2(512.0, 1.0), canvas, 1.0, false, fields,)
                .is_none()
        );
    }

    #[test]
    fn canvas_object_drag_maps_screen_and_coordinates_in_both_orientations() {
        let horizontal_canvas =
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(512.0, 256.0));
        assert_eq!(
            object_placement_at_canvas_position(
                egui::pos2(10.0 + 35.5 * 8.0, 20.0 + 12.5 * 8.0),
                horizontal_canvas,
                8.0,
                false,
            ),
            Some((
                2,
                ObjectCoordinateNibbles {
                    first: 12,
                    second: 3,
                },
                false,
            ))
        );
        let vertical_canvas =
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(256.0, 512.0));
        assert_eq!(
            object_placement_at_canvas_position(
                egui::pos2(10.0 + 7.5 * 8.0, 20.0 + 31.5 * 8.0),
                vertical_canvas,
                8.0,
                true,
            ),
            Some((
                1,
                ObjectCoordinateNibbles {
                    first: 15,
                    second: 7,
                },
                false,
            ))
        );
    }

    #[test]
    fn canvas_object_drag_accepts_cross_screen_but_rejects_invalid_positions() {
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(512.0, 256.0));
        assert_eq!(
            object_placement_at_canvas_position(egui::pos2(31.0, 4.0), canvas, 1.0, false,),
            Some((
                1,
                ObjectCoordinateNibbles {
                    first: 4,
                    second: 15,
                },
                false,
            ))
        );
        assert_eq!(
            object_placement_at_canvas_position(egui::pos2(35.0, 16.0), canvas, 1.0, false,),
            Some((
                2,
                ObjectCoordinateNibbles {
                    first: 0,
                    second: 3,
                },
                true,
            ))
        );
        assert!(
            object_placement_at_canvas_position(egui::pos2(-1.0, 4.0), canvas, 1.0, false,)
                .is_none()
        );
    }

    #[test]
    fn object_insertion_supports_empty_and_selected_streams() {
        assert_eq!(object_insertion_index(0, 0), 0);
        assert_eq!(object_insertion_index(0, 3), 1);
        assert_eq!(object_insertion_index(2, 3), 3);
        assert_eq!(object_insertion_index(99, 3), 3);
    }

    #[test]
    fn overlap_z_order_traversal_skips_nonintersections_and_distinguishes_near_from_far() {
        let bounds = [
            (
                0,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(9.0, 9.0)),
            ),
            (
                1,
                egui::Rect::from_min_max(egui::pos2(2.0, 2.0), egui::pos2(8.0, 8.0)),
            ),
            (
                2,
                egui::Rect::from_min_max(egui::pos2(20.0, 20.0), egui::pos2(30.0, 30.0)),
            ),
            (
                3,
                egui::Rect::from_min_max(egui::pos2(4.0, 4.0), egui::pos2(12.0, 12.0)),
            ),
            (
                4,
                egui::Rect::from_min_max(egui::pos2(7.0, 7.0), egui::pos2(15.0, 15.0)),
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();
        let order = [0, 1, 2, 3, 4];
        assert_eq!(
            overlap_z_order_permutation(&order, &[1], &bounds, ZOrderTraversal::Forward, |_, _| {
                true
            },)
            .unwrap(),
            [0, 2, 3, 1, 4]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &[1],
                &bounds,
                ZOrderTraversal::Front,
                |_, _| true,
            )
            .unwrap(),
            [0, 2, 3, 4, 1]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &[3],
                &bounds,
                ZOrderTraversal::Backward,
                |_, _| true,
            )
            .unwrap(),
            [0, 3, 1, 2, 4]
        );
        assert_eq!(
            overlap_z_order_permutation(&order, &[3], &bounds, ZOrderTraversal::Back, |_, _| true,)
                .unwrap(),
            [3, 0, 1, 2, 4]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &[1],
                &bounds,
                ZOrderTraversal::Front,
                |_, right| *right != 4,
            )
            .unwrap(),
            [0, 2, 3, 1, 4],
            "an incompatible sprite sort group is not crossed"
        );
        assert!(!strict_rect_overlap(
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Rect::from_min_max(egui::pos2(1.0, 0.0), egui::pos2(2.0, 1.0)),
        ));
    }

    #[test]
    fn overlap_z_order_traversal_keeps_multi_selection_order_stable() {
        let order = [0, 1, 2, 3, 4, 5];
        let bounds = order
            .into_iter()
            .map(|identity| {
                (
                    identity,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(16.0, 16.0)),
                )
            })
            .collect::<HashMap<_, _>>();
        let selected = [1, 3];

        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &selected,
                &bounds,
                ZOrderTraversal::Forward,
                |_, _| true,
            )
            .unwrap(),
            [0, 2, 1, 4, 3, 5]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &selected,
                &bounds,
                ZOrderTraversal::Front,
                |_, _| true,
            )
            .unwrap(),
            [0, 2, 4, 5, 1, 3]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &selected,
                &bounds,
                ZOrderTraversal::Backward,
                |_, _| true,
            )
            .unwrap(),
            [1, 0, 3, 2, 4, 5]
        );
        assert_eq!(
            overlap_z_order_permutation(
                &order,
                &selected,
                &bounds,
                ZOrderTraversal::Back,
                |_, _| true,
            )
            .unwrap(),
            [1, 3, 0, 2, 4, 5]
        );
    }

    #[test]
    fn move_buttons_translate_to_pre_move_before_indexes() {
        assert_eq!(move_before_indexes(1, 4, false), Some((0, 0)));
        assert_eq!(move_before_indexes(1, 4, true), Some((3, 2)));
        assert_eq!(move_before_indexes(2, 3, true), None);
        assert_eq!(move_before_indexes(0, 3, false), None);
        assert_eq!(move_before_indexes(9, 3, false), None);
    }

    #[test]
    fn typed_entity_paste_builds_exact_insertions_and_rejects_cross_domain_data() {
        let object = ObjectRecord::new(vec![1, 2, 3]).unwrap();
        let object_text = crate::native_clipboard::encode_level_object(&object).unwrap();
        assert_eq!(
            pasted_object_edit(&object_text, 4).unwrap(),
            NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index: 4,
                record: object
            }])
        );

        let sprite = lm_level::SpriteRecord {
            encoded: vec![4, 5, 6],
        };
        let sprite_text = crate::native_clipboard::encode_level_sprite(&sprite).unwrap();
        assert_eq!(
            pasted_sprite_edit(&sprite_text, 2).unwrap(),
            NativeLevelEdit::InsertSprite {
                index: 2,
                token: SpriteToken::Record(sprite)
            }
        );
        assert!(pasted_object_edit(&sprite_text, 0).is_err());
        assert!(pasted_sprite_edit(&object_text, 0).is_err());
    }

    #[test]
    fn toolbar_group_copy_paste_and_cut_share_one_typed_atomic_object_path() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut editor = VanillaLevelEditor::default();
        editor.load(
            &snapshot,
            EditorKey {
                revision: snapshot.revision,
                level: 0x105,
                sprite_lengths_signature: ssc_sprite_lengths_signature(None),
            },
            None,
        );
        let placements = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .native_placements();
        let selected = vec![placements[0].record_index, placements[1].record_index];
        editor.canvas_entity_selection = Some(CanvasEntitySelection::Layer1Object);
        editor.selected_object = selected[1];
        editor.selected_object_group = selected.clone();
        let text = editor.toolbar_copy_selection().unwrap();
        let copied = crate::native_clipboard::decode_level_objects(&text).unwrap();
        assert_eq!(copied.len(), 2);

        let before = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .layer1
            .objects
            .records
            .len();
        editor.paste_object(&text, before);
        assert_eq!(editor.selected_object_group.len(), 2);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records
                .len(),
            before + 2
        );
        let cut = editor.toolbar_cut_selection().unwrap();
        assert_eq!(
            crate::native_clipboard::decode_level_objects(&cut).unwrap(),
            copied
        );
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .layer1
                .objects
                .records
                .len(),
            before
        );
        assert!(editor.toolbar_copy_selection().is_err());

        editor.toolbar_edit_sprites();
        let sprite_indexes = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .native_placements()
            .into_iter()
            .take(2)
            .map(|placement| placement.token_index)
            .collect::<Vec<_>>();
        assert_eq!(sprite_indexes.len(), 2);
        editor.selected_sprite = sprite_indexes[0];
        editor.selected_sprite_group = sprite_indexes;
        let sprites = editor.toolbar_copy_selection().unwrap();
        assert_eq!(
            crate::native_clipboard::decode_level_sprites(&sprites)
                .unwrap()
                .len(),
            2
        );
        let token_count = editor
            .controller
            .as_ref()
            .unwrap()
            .level()
            .sprites
            .tokens
            .len();
        editor.paste_sprite(&sprites, token_count);
        assert_eq!(
            editor
                .controller
                .as_ref()
                .unwrap()
                .level()
                .sprites
                .tokens
                .len(),
            token_count + 2
        );
    }

    #[test]
    fn external_sprite_texture_cache_materializes_complete_remapped_definition() {
        let source =
            lm_level::SscSidecar::decode(b"10\t2\t0,0,0\n10000\t0\t0-0,0\n20000\t0\t0-0,0\n")
                .unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&source);
        let sprite = table.default_display(0x10, 0).unwrap();
        let parts =
            lm_render::render_remapped_lunar_magic_custom_sprite_with(&table, sprite, |index| {
                (index == 0).then_some([0; 4])
            })
            .unwrap();
        assert_eq!(parts[0].graphics_base, 0x2000);
        let mut assets = lm_graphics::ExternalSpriteAssets::default();
        let opaque = lm_graphics::IndexedTile::new([1; lm_graphics::IndexedTile::PIXEL_COUNT]);
        assets
            .set_graphics_slot(0, &lm_graphics::encode_4bpp_tile(&opaque).unwrap())
            .unwrap();
        assets.set_rgb_palette(&[0, 0, 0, 255, 0, 0]).unwrap();
        let mut textures = HashMap::new();
        ensure_remapped_part_textures(
            &egui::Context::default(),
            &mut textures,
            &parts,
            SpriteRasterAssets {
                external: &assets,
                foreground_tiles: &[],
                layer3_tiles: &[],
                vanilla_tiles: &[],
                vanilla_palette: None,
            },
        );
        assert_eq!(textures.len(), parts.len());

        ensure_remapped_part_textures(
            &egui::Context::default(),
            &mut textures,
            &parts,
            SpriteRasterAssets {
                external: &lm_graphics::ExternalSpriteAssets::default(),
                foreground_tiles: &[],
                layer3_tiles: &[],
                vanilla_tiles: &[],
                vanilla_palette: None,
            },
        );
        assert_eq!(
            textures.len(),
            parts.len(),
            "existing textures remain stable until the owning asset revision invalidates them"
        );

        let vanilla_graphics_part = lm_render::RemappedCustomSpritePreviewTile {
            graphics_base: 0,
            ..parts[0]
        };
        ensure_remapped_part_textures(
            &egui::Context::default(),
            &mut textures,
            &[vanilla_graphics_part],
            SpriteRasterAssets {
                external: &assets,
                foreground_tiles: std::slice::from_ref(&opaque),
                layer3_tiles: &[],
                vanilla_tiles: &[],
                vanilla_palette: None,
            },
        );
        assert!(textures.contains_key(&vanilla_graphics_part));

        let vanilla_palette_part = lm_render::RemappedCustomSpritePreviewTile {
            palette_source: None,
            ..parts[0]
        };
        let mut colors = vec![lm_graphics::Bgr555::default(); 16 * 16];
        colors[8 * 16 + 1] = lm_graphics::Bgr555::from_rgb8(lm_graphics::Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        });
        ensure_remapped_part_textures(
            &egui::Context::default(),
            &mut textures,
            &[vanilla_palette_part],
            SpriteRasterAssets {
                external: &assets,
                foreground_tiles: &[],
                layer3_tiles: &[],
                vanilla_tiles: &[],
                vanilla_palette: Some(&lm_graphics::Palette { colors }),
            },
        );
        assert!(textures.contains_key(&vanilla_palette_part));
    }

    #[test]
    fn ssc_global_graphics_regions_route_foreground_and_sprite_tiles_separately() {
        let foreground = lm_graphics::IndexedTile::new([1; lm_graphics::IndexedTile::PIXEL_COUNT]);
        let sprite = lm_graphics::IndexedTile::new([2; lm_graphics::IndexedTile::PIXEL_COUNT]);
        let layer3 = lm_graphics::IndexedTile::new([3; lm_graphics::IndexedTile::PIXEL_COUNT]);
        let external = lm_graphics::ExternalSpriteAssets::default();
        let assets = SpriteRasterAssets {
            external: &external,
            foreground_tiles: std::slice::from_ref(&foreground),
            layer3_tiles: std::slice::from_ref(&layer3),
            vanilla_tiles: std::slice::from_ref(&sprite),
            vanilla_palette: None,
        };
        assert_eq!(resolve_ssc_graphics_tile(assets, 0), Some(&foreground));
        assert_eq!(resolve_ssc_graphics_tile(assets, 0x400), Some(&sprite));
        assert_eq!(resolve_ssc_graphics_tile(assets, 0x900), Some(&layer3));
        assert_eq!(resolve_ssc_graphics_tile(assets, 0xd00), None);
        assert_eq!(resolve_ssc_graphics_tile(assets, 0x2000), None);
    }

    #[test]
    fn native_canvas_draws_layer2_before_layer1() {
        let layer2_records = vec![ObjectRecord::new(vec![0, 0x10, 0]).unwrap()];
        let layer1_records = vec![ObjectRecord::new(vec![0, 0x20, 0]).unwrap()];
        let layer2_placements = lm_level::ObjectStream {
            records: layer2_records.clone(),
        }
        .native_placements();
        let layer1_placements = lm_level::ObjectStream {
            records: layer1_records.clone(),
        }
        .native_placements();
        let layers = object_draw_layers(
            &layer2_records,
            &layer2_placements,
            &layer1_records,
            &layer1_placements,
        );
        assert_eq!(layers[0].0[0].command_id(), 1);
        assert_eq!(layers[1].0[0].command_id(), 2);
    }

    #[test]
    fn compressed_layer2_index_matches_lunar_magic_row_major_halves() {
        assert_eq!(lm_level::native_layer2_tilemap_index(0, 0), Some(0));
        assert_eq!(lm_level::native_layer2_tilemap_index(1, 0), Some(1));
        assert_eq!(lm_level::native_layer2_tilemap_index(31, 15), Some(767));
        assert_eq!(lm_level::native_layer2_tilemap_index(0, 16), Some(256));
        assert_eq!(lm_level::native_layer2_tilemap_index(31, 31), Some(1023));
        assert_eq!(lm_level::native_layer2_tilemap_index(32, 0), None);
        assert_eq!(lm_level::native_layer2_tilemap_index(0, 32), None);
        let mut indexes = (0..32)
            .flat_map(|y| {
                (0..32).map(move |x| lm_level::native_layer2_tilemap_index(x, y).unwrap())
            })
            .collect::<Vec<_>>();
        indexes.sort_unstable();
        assert_eq!(indexes, (0..1024).collect::<Vec<_>>());
    }

    #[test]
    fn compressed_layer2_outer_flips_drive_atlas_uvs_and_custom_quadrants() {
        let normal = map16_atlas_uv(0x21, false, false);
        let flipped = map16_atlas_uv(0x21, true, true);
        assert_eq!(normal.left(), 1.0 / 32.0);
        assert_eq!(normal.right(), 2.0 / 32.0);
        assert_eq!(normal.top(), 1.0 / 16.0);
        assert_eq!(normal.bottom(), 2.0 / 16.0);
        assert_eq!(flipped.left(), normal.right());
        assert_eq!(flipped.right(), normal.left());
        assert_eq!(flipped.top(), normal.bottom());
        assert_eq!(flipped.bottom(), normal.top());

        let definition = lm_level::Map16Tile {
            top_left: lm_level::Subtile(0x0001),
            top_right: lm_level::Subtile(0x4002),
            bottom_left: lm_level::Subtile(0x8003),
            bottom_right: lm_level::Subtile(0xc004),
            acts_like: 0,
        };
        assert_eq!(
            map16_visual_subtiles(definition, false, false).map(|subtile| subtile.0),
            [0x0001, 0x4002, 0x8003, 0xc004]
        );
        assert_eq!(
            map16_visual_subtiles(definition, true, false).map(|subtile| subtile.0),
            [0x0002, 0x4001, 0x8004, 0xc003]
        );
        assert_eq!(
            map16_visual_subtiles(definition, false, true).map(|subtile| subtile.0),
            [0x0003, 0x4004, 0x8001, 0xc002]
        );
        assert_eq!(
            map16_visual_subtiles(definition, true, true).map(|subtile| subtile.0),
            [0x0004, 0x4003, 0x8002, 0xc001]
        );
    }

    #[test]
    fn native_layer2_canvas_wraps_the_snes_tilemap_plane() {
        for (x, y) in [(32, 0), (63, 31), (0, 32), (95, 79)] {
            assert_eq!(
                presented_layer2_tilemap_index(x, y, false),
                lm_level::native_layer2_tilemap_index(x % 32, y % 32)
            );
        }
        assert_eq!(
            presented_layer2_tilemap_index(27, 0, true),
            lm_level::native_layer2_tilemap_index(27, 0)
        );
        assert_eq!(
            presented_layer2_tilemap_index(53, 15, true),
            lm_level::native_layer2_tilemap_index(21, 15)
        );
    }

    #[test]
    fn background_canvas_height_does_not_expand_the_native_object_cache() {
        assert_eq!(native_object_cache_minor_tiles(16, false), 16);
        assert_eq!(native_object_cache_minor_tiles(27, false), 27);
        assert_eq!(native_object_cache_minor_tiles(32, false), 27);
        assert_eq!(native_object_cache_minor_tiles(27, true), 27);
        assert_eq!(native_object_cache_minor_tiles(32, true), 32);
    }

    #[test]
    fn retained_lunar_magic_level_105_supplies_complete_compressed_layer2_plane() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let project =
            lm_project::Project::new(lm_rom::RomImage::from_bytes(bytes).expect("fixture ROM"));
        let layer2 = project
            .load_level_layer2(0x105, 0, lm_profile::smw_us_v1_vanilla_layer2_layout())
            .expect("Lunar Magic Layer 2 fixture");
        let lm_level::NativeLayer2Data::Tilemap(bytes) = layer2 else {
            panic!("level 105 fixture must contain a compressed Layer 2 tilemap");
        };
        assert_eq!(bytes.len(), 0x800);
        assert_eq!(bytes.chunks_exact(2).count(), 0x400);
    }

    #[test]
    fn ordinary_ssc_palette_base_matches_zero_versus_nonzero_graphics_base() {
        let foreground = lm_graphics::Bgr555::from_rgb8(lm_graphics::Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let sprite = lm_graphics::Bgr555::from_rgb8(lm_graphics::Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        });
        let mut colors = vec![lm_graphics::Bgr555::default(); 16 * 16];
        colors[1] = foreground;
        colors[8 * 16 + 1] = sprite;
        let palette = lm_graphics::Palette { colors };
        assert_eq!(
            ordinary_ssc_palette_color(&palette, 0, 0, 1),
            Some(foreground.to_rgb8())
        );
        assert_eq!(
            ordinary_ssc_palette_color(&palette, 0x400, 0, 1),
            Some(sprite.to_rgb8())
        );
        assert_eq!(
            ordinary_ssc_palette_color(&palette, 0x30, 0, 1),
            Some(sprite.to_rgb8()),
            "the native renderer selects rows 8–15 for every nonzero graphics base"
        );
    }

    #[test]
    fn pristine_sprite_growth_relocates_in_the_shared_bank_and_reopens() {
        let _root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let mut controller =
            LevelController::decode(&snapshot, layout, &SpriteLengthTable::standard()).unwrap();
        let original_pointer = layout
            .sprites
            .read_snes_pointer(
                &RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap(),
                0x105,
            )
            .unwrap();
        let token = controller.level().sprites.tokens[0].clone();
        controller
            .apply_edits(&[NativeLevelEdit::InsertSprite { index: 1, token }])
            .unwrap();

        let command = prepare_commit(&controller, &snapshot).unwrap();
        app.dispatch(command).unwrap();

        let project = app.project().unwrap();
        assert_eq!(project.rom.logical_len(), 0x80_000);
        let relocated_pointer = layout
            .sprites
            .read_snes_pointer(&project.rom, 0x105)
            .unwrap();
        assert_ne!(relocated_pointer, original_pointer);
        assert_eq!(relocated_pointer.encode()[2], original_pointer.encode()[2]);
        let reopened = project
            .load_level_slot(0x105, layout, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(reopened.sprites, controller.level().sprites);
        let vertical =
            lm_profile::smw_us_v1_level_mode(reopened.layer1.header.level_mode()).vertical;
        let (placement, parts) = reopened
            .sprites
            .native_placements()
            .into_iter()
            .find_map(|placement| {
                let parts = lm_render::render_lunar_magic_standard_sprite_with_mode(
                    placement.sprite_number,
                    standard_sprite_preview_mode(
                        &placement,
                        vertical,
                        reopened.layer1.header.level_mode(),
                        reopened.layer1.header.sprite_tileset(),
                        reopened.sprites.header & 0x3f,
                        0,
                        0,
                    ),
                )?;
                parts
                    .iter()
                    .any(|part| part.x != 0 || part.y != 0)
                    .then_some((placement, parts))
            })
            .expect("pristine level 105 must retain a composite standard-sprite preview");
        let marker = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(ROM_LEVEL_CANVAS_CELL, ROM_LEVEL_CANVAS_CELL),
        );
        let bounds = sprite_preview_bounds(
            marker,
            parts.iter().map(|part| (part.x, part.y)),
            ROM_LEVEL_CANVAS_CELL,
        );
        assert_ne!(bounds, marker);
        assert!(reopened.sprites.tokens.get(placement.token_index).is_some());
        assert!(
            lm_rats::parse_at(
                project.rom.logical_bytes(),
                relocated_pointer.to_pc(Mapper::LoRom).unwrap() - lm_rats::HEADER_LEN
            )
            .is_ok()
        );
    }
}
