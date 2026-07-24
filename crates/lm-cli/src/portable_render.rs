use crate::command_types::Command;

pub fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    if crate::portable_dsc_render::execute(command)? {
        return Ok(true);
    }
    match command {
        Command::RenderMap16Page {
            graphics,
            palette,
            page,
            output,
        } => crate::render_map16::execute(graphics, palette, page, output)?,
        Command::RenderGraphics {
            graphics,
            palette,
            palette_row,
            columns,
            output,
        } => crate::render_graphics::execute(graphics, palette, *palette_row, *columns, output)?,
        Command::RenderPalette {
            palette,
            columns,
            cell_size,
            output,
        } => crate::render_palette::execute(palette, *columns, *cell_size, output)?,
        Command::RenderLevel {
            level,
            map16,
            graphics,
            palette,
            appearances,
            layer3_plane,
            layer1_width,
            layer1_height,
            layer2_width,
            layer2_height,
            output,
        } => crate::render_level::execute(crate::render_level::LevelRenderRequest {
            paths: crate::render_level::LevelRenderPaths {
                level,
                map16,
                graphics,
                palette,
                appearances: appearances.as_deref(),
                layer3_plane: layer3_plane.as_deref(),
            },
            dimensions: lm_render::PortableLevelRenderDimensions {
                layer1_width: *layer1_width,
                layer1_height: *layer1_height,
                layer2_width: *layer2_width,
                layer2_height: *layer2_height,
            },
            output,
        })?,
        Command::RenderOverworld {
            overworld,
            size_modes,
            maximum_animation_records,
            map16,
            graphics,
            appearances,
            animation_frame,
            completed_reveals,
            output,
        } => crate::render_overworld::execute(crate::render_overworld::OverworldRenderRequest {
            overworld,
            size_modes,
            maximum_animation_records: *maximum_animation_records,
            map16,
            graphics,
            appearances: appearances.as_deref(),
            animation_frame: animation_frame.as_deref(),
            completed_reveals: *completed_reveals,
            output,
        })?,
        _ => return Ok(false),
    }
    Ok(true)
}
