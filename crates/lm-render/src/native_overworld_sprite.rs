//! Native `.sscov` display resolution through built-in and `.s16ov` Sprite Map16 definitions.

use crate::{Canvas, native_level_raster};
use lm_graphics::{IndexedTile, Palette};
use lm_level::{Map16Tile, S16OvSidecar, Subtile};
use lm_overworld::{NativeOverworldSpriteDisplay, NativeOverworldSpriteSidecar};

/// One sprite placement in native editor pixel coordinates.
///
/// IDs `$000..$0ff` are original sprites and `$100..$17f` are Lunar Magic custom sprites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpritePlacement {
    pub id: u16,
    pub x: i32,
    pub y: i32,
}

/// A display element after `.sscov` Sprite Map16 references have been expanded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedNativeOverworldSpriteElement {
    Tile {
        sprite_index: usize,
        tile_number: u16,
        palette: u8,
        x: i32,
        y: i32,
        priority: bool,
        x_flip: bool,
        y_flip: bool,
        translucent: bool,
    },
    Label {
        sprite_index: usize,
        x: i32,
        y: i32,
        text: String,
    },
    /// Lunar Magic's dynamic editor-only `$C00..$CFF` Sprite Map16 definition cache.
    EditorTextDefinition {
        sprite_index: usize,
        definition_index: u8,
        x: i32,
        y: i32,
        translucent: bool,
    },
    /// A caller omitted a built-in definition. Preserve the reference instead of making the
    /// sprite disappear.
    UnresolvedMap16 {
        sprite_index: usize,
        native_tile: u16,
        x: i32,
        y: i32,
        translucent: bool,
    },
}

/// Resolves native overworld sprite appearances to 8x8 tile and label elements.
///
/// `builtin_sprite_map16` owns native definitions `$000..$3FF`; `.s16ov` owns `$400..$BFF`.
/// Missing appearances intentionally emit no replacement elements, allowing the caller to retain
/// its ordinary built-in sprite renderer.
#[must_use]
pub fn resolve_native_overworld_sprite_elements(
    placements: &[NativeOverworldSpritePlacement],
    definitions: &NativeOverworldSpriteSidecar,
    builtin_sprite_map16: &[Map16Tile],
    custom_sprite_map16: &S16OvSidecar,
) -> Vec<ResolvedNativeOverworldSpriteElement> {
    let mut output = Vec::new();
    for (sprite_index, placement) in placements.iter().enumerate() {
        let Some(appearance) = definitions.appearances.get(&placement.id) else {
            continue;
        };
        if appearance.shadow {
            expand_map16(
                &mut output,
                sprite_index,
                placement.x,
                placement.y,
                0x20,
                true,
                builtin_sprite_map16,
                custom_sprite_map16,
            );
        }
        match &appearance.display {
            NativeOverworldSpriteDisplay::Tiles(parts) => {
                for part in parts {
                    expand_map16(
                        &mut output,
                        sprite_index,
                        placement.x + i32::from(part.x),
                        placement.y + i32::from(part.y),
                        part.tile,
                        part.translucent,
                        builtin_sprite_map16,
                        custom_sprite_map16,
                    );
                }
            }
            NativeOverworldSpriteDisplay::Label { x, y, text } => {
                output.push(ResolvedNativeOverworldSpriteElement::Label {
                    sprite_index,
                    x: placement.x + i32::from(*x),
                    y: placement.y + i32::from(*y),
                    text: text.clone(),
                });
            }
        }
    }
    output
}

/// Paints resolved native overworld sprite elements in their retained painter order.
///
/// Tile translucency uses Lunar Magic's packed-channel average with the existing framebuffer.
/// Internal `$C00..$CFF` references use the authenticated dynamic editor-text cache; labels use
/// the authenticated native editor font cache. Unresolved definitions intentionally remain
/// unpainted so callers can surface them separately.
pub fn draw_resolved_native_overworld_sprite_elements(
    canvas: &mut Canvas,
    elements: &[ResolvedNativeOverworldSpriteElement],
    ordinary_tiles: &[IndexedTile],
    animated_tiles: &[IndexedTile],
    palette: &Palette,
) {
    for element in elements {
        match element {
            ResolvedNativeOverworldSpriteElement::Tile {
                tile_number,
                palette: palette_index,
                x,
                y,
                priority,
                x_flip,
                y_flip,
                translucent,
                ..
            } => {
                let word = tile_number
                    | (u16::from(*palette_index) << 10)
                    | (u16::from(*priority) << 13)
                    | (u16::from(*x_flip) << 14)
                    | (u16::from(*y_flip) << 15);
                native_level_raster::draw_sprite_subtile_clipped(
                    canvas,
                    word,
                    if word & 0x0200 != 0 {
                        animated_tiles
                    } else {
                        ordinary_tiles
                    },
                    palette,
                    (*x, *y),
                    *translucent,
                );
            }
            ResolvedNativeOverworldSpriteElement::Label { x, y, text, .. } => {
                crate::draw_lunar_magic_editor_label(canvas, text, *x, *y);
            }
            ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
                definition_index,
                x,
                y,
                ..
            } => native_level_raster::draw_lunar_magic_editor_text_definition(
                canvas,
                *definition_index,
                *x,
                *y,
            ),
            ResolvedNativeOverworldSpriteElement::UnresolvedMap16 { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn expand_map16(
    output: &mut Vec<ResolvedNativeOverworldSpriteElement>,
    sprite_index: usize,
    x: i32,
    y: i32,
    native_tile: u16,
    translucent: bool,
    builtin: &[Map16Tile],
    custom: &S16OvSidecar,
) {
    if (0xc00..=0xcff).contains(&native_tile) {
        output.push(ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
            sprite_index,
            definition_index: native_tile as u8,
            x,
            y,
            translucent,
        });
        return;
    }
    let definition = if usize::from(native_tile) < S16OvSidecar::FIRST_NATIVE_TILE {
        builtin.get(usize::from(native_tile)).copied()
    } else {
        custom.native_tile(usize::from(native_tile))
    };
    let Some(definition) = definition else {
        output.push(ResolvedNativeOverworldSpriteElement::UnresolvedMap16 {
            sprite_index,
            native_tile,
            x,
            y,
            translucent,
        });
        return;
    };
    for (subtile, dx, dy) in [
        (definition.top_left, 0, 0),
        (definition.top_right, 8, 0),
        (definition.bottom_left, 0, 8),
        (definition.bottom_right, 8, 8),
    ] {
        push_subtile(output, sprite_index, subtile, x + dx, y + dy, translucent);
    }
}

fn push_subtile(
    output: &mut Vec<ResolvedNativeOverworldSpriteElement>,
    sprite_index: usize,
    subtile: Subtile,
    x: i32,
    y: i32,
    translucent: bool,
) {
    output.push(ResolvedNativeOverworldSpriteElement::Tile {
        sprite_index,
        tile_number: subtile.tile_number(),
        palette: subtile.palette(),
        x,
        y,
        priority: subtile.priority(),
        x_flip: subtile.x_flip(),
        y_flip: subtile.y_flip(),
        translucent,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Bgr555;
    use lm_overworld::{NativeOverworldSpriteAppearance, NativeOverworldSpriteMap16Part};
    use std::collections::BTreeMap;

    fn definitions() -> NativeOverworldSpriteSidecar {
        NativeOverworldSpriteSidecar {
            tooltips: BTreeMap::new(),
            appearances: BTreeMap::from([
                (
                    0x102,
                    NativeOverworldSpriteAppearance {
                        shadow: true,
                        display: NativeOverworldSpriteDisplay::Tiles(vec![
                            NativeOverworldSpriteMap16Part {
                                x: -4,
                                y: 6,
                                tile: 0x400,
                                translucent: true,
                            },
                            NativeOverworldSpriteMap16Part {
                                x: 20,
                                y: 6,
                                tile: 0xc00,
                                translucent: false,
                            },
                        ]),
                    },
                ),
                (
                    3,
                    NativeOverworldSpriteAppearance {
                        shadow: false,
                        display: NativeOverworldSpriteDisplay::Label {
                            x: 2,
                            y: -3,
                            text: "Warp".into(),
                        },
                    },
                ),
            ]),
            graphics_ranges: Vec::new(),
            palette_ranges: Vec::new(),
        }
    }

    #[test]
    fn resolves_shadow_custom_tiles_labels_and_retains_internal_references() {
        let mut builtin = vec![Map16Tile::default(); 0x400];
        builtin[0x20] = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0,
        };
        let custom = S16OvSidecar::decode(&[5, 0x04, 6, 0x40, 7, 0x80, 8, 0xe0]).unwrap();
        let result = resolve_native_overworld_sprite_elements(
            &[
                NativeOverworldSpritePlacement {
                    id: 0x102,
                    x: 100,
                    y: 40,
                },
                NativeOverworldSpritePlacement {
                    id: 3,
                    x: 10,
                    y: 20,
                },
            ],
            &definitions(),
            &builtin,
            &custom,
        );

        assert_eq!(result.len(), 10);
        assert!(matches!(
            result[0],
            ResolvedNativeOverworldSpriteElement::Tile {
                tile_number: 1,
                x: 100,
                y: 40,
                translucent: true,
                ..
            }
        ));
        assert!(matches!(
            result[4],
            ResolvedNativeOverworldSpriteElement::Tile {
                tile_number: 5,
                palette: 1,
                x: 96,
                y: 46,
                translucent: true,
                ..
            }
        ));
        assert_eq!(
            result[8],
            ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
                sprite_index: 0,
                definition_index: 0,
                x: 120,
                y: 46,
                translucent: false,
            }
        );
        assert_eq!(
            result[9],
            ResolvedNativeOverworldSpriteElement::Label {
                sprite_index: 1,
                x: 12,
                y: 17,
                text: "Warp".into(),
            }
        );
    }

    #[test]
    fn rasterizes_tiles_translucency_and_internal_editor_text() {
        let backdrop = crate::Rgba {
            red: 20,
            green: 40,
            blue: 60,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(32, 16, vec![backdrop; 512]).unwrap();
        let mut pixels = [0; 64];
        pixels.fill(1);
        let tile = IndexedTile::new(pixels);
        let mut colors = vec![Bgr555(0); 256];
        colors[8 * 16 + 1] = Bgr555(0x001f);
        let palette = Palette { colors };
        draw_resolved_native_overworld_sprite_elements(
            &mut canvas,
            &[
                ResolvedNativeOverworldSpriteElement::Tile {
                    sprite_index: 0,
                    tile_number: 0,
                    palette: 0,
                    x: 0,
                    y: 0,
                    priority: false,
                    x_flip: false,
                    y_flip: false,
                    translucent: true,
                },
                ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
                    sprite_index: 0,
                    definition_index: b'A',
                    x: 16,
                    y: 0,
                    translucent: false,
                },
            ],
            std::slice::from_ref(&tile),
            &[],
            &palette,
        );
        assert_eq!(
            canvas.get(0, 0),
            Some(crate::Rgba {
                red: 137,
                green: 20,
                blue: 30,
                alpha: 255,
            })
        );
        assert!(
            (16..32)
                .flat_map(|x| (0..16).map(move |y| (x, y)))
                .any(|(x, y)| canvas.get(x, y) != Some(backdrop))
        );
    }
}
