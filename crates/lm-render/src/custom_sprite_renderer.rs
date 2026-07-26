use crate::standard_sprite_renderer::preview_definition;
use crate::{Canvas, Rgba, StandardSpritePreviewTile};
use lm_graphics::{ExternalSpriteAssets, IndexedTile, Rgb8};
use lm_level::{SscDirective, SscEntry, SscResolvedSprite, SscResolvedTable};

/// One custom-sprite display definition with Lunar Magic's global graphics/palette remaps
/// retained explicitly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RemappedCustomSpritePreviewTile {
    pub definition_index: u16,
    pub subtiles: [u16; 4],
    pub graphics_base: u16,
    pub palette_source: Option<u16>,
    pub x: i16,
    pub y: i16,
}

/// Rasterizes one 16×16 SSC Map16 definition backed by Lunar Magic's external sprite assets.
///
/// Color zero remains transparent. The Map16 palette and flip bits are applied independently to
/// each 8×8 subtile, while `graphics_base` selects the global external-graphics tile page.
#[must_use]
pub fn raster_external_custom_sprite_tile(
    part: &RemappedCustomSpritePreviewTile,
    assets: &ExternalSpriteAssets,
) -> Option<Canvas> {
    let palette_source = part.palette_source?;
    raster_remapped_custom_sprite_tile_with(
        part,
        |global_tile| assets.graphics_tile(global_tile),
        |_, palette, color| assets.palette_color(palette_source, palette, color),
    )
}

/// Rasterizes one remapped SSC definition with caller-owned indexed graphics and palette sources.
///
/// This handles mixed definitions that combine ordinary SP graphics with an external palette or
/// external graphics with the level's ordinary sprite palette.
#[must_use]
pub fn raster_remapped_custom_sprite_tile_with<'a>(
    part: &RemappedCustomSpritePreviewTile,
    mut tile: impl FnMut(u16) -> Option<&'a IndexedTile>,
    mut color: impl FnMut(Option<u16>, u8, u8) -> Option<Rgb8>,
) -> Option<Canvas> {
    let mut canvas = Canvas::try_new(16, 16).ok()?;
    for (quadrant, word) in part.subtiles.into_iter().enumerate() {
        let global_tile = (word & 0x03ff).checked_add(part.graphics_base)?;
        let tile = tile(global_tile)?;
        let palette = u8::try_from((word >> 10) & 7).ok()?;
        draw_remapped_subtile(
            &mut canvas,
            tile,
            part.palette_source,
            palette,
            word & 0x4000 != 0,
            word & 0x8000 != 0,
            (quadrant & 1) * 8,
            (quadrant >> 1) * 8,
            &mut color,
        )?;
    }
    Some(canvas)
}

#[allow(clippy::too_many_arguments)]
fn draw_remapped_subtile(
    canvas: &mut Canvas,
    tile: &IndexedTile,
    palette_source: Option<u16>,
    palette: u8,
    flip_x: bool,
    flip_y: bool,
    target_x: usize,
    target_y: usize,
    color_resolver: &mut impl FnMut(Option<u16>, u8, u8) -> Option<Rgb8>,
) -> Option<()> {
    for row in 0..IndexedTile::HEIGHT {
        for column in 0..IndexedTile::WIDTH {
            let source_x = if flip_x {
                IndexedTile::WIDTH - 1 - column
            } else {
                column
            };
            let source_y = if flip_y {
                IndexedTile::HEIGHT - 1 - row
            } else {
                row
            };
            let color = tile.pixels()[source_y * IndexedTile::WIDTH + source_x];
            if color == 0 {
                continue;
            }
            let rgb = color_resolver(palette_source, palette, color)?;
            canvas.set(
                target_x + column,
                target_y + row,
                Rgba {
                    red: rgb.red,
                    green: rgb.green,
                    blue: rgb.blue,
                    alpha: 255,
                },
            );
        }
    }
    Some(())
}

/// Materializes one decoded `.ssc` display record through the same preview-definition table used
/// by Lunar Magic's standard sprite renderer.
///
/// Returns `None` for non-display records or when a referenced definition has not been loaded.
#[must_use]
pub fn render_lunar_magic_custom_sprite(
    entry: &SscEntry,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let SscDirective::Display(display) = &entry.directive else {
        return None;
    };
    display
        .iter()
        .map(|tile| {
            Some(StandardSpritePreviewTile {
                definition_index: tile.tile,
                subtiles: preview_definition(tile.tile)?,
                x: tile.x,
                y: tile.y,
            })
        })
        .collect()
}

/// Renders a materialized custom-sprite display after source-order replacement has been applied.
#[must_use]
pub fn render_resolved_lunar_magic_custom_sprite(
    sprite: &SscResolvedSprite,
) -> Option<Vec<StandardSpritePreviewTile>> {
    render_resolved_lunar_magic_custom_sprite_with(sprite, preview_definition)
}

/// Renders a materialized custom-sprite display with a caller-owned Map16 definition resolver.
///
/// Lunar Magic routes SSC display records through the same Map16 renderer used by external
/// object definitions. Frontends with an open `.m16` sidecar can therefore resolve its definitions
/// first and delegate unclaimed indexes to the built-in table.
#[must_use]
pub fn render_resolved_lunar_magic_custom_sprite_with(
    sprite: &SscResolvedSprite,
    mut definition: impl FnMut(u16) -> Option<[u16; 4]>,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let display = sprite.display.as_ref()?;
    display
        .iter()
        .map(|tile| {
            Some(StandardSpritePreviewTile {
                definition_index: tile.tile,
                subtiles: definition(tile.tile).or_else(|| preview_definition(tile.tile))?,
                x: tile.x,
                y: tile.y,
            })
        })
        .collect()
}

/// Resolves a custom display while preserving the global SSC graphics-base and palette-source
/// tables used by Lunar Magic's renderer.
///
/// The returned subtile words remain the selected Map16 definition. `graphics_base` is added to
/// each subtile's ten-bit tile number at draw time; `palette_source` selects an SSC custom palette
/// block instead of the ordinary sprite palette when present.
#[must_use]
pub fn render_remapped_lunar_magic_custom_sprite_with(
    table: &SscResolvedTable,
    sprite: &SscResolvedSprite,
    mut definition: impl FnMut(u16) -> Option<[u16; 4]>,
) -> Option<Vec<RemappedCustomSpritePreviewTile>> {
    let display = sprite.display.as_ref()?;
    display
        .iter()
        .map(|tile| {
            Some(RemappedCustomSpritePreviewTile {
                definition_index: tile.tile,
                subtiles: definition(tile.tile).or_else(|| preview_definition(tile.tile))?,
                graphics_base: table.tile_remap(tile.tile).unwrap_or(0),
                palette_source: table.palette_remap(tile.tile),
                x: tile.x,
                y: tile.y,
            })
        })
        .collect()
}

/// Resolves a remapped SSC display when it selects Lunar Magic's `$400` sprite-graphics region.
///
/// Custom palette blocks require a palette-aware raster source and therefore return `None` here
/// instead of silently drawing with the wrong vanilla colors. Foreground (`$000`) and Layer 3
/// (`$900`) sources also return `None`; they are not aliases of the sprite atlas.
#[must_use]
pub fn render_atlas_lunar_magic_custom_sprite_with(
    table: &SscResolvedTable,
    sprite: &SscResolvedSprite,
    definition: impl FnMut(u16) -> Option<[u16; 4]>,
) -> Option<Vec<StandardSpritePreviewTile>> {
    render_remapped_lunar_magic_custom_sprite_with(table, sprite, definition)?
        .into_iter()
        .map(|tile| {
            if tile.palette_source.is_some() {
                return None;
            }
            let subtiles = tile
                .subtiles
                .map(|word| remap_atlas_subtile(word, tile.graphics_base))
                .into_iter()
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;
            Some(StandardSpritePreviewTile {
                definition_index: tile.definition_index,
                subtiles,
                x: tile.x,
                y: tile.y,
            })
        })
        .collect()
}

fn remap_atlas_subtile(word: u16, graphics_base: u16) -> Option<u16> {
    let tile = (word & 0x03ff)
        .checked_add(graphics_base)?
        .checked_sub(0x400)?;
    (tile < 0x400).then_some((word & !0x03ff) | tile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Rgb8, encode_4bpp_tile};
    use lm_level::SscSidecar;

    #[test]
    fn renders_explicit_and_text_macro_definitions() {
        let sidecar = SscSidecar::decode(b"12\t2\t-8,4,10;8,4,*A*\n").unwrap();
        let parts = render_lunar_magic_custom_sprite(&sidecar.entries()[0]).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].definition_index, 0x10);
        assert_eq!((parts[0].x, parts[0].y), (-8, 4));
        assert_eq!(parts[1].definition_index, 0x3c7c);
        assert_eq!(parts[2].definition_index, 0x3c41);
    }

    #[test]
    fn rejects_non_display_and_unknown_definitions() {
        let descriptions = SscSidecar::decode(b"1\t0\tdescription\n").unwrap();
        assert!(render_lunar_magic_custom_sprite(&descriptions.entries()[0]).is_none());
        let unknown = SscSidecar::decode(b"1\t2\t0,0,3BFF\n").unwrap();
        assert!(render_lunar_magic_custom_sprite(&unknown.entries()[0]).is_none());
    }

    #[test]
    fn caller_can_resolve_external_map16_definitions() {
        let sidecar = SscSidecar::decode(b"12\t2\t-8,4,3BFF\n").unwrap();
        let resolved = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let sprite = resolved.default_display(0x12, 0).unwrap();
        let parts = render_resolved_lunar_magic_custom_sprite_with(sprite, |index| {
            (index == 0x3bff).then_some([0x1111, 0x2222, 0x3333, 0x4444])
        })
        .unwrap();
        assert_eq!(parts[0].definition_index, 0x3bff);
        assert_eq!(parts[0].subtiles, [0x1111, 0x2222, 0x3333, 0x4444]);
    }

    #[test]
    fn resolved_render_retains_global_graphics_and_palette_remaps() {
        let sidecar =
            SscSidecar::decode(b"10\t2\t-8,4,20\n10000\t1\t20-20,30\n20000\t0\t20-20,7\n").unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&sidecar);
        let sprite = table.default_display(0x10, 0).unwrap();
        let parts = render_remapped_lunar_magic_custom_sprite_with(&table, sprite, |_| {
            Some([1, 0x4002, 0x8003, 0xc004])
        })
        .unwrap();
        assert_eq!(
            parts,
            [RemappedCustomSpritePreviewTile {
                definition_index: 0x20,
                subtiles: [1, 0x4002, 0x8003, 0xc004],
                graphics_base: 0x30,
                palette_source: Some(7),
                x: -8,
                y: 4,
            }]
        );
        assert!(
            render_atlas_lunar_magic_custom_sprite_with(&table, sprite, |_| Some([1, 2, 3, 4]))
                .is_none(),
            "the vanilla atlas must not fabricate an SSC custom palette"
        );
    }

    #[test]
    fn atlas_render_applies_representable_tile_bases_and_rejects_external_pages() {
        let source = SscSidecar::decode(b"10\t2\t0,0,20\n10000\t2\t20-20,30\n").unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&source);
        let sprite = table.default_display(0x10, 0).unwrap();
        let parts = render_atlas_lunar_magic_custom_sprite_with(&table, sprite, |_| {
            Some([1, 0x4002, 0x8003, 0xc004])
        })
        .unwrap();
        assert_eq!(parts[0].subtiles, [0x31, 0x4032, 0x8033, 0xc034]);

        let foreground = SscSidecar::decode(b"10\t2\t0,0,20\n10000\t1\t20-20,30\n").unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&foreground);
        assert!(
            render_atlas_lunar_magic_custom_sprite_with(
                &table,
                table.default_display(0x10, 0).unwrap(),
                |_| Some([1, 2, 3, 4]),
            )
            .is_none()
        );

        let external = SscSidecar::decode(b"10\t2\t0,0,20\n10000\t0\t20-20,30\n").unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&external);
        assert!(
            render_atlas_lunar_magic_custom_sprite_with(
                &table,
                table.default_display(0x10, 0).unwrap(),
                |_| Some([1, 2, 3, 4]),
            )
            .is_none()
        );
    }

    #[test]
    fn external_raster_applies_palette_rows_transparency_and_subtile_flips() {
        let mut assets = ExternalSpriteAssets::default();
        let pixels = std::array::from_fn(|index| {
            if index == 0 {
                1
            } else if index == IndexedTile::PIXEL_COUNT - 1 {
                2
            } else {
                0
            }
        });
        assets
            .set_graphics_slot(0, &encode_4bpp_tile(&IndexedTile::new(pixels)).unwrap())
            .unwrap();
        let red = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let blue = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        });
        let mut palette = vec![0; 0x43 * 2];
        palette[0x41 * 2..0x41 * 2 + 2].copy_from_slice(&red.0.to_le_bytes());
        palette[0x42 * 2..0x42 * 2 + 2].copy_from_slice(&blue.0.to_le_bytes());
        assets.set_snes_palette(&palette).unwrap();

        let part = RemappedCustomSpritePreviewTile {
            definition_index: 0x20,
            subtiles: [0x0800, 0x4800, 0x8800, 0xc800],
            graphics_base: 0x2000,
            palette_source: Some(2),
            x: 0,
            y: 0,
        };
        let raster = raster_external_custom_sprite_tile(&part, &assets).unwrap();
        let red = Rgba {
            red: red.to_rgb8().red,
            green: red.to_rgb8().green,
            blue: red.to_rgb8().blue,
            alpha: 255,
        };
        let blue = Rgba {
            red: blue.to_rgb8().red,
            green: blue.to_rgb8().green,
            blue: blue.to_rgb8().blue,
            alpha: 255,
        };
        assert_eq!(raster.get(0, 0), Some(red));
        assert_eq!(raster.get(15, 0), Some(red));
        assert_eq!(raster.get(0, 15), Some(red));
        assert_eq!(raster.get(15, 15), Some(red));
        assert_eq!(raster.get(7, 7), Some(blue));
        assert_eq!(raster.get(8, 7), Some(blue));
        assert_eq!(raster.get(7, 8), Some(blue));
        assert_eq!(raster.get(8, 8), Some(blue));
        assert_eq!(raster.get(1, 0), Some(Rgba::default()));
    }

    #[test]
    fn mixed_raster_accepts_vanilla_tiles_or_vanilla_palette_independently() {
        let tile = IndexedTile::new([1; IndexedTile::PIXEL_COUNT]);
        let custom_palette_part = RemappedCustomSpritePreviewTile {
            definition_index: 1,
            subtiles: [0x0800; 4],
            graphics_base: 0,
            palette_source: Some(3),
            x: 0,
            y: 0,
        };
        let red = Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        };
        let raster = raster_remapped_custom_sprite_tile_with(
            &custom_palette_part,
            |index| (index == 0).then_some(&tile),
            |source, palette, color| {
                (source == Some(3) && palette == 2 && color == 1).then_some(red)
            },
        )
        .unwrap();
        assert_eq!(
            raster.get(0, 0),
            Some(Rgba {
                red: 255,
                alpha: 255,
                ..Rgba::default()
            })
        );

        let vanilla_palette_part = RemappedCustomSpritePreviewTile {
            palette_source: None,
            graphics_base: 0x2000,
            ..custom_palette_part
        };
        let blue = Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        };
        let raster = raster_remapped_custom_sprite_tile_with(
            &vanilla_palette_part,
            |index| (index == 0x2000).then_some(&tile),
            |source, palette, color| {
                (source.is_none() && palette == 2 && color == 1).then_some(blue)
            },
        )
        .unwrap();
        assert_eq!(
            raster.get(15, 15),
            Some(Rgba {
                blue: 255,
                alpha: 255,
                ..Rgba::default()
            })
        );
    }
}
