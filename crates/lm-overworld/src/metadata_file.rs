//! Portable lossless serialization for overworld names, starts, and submap settings.

use crate::{
    MetadataError, OverworldLevelName, OverworldMetadata, PlayerStart, Submap, SubmapSettings,
};

const MAGIC: &[u8; 8] = b"LMOWMETA";
const VERSION: u16 = 1;
const HEADER_LEN: usize = 16;
const NAME_LEN: usize = 22;
const START_LEN: usize = 7;
const SETTINGS_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    WrongLength { expected: usize, actual: usize },
    InvalidStartSubmap { record: usize, value: u8 },
    InvalidSettingsSubmap { record: usize, value: u8 },
    TooManyLevelNames(usize),
    TooManyPlayerStarts(usize),
    TooManySubmapSettings(usize),
    Overflow,
    Metadata(MetadataError),
}

impl std::fmt::Display for MetadataFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "overworld metadata file error: {self:?}")
    }
}

impl std::error::Error for MetadataFileError {}

impl From<MetadataError> for MetadataFileError {
    fn from(value: MetadataError) -> Self {
        Self::Metadata(value)
    }
}

impl OverworldMetadata {
    pub const MAX_FILE_LEN: usize = HEADER_LEN
        + Self::MAX_LEVEL_NAMES * NAME_LEN
        + Self::MAX_PLAYER_STARTS * START_LEN
        + Self::MAX_SUBMAP_SETTINGS * SETTINGS_LEN;

    /// Encodes exact metadata records as deterministic `LMOWMETA` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataFileError`] for invalid metadata or unrepresentable counts/sizes.
    pub fn encode_file(&self) -> Result<Vec<u8>, MetadataFileError> {
        self.validate()?;
        let names = count_u16(self.level_names.len(), MetadataFileError::TooManyLevelNames)?;
        let starts = count_u16(
            self.player_starts.len(),
            MetadataFileError::TooManyPlayerStarts,
        )?;
        let settings = count_u16(
            self.submap_settings.len(),
            MetadataFileError::TooManySubmapSettings,
        )?;
        let mut bytes = Vec::with_capacity(encoded_len(
            self.level_names.len(),
            self.player_starts.len(),
            self.submap_settings.len(),
        )?);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&names.to_le_bytes());
        bytes.extend_from_slice(&starts.to_le_bytes());
        bytes.extend_from_slice(&settings.to_le_bytes());
        for name in &self.level_names {
            bytes.extend_from_slice(&name.level.to_le_bytes());
            bytes.extend_from_slice(&name.tiles);
            bytes.push(name.raw_flags);
        }
        for start in &self.player_starts {
            bytes.push(start.player);
            bytes.extend_from_slice(&start.x.to_le_bytes());
            bytes.extend_from_slice(&start.y.to_le_bytes());
            bytes.push(start.submap.encoded());
            bytes.push(start.raw_flags);
        }
        for settings in &self.submap_settings {
            bytes.push(settings.submap.encoded());
            bytes.push(settings.music);
            bytes.push(settings.palette);
            bytes.push(settings.layer1_scroll);
            bytes.push(settings.layer2_scroll);
            bytes.extend_from_slice(&settings.raw_flags.to_le_bytes());
            bytes.extend_from_slice(&settings.unknown);
        }
        Ok(bytes)
    }

    /// Decodes exactly one complete bounded `LMOWMETA` file.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataFileError`] for framing, length, enum, count, or uniqueness failures.
    pub fn decode_file(bytes: &[u8]) -> Result<Self, MetadataFileError> {
        let header = bytes
            .get(..HEADER_LEN)
            .ok_or(MetadataFileError::Truncated)?;
        if &header[..8] != MAGIC {
            return Err(MetadataFileError::WrongMagic);
        }
        let version = read_u16(header, 8);
        if version != VERSION {
            return Err(MetadataFileError::UnsupportedVersion(version));
        }
        let names = usize::from(read_u16(header, 10));
        let starts = usize::from(read_u16(header, 12));
        let settings = usize::from(read_u16(header, 14));
        check_limits(names, starts, settings)?;
        let expected = encoded_len(names, starts, settings)?;
        if bytes.len() != expected {
            return Err(MetadataFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut offset = HEADER_LEN;
        let mut level_names = Vec::with_capacity(names);
        for _ in 0..names {
            let record = &bytes[offset..offset + NAME_LEN];
            offset += NAME_LEN;
            let mut tiles = [0; OverworldLevelName::TILE_COUNT];
            tiles.copy_from_slice(&record[2..21]);
            level_names.push(OverworldLevelName {
                level: read_u16(record, 0),
                tiles,
                raw_flags: record[21],
            });
        }
        let mut player_starts = Vec::with_capacity(starts);
        for record_index in 0..starts {
            let record = &bytes[offset..offset + START_LEN];
            offset += START_LEN;
            let submap =
                Submap::decode(record[5]).ok_or(MetadataFileError::InvalidStartSubmap {
                    record: record_index,
                    value: record[5],
                })?;
            player_starts.push(PlayerStart {
                player: record[0],
                x: read_u16(record, 1),
                y: read_u16(record, 3),
                submap,
                raw_flags: record[6],
            });
        }
        let mut submap_settings = Vec::with_capacity(settings);
        for record_index in 0..settings {
            let record = &bytes[offset..offset + SETTINGS_LEN];
            offset += SETTINGS_LEN;
            let submap =
                Submap::decode(record[0]).ok_or(MetadataFileError::InvalidSettingsSubmap {
                    record: record_index,
                    value: record[0],
                })?;
            let mut unknown = [0; 5];
            unknown.copy_from_slice(&record[7..12]);
            submap_settings.push(SubmapSettings {
                submap,
                music: record[1],
                palette: record[2],
                layer1_scroll: record[3],
                layer2_scroll: record[4],
                raw_flags: read_u16(record, 5),
                unknown,
            });
        }
        let metadata = Self {
            level_names,
            player_starts,
            submap_settings,
        };
        metadata.validate()?;
        Ok(metadata)
    }
}

fn check_limits(names: usize, starts: usize, settings: usize) -> Result<(), MetadataFileError> {
    if names > OverworldMetadata::MAX_LEVEL_NAMES {
        return Err(MetadataFileError::TooManyLevelNames(names));
    }
    if starts > OverworldMetadata::MAX_PLAYER_STARTS {
        return Err(MetadataFileError::TooManyPlayerStarts(starts));
    }
    if settings > OverworldMetadata::MAX_SUBMAP_SETTINGS {
        return Err(MetadataFileError::TooManySubmapSettings(settings));
    }
    Ok(())
}

fn encoded_len(names: usize, starts: usize, settings: usize) -> Result<usize, MetadataFileError> {
    HEADER_LEN
        .checked_add(
            names
                .checked_mul(NAME_LEN)
                .ok_or(MetadataFileError::Overflow)?,
        )
        .and_then(|len| len.checked_add(starts.checked_mul(START_LEN)?))
        .and_then(|len| len.checked_add(settings.checked_mul(SETTINGS_LEN)?))
        .ok_or(MetadataFileError::Overflow)
}

fn count_u16(
    count: usize,
    error: fn(usize) -> MetadataFileError,
) -> Result<u16, MetadataFileError> {
    u16::try_from(count).map_err(|_| error(count))
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> OverworldMetadata {
        OverworldMetadata {
            level_names: vec![OverworldLevelName {
                level: 0x105,
                tiles: *b"YOSHI'S ISLAND 1   ",
                raw_flags: 0x80,
            }],
            player_starts: vec![PlayerStart {
                player: 1,
                x: 0x123,
                y: 0x456,
                submap: Submap::YoshiIsland,
                raw_flags: 0x40,
            }],
            submap_settings: vec![SubmapSettings {
                submap: Submap::YoshiIsland,
                music: 2,
                palette: 3,
                layer1_scroll: 4,
                layer2_scroll: 5,
                raw_flags: 0x8123,
                unknown: [6, 7, 8, 9, 10],
            }],
        }
    }

    #[test]
    fn exact_metadata_and_unknown_fields_round_trip() {
        let expected = metadata();
        let bytes = expected.encode_file().unwrap();
        assert_eq!(OverworldMetadata::decode_file(&bytes).unwrap(), expected);
        assert_eq!(
            OverworldMetadata::decode_file(&bytes)
                .unwrap()
                .encode_file()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn every_truncation_and_invalid_submap_is_rejected() {
        let bytes = metadata().encode_file().unwrap();
        for end in 0..bytes.len() {
            assert!(OverworldMetadata::decode_file(&bytes[..end]).is_err());
        }
        let mut invalid = bytes;
        invalid[HEADER_LEN + NAME_LEN + 5] = 7;
        assert!(matches!(
            OverworldMetadata::decode_file(&invalid),
            Err(MetadataFileError::InvalidStartSubmap { .. })
        ));
    }
}
