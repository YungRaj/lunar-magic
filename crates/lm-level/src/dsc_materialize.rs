use crate::{DscDisplayContext, DscResolvedTable, Map16Set, Map16SetError};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DscMaterializationContext {
    pub custom_display_enabled: bool,
    pub special_markers_enabled: bool,
    pub display: DscDisplayContext,
    pub level_mode: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscMaterialization {
    /// Zero means no substitution. Nonzero words retain native `0x4000`/`0x8000` control bits.
    pub mappings: Vec<u16>,
    /// Per-cell native flags; this pass owns bit `0x20` and leaves other bits clear.
    pub flags: Vec<u8>,
}

impl DscResolvedTable {
    /// Reproduces `BuildMap16CustomDisplayMappings` for an arbitrary bounded cell slice.
    ///
    /// # Errors
    ///
    /// Rejects source IDs absent from `map16` and invalid Acts Like chains needed by expanded
    /// definitions. The operation returns no partial buffers on failure.
    pub fn materialize_cells(
        &self,
        cells: &[u16],
        map16: &Map16Set,
        context: DscMaterializationContext,
    ) -> Result<DscMaterialization, DscMaterializationError> {
        let mut mappings = Vec::with_capacity(cells.len());
        let mut flags = Vec::with_capacity(cells.len());
        for (position, source) in cells.iter().copied().enumerate() {
            let definition =
                map16
                    .tile(source & 0x7fff)
                    .ok_or(DscMaterializationError::MissingSource {
                        position,
                        tile: source & 0x7fff,
                    })?;
            let acts = definition.acts_like & 0x7fff;
            let (root, lookup) = if acts > 0x1ff {
                let resolved = map16
                    .resolve_acts_like(source & 0x7fff, map16_tile_count(map16))
                    .map_err(|error| DscMaterializationError::ActsLike { position, error })?;
                (resolved.terminal & 0x7fff, resolved.terminal & 0x7fff)
            } else {
                (acts, source & 0x7fff)
            };
            let flag = marker_flag(self, source & 0x7fff, root, context);
            let mapping = if context.custom_display_enabled {
                alternate_or_builtin(self, lookup, root, position, context)
            } else {
                0
            };
            mappings.push(mapping);
            flags.push(flag);
        }
        Ok(DscMaterialization { mappings, flags })
    }
}

fn marker_flag(
    table: &DscResolvedTable,
    source: u16,
    root: u16,
    context: DscMaterializationContext,
) -> u8 {
    if !context.special_markers_enabled {
        return 0;
    }
    let built_in = matches!(root, 0x1f | 0x20 | 0x27 | 0x28 | 0x137 | 0x138 | 0x13f)
        || root == 0x9c && context.level_mode == 1;
    let dsc_marker = table
        .get(root)
        .is_some_and(|entry| entry.native_flags & 8 != 0)
        || table
            .get(source)
            .is_some_and(|entry| entry.native_flags & 8 != 0);
    if built_in || dsc_marker { 0x20 } else { 0 }
}

fn alternate_or_builtin(
    table: &DscResolvedTable,
    lookup: u16,
    root: u16,
    position: usize,
    context: DscMaterializationContext,
) -> u16 {
    if let Some(entry) = table.get(lookup)
        && let Some(mapping) = entry.alternate_mapping.filter(|mapping| *mapping != 0)
    {
        let mut encoded = mapping;
        if entry.native_flags & 1 != 0 {
            encoded |= 0x4000;
        }
        if entry.native_flags & 2 != 0 && !context.display.first_feature_suppressed {
            return if context.display.first_feature_enabled {
                encoded | 0x8000
            } else {
                0
            };
        }
        if entry.native_flags & 4 != 0 {
            if context.display.second_feature_enabled {
                return encoded | 0x8000;
            }
            return built_in_mapping(root, position, context);
        }
        return encoded;
    }
    built_in_mapping(root, position, context)
}

fn built_in_mapping(root: u16, position: usize, context: DscMaterializationContext) -> u16 {
    let Some(value) = (match root {
        0x21 => context.display.second_feature_enabled.then_some(0x1a),
        0x22 => Some(if context.display.second_feature_enabled {
            0x8100
        } else {
            0
        }),
        0x29 if !context.display.first_feature_suppressed => {
            Some(if context.display.first_feature_enabled {
                0x801a
            } else {
                0
            })
        }
        0x111 => Some([0x104, 0x106, 0x105][(position & 0xf) % 3]),
        0x114 => Some(0x96),
        0x117 | 0x11f => Some(0x104),
        0x118 | 0x120 | 0x12a => Some(0x106),
        0x119 | 0x121 => Some(0x105),
        0x11a => Some([0x219, 0x100, 0xc8][(position & 0xf) % 3]),
        0x11b | 0x123 => Some(0x21a),
        0x11c | 0x124 => Some(0x1a),
        0x11d => Some([0x55, 0x65][position & 1]),
        0x122 => Some(0x219),
        0x125 => Some([0x0b, 0x07, 0xba, 0x30][position & 3]),
        0x126 => Some(0x70),
        0x127 | 0x128 => Some(0x30),
        0x129 => Some(0x80b8),
        0x12d => Some(0x100),
        0x16a if !matches!(context.level_mode, 4 | 5 | 0x0d) => Some(0x106),
        0x16b if !matches!(context.level_mode, 4 | 5 | 0x0d) => Some(0x101),
        _ => None,
    }) else {
        return 0;
    };
    if value != 0 && value & 0x8000 == 0 {
        value | 0x4000
    } else {
        value
    }
}

fn map16_tile_count(map16: &Map16Set) -> usize {
    map16
        .pages
        .iter()
        .map(|page| page.tiles.len())
        .sum::<usize>()
        .max(1)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DscMaterializationError {
    MissingSource {
        position: usize,
        tile: u16,
    },
    ActsLike {
        position: usize,
        error: Map16SetError,
    },
}

impl fmt::Display for DscMaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "DSC cell materialization failed: {self:?}")
    }
}

impl std::error::Error for DscMaterializationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DscDescriptionStyle, DscSidecar, Map16Page, Map16Tile};

    const DEFAULTS: DscDescriptionStyle = DscDescriptionStyle {
        background: 0,
        detail: 0,
        foreground: 0,
        mode: 0,
    };

    fn set() -> Map16Set {
        Map16Set {
            pages: (0..2)
                .map(|page| {
                    Map16Page::new(
                        (0..Map16Page::TILE_COUNT)
                            .map(|index| Map16Tile {
                                acts_like: u16::try_from(page * Map16Page::TILE_COUNT + index)
                                    .unwrap(),
                                ..Map16Tile::default()
                            })
                            .collect(),
                    )
                    .unwrap()
                })
                .collect(),
        }
    }

    #[test]
    fn alternate_conditions_and_native_control_bits_are_preserved() {
        let source = DscSidecar::decode(
            b"10\t10\t20\n11\t2\t30\n11\t10\t21\n12\t4\t31\n12\t8\tdim\n12\t10\t22\n",
        )
        .unwrap();
        let table = DscResolvedTable::from_sidecar(&source, DEFAULTS);
        let context = DscMaterializationContext {
            custom_display_enabled: true,
            display: DscDisplayContext {
                first_feature_enabled: true,
                second_feature_enabled: true,
                ..DscDisplayContext::default()
            },
            ..DscMaterializationContext::default()
        };
        let result = table
            .materialize_cells(&[0x10, 0x11, 0x12], &set(), context)
            .unwrap();
        assert_eq!(result.mappings, [0x20, 0x8021, 0xc022]);
    }

    #[test]
    fn built_in_position_patterns_and_markers_match_native_tables() {
        let table = DscResolvedTable::from_sidecar(&DscSidecar::decode(b"").unwrap(), DEFAULTS);
        let mut map16 = set();
        map16.pages[0].tiles[0].acts_like = 0x111;
        let context = DscMaterializationContext {
            custom_display_enabled: true,
            special_markers_enabled: true,
            ..DscMaterializationContext::default()
        };
        let result = table
            .materialize_cells(&[0, 0, 0], &map16, context)
            .unwrap();
        assert_eq!(result.mappings, [0x4104, 0x4106, 0x4105]);

        map16.pages[0].tiles[1].acts_like = 0x1f;
        assert_eq!(
            table
                .materialize_cells(&[1], &map16, context)
                .unwrap()
                .flags,
            [0x20]
        );
    }

    #[test]
    fn missing_sources_fail_without_partial_output() {
        let table = DscResolvedTable::from_sidecar(&DscSidecar::decode(b"").unwrap(), DEFAULTS);
        assert!(matches!(
            table.materialize_cells(&[0, 0x200], &set(), DscMaterializationContext::default()),
            Err(DscMaterializationError::MissingSource {
                position: 1,
                tile: 0x200
            })
        ));
    }
}
