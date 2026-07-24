use crate::command_types::Command;

pub fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::RenderMap16Dsc {
            graphics,
            palette,
            map16,
            dsc,
            page,
            first_feature,
            first_suppressed,
            second_feature,
            output,
        } => crate::render_map16::execute_dsc(
            graphics,
            palette,
            map16,
            dsc,
            *page,
            lm_level::DscDisplayContext {
                first_feature_enabled: *first_feature,
                first_feature_suppressed: *first_suppressed,
                second_feature_enabled: *second_feature,
            },
            output,
        )?,
        Command::RenderLevelDsc {
            level,
            map16,
            graphics,
            palette,
            appearances,
            layer3_plane,
            dsc,
            custom_display,
            special_markers,
            first_feature,
            first_suppressed,
            second_feature,
            level_mode,
            layer1_width,
            layer1_height,
            layer2_width,
            layer2_height,
            output,
        } => crate::render_level::execute_dsc(crate::render_level::DscLevelRenderRequest {
            paths: crate::render_level::LevelRenderPaths {
                level,
                map16,
                graphics,
                palette,
                appearances: appearances.as_deref(),
                layer3_plane: layer3_plane.as_deref(),
            },
            dsc,
            context: lm_level::DscMaterializationContext {
                custom_display_enabled: *custom_display,
                special_markers_enabled: *special_markers,
                display: lm_level::DscDisplayContext {
                    first_feature_enabled: *first_feature,
                    first_feature_suppressed: *first_suppressed,
                    second_feature_enabled: *second_feature,
                },
                level_mode: *level_mode,
            },
            dimensions: lm_render::PortableLevelRenderDimensions {
                layer1_width: *layer1_width,
                layer1_height: *layer1_height,
                layer2_width: *layer2_width,
                layer2_height: *layer2_height,
            },
            output,
        })?,
        _ => return Ok(false),
    }
    Ok(true)
}
