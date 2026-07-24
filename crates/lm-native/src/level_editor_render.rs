use eframe::egui;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{CompleteLevelFile, Map16SetFile};
use lm_render::PortableLevelRenderDimensions;

pub(crate) struct LevelAssets {
    pub(crate) map16: Map16SetFile,
    pub(crate) graphics: GraphicsInterchangeFile,
    pub(crate) palette: PaletteInterchangeFile,
}

pub(crate) fn render_texture(
    context: &egui::Context,
    level: &CompleteLevelFile,
    assets: &LevelAssets,
    dimensions: PortableLevelRenderDimensions,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_portable_level(
        level,
        &assets.map16,
        &assets.graphics,
        &assets.palette,
        None,
        None,
        dimensions,
    )
    .map_err(|error| error.to_string())?;
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture(
        "portable-complete-level",
        image,
        egui::TextureOptions::NEAREST,
    ))
}

pub(crate) fn selected_coordinate(
    rect: egui::Rect,
    position: egui::Pos2,
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    if !rect.contains(position) || width == 0 || height == 0 {
        return None;
    }
    let width_f32 = f32::from(u16::try_from(width).ok()?);
    let height_f32 = f32::from(u16::try_from(height).ok()?);
    let relative_x = (position.x - rect.min.x) / rect.width();
    let relative_y = (position.y - rect.min.y) / rect.height();
    let x = (0..width).find(|index| {
        let end = u16::try_from(index + 1).map_or(1.0, |value| f32::from(value) / width_f32);
        relative_x < end
    })?;
    let y = (0..height).find(|index| {
        let end = u16::try_from(index + 1).map_or(1.0, |value| f32::from(value) / height_f32);
        relative_y < end
    })?;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_level_hit_testing_maps_last_cell() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(160.0, 80.0));
        assert_eq!(
            selected_coordinate(rect, egui::pos2(159.0, 79.0), 10, 5),
            Some((9, 4))
        );
        assert_eq!(
            selected_coordinate(rect, egui::pos2(161.0, 20.0), 10, 5),
            None
        );
    }
}
