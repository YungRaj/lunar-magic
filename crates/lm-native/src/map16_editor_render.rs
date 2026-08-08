use eframe::egui;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::Map16PageFile;

const TILE_ENDS: [f32; 16] = [
    0.0625, 0.125, 0.1875, 0.25, 0.3125, 0.375, 0.4375, 0.5, 0.5625, 0.625, 0.6875, 0.75, 0.8125,
    0.875, 0.9375, 1.0,
];

pub(crate) fn render_texture(
    context: &egui::Context,
    page: &Map16PageFile,
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_portable_map16_page(graphics, palette, page)
        .map_err(|error| error.to_string())?;
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture("portable-map16-page", image, egui::TextureOptions::NEAREST))
}

pub(crate) fn selected_tile(rect: egui::Rect, position: egui::Pos2) -> Option<usize> {
    if !rect.contains(position) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let relative_x = (position.x - rect.min.x) / rect.width();
    let relative_y = (position.y - rect.min.y) / rect.height();
    let column = TILE_ENDS.into_iter().position(|end| relative_x < end)?;
    let row = TILE_ENDS.into_iter().position(|end| relative_y < end)?;
    row.checked_mul(16)?.checked_add(column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hit_testing_maps_edges_without_float_casts() {
        let rect = egui::Rect::from_min_size(egui::Pos2::new(5.0, 7.0), egui::Vec2::splat(256.0));
        assert_eq!(selected_tile(rect, egui::Pos2::new(5.0, 7.0)), Some(0));
        assert_eq!(
            selected_tile(rect, egui::Pos2::new(260.9, 262.9)),
            Some(255)
        );
        assert_eq!(selected_tile(rect, egui::Pos2::new(261.1, 100.0)), None);

        let zoomed =
            egui::Rect::from_min_size(egui::Pos2::new(11.0, 13.0), egui::Vec2::splat(12_800.0));
        assert_eq!(selected_tile(zoomed, zoomed.min), Some(0));
        assert_eq!(
            selected_tile(zoomed, zoomed.min + egui::vec2(12_799.0, 12_799.0)),
            Some(255)
        );
        assert_eq!(
            selected_tile(zoomed, zoomed.min + egui::vec2(6_401.0, 6_401.0)),
            Some(136)
        );
    }
}
