//! Lunar Magic's variable-length custom overworld sprite stream.

use std::fmt;

pub const CUSTOM_OVERWORLD_MAP_COUNT: usize = 7;
pub const CUSTOM_OVERWORLD_SPRITES_PER_MAP: usize = 24;
pub const CUSTOM_OVERWORLD_SPRITE_ID_COUNT: usize = 128;
const OFFSET_TABLE_LEN: usize = CUSTOM_OVERWORLD_MAP_COUNT * 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCustomOverworldSprite {
    pub id: u8,
    pub x: u16,
    pub y: u16,
    pub screen: u8,
    pub extra: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCustomOverworldSpriteTable {
    pub maps: [Vec<NativeCustomOverworldSprite>; CUSTOM_OVERWORLD_MAP_COUNT],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeCustomOverworldSpriteError {
    TruncatedOffsetTable,
    OffsetOutOfBounds {
        map: usize,
        offset: usize,
    },
    TruncatedRecord {
        map: usize,
        offset: usize,
    },
    InvalidRecordSize {
        id: u8,
        size: u8,
    },
    MissingTerminator(usize),
    TooManySprites {
        map: usize,
        count: usize,
    },
    IdOutOfRange(u8),
    CoordinateOutOfRange {
        axis: &'static str,
        value: u16,
    },
    CoordinateNotGridAligned {
        axis: &'static str,
        value: u16,
    },
    ScreenOutOfRange(u8),
    ScreenNotGridAligned(u8),
    ExtraLength {
        map: usize,
        record: usize,
        actual: usize,
        expected: usize,
    },
    SizeOverflow,
}

impl fmt::Display for NativeCustomOverworldSpriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid native custom overworld sprite stream: {self:?}"
        )
    }
}

impl std::error::Error for NativeCustomOverworldSpriteError {}

impl NativeCustomOverworldSpriteTable {
    /// Decodes the seven offset-addressed, zero-terminated map streams.
    ///
    /// `record_sizes[id]` is Lunar Magic's complete record length, including the packed
    /// three-byte prefix.
    ///
    /// # Errors
    ///
    /// Rejects invalid offsets, truncated or unterminated streams, invalid size-table entries,
    /// and maps exceeding Lunar Magic's recovered 24-record limit.
    pub fn decode(
        bytes: &[u8],
        record_sizes: &[u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    ) -> Result<Self, NativeCustomOverworldSpriteError> {
        let offsets = bytes
            .get(..OFFSET_TABLE_LEN)
            .ok_or(NativeCustomOverworldSpriteError::TruncatedOffsetTable)?;
        let mut maps: [Vec<NativeCustomOverworldSprite>; CUSTOM_OVERWORLD_MAP_COUNT] =
            std::array::from_fn(|_| Vec::new());
        for (map, output) in maps.iter_mut().enumerate() {
            let pair = &offsets[map * 2..map * 2 + 2];
            let mut cursor = usize::from(u16::from_le_bytes([pair[0], pair[1]]));
            if cursor >= bytes.len() {
                return Err(NativeCustomOverworldSpriteError::OffsetOutOfBounds {
                    map,
                    offset: cursor,
                });
            }
            loop {
                let terminator = bytes
                    .get(cursor..cursor + 2)
                    .ok_or(NativeCustomOverworldSpriteError::MissingTerminator(map))?;
                if terminator == [0, 0] {
                    break;
                }
                let prefix = bytes.get(cursor..cursor + 3).ok_or(
                    NativeCustomOverworldSpriteError::TruncatedRecord {
                        map,
                        offset: cursor,
                    },
                )?;
                if output.len() == CUSTOM_OVERWORLD_SPRITES_PER_MAP {
                    return Err(NativeCustomOverworldSpriteError::TooManySprites {
                        map,
                        count: output.len() + 1,
                    });
                }
                let packed = u32::from_le_bytes([prefix[0], prefix[1], prefix[2], 0]);
                let id = (packed & 0x7f) as u8;
                let size = record_sizes[usize::from(id)];
                if size < 3 {
                    return Err(NativeCustomOverworldSpriteError::InvalidRecordSize { id, size });
                }
                let end = cursor
                    .checked_add(usize::from(size))
                    .ok_or(NativeCustomOverworldSpriteError::SizeOverflow)?;
                let record = bytes.get(cursor..end).ok_or(
                    NativeCustomOverworldSpriteError::TruncatedRecord {
                        map,
                        offset: cursor,
                    },
                )?;
                output.push(NativeCustomOverworldSprite {
                    id,
                    x: ((packed >> 7) & 0x3f) as u16 * 8,
                    y: ((packed >> 13) & 0x3f) as u16 * 8,
                    screen: ((packed >> 19) & 0x1f) as u8 * 8,
                    extra: record[3..].to_vec(),
                });
                cursor = end;
                if cursor >= bytes.len() {
                    return Err(NativeCustomOverworldSpriteError::MissingTerminator(map));
                }
            }
        }
        Ok(Self { maps })
    }

    /// Encodes the canonical offset table and seven terminated map streams.
    ///
    /// # Errors
    ///
    /// Rejects records outside the recovered packed field bounds, non-grid-aligned coordinates,
    /// invalid extension lengths, and maps exceeding 24 sprites.
    pub fn encode(
        &self,
        record_sizes: &[u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    ) -> Result<Vec<u8>, NativeCustomOverworldSpriteError> {
        let mut output = vec![0; OFFSET_TABLE_LEN];
        for (map, records) in self.maps.iter().enumerate() {
            if records.len() > CUSTOM_OVERWORLD_SPRITES_PER_MAP {
                return Err(NativeCustomOverworldSpriteError::TooManySprites {
                    map,
                    count: records.len(),
                });
            }
            let current = u16::try_from(output.len())
                .map_err(|_| NativeCustomOverworldSpriteError::SizeOverflow)?;
            let offset = if records.is_empty() && map != 0 {
                current
                    .checked_sub(2)
                    .ok_or(NativeCustomOverworldSpriteError::SizeOverflow)?
            } else {
                current
            };
            output[map * 2..map * 2 + 2].copy_from_slice(&offset.to_le_bytes());

            for (record_index, record) in records.iter().enumerate() {
                if usize::from(record.id) >= CUSTOM_OVERWORLD_SPRITE_ID_COUNT {
                    return Err(NativeCustomOverworldSpriteError::IdOutOfRange(record.id));
                }
                validate_coordinate("x", record.x)?;
                validate_coordinate("y", record.y)?;
                if record.screen > 0xf8 {
                    return Err(NativeCustomOverworldSpriteError::ScreenOutOfRange(
                        record.screen,
                    ));
                }
                if record.screen & 7 != 0 {
                    return Err(NativeCustomOverworldSpriteError::ScreenNotGridAligned(
                        record.screen,
                    ));
                }
                let size = record_sizes[usize::from(record.id)];
                if size < 3 {
                    return Err(NativeCustomOverworldSpriteError::InvalidRecordSize {
                        id: record.id,
                        size,
                    });
                }
                let expected = usize::from(size) - 3;
                if record.extra.len() != expected {
                    return Err(NativeCustomOverworldSpriteError::ExtraLength {
                        map,
                        record: record_index,
                        actual: record.extra.len(),
                        expected,
                    });
                }
                let mut packed = u32::from(record.id)
                    | (u32::from(record.x / 8) << 7)
                    | (u32::from(record.y / 8) << 13)
                    | (u32::from(record.screen / 8) << 19);
                // Lunar Magic uses bit 7 as an escape when the packed prefix would otherwise
                // collide with the two-zero-byte map terminator.
                if packed.trailing_zeros() >= 16 {
                    packed |= 0x80;
                }
                output.extend_from_slice(&packed.to_le_bytes()[..3]);
                output.extend_from_slice(&record.extra);
            }
            output.extend_from_slice(&[0, 0]);
        }
        Ok(output)
    }
}

fn validate_coordinate(
    axis: &'static str,
    value: u16,
) -> Result<(), NativeCustomOverworldSpriteError> {
    if value > 0x1f8 {
        return Err(NativeCustomOverworldSpriteError::CoordinateOutOfRange { axis, value });
    }
    if value & 7 != 0 {
        return Err(NativeCustomOverworldSpriteError::CoordinateNotGridAligned { axis, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes() -> [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT] {
        let mut sizes = [4; CUSTOM_OVERWORLD_SPRITE_ID_COUNT];
        sizes[3] = 6;
        sizes
    }

    #[test]
    fn round_trips_variable_records_and_empty_map_aliases() {
        let table = NativeCustomOverworldSpriteTable {
            maps: [
                vec![NativeCustomOverworldSprite {
                    id: 3,
                    x: 0x118,
                    y: 0x1f0,
                    screen: 0x28,
                    extra: vec![0xaa, 0xbb, 0xcc],
                }],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
        };
        let encoded = table.encode(&sizes()).unwrap();
        assert_eq!(&encoded[..2], &14_u16.to_le_bytes());
        assert_eq!(&encoded[2..4], &20_u16.to_le_bytes());
        assert_eq!(
            NativeCustomOverworldSpriteTable::decode(&encoded, &sizes()).unwrap(),
            table
        );
    }

    #[test]
    fn zero_prefix_is_escaped_like_lunar_magic() {
        let table = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                if map == 0 {
                    vec![NativeCustomOverworldSprite {
                        id: 0,
                        x: 0,
                        y: 0,
                        screen: 0,
                        extra: vec![0],
                    }]
                } else {
                    Vec::new()
                }
            }),
        };
        let encoded = table.encode(&sizes()).unwrap();
        assert_eq!(encoded[OFFSET_TABLE_LEN], 0x80);
        let decoded = NativeCustomOverworldSpriteTable::decode(&encoded, &sizes()).unwrap();
        assert_eq!(decoded.maps[0][0].x, 8);
    }

    #[test]
    fn rejects_wrong_variable_extension_length() {
        let table = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                if map == 0 {
                    vec![NativeCustomOverworldSprite {
                        id: 3,
                        x: 8,
                        y: 8,
                        screen: 0,
                        extra: vec![0],
                    }]
                } else {
                    Vec::new()
                }
            }),
        };
        assert!(matches!(
            table.encode(&sizes()),
            Err(NativeCustomOverworldSpriteError::ExtraLength { .. })
        ));
    }

    #[test]
    fn out_of_range_id_is_typed_instead_of_indexing_the_size_table() {
        let table = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                if map == 0 {
                    vec![NativeCustomOverworldSprite {
                        id: 0x80,
                        x: 0,
                        y: 0,
                        screen: 0,
                        extra: vec![0],
                    }]
                } else {
                    Vec::new()
                }
            }),
        };
        assert_eq!(
            table.encode(&sizes()),
            Err(NativeCustomOverworldSpriteError::IdOutOfRange(0x80))
        );
    }
}
