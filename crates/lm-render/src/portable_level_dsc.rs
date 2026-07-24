use crate::level::build_level_scene_with_cell_blends;
use crate::portable_level::{palette_rows, placement, validate_assets, validate_layer3};
use crate::scene::draw_scene_with_average;
use crate::{Canvas, PortableLevelRenderDimensions, PortableLevelRenderError};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{
    CompleteLevelFile, DscMaterializationContext, DscMaterializationError, DscResolvedTable,
    EntityAppearanceFile, Map16SetFile,
};
use std::fmt;

#[derive(Debug)]
pub enum PortableDscLevelRenderError {
    Materialization {
        layer: u8,
        error: DscMaterializationError,
    },
    MissingMappedTile {
        layer: u8,
        cell: usize,
        tile: u16,
    },
    Base(PortableLevelRenderError),
}

impl fmt::Display for PortableDscLevelRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable DSC level render failed: {self:?}")
    }
}

impl std::error::Error for PortableDscLevelRenderError {}

#[derive(Clone, Copy)]
pub struct PortableDscLevelRenderRequest<'a> {
    pub appearances: Option<&'a EntityAppearanceFile>,
    pub layer3: Option<&'a crate::MaterializedLayer3Plane>,
    pub dimensions: PortableLevelRenderDimensions,
    pub dsc: &'a DscResolvedTable,
    pub context: DscMaterializationContext,
}

/// Materializes alternate mappings, applies direct display substitutions, and renders a level.
///
/// Native per-cell `0x20` flags describe editor diagnostics and remain available from the
/// materializer; this gameplay-oriented render boundary does not enable editor overlays.
///
/// # Errors
///
/// Rejects invalid Acts Like chains, missing source or mapped definitions, malformed assets and
/// dimensions, stale Layer 3 materializations, or excessive canvas allocation.
pub fn render_portable_level_with_dsc(
    level: &CompleteLevelFile,
    map16: &Map16SetFile,
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
    request: PortableDscLevelRenderRequest<'_>,
) -> Result<Canvas, PortableDscLevelRenderError> {
    let mut displayed = level.clone();
    let layer1_average = materialize_layer(
        1,
        &mut displayed.0.layer1.raw_tilemap,
        map16,
        request.dsc,
        request.context,
    )?;
    let layer2_average = materialize_layer(
        2,
        &mut displayed.0.layer2.raw_tilemap,
        map16,
        request.dsc,
        request.context,
    )?;
    validate_layer3(&displayed, request.layer3).map_err(PortableDscLevelRenderError::Base)?;
    let definitions: Vec<_> = map16
        .set
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect();
    let appearances = request
        .appearances
        .map(crate::resolve_entity_appearances)
        .unwrap_or_default();
    let layout = crate::LevelSceneLayout {
        layer1: placement(
            request.dimensions.layer1_width,
            request.dimensions.layer1_height,
        ),
        layer2: placement(
            request.dimensions.layer2_width,
            request.dimensions.layer2_height,
        ),
    };
    let blended = build_level_scene_with_cell_blends(
        &displayed.0,
        layout,
        &definitions,
        &appearances,
        request.layer3,
        &layer1_average,
        &layer2_average,
    )
    .map_err(|error| PortableDscLevelRenderError::Base(PortableLevelRenderError::Scene(error)))?;
    let palettes = palette_rows(&palette.palette).map_err(PortableDscLevelRenderError::Base)?;
    validate_assets(&blended.scene.instances, graphics, &palettes)
        .map_err(PortableDscLevelRenderError::Base)?;
    let width = request
        .dimensions
        .layer1_width
        .max(request.dimensions.layer2_width)
        .checked_mul(16)
        .ok_or(PortableDscLevelRenderError::Base(
            PortableLevelRenderError::DimensionOverflow,
        ))?;
    let height = request
        .dimensions
        .layer1_height
        .max(request.dimensions.layer2_height)
        .checked_mul(16)
        .ok_or(PortableDscLevelRenderError::Base(
            PortableLevelRenderError::DimensionOverflow,
        ))?;
    if width == 0 || height == 0 {
        return Err(PortableDscLevelRenderError::Base(
            PortableLevelRenderError::EmptyDimensions,
        ));
    }
    let mut canvas = Canvas::try_new(width, height).map_err(|error| {
        PortableDscLevelRenderError::Base(PortableLevelRenderError::Canvas(error))
    })?;
    draw_scene_with_average(
        &mut canvas,
        &blended.scene,
        &blended.average,
        &graphics.graphics.tiles,
        &palettes,
    );
    Ok(canvas)
}

fn materialize_layer(
    layer: u8,
    cells: &mut [u16],
    map16: &Map16SetFile,
    dsc: &DscResolvedTable,
    context: DscMaterializationContext,
) -> Result<Vec<bool>, PortableDscLevelRenderError> {
    let materialized = dsc
        .materialize_cells(cells, &map16.set, context)
        .map_err(|error| PortableDscLevelRenderError::Materialization { layer, error })?;
    let mut average = Vec::with_capacity(cells.len());
    for (cell, (source, mapping)) in cells.iter_mut().zip(materialized.mappings).enumerate() {
        let selected = if mapping == 0 {
            *source
        } else {
            mapping & 0x7fff
        };
        let resolution = dsc.resolve_display(selected, context.display);
        if map16.set.tile(resolution.tile_id).is_none() {
            return Err(PortableDscLevelRenderError::MissingMappedTile {
                layer,
                cell,
                tile: resolution.tile_id,
            });
        }
        *source = resolution.tile_id;
        average.push(resolution.blended);
    }
    Ok(average)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use lm_level::{
        DscDescriptionStyle, DscDisplayContext, DscSidecar, LayerData, Level, Map16Page, Map16Set,
        Map16Tile, Subtile,
    };

    #[test]
    fn alternate_then_direct_mapping_is_composed_before_painter_order() {
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[1].acts_like = 1;
        definitions[2] = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0),
            bottom_left: Subtile(0),
            bottom_right: Subtile(0),
            acts_like: 2,
        };
        let map16 = Map16SetFile {
            set: Map16Set {
                pages: vec![Map16Page::new(definitions).unwrap()],
            },
        };
        let source = DscSidecar::decode(b"0\t10\t1\n1\t4\t2\n2\t8\tdim\n").unwrap();
        let dsc = DscResolvedTable::from_sidecar(
            &source,
            DscDescriptionStyle {
                background: 0,
                detail: 0,
                foreground: 0,
                mode: 0,
            },
        );
        let level = CompleteLevelFile(Level {
            layer1: LayerData {
                raw_tilemap: vec![0],
                ..LayerData::default()
            },
            layer2: LayerData {
                raw_tilemap: Vec::new(),
                ..LayerData::default()
            },
            ..Level::default()
        });
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; IndexedTile::PIXEL_COUNT])],
            },
        };
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: [Bgr555(0), Bgr555(0x001f)]
                    .into_iter()
                    .chain(std::iter::repeat_n(Bgr555(0), 126))
                    .collect(),
            },
        };
        let canvas = render_portable_level_with_dsc(
            &level,
            &map16,
            &graphics,
            &palette,
            PortableDscLevelRenderRequest {
                appearances: None,
                layer3: None,
                dimensions: PortableLevelRenderDimensions {
                    layer1_width: 1,
                    layer1_height: 1,
                    layer2_width: 0,
                    layer2_height: 0,
                },
                dsc: &dsc,
                context: DscMaterializationContext {
                    custom_display_enabled: true,
                    display: DscDisplayContext {
                        first_feature_enabled: true,
                        ..DscDisplayContext::default()
                    },
                    ..DscMaterializationContext::default()
                },
            },
        )
        .unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().red, 127);
    }
}
