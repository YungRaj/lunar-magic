use crate::{
    Canvas, CanvasError, GridPlacement, LevelRenderError, LevelSceneLayout,
    MaterializedLayer3Plane, build_level_scene_with_layer3, draw_scene, resolve_entity_appearances,
};
use lm_graphics::{GraphicsInterchangeFile, Palette, PaletteInterchangeFile};
use lm_level::{CompleteLevelFile, EntityAppearanceFile, Layer3Error, Layer3File, Map16SetFile};
use std::fmt;

const PALETTE_ROW_COLORS: usize = 16;
const REQUIRED_PALETTE_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableLevelRenderDimensions {
    pub layer1_width: usize,
    pub layer1_height: usize,
    pub layer2_width: usize,
    pub layer2_height: usize,
}

#[derive(Debug)]
pub enum PortableLevelRenderError {
    MissingMap16Tile { layer: u8, tile: u16 },
    InvalidPaletteShape(usize),
    TooFewPaletteRows(usize),
    MissingGraphicsTile { instance: usize, tile: usize },
    MissingPaletteRow { instance: usize, row: usize },
    UnexpectedLayer3Plane,
    StaleLayer3Plane,
    DimensionOverflow,
    EmptyDimensions,
    Layer3(Layer3Error),
    Scene(LevelRenderError),
    Canvas(CanvasError),
}

impl fmt::Display for PortableLevelRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable level render failed: {self:?}")
    }
}

impl std::error::Error for PortableLevelRenderError {}

/// Renders one complete portable level through the fully validated software-render boundary.
///
/// Entity appearances and a materialized Layer 3 plane are optional. Any supplied Layer 3 plane
/// must correspond to a level with Layer 3 and be bound to its exact lossless source bytes.
///
/// # Errors
///
/// Rejects invalid layer shapes, missing Map16/graphics/palette references, malformed palette
/// rows, unexpected/stale Layer 3 planes, scene failures, and excessive canvases.
pub fn render_portable_level(
    level: &CompleteLevelFile,
    map16: &Map16SetFile,
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
    appearance_file: Option<&EntityAppearanceFile>,
    layer3_plane: Option<&MaterializedLayer3Plane>,
    dimensions: PortableLevelRenderDimensions,
) -> Result<Canvas, PortableLevelRenderError> {
    let definitions: Vec<_> = map16
        .set
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect();
    for (layer, tiles) in [
        (1, &level.0.layer1.raw_tilemap),
        (2, &level.0.layer2.raw_tilemap),
    ] {
        if let Some(tile) = tiles
            .iter()
            .find(|tile| usize::from(**tile) >= definitions.len())
        {
            return Err(PortableLevelRenderError::MissingMap16Tile { layer, tile: *tile });
        }
    }
    validate_layer3(level, layer3_plane)?;
    let layout = LevelSceneLayout {
        layer1: placement(dimensions.layer1_width, dimensions.layer1_height),
        layer2: placement(dimensions.layer2_width, dimensions.layer2_height),
    };
    let appearances = appearance_file
        .map(resolve_entity_appearances)
        .unwrap_or_default();
    let scene =
        build_level_scene_with_layer3(&level.0, layout, &definitions, &appearances, layer3_plane)
            .map_err(PortableLevelRenderError::Scene)?;
    let palettes = palette_rows(&palette.palette)?;
    validate_assets(&scene.instances, graphics, &palettes)?;
    let width = dimensions
        .layer1_width
        .max(dimensions.layer2_width)
        .checked_mul(16)
        .ok_or(PortableLevelRenderError::DimensionOverflow)?;
    let height = dimensions
        .layer1_height
        .max(dimensions.layer2_height)
        .checked_mul(16)
        .ok_or(PortableLevelRenderError::DimensionOverflow)?;
    if width == 0 || height == 0 {
        return Err(PortableLevelRenderError::EmptyDimensions);
    }
    let mut canvas = Canvas::try_new(width, height).map_err(PortableLevelRenderError::Canvas)?;
    draw_scene(&mut canvas, &scene, &graphics.graphics.tiles, &palettes);
    Ok(canvas)
}

pub(super) fn validate_layer3(
    level: &CompleteLevelFile,
    plane: Option<&MaterializedLayer3Plane>,
) -> Result<(), PortableLevelRenderError> {
    match (level.0.layer3.as_ref(), plane) {
        (_, None) => Ok(()),
        (None, Some(_)) => Err(PortableLevelRenderError::UnexpectedLayer3Plane),
        (Some(source), Some(plane)) => {
            let encoded = Layer3File(source.clone())
                .encode()
                .map_err(PortableLevelRenderError::Layer3)?;
            if plane.source_digest != lm_oracle::sha256(&encoded) {
                return Err(PortableLevelRenderError::StaleLayer3Plane);
            }
            Ok(())
        }
    }
}

pub(super) fn palette_rows(palette: &Palette) -> Result<Vec<Palette>, PortableLevelRenderError> {
    if palette.colors.len() % PALETTE_ROW_COLORS != 0 {
        return Err(PortableLevelRenderError::InvalidPaletteShape(
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
    if rows.len() < REQUIRED_PALETTE_ROWS {
        return Err(PortableLevelRenderError::TooFewPaletteRows(rows.len()));
    }
    Ok(rows)
}

pub(super) fn validate_assets(
    instances: &[crate::TileInstance],
    graphics: &GraphicsInterchangeFile,
    palettes: &[Palette],
) -> Result<(), PortableLevelRenderError> {
    for (instance, value) in instances.iter().enumerate() {
        if value.tile_index >= graphics.graphics.tiles.len() {
            return Err(PortableLevelRenderError::MissingGraphicsTile {
                instance,
                tile: value.tile_index,
            });
        }
        if value.palette_index >= palettes.len() {
            return Err(PortableLevelRenderError::MissingPaletteRow {
                instance,
                row: value.palette_index,
            });
        }
    }
    Ok(())
}

pub(super) const fn placement(width: usize, height: usize) -> GridPlacement {
    GridPlacement {
        width,
        height,
        origin_x: 0,
        origin_y: 0,
    }
}
