//! Native two-player overworld start-position block used by SMW and Lunar Magic.

use crate::{PlayerStart, Submap};

const FILE_MAGIC: &[u8; 8] = b"LMOWST1\0";
const FILE_VERSION: u16 = 1;
const FILE_HEADER_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldPlayerStarts {
    pub starts: [PlayerStart; 2],
    /// Four runtime-option bytes adjacent to the starts but not owned by this editor boundary.
    pub reserved: [u8; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldPlayerStartError {
    WrongLength(usize),
    WrongCount(usize),
    WrongPlayer { index: usize, actual: u8 },
    InvalidSubmap { player: usize, value: u8 },
    UnsupportedRawFlags { player: usize, flags: u8 },
    UnalignedCoordinates { player: usize, x: u16, y: u16 },
    WrongFileMagic,
    UnsupportedFileVersion(u16),
    WrongFilePayloadLength(u16),
}

impl std::fmt::Display for NativeOverworldPlayerStartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld player-start block: {self:?}"
        )
    }
}

impl std::error::Error for NativeOverworldPlayerStartError {}

impl NativeOverworldPlayerStarts {
    pub const ENCODED_LEN: usize = 22;
    pub const FILE_LEN: usize = FILE_HEADER_LEN + Self::ENCODED_LEN;
    pub const VANILLA_SUBMAP: Submap = Submap::YoshiIsland;
    pub const VANILLA_X: u16 = 0x68;
    pub const VANILLA_Y: u16 = 0x78;

    /// Decodes the exact 22-byte runtime-options block, preserving its four unrelated bytes.
    ///
    /// # Errors
    ///
    /// Rejects the wrong block length or submap values outside the seven native maps.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeOverworldPlayerStartError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(NativeOverworldPlayerStartError::WrongLength(bytes.len()));
        }
        let start = |player: usize, submap_offset: usize, coordinate_offset: usize| {
            let submap = Submap::decode(bytes[submap_offset]).ok_or(
                NativeOverworldPlayerStartError::InvalidSubmap {
                    player,
                    value: bytes[submap_offset],
                },
            )?;
            Ok(PlayerStart {
                player: u8::try_from(player)
                    .map_err(|_| NativeOverworldPlayerStartError::WrongCount(player))?,
                x: read_u16(bytes, coordinate_offset),
                y: read_u16(bytes, coordinate_offset + 2),
                submap,
                raw_flags: 0,
            })
        };
        Ok(Self {
            starts: [start(0, 0, 6)?, start(1, 1, 10)?],
            reserved: bytes[2..6]
                .try_into()
                .map_err(|_| NativeOverworldPlayerStartError::WrongLength(bytes.len()))?,
        })
    }

    /// Builds the native two-player boundary from portable records and retained reserved bytes.
    ///
    /// # Errors
    ///
    /// Requires exactly players zero and one in order, no portable-only flags, and coordinates
    /// centered on an 8-pixel point inside a 16x16 overworld tile.
    pub fn from_portable(
        starts: &[PlayerStart],
        reserved: [u8; 4],
    ) -> Result<Self, NativeOverworldPlayerStartError> {
        if starts.len() != 2 {
            return Err(NativeOverworldPlayerStartError::WrongCount(starts.len()));
        }
        for (index, start) in starts.iter().enumerate() {
            let expected = u8::try_from(index)
                .map_err(|_| NativeOverworldPlayerStartError::WrongCount(starts.len()))?;
            if start.player != expected {
                return Err(NativeOverworldPlayerStartError::WrongPlayer {
                    index,
                    actual: start.player,
                });
            }
            if start.raw_flags != 0 {
                return Err(NativeOverworldPlayerStartError::UnsupportedRawFlags {
                    player: index,
                    flags: start.raw_flags,
                });
            }
            if start.x & 0x0f != 8 || start.y & 0x0f != 8 {
                return Err(NativeOverworldPlayerStartError::UnalignedCoordinates {
                    player: index,
                    x: start.x,
                    y: start.y,
                });
            }
        }
        Ok(Self {
            starts: [starts[0], starts[1]],
            reserved,
        })
    }

    #[must_use]
    pub fn is_vanilla(&self) -> bool {
        self.starts.iter().all(|start| {
            start.submap == Self::VANILLA_SUBMAP
                && start.x == Self::VANILLA_X
                && start.y == Self::VANILLA_Y
        })
    }

    /// Encodes the exact runtime block and derives its redundant tile-coordinate words.
    ///
    /// # Errors
    ///
    /// Rejects noncanonical player keys, raw flags, or unaligned pixel coordinates.
    pub fn encode(&self) -> Result<[u8; Self::ENCODED_LEN], NativeOverworldPlayerStartError> {
        Self::from_portable(&self.starts, self.reserved)?;
        let mut bytes = [0; Self::ENCODED_LEN];
        bytes[0] = self.starts[0].submap.encoded();
        bytes[1] = self.starts[1].submap.encoded();
        bytes[2..6].copy_from_slice(&self.reserved);
        write_u16(&mut bytes, 6, self.starts[0].x);
        write_u16(&mut bytes, 8, self.starts[0].y);
        write_u16(&mut bytes, 10, self.starts[1].x);
        write_u16(&mut bytes, 12, self.starts[1].y);
        write_u16(&mut bytes, 14, self.starts[0].x >> 4);
        write_u16(&mut bytes, 16, self.starts[0].y >> 4);
        write_u16(&mut bytes, 18, self.starts[1].x >> 4);
        write_u16(&mut bytes, 20, self.starts[1].y >> 4);
        Ok(bytes)
    }

    /// Encodes one exact, bounded `LMOWST1` native player-start file.
    ///
    /// # Errors
    ///
    /// Rejects any model that cannot be represented by the native runtime block.
    pub fn encode_file(&self) -> Result<Vec<u8>, NativeOverworldPlayerStartError> {
        let mut bytes = Vec::with_capacity(Self::FILE_LEN);
        bytes.extend_from_slice(FILE_MAGIC);
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(
            &u16::try_from(Self::ENCODED_LEN)
                .map_err(|_| NativeOverworldPlayerStartError::WrongLength(Self::ENCODED_LEN))?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&self.encode()?);
        Ok(bytes)
    }

    /// Decodes one exact, bounded `LMOWST1` native player-start file.
    ///
    /// # Errors
    ///
    /// Rejects incorrect framing, versions, payload lengths, or native values.
    pub fn decode_file(bytes: &[u8]) -> Result<Self, NativeOverworldPlayerStartError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(NativeOverworldPlayerStartError::WrongLength(bytes.len()));
        }
        if &bytes[..8] != FILE_MAGIC {
            return Err(NativeOverworldPlayerStartError::WrongFileMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != FILE_VERSION {
            return Err(NativeOverworldPlayerStartError::UnsupportedFileVersion(
                version,
            ));
        }
        let payload_len = u16::from_le_bytes([bytes[10], bytes[11]]);
        if usize::from(payload_len) != Self::ENCODED_LEN {
            return Err(NativeOverworldPlayerStartError::WrongFilePayloadLength(
                payload_len,
            ));
        }
        Self::decode(&bytes[FILE_HEADER_LEN..])
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_block_round_trips_and_preserves_adjacent_options() {
        let bytes = [
            1, 1, 2, 0, 2, 0, 0x68, 0, 0x78, 0, 0x68, 0, 0x78, 0, 6, 0, 7, 0, 6, 0, 7, 0,
        ];
        let starts = NativeOverworldPlayerStarts::decode(&bytes).unwrap();
        assert!(starts.is_vanilla());
        assert_eq!(starts.reserved, [2, 0, 2, 0]);
        assert_eq!(starts.encode().unwrap(), bytes);
    }

    #[test]
    fn noncanonical_portable_values_are_rejected() {
        let mut value = NativeOverworldPlayerStarts::decode(&[
            1, 1, 2, 0, 2, 0, 0x68, 0, 0x78, 0, 0x68, 0, 0x78, 0, 6, 0, 7, 0, 6, 0, 7, 0,
        ])
        .unwrap();
        value.starts[1].raw_flags = 1;
        assert!(matches!(
            value.encode(),
            Err(NativeOverworldPlayerStartError::UnsupportedRawFlags { .. })
        ));
        value.starts[1].raw_flags = 0;
        value.starts[1].x = 9;
        assert!(matches!(
            value.encode(),
            Err(NativeOverworldPlayerStartError::UnalignedCoordinates { .. })
        ));
    }

    #[test]
    fn exact_file_round_trips_and_rejects_every_framing_dimension() {
        let starts = NativeOverworldPlayerStarts::decode(&[
            1, 1, 2, 0, 2, 0, 0x68, 0, 0x78, 0, 0x68, 0, 0x78, 0, 6, 0, 7, 0, 6, 0, 7, 0,
        ])
        .unwrap();
        let encoded = starts.encode_file().unwrap();
        assert_eq!(encoded.len(), NativeOverworldPlayerStarts::FILE_LEN);
        assert_eq!(
            NativeOverworldPlayerStarts::decode_file(&encoded).unwrap(),
            starts
        );
        for offset in [0, 8, 10] {
            let mut malformed = encoded.clone();
            malformed[offset] ^= 1;
            assert!(NativeOverworldPlayerStarts::decode_file(&malformed).is_err());
        }
        assert!(NativeOverworldPlayerStarts::decode_file(&encoded[..encoded.len() - 1]).is_err());
    }
}
