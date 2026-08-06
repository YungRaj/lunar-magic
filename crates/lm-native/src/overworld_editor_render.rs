use eframe::egui;
use lm_graphics::GraphicsInterchangeFile;
use lm_level::Map16SetFile;
use lm_project::CompleteOverworldFile;

pub(crate) struct OverworldAssets {
    pub(crate) map16: Map16SetFile,
    pub(crate) graphics: GraphicsInterchangeFile,
    pub(crate) native_sprite_graphics_cache: Vec<lm_graphics::IndexedTile>,
}

pub(crate) fn render_layer_texture(
    context: &egui::Context,
    layer: &lm_overworld::OverworldLayer,
    palette: &lm_graphics::Palette,
    assets: &OverworldAssets,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_smw_overworld_layer2_tilemap(layer, &assets.graphics, palette)
        .map_err(|error| error.to_string())?;
    texture_from_canvas(context, "native-main-overworld-layer2", &canvas)
}

pub(crate) fn render_layer2_graphics_texture(
    context: &egui::Context,
    graphics: &GraphicsInterchangeFile,
    palette: &lm_graphics::Palette,
    palette_row: usize,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_portable_graphics(
        graphics,
        &lm_graphics::PaletteInterchangeFile {
            source_palette: 0,
            palette: palette.clone(),
        },
        palette_row,
        16,
    )
    .map_err(|error| error.to_string())?;
    texture_from_canvas(context, "native-overworld-layer2-8x8", &canvas)
}

fn texture_from_canvas(
    context: &egui::Context,
    name: &str,
    canvas: &lm_render::Canvas,
) -> Result<egui::TextureHandle, String> {
    let capacity = canvas
        .pixels()
        .len()
        .checked_mul(4)
        .ok_or("overworld texture byte count overflow")?;
    let mut rgba = Vec::with_capacity(capacity);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture(name, image, egui::TextureOptions::NEAREST))
}

pub(crate) fn render_texture(
    context: &egui::Context,
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    native_appearances: Option<&lm_render::NativeOverworldAppearancePair>,
    completed_reveals: usize,
) -> Result<egui::TextureHandle, String> {
    let mut canvas = lm_render::render_portable_overworld(
        overworld,
        &assets.map16,
        &assets.graphics,
        None,
        None,
        completed_reveals,
    )
    .map_err(|error| error.to_string())?;
    if let Some(native) = native_appearances {
        let placements = overworld
            .data
            .sprites
            .iter()
            .map(|sprite| lm_render::NativeOverworldSpritePlacement {
                id: sprite.id,
                x: i32::from(sprite.x),
                y: i32::from(sprite.y),
                submap: sprite.submap.encoded(),
            })
            .collect::<Vec<_>>();
        let elements = lm_render::resolve_native_overworld_sprite_elements(
            &placements,
            &native.definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &native.sprite_map16,
        );
        lm_render::draw_resolved_native_overworld_sprite_resource_elements(
            &mut canvas,
            &elements,
            &assets.native_sprite_graphics_cache,
            &overworld.data.palette,
        );
    }
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture("portable-overworld", image, egui::TextureOptions::NEAREST))
}

pub(crate) fn selected_tile(
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
    let x_position = (position.x - rect.min.x) / rect.width();
    let y_position = (position.y - rect.min.y) / rect.height();
    let x = find_axis(x_position, width, width_f32)?;
    let y = find_axis(y_position, height, height_f32)?;
    Some((x, y))
}

fn find_axis(position: f32, count: usize, count_f32: f32) -> Option<usize> {
    (0..count).find(|index| {
        let end = u16::try_from(index + 1).map_or(1.0, |value| f32::from(value) / count_f32);
        position < end
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_world_hit_test_is_exact() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64.0, 32.0));
        assert_eq!(
            selected_tile(rect, egui::pos2(63.0, 31.0), 4, 2),
            Some((3, 1))
        );
        assert_eq!(selected_tile(rect, egui::pos2(65.0, 1.0), 4, 2), None);
    }
}
