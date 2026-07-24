use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::{GraphicsInterchangeFile, MaterializedAnimationFrame};
use lm_level::Map16SetFile;
use lm_overworld::SpriteAppearanceFile;
use lm_project::CompleteOverworldFile;
use lm_render::{encode_png, render_portable_overworld};
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) struct OverworldRenderRequest<'a> {
    pub overworld: &'a Path,
    pub size_modes: &'a Path,
    pub maximum_animation_records: usize,
    pub map16: &'a Path,
    pub graphics: &'a Path,
    pub appearances: Option<&'a Path>,
    pub animation_frame: Option<&'a Path>,
    pub completed_reveals: usize,
    pub output: &'a Path,
}

pub(crate) fn execute(
    request: OverworldRenderRequest<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [
        request.overworld,
        request.size_modes,
        request.map16,
        request.graphics,
    ]
    .contains(&request.output)
        || request.appearances == Some(request.output)
        || request.animation_frame == Some(request.output)
    {
        return Err("render output must differ from every input".into());
    }
    let modes = crate::size_mode_file::read(request.size_modes)?;
    let overworld = CompleteOverworldFile::decode(
        &read_bounded(request.overworld, CompleteOverworldFile::MAX_FILE_LEN)?,
        request.maximum_animation_records,
        &modes,
    )?;
    let map16 = Map16SetFile::decode(&read_bounded(request.map16, Map16SetFile::MAX_FILE_LEN)?)?;
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        request.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let appearances = if let Some(path) = request.appearances {
        Some(SpriteAppearanceFile::decode(&read_bounded(
            path,
            SpriteAppearanceFile::MAX_FILE_LEN,
        )?)?)
    } else {
        None
    };
    let animation_frame = if let Some(path) = request.animation_frame {
        Some(MaterializedAnimationFrame::decode(&read_bounded(
            path,
            MaterializedAnimationFrame::MAX_FILE_LEN,
        )?)?)
    } else {
        None
    };
    let canvas = render_portable_overworld(
        &overworld,
        &map16,
        &graphics,
        appearances.as_ref(),
        animation_frame.as_ref(),
        request.completed_reveals,
    )?;
    write_new(request.output, encode_png(&canvas)?)?;
    println!("width: {}", canvas.width());
    println!("height: {}", canvas.height());
    println!("completed-reveals: {}", request.completed_reveals);
    println!(
        "sprites-skipped-without-appearance-definitions: {}",
        if appearances.is_some() {
            0
        } else {
            overworld.data.sprites.len()
        }
    );
    println!("output: {}", request.output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{
        Bgr555, CompactExAnimation, GraphicsFile4bpp, IndexedTile, MaterializedPaletteOverride,
        MaterializedTileOverride, Palette,
    };
    use lm_level::{Map16Page, Map16Set, Map16Tile, Subtile};
    use lm_overworld::{EventReveal, EventRevealTable, OverworldLayer};
    use lm_overworld::{OverworldSprite, SpriteAppearanceDefinition, SpriteAppearancePart, Submap};
    use lm_project::{CompleteOverworldData, CompleteOverworldShape, OverworldLayers};
    use lm_render::Rgba;

    fn assets() -> (CompleteOverworldFile, Map16SetFile, GraphicsInterchangeFile) {
        let tile = |graphics| Map16Tile {
            top_left: Subtile(graphics),
            top_right: Subtile(graphics),
            bottom_left: Subtile(graphics),
            bottom_right: Subtile(graphics),
            acts_like: 0,
        };
        let mut definitions = vec![tile(0); Map16Page::TILE_COUNT];
        definitions[1] = tile(1);
        let overworld = CompleteOverworldFile {
            source_slot: 0,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 1,
                endpoints: 0,
                messages: 0,
                sprites: 0,
                sprite_record_len: 3,
                palette_colors: 16,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                },
                event_reveals: EventRevealTable {
                    entries: vec![EventReveal {
                        source_tile: 0,
                        destination_tile: 1,
                    }],
                },
                endpoints: vec![],
                messages: vec![],
                sprites: vec![],
                palette: Palette {
                    colors: (0..16)
                        .map(|index| {
                            Bgr555(if index == 1 {
                                0x001f
                            } else if index == 2 {
                                0x03e0
                            } else {
                                0
                            })
                        })
                        .collect(),
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: vec![],
                },
            },
        };
        let map16 = Map16SetFile {
            set: Map16Set {
                pages: vec![Map16Page::new(definitions).unwrap()],
            },
        };
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; 64]), IndexedTile::new([2; 64])],
            },
        };
        (overworld, map16, graphics)
    }

    #[test]
    fn reveals_change_pixels_and_png_is_deterministic() {
        let (overworld, map16, graphics) = assets();
        let before =
            render_portable_overworld(&overworld, &map16, &graphics, None, None, 0).unwrap();
        let after =
            render_portable_overworld(&overworld, &map16, &graphics, None, None, 1).unwrap();
        assert_eq!(
            before.get(0, 0),
            Some(Rgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255
            })
        );
        assert_eq!(
            after.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255
            })
        );
        assert_eq!(
            lm_oracle::sha256_hex(&encode_png(&after).unwrap()),
            "2ec19dc535e091168dbdb741b4c82a7f8b077d6fa0be2b8f9d9aa8dff5a00bea"
        );
    }

    #[test]
    fn malformed_references_and_reveal_bounds_fail() {
        let (mut overworld, map16, mut graphics) = assets();
        assert!(render_portable_overworld(&overworld, &map16, &graphics, None, None, 2).is_err());
        overworld.data.layers.layer2.tiles[0] = 0x100;
        assert!(render_portable_overworld(&overworld, &map16, &graphics, None, None, 0).is_err());
        overworld.data.layers.layer2.tiles[0] = 0;
        graphics.graphics.tiles.clear();
        assert!(render_portable_overworld(&overworld, &map16, &graphics, None, None, 0).is_err());
    }

    #[test]
    fn supplied_sprite_appearance_paints_after_layers() {
        let (mut overworld, map16, graphics) = assets();
        overworld.data.sprites.push(OverworldSprite {
            id: 0x123,
            x: 0,
            y: 0,
            submap: Submap::Main,
            extra: vec![],
        });
        let appearances = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x123,
                parts: vec![SpriteAppearancePart {
                    tile_index: 1,
                    palette_index: 0,
                    x_offset: 0,
                    y_offset: 0,
                    x_flip: false,
                    y_flip: false,
                }],
            }],
        };
        let canvas =
            render_portable_overworld(&overworld, &map16, &graphics, Some(&appearances), None, 0)
                .unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn materialized_animation_changes_tiles_and_palette_before_rendering() {
        let (overworld, map16, graphics) = assets();
        let frame = MaterializedAnimationFrame {
            tick: 7,
            tile_overrides: vec![MaterializedTileOverride {
                tile_index: 0,
                tile: IndexedTile::new([2; 64]),
            }],
            palette_overrides: vec![MaterializedPaletteOverride {
                color_index: 2,
                color: Bgr555(0x7c00),
            }],
        };
        let canvas =
            render_portable_overworld(&overworld, &map16, &graphics, None, Some(&frame), 0)
                .unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            })
        );
    }

    #[test]
    fn output_cannot_alias_an_input() {
        let path = Path::new("same.file");
        assert!(
            execute(OverworldRenderRequest {
                overworld: path,
                size_modes: path,
                maximum_animation_records: 1,
                map16: path,
                graphics: path,
                appearances: None,
                animation_frame: None,
                completed_reveals: 0,
                output: path,
            })
            .is_err()
        );
    }
}
