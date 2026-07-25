use super::NativeLevelDocumentEditor;
use eframe::egui;
use lm_level::{NativeLevelFile, NativeObjectPlacement, NativeSpritePlacement};

const MIN_MAJOR_TILES: u16 = 16;
const MAX_CANVAS_MAJOR_TILES: u16 = 512;

impl NativeLevelDocumentEditor {
    pub(super) fn level_canvas(&mut self, ui: &mut egui::Ui, value: &NativeLevelFile) {
        let objects = value.layer1.objects.native_placements();
        let sprites = value.sprites.native_placements();
        let vertical = lm_profile::smw_us_v1_level_mode(value.layer1.header.level_mode()).vertical;
        let level_mode = value.layer1.header.level_mode();
        let major_tiles = canvas_major_tiles(&objects, &sprites);
        let size = if vertical {
            egui::vec2(260.0, 420.0)
        } else {
            egui::vec2(ui.available_width().max(420.0), 260.0)
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(18));
        let major_extent = if vertical {
            rect.height()
        } else {
            rect.width()
        };
        let cell = (major_extent / f32::from(major_tiles)).clamp(2.0, 14.0);
        draw_grid(&painter, rect, cell, major_tiles, vertical);
        let cursor = response.interact_pointer_pos();
        let object_hit = draw_objects(
            &painter,
            rect,
            cell,
            vertical,
            &objects,
            self.object_index,
            cursor,
        );
        let sprite_hit = draw_sprites(
            &painter,
            rect,
            cell,
            vertical,
            level_mode,
            &sprites,
            self.sprite_index,
            cursor,
        );
        if response.clicked() {
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
        ui.small(format!(
            "{} placement canvas · {} major tiles · click an object or sprite to load its semantic fields",
            if vertical { "Vertical" } else { "Horizontal" },
            major_tiles
        ));
    }
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
    vertical: bool,
) {
    let (columns, rows) = if vertical {
        (16, major_tiles)
    } else {
        (major_tiles, 16)
    };
    for column in 0..=columns {
        let x = rect.left() + f32::from(column) * cell;
        let boundary = (!vertical && column % 16 == 0) || (vertical && column == 16);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            grid_stroke(boundary),
        );
    }
    for row in 0..=rows {
        let y = rect.top() + f32::from(row) * cell;
        let boundary = (vertical && row % 16 == 0) || (!vertical && row == 16);
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
}
