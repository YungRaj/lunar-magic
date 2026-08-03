use super::OverworldAppearanceEditor;
use eframe::egui;
use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};

const TILE_SIDE: i32 = 8;
const PREVIEW_SIZE: egui::Vec2 = egui::vec2(420.0, 260.0);
const PREVIEW_PADDING: f32 = 18.0;
const MAX_SCALE: f32 = 12.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl PreviewBounds {
    fn for_parts(parts: &[SpriteAppearancePart]) -> Self {
        let mut bounds = Self {
            min_x: 0,
            min_y: 0,
            max_x: TILE_SIDE,
            max_y: TILE_SIDE,
        };
        for part in parts {
            let x = i32::from(part.x_offset);
            let y = i32::from(part.y_offset);
            bounds.min_x = bounds.min_x.min(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_x = bounds.max_x.max(x + TILE_SIDE);
            bounds.max_y = bounds.max_y.max(y + TILE_SIDE);
        }
        bounds
    }

    const fn width(self) -> i32 {
        self.max_x - self.min_x
    }

    const fn height(self) -> i32 {
        self.max_y - self.min_y
    }
}

impl OverworldAppearanceEditor {
    pub(super) fn appearance_preview(
        &mut self,
        ui: &mut egui::Ui,
        definition: &SpriteAppearanceDefinition,
    ) {
        ui.heading("Composition preview");
        ui.label(
            "Click the topmost visible part to select it. Later parts paint above earlier parts.",
        );
        let (rect, response) = ui.allocate_exact_size(PREVIEW_SIZE, egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(22));

        let bounds = PreviewBounds::for_parts(&definition.parts);
        let available = rect.shrink(PREVIEW_PADDING);
        let scale = (available.width() / bounds.width() as f32)
            .min(available.height() / bounds.height() as f32)
            .min(MAX_SCALE)
            .max(1.0);
        let content_size = egui::vec2(
            bounds.width() as f32 * scale,
            bounds.height() as f32 * scale,
        );
        let content = egui::Rect::from_center_size(rect.center(), content_size);
        let to_screen = |x: i32, y: i32| {
            egui::pos2(
                content.left() + (x - bounds.min_x) as f32 * scale,
                content.top() + (y - bounds.min_y) as f32 * scale,
            )
        };

        let origin = to_screen(0, 0);
        painter.line_segment(
            [
                egui::pos2(content.left(), origin.y),
                egui::pos2(content.right(), origin.y),
            ],
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)),
        );
        painter.line_segment(
            [
                egui::pos2(origin.x, content.top()),
                egui::pos2(origin.x, content.bottom()),
            ],
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)),
        );

        let part_rects: Vec<_> = definition
            .parts
            .iter()
            .map(|part| {
                let min = to_screen(i32::from(part.x_offset), i32::from(part.y_offset));
                egui::Rect::from_min_size(min, egui::Vec2::splat(TILE_SIDE as f32 * scale))
            })
            .collect();
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
            && content.contains(pointer)
            && let Some(index) = topmost_part_at(
                &definition.parts,
                bounds.min_x + ((pointer.x - content.left()) / scale).floor() as i32,
                bounds.min_y + ((pointer.y - content.top()) / scale).floor() as i32,
            )
        {
            self.part_index = index;
            self.part_key = None;
        }

        for (index, (part, part_rect)) in definition.parts.iter().zip(&part_rects).enumerate() {
            painter.rect_filled(*part_rect, 1.0, palette_color(part.palette_index));
            let selected = index == self.part_index;
            painter.rect_stroke(
                *part_rect,
                1.0,
                egui::Stroke::new(
                    if selected { 3.0_f32 } else { 1.0_f32 },
                    if selected {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::WHITE
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if part_rect.width() >= 54.0 {
                let flips = match (part.x_flip, part.y_flip) {
                    (false, false) => "",
                    (true, false) => " X",
                    (false, true) => " Y",
                    (true, true) => " XY",
                };
                painter.text(
                    part_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "#{index} {:04X}\nP{}{flips}",
                        part.tile_index, part.palette_index
                    ),
                    egui::FontId::monospace(10.0),
                    egui::Color32::BLACK,
                );
            }
        }
        painter.circle_filled(origin, 3.0, egui::Color32::LIGHT_RED);
        response.on_hover_text("Red dot: sprite origin; X/Y suffixes: tile flips");
    }
}

fn palette_color(index: u8) -> egui::Color32 {
    const COLORS: [egui::Color32; 8] = [
        egui::Color32::from_rgb(120, 170, 255),
        egui::Color32::from_rgb(120, 220, 150),
        egui::Color32::from_rgb(255, 190, 100),
        egui::Color32::from_rgb(220, 130, 255),
        egui::Color32::from_rgb(255, 130, 150),
        egui::Color32::from_rgb(110, 220, 220),
        egui::Color32::from_rgb(230, 220, 110),
        egui::Color32::from_rgb(190, 190, 200),
    ];
    COLORS[usize::from(index.min(7))]
}

fn topmost_part_at(parts: &[SpriteAppearancePart], x: i32, y: i32) -> Option<usize> {
    parts.iter().rposition(|part| {
        let left = i32::from(part.x_offset);
        let top = i32::from(part.y_offset);
        (left..left + TILE_SIDE).contains(&x) && (top..top + TILE_SIDE).contains(&y)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(x: i16, y: i16) -> SpriteAppearancePart {
        SpriteAppearancePart {
            tile_index: 0,
            palette_index: 0,
            x_offset: x,
            y_offset: y,
            x_flip: false,
            y_flip: false,
        }
    }

    #[test]
    fn preview_bounds_use_exact_eight_pixel_tiles_and_include_origin() {
        assert_eq!(
            PreviewBounds::for_parts(&[part(-12, 7), part(20, -9)]),
            PreviewBounds {
                min_x: -12,
                min_y: -9,
                max_x: 28,
                max_y: 15,
            }
        );
        assert_eq!(
            PreviewBounds::for_parts(&[]),
            PreviewBounds {
                min_x: 0,
                min_y: 0,
                max_x: 8,
                max_y: 8,
            }
        );
    }

    #[test]
    fn hit_testing_selects_the_later_painter_order_part() {
        let parts = [part(0, 0), part(4, 4), part(20, 20)];
        assert_eq!(topmost_part_at(&parts, 5, 5), Some(1));
        assert_eq!(topmost_part_at(&parts, 1, 1), Some(0));
        assert_eq!(topmost_part_at(&parts, 8, 0), None);
        assert_eq!(topmost_part_at(&parts, 19, 20), None);
    }
}
