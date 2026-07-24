//! Lunar Magic's native fixed-width overworld level-name table.

use crate::OverworldLevelName;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeOverworldLevelNameTable {
    pub names: Vec<OverworldLevelName>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldLevelNameError {
    TooManyNames(usize),
    NonCanonicalLevel {
        slot: usize,
        expected: u16,
        actual: u16,
    },
    UnsupportedRawFlags {
        slot: usize,
        flags: u8,
    },
    MisalignedTable(usize),
    InvalidVanillaCodeIndex {
        name: usize,
        part: &'static str,
        index: usize,
    },
    InvalidSegmentOffset {
        name: usize,
        part: &'static str,
        offset: usize,
    },
    UnterminatedSegment {
        name: usize,
        part: &'static str,
    },
}

impl std::fmt::Display for NativeOverworldLevelNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld level-name table: {self:?}"
        )
    }
}

impl std::error::Error for NativeOverworldLevelNameError {}

impl NativeOverworldLevelNameTable {
    pub const RECORD_LEN: usize = OverworldLevelName::TILE_COUNT;
    pub const MAX_NAMES: usize = 256;
    pub const VANILLA_NAMES: usize = 93;

    /// Converts Lunar Magic's positional slot number into the corresponding SMW level number.
    #[must_use]
    pub fn level_for_slot(slot: usize) -> Option<u16> {
        if slot >= Self::MAX_NAMES {
            None
        } else {
            let slot = u16::try_from(slot).ok()?;
            if slot <= 0x24 {
                Some(slot)
            } else {
                Some(slot + 0xdc)
            }
        }
    }

    /// Encodes direct 19-byte records used by the expanded native patch.
    ///
    /// # Errors
    ///
    /// Native records are positional, so names must be a canonical contiguous prefix and cannot
    /// carry portable-only raw flag bits.
    pub fn encode(&self) -> Result<Vec<u8>, NativeOverworldLevelNameError> {
        if self.names.len() > Self::MAX_NAMES {
            return Err(NativeOverworldLevelNameError::TooManyNames(
                self.names.len(),
            ));
        }
        let mut bytes = Vec::with_capacity(self.names.len() * Self::RECORD_LEN);
        for (slot, name) in self.names.iter().enumerate() {
            let expected = Self::level_for_slot(slot).ok_or(
                NativeOverworldLevelNameError::TooManyNames(self.names.len()),
            )?;
            if name.level != expected {
                return Err(NativeOverworldLevelNameError::NonCanonicalLevel {
                    slot,
                    expected,
                    actual: name.level,
                });
            }
            if name.raw_flags != 0 {
                return Err(NativeOverworldLevelNameError::UnsupportedRawFlags {
                    slot,
                    flags: name.raw_flags,
                });
            }
            bytes.extend_from_slice(&name.tiles);
        }
        Ok(bytes)
    }

    /// Decodes direct 19-byte records used by the expanded native patch.
    ///
    /// # Errors
    ///
    /// Rejects partial records and tables exceeding Lunar Magic's 256-slot limit.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeOverworldLevelNameError> {
        if bytes.len() % Self::RECORD_LEN != 0 {
            return Err(NativeOverworldLevelNameError::MisalignedTable(bytes.len()));
        }
        let count = bytes.len() / Self::RECORD_LEN;
        if count > Self::MAX_NAMES {
            return Err(NativeOverworldLevelNameError::TooManyNames(count));
        }
        let names = bytes
            .chunks_exact(Self::RECORD_LEN)
            .enumerate()
            .map(|(slot, bytes)| {
                let mut tiles = [0; OverworldLevelName::TILE_COUNT];
                tiles.copy_from_slice(bytes);
                Ok(OverworldLevelName {
                    level: Self::level_for_slot(slot)
                        .ok_or(NativeOverworldLevelNameError::TooManyNames(count))?,
                    tiles,
                    raw_flags: 0,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { names })
    }

    /// Reconstructs the 93 original SMW names from its three-part dictionary encoding.
    ///
    /// The offset dictionary is partitioned exactly as Lunar Magic does: 31 prefix, 15 middle,
    /// and 13 suffix entries. High-bit bytes terminate a segment and are stored with that bit
    /// removed. Missing optional prefix/middle sentinels are preserved as spaces (`0x1f`).
    ///
    /// # Errors
    ///
    /// Rejects malformed table sizes, impossible indices, out-of-blob offsets, and unterminated
    /// source segments.
    pub fn decode_vanilla(
        codes: &[u8],
        offsets: &[u8],
        text: &[u8],
    ) -> Result<Self, NativeOverworldLevelNameError> {
        if codes.len() != Self::VANILLA_NAMES * 2 {
            return Err(NativeOverworldLevelNameError::MisalignedTable(codes.len()));
        }
        if offsets.len() != 59 * 2 {
            return Err(NativeOverworldLevelNameError::MisalignedTable(
                offsets.len(),
            ));
        }
        let dictionary: Vec<usize> = offsets
            .chunks_exact(2)
            .map(|word| usize::from(u16::from_le_bytes([word[0], word[1]])))
            .collect();
        let mut names = Vec::with_capacity(Self::VANILLA_NAMES);
        for (slot, word) in codes.chunks_exact(2).enumerate() {
            let code = u16::from_le_bytes([word[0], word[1]]);
            let indexes = [
                ("prefix", usize::from((code >> 8) & 0x7f), 0usize, true),
                ("middle", usize::from((code >> 4) & 0x0f), 31usize, true),
                ("suffix", usize::from(code & 0x0f), 46usize, false),
            ];
            let mut tiles = [0x1f; OverworldLevelName::TILE_COUNT];
            let mut output = 0;
            for (part, index, base, optional) in indexes {
                let dictionary_index = base + index;
                let Some(&offset) = dictionary.get(dictionary_index) else {
                    return Err(NativeOverworldLevelNameError::InvalidVanillaCodeIndex {
                        name: slot,
                        part,
                        index,
                    });
                };
                let Some(&first) = text.get(offset) else {
                    return Err(NativeOverworldLevelNameError::InvalidSegmentOffset {
                        name: slot,
                        part,
                        offset,
                    });
                };
                if optional
                    && ((part == "prefix" && first >= 0x80) || (part == "middle" && first == 0x9f))
                {
                    continue;
                }
                let mut terminated = false;
                for &source in &text[offset..] {
                    if output < tiles.len() {
                        tiles[output] = source & 0x7f;
                        output += 1;
                    }
                    if source & 0x80 != 0 {
                        terminated = true;
                        break;
                    }
                    if output == tiles.len() {
                        terminated = true;
                        break;
                    }
                }
                if !terminated {
                    return Err(NativeOverworldLevelNameError::UnterminatedSegment {
                        name: slot,
                        part,
                    });
                }
            }
            names.push(OverworldLevelName {
                level: Self::level_for_slot(slot).ok_or(
                    NativeOverworldLevelNameError::TooManyNames(Self::VANILLA_NAMES),
                )?,
                tiles,
                raw_flags: 0,
            });
        }
        Ok(Self { names })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_table_round_trips_across_level_number_gap() {
        let names = (0..40)
            .map(|slot| OverworldLevelName {
                level: NativeOverworldLevelNameTable::level_for_slot(slot).unwrap(),
                tiles: [u8::try_from(slot).unwrap(); OverworldLevelName::TILE_COUNT],
                raw_flags: 0,
            })
            .collect();
        let table = NativeOverworldLevelNameTable { names };
        assert_eq!(
            NativeOverworldLevelNameTable::decode(&table.encode().unwrap()).unwrap(),
            table
        );
        assert_eq!(table.names[0x25].level, 0x101);
    }

    #[test]
    fn positional_and_flag_loss_are_rejected() {
        let mut table = NativeOverworldLevelNameTable {
            names: vec![OverworldLevelName {
                level: 7,
                tiles: [0; OverworldLevelName::TILE_COUNT],
                raw_flags: 0,
            }],
        };
        assert!(matches!(
            table.encode(),
            Err(NativeOverworldLevelNameError::NonCanonicalLevel { .. })
        ));
        table.names[0].level = 0;
        table.names[0].raw_flags = 1;
        assert!(matches!(
            table.encode(),
            Err(NativeOverworldLevelNameError::UnsupportedRawFlags { .. })
        ));
    }
}
