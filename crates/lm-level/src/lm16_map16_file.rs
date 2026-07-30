use crate::{Map16Tile, Subtile};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Lm16Map16SectionKind {
    CombinedTiles = 0,
    ActsLike = 1,
    ForegroundTiles = 2,
    BackgroundTiles = 3,
    ExtendedTiles = 4,
    AuxiliaryTiles = 5,
    SelectionState = 6,
    EditorState = 7,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lm16Map16Section {
    pub offset: usize,
    pub len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lm16Map16File {
    pub format_version: u32,
    pub lunar_magic_version: u32,
    pub flags: u32,
    pub attribution: [u8; Self::ATTRIBUTION_LEN],
    pub sections: [Lm16Map16Section; Self::SECTION_COUNT],
    bytes: Vec<u8>,
}

impl Lm16Map16File {
    pub const MAGIC: [u8; 4] = *b"LM16";
    pub const HEADER_LEN: usize = 0x40;
    pub const ATTRIBUTION_LEN: usize = 0x30;
    pub const DIRECTORY_OFFSET: usize = 0x70;
    pub const DIRECTORY_LEN: usize = 0x40;
    pub const DATA_OFFSET: usize = 0xb0;
    pub const SECTION_COUNT: usize = 8;
    pub const REQUIRED_CAPABILITY: u32 = 2;
    pub const MAX_FILE_LEN: usize = 2 * 1024 * 1024;
    pub const TILE_COUNT: usize = 0x1_0000;
    pub const FOREGROUND_TILE_COUNT: usize = 0x8000;
    pub const BACKGROUND_TILE_COUNT: usize = 0x8000;
    pub const TILE_BYTES: usize = 8;
    pub const COMBINED_TILES_LEN: usize = Self::TILE_COUNT * Self::TILE_BYTES;
    pub const FOREGROUND_TILES_LEN: usize = Self::FOREGROUND_TILE_COUNT * Self::TILE_BYTES;
    pub const BACKGROUND_TILES_LEN: usize = Self::BACKGROUND_TILE_COUNT * Self::TILE_BYTES;
    pub const ACTS_LIKE_LEN: usize = Self::FOREGROUND_TILE_COUNT * 2;

    /// Decodes Lunar Magic's structured complete `.map16` container losslessly.
    ///
    /// Directory entries may intentionally alias: the foreground and background sections point
    /// into the two halves of the combined-tile section. The decoder therefore validates bounded
    /// ranges without incorrectly rejecting overlap.
    ///
    /// # Errors
    ///
    /// Returns [`Lm16Map16FileError`] for malformed framing, unsupported capabilities, invalid
    /// directory entries, trailing bytes, or excessive input.
    pub fn decode(bytes: &[u8]) -> Result<Self, Lm16Map16FileError> {
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(Lm16Map16FileError::TooLarge(bytes.len()));
        }
        let prefix = bytes
            .get(..Self::DATA_OFFSET)
            .ok_or(Lm16Map16FileError::Truncated)?;
        if prefix[..4] != Self::MAGIC {
            return Err(Lm16Map16FileError::WrongMagic);
        }
        let format_version = read_u32(prefix, 4)?;
        if format_version & 0xff00 != 0x0100 || format_version >> 16 != 1 {
            return Err(Lm16Map16FileError::UnsupportedFormat(format_version));
        }
        let lunar_magic_version = read_u32(prefix, 8)?;
        let directory_offset =
            usize::try_from(read_u32(prefix, 0x10)?).map_err(|_| Lm16Map16FileError::Overflow)?;
        let directory_len =
            usize::try_from(read_u32(prefix, 0x14)?).map_err(|_| Lm16Map16FileError::Overflow)?;
        if directory_offset != Self::DIRECTORY_OFFSET || directory_len != Self::DIRECTORY_LEN {
            return Err(Lm16Map16FileError::DirectoryShape {
                offset: directory_offset,
                len: directory_len,
            });
        }
        let flags = read_u32(prefix, 0x28)?;
        if flags & Self::REQUIRED_CAPABILITY == 0 {
            return Err(Lm16Map16FileError::MissingCapability(flags));
        }
        let attribution = prefix[Self::HEADER_LEN..Self::DIRECTORY_OFFSET]
            .try_into()
            .map_err(|_| Lm16Map16FileError::Truncated)?;

        let mut maximum_end = Self::DATA_OFFSET;
        let mut sections = [Lm16Map16Section::default(); Self::SECTION_COUNT];
        for (index, section) in sections.iter_mut().enumerate() {
            let entry = Self::DIRECTORY_OFFSET + index * 8;
            let offset = usize::try_from(read_u32(prefix, entry)?)
                .map_err(|_| Lm16Map16FileError::Overflow)?;
            let len = usize::try_from(read_u32(prefix, entry + 4)?)
                .map_err(|_| Lm16Map16FileError::Overflow)?;
            if len == 0 {
                if offset != 0 {
                    return Err(Lm16Map16FileError::EmptySectionOffset { index, offset });
                }
                continue;
            }
            if offset < Self::DATA_OFFSET {
                return Err(Lm16Map16FileError::SectionOverlapsPrefix { index, offset });
            }
            let end = offset
                .checked_add(len)
                .ok_or(Lm16Map16FileError::Overflow)?;
            if end > bytes.len() {
                return Err(Lm16Map16FileError::SectionOutOfBounds { index, offset, len });
            }
            maximum_end = maximum_end.max(end);
            *section = Lm16Map16Section { offset, len };
        }
        if maximum_end != bytes.len() {
            return Err(Lm16Map16FileError::TrailingBytes {
                expected: maximum_end,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            format_version,
            lunar_magic_version,
            flags,
            attribution,
            sections,
            bytes: bytes.to_vec(),
        })
    }

    #[must_use]
    pub fn section(&self, kind: Lm16Map16SectionKind) -> &[u8] {
        let section = self.sections[kind as usize];
        &self.bytes[section.offset..section.offset + section.len]
    }

    /// Returns one foreground or background definition from the complete combined tile bank.
    ///
    /// Lunar Magic stores four little-endian subtile words per definition. Background definitions
    /// do not have gameplay behavior; their returned `acts_like` value is zero.
    ///
    /// Returns `None` when `tile` is outside the 16-bit namespace or the file only contains a
    /// shorter, partial combined section.
    #[must_use]
    pub fn tile(&self, tile: usize) -> Option<Map16Tile> {
        let words = self.tile_words(tile)?;
        let acts_like = if tile < Self::FOREGROUND_TILE_COUNT {
            self.acts_like(tile)?
        } else {
            0
        };
        Some(Map16Tile {
            top_left: Subtile(words[0]),
            top_right: Subtile(words[1]),
            bottom_left: Subtile(words[2]),
            bottom_right: Subtile(words[3]),
            acts_like,
        })
    }

    /// Returns the four raw little-endian subtile words for one definition.
    #[must_use]
    pub fn tile_words(&self, tile: usize) -> Option<[u16; 4]> {
        if tile >= Self::TILE_COUNT {
            return None;
        }
        let offset = tile.checked_mul(Self::TILE_BYTES)?;
        let bytes = self
            .section(Lm16Map16SectionKind::CombinedTiles)
            .get(offset..offset + Self::TILE_BYTES)?;
        Some([
            u16::from_le_bytes(bytes[0..2].try_into().ok()?),
            u16::from_le_bytes(bytes[2..4].try_into().ok()?),
            u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            u16::from_le_bytes(bytes[6..8].try_into().ok()?),
        ])
    }

    /// Returns one foreground gameplay-behavior value.
    ///
    /// The complete file contains 0x8000 little-endian values. Tile numbers 0x8000 and above are
    /// background definitions and therefore have no Acts-Like entry.
    #[must_use]
    pub fn acts_like(&self, tile: usize) -> Option<u16> {
        if tile >= Self::FOREGROUND_TILE_COUNT {
            return None;
        }
        let offset = tile.checked_mul(2)?;
        let bytes = self
            .section(Lm16Map16SectionKind::ActsLike)
            .get(offset..offset + 2)?;
        Some(u16::from_le_bytes(bytes.try_into().ok()?))
    }

    /// Replaces one definition in-place while preserving every unrelated container byte.
    ///
    /// Foreground writes also update Acts-Like when that section contains the requested value.
    /// Background writes ignore `tile.acts_like`, matching Lunar Magic's separate foreground-only
    /// behavior table.
    ///
    /// # Errors
    ///
    /// Rejects indices outside the 16-bit namespace and partial files that do not contain the
    /// requested definition or foreground behavior value.
    pub fn set_tile(
        &mut self,
        tile_number: usize,
        tile: Map16Tile,
    ) -> Result<(), Lm16Map16FileError> {
        if tile_number >= Self::TILE_COUNT {
            return Err(Lm16Map16FileError::TileOutOfRange(tile_number));
        }
        let tile_offset = tile_number
            .checked_mul(Self::TILE_BYTES)
            .ok_or(Lm16Map16FileError::Overflow)?;
        let combined = self.sections[Lm16Map16SectionKind::CombinedTiles as usize];
        if tile_offset + Self::TILE_BYTES > combined.len {
            return Err(Lm16Map16FileError::TileNotPresent(tile_number));
        }
        let acts_offset = if tile_number < Self::FOREGROUND_TILE_COUNT {
            let offset = tile_number
                .checked_mul(2)
                .ok_or(Lm16Map16FileError::Overflow)?;
            let acts = self.sections[Lm16Map16SectionKind::ActsLike as usize];
            if offset + 2 > acts.len {
                return Err(Lm16Map16FileError::ActsLikeNotPresent(tile_number));
            }
            Some(acts.offset + offset)
        } else {
            None
        };
        let offset = combined.offset + tile_offset;
        for (target, word) in self.bytes[offset..offset + Self::TILE_BYTES]
            .chunks_exact_mut(2)
            .zip([
                tile.top_left.0,
                tile.top_right.0,
                tile.bottom_left.0,
                tile.bottom_right.0,
            ])
        {
            target.copy_from_slice(&word.to_le_bytes());
        }
        if let Some(offset) = acts_offset {
            self.bytes[offset..offset + 2].copy_from_slice(&tile.acts_like.to_le_bytes());
        }
        Ok(())
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Lm16Map16FileError {
    TooLarge(usize),
    Truncated,
    WrongMagic,
    UnsupportedFormat(u32),
    DirectoryShape {
        offset: usize,
        len: usize,
    },
    MissingCapability(u32),
    EmptySectionOffset {
        index: usize,
        offset: usize,
    },
    SectionOverlapsPrefix {
        index: usize,
        offset: usize,
    },
    SectionOutOfBounds {
        index: usize,
        offset: usize,
        len: usize,
    },
    TrailingBytes {
        expected: usize,
        actual: usize,
    },
    Overflow,
    TileOutOfRange(usize),
    TileNotPresent(usize),
    ActsLikeNotPresent(usize),
}

impl fmt::Display for Lm16Map16FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic LM16 Map16 file: {self:?}")
    }
}

impl std::error::Error for Lm16Map16FileError {}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Lm16Map16FileError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(Lm16Map16FileError::Truncated)?
            .try_into()
            .map_err(|_| Lm16Map16FileError::Truncated)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical() -> Vec<u8> {
        let mut bytes = vec![0; 0xe0];
        bytes[..4].copy_from_slice(&Lm16Map16File::MAGIC);
        bytes[4..8].copy_from_slice(&0x0001_0100_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0x0001_0363_u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_OFFSET)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x14..0x18].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_LEN)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x28..0x2c].copy_from_slice(&2_u32.to_le_bytes());
        bytes[0x40..0x46].copy_from_slice(b"Lunar ");
        for (index, (offset, len)) in [
            (0xb0_u32, 0x20_u32),
            (0xd0, 0x10),
            (0xb0, 0x10),
            (0xc0, 0x10),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ]
        .into_iter()
        .enumerate()
        {
            let entry = Lm16Map16File::DIRECTORY_OFFSET + index * 8;
            bytes[entry..entry + 4].copy_from_slice(&offset.to_le_bytes());
            bytes[entry + 4..entry + 8].copy_from_slice(&len.to_le_bytes());
        }
        bytes
    }

    fn complete_tile_banks() -> Vec<u8> {
        let combined_offset = Lm16Map16File::DATA_OFFSET;
        let acts_offset = combined_offset + Lm16Map16File::COMBINED_TILES_LEN;
        let mut bytes = vec![0; acts_offset + Lm16Map16File::ACTS_LIKE_LEN];
        bytes[..4].copy_from_slice(&Lm16Map16File::MAGIC);
        bytes[4..8].copy_from_slice(&0x0001_0100_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0x0001_0363_u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_OFFSET)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x14..0x18].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_LEN)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x28..0x2c].copy_from_slice(&2_u32.to_le_bytes());
        for (index, (offset, len)) in [
            (combined_offset, Lm16Map16File::COMBINED_TILES_LEN),
            (acts_offset, Lm16Map16File::ACTS_LIKE_LEN),
            (combined_offset, Lm16Map16File::FOREGROUND_TILES_LEN),
            (
                combined_offset + Lm16Map16File::FOREGROUND_TILES_LEN,
                Lm16Map16File::BACKGROUND_TILES_LEN,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let entry = Lm16Map16File::DIRECTORY_OFFSET + index * 8;
            bytes[entry..entry + 4].copy_from_slice(&u32::try_from(offset).unwrap().to_le_bytes());
            bytes[entry + 4..entry + 8].copy_from_slice(&u32::try_from(len).unwrap().to_le_bytes());
        }
        bytes
    }

    #[test]
    fn preserves_aliasing_and_round_trips_exactly() {
        let bytes = canonical();
        let file = Lm16Map16File::decode(&bytes).unwrap();
        assert_eq!(file.lunar_magic_version, 0x0001_0363);
        assert_eq!(
            file.section(Lm16Map16SectionKind::CombinedTiles).len(),
            0x20
        );
        assert_eq!(
            file.section(Lm16Map16SectionKind::ForegroundTiles),
            &file.section(Lm16Map16SectionKind::CombinedTiles)[..0x10]
        );
        assert_eq!(file.encode(), bytes);
    }

    #[test]
    fn rejects_bad_framing_ranges_and_trailing_bytes() {
        let mut bad_magic = canonical();
        bad_magic[0] = 0;
        assert_eq!(
            Lm16Map16File::decode(&bad_magic),
            Err(Lm16Map16FileError::WrongMagic)
        );

        let mut out_of_bounds = canonical();
        out_of_bounds[0x74..0x78].copy_from_slice(&0x100_u32.to_le_bytes());
        assert!(matches!(
            Lm16Map16File::decode(&out_of_bounds),
            Err(Lm16Map16FileError::SectionOutOfBounds { index: 0, .. })
        ));

        let mut trailing = canonical();
        trailing.push(0);
        assert_eq!(
            Lm16Map16File::decode(&trailing),
            Err(Lm16Map16FileError::TrailingBytes {
                expected: 0xe0,
                actual: 0xe1,
            })
        );
    }

    #[test]
    fn reads_and_replaces_foreground_definition_and_behavior() {
        let mut file = Lm16Map16File::decode(&complete_tile_banks()).unwrap();
        let tile = Map16Tile {
            top_left: Subtile(0x1004),
            top_right: Subtile(0x4321),
            bottom_left: Subtile(0xabcd),
            bottom_right: Subtile(0xffff),
            acts_like: 0x0130,
        };
        file.set_tile(0x2345, tile).unwrap();
        assert_eq!(file.tile(0x2345), Some(tile));
        assert_eq!(file.acts_like(0x2345), Some(0x0130));
        assert_eq!(
            file.tile_words(0x2345),
            Some([0x1004, 0x4321, 0xabcd, 0xffff])
        );
        assert_eq!(
            Lm16Map16File::decode(&file.encode()).unwrap().tile(0x2345),
            Some(tile)
        );
    }

    #[test]
    fn background_definition_has_no_acts_like_value() {
        let mut file = Lm16Map16File::decode(&complete_tile_banks()).unwrap();
        let tile = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0xbeef,
        };
        file.set_tile(0x8200, tile).unwrap();
        assert_eq!(file.tile_words(0x8200), Some([1, 2, 3, 4]));
        assert_eq!(file.acts_like(0x8200), None);
        assert_eq!(
            file.tile(0x8200),
            Some(Map16Tile {
                acts_like: 0,
                ..tile
            })
        );
    }

    #[test]
    fn partial_file_rejects_missing_behavior_before_mutating_definition() {
        let mut bytes = canonical();
        bytes[0x7c..0x80].copy_from_slice(&4_u32.to_le_bytes());
        bytes.truncate(0xd4);
        let mut file = Lm16Map16File::decode(&bytes).unwrap();
        let before = file.encode();
        let tile = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 5,
        };
        assert_eq!(
            file.set_tile(3, tile),
            Err(Lm16Map16FileError::ActsLikeNotPresent(3))
        );
        assert_eq!(file.encode(), before);
        assert_eq!(
            file.set_tile(Lm16Map16File::TILE_COUNT, tile),
            Err(Lm16Map16FileError::TileOutOfRange(
                Lm16Map16File::TILE_COUNT
            ))
        );
    }
}
