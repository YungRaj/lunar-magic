use super::{NativeLevelCanvasTool, NativeLevelDocumentEditor};
use eframe::egui;
use lm_app::NativeLevelEdit;
use lm_level::{
    NativeLevelFile, NativeObjectPlacement, NativeSpritePlacement, NativeSpriteRecordFields,
    ObjectCoordinateNibbles, ObjectEdit, SpriteLengthTable, SpriteToken,
};

const MIN_MAJOR_TILES: u16 = 16;
const MAX_CANVAS_MAJOR_TILES: u16 = 512;
const CANVAS_CELL: f32 = 12.0;
const CANVAS_VIEW_HEIGHT: f32 = 280.0;

impl NativeLevelDocumentEditor {
    pub(super) fn level_canvas(&mut self, ui: &mut egui::Ui, value: &NativeLevelFile) {
        let objects = value.layer1.objects.native_placements();
        let sprites = value.sprites.native_placements();
        let vertical = lm_profile::smw_us_v1_level_mode(value.layer1.header.level_mode()).vertical;
        let level_mode = value.layer1.header.level_mode();
        let sprite_lengths = self.current_sprite_lengths();
        let major_tiles = canvas_major_tiles(&objects, &sprites);
        let minor_tiles = canvas_minor_tiles(&objects, &sprites);
        let size = canvas_size(major_tiles, minor_tiles, vertical);
        canvas_tool_row(ui, &mut self.canvas_tool);
        let mut canvas_edit = None;
        egui::ScrollArea::both()
            .id_salt("native-level-placement-canvas")
            .max_height(CANVAS_VIEW_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(18));
                draw_grid(
                    &painter,
                    rect,
                    CANVAS_CELL,
                    major_tiles,
                    minor_tiles,
                    vertical,
                );
                let cursor = response.interact_pointer_pos();
                let object_hit = draw_objects(
                    &painter,
                    rect,
                    CANVAS_CELL,
                    vertical,
                    &objects,
                    self.object_index,
                    cursor,
                );
                let sprite_hit = draw_sprites(
                    &painter,
                    rect,
                    CANVAS_CELL,
                    vertical,
                    level_mode,
                    &sprites,
                    self.sprite_index,
                    cursor,
                );
                if response.clicked() {
                    match self.canvas_tool {
                        NativeLevelCanvasTool::Select => {
                            if let Some(index) = sprite_hit {
                                self.sprite_index = index;
                                self.form.load_sprite(value.sprites.tokens.get(index));
                            } else if let Some(index) = object_hit {
                                self.object_index = index;
                                let screen = objects
                                    .iter()
                                    .find(|placement| placement.record_index == index)
                                    .map(|placement| placement.screen);
                                self.form
                                    .load_object(value.layer1.objects.records.get(index), screen);
                            }
                        }
                        NativeLevelCanvasTool::MoveObject | NativeLevelCanvasTool::MoveSprite => {
                            if let Some(position) = cursor {
                                canvas_edit = canvas_tile_at(rect, position, vertical).map(
                                    |(major, minor)| match self.canvas_tool {
                                        NativeLevelCanvasTool::MoveObject => {
                                            object_move_edit(value, self.object_index, major, minor)
                                                .map(|edit| (edit, None))
                                        }
                                        NativeLevelCanvasTool::MoveSprite => sprite_move_edit(
                                            value,
                                            &sprite_lengths,
                                            self.sprite_index,
                                            major,
                                            minor,
                                            &sprites,
                                        ),
                                        NativeLevelCanvasTool::Select => unreachable!(),
                                    },
                                );
                            }
                        }
                    }
                }
            });
        if let Some(edit) = canvas_edit {
            self.apply_canvas_edit(edit);
        }
        ui.small(format!(
            "{} placement canvas · {}×{} native-axis tiles · Select loads fields; move tools commit one undoable placement edit",
            if vertical { "Vertical" } else { "Horizontal" },
            major_tiles,
            minor_tiles
        ));
    }

    fn apply_canvas_edit(&mut self, result: Result<(NativeLevelEdit, Option<usize>), String>) {
        match result {
            Ok((edit, selected)) => {
                if self.apply(edit)
                    && let Some(selected) = selected
                {
                    self.sprite_index = selected;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn canvas_tool_row(ui: &mut egui::Ui, tool: &mut NativeLevelCanvasTool) {
    ui.horizontal(|ui| {
        ui.selectable_value(tool, NativeLevelCanvasTool::Select, "Select");
        ui.selectable_value(tool, NativeLevelCanvasTool::MoveObject, "Move object");
        ui.selectable_value(tool, NativeLevelCanvasTool::MoveSprite, "Move sprite");
    });
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn canvas_tile_at(canvas: egui::Rect, position: egui::Pos2, vertical: bool) -> Option<(u16, u16)> {
    if !canvas.contains(position) {
        return None;
    }
    let x = ((position.x - canvas.left()) / CANVAS_CELL).floor() as u16;
    let y = ((position.y - canvas.top()) / CANVAS_CELL).floor() as u16;
    Some(if vertical { (y, x) } else { (x, y) })
}

fn object_move_edit(
    value: &NativeLevelFile,
    index: usize,
    major: u16,
    minor: u16,
) -> Result<NativeLevelEdit, String> {
    let selected = value
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .any(|placement| placement.record_index == index);
    if !selected {
        return Err("select an ordinary object before moving it on the canvas".into());
    }
    let screen = major / 16;
    if screen > 0x1f || minor > 0x0f {
        return Err("object canvas destination is outside native coordinate bounds".into());
    }
    Ok(NativeLevelEdit::Objects(vec![
        ObjectEdit::RelocateOrdinary {
            index,
            screen,
            coordinates: ObjectCoordinateNibbles {
                first: u8::try_from(major % 16).expect("major remainder is at most 15"),
                second: u8::try_from(minor).expect("minor was bounded to 15"),
            },
        },
    ]))
}

fn sprite_move_edit(
    value: &NativeLevelFile,
    lengths: &SpriteLengthTable,
    index: usize,
    major: u16,
    minor: u16,
    placements: &[NativeSpritePlacement],
) -> Result<(NativeLevelEdit, Option<usize>), String> {
    let placement = placements
        .iter()
        .find(|placement| placement.token_index == index)
        .ok_or_else(|| "select a sprite record before moving it on the canvas".to_owned())?;
    let screen = major / 16;
    if screen > 0x1f {
        return Err("sprite canvas destination is outside the native screen range".into());
    }
    if value.sprites.expanded {
        let mut relocated = value.sprites.clone();
        let selected = relocated
            .relocate_expanded_record(
                index,
                u8::try_from(screen).expect("screen was bounded to 31"),
                u8::try_from(major % 16).expect("major remainder is at most 15"),
                minor,
                lengths,
            )
            .map_err(|error| error.to_string())?;
        return Ok((
            NativeLevelEdit::RelocateExpandedSprite {
                selected: index,
                screen: u8::try_from(screen).expect("screen was bounded to 31"),
                x: u8::try_from(major % 16).expect("major remainder is at most 15"),
                y: minor,
            },
            Some(selected),
        ));
    }
    if (minor / 32) != (placement.minor / 32) {
        return Err("legacy sprite destination is outside its coordinate band".into());
    }
    let Some(SpriteToken::Record(record)) = value.sprites.tokens.get(index) else {
        return Err("select a sprite record before moving it on the canvas".into());
    };
    let mut moved = record.clone();
    let fields = moved.native_fields().map_err(|error| error.to_string())?;
    moved
        .set_native_fields(
            NativeSpriteRecordFields {
                screen: u8::try_from(screen).expect("screen was bounded to 31"),
                x: u8::try_from(major % 16).expect("major remainder is at most 15"),
                y_low: u8::try_from(minor % 32).expect("minor remainder is at most 31"),
                ..fields
            },
            lengths,
        )
        .map_err(|error| error.to_string())?;
    Ok((
        NativeLevelEdit::ReplaceSprite {
            index,
            token: SpriteToken::Record(moved),
        },
        None,
    ))
}

fn canvas_size(major_tiles: u16, minor_tiles: u16, vertical: bool) -> egui::Vec2 {
    let major = f32::from(major_tiles) * CANVAS_CELL;
    let minor = f32::from(minor_tiles) * CANVAS_CELL;
    if vertical {
        egui::vec2(minor, major)
    } else {
        egui::vec2(major, minor)
    }
}

fn canvas_minor_tiles(objects: &[NativeObjectPlacement], sprites: &[NativeSpritePlacement]) -> u16 {
    let object_end = objects
        .iter()
        .map(|placement| u16::from(placement.minor).saturating_add(u16::from(placement.minor_span)))
        .max()
        .unwrap_or(MIN_MAJOR_TILES);
    let sprite_end = sprites
        .iter()
        .map(|placement| placement.minor.saturating_add(1))
        .max()
        .unwrap_or(MIN_MAJOR_TILES);
    object_end
        .max(sprite_end)
        .clamp(MIN_MAJOR_TILES, MAX_CANVAS_MAJOR_TILES)
}

fn canvas_major_tiles(objects: &[NativeObjectPlacement], sprites: &[NativeSpritePlacement]) -> u16 {
    let object_end = objects
        .iter()
        .map(|placement| {
            placement
                .major
                .saturating_add(u16::from(placement.major_span))
        })
        .max()
        .unwrap_or(MIN_MAJOR_TILES);
    let sprite_end = sprites
        .iter()
        .map(|placement| placement.major.saturating_add(1))
        .max()
        .unwrap_or(MIN_MAJOR_TILES);
    object_end
        .max(sprite_end)
        .clamp(MIN_MAJOR_TILES, MAX_CANVAS_MAJOR_TILES)
}

fn draw_grid(
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
        let x = rect.left() + f32::from(column) * cell;
        let boundary = column % 16 == 0;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            grid_stroke(boundary),
        );
    }
    for row in 0..=rows {
        let y = rect.top() + f32::from(row) * cell;
        let boundary = row % 16 == 0;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            grid_stroke(boundary),
        );
    }
}

fn grid_stroke(boundary: bool) -> egui::Stroke {
    egui::Stroke::new(
        if boundary { 1.5_f32 } else { 0.5_f32 },
        egui::Color32::from_gray(if boundary { 88 } else { 42 }),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_objects(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
    placements: &[NativeObjectPlacement],
    selected: usize,
    cursor: Option<egui::Pos2>,
) -> Option<usize> {
    let mut hit = None;
    for placement in placements {
        let (x, y) = placement.tile_coordinates(vertical);
        let (width, height) = if vertical {
            (placement.minor_span, placement.major_span)
        } else {
            (placement.major_span, placement.minor_span)
        };
        let rect = egui::Rect::from_min_size(
            canvas.min + egui::vec2(f32::from(x) * cell, f32::from(y) * cell),
            egui::vec2(
                (f32::from(width) * cell).max(8.0),
                (f32::from(height) * cell).max(8.0),
            ),
        );
        painter.rect_filled(
            rect,
            1.0,
            if placement.record_index == selected {
                egui::Color32::from_rgb(70, 120, 210)
            } else {
                egui::Color32::from_rgb(45, 80, 145)
            },
        );
        painter.rect_stroke(
            rect,
            1.0,
            egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.left_top() + egui::vec2(2.0, 1.0),
            egui::Align2::LEFT_TOP,
            format!("O{:02X}", placement.record_index),
            egui::FontId::monospace(7.0),
            egui::Color32::WHITE,
        );
        if cursor.is_some_and(|position| rect.contains(position)) {
            hit = Some(placement.record_index);
        }
    }
    hit
}

#[allow(clippy::too_many_arguments)]
fn draw_sprites(
    painter: &egui::Painter,
    canvas: egui::Rect,
    cell: f32,
    vertical: bool,
    level_mode: u8,
    placements: &[NativeSpritePlacement],
    selected: usize,
    cursor: Option<egui::Pos2>,
) -> Option<usize> {
    let mut hit = None;
    for placement in placements {
        let (x, y) = placement.tile_coordinates(vertical);
        let origin = canvas.min + egui::vec2(f32::from(x) * cell, f32::from(y) * cell);
        let center = origin + egui::vec2(cell / 2.0, cell / 2.0);
        let rect = egui::Rect::from_center_size(center, egui::vec2(cell.max(10.0), cell.max(10.0)));
        let parts = lm_render::render_lunar_magic_standard_sprite_with_mode(
            placement.sprite_number,
            lm_render::StandardSpritePreviewMode {
                placement_first: placement.first_byte,
                level_mode,
                level_orientation: if vertical {
                    lm_render::StandardLevelOrientation::Vertical
                } else {
                    lm_render::StandardLevelOrientation::Horizontal
                },
                ..lm_render::StandardSpritePreviewMode::default()
            },
        );
        let mut hit_rect = rect;
        if let Some(parts) = &parts {
            for part in parts {
                let part_rect = egui::Rect::from_min_size(
                    origin
                        + egui::vec2(
                            f32::from(part.x) * cell / 16.0,
                            f32::from(part.y) * cell / 16.0,
                        ),
                    egui::vec2(cell.max(3.0), cell.max(3.0)),
                );
                hit_rect = hit_rect.union(part_rect);
                painter.rect_filled(
                    part_rect,
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(150, 45, 45, 150),
                );
                painter.rect_stroke(
                    part_rect,
                    1.0,
                    egui::Stroke::new(0.75_f32, egui::Color32::LIGHT_RED),
                    egui::StrokeKind::Inside,
                );
            }
        }
        painter.rect_filled(
            rect,
            rect.width() / 2.0,
            match (placement.token_index == selected, parts.is_some()) {
                (true, _) => egui::Color32::YELLOW,
                (false, true) => egui::Color32::from_rgb(205, 80, 80),
                (false, false) => egui::Color32::from_rgb(150, 65, 150),
            },
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{:02X}", placement.sprite_number),
            egui::FontId::monospace(7.0),
            egui::Color32::BLACK,
        );
        if cursor.is_some_and(|position| hit_rect.contains(position)) {
            hit = Some(placement.token_index);
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{LevelObjectData, NativeSpriteStream};

    fn native_file() -> NativeLevelFile {
        NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0x00, 0x20, 0x01, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn canvas_extent_covers_both_domains_and_is_bounded() {
        let objects = [NativeObjectPlacement {
            record_index: 0,
            screen: 3,
            major: 63,
            minor: 2,
            major_span: 4,
            minor_span: 1,
        }];
        let sprites = [NativeSpritePlacement {
            token_index: 0,
            first_byte: 0,
            screen: 5,
            major: 95,
            minor: 2,
            sprite_number: 1,
            extra_bits: 0,
        }];
        assert_eq!(canvas_major_tiles(&objects, &sprites), 96);
        let far = [NativeSpritePlacement {
            major: u16::MAX,
            ..sprites[0]
        }];
        assert_eq!(canvas_major_tiles(&[], &far), 512);
        assert_eq!(canvas_major_tiles(&[], &[]), 16);
    }

    #[test]
    fn canvas_orientation_uses_the_native_placement_contract() {
        let object = NativeObjectPlacement {
            record_index: 0,
            screen: 2,
            major: 35,
            minor: 7,
            major_span: 2,
            minor_span: 3,
        };
        assert_eq!(object.tile_coordinates(false), (35, 7));
        assert_eq!(object.tile_coordinates(true), (7, 35));
    }

    #[test]
    fn canvas_keeps_a_fixed_editing_scale_and_swaps_axes() {
        assert_eq!(canvas_size(32, 16, false), egui::vec2(384.0, 192.0));
        assert_eq!(canvas_size(32, 16, true), egui::vec2(192.0, 384.0));
        assert_eq!(canvas_size(512, 32, false), egui::vec2(6144.0, 384.0));
    }

    #[test]
    fn expanded_sprite_perpendicular_coordinates_expand_the_minor_axis() {
        let sprites = [NativeSpritePlacement {
            token_index: 0,
            first_byte: 0,
            screen: 0,
            major: 1,
            minor: 176,
            sprite_number: 1,
            extra_bits: 0,
        }];
        assert_eq!(canvas_minor_tiles(&[], &sprites), 177);
        assert_eq!(canvas_minor_tiles(&[], &[]), 16);
    }

    #[test]
    fn pointer_tiles_are_orientation_neutral() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 200.0));
        let position = egui::pos2(10.0 + 3.5 * CANVAS_CELL, 20.0 + 7.25 * CANVAS_CELL);
        assert_eq!(canvas_tile_at(canvas, position, false), Some((3, 7)));
        assert_eq!(canvas_tile_at(canvas, position, true), Some((7, 3)));
        assert_eq!(canvas_tile_at(canvas, egui::pos2(9.0, 20.0), false), None);
    }

    #[test]
    fn object_canvas_move_emits_one_semantic_relocation() {
        assert_eq!(
            object_move_edit(&native_file(), 0, 35, 6).unwrap(),
            NativeLevelEdit::Objects(vec![ObjectEdit::RelocateOrdinary {
                index: 0,
                screen: 2,
                coordinates: ObjectCoordinateNibbles {
                    first: 3,
                    second: 6,
                },
            }])
        );
        assert!(object_move_edit(&native_file(), 0, 3, 16).is_err());
        assert!(object_move_edit(&native_file(), 99, 3, 2).is_err());
    }

    #[test]
    fn sprite_canvas_move_preserves_identity_and_extra_bits() {
        let value = native_file();
        let placements = value.sprites.native_placements();
        let (edit, selected) = sprite_move_edit(
            &value,
            &SpriteLengthTable::standard(),
            0,
            50,
            12,
            &placements,
        )
        .unwrap();
        assert_eq!(selected, None);
        let NativeLevelEdit::ReplaceSprite {
            index,
            token: SpriteToken::Record(record),
        } = edit
        else {
            panic!("expected one sprite replacement");
        };
        assert_eq!(index, 0);
        let fields = record.native_fields().unwrap();
        assert_eq!(fields.screen, 3);
        assert_eq!(fields.x, 2);
        assert_eq!(fields.y_low, 12);
        assert_eq!(fields.sprite_number, 1);
        assert_eq!(fields.extra_bits, 0);
        assert!(
            sprite_move_edit(
                &value,
                &SpriteLengthTable::standard(),
                0,
                50,
                32,
                &placements
            )
            .is_err()
        );
    }

    #[test]
    fn expanded_sprite_canvas_move_can_cross_upper_coordinate_bands() {
        let mut value = native_file();
        value.sprites.expanded = true;
        value.sprites.tokens.insert(0, SpriteToken::Screen(2));
        let placements = value.sprites.native_placements();
        let (edit, selected) = sprite_move_edit(
            &value,
            &SpriteLengthTable::standard(),
            1,
            4 * 16 + 3,
            5 * 32 + 7,
            &placements,
        )
        .unwrap();
        assert_eq!(selected, Some(1));
        assert_eq!(
            edit,
            NativeLevelEdit::RelocateExpandedSprite {
                selected: 1,
                screen: 4,
                x: 3,
                y: 167,
            }
        );
    }
}
