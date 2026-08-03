use super::OverworldAppearanceEditor;
use eframe::egui;
use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};

const TILE_SIDE: i32 = 8;
const PREVIEW_SIZE: egui::Vec2 = egui::vec2(420.0, 260.0);
const PREVIEW_PADDING: f32 = 18.0;
const MAX_SCALE: f32 = 12.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct PreviewDrag {
    revision: u64,
    sprite_id: u16,
    part_index: usize,
    pointer: egui::Pos2,
    original: SpriteAppearancePart,
    current: SpriteAppearancePart,
}

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
        revision: u64,
        definition: &SpriteAppearanceDefinition,
    ) -> Option<lm_app::OverworldAppearanceDocumentEdit> {
        if self
            .preview_drag
            .is_some_and(|drag| drag.revision != revision || drag.sprite_id != definition.sprite_id)
        {
            self.preview_drag = None;
        }
        ui.heading("Composition preview");
        ui.label(
            "Click to select; drag to move one pixel at a time. Later parts paint above earlier parts.",
        );
        let (rect, response) = ui.allocate_exact_size(PREVIEW_SIZE, egui::Sense::click_and_drag());
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

        let mut displayed_parts = definition.parts.clone();
        if let Some(drag) = self.preview_drag
            && let Some(part) = displayed_parts.get_mut(drag.part_index)
        {
            *part = drag.current;
        }
        let part_rects: Vec<_> = displayed_parts
            .iter()
            .map(|part| {
                let min = to_screen(i32::from(part.x_offset), i32::from(part.y_offset));
                egui::Rect::from_min_size(min, egui::Vec2::splat(TILE_SIDE as f32 * scale))
            })
            .collect();
        if (response.clicked() || response.drag_started())
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
            if response.drag_started()
                && let Some(original) = definition.parts.get(index).copied()
            {
                self.preview_drag = Some(PreviewDrag {
                    revision,
                    sprite_id: definition.sprite_id,
                    part_index: index,
                    pointer,
                    original,
                    current: original,
                });
            }
        }
        if response.dragged()
            && let (Some(drag), Some(pointer)) =
                (self.preview_drag.as_mut(), response.interact_pointer_pos())
        {
            drag.current = dragged_part(
                drag.original,
                pointer.x - drag.pointer.x,
                pointer.y - drag.pointer.y,
                scale,
            );
        }

        for (index, (part, part_rect)) in displayed_parts.iter().zip(&part_rects).enumerate() {
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
        if let Some(drag) = self.preview_drag {
            painter.text(
                egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "Offset: {}, {}",
                    drag.current.x_offset, drag.current.y_offset
                ),
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
        }
        let drag_stopped = response.drag_stopped();
        response
            .on_hover_cursor(if self.preview_drag.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            })
            .on_hover_text("Red dot: sprite origin; X/Y suffixes: tile flips");
        if drag_stopped
            && let Some(drag) = self.preview_drag.take()
            && let Some(edit) = completed_drag_edit(drag)
        {
            return Some(edit);
        }
        None
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

fn dragged_part(
    mut part: SpriteAppearancePart,
    screen_delta_x: f32,
    screen_delta_y: f32,
    scale: f32,
) -> SpriteAppearancePart {
    let delta_x = (screen_delta_x / scale).round() as i32;
    let delta_y = (screen_delta_y / scale).round() as i32;
    part.x_offset = i32::from(part.x_offset)
        .saturating_add(delta_x)
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    part.y_offset = i32::from(part.y_offset)
        .saturating_add(delta_y)
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    part
}

fn completed_drag_edit(drag: PreviewDrag) -> Option<lm_app::OverworldAppearanceDocumentEdit> {
    (drag.current != drag.original).then_some(
        lm_app::OverworldAppearanceDocumentEdit::ReplacePart {
            sprite_id: drag.sprite_id,
            index: drag.part_index,
            value: drag.current,
        },
    )
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

    #[test]
    fn dragging_snaps_to_world_pixels_clamps_and_preserves_other_fields() {
        let mut original = part(i16::MAX - 1, i16::MIN + 1);
        original.tile_index = 0x123;
        original.palette_index = 7;
        original.x_flip = true;
        let moved = dragged_part(original, 18.0, -18.0, 4.0);
        assert_eq!(moved.x_offset, i16::MAX);
        assert_eq!(moved.y_offset, i16::MIN);
        assert_eq!(moved.tile_index, 0x123);
        assert_eq!(moved.palette_index, 7);
        assert!(moved.x_flip);
        assert!(!moved.y_flip);

        let extreme = dragged_part(original, f32::MAX, f32::MIN, 1.0);
        assert_eq!(extreme.x_offset, i16::MAX);
        assert_eq!(extreme.y_offset, i16::MIN);

        let snapped = dragged_part(part(10, 20), 5.9, -6.1, 4.0);
        assert_eq!((snapped.x_offset, snapped.y_offset), (11, 18));
    }

    #[test]
    fn completed_drag_emits_exactly_one_replacement_only_after_motion() {
        let original = part(1, 2);
        let mut drag = PreviewDrag {
            revision: 7,
            sprite_id: 0x1234,
            part_index: 9,
            pointer: egui::pos2(10.0, 20.0),
            original,
            current: original,
        };
        assert_eq!(completed_drag_edit(drag), None);
        drag.current.x_offset = 3;
        assert_eq!(
            completed_drag_edit(drag),
            Some(lm_app::OverworldAppearanceDocumentEdit::ReplacePart {
                sprite_id: 0x1234,
                index: 9,
                value: drag.current,
            })
        );
    }
}
