mod asset_loader;
mod output;

use asset_loader::{load_dsc, load_level_assets};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{
    CompleteLevelFile, DscDescriptionStyle, DscMaterializationContext, DscResolvedTable,
    EntityAppearanceFile, Map16SetFile,
};
use lm_render::{
    Canvas, MaterializedLayer3Plane, PortableDscLevelRenderRequest, PortableLevelRenderDimensions,
    render_portable_level, render_portable_level_with_dsc,
};
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) struct LevelRenderPaths<'a> {
    pub level: &'a Path,
    pub map16: &'a Path,
    pub graphics: &'a Path,
    pub palette: &'a Path,
    pub appearances: Option<&'a Path>,
    pub layer3_plane: Option<&'a Path>,
}

#[derive(Clone, Copy)]
pub(crate) struct LevelRenderRequest<'a> {
    pub paths: LevelRenderPaths<'a>,
    pub dimensions: PortableLevelRenderDimensions,
    pub output: &'a Path,
}

#[derive(Clone, Copy)]
pub(crate) struct DscLevelRenderRequest<'a> {
    pub paths: LevelRenderPaths<'a>,
    pub dsc: &'a Path,
    pub context: DscMaterializationContext,
    pub dimensions: PortableLevelRenderDimensions,
    pub output: &'a Path,
}

#[derive(Clone, Copy)]
struct DecodedLevelAssets<'a> {
    level: &'a CompleteLevelFile,
    map16: &'a Map16SetFile,
    graphics: &'a GraphicsInterchangeFile,
    palette: &'a PaletteInterchangeFile,
    appearances: Option<&'a EntityAppearanceFile>,
    layer3_plane: Option<&'a MaterializedLayer3Plane>,
}

pub(crate) fn execute(request: LevelRenderRequest<'_>) -> Result<(), Box<dyn std::error::Error>> {
    output::validate_distinct(request.paths, None, request.output, "render")?;
    let loaded = load_level_assets(request.paths)?;
    let canvas = render_level(
        DecodedLevelAssets {
            level: &loaded.level,
            map16: &loaded.map16,
            graphics: &loaded.graphics,
            palette: &loaded.palette,
            appearances: loaded.appearances.as_ref(),
            layer3_plane: loaded.layer3_plane.as_ref(),
        },
        request.dimensions,
    )?;
    output::write_canvas(request.output, &canvas, None)?;
    Ok(())
}

pub(crate) fn execute_dsc(
    request: DscLevelRenderRequest<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    output::validate_distinct(
        request.paths,
        Some(request.dsc),
        request.output,
        "DSC level-render",
    )?;
    let loaded = load_level_assets(request.paths)?;
    let dsc_source = load_dsc(request.dsc)?;
    let dsc = DscResolvedTable::from_sidecar(
        &dsc_source,
        DscDescriptionStyle {
            background: 0,
            detail: 0,
            foreground: 0,
            mode: 0,
        },
    );
    let canvas = render_portable_level_with_dsc(
        &loaded.level,
        &loaded.map16,
        &loaded.graphics,
        &loaded.palette,
        PortableDscLevelRenderRequest {
            appearances: loaded.appearances.as_ref(),
            layer3: loaded.layer3_plane.as_ref(),
            dimensions: request.dimensions,
            dsc: &dsc,
            context: request.context,
        },
    )?;
    output::write_canvas(
        request.output,
        &canvas,
        Some(("DSC entries", dsc_source.entries().len())),
    )?;
    Ok(())
}

fn render_level(
    assets: DecodedLevelAssets<'_>,
    dimensions: PortableLevelRenderDimensions,
) -> Result<Canvas, Box<dyn std::error::Error>> {
    Ok(render_portable_level(
        assets.level,
        assets.map16,
        assets.graphics,
        assets.palette,
        assets.appearances,
        assets.layer3_plane,
        dimensions,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use lm_level::{
        AppearanceSource, EntityAppearanceRecord, Layer3File, LayerData, Level, Map16Page,
        Map16Set, Map16Tile, ObjectRecord, Subtile,
    };
    use lm_render::{Rgba, encode_png};

    fn assets() -> (
        CompleteLevelFile,
        Map16SetFile,
        GraphicsInterchangeFile,
        PaletteInterchangeFile,
    ) {
        let level = CompleteLevelFile(Level {
            layer1: LayerData {
                raw_tilemap: vec![1],
                ..LayerData::default()
            },
            layer2: LayerData {
                raw_tilemap: vec![0],
                ..LayerData::default()
            },
            ..Level::default()
        });
        let definition = |tile| Map16Tile {
            top_left: Subtile(tile),
            top_right: Subtile(tile),
            bottom_left: Subtile(tile),
            bottom_right: Subtile(tile),
            acts_like: 0,
        };
        let mut definitions = vec![definition(0); Map16Page::TILE_COUNT];
        definitions[1] = definition(1);
        let map16 = Map16SetFile {
            set: Map16Set {
                pages: vec![Map16Page::new(definitions).unwrap()],
            },
        };
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![
                    IndexedTile::new([1; IndexedTile::PIXEL_COUNT]),
                    IndexedTile::new([2; IndexedTile::PIXEL_COUNT]),
                ],
            },
        };
        let mut colors = vec![Bgr555(0); 8 * 16];
        colors[1] = Bgr555(0x001f);
        colors[2] = Bgr555(0x03e0);
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette { colors },
        };
        (level, map16, graphics, palette)
    }

    fn render_assets(
        level: &CompleteLevelFile,
        map16: &Map16SetFile,
        graphics: &GraphicsInterchangeFile,
        palette: &PaletteInterchangeFile,
        appearances: Option<&EntityAppearanceFile>,
        layer3_plane: Option<&MaterializedLayer3Plane>,
        dimensions: [usize; 4],
    ) -> Result<Canvas, Box<dyn std::error::Error>> {
        render_level(
            DecodedLevelAssets {
                level,
                map16,
                graphics,
                palette,
                appearances,
                layer3_plane,
            },
            PortableLevelRenderDimensions {
                layer1_width: dimensions[0],
                layer1_height: dimensions[1],
                layer2_width: dimensions[2],
                layer2_height: dimensions[3],
            },
        )
    }

    #[test]
    fn layer_one_paints_over_layer_two_with_exact_dimensions() {
        let (level, map16, graphics, palette) = assets();
        let canvas = render_assets(
            &level,
            &map16,
            &graphics,
            &palette,
            None,
            None,
            [1, 1, 1, 1],
        )
        .unwrap();
        assert_eq!((canvas.width(), canvas.height()), (16, 16));
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255,
            })
        );
        assert_eq!(
            lm_oracle::sha256_hex(&encode_png(&canvas).unwrap()),
            "2ec19dc535e091168dbdb741b4c82a7f8b077d6fa0be2b8f9d9aa8dff5a00bea"
        );
    }

    #[test]
    fn shape_missing_map16_and_missing_graphics_fail() {
        let (mut level, map16, mut graphics, palette) = assets();
        assert!(
            render_assets(
                &level,
                &map16,
                &graphics,
                &palette,
                None,
                None,
                [2, 1, 1, 1],
            )
            .is_err()
        );
        level.0.layer1.raw_tilemap[0] = 0x100;
        assert!(
            render_assets(
                &level,
                &map16,
                &graphics,
                &palette,
                None,
                None,
                [1, 1, 1, 1],
            )
            .is_err()
        );
        level.0.layer1.raw_tilemap[0] = 1;
        graphics.graphics.tiles.truncate(1);
        assert!(
            render_assets(
                &level,
                &map16,
                &graphics,
                &palette,
                None,
                None,
                [1, 1, 1, 1],
            )
            .is_err()
        );
    }

    #[test]
    fn supplied_object_appearance_paints_after_raw_layers() {
        let (mut level, map16, graphics, palette) = assets();
        level
            .0
            .layer1
            .objects
            .records
            .push(ObjectRecord::new(vec![1, 2, 3]).unwrap());
        let appearances = EntityAppearanceFile {
            appearances: vec![EntityAppearanceRecord {
                source: AppearanceSource::Layer1Object(0),
                tile_index: 0,
                palette_index: 0,
                x: 0,
                y: 0,
                x_flip: false,
                y_flip: false,
            }],
        };
        let canvas = render_assets(
            &level,
            &map16,
            &graphics,
            &palette,
            Some(&appearances),
            None,
            [1, 1, 1, 1],
        )
        .unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn layer_three_plane_is_source_bound_and_composited() {
        let (mut level, map16, graphics, palette) = assets();
        level.0.layer3 = Some(lm_level::Layer3Data::default());
        let source = Layer3File(level.0.layer3.clone().unwrap())
            .encode()
            .unwrap();
        let mut plane = MaterializedLayer3Plane {
            source_digest: lm_oracle::sha256(&source),
            placement: lm_render::Layer3Placement::AboveEntities,
            instances: vec![lm_render::TileInstance {
                tile_index: 0,
                palette_index: 0,
                x: 0,
                y: 0,
                x_flip: false,
                y_flip: false,
            }],
        };
        let canvas = render_assets(
            &level,
            &map16,
            &graphics,
            &palette,
            None,
            Some(&plane),
            [1, 1, 1, 1],
        )
        .unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            })
        );
        plane.source_digest[0] ^= 1;
        assert!(
            render_assets(
                &level,
                &map16,
                &graphics,
                &palette,
                None,
                Some(&plane),
                [1, 1, 1, 1],
            )
            .is_err()
        );
    }

    #[test]
    fn output_cannot_alias_any_input() {
        let path = Path::new("same.file");
        assert!(
            execute(LevelRenderRequest {
                paths: LevelRenderPaths {
                    level: path,
                    map16: path,
                    graphics: path,
                    palette: path,
                    appearances: None,
                    layer3_plane: None,
                },
                dimensions: PortableLevelRenderDimensions {
                    layer1_width: 1,
                    layer1_height: 1,
                    layer2_width: 1,
                    layer2_height: 1,
                },
                output: path,
            })
            .is_err()
        );
    }
}
