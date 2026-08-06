//! Native `.sscov` display resolution through built-in and `.s16ov` Sprite Map16 definitions.

use crate::{Canvas, native_level_raster};
use lm_graphics::{IndexedTile, Palette};
use lm_level::{Map16Tile, NativeMap16SidecarError, S16OvSidecar, Subtile};
use lm_overworld::{
    NativeOverworldSpriteAppearance, NativeOverworldSpriteDisplay, NativeOverworldSpriteMap16Part,
    NativeOverworldSpriteSidecar, NativeOverworldSpriteSidecarError, SpriteAppearanceDefinition,
    SpriteAppearanceFile, SpriteAppearanceFileError, SpriteAppearancePart,
};
use std::{collections::BTreeMap, fmt};

const LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES: &[u8; 0x2000] =
    include_bytes!("assets/lm363-overworld-sprite-map16-builtins.bin");
static LM363_BUILTIN_OVERWORLD_SPRITE_MAP16: [Map16Tile; 0x400] =
    decode_builtin_overworld_sprite_map16();

/// Returns Lunar Magic 3.63's exact four built-in overworld Sprite Map16 pages (`$000..$3FF`).
#[must_use]
pub const fn lunar_magic_builtin_overworld_sprite_map16() -> &'static [Map16Tile; 0x400] {
    &LM363_BUILTIN_OVERWORLD_SPRITE_MAP16
}

const fn decode_builtin_overworld_sprite_map16() -> [Map16Tile; 0x400] {
    let mut output = [Map16Tile {
        top_left: Subtile(0),
        top_right: Subtile(0),
        bottom_left: Subtile(0),
        bottom_right: Subtile(0),
        acts_like: 0,
    }; 0x400];
    let mut index = 0;
    while index < output.len() {
        let offset = index * Map16Tile::GRAPHICS_LEN;
        output[index] = Map16Tile {
            top_left: Subtile(u16::from_le_bytes([
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset],
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 1],
            ])),
            top_right: Subtile(u16::from_le_bytes([
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 2],
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 3],
            ])),
            bottom_left: Subtile(u16::from_le_bytes([
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 4],
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 5],
            ])),
            bottom_right: Subtile(u16::from_le_bytes([
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 6],
                LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES[offset + 7],
            ])),
            acts_like: 0,
        };
        index += 1;
    }
    output
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldAppearancePair {
    pub definitions: NativeOverworldSpriteSidecar,
    pub sprite_map16: S16OvSidecar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldAppearanceConversionError {
    Portable(SpriteAppearanceFileError),
    Sidecar(NativeOverworldSpriteSidecarError),
    Map16(NativeMap16SidecarError),
    Shadow(u16),
    Label(u16),
    Translucent {
        sprite_id: u16,
        part: usize,
    },
    MissingDefinition {
        sprite_id: u16,
        native_tile: u16,
    },
    Priority {
        sprite_id: u16,
        part: usize,
    },
    CoordinateOverflow {
        sprite_id: u16,
        part: usize,
    },
    IncompleteQuadrantGroup {
        sprite_id: u16,
        parts: usize,
    },
    InvalidQuadrantGeometry {
        sprite_id: u16,
        group: usize,
    },
    TileOutOfRange {
        sprite_id: u16,
        part: usize,
        tile: u16,
    },
    TooManyMap16Definitions(usize),
}

impl fmt::Display for NativeOverworldAppearanceConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot convert overworld sprite appearances: {self:?}"
        )
    }
}

impl std::error::Error for NativeOverworldAppearanceConversionError {}

/// Imports the exactly representable visual subset of Lunar Magic's native sidecars.
///
/// Portable appearances have no shadow, text-label, translucency, or priority fields, so those
/// native constructs are rejected explicitly instead of being silently discarded.
pub fn import_native_overworld_appearances(
    definitions: &NativeOverworldSpriteSidecar,
    builtin_sprite_map16: &[Map16Tile],
    custom_sprite_map16: &S16OvSidecar,
) -> Result<SpriteAppearanceFile, NativeOverworldAppearanceConversionError> {
    let mut portable = Vec::with_capacity(definitions.appearances.len());
    for (&sprite_id, appearance) in &definitions.appearances {
        if appearance.shadow {
            return Err(NativeOverworldAppearanceConversionError::Shadow(sprite_id));
        }
        let NativeOverworldSpriteDisplay::Tiles(parts) = &appearance.display else {
            return Err(NativeOverworldAppearanceConversionError::Label(sprite_id));
        };
        let mut output_parts = Vec::with_capacity(parts.len().saturating_mul(4));
        for (part_index, part) in parts.iter().enumerate() {
            if part.translucent {
                return Err(NativeOverworldAppearanceConversionError::Translucent {
                    sprite_id,
                    part: part_index,
                });
            }
            let definition =
                resolve_map16_definition(part.tile, builtin_sprite_map16, custom_sprite_map16)
                    .ok_or(
                        NativeOverworldAppearanceConversionError::MissingDefinition {
                            sprite_id,
                            native_tile: part.tile,
                        },
                    )?;
            for (subtile, dx, dy) in [
                (definition.top_left, 0_i16, 0_i16),
                (definition.top_right, 8, 0),
                (definition.bottom_left, 0, 8),
                (definition.bottom_right, 8, 8),
            ] {
                if subtile.priority() {
                    return Err(NativeOverworldAppearanceConversionError::Priority {
                        sprite_id,
                        part: part_index,
                    });
                }
                output_parts.push(SpriteAppearancePart {
                    tile_index: subtile.tile_number(),
                    palette_index: subtile.palette(),
                    x_offset: part.x.checked_add(dx).ok_or(
                        NativeOverworldAppearanceConversionError::CoordinateOverflow {
                            sprite_id,
                            part: part_index,
                        },
                    )?,
                    y_offset: part.y.checked_add(dy).ok_or(
                        NativeOverworldAppearanceConversionError::CoordinateOverflow {
                            sprite_id,
                            part: part_index,
                        },
                    )?,
                    x_flip: subtile.x_flip(),
                    y_flip: subtile.y_flip(),
                });
            }
        }
        portable.push(SpriteAppearanceDefinition {
            sprite_id,
            parts: output_parts,
        });
    }
    let file = SpriteAppearanceFile {
        definitions: portable,
    };
    file.encode()
        .map_err(NativeOverworldAppearanceConversionError::Portable)?;
    Ok(file)
}

/// Exports portable appearances whose ordered parts form exact 2x2 Map16 quadrant groups.
///
/// One custom `.s16ov` definition is allocated per four portable parts. Requiring complete groups
/// avoids inventing a supposedly transparent filler tile and guarantees reciprocal import.
pub fn export_native_overworld_appearances(
    portable: &SpriteAppearanceFile,
) -> Result<NativeOverworldAppearancePair, NativeOverworldAppearanceConversionError> {
    portable
        .encode()
        .map_err(NativeOverworldAppearanceConversionError::Portable)?;
    let definition_count = portable
        .definitions
        .iter()
        .try_fold(0_usize, |count, definition| {
            if definition.parts.len() % 4 != 0 {
                return Err(
                    NativeOverworldAppearanceConversionError::IncompleteQuadrantGroup {
                        sprite_id: definition.sprite_id,
                        parts: definition.parts.len(),
                    },
                );
            }
            count.checked_add(definition.parts.len() / 4).ok_or(
                NativeOverworldAppearanceConversionError::TooManyMap16Definitions(usize::MAX),
            )
        })?;
    if definition_count > S16OvSidecar::TILE_COUNT {
        return Err(
            NativeOverworldAppearanceConversionError::TooManyMap16Definitions(definition_count),
        );
    }
    let mut s16ov =
        S16OvSidecar::decode(&[]).map_err(NativeOverworldAppearanceConversionError::Map16)?;
    let mut appearances = BTreeMap::new();
    let mut definition_index = 0_usize;
    for definition in &portable.definitions {
        let mut native_parts = Vec::with_capacity(definition.parts.len() / 4);
        for (group, parts) in definition.parts.chunks_exact(4).enumerate() {
            let x = parts[0].x_offset;
            let y = parts[0].y_offset;
            let expected = [
                (Some(x), Some(y)),
                (x.checked_add(8), Some(y)),
                (Some(x), y.checked_add(8)),
                (x.checked_add(8), y.checked_add(8)),
            ];
            for (part, (expected_x, expected_y)) in parts.iter().zip(expected) {
                if Some(part.x_offset) != expected_x || Some(part.y_offset) != expected_y {
                    return Err(
                        NativeOverworldAppearanceConversionError::InvalidQuadrantGeometry {
                            sprite_id: definition.sprite_id,
                            group,
                        },
                    );
                }
            }
            let mut words = [0_u16; 4];
            for (part_index, part) in parts.iter().enumerate() {
                if part.tile_index > 0x03ff {
                    return Err(NativeOverworldAppearanceConversionError::TileOutOfRange {
                        sprite_id: definition.sprite_id,
                        part: group * 4 + part_index,
                        tile: part.tile_index,
                    });
                }
                words[part_index] = part.tile_index
                    | (u16::from(part.palette_index) << 10)
                    | (u16::from(part.x_flip) << 14)
                    | (u16::from(part.y_flip) << 15);
            }
            let map16 = Map16Tile {
                top_left: Subtile(words[0]),
                top_right: Subtile(words[1]),
                bottom_left: Subtile(words[2]),
                bottom_right: Subtile(words[3]),
                acts_like: 0,
            };
            let encoded = map16.encode_graphics();
            for (entry, bytes) in encoded.chunks_exact(4).enumerate() {
                s16ov
                    .set_entry(
                        definition_index * 2 + entry,
                        u32::from_le_bytes(bytes.try_into().unwrap()),
                    )
                    .map_err(NativeOverworldAppearanceConversionError::Map16)?;
            }
            native_parts.push(NativeOverworldSpriteMap16Part {
                x,
                y,
                tile: u16::try_from(S16OvSidecar::FIRST_NATIVE_TILE + definition_index).map_err(
                    |_| {
                        NativeOverworldAppearanceConversionError::TooManyMap16Definitions(
                            definition_count,
                        )
                    },
                )?,
                translucent: false,
            });
            definition_index += 1;
        }
        appearances.insert(
            definition.sprite_id,
            NativeOverworldSpriteAppearance {
                shadow: false,
                display: NativeOverworldSpriteDisplay::Tiles(native_parts),
            },
        );
    }
    let definitions = NativeOverworldSpriteSidecar {
        tooltips: BTreeMap::new(),
        appearances,
        graphics_ranges: Vec::new(),
        palette_ranges: Vec::new(),
    };
    definitions
        .encode()
        .map_err(NativeOverworldAppearanceConversionError::Sidecar)?;
    Ok(NativeOverworldAppearancePair {
        definitions,
        sprite_map16: s16ov,
    })
}

fn resolve_map16_definition(
    native_tile: u16,
    builtin: &[Map16Tile],
    custom: &S16OvSidecar,
) -> Option<Map16Tile> {
    if usize::from(native_tile) < S16OvSidecar::FIRST_NATIVE_TILE {
        builtin.get(usize::from(native_tile)).copied()
    } else {
        custom.native_tile(usize::from(native_tile))
    }
}

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

    #[test]
    fn portable_native_pair_round_trip_is_exact_for_complete_quadrants() {
        let portable = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x105,
                parts: vec![
                    SpriteAppearancePart {
                        tile_index: 1,
                        palette_index: 2,
                        x_offset: -4,
                        y_offset: 6,
                        x_flip: false,
                        y_flip: false,
                    },
                    SpriteAppearancePart {
                        tile_index: 2,
                        palette_index: 3,
                        x_offset: 4,
                        y_offset: 6,
                        x_flip: true,
                        y_flip: false,
                    },
                    SpriteAppearancePart {
                        tile_index: 3,
                        palette_index: 4,
                        x_offset: -4,
                        y_offset: 14,
                        x_flip: false,
                        y_flip: true,
                    },
                    SpriteAppearancePart {
                        tile_index: 4,
                        palette_index: 5,
                        x_offset: 4,
                        y_offset: 14,
                        x_flip: true,
                        y_flip: true,
                    },
                ],
            }],
        };
        let native = export_native_overworld_appearances(&portable).unwrap();
        assert_eq!(native.sprite_map16.loaded_len(), Map16Tile::GRAPHICS_LEN);
        let encoded = native.definitions.encode().unwrap();
        assert!(
            std::str::from_utf8(&encoded)
                .unwrap()
                .contains("05\t12\t-4,6,400")
        );
        assert_eq!(
            import_native_overworld_appearances(
                &NativeOverworldSpriteSidecar::decode(&encoded).unwrap(),
                &[],
                &S16OvSidecar::decode(&native.sprite_map16.encode()).unwrap(),
            )
            .unwrap(),
            portable
        );
    }

    #[test]
    fn conversion_rejects_every_non_representable_semantic_instead_of_narrowing() {
        let incomplete = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 1,
                parts: vec![SpriteAppearancePart {
                    tile_index: 0,
                    palette_index: 0,
                    x_offset: 0,
                    y_offset: 0,
                    x_flip: false,
                    y_flip: false,
                }],
            }],
        };
        assert!(matches!(
            export_native_overworld_appearances(&incomplete),
            Err(NativeOverworldAppearanceConversionError::IncompleteQuadrantGroup { .. })
        ));

        let mut native = definitions();
        native.appearances.remove(&3);
        assert!(matches!(
            import_native_overworld_appearances(
                &native,
                &vec![Map16Tile::default(); 0x400],
                &S16OvSidecar::decode(&[]).unwrap(),
            ),
            Err(NativeOverworldAppearanceConversionError::Shadow(0x102))
        ));
        native = definitions();
        native.appearances.remove(&0x102);
        assert!(matches!(
            import_native_overworld_appearances(
                &native,
                &vec![Map16Tile::default(); 0x400],
                &S16OvSidecar::decode(&[]).unwrap(),
            ),
            Err(NativeOverworldAppearanceConversionError::Label(3))
        ));
    }

    #[test]
    fn authenticated_builtin_sprite_map16_cache_decodes_all_four_pages_exactly() {
        let definitions = lunar_magic_builtin_overworld_sprite_map16();
        assert_eq!(definitions.len(), 0x400);
        assert_eq!(
            definitions[1].encode_graphics(),
            [0x26, 4, 0x36, 4, 0x27, 4, 0x37, 4]
        );
        assert_eq!(definitions[0x3ff].encode_graphics(), [0; 8]);
        let encoded = definitions
            .iter()
            .flat_map(|definition| definition.encode_graphics())
            .collect::<Vec<_>>();
        assert_eq!(
            encoded.as_slice(),
            LM363_BUILTIN_OVERWORLD_SPRITE_MAP16_BYTES
        );
    }
}
