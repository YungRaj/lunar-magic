use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, LevelController, NativeLevelEdit, RomExpansionCommand,
    VanillaEntranceController,
};
use lm_level::{
    LegacyHeaderEdit, NativeSpriteRecordFields, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord,
    SeparateMidwayEntrance, SpriteLengthTable, SpriteToken,
};
use lm_project::LevelSaveOptions;
use lm_project::VanillaMainEntrance;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SnesPointer24, SupportedGame};
use std::collections::HashMap;

const ROM_LEVEL_CANVAS_CELL: f32 = 12.0;
const ROM_LEVEL_CANVAS_MIN_ZOOM: u16 = 50;
const ROM_LEVEL_CANVAS_MAX_ZOOM: u16 = 400;
const ROM_LEVEL_CANVAS_ZOOM_STEP: u16 = 25;
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
const ROM_LEVEL_TOOL_PANEL_WIDTH: f32 = 380.0;
const STANDARD_SPRITE_MAX: u8 = 0xed;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    level: u16,
    sprite_lengths_signature: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct HeaderForm {
    background_palette: u8,
    level_mode: u8,
    background_color: u8,
    sprite_tileset: u8,
    sprite_palette: u8,
    foreground_palette: u8,
    object_tileset: u8,
}

impl HeaderForm {
    fn from_controller(controller: &LevelController) -> Self {
        let header = controller.level().layer1.header;
        Self {
            background_palette: header.background_palette(),
            level_mode: header.level_mode(),
            background_color: header.background_color(),
            sprite_tileset: header.sprite_tileset(),
            sprite_palette: header.sprite_palette(),
            foreground_palette: header.foreground_palette(),
            object_tileset: header.object_tileset(),
        }
    }

    fn edits(self) -> [NativeLevelEdit; 7] {
        [
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(
                self.background_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(self.level_mode)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(self.background_color)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(self.sprite_tileset)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(self.sprite_palette)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(
                self.foreground_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(self.object_tileset)),
        ]
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanvasPlacementMode {
    Object,
    Sprite,
    Layer2Object,
    Layer2Tile,
}

impl SpriteForm {
    fn from_token(header: u8, token: Option<&SpriteToken>) -> Self {
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
            encoded,
            y_low: fields.y_low,
            extra_bits: fields.extra_bits,
            screen: fields.screen,
            x: fields.x,
            sprite_number: fields.sprite_number,
            semantic_record: token.is_some_and(|token| matches!(token, SpriteToken::Record(_))),
        }
    }

    fn semantic_edit(
        &self,
        index: usize,
        token: Option<&SpriteToken>,
        lengths: &SpriteLengthTable,
    ) -> Result<NativeLevelEdit, String> {
        let Some(SpriteToken::Record(record)) = token else {
            return Err("select a sprite record before applying semantic fields".into());
        };
        let mut record = record.clone();
        record
            .set_native_fields(
                NativeSpriteRecordFields {
                    y_low: self.y_low,
                    extra_bits: self.extra_bits,
                    screen: self.screen,
                    x: self.x,
                    sprite_number: self.sprite_number,
                },
                lengths,
            )
            .map_err(|error| error.to_string())?;
        Ok(NativeLevelEdit::ReplaceSprite {
            index,
            token: SpriteToken::Record(record),
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

#[derive(Default)]
pub(crate) struct VanillaLevelEditor {
    key: Option<EditorKey>,
    controller: Option<LevelController>,
    entrance_controller: Option<VanillaEntranceController>,
    entrance_form: VanillaMainEntrance,
    midway_form: Option<SeparateMidwayEntrance>,
    midway_install_form: SeparateMidwayEntrance,
    form: HeaderForm,
    selected_object: usize,
    object_form: ObjectForm,
    dragging_object: Option<usize>,
    dragging_layer2_object: Option<usize>,
    resizing_object: Option<usize>,
    resizing_layer2_object: Option<usize>,
    object_catalog_filter: String,
    custom_object_catalog_filter: String,
    object_placement_template: Option<ObjectRecord>,
    selected_sprite: usize,
    sprite_form: SpriteForm,
    dragging_sprite: Option<usize>,
    sprite_catalog_filter: String,
    custom_sprite_catalog_filter: String,
    canvas_zoom_percent: Option<u16>,
    tools_panel_visible: Option<bool>,
    game_preview: Option<bool>,
    snes_viewport: Option<bool>,
    preview_camera_major_offset: i16,
    preview_camera_minor_offset: i16,
    initial_vertical_scroll_tiles: Option<u16>,
    placement_mode: Option<CanvasPlacementMode>,
    paste_target: Option<EntityPasteTarget>,
    error: Option<String>,
    map16_key: Option<(u64, u16, u8, u8)>,
    map16_texture: Option<egui::TextureHandle>,
    background_map16_texture: Option<egui::TextureHandle>,
    animated_map16_textures: Vec<egui::TextureHandle>,
    animated_background_map16_textures: Vec<egui::TextureHandle>,
    animated_background_plane_textures: Vec<egui::TextureHandle>,
    shared_vanilla_background: bool,
    sprite_texture: Option<egui::TextureHandle>,
    entrance_texture: Option<egui::TextureHandle>,
    sprite_tiles: Vec<lm_graphics::IndexedTile>,
    foreground_tiles: Vec<lm_graphics::IndexedTile>,
    layer3_tiles: Vec<lm_graphics::IndexedTile>,
    layer3_low_texture: Option<egui::TextureHandle>,
    layer3_high_texture: Option<egui::TextureHandle>,
    layer3_position: Option<(i16, i16)>,
    sprite_palette: Option<lm_graphics::Palette>,
    canvas_backdrop: Option<lm_graphics::Bgr555>,
    foreground_texture: Option<egui::TextureHandle>,
    map16_summary: Option<Map16Summary>,
    map16_error: Option<String>,
    standard_object_map: Option<lm_profile::SmwUsV1StandardObjectDefinitionMap>,
    selected_layer2_tile: usize,
    layer2_word: u16,
    selected_layer2_object: usize,
    layer2_object_form: ObjectForm,
    layer2_object_placement_template: Option<ObjectRecord>,
    external_asset_revision: u64,
    external_sprite_textures:
        HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
}

impl VanillaLevelEditor {
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
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        external_asset_revision: u64,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
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
        if self.key != Some(key) {
            self.load(&snapshot, key, custom_sprites);
        }
        if self.external_asset_revision != external_asset_revision {
            self.external_asset_revision = external_asset_revision;
            self.external_sprite_textures.clear();
        }

        ui.heading(format!("Level {level:03X} — built-in SMW editor"));
        ui.label("Pristine SMW-US layout detected automatically.");
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            ui.colored_label(
                egui::Color32::RED,
                self.error.as_deref().unwrap_or("load failed"),
            );
            return None;
        };
        let object_count = controller.level().layer1.objects.records.len();
        let sprite_count = controller.level().sprites.tokens.len();
        let object_tileset = controller.level().layer1.header.object_tileset();
        let object_family = lm_profile::smw_us_v1_object_family(object_tileset);
        self.ensure_map16_assets(ui.ctx(), &snapshot, object_tileset);
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
        let mut pending_command = None;
        ui.horizontal_top(|ui| {
            if self.tools_panel_visible() {
                ui.allocate_ui_with_layout(
                    egui::vec2(tool_width, workspace_size.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("vanilla-level-tool-panel")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.show_staged_history(ui);
                                egui::CollapsingHeader::new("Level and entrance settings")
                                    .id_salt("vanilla-level-settings")
                                    .show(ui, |ui| {
                                        self.show_header_editor(ui, object_count, sprite_count);
                                        if pending_command.is_none() {
                                            pending_command = self.show_entrance_editor(ui, level);
                                        }
                                    });
                                self.show_layer2_editor(ui, custom_objects);
                                self.show_map16_preview(ui, object_tileset);
                                egui::CollapsingHeader::new("Layer 1 objects")
                                    .id_salt("vanilla-layer1-tools")
                                    .show(ui, |ui| {
                                        self.object_list(ui);
                                        self.object_editor(ui, custom_objects, custom_map16);
                                    });
                                egui::CollapsingHeader::new("Sprites")
                                    .id_salt("vanilla-sprite-tools")
                                    .show(ui, |ui| {
                                        self.sprite_list(ui);
                                        self.sprite_editor(
                                            ui,
                                            custom_sprites,
                                            external_assets,
                                            custom_map16,
                                        );
                                    });
                                if pending_command.is_none() {
                                    pending_command = self.show_commit_controls(ui, &snapshot);
                                }
                            });
                    },
                );
                ui.separator();
            }
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), workspace_size.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.object_canvas(
                        ui,
                        custom_sprites,
                        external_assets,
                        custom_objects,
                        custom_map16,
                    );
                },
            );
        });
        pending_command
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

    fn show_commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &lm_app::ControllerSnapshot,
    ) -> Option<Command> {
        ui.separator();
        let expanded = snapshot.rom_bytes.len() > 0x80_000;
        let relocation_needed = self.controller.as_ref().is_some_and(|controller| {
            controller.layer1_is_modified() || controller.layer2_is_modified()
        });
        if !expanded && relocation_needed {
            ui.label("Layer 1/2 relocation needs one expanded free-space bank.");
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
    ) {
        let Some(layer2) = self
            .controller
            .as_ref()
            .and_then(LevelController::layer2)
            .cloned()
        else {
            return;
        };
        ui.collapsing("Layer 2", |ui| match &layer2 {
            lm_level::NativeLayer2Data::Tilemap(bytes) => {
                let count = bytes.len() / 2;
                self.selected_layer2_tile = self.selected_layer2_tile.min(count.saturating_sub(1));
                ui.label(format!(
                    "Compressed 32×32 background tilemap · selected storage word {}",
                    self.selected_layer2_tile
                ));
                if layer2_tilemap_editable(self.shared_vanilla_background) {
                    ui.horizontal(|ui| {
                        ui.label("Map16 word");
                        ui.add(
                            egui::DragValue::new(&mut self.layer2_word).hexadecimal(4, false, true),
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
            }
            lm_level::NativeLayer2Data::Objects(objects) => {
                ui.label(format!(
                    "{} native Layer 2 object records are decoded and rendered.",
                    objects.objects.records.len()
                ));
                self.show_layer2_object_editor(ui, &objects.objects.records, custom_objects);
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
        ui.horizontal(|ui| {
            if ui.button("Place on canvas").clicked() {
                self.placement_mode = Some(CanvasPlacementMode::Layer2Object);
                self.error = None;
            }
            if ui.button("Insert after selection").clicked() {
                let result = self
                    .layer2_object_record_for_placement()
                    .and_then(|record| {
                        self.controller
                            .as_mut()
                            .ok_or_else(|| "level controller is unavailable".to_owned())?
                            .apply_layer2_object_edits(&[ObjectEdit::Insert {
                                index: object_insertion_index(
                                    self.selected_layer2_object,
                                    records.len(),
                                ),
                                record,
                            }])
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => {
                        if let Some(record) = self.controller.as_ref().and_then(|controller| {
                            controller
                                .layer2()
                                .and_then(|layer2| match layer2 {
                                    lm_level::NativeLayer2Data::Objects(objects) => Some(objects),
                                    lm_level::NativeLayer2Data::Tilemap(_) => None,
                                })
                                .and_then(|objects| {
                                    objects.objects.records.get(self.selected_layer2_object)
                                })
                        }) {
                            self.layer2_object_form = ObjectForm::from_record(record);
                            self.layer2_object_placement_template = Some(record.clone());
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
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
                self.selected_layer2_object = self
                    .selected_layer2_object
                    .min(records.len().saturating_sub(2));
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
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error),
                }
            }
            Err(error) => self.error = Some(error),
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
            header_row(ui, "Background color", &mut self.form.background_color, 7);
            header_row(ui, "Sprite tileset", &mut self.form.sprite_tileset, 15);
            header_row(
                ui,
                "Foreground palette",
                &mut self.form.foreground_palette,
                7,
            );
            header_row(ui, "Sprite palette", &mut self.form.sprite_palette, 7);
            header_row(ui, "Object tileset", &mut self.form.object_tileset, 15);
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.horizontal(|ui| {
            if ui.button("Stage header changes").clicked() {
                match self
                    .controller
                    .as_mut()
                    .expect("controller presence checked above")
                    .apply_edits(&self.form.edits())
                {
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error.to_string()),
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

    fn show_entrance_editor(&mut self, ui: &mut egui::Ui, level: u16) -> Option<Command> {
        ui.collapsing("Main entrance", |ui| {
            ui.label("Exact four-plane vanilla SMW entrance record.");
            egui::Grid::new("vanilla-main-entrance").show(ui, |ui| {
                header_row(ui, "Position", &mut self.entrance_form.position, u8::MAX);
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
        ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
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

    fn refresh_forms_after_history(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
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
        self.object_placement_template = None;
        self.dragging_object = None;
        self.dragging_layer2_object = None;
        self.resizing_object = None;
        self.resizing_layer2_object = None;
        self.external_sprite_textures.clear();
        self.dragging_sprite = None;
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
        match LevelController::decode_with_layer2(
            snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            layer2_layout,
            &sprite_lengths,
        ) {
            Ok(controller) => {
                let entrance_error = match VanillaEntranceController::decode_with_midway(
                    snapshot,
                    lm_profile::smw_us_v1_vanilla_entrance_layout(),
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
                self.initial_vertical_scroll_tiles = (!lm_profile::smw_us_v1_level_mode(
                    controller.level().layer1.header.level_mode(),
                )
                .vertical)
                    .then(|| vanilla_horizontal_entrance_scroll_row(self.entrance_form));
                self.preview_camera_major_offset = visual_smoke_camera_offset("MAJOR");
                self.preview_camera_minor_offset = visual_smoke_camera_offset("MINOR");
                self.midway_form = self
                    .entrance_controller
                    .as_ref()
                    .and_then(VanillaEntranceController::midway_entrance);
                self.midway_install_form = SeparateMidwayEntrance::default();
                self.standard_object_map = RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .ok()
                    .and_then(|rom| {
                        lm_profile::load_smw_us_v1_standard_object_definition_map(&rom).ok()
                    });
                self.shared_vanilla_background = RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .ok()
                    .and_then(|rom| {
                        lm_profile::smw_us_v1_level_uses_shared_background(
                            &rom,
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
                self.object_placement_template = None;
                self.selected_sprite = 0;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.first(),
                );
                self.controller = Some(controller);
                self.error = entrance_error;
            }
            Err(error) => {
                self.controller = None;
                self.entrance_controller = None;
                self.midway_form = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.entrance_controller = None;
        self.midway_form = None;
        self.midway_install_form = SeparateMidwayEntrance::default();
        self.preview_camera_major_offset = 0;
        self.preview_camera_minor_offset = 0;
        self.error = None;
        self.map16_key = None;
        self.map16_texture = None;
        self.background_map16_texture = None;
        self.animated_map16_textures.clear();
        self.animated_background_map16_textures.clear();
        self.animated_background_plane_textures.clear();
        self.shared_vanilla_background = false;
        self.sprite_texture = None;
        self.entrance_texture = None;
        self.sprite_tiles.clear();
        self.foreground_tiles.clear();
        self.layer3_tiles.clear();
        self.layer3_low_texture = None;
        self.layer3_high_texture = None;
        self.layer3_position = None;
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
        self.resizing_object = None;
        self.resizing_layer2_object = None;
    }

    #[allow(clippy::too_many_lines)]
    fn show_map16_preview(&mut self, ui: &mut egui::Ui, object_tileset: u8) {
        egui::CollapsingHeader::new(format!(
            "Pristine Map16 graphics — object tileset {object_tileset:X}"
        ))
        .default_open(true)
        .show(ui, |ui| {
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
    ) {
        let sprite_tileset = self.form.sprite_tileset;
        let level = self.controller.as_ref().map_or(0, |controller| {
            u16::try_from(controller.level().number).unwrap_or(0)
        });
        let key = (snapshot.revision, level, object_tileset, sprite_tileset);
        if self.map16_key == Some(key) {
            return;
        }
        self.map16_texture = None;
        self.background_map16_texture = None;
        self.animated_map16_textures.clear();
        self.animated_background_map16_textures.clear();
        self.animated_background_plane_textures.clear();
        self.sprite_texture = None;
        self.entrance_texture = None;
        self.sprite_tiles.clear();
        self.foreground_tiles.clear();
        self.layer3_tiles.clear();
        self.layer3_low_texture = None;
        self.layer3_high_texture = None;
        self.layer3_position = None;
        self.sprite_palette = None;
        self.canvas_backdrop = None;
        self.external_sprite_textures.clear();
        self.foreground_texture = None;
        self.map16_summary = None;
        self.map16_error = None;
        match crate::vanilla_map16_preview::render(
            snapshot.rom_bytes.clone(),
            level,
            self.controller
                .as_ref()
                .map_or_default(|controller| controller.level().layer1.header),
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
        ui.label(format!("Layer 1 objects — {}", family.display_name()));
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (index, record) in controller.level().layer1.objects.records.iter().enumerate()
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
                        self.object_placement_template = None;
                    }
                }
            });
    }

    #[allow(clippy::too_many_lines)]
    fn object_canvas(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
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
        let animation_phase = sprite_animation_phase(ui.input(|input| input.time));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(125));
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
        self.show_canvas_tools(ui, major_tiles, minor_tiles, vertical);
        let cell = if snes_viewport {
            fitted_snes_viewport_cell(ui.available_size(), self.canvas_zoom_percent())
        } else {
            self.canvas_cell()
        };
        let world_size = rom_canvas_size(major_tiles, minor_tiles, vertical, cell);
        let canvas_size = if snes_viewport {
            egui::vec2(16.0 * cell, 14.0 * cell)
        } else {
            world_size
        };
        if self.placement_mode.is_some() {
            ui.label("Click a canvas tile to place the values from the matching editor below.");
        }
        let mut scroll_area = egui::ScrollArea::both()
            .id_salt("vanilla-rom-level-canvas")
            .max_height(ui.available_height().max(160.0))
            .auto_shrink([false, false]);
        let requested_vertical_scroll = self
            .initial_vertical_scroll_tiles
            .map(|row| f32::from(row) * cell);
        if !snes_viewport && let Some(offset) = requested_vertical_scroll {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
        let scroll_output = scroll_area.show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let paint_rect = if snes_viewport {
                let (origin_x, origin_y) =
                    self.game_preview_camera_origin(major_tiles, minor_tiles, vertical);
                egui::Rect::from_min_size(
                    rect.min - egui::vec2(f32::from(origin_x) * cell, f32::from(origin_y) * cell),
                    world_size,
                )
            } else {
                rect
            };
            self.paint_object_canvas(
                &painter,
                &response,
                paint_rect,
                cell,
                major_tiles,
                minor_tiles,
                vertical,
                level_mode,
                animation_phase,
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
        });
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
    ) {
        ui.horizontal_wrapped(|ui| {
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
                    self.show_preview_camera_tools(ui, major_tiles, minor_tiles, vertical);
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
            self.canvas_zoom_percent = Some(clamp_canvas_zoom(zoom));
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
            ROM_LEVEL_CANVAS_CELL
        };
        base * f32::from(self.canvas_zoom_percent()) / 100.0
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
        animation_phase: u8,
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
        let animation_phase_index = usize::from(animation_phase);
        let map16_texture = self
            .animated_map16_textures
            .get(animation_phase_index)
            .or(self.map16_texture.as_ref());
        let background_map16_texture = self
            .animated_background_map16_textures
            .get(animation_phase_index)
            .or(self.background_map16_texture.as_ref());
        let background_plane_texture = self
            .animated_background_plane_textures
            .get(animation_phase_index);
        let game_camera = (self.game_preview() && self.snes_viewport())
            .then(|| self.game_preview_camera_origin(major_tiles, minor_tiles, vertical));
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
            self.shared_vanilla_background
                .then_some(())
                .and(background_map16_texture),
            self.shared_vanilla_background
                .then_some(())
                .and(background_plane_texture),
            self.foreground_texture.as_ref(),
            custom_map16,
            self.entrance_form,
            major_tiles,
            minor_tiles,
            vertical,
            game_camera,
        );
        // The object cache uses SMW's 0x1B0-byte 16×27 screen pages. The 32×32 Layer 2 plane may
        // enlarge the visible canvas, but its final five rows are not object-cache coordinates.
        let object_minor_tiles = native_object_cache_minor_tiles(minor_tiles, vertical);
        let layer2_artwork_bounds = self.draw_object_artwork(
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
        );
        let game_preview = self.game_preview();
        if game_preview
            && let (Some(texture), Some(position), Some(camera)) = (
                self.layer3_low_texture.as_ref(),
                self.layer3_position,
                game_camera,
            )
        {
            draw_wrapped_layer3_viewport(painter, rect, cell, texture, position, camera);
        }
        let layer1_artwork_bounds = self.draw_object_artwork(
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
        );
        let (hit_layer2, hit) = if game_preview {
            (
                ObjectPlacementHits::default(),
                ObjectPlacementHits::default(),
            )
        } else {
            let layer2_resize_models =
                self.active_object_resize_models(layer2_records, custom_objects);
            let layer1_resize_models = self.active_object_resize_models(records, custom_objects);
            (
                draw_object_placement_markers(
                    painter,
                    response,
                    rect,
                    vertical,
                    layer2_records,
                    layer2_placements,
                    self.selected_layer2_object,
                    map16_texture,
                    &layer2_artwork_bounds,
                    &layer2_resize_models,
                    cell,
                ),
                draw_object_placement_markers(
                    painter,
                    response,
                    rect,
                    vertical,
                    records,
                    placements,
                    self.selected_object,
                    map16_texture,
                    &layer1_artwork_bounds,
                    &layer1_resize_models,
                    cell,
                ),
            )
        };
        let hit_sprite = draw_sprite_placements(SpritePlacementDraw {
            painter,
            target: rect,
            cell_size: cell,
            texture: self.sprite_texture.as_ref(),
            placements: sprite_placements,
            cursor: response.interact_pointer_pos(),
            selected: self.selected_sprite,
            vertical,
            level_mode,
            animation_phase,
            custom_sprites,
            custom_map16,
            external_textures: &self.external_sprite_textures,
            editor_overlays: !game_preview,
        });
        if game_preview
            && let (Some(texture), Some(position), Some(camera)) = (
                self.layer3_high_texture.as_ref(),
                self.layer3_position,
                game_camera,
            )
        {
            draw_wrapped_layer3_viewport(painter, rect, cell, texture, position, camera);
        }
        // Paint the editor grid after the level artwork. Drawing opaque grid lines underneath
        // transparent Map16 pixels turns SMW's solid backdrop into a misleading checkerboard.
        if !game_preview {
            draw_object_grid(painter, rect, cell, major_tiles, minor_tiles, vertical);
            let alternate_vertical_layout =
                lm_profile::smw_us_v1_level_mode(level_mode).alternate_layer_layout;
            let level = self.controller.as_ref().map_or(0, |controller| {
                u16::try_from(controller.level().number).unwrap_or(0)
            });
            draw_primary_entrance_label(
                painter,
                rect,
                cell,
                level,
                self.entrance_form,
                vertical,
                alternate_vertical_layout,
            );
            draw_primary_entrance_position_warning(
                painter,
                rect,
                cell,
                self.entrance_form,
                vertical,
            );
            if let Some(texture) = self.entrance_texture.as_ref() {
                draw_primary_entrance_marker(
                    painter,
                    rect,
                    cell,
                    texture,
                    self.entrance_form,
                    vertical,
                    alternate_vertical_layout,
                );
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
            );
        }
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
    ) {
        if response.clicked()
            && let Some(mode) = self.placement_mode
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
        if response.clicked()
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            self.selected_object = index;
            self.object_form = ObjectForm::from_record(record);
            self.object_placement_template = None;
        }
        if response.clicked()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_object
            && let Some(record) = layer2_records.get(index)
        {
            self.selected_layer2_object = index;
            self.layer2_object_form = ObjectForm::from_record(record);
        }
        if response.clicked()
            && hit_object.is_none()
            && hit_layer2_object.is_none()
            && hit_sprite.is_none()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(index) = layer2_tile_at_canvas_position(position, rect, cell)
            && let Some(lm_level::NativeLayer2Data::Tilemap(bytes)) =
                self.controller.as_ref().and_then(LevelController::layer2)
            && let Some(word) = bytes.get(index * 2..index * 2 + 2)
        {
            self.selected_layer2_tile = index;
            self.layer2_word = u16::from_le_bytes([word[0], word[1]]);
        }
        if (response.clicked() || response.drag_started())
            && let Some(index) = hit_sprite
            && let Some(controller) = &self.controller
        {
            self.selected_sprite = index;
            self.sprite_form = SpriteForm::from_token(
                controller.level().sprites.header,
                controller.level().sprites.tokens.get(index),
            );
            if response.drag_started() {
                self.dragging_sprite = Some(index);
            }
        }
        if response.drag_started()
            && hit_sprite.is_none()
            && let Some(index) = hit_object_resize
        {
            self.resizing_object = Some(index);
            self.selected_object = index;
            if let Some(record) = records.get(index) {
                self.object_form = ObjectForm::from_record(record);
                self.object_placement_template = None;
            }
        } else if response.drag_started()
            && hit_sprite.is_none()
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            self.dragging_object = Some(index);
            self.selected_object = index;
            self.object_form = ObjectForm::from_record(record);
            self.object_placement_template = None;
        }
        if response.drag_started()
            && hit_sprite.is_none()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_resize
        {
            self.resizing_layer2_object = Some(index);
            self.selected_layer2_object = index;
            if let Some(record) = layer2_records.get(index) {
                self.layer2_object_form = ObjectForm::from_record(record);
            }
        } else if response.drag_started()
            && hit_sprite.is_none()
            && hit_object.is_none()
            && let Some(index) = hit_layer2_object
            && let Some(record) = layer2_records.get(index)
        {
            self.dragging_layer2_object = Some(index);
            self.selected_layer2_object = index;
            self.layer2_object_form = ObjectForm::from_record(record);
        }
        if response.drag_stopped() {
            let position = response.interact_pointer_pos();
            if let (Some(index), Some(position)) = (self.resizing_object.take(), position) {
                self.resize_object_to_canvas(index, position, rect, cell, vertical);
            } else if let (Some(index), Some(position)) =
                (self.resizing_layer2_object.take(), position)
            {
                self.resize_layer2_object_to_canvas(index, position, rect, cell, vertical);
            } else if let (Some(index), Some(position)) = (self.dragging_sprite.take(), position) {
                self.move_sprite_to_canvas(index, position, rect, cell, vertical);
            } else if let (Some(index), Some(position)) = (self.dragging_object.take(), position) {
                self.move_object_to_canvas(index, position, rect, cell, vertical);
            } else if let (Some(index), Some(position)) =
                (self.dragging_layer2_object.take(), position)
            {
                self.move_layer2_object_to_canvas(index, position, rect, cell, vertical);
            }
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
            Ok(record) if record.command_id() != 0 => record,
            Ok(_) => {
                self.error =
                    Some("canvas placement requires an ordinary nonzero object command".into());
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
                self.object_form =
                    ObjectForm::from_record(&controller.level().layer1.objects.records[selected]);
                self.object_placement_template = None;
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
            Ok(record) if record.command_id() != 0 => record,
            Ok(_) => {
                self.error =
                    Some("canvas placement requires an ordinary nonzero Layer 2 command".into());
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
        let lengths = self
            .controller
            .as_ref()
            .map_or_else(SpriteLengthTable::standard, |controller| {
                controller.sprite_lengths().clone()
            });
        let token = match crate::native_level_document_form::parse_sprite_token(
            &self.sprite_form.encoded,
        ) {
            Ok(SpriteToken::Record(mut record)) => {
                if let Err(error) = record.set_native_fields(fields, &lengths) {
                    self.error = Some(error.to_string());
                    return;
                }
                SpriteToken::Record(record)
            }
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
        let index = controller.level().sprites.tokens.len();
        let mut predicted = controller.level().sprites.clone();
        predicted.tokens.push(token.clone());
        let selected = match predicted.sort_legacy_records_by_screen(index) {
            Ok(selected) => selected,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[
            NativeLevelEdit::InsertSprite { index, token },
            NativeLevelEdit::SortLegacySpritesByScreen { selected: index },
        ]) {
            Ok(()) => {
                self.selected_sprite = selected;
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
                self.object_form =
                    ObjectForm::from_record(&controller.level().layer1.objects.records[new_index]);
                self.object_placement_template = None;
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
                self.object_form =
                    ObjectForm::from_record(&controller.level().layer1.objects.records[index]);
                self.object_placement_template = None;
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
        let token = controller.level().sprites.tokens.get(index);
        let Ok(replacement) =
            self.sprite_form
                .semantic_edit(index, token, controller.sprite_lengths())
        else {
            self.error = Some("selected sprite cannot be moved semantically".into());
            return;
        };
        let NativeLevelEdit::ReplaceSprite { token, .. } = &replacement else {
            unreachable!("semantic sprite edit is always a replacement");
        };
        let mut predicted = controller.level().sprites.clone();
        predicted.tokens[index] = token.clone();
        let new_index = match predicted.sort_legacy_records_by_screen(index) {
            Ok(index) => index,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[
            replacement,
            NativeLevelEdit::SortLegacySpritesByScreen { selected: index },
        ]) {
            Ok(()) => {
                self.selected_sprite = new_index;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(new_index),
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
    ) -> HashMap<usize, egui::Rect> {
        let Some(texture) = self.map16_texture.as_ref() else {
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
                custom_map16,
                foreground_texture: self.foreground_texture.as_ref(),
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
        self.object_catalog(ui);
        self.custom_object_catalog(ui, custom_objects, custom_map16);
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
                "Values below 1000 use the compact four-byte form; higher flag values use Lunar Magic's five-byte extended form.",
            );
        } else if let Some((encoding, target)) = &mut self.object_form.screen_jump {
            ui.label(format!(
                "Screen-jump control ({})",
                match encoding {
                    lm_level::ScreenJumpEncoding::FirstLow => "low byte first",
                    lm_level::ScreenJumpEncoding::FirstHigh => "high byte first",
                }
            ));
            egui::Grid::new("vanilla-screen-jump-fields").show(ui, |ui| {
                ui.label("Packed target");
                ui.add(
                    egui::DragValue::new(target)
                        .range(0..=0x1f1f)
                        .hexadecimal(4, false, true),
                );
                ui.end_row();
            });
            ui.small("Only bits representable by the selected native encoding are accepted.");
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
        if self.paste_target == Some(EntityPasteTarget::Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.paste_object(&text, record_count);
        }
    }

    fn object_catalog(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Add standard object visually")
            .id_salt("vanilla-standard-object-catalog")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Hex filter");
                    ui.text_edit_singleline(&mut self.object_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.object_catalog_filter.clear();
                    }
                });
                ui.label("Choose a tileset-resolved object, then click its destination tile.");
                let commands = object_catalog_commands(&self.object_catalog_filter);
                let texture = self.map16_texture.clone();
                let handler_map = self.active_standard_object_handler_map().copied();
                let Some(handler_map) = handler_map else {
                    ui.label("The active standard-object handler map is unavailable.");
                    return;
                };
                let Some(definitions) = standard_object_definitions() else {
                    ui.label("The recovered standard-object definitions are unavailable.");
                    return;
                };
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-standard-object-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for command in commands {
                                let response = draw_object_catalog_entry(
                                    ui,
                                    texture.as_ref(),
                                    command,
                                    &handler_map,
                                    &definitions,
                                    command == self.object_form.command_id,
                                );
                                if response.clicked() {
                                    chosen = Some(command);
                                }
                            }
                        });
                    });
                if let Some(command) = chosen {
                    self.object_form.command_id = command;
                    self.object_form.parameter = 0;
                    self.object_form.advances_screen = false;
                    self.object_placement_template = None;
                    self.placement_mode = Some(CanvasPlacementMode::Object);
                    self.error = None;
                }
            });
    }

    fn custom_object_catalog(
        &mut self,
        ui: &mut egui::Ui,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let Some(custom_objects) = custom_objects else {
            return;
        };
        egui::CollapsingHeader::new("Add custom OSC object visually")
            .id_salt("vanilla-custom-object-catalog")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
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
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-custom-object-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for entry in entries {
                                let response = draw_custom_object_catalog_entry(
                                    ui,
                                    map16_texture.as_ref(),
                                    foreground_texture.as_ref(),
                                    custom_map16,
                                    entry,
                                );
                                if response.clicked() {
                                    chosen = Some(entry.selector);
                                }
                            }
                        });
                    });
                if let Some(selector) = chosen {
                    match custom_object_native_record(selector) {
                        Ok(record) => {
                            self.object_form = ObjectForm::from_record(&record);
                            self.object_placement_template = Some(record);
                            self.placement_mode = Some(CanvasPlacementMode::Object);
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            });
    }

    fn object_action_buttons(
        &mut self,
        ui: &mut egui::Ui,
        record_count: usize,
        has_selection: bool,
    ) {
        ui.horizontal(|ui| {
            if ui.button("Insert after selection").clicked() {
                let edit = self.object_form.ordinary_record().map(|record| {
                    NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                        index: object_insertion_index(self.selected_object, record_count),
                        record,
                    }])
                });
                self.apply_object_result(edit);
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
                        Ok(()) => self.error = None,
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
                && let Some(controller) = self.controller.as_mut()
            {
                match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
                    ObjectEdit::Remove {
                        index: self.selected_object,
                    },
                ])]) {
                    Ok(()) => {
                        self.selected_object =
                            self.selected_object.min(record_count.saturating_sub(2));
                        if let Some(record) = controller
                            .level()
                            .layer1
                            .objects
                            .records
                            .get(self.selected_object)
                        {
                            self.object_form = ObjectForm::from_record(record);
                            self.object_placement_template = None;
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
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
        });
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

    fn apply_object_result(&mut self, edit: Result<NativeLevelEdit, String>) {
        match edit {
            Ok(edit) => {
                if let Some(controller) = self.controller.as_mut() {
                    match controller.apply_edits(&[edit]) {
                        Ok(()) => {
                            if let Some(record) = controller
                                .level()
                                .layer1
                                .objects
                                .records
                                .get(self.selected_object)
                            {
                                self.object_form = ObjectForm::from_record(record);
                                self.object_placement_template = None;
                            }
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn sprite_list(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = &self.controller else {
            return;
        };
        ui.label("Sprites");
        let placements = controller.level().sprites.native_placements();
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for (index, token) in controller.level().sprites.tokens.iter().enumerate() {
                    let text =
                        SpriteForm::from_token(controller.level().sprites.header, Some(token))
                            .encoded;
                    let semantic = placements
                        .iter()
                        .find(|placement| placement.token_index == index)
                        .map_or_else(String::new, |placement| {
                            format!(
                                "sprite {:02X} @ {}:{:X},{} · ",
                                placement.sprite_number,
                                placement.screen,
                                placement.major & 0x0f,
                                placement.minor
                            )
                        });
                    if ui
                        .selectable_label(
                            index == self.selected_sprite,
                            format!("{index:03}: {semantic}{text}"),
                        )
                        .clicked()
                    {
                        self.selected_sprite = index;
                        self.sprite_form =
                            SpriteForm::from_token(controller.level().sprites.header, Some(token));
                    }
                }
            });
    }

    fn sprite_editor(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        external_assets: &lm_graphics::ExternalSpriteAssets,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let token_count = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.level().sprites.tokens.len());
        ui.label("Native sprite stream");
        self.sprite_catalog(ui);
        self.custom_sprite_catalog(ui, custom_sprites, external_assets, custom_map16);
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
        ui.horizontal(|ui| {
            if ui.button("Stage sprite header").clicked()
                && let Some(controller) = self.controller.as_mut()
            {
                match controller
                    .apply_edits(&[NativeLevelEdit::SetSpriteHeader(self.sprite_form.header)])
                {
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error.to_string()),
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
        egui::CollapsingHeader::new("Add standard sprite visually")
            .id_salt("vanilla-standard-sprite-catalog")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Hex filter");
                    ui.text_edit_singleline(&mut self.sprite_catalog_filter);
                    if ui.button("Clear").clicked() {
                        self.sprite_catalog_filter.clear();
                    }
                });
                ui.label(
                    "Choose a recovered standard-sprite preview, then click its destination tile.",
                );
                let ids = sprite_catalog_ids(&self.sprite_catalog_filter);
                let texture = self.sprite_texture.clone();
                let (vertical, level_mode) =
                    self.controller.as_ref().map_or((false, 0), |controller| {
                        let header = &controller.level().layer1.header;
                        (
                            lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical,
                            header.level_mode(),
                        )
                    });
                let mode = sprite_catalog_preview_mode(&self.sprite_form, vertical, level_mode);
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-standard-sprite-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for id in ids {
                                let response = draw_sprite_catalog_entry(
                                    ui,
                                    texture.as_ref(),
                                    id,
                                    mode,
                                    id == self.sprite_form.sprite_number,
                                );
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
        let Some(custom_sprites) = custom_sprites else {
            return;
        };
        egui::CollapsingHeader::new("Add custom SSC sprite visually")
            .id_salt("vanilla-custom-sprite-catalog")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
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
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .id_salt("vanilla-custom-sprite-catalog-scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for entry in entries {
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
                                let response = draw_custom_sprite_catalog_entry(
                                    ui,
                                    texture.as_ref(),
                                    entry,
                                    atlas_parts.as_deref(),
                                    external_parts.as_deref(),
                                    &self.external_sprite_textures,
                                );
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
                self.sprite_form = SpriteForm::from_token(self.sprite_form.header, Some(&token));
                self.placement_mode = Some(CanvasPlacementMode::Sprite);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn choose_standard_sprite(&mut self, sprite_number: u8) {
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
            header_row(ui, "Sprite memory", &mut self.sprite_form.header, 0xff);
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
        self.apply_sprite_result(edit);
        if self.error.is_none()
            && let Some(controller) = self.controller.as_ref()
        {
            self.sprite_form = SpriteForm::from_token(
                controller.level().sprites.header,
                controller.level().sprites.tokens.get(self.selected_sprite),
            );
        }
    }

    fn apply_sprite_result(&mut self, edit: Result<NativeLevelEdit, String>) {
        match edit {
            Ok(edit) => {
                if let Some(controller) = self.controller.as_mut() {
                    match controller.apply_edits(&[edit]) {
                        Ok(()) => self.error = None,
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
        match controller.apply_edits(&[NativeLevelEdit::InsertSprite { index, token }]) {
            Ok(()) => {
                self.selected_sprite = index;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(index),
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
        let edit = match pasted_object_edit(text, index) {
            Ok(edit) => edit,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[edit]) {
            Ok(()) => {
                self.selected_object = index;
                if let Some(record) = controller.level().layer1.objects.records.get(index) {
                    self.object_form = ObjectForm::from_record(record);
                    self.object_placement_template = None;
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn paste_layer2_object(&mut self, text: &str, record_count: usize) {
        let index = object_insertion_index(self.selected_layer2_object, record_count);
        let record = match crate::native_clipboard::decode_level_object(text) {
            Ok(record) => record,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_layer2_object_edits(&[ObjectEdit::Insert { index, record }]) {
            Ok(()) => {
                self.selected_layer2_object = index;
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
        let edit = match pasted_sprite_edit(text, index) {
            Ok(edit) => edit,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        match controller.apply_edits(&[edit]) {
            Ok(()) => {
                self.selected_sprite = index;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.get(index),
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

fn object_insertion_index(selected: usize, record_count: usize) -> usize {
    selected.saturating_add(1).min(record_count)
}

fn pasted_object_edit(text: &str, index: usize) -> Result<NativeLevelEdit, String> {
    crate::native_clipboard::decode_level_object(text)
        .map(|record| NativeLevelEdit::Objects(vec![ObjectEdit::Insert { index, record }]))
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
    Some((major_end.clamp(16, 512), minor_end.clamp(16, 32)))
}

fn clamp_canvas_zoom(zoom: u16) -> u16 {
    zoom.clamp(ROM_LEVEL_CANVAS_MIN_ZOOM, ROM_LEVEL_CANVAS_MAX_ZOOM)
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
        .min(1.0);
    let origin = target.center() - egui::vec2(width * scale, height * scale) / 2.0;
    for part in parts {
        let position = origin
            + egui::vec2(
                f32::from(part.x.saturating_sub(min_x)) * scale,
                f32::from(part.y.saturating_sub(min_y)) * scale,
            );
        let tile_rect = egui::Rect::from_min_size(position, egui::vec2(16.0 * scale, 16.0 * scale));
        let definition = match custom_map16 {
            Some(lm_app::NativeMap16SidecarDocument::M16(sidecar)) => {
                sidecar.tile(usize::from(part.tile & 0x3fff))
            }
            Some(lm_app::NativeMap16SidecarDocument::S16(_)) | None => None,
        };
        if let (Some(definition), Some(texture)) = (definition, foreground_texture) {
            draw_custom_map16_tile(painter, texture, tile_rect, definition);
        } else if part.tile < 0x200
            && let Some(texture) = map16_texture
        {
            draw_map16_atlas_tile(painter, texture, tile_rect, part.tile);
        }
    }
}

fn standard_object_definitions() -> Option<lm_render::StandardObjectDefinitionSet> {
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).ok()?;
    lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).ok()?;
    Some(definitions)
}

fn draw_object_catalog_entry(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
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
    if let Some(texture) = texture
        && let Some(tiles) = object_catalog_tiles(command, handler_map, definitions)
    {
        draw_fitted_object_catalog_preview(&painter, texture, preview_rect, &tiles);
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
    let layout = lm_render::NativeLevelMap16Layout {
        width: 16,
        height: 16,
        page_stride: 0x1b0,
        base_cell: 0,
        vertical: false,
    };
    let report = lm_render::render_mapped_standard_object_stream(
        &lm_level::ObjectStream {
            records: vec![record],
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
    texture: &egui::TextureHandle,
    target: egui::Rect,
    tiles: &[(usize, usize, u16)],
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
        .min(16.0);
    let origin = target.center() - egui::vec2(width * cell, height * cell) / 2.0;
    for &(x, y, tile) in tiles {
        let relative_x = u16::try_from(x - min_x).expect("catalog x is at most 15");
        let relative_y = u16::try_from(y - min_y).expect("catalog y is at most 15");
        let position =
            origin + egui::vec2(f32::from(relative_x) * cell, f32::from(relative_y) * cell);
        draw_map16_atlas_tile(
            painter,
            texture,
            egui::Rect::from_min_size(position, egui::vec2(cell, cell)),
            tile,
        );
    }
}

fn sprite_catalog_ids(filter: &str) -> Vec<u8> {
    let filter = filter.trim().to_ascii_uppercase();
    (0..=STANDARD_SPRITE_MAX)
        .filter(|id| filter.is_empty() || format!("{id:02X}").contains(&filter))
        .collect()
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
) -> lm_render::StandardSpritePreviewMode {
    let placement_first = packed_sprite_first(NativeSpriteRecordFields {
        y_low: form.y_low,
        extra_bits: form.extra_bits,
        screen: form.screen,
        x: form.x,
        sprite_number: form.sprite_number,
    });
    lm_render::StandardSpritePreviewMode {
        placement_first,
        level_mode,
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
        draw_fitted_sprite_catalog_preview(&painter, texture, preview_rect, parts);
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
        .min(1.0);
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
        draw_fitted_sprite_catalog_preview(&painter, texture, preview_rect, &parts);
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
    target: egui::Rect,
    parts: &[lm_render::StandardSpritePreviewTile],
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
        .min(1.0);
    let origin = target.center() - egui::vec2(width * scale, height * scale) / 2.0;
    for part in parts {
        let position = origin
            + egui::vec2(
                f32::from(part.x.saturating_sub(min_x)) * scale,
                f32::from(part.y.saturating_sub(min_y)) * scale,
            );
        draw_sprite_preview_definition(
            painter,
            texture,
            egui::Rect::from_min_size(position, egui::vec2(16.0 * scale, 16.0 * scale)),
            part.subtiles,
        );
    }
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
fn horizontal_primary_entrance_label_pixels(entrance: VanillaMainEntrance) -> (u16, u16) {
    let screen = u16::from(entrance.level_mode_and_screen & 0x1f) * 0x100;
    let x_setting = usize::from(entrance.vertical_settings & 7);
    let y_setting = usize::from(entrance.position & 0x0f);
    let x = screen + u16::from(VANILLA_ENTRANCE_X_LOW[x_setting]);
    let y = u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
        + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]);
    let pose = entrance.vertical_settings >> 3 & 7;
    let label_clearance = if pose < 3 || pose == 5 { 18 } else { 24 };
    (x.saturating_add(label_clearance), y)
}

fn horizontal_primary_entrance_marker_pixels(entrance: VanillaMainEntrance) -> (u16, u16) {
    let screen = u16::from(entrance.level_mode_and_screen & 0x1f) * 0x100;
    let x_setting = usize::from(entrance.vertical_settings & 7);
    let y_setting = usize::from(entrance.position & 0x0f);
    (
        screen + u16::from(VANILLA_ENTRANCE_X_LOW[x_setting]),
        u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
            + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]),
    )
}

fn vertical_primary_entrance_marker_pixels(
    entrance: VanillaMainEntrance,
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
        u16::from(VANILLA_ENTRANCE_Y_HIGH[y_setting]) * 0x100
            + u16::from(VANILLA_ENTRANCE_Y_LOW[y_setting]),
    )
}

fn vertical_primary_entrance_label_pixels(
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
        format!("Entrance to level {level:X}"),
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
    if !vertical || entrance.screen_and_method & 1 != 0 {
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

fn draw_map16_atlas_tile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    tile: u16,
) {
    let column = f32::from(tile % 32);
    let row = f32::from(tile / 32);
    let uv = egui::Rect::from_min_max(
        egui::pos2(column / 32.0, row / 16.0),
        egui::pos2((column + 1.0) / 32.0, (row + 1.0) / 16.0),
    );
    painter.image(texture.id(), target, uv, egui::Color32::WHITE);
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
    background_map16_texture: Option<&egui::TextureHandle>,
    background_plane_texture: Option<&egui::TextureHandle>,
    foreground_texture: Option<&egui::TextureHandle>,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    entrance: VanillaMainEntrance,
    major_tiles: u16,
    minor_tiles: u16,
    vertical: bool,
    game_camera: Option<(u16, u16)>,
) {
    if let (Some(texture), Some(camera)) = (background_plane_texture, game_camera) {
        draw_wrapped_background_viewport(painter, target, cell_size, texture, entrance, camera);
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
                    |camera| vanilla_game_background_coordinates(x, y, entrance, camera),
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
            let x_offset = f32::from(u8::try_from(x).unwrap_or_default()) * cell_size;
            let y_offset = f32::from(u8::try_from(y).unwrap_or_default()) * cell_size;
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
                draw_custom_map16_tile(painter, texture, cell, definition);
            } else if background_map16_texture.is_some()
                && tile < 0x200
                && let Some(texture) = background_map16_texture
            {
                draw_map16_atlas_tile(painter, texture, cell, tile);
            } else if tile < 0x200
                && let Some(texture) = map16_texture
            {
                draw_map16_atlas_tile(painter, texture, cell, tile);
            }
        }
    }
}

fn draw_wrapped_background_viewport(
    painter: &egui::Painter,
    world: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    entrance: VanillaMainEntrance,
    camera: (u16, u16),
) {
    const PLANE_PIXELS: i32 = 512;
    const VIEW_WIDTH: i32 = 256;
    const VIEW_HEIGHT: i32 = 224;
    let layer1_camera = (i32::from(camera.0) * 16, i32::from(camera.1) * 16);
    let (source_x, source_y) = vanilla_layer2_camera_pixels(entrance, layer1_camera);
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
        let plane_y = (source_y + output_y).rem_euclid(PLANE_PIXELS);
        let rows = (PLANE_PIXELS - plane_y).min(VIEW_HEIGHT - output_y);
        let mut output_x = 0;
        while output_x < VIEW_WIDTH {
            let plane_x = (source_x + output_x).rem_euclid(PLANE_PIXELS);
            let columns = (PLANE_PIXELS - plane_x).min(VIEW_WIDTH - output_x);
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

fn draw_wrapped_layer3_viewport(
    painter: &egui::Painter,
    world: egui::Rect,
    cell_size: f32,
    texture: &egui::TextureHandle,
    position: (i16, i16),
    camera: (u16, u16),
) {
    const PLANE_PIXELS: i32 = 512;
    const VIEW_WIDTH: i32 = 256;
    const VIEW_HEIGHT: i32 = 224;
    let viewport = egui::Rect::from_min_size(
        world.min
            + egui::vec2(
                f32::from(camera.0) * cell_size,
                f32::from(camera.1) * cell_size,
            ),
        egui::vec2(16.0 * cell_size, 14.0 * cell_size),
    );
    let pixel_scale = cell_size / 16.0;
    let source_x = i32::from(position.0).rem_euclid(PLANE_PIXELS);
    let source_y = i32::from(position.1).rem_euclid(PLANE_PIXELS);
    let mut output_y = 0;
    while output_y < VIEW_HEIGHT {
        let plane_y = (source_y + output_y).rem_euclid(PLANE_PIXELS);
        let rows = (PLANE_PIXELS - plane_y).min(VIEW_HEIGHT - output_y);
        let mut output_x = 0;
        while output_x < VIEW_WIDTH {
            let plane_x = (source_x + output_x).rem_euclid(PLANE_PIXELS);
            let columns = (PLANE_PIXELS - plane_x).min(VIEW_WIDTH - output_x);
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
        usize::try_from(source_y.rem_euclid(32)).unwrap_or_default(),
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
    custom_map16: Option<&'a lm_app::NativeMap16SidecarDocument>,
    foreground_texture: Option<&'a egui::TextureHandle>,
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
    let mut artwork_bounds = HashMap::new();
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    if lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).is_err()
        || lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).is_err()
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
    let shared_standard_cache = if has_custom_displays {
        false
    } else {
        request.handler_map.is_some_and(|handler_map| {
            let stream = lm_level::ObjectStream {
                records: request.records.to_vec(),
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
        let Ok(Some(cache)) = lm_render::render_mapped_standard_object_placement(
            record,
            *placement,
            &definitions,
            handler_map,
            layout,
            VANILLA_EMPTY_MAP16_TILE,
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
            draw_map16_atlas_tile(painter, request.texture, tile_rect, tile);
        }
    }
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
        match custom_object_part_source(part.tile, request.custom_map16) {
            CustomObjectPartSource::Base(tile) => {
                draw_map16_atlas_tile(painter, request.texture, target, tile);
            }
            CustomObjectPartSource::Custom(definition) => {
                if let Some(texture) = request.foreground_texture {
                    draw_custom_map16_tile(painter, texture, target, definition);
                } else {
                    draw_unresolved_custom_object_part(painter, target, part.tile);
                }
            }
            CustomObjectPartSource::Unresolved => {
                draw_unresolved_custom_object_part(painter, target, part.tile);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CustomObjectPartSource {
    Base(u16),
    Custom(lm_level::Map16Tile),
    Unresolved,
}

fn custom_object_part_source(
    tile: u16,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
) -> CustomObjectPartSource {
    if tile < 0x200 {
        return CustomObjectPartSource::Base(tile);
    }
    let Some(lm_app::NativeMap16SidecarDocument::M16(sidecar)) = custom_map16 else {
        return CustomObjectPartSource::Unresolved;
    };
    sidecar.tile(usize::from(tile & 0x3fff)).map_or(
        CustomObjectPartSource::Unresolved,
        CustomObjectPartSource::Custom,
    )
}

fn draw_unresolved_custom_object_part(painter: &egui::Painter, target: egui::Rect, tile: u16) {
    painter.rect_filled(
        target.shrink(1.0),
        1.0,
        egui::Color32::from_rgb(220, 70, 70),
    );
    painter.text(
        target.center(),
        egui::Align2::CENTER_CENTER,
        format!("{:03X}", tile & 0x3fff),
        egui::FontId::monospace(6.0),
        egui::Color32::WHITE,
    );
}

pub(crate) fn draw_custom_map16_tile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    definition: lm_level::Map16Tile,
) {
    let half = target.size() / 2.0;
    for (offset, subtile) in [
        egui::vec2(0.0, 0.0),
        egui::vec2(half.x, 0.0),
        egui::vec2(0.0, half.y),
        half,
    ]
    .into_iter()
    .zip([
        definition.top_left,
        definition.top_right,
        definition.bottom_left,
        definition.bottom_right,
    ]) {
        let position = target.min + offset;
        let quadrant = egui::Rect::from_min_size(position, half);
        draw_foreground_subtile(painter, texture, quadrant, subtile);
    }
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
    response: &egui::Response,
    canvas: egui::Rect,
    vertical: bool,
    records: &[ObjectRecord],
    placements: &[lm_level::NativeObjectPlacement],
    selected: usize,
    map16_texture: Option<&egui::TextureHandle>,
    artwork_bounds: &HashMap<usize, egui::Rect>,
    resize_models: &HashMap<usize, lm_render::StandardObjectResizeModel>,
    cell: f32,
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
        draw_object_marker(
            painter,
            map16_texture,
            object_rect,
            record,
            index == selected,
            artwork_rect.is_some(),
        );
        if index == selected
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
            if response
                .interact_pointer_pos()
                .is_some_and(|position| handle.contains(position))
            {
                hits.resize = Some(index);
            }
        }
        if response
            .interact_pointer_pos()
            .is_some_and(|position| object_rect.contains(position))
        {
            hits.body = Some(index);
        }
    }
    hits
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

#[derive(Clone, Copy)]
struct SpritePlacementDraw<'a> {
    painter: &'a egui::Painter,
    target: egui::Rect,
    cell_size: f32,
    texture: Option<&'a egui::TextureHandle>,
    placements: &'a [lm_level::NativeSpritePlacement],
    cursor: Option<egui::Pos2>,
    selected: usize,
    vertical: bool,
    level_mode: u8,
    animation_phase: u8,
    custom_sprites: Option<&'a lm_level::SscResolvedTable>,
    custom_map16: Option<&'a lm_app::NativeMap16SidecarDocument>,
    external_textures: &'a HashMap<lm_render::RemappedCustomSpritePreviewTile, egui::TextureHandle>,
    editor_overlays: bool,
}

#[allow(clippy::too_many_lines)]
fn draw_sprite_placements(request: SpritePlacementDraw<'_>) -> Option<usize> {
    let SpritePlacementDraw {
        painter,
        target,
        cell_size,
        texture,
        placements,
        cursor,
        selected,
        vertical,
        level_mode,
        animation_phase,
        custom_sprites,
        custom_map16,
        external_textures,
        editor_overlays,
    } = request;
    let mut hit = None;
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
            lm_render::render_lunar_magic_standard_sprite_with_mode(
                placement.sprite_number,
                standard_sprite_preview_mode(
                    placement,
                    vertical,
                    level_mode,
                    animation_phase,
                    standard_8a_count,
                ),
            )
        } else {
            custom_preview
        };
        if uses_standard && placement.sprite_number == 0x8a {
            standard_8a_count = standard_8a_count.saturating_add(1);
        }
        let mut interactive_rect = marker;
        if let (Some(texture), Some(parts)) = (texture, preview.as_deref()) {
            interactive_rect =
                sprite_preview_bounds(marker, parts.iter().map(|part| (part.x, part.y)), cell_size);
            for part in parts {
                draw_sprite_preview_definition(
                    painter,
                    texture,
                    sprite_preview_part_rect(marker, part.x, part.y, cell_size),
                    part.subtiles,
                );
            }
        } else if let Some(parts) = external_preview.as_deref()
            && parts
                .iter()
                .all(|part| external_textures.contains_key(part))
        {
            interactive_rect =
                sprite_preview_bounds(marker, parts.iter().map(|part| (part.x, part.y)), cell_size);
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
            painter.rect_filled(
                marker,
                marker.width() / 2.0,
                if placement.token_index == selected {
                    egui::Color32::LIGHT_RED
                } else {
                    egui::Color32::from_rgb(220, 70, 70)
                },
            );
            painter.text(
                marker.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:02X}", placement.sprite_number),
                egui::FontId::monospace(7.0),
                egui::Color32::WHITE,
            );
        }
        if editor_overlays && placement.token_index == selected {
            painter.rect_stroke(
                interactive_rect,
                marker.width() / 2.0,
                egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                egui::StrokeKind::Inside,
            );
        }
        if editor_overlays && cursor.is_some_and(|position| interactive_rect.contains(position)) {
            hit = Some(placement.token_index);
        }
    }
    hit
}

fn sprite_preview_part_rect(marker: egui::Rect, x: i16, y: i16, cell_size: f32) -> egui::Rect {
    marker.translate(egui::vec2(
        f32::from(x) * cell_size / 16.0,
        f32::from(y) * cell_size / 16.0,
    ))
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

fn should_draw_unresolved_sprite_marker(uses_standard: bool, sprite_number: u8) -> bool {
    !uses_standard
        || lm_render::lunar_magic_standard_sprite_preview_source(sprite_number)
            != lm_render::StandardSpritePreviewSource::NativeEmpty
}

fn standard_sprite_preview_mode(
    placement: &lm_level::NativeSpritePlacement,
    vertical: bool,
    level_mode: u8,
    animation_phase: u8,
    sprite_8a_sequence_index: u8,
) -> lm_render::StandardSpritePreviewMode {
    lm_render::StandardSpritePreviewMode {
        placement_first: placement.first_byte,
        level_mode,
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
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds * 8.0).floor() as u64;
    u8::try_from(ticks & 3).expect("two-bit animation phase")
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
    for (quadrant, word) in subtiles.into_iter().enumerate() {
        let half = target.size() / 2.0;
        let (x, y) = sprite_definition_quadrant_position(quadrant);
        let minimum = target.min + egui::vec2(f32::from(x) * half.x, f32::from(y) * half.y);
        draw_sprite_atlas_subtile(
            painter,
            texture,
            egui::Rect::from_min_size(minimum, half),
            word,
        );
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
) {
    let tile = usize::from(word & 0x03ff);
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
    painter.image(texture.id(), target, uv, egui::Color32::WHITE);
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

fn workspace_tool_width(available_width: f32) -> f32 {
    ROM_LEVEL_TOOL_PANEL_WIDTH.min((available_width * 0.42).max(280.0))
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
    const CAPTION_RESERVE: f32 = 28.0;
    let horizontal_scale = available.x.max(VIEWPORT_WIDTH) / VIEWPORT_WIDTH;
    let vertical_scale = (available.y - CAPTION_RESERVE).max(VIEWPORT_HEIGHT) / VIEWPORT_HEIGHT;
    let fitted_pixel_scale = horizontal_scale.min(vertical_scale).floor().max(1.0);
    let zoom_steps = f32::from(clamp_canvas_zoom(zoom_percent)) / 100.0;
    (fitted_pixel_scale * zoom_steps).max(1.0).floor() * TILE_PIXELS
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

fn is_supported(snapshot: &lm_app::ControllerSnapshot) -> bool {
    snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
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
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let layer2_layout =
        lm_profile::smw_us_v1_layer2_layout(&image).map_err(|error| error.to_string())?;
    let allocation = AllocationPolicy {
        search: logical_len.min(0x80_000)..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
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
            fill_bytes: vec![0xff],
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
            Some(16)
        );
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(504.0, 504.0), canvas, 16.0),
            Some(1023)
        );
        assert_eq!(
            layer2_tile_at_canvas_position(egui::pos2(513.0, 8.0), canvas, 16.0),
            None
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
    fn window_workspace_reserves_the_majority_for_the_canvas() {
        for width in [720.0_f32, 1_100.0, 1_600.0, 3_200.0] {
            let tools = workspace_tool_width(width);
            assert!(tools >= 280.0);
            assert!(tools <= ROM_LEVEL_TOOL_PANEL_WIDTH);
            assert!(width - tools > width * 0.57);
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
        let before = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(6144.0, 384.0));
        let vertical = editor.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        editor.place_layer2_object_at_canvas(
            egui::pos2(ROM_LEVEL_CANVAS_CELL * 2.5, ROM_LEVEL_CANVAS_CELL * 3.5),
            canvas,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
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
        let count_before_invalid = before_paste + 1;
        editor.paste_layer2_object("not a typed object", count_before_invalid);
        let count_after_invalid = match editor.controller.as_ref().unwrap().layer2().unwrap() {
            lm_level::NativeLayer2Data::Objects(objects) => objects.objects.records.len(),
            lm_level::NativeLayer2Data::Tilemap(_) => unreachable!(),
        };
        assert_eq!(count_after_invalid, count_before_invalid);
        assert!(editor.error.is_some());

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
    fn unresolved_sprite_markers_preserve_lunar_magics_native_empty_handlers() {
        for sprite_number in [0x29, 0x30, 0xee, 0xf0, 0xf1] {
            assert!(!should_draw_unresolved_sprite_marker(true, sprite_number));
        }
        assert!(should_draw_unresolved_sprite_marker(true, 0x00));
        assert!(should_draw_unresolved_sprite_marker(true, 0xf6));
        assert!(should_draw_unresolved_sprite_marker(false, 0xee));
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
    fn custom_object_parts_distinguish_base_external_and_unresolved_tiles() {
        let bytes = vec![0; lm_level::M16Sidecar::ENCODED_LEN];
        let m16 =
            lm_app::NativeMap16SidecarDocument::M16(lm_level::M16Sidecar::decode(&bytes).unwrap());
        let s16 =
            lm_app::NativeMap16SidecarDocument::S16(lm_level::S16Sidecar::decode(&bytes).unwrap());
        assert_eq!(
            custom_object_part_source(0x1ff, None),
            CustomObjectPartSource::Base(0x1ff)
        );
        assert_eq!(
            custom_object_part_source(0x200, Some(&m16)),
            CustomObjectPartSource::Custom(lm_level::Map16Tile::default())
        );
        assert_eq!(
            custom_object_part_source(0x200, None),
            CustomObjectPartSource::Unresolved
        );
        assert_eq!(
            custom_object_part_source(0x200, Some(&s16)),
            CustomObjectPartSource::Unresolved
        );
        assert_eq!(
            custom_object_part_source(0x400, Some(&m16)),
            CustomObjectPartSource::Unresolved
        );
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
        let layout = lm_render::NativeLevelMap16Layout {
            width: if vertical { 32 } else { 512 },
            height: if vertical { 448 } else { 27 },
            page_stride: 0x1b0,
            base_cell: 0,
            vertical,
        };
        let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
        lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let handler_map = definition_map
            .family(usize::from(level.layer1.header.object_tileset()))
            .unwrap();
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
                if vertical {
                    // The secondary vertical-plane base has not yet been authenticated.
                    return None;
                }
                let lm_level::NativeLayer2Data::Objects(layer2) = layer2 else {
                    return None;
                };
                let layer2_layout = lm_render::NativeLevelMap16Layout {
                    base_cell: 16 * 0x1b0,
                    ..layout
                };
                Some(
                    lm_render::render_mapped_standard_object_stream(
                        &layer2.objects,
                        &definitions,
                        handler_map,
                        layer2_layout,
                        VANILLA_EMPTY_MAP16_TILE,
                    )
                    .unwrap()
                    .cache,
                )
            });
        let mut mismatches = Vec::new();
        for x in 0..layout.width {
            for y in 0..layout.height {
                let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
                let actual = if rendered.was_written(index) {
                    rendered.get(layout, x, y).unwrap()
                } else {
                    layer2_rendered
                        .as_ref()
                        .map_or(VANILLA_EMPTY_MAP16_TILE, |layer2| {
                            layer2.get(layout, x, y).unwrap()
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
            for &(x, y, actual, expected) in mismatches.iter().take(100) {
                let owner = owners[y * layout.width + x].map_or_else(
                    || "unwritten".to_owned(),
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
    fn exact_snes_viewport_fits_the_available_editor_area_and_preserves_zoom() {
        for (available, zoom, expected) in [
            (egui::vec2(800.0, 600.0), 100, 32.0),
            (egui::vec2(800.0, 600.0), 200, 64.0),
            (egui::vec2(256.0, 252.0), 100, 16.0),
            (egui::vec2(1_200.0, 1_000.0), 125, 80.0),
        ] {
            assert!((fitted_snes_viewport_cell(available, zoom) - expected).abs() < f32::EPSILON);
        }
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
    fn level_105_game_preview_uses_initial_smw_camera_positions() {
        let entrance = VanillaMainEntrance {
            position: 0x5b,
            screen_and_method: 0x9a,
            ..VanillaMainEntrance::default()
        };
        assert_eq!(game_preview_origin(entrance, 512, 27, false), (0, 12));
        assert_eq!(
            vanilla_game_background_coordinates(0, 12, entrance, (0, 12)),
            (0, 12)
        );
        assert_eq!(
            vanilla_game_background_coordinates(15, 25, entrance, (0, 12)),
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
        let mut controller =
            LevelController::decode(&snapshot, layout, &SpriteLengthTable::standard()).unwrap();
        let insertion = controller.level().layer1.objects.records.len();
        controller
            .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index: insertion,
                record: record.clone(),
            }])])
            .unwrap();
        app.dispatch(prepare_commit(&controller, &snapshot).unwrap())
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
        let NativeLevelEdit::ReplaceSprite { token, .. } =
            form.semantic_edit(0, Some(&token), &lengths).unwrap()
        else {
            panic!("semantic edit must replace the selected record");
        };
        let SpriteToken::Record(record) = token else {
            panic!("semantic edit must retain an ordinary record");
        };
        assert_eq!(&record.encoded[3..], &[0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap().x, 7);
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
            extra_bits: 3,
            screen: 0x1f,
            ..SpriteForm::default()
        };
        let mode = sprite_catalog_preview_mode(&form, true, 7);
        assert_eq!(mode.placement_first, 0x1f);
        assert_eq!(mode.level_mode, 7);
        assert_eq!(
            mode.level_orientation,
            lm_render::StandardLevelOrientation::Vertical
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
            NativeLevelEdit::ReplaceSprite {
                index: 4,
                token: SpriteToken::Record(lm_level::SpriteRecord {
                    encoded: vec![0xdb, 0x3e, 0x55],
                }),
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
        let horizontal = standard_sprite_preview_mode(&placement, false, 3, 2, 4);
        assert_eq!(horizontal.placement_first, 0x91);
        assert_eq!(horizontal.level_mode, 3);
        assert_eq!(horizontal.animation_phase, 2);
        assert_eq!(horizontal.sprite_8a_sequence_index, 4);
        assert_eq!(
            horizontal.level_orientation,
            lm_render::StandardLevelOrientation::Horizontal
        );
        let vertical = standard_sprite_preview_mode(&placement, true, 7, 1, 2);
        assert_eq!(vertical.level_mode, 7);
        assert_eq!(vertical.animation_phase, 1);
        assert_eq!(vertical.sprite_8a_sequence_index, 2);
        assert_eq!(
            vertical.level_orientation,
            lm_render::StandardLevelOrientation::Vertical
        );
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
    fn compressed_layer2_index_matches_lunar_magic_column_halves() {
        assert_eq!(lm_level::native_layer2_tilemap_index(0, 0), Some(0));
        assert_eq!(lm_level::native_layer2_tilemap_index(1, 0), Some(16));
        assert_eq!(lm_level::native_layer2_tilemap_index(31, 15), Some(511));
        assert_eq!(lm_level::native_layer2_tilemap_index(0, 16), Some(512));
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
