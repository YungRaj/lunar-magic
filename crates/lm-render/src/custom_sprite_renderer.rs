use crate::StandardSpritePreviewTile;
use crate::standard_sprite_renderer::preview_definition;
use lm_level::{SscDirective, SscEntry, SscResolvedSprite, SscResolvedTable};

/// One custom-sprite display definition with Lunar Magic's global graphics/palette remaps
/// retained explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemappedCustomSpritePreviewTile {
    pub definition_index: u16,
    pub subtiles: [u16; 4],
    pub graphics_base: u16,
    pub palette_source: Option<u16>,
    pub x: i16,
    pub y: i16,
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

/// Resolves a remapped SSC display when it can be drawn by the ordinary 1,024-tile sprite atlas.
///
/// Custom palette blocks require a palette-aware raster source and therefore return `None` here
/// instead of silently drawing with the wrong vanilla colors.
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
    let tile = (word & 0x03ff).checked_add(graphics_base)?;
    (tile < 0x400).then_some((word & !0x03ff) | tile)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let source = SscSidecar::decode(b"10\t2\t0,0,20\n10000\t1\t20-20,30\n").unwrap();
        let table = lm_level::SscResolvedTable::from_sidecar(&source);
        let sprite = table.default_display(0x10, 0).unwrap();
        let parts = render_atlas_lunar_magic_custom_sprite_with(&table, sprite, |_| {
            Some([1, 0x4002, 0x8003, 0xc004])
        })
        .unwrap();
        assert_eq!(parts[0].subtiles, [0x31, 0x4032, 0x8033, 0xc034]);

        let external = SscSidecar::decode(b"10\t2\t0,0,20\n10000\t2\t20-20,30\n").unwrap();
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
}
