use eframe::egui;
use lm_graphics::{IndexedTile, PaletteInterchangeFile};

const CELL_OFFSETS: [f32; 8] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
const CELL_ENDS: [f32; 8] = [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

pub(crate) fn palette_color(
    palette: &PaletteInterchangeFile,
    row: usize,
    color: u8,
) -> egui::Color32 {
    let index = row.saturating_mul(16).saturating_add(usize::from(color));
    let rgb = palette.palette.colors[index].to_rgb8();
    egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)
}

pub(crate) fn tile_button(
    ui: &mut egui::Ui,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    row: usize,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(34.0), egui::Sense::click());
    paint_tile(ui.painter(), rect.shrink(1.0), tile, palette, row);
    if selected {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    response
}

pub(crate) fn paint_tile(
    painter: &egui::Painter,
    rect: egui::Rect,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    row: usize,
) {
    let cell = rect.width().min(rect.height()) / 8.0;
    for (y, y_offset) in CELL_OFFSETS.into_iter().enumerate() {
        for (x, x_offset) in CELL_OFFSETS.into_iter().enumerate() {
            let color = palette_color(palette, row, tile.pixel(x, y).unwrap_or(0));
            let minimum = rect.min + egui::vec2(x_offset * cell, y_offset * cell);
            painter.rect_filled(
                egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell + 0.25)),
                0.0,
                color,
            );
        }
    }
}

pub(crate) fn tile_coordinate(rect: egui::Rect, position: egui::Pos2) -> Option<(usize, usize)> {
    if !rect.contains(position) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let relative_x = (position.x - rect.min.x) / rect.width();
    let relative_y = (position.y - rect.min.y) / rect.height();
    let x = CELL_ENDS.into_iter().position(|end| relative_x < end)?;
    let y = CELL_ENDS.into_iter().position(|end| relative_y < end)?;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_hit_testing_is_bounded_and_exact() {
        let rect = egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::Vec2::splat(80.0));
        assert_eq!(
            tile_coordinate(rect, egui::Pos2::new(10.0, 20.0)),
            Some((0, 0))
        );
        assert_eq!(
            tile_coordinate(rect, egui::Pos2::new(89.9, 99.9)),
            Some((7, 7))
        );
        assert_eq!(tile_coordinate(rect, egui::Pos2::new(90.1, 50.0)), None);
    }
}
