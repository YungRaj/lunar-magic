use crate::{
    Canvas, CanvasError, OverworldRenderError, apply_event_reveals, build_overworld_scene,
    draw_scene, resolve_sprite_appearances,
};
use lm_graphics::{
    GraphicsInterchangeFile, MaterializedAnimationFrame, MaterializedFrameError, Palette,
};
use lm_level::{Map16SetFile, Map16Tile};
use lm_overworld::SpriteAppearanceFile;
use lm_project::CompleteOverworldFile;
use std::fmt;

const PALETTE_ROW_COLORS: usize = 16;

#[derive(Debug)]
pub enum PortableOverworldRenderError {
    TooManyCompletedReveals { requested: usize, available: usize },
    MissingMap16Tile { layer: u8, tile: u16 },
    InvalidPaletteShape(usize),
    EmptyPalette,
    MissingGraphicsTile { instance: usize, tile: usize },
    MissingPaletteRow { instance: usize, row: usize },
    DimensionOverflow,
    Animation(MaterializedFrameError),
    Scene(OverworldRenderError),
    Canvas(CanvasError),
}

impl fmt::Display for PortableOverworldRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable overworld render failed: {self:?}")
    }
}

impl std::error::Error for PortableOverworldRenderError {}

/// Renders one complete portable overworld snapshot, optionally at an animation frame.
///
/// # Errors
///
/// Rejects reveal counts outside the model, malformed layers or palettes, missing Map16,
/// graphics, or palette references, invalid animation overrides, and excessive dimensions.
pub fn render_portable_overworld(
    overworld: &CompleteOverworldFile,
    map16: &Map16SetFile,
    graphics: &GraphicsInterchangeFile,
    appearance_definitions: Option<&SpriteAppearanceFile>,
    animation_frame: Option<&MaterializedAnimationFrame>,
    completed_reveals: usize,
) -> Result<Canvas, PortableOverworldRenderError> {
    let available = overworld.data.event_reveals.entries.len();
    if completed_reveals > available {
        return Err(PortableOverworldRenderError::TooManyCompletedReveals {
            requested: completed_reveals,
            available,
        });
    }
    let definitions: Vec<Map16Tile> = map16
        .set
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect();
    let layer1 = apply_event_reveals(
        &overworld.data.layers.layer1,
        &overworld.data.event_reveals.entries,
        completed_reveals,
    );
    validate_map16_references(1, &layer1.tiles, definitions.len())?;
    validate_map16_references(2, &overworld.data.layers.layer2.tiles, definitions.len())?;
    let appearances = appearance_definitions
        .map(|values| resolve_sprite_appearances(&overworld.data.sprites, values))
        .unwrap_or_default();
    let scene = build_overworld_scene(
        &layer1,
        &overworld.data.layers.layer2,
        &definitions,
        &overworld.data.sprites,
        &appearances,
    )
    .map_err(PortableOverworldRenderError::Scene)?;
    let (animated_graphics, animated_palette);
    let (tiles, palette) = if let Some(frame) = animation_frame {
        (animated_graphics, animated_palette) = frame
            .apply(&graphics.graphics, &overworld.data.palette)
            .map_err(PortableOverworldRenderError::Animation)?;
        (&animated_graphics.tiles, &animated_palette)
    } else {
        (&graphics.graphics.tiles, &overworld.data.palette)
    };
    let palettes = palette_rows(palette)?;
    validate_scene_assets(&scene.instances, tiles.len(), palettes.len())?;
    let width = overworld
        .shape
        .width
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let height = overworld
        .shape
        .height
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let mut canvas =
        Canvas::try_new(width, height).map_err(PortableOverworldRenderError::Canvas)?;
    draw_scene(&mut canvas, &scene, tiles, &palettes);
    Ok(canvas)
}

fn validate_map16_references(
    layer: u8,
    tiles: &[u16],
    definition_count: usize,
) -> Result<(), PortableOverworldRenderError> {
    if let Some(tile) = tiles
        .iter()
        .find(|tile| usize::from(**tile) >= definition_count)
    {
        return Err(PortableOverworldRenderError::MissingMap16Tile { layer, tile: *tile });
    }
    Ok(())
}

fn palette_rows(palette: &Palette) -> Result<Vec<Palette>, PortableOverworldRenderError> {
    if palette.colors.len() % PALETTE_ROW_COLORS != 0 {
        return Err(PortableOverworldRenderError::InvalidPaletteShape(
            palette.colors.len(),
        ));
    }
    let rows = palette
        .colors
        .chunks_exact(PALETTE_ROW_COLORS)
        .map(|colors| Palette {
            colors: colors.to_vec(),
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(PortableOverworldRenderError::EmptyPalette);
    }
    Ok(rows)
}

fn validate_scene_assets(
    instances: &[crate::TileInstance],
    graphics_count: usize,
    palette_count: usize,
) -> Result<(), PortableOverworldRenderError> {
    for (instance, value) in instances.iter().enumerate() {
        if value.tile_index >= graphics_count {
            return Err(PortableOverworldRenderError::MissingGraphicsTile {
                instance,
                tile: value.tile_index,
            });
        }
        if value.palette_index >= palette_count {
            return Err(PortableOverworldRenderError::MissingPaletteRow {
                instance,
                row: value.palette_index,
            });
        }
    }
    Ok(())
}
