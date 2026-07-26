use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, LevelController, NativeLevelEdit, RomExpansionCommand,
};
use lm_level::{
    LegacyHeaderEdit, NativeSpriteRecordFields, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord,
    SpriteLengthTable, SpriteToken,
};
use lm_project::LevelSaveOptions;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SnesPointer24, SupportedGame};

const ROM_LEVEL_CANVAS_CELL: f32 = 12.0;
const ROM_LEVEL_CANVAS_VIEW_HEIGHT: f32 = 420.0;
const STANDARD_SPRITE_MAX: u8 = 0xed;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    level: u16,
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

#[derive(Clone, Copy, Debug, Default)]
struct ObjectForm {
    command_id: u8,
    parameter: u8,
    first_coordinate: u8,
    second_coordinate: u8,
    advances_screen: bool,
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
    Sprite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanvasPlacementMode {
    Object,
    Sprite,
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
                &SpriteLengthTable::standard(),
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
            command_id: record.command_id(),
            parameter: record.parameter(),
            first_coordinate: coordinates.first,
            second_coordinate: coordinates.second,
            advances_screen: record.advances_screen(),
        }
    }

    fn ordinary_record(self) -> Result<ObjectRecord, String> {
        if self.command_id > 0x3f || self.first_coordinate > 0x0f || self.second_coordinate > 0x0f {
            return Err("object command or coordinate is out of range".into());
        }
        let first = self.first_coordinate
            | ((self.command_id & 0x30) << 1)
            | if self.advances_screen { 0x80 } else { 0 };
        let second = self.second_coordinate | ((self.command_id & 0x0f) << 4);
        ObjectRecord::new(vec![first, second, self.parameter]).map_err(|error| error.to_string())
    }
}

#[derive(Default)]
pub(crate) struct VanillaLevelEditor {
    key: Option<EditorKey>,
    controller: Option<LevelController>,
    form: HeaderForm,
    selected_object: usize,
    object_form: ObjectForm,
    dragging_object: Option<usize>,
    selected_sprite: usize,
    sprite_form: SpriteForm,
    dragging_sprite: Option<usize>,
    sprite_catalog_filter: String,
    placement_mode: Option<CanvasPlacementMode>,
    paste_target: Option<EntityPasteTarget>,
    error: Option<String>,
    map16_key: Option<(u64, u8, u8)>,
    map16_texture: Option<egui::TextureHandle>,
    sprite_texture: Option<egui::TextureHandle>,
    foreground_texture: Option<egui::TextureHandle>,
    map16_summary: Option<([usize; 4], [usize; 4], usize, usize)>,
    map16_error: Option<String>,
    standard_object_map: Option<lm_profile::SmwUsV1StandardObjectDefinitionMap>,
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

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        app: &AppState,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
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
        };
        if self.key != Some(key) {
            self.load(&snapshot, key);
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
        ui.label(format!(
            "{} standard-object definitions (tileset {object_tileset:X})",
            object_family.display_name()
        ));
        self.show_header_editor(ui, object_count, sprite_count);
        ui.separator();
        self.show_map16_preview(ui, &snapshot, object_tileset);
        ui.separator();
        self.object_canvas(ui, custom_sprites, custom_objects, custom_map16);
        ui.separator();
        ui.columns(2, |columns| {
            self.object_list(&mut columns[0]);
            self.object_editor(&mut columns[1]);
        });
        ui.separator();
        ui.columns(2, |columns| {
            self.sprite_list(&mut columns[0]);
            self.sprite_editor(&mut columns[1]);
        });
        ui.add_space(8.0);
        let expanded = snapshot.rom_bytes.len() > 0x80_000;
        let layer1_modified = self
            .controller
            .as_ref()
            .is_some_and(LevelController::layer1_is_modified);
        if !expanded && layer1_modified {
            ui.label("Level relocation needs one expanded free-space bank.");
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
                (expanded || !layer1_modified)
                    && self
                        .controller
                        .as_ref()
                        .is_some_and(LevelController::is_modified),
                egui::Button::new("Commit level changes to ROM"),
            )
            .clicked()
        {
            match prepare_commit(
                self.controller
                    .as_ref()
                    .expect("controller presence checked above"),
                &snapshot,
            ) {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        None
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

    fn load(&mut self, snapshot: &lm_app::ControllerSnapshot, key: EditorKey) {
        match LevelController::decode(
            snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        ) {
            Ok(controller) => {
                self.standard_object_map = RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .ok()
                    .and_then(|rom| {
                        lm_profile::load_smw_us_v1_standard_object_definition_map(&rom).ok()
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
                self.selected_sprite = 0;
                self.sprite_form = SpriteForm::from_token(
                    controller.level().sprites.header,
                    controller.level().sprites.tokens.first(),
                );
                self.controller = Some(controller);
                self.error = None;
            }
            Err(error) => {
                self.controller = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.error = None;
        self.map16_key = None;
        self.map16_texture = None;
        self.sprite_texture = None;
        self.foreground_texture = None;
        self.map16_summary = None;
        self.map16_error = None;
        self.standard_object_map = None;
        self.paste_target = None;
        self.dragging_sprite = None;
        self.dragging_object = None;
    }

    fn show_map16_preview(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &lm_app::ControllerSnapshot,
        object_tileset: u8,
    ) {
        egui::CollapsingHeader::new(format!(
            "Pristine Map16 graphics — object tileset {object_tileset:X}"
        ))
        .default_open(true)
        .show(ui, |ui| {
            let sprite_tileset = self.form.sprite_tileset;
            let key = (snapshot.revision, object_tileset, sprite_tileset);
            if self.map16_key != Some(key) {
                self.map16_texture = None;
                self.sprite_texture = None;
                self.foreground_texture = None;
                self.map16_summary = None;
                self.map16_error = None;
                match crate::vanilla_map16_preview::render(
                    snapshot.rom_bytes.clone(),
                    self.controller
                        .as_ref()
                        .map_or(0, |controller| {
                            u16::try_from(controller.level().number).unwrap_or(0)
                        }),
                    self.controller
                        .as_ref()
                        .map_or_default(|controller| controller.level().layer1.header),
                ) {
                    Ok(preview) => {
                        self.map16_summary = Some((
                            preview.graphics_files,
                            preview.sprite_graphics_files,
                            preview.common_tiles,
                            preview.tileset_tiles,
                        ));
                        self.map16_texture = Some(ui.ctx().load_texture(
                            format!("vanilla-map16-{object_tileset:X}-{}", snapshot.revision),
                            preview.image,
                            egui::TextureOptions::NEAREST,
                        ));
                        self.sprite_texture = Some(ui.ctx().load_texture(
                            format!(
                                "vanilla-sprite-gfx-{sprite_tileset:X}-{}",
                                snapshot.revision
                            ),
                            preview.sprite_image,
                            egui::TextureOptions::NEAREST,
                        ));
                        self.foreground_texture = Some(ui.ctx().load_texture(
                            format!(
                                "vanilla-foreground-gfx-{object_tileset:X}-{}",
                                snapshot.revision
                            ),
                            preview.foreground_image,
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    Err(error) => self.map16_error = Some(error),
                }
                self.map16_key = Some(key);
            }
            if let Some((files, sprite_files, common, specific)) = self.map16_summary {
                ui.label(format!(
                    "GFX{:02X}/GFX{:02X}/GFX{:02X}/GFX{:02X}; {common} common and {specific} tileset-specific definitions",
                    files[0], files[1], files[2], files[3]
                ));
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
                    }
                }
            });
    }

    fn object_canvas(
        &mut self,
        ui: &mut egui::Ui,
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let (records, placements, sprite_placements) = self.canvas_model();
        let vertical = self.controller.as_ref().is_some_and(|controller| {
            lm_profile::smw_us_v1_level_mode(controller.level().layer1.header.level_mode()).vertical
        });
        let level_mode = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.header.level_mode()
        });
        let animation_phase = sprite_animation_phase(ui.input(|input| input.time));
        if sprite_placements
            .iter()
            .any(|placement| placement.sprite_number == 0xa6)
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(125));
        }
        let major_tiles = canvas_major_tiles(&placements, &sprite_placements);
        let minor_tiles = canvas_minor_tiles(&placements, &sprite_placements);
        let canvas_size = rom_canvas_size(major_tiles, minor_tiles, vertical);
        ui.horizontal(|ui| {
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
        });
        if self.placement_mode.is_some() {
            ui.label("Click a canvas tile to place the values from the matching editor below.");
        }
        egui::ScrollArea::both()
            .id_salt("vanilla-rom-level-canvas")
            .max_height(ROM_LEVEL_CANVAS_VIEW_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                self.paint_object_canvas(
                    &painter,
                    &response,
                    rect,
                    major_tiles,
                    minor_tiles,
                    vertical,
                    level_mode,
                    animation_phase,
                    &records,
                    &placements,
                    &sprite_placements,
                    custom_sprites,
                    custom_objects,
                    custom_map16,
                );
            });
        draw_canvas_caption(ui, vertical);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_object_canvas(
        &mut self,
        painter: &egui::Painter,
        response: &egui::Response,
        rect: egui::Rect,
        major_tiles: u16,
        minor_tiles: u16,
        vertical: bool,
        level_mode: u8,
        animation_phase: u8,
        records: &[ObjectRecord],
        placements: &[lm_level::NativeObjectPlacement],
        sprite_placements: &[lm_level::NativeSpritePlacement],
        custom_sprites: Option<&lm_level::SscResolvedTable>,
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
        draw_object_grid(
            painter,
            rect,
            ROM_LEVEL_CANVAS_CELL,
            major_tiles,
            minor_tiles,
            vertical,
        );
        self.draw_object_artwork(
            painter,
            rect,
            ROM_LEVEL_CANVAS_CELL,
            major_tiles,
            vertical,
            records,
            placements,
            custom_objects,
            custom_map16,
        );
        let mut hit = None;
        for placement in placements {
            let index = placement.record_index;
            let Some(record) = records.get(index) else {
                continue;
            };
            let (tile_x, tile_y) = placement.tile_coordinates(vertical);
            let position = rect.min
                + egui::vec2(
                    f32::from(tile_x) * ROM_LEVEL_CANVAS_CELL,
                    f32::from(tile_y) * ROM_LEVEL_CANVAS_CELL,
                );
            let (tile_width, tile_height) = if vertical {
                (placement.minor_span, placement.major_span)
            } else {
                (placement.major_span, placement.minor_span)
            };
            let object_rect = egui::Rect::from_min_size(
                position,
                egui::vec2(
                    (f32::from(tile_width) * ROM_LEVEL_CANVAS_CELL).max(8.0),
                    (f32::from(tile_height) * ROM_LEVEL_CANVAS_CELL).max(8.0),
                ),
            );
            draw_object_marker(
                painter,
                self.map16_texture.as_ref(),
                object_rect,
                record,
                index == self.selected_object,
            );
            if response
                .interact_pointer_pos()
                .is_some_and(|position| object_rect.contains(position))
            {
                hit = Some(index);
            }
        }
        let hit_sprite = draw_sprite_placements(SpritePlacementDraw {
            painter,
            target: rect,
            cell_size: ROM_LEVEL_CANVAS_CELL,
            texture: self.sprite_texture.as_ref(),
            placements: sprite_placements,
            cursor: response.interact_pointer_pos(),
            selected: self.selected_sprite,
            vertical,
            level_mode,
            animation_phase,
            custom_sprites,
            custom_map16,
        });
        self.handle_canvas_interaction(
            response,
            hit,
            hit_sprite,
            records,
            rect,
            ROM_LEVEL_CANVAS_CELL,
            vertical,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_canvas_interaction(
        &mut self,
        response: &egui::Response,
        hit_object: Option<usize>,
        hit_sprite: Option<usize>,
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
            }
            return;
        }
        if response.clicked()
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            self.selected_object = index;
            self.object_form = ObjectForm::from_record(record);
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
            && let Some(index) = hit_object
            && let Some(record) = records.get(index)
        {
            self.dragging_object = Some(index);
            self.selected_object = index;
            self.object_form = ObjectForm::from_record(record);
        }
        if response.drag_stopped() {
            let position = response.interact_pointer_pos();
            if let (Some(index), Some(position)) = (self.dragging_sprite.take(), position) {
                self.move_sprite_to_canvas(index, position, rect, cell, vertical);
            } else if let (Some(index), Some(position)) = (self.dragging_object.take(), position) {
                self.move_object_to_canvas(index, position, rect, cell, vertical);
            }
        }
    }

    fn place_object_at_canvas(
        &mut self,
        position: egui::Pos2,
        canvas: egui::Rect,
        cell: f32,
        vertical: bool,
    ) {
        let Some((screen, coordinates)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error = Some("object placement is outside the native 16×512-tile space".into());
            return;
        };
        let record = match self.object_form.ordinary_record() {
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
        let selected =
            match predicted.insert_ordinary_object_at(record.clone(), screen, coordinates) {
                Ok(selected) => selected,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
            ObjectEdit::InsertOrdinaryAt {
                record,
                screen,
                coordinates,
            },
        ])]) {
            Ok(()) => {
                self.selected_object = selected;
                self.object_form =
                    ObjectForm::from_record(&controller.level().layer1.objects.records[selected]);
                self.placement_mode = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
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
        let token = match crate::native_level_document_form::parse_sprite_token(
            &self.sprite_form.encoded,
        ) {
            Ok(SpriteToken::Record(mut record)) => {
                if let Err(error) = record.set_native_fields(fields, &SpriteLengthTable::standard())
                {
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
        let Some((screen, coordinates)) =
            object_placement_at_canvas_position(position, canvas, cell, vertical)
        else {
            self.error = Some("object drag ended outside the native 16×512-tile space".into());
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut predicted = controller.level().layer1.objects.clone();
        let new_index = match predicted.relocate_ordinary_object(index, screen, coordinates) {
            Ok(index) => index,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
            ObjectEdit::RelocateOrdinary {
                index,
                screen,
                coordinates,
            },
        ])]) {
            Ok(()) => {
                self.selected_object = new_index;
                self.object_form =
                    ObjectForm::from_record(&controller.level().layer1.objects.records[new_index]);
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
        let Ok(replacement) = self.sprite_form.semantic_edit(index, token) else {
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
        vertical: bool,
        records: &[ObjectRecord],
        placements: &[lm_level::NativeObjectPlacement],
        custom_objects: Option<&lm_level::OscResolvedTable>,
        custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    ) {
        let Some(texture) = self.map16_texture.as_ref() else {
            return;
        };
        draw_recovered_object_tiles(
            painter,
            RecoveredObjectDraw {
                texture,
                target,
                cell_size,
                major_tiles,
                vertical,
                records,
                handler_map: self.active_standard_object_handler_map(),
            },
        );
        if let Some(metadata) = custom_objects {
            draw_custom_object_tiles(
                painter,
                CustomObjectDraw {
                    texture,
                    target,
                    cell_size,
                    vertical,
                    records,
                    placements,
                    metadata,
                    variant: self.active_object_family_index(),
                    custom_map16,
                    foreground_texture: self.foreground_texture.as_ref(),
                },
            );
        }
    }

    fn active_standard_object_handler_map(&self) -> Option<&[u8; 64]> {
        let family_index = self.active_object_family_index();
        self.standard_object_map
            .as_ref()?
            .family(usize::from(family_index))
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

    fn canvas_model(
        &self,
    ) -> (
        Vec<ObjectRecord>,
        Vec<lm_level::NativeObjectPlacement>,
        Vec<lm_level::NativeSpritePlacement>,
    ) {
        self.controller
            .as_ref()
            .map(|controller| {
                (
                    controller.level().layer1.objects.records.clone(),
                    controller.level().layer1.objects.native_placements(),
                    controller.level().sprites.native_placements(),
                )
            })
            .unwrap_or_default()
    }

    fn object_editor(&mut self, ui: &mut egui::Ui) {
        let record_count = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.objects.records.len()
        });
        let has_selection = self.selected_object < record_count;
        if has_selection {
            ui.label(format!("Object {}", self.selected_object));
        } else {
            ui.label("No selected object.");
        }
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
        self.object_action_buttons(ui, record_count, has_selection);
        if self.paste_target == Some(EntityPasteTarget::Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            self.paste_object(&text, record_count);
        }
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
                .add_enabled(has_selection, egui::Button::new("Apply object fields"))
                .clicked()
            {
                let edits = vec![
                    ObjectEdit::SetCommandId {
                        index: self.selected_object,
                        command_id: self.object_form.command_id,
                    },
                    ObjectEdit::SetParameter {
                        index: self.selected_object,
                        parameter: self.object_form.parameter,
                    },
                    ObjectEdit::SetCoordinateNibbles {
                        index: self.selected_object,
                        coordinates: ObjectCoordinateNibbles {
                            first: self.object_form.first_coordinate,
                            second: self.object_form.second_coordinate,
                        },
                    },
                    ObjectEdit::SetAdvancesScreen {
                        index: self.selected_object,
                        advances: self.object_form.advances_screen,
                    },
                ];
                if let Some(controller) = self.controller.as_mut() {
                    match controller.apply_edits(&[NativeLevelEdit::Objects(edits)]) {
                        Ok(()) => self.error = None,
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
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

    fn apply_object_result(&mut self, edit: Result<NativeLevelEdit, String>) {
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

    fn sprite_editor(&mut self, ui: &mut egui::Ui) {
        let token_count = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.level().sprites.tokens.len());
        ui.label("Native sprite stream");
        self.sprite_catalog(ui);
        self.sprite_form_controls(ui);
        sprite_save_constraint(ui);
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

    fn choose_standard_sprite(&mut self, sprite_number: u8) {
        let fields = NativeSpriteRecordFields {
            y_low: self.sprite_form.y_low,
            extra_bits: self.sprite_form.extra_bits,
            screen: self.sprite_form.screen,
            x: self.sprite_form.x,
            sprite_number,
        };
        match standard_sprite_token(fields) {
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
        let token = self
            .controller
            .as_ref()
            .and_then(|controller| controller.level().sprites.tokens.get(self.selected_sprite));
        let edit = self.sprite_form.semantic_edit(self.selected_sprite, token);
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
    let boundary = index % 16 == 0;
    let stroke = egui::Stroke::new(
        if boundary { 1.5_f32 } else { 0.5_f32 },
        egui::Color32::from_gray(if boundary { 90 } else { 45 }),
    );
    let points = if column {
        let x = rect.left() + coordinate;
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())]
    } else {
        let y = rect.top() + coordinate;
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)]
    };
    painter.line_segment(points, stroke);
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

fn canvas_minor_tiles(
    objects: &[lm_level::NativeObjectPlacement],
    sprites: &[lm_level::NativeSpritePlacement],
) -> u16 {
    let object_end = objects
        .iter()
        .map(|placement| u16::from(placement.minor).saturating_add(u16::from(placement.minor_span)))
        .max()
        .unwrap_or(16);
    let sprite_end = sprites
        .iter()
        .map(|placement| placement.minor.saturating_add(1))
        .max()
        .unwrap_or(16);
    object_end.max(sprite_end).clamp(16, 32)
}

fn rom_canvas_size(major_tiles: u16, minor_tiles: u16, vertical: bool) -> egui::Vec2 {
    let major = f32::from(major_tiles) * ROM_LEVEL_CANVAS_CELL;
    let minor = f32::from(minor_tiles) * ROM_LEVEL_CANVAS_CELL;
    if vertical {
        egui::vec2(minor, major)
    } else {
        egui::vec2(major, minor)
    }
}

fn sprite_catalog_ids(filter: &str) -> Vec<u8> {
    let filter = filter.trim().to_ascii_uppercase();
    (0..=STANDARD_SPRITE_MAX)
        .filter(|id| filter.is_empty() || format!("{id:02X}").contains(&filter))
        .collect()
}

fn standard_sprite_token(fields: NativeSpriteRecordFields) -> Result<SpriteToken, String> {
    let mut record = lm_level::SpriteRecord {
        encoded: vec![0, 0, fields.sprite_number],
    };
    record
        .set_native_fields(fields, &SpriteLengthTable::standard())
        .map_err(|error| error.to_string())?;
    Ok(SpriteToken::Record(record))
}

fn sprite_catalog_preview_mode(
    form: &SpriteForm,
    vertical: bool,
    level_mode: u8,
) -> lm_render::StandardSpritePreviewMode {
    let placement_first = (form.y_low & 0x0f) << 4
        | (form.extra_bits & 3) << 2
        | (form.screen >> 4) << 1
        | form.y_low >> 4;
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

fn draw_sprite_catalog_entry(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    sprite_number: u8,
    mode: lm_render::StandardSpritePreviewMode,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(62.0, 62.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
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
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(3.0, 3.0),
        rect.max - egui::vec2(3.0, 15.0),
    );
    let parts = lm_render::render_lunar_magic_standard_sprite_with_mode(sprite_number, mode);
    if let (Some(texture), Some(parts)) = (texture, parts) {
        draw_fitted_sprite_catalog_preview(&painter, texture, preview_rect, &parts);
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
    response.on_hover_text(format!("Standard sprite ${sprite_number:02X}"))
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
    if major >= 0x200 || minor >= 0x20 {
        return None;
    }
    fields.screen = u8::try_from(major / 16).ok()?;
    fields.x = u8::try_from(major % 16).ok()?;
    fields.y_low = u8::try_from(minor).ok()?;
    Some(fields)
}

fn object_placement_at_canvas_position(
    position: egui::Pos2,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
) -> Option<(u16, ObjectCoordinateNibbles)> {
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
    if screen >= 32 || minor >= 16 {
        return None;
    }
    Some((
        screen,
        ObjectCoordinateNibbles {
            first: u8::try_from(major % 16).ok()?,
            second: u8::try_from(minor).ok()?,
        },
    ))
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

#[derive(Clone, Copy)]
struct RecoveredObjectDraw<'a> {
    texture: &'a egui::TextureHandle,
    target: egui::Rect,
    cell_size: f32,
    major_tiles: u16,
    vertical: bool,
    records: &'a [ObjectRecord],
    handler_map: Option<&'a [u8; 64]>,
}

fn draw_canvas_caption(ui: &mut egui::Ui, vertical: bool) {
    ui.label(format!(
        "Screen-aware {} layout: recovered object and sprite artwork with red fallbacks for unresolved sprites; stronger lines mark screen boundaries.",
        if vertical { "vertical" } else { "horizontal" }
    ));
}

fn draw_recovered_object_tiles(painter: &egui::Painter, request: RecoveredObjectDraw<'_>) {
    let RecoveredObjectDraw {
        texture,
        target,
        cell_size,
        major_tiles,
        vertical,
        records,
        handler_map,
    } = request;
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    if lm_render::install_lunar_magic_shared_extended_objects(&mut definitions).is_err()
        || lm_render::install_lunar_magic_shared_standard_objects(&mut definitions).is_err()
    {
        return;
    }
    let layout = lm_render::NativeLevelMap16Layout {
        width: if vertical {
            16
        } else {
            usize::from(major_tiles)
        },
        height: if vertical {
            usize::from(major_tiles)
        } else {
            16
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical,
    };
    let stream = lm_level::ObjectStream {
        records: records.to_vec(),
    };
    let report = match handler_map {
        Some(handler_map) => lm_render::render_mapped_standard_object_stream(
            &stream,
            &definitions,
            handler_map,
            layout,
            u16::MAX,
        ),
        None => lm_render::render_standard_object_stream(&stream, &definitions, layout, u16::MAX),
    };
    let Ok(report) = report else {
        return;
    };
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            let Some(&tile) = report.cache.cells().get(index) else {
                continue;
            };
            if tile == u16::MAX {
                continue;
            }
            let Ok(tile_x) = u16::try_from(x) else {
                continue;
            };
            let Ok(tile_y) = u16::try_from(y) else {
                continue;
            };
            let tile_rect = egui::Rect::from_min_size(
                target.min
                    + egui::vec2(f32::from(tile_x) * cell_size, f32::from(tile_y) * cell_size),
                egui::vec2(cell_size, cell_size),
            );
            draw_map16_atlas_tile(painter, texture, tile_rect, tile);
        }
    }
}

#[derive(Clone, Copy)]
struct CustomObjectDraw<'a> {
    texture: &'a egui::TextureHandle,
    target: egui::Rect,
    cell_size: f32,
    vertical: bool,
    records: &'a [ObjectRecord],
    placements: &'a [lm_level::NativeObjectPlacement],
    metadata: &'a lm_level::OscResolvedTable,
    variant: u8,
    custom_map16: Option<&'a lm_app::NativeMap16SidecarDocument>,
    foreground_texture: Option<&'a egui::TextureHandle>,
}

fn draw_custom_object_tiles(painter: &egui::Painter, request: CustomObjectDraw<'_>) {
    for placement in request.placements {
        let Some(record) = request.records.get(placement.record_index) else {
            continue;
        };
        let Some(metadata) = request.metadata.default_display(
            record.command_id(),
            record.parameter(),
            request.variant,
        ) else {
            continue;
        };
        let Some(parts) = lm_render::render_resolved_lunar_magic_custom_object(metadata) else {
            continue;
        };
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
            let definition = match request.custom_map16 {
                Some(lm_app::NativeMap16SidecarDocument::M16(sidecar)) => {
                    sidecar.tile(usize::from(part.tile & 0x3fff))
                }
                Some(lm_app::NativeMap16SidecarDocument::S16(_)) | None => None,
            };
            if let (Some(definition), Some(texture)) = (definition, request.foreground_texture) {
                draw_custom_map16_tile(painter, texture, target, definition);
            } else if part.tile < 0x200 {
                draw_map16_atlas_tile(painter, request.texture, target, part.tile);
            }
        }
    }
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

fn draw_object_marker(
    painter: &egui::Painter,
    texture: Option<&egui::TextureHandle>,
    target: egui::Rect,
    record: &ObjectRecord,
    selected: bool,
) {
    let recovered_tile = (record.command_id() == 0)
        .then(|| lm_render::lunar_magic_shared_extended_object_tile(record.parameter()))
        .flatten();
    let recovered_standard = matches!(
        record.command_id(),
        15..=17 | 20..=29 | 31..=33
    );
    if let (Some(tile), Some(texture)) = (recovered_tile, texture) {
        draw_map16_atlas_tile(painter, texture, target.shrink(1.0), tile);
    } else if !recovered_standard {
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
}

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
    } = request;
    let mut hit = None;
    for placement in placements {
        let (tile_x, tile_y) = placement.tile_coordinates(vertical);
        let center = target.min
            + egui::vec2(
                (f32::from(tile_x) + 0.5) * cell_size,
                (f32::from(tile_y) + 0.5) * cell_size,
            );
        let marker = egui::Rect::from_center_size(
            center,
            egui::vec2(cell_size.max(9.0), cell_size.max(9.0)),
        );
        let preview = custom_sprites
            .and_then(|table| table.default_display(placement.sprite_number, placement.extra_bits))
            .and_then(|sprite| {
                lm_render::render_resolved_lunar_magic_custom_sprite_with(sprite, |index| {
                    external_sprite_definition(custom_map16, index)
                })
            })
            .or_else(|| {
                lm_render::render_lunar_magic_standard_sprite_with_mode(
                    placement.sprite_number,
                    standard_sprite_preview_mode(placement, vertical, level_mode, animation_phase),
                )
            });
        if let (Some(texture), Some(parts)) = (texture, preview) {
            for part in parts {
                draw_sprite_preview_definition(
                    painter,
                    texture,
                    marker.translate(egui::vec2(f32::from(part.x), f32::from(part.y))),
                    part.subtiles,
                );
            }
            if placement.token_index == selected {
                painter.rect_stroke(
                    marker,
                    marker.width() / 2.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
        } else {
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
        if cursor.is_some_and(|position| marker.contains(position)) {
            hit = Some(placement.token_index);
        }
    }
    hit
}

fn standard_sprite_preview_mode(
    placement: &lm_level::NativeSpritePlacement,
    vertical: bool,
    level_mode: u8,
    animation_phase: u8,
) -> lm_render::StandardSpritePreviewMode {
    lm_render::StandardSpritePreviewMode {
        placement_first: placement.first_byte,
        level_mode,
        animation_phase,
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

pub(crate) fn draw_sprite_preview_definition(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    subtiles: [u16; 4],
) {
    for (quadrant, word) in subtiles.into_iter().enumerate() {
        let half = target.size() / 2.0;
        let x = u16::try_from(quadrant % 2).expect("quadrant x fits u16");
        let y = u16::try_from(quadrant / 2).expect("quadrant y fits u16");
        let minimum = target.min + egui::vec2(f32::from(x) * half.x, f32::from(y) * half.y);
        draw_sprite_atlas_subtile(
            painter,
            texture,
            egui::Rect::from_min_size(minimum, half),
            word,
        );
    }
}

fn draw_sprite_atlas_subtile(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    target: egui::Rect,
    word: u16,
) {
    let tile = usize::from(word & 0x03ff);
    let slot = tile / 128;
    let within_slot = tile % 128;
    let column = slot % 2 * 16 + within_slot % 16;
    let row = slot / 2 * 8 + within_slot / 16;
    let column = u16::try_from(column).expect("sprite atlas has 32 columns");
    let row = u16::try_from(row).expect("sprite atlas has 16 rows");
    let mut minimum = egui::pos2(f32::from(column) / 32.0, f32::from(row) / 16.0);
    let mut maximum = egui::pos2(f32::from(column + 1) / 32.0, f32::from(row + 1) / 16.0);
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

fn sprite_save_constraint(ui: &mut egui::Ui) {
    ui.small(
        "Pristine ROM saves are pointer-preserving: header edits, same-size replacements, and removals are safe; growth is rejected.",
    );
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
        ],
    };
    let sprite_bank = pristine_sprite_bank_range(&image, layout)?;
    controller
        .prepare_commit_with_shared_bank_sprite_relocation(
            format!("Edit pristine SMW level {:03X}", controller.level().number),
            &LevelSaveOptions {
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
            },
        )
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
    fn object_form_constructs_native_three_byte_record() {
        let form = ObjectForm {
            command_id: 0x31,
            parameter: 0x42,
            first_coordinate: 5,
            second_coordinate: 6,
            advances_screen: true,
        };
        let record = form.ordinary_record().unwrap();
        assert_eq!(record.encoded(), &[0xe5, 0x16, 0x42]);
        assert_eq!(ObjectForm::from_record(&record).command_id, 0x31);
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
    fn sprite_insertion_follows_selection_or_appends_to_an_empty_stream() {
        assert_eq!(sprite_insertion_index(0, 0), 0);
        assert_eq!(sprite_insertion_index(0, 3), 1);
        assert_eq!(sprite_insertion_index(2, 3), 3);
        assert_eq!(sprite_insertion_index(99, 3), 3);
    }

    #[test]
    fn rom_canvas_has_fixed_scale_and_swaps_orientation_axes() {
        assert_eq!(rom_canvas_size(32, 16, false), egui::vec2(384.0, 192.0));
        assert_eq!(rom_canvas_size(32, 16, true), egui::vec2(192.0, 384.0));
        assert_eq!(rom_canvas_size(512, 32, false), egui::vec2(6144.0, 384.0));
    }

    #[test]
    fn rom_canvas_minor_extent_keeps_second_sprite_row_visible() {
        let sprites = [lm_level::NativeSpritePlacement {
            token_index: 0,
            first_byte: 1,
            screen: 0,
            major: 3,
            minor: 31,
            sprite_number: 1,
            extra_bits: 0,
        }];
        assert_eq!(canvas_minor_tiles(&[], &sprites), 32);
        assert_eq!(canvas_minor_tiles(&[], &[]), 16);
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
    fn catalog_selection_constructs_a_valid_native_sprite_record() {
        let fields = NativeSpriteRecordFields {
            y_low: 0x1d,
            extra_bits: 2,
            screen: 0x1e,
            x: 3,
            sprite_number: 0xa6,
        };
        let token = standard_sprite_token(fields).unwrap();
        let SpriteToken::Record(record) = token else {
            panic!("catalog always constructs an ordinary record");
        };
        assert_eq!(record.encoded, vec![0xdb, 0x3e, 0xa6]);
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
            form.semantic_edit(4, Some(&token)).unwrap(),
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
        assert!(form.semantic_edit(0, Some(&token)).is_err());
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
        let horizontal = standard_sprite_preview_mode(&placement, false, 3, 2);
        assert_eq!(horizontal.placement_first, 0x91);
        assert_eq!(horizontal.level_mode, 3);
        assert_eq!(horizontal.animation_phase, 2);
        assert_eq!(
            horizontal.level_orientation,
            lm_render::StandardLevelOrientation::Horizontal
        );
        let vertical = standard_sprite_preview_mode(&placement, true, 7, 1);
        assert_eq!(vertical.level_mode, 7);
        assert_eq!(vertical.animation_phase, 1);
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
            sprite_fields_at_canvas_position(egui::pos2(1.0, 32.0), canvas, 1.0, false, fields,)
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
                    first: 3,
                    second: 12,
                }
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
                }
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
                    first: 15,
                    second: 4,
                }
            ))
        );
        assert!(
            object_placement_at_canvas_position(egui::pos2(35.0, 16.0), canvas, 1.0, false,)
                .is_none()
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
    fn pristine_sprite_growth_relocates_in_the_shared_bank_and_reopens() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
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
        assert_eq!(
            project
                .load_level_slot(0x105, layout, &SpriteLengthTable::standard())
                .unwrap()
                .sprites,
            controller.level().sprites
        );
        assert!(
            lm_rats::parse_at(
                project.rom.logical_bytes(),
                relocated_pointer.to_pc(Mapper::LoRom).unwrap() - lm_rats::HEADER_LEN
            )
            .is_ok()
        );
    }
}
