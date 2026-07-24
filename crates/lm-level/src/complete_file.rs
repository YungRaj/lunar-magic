//! Complete, revision-independent semantic level interchange.

use crate::{
    BinaryError, ByteCursor, ExpandedLevelHeader, Layer3Error, Layer3File, LayerData,
    LegacyLevelHeader, Level, LevelHeader, Map16Tile, ObjectRecord, ObjectStreamError, ScreenExit,
    SecondaryExit, SpriteRecord, SpriteStream,
};
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"LMLEVEL2";
const VERSION: u16 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteLevelFile(pub Level);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelCollection {
    Layer1Objects,
    Layer1Tiles,
    Layer2Objects,
    Layer2Tiles,
    Sprites,
    Entrances,
    ScreenExits,
    SecondaryExits,
    Map16Overrides,
    UnknownExtensions,
}

#[derive(Debug)]
pub enum CompleteLevelFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    FileTooLarge(usize),
    TooManyRecords {
        collection: LevelCollection,
        count: usize,
    },
    RecordTooLarge {
        collection: LevelCollection,
        len: usize,
    },
    InvalidExpandedFlag(u8),
    InvalidEntranceKind {
        record: usize,
        value: u8,
    },
    DuplicateMap16Override(u32),
    InvalidObject(ObjectStreamError),
    InvalidLayer3Flag(u8),
    Layer3(Layer3Error),
    Binary(BinaryError),
    TrailingBytes(usize),
    Overflow,
}

impl std::fmt::Display for CompleteLevelFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "complete level file error: {self:?}")
    }
}

impl std::error::Error for CompleteLevelFileError {}

impl From<BinaryError> for CompleteLevelFileError {
    fn from(value: BinaryError) -> Self {
        Self::Binary(value)
    }
}

impl From<ObjectStreamError> for CompleteLevelFileError {
    fn from(value: ObjectStreamError) -> Self {
        Self::InvalidObject(value)
    }
}

impl From<Layer3Error> for CompleteLevelFileError {
    fn from(value: Layer3Error) -> Self {
        Self::Layer3(value)
    }
}

impl CompleteLevelFile {
    pub const MAX_FILE_LEN: usize = 0x100_0000;
    pub const MAX_RECORDS: usize = 0x1_0000;
    pub const MAX_RECORD_LEN: usize = 0x1_0000;

    /// Encodes all semantic [`Level`] domains into deterministic `LMLEVEL2` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteLevelFileError`] for excessive counts, record lengths, total size, or
    /// invalid object records.
    pub fn encode(&self) -> Result<Vec<u8>, CompleteLevelFileError> {
        let level = &self.0;
        let encoded_layer3 = level
            .layer3
            .as_ref()
            .map(|layer3| Layer3File(layer3.clone()).encode())
            .transpose()?;
        validate_map16_override_keys(&level.map16_overrides)?;
        let encoded_len = encoded_file_len(level, encoded_layer3.as_deref())?;
        let mut output = Vec::with_capacity(encoded_len);
        output.extend_from_slice(MAGIC);
        push_u16(&mut output, VERSION);
        push_u16(&mut output, level.number);
        output.extend_from_slice(&level.header.legacy.encoded());
        match level.header.expanded {
            Some(header) => {
                output.push(1);
                output.extend_from_slice(&header.encode());
            }
            None => output.push(0),
        }
        encode_layer(
            &mut output,
            &level.layer1,
            LevelCollection::Layer1Objects,
            LevelCollection::Layer1Tiles,
        )?;
        encode_layer(
            &mut output,
            &level.layer2,
            LevelCollection::Layer2Objects,
            LevelCollection::Layer2Tiles,
        )?;
        match encoded_layer3 {
            Some(encoded) => {
                output.push(1);
                push_u32(
                    &mut output,
                    u32::try_from(encoded.len()).map_err(|_| CompleteLevelFileError::Overflow)?,
                );
                output.extend_from_slice(&encoded);
            }
            None => output.push(0),
        }
        output.push(level.sprites.header);
        encode_blobs(
            &mut output,
            LevelCollection::Sprites,
            level
                .sprites
                .records
                .iter()
                .map(|record| record.encoded.as_slice()),
        )?;
        encode_entrances(&mut output, &level.entrances)?;
        encode_count(
            &mut output,
            LevelCollection::ScreenExits,
            level.screen_exits.len(),
        )?;
        for exit in &level.screen_exits {
            push_u32(&mut output, exit.encoded);
        }
        encode_count(
            &mut output,
            LevelCollection::SecondaryExits,
            level.secondary_exits.len(),
        )?;
        for exit in &level.secondary_exits {
            push_u16(&mut output, exit.destination_level);
            output.extend_from_slice(&[
                exit.position_and_method,
                exit.screen,
                exit.x,
                exit.y,
                exit.destination_flags,
                exit.x_and_overworld_flags,
                exit.additional_flags,
            ]);
        }
        encode_count(
            &mut output,
            LevelCollection::Map16Overrides,
            level.map16_overrides.len(),
        )?;
        for (index, tile) in &level.map16_overrides {
            push_u32(&mut output, *index);
            output.extend_from_slice(&tile.encode_graphics());
            push_u16(&mut output, tile.acts_like);
        }
        encode_blobs(
            &mut output,
            LevelCollection::UnknownExtensions,
            level.unknown_extensions.iter().map(Vec::as_slice),
        )?;
        debug_assert_eq!(output.len(), encoded_len);
        Ok(output)
    }

    /// Decodes one complete bounded `LMLEVEL2` file with no trailing bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteLevelFileError`] for framing, truncation, excessive lengths/counts,
    /// invalid enums/objects, overflow, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, CompleteLevelFileError> {
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(CompleteLevelFileError::FileTooLarge(bytes.len()));
        }
        if bytes.len() < MAGIC.len() {
            return Err(CompleteLevelFileError::Truncated);
        }
        let mut input = ByteCursor::new(bytes);
        if input.take(MAGIC.len())? != MAGIC {
            return Err(CompleteLevelFileError::WrongMagic);
        }
        let version = input.u16_le()?;
        if !matches!(version, 1 | VERSION) {
            return Err(CompleteLevelFileError::UnsupportedVersion(version));
        }
        let number = input.u16_le()?;
        let legacy = LegacyLevelHeader::decode(input.take(LegacyLevelHeader::ENCODED_LEN)?)
            .map_err(|_| CompleteLevelFileError::Truncated)?;
        let expanded = match input.u8()? {
            0 => None,
            1 => Some(
                ExpandedLevelHeader::decode(input.take(ExpandedLevelHeader::ENCODED_LEN)?)
                    .map_err(|_| CompleteLevelFileError::Truncated)?,
            ),
            value => return Err(CompleteLevelFileError::InvalidExpandedFlag(value)),
        };
        let layer1 = decode_layer(
            &mut input,
            LevelCollection::Layer1Objects,
            LevelCollection::Layer1Tiles,
        )?;
        let layer2 = decode_layer(
            &mut input,
            LevelCollection::Layer2Objects,
            LevelCollection::Layer2Tiles,
        )?;
        let layer3 = decode_optional_layer3(&mut input, version)?;
        let sprite_header = input.u8()?;
        let sprites = SpriteStream {
            header: sprite_header,
            records: decode_blobs(&mut input, LevelCollection::Sprites)?
                .into_iter()
                .map(|encoded| SpriteRecord { encoded })
                .collect(),
        };
        let entrances = decode_entrances(&mut input)?;
        let screen_exits = (0..decode_count(&mut input, LevelCollection::ScreenExits)?)
            .map(|_| {
                input
                    .u32_le()
                    .map(|encoded| ScreenExit { encoded })
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>, CompleteLevelFileError>>()?;
        let secondary_exits = (0..decode_count(&mut input, LevelCollection::SecondaryExits)?)
            .map(|_| {
                Ok(SecondaryExit {
                    destination_level: input.u16_le()?,
                    position_and_method: input.u8()?,
                    screen: input.u8()?,
                    x: input.u8()?,
                    y: input.u8()?,
                    destination_flags: input.u8()?,
                    x_and_overworld_flags: input.u8()?,
                    additional_flags: input.u8()?,
                })
            })
            .collect::<Result<Vec<_>, CompleteLevelFileError>>()?;
        let map16_overrides = (0..decode_count(&mut input, LevelCollection::Map16Overrides)?)
            .map(|_| {
                let index = input.u32_le()?;
                let tile =
                    Map16Tile::decode(input.take(Map16Tile::GRAPHICS_LEN)?, input.u16_le()?)?;
                Ok((index, tile))
            })
            .collect::<Result<Vec<_>, CompleteLevelFileError>>()?;
        validate_map16_override_keys(&map16_overrides)?;
        let unknown_extensions = decode_blobs(&mut input, LevelCollection::UnknownExtensions)?;
        if input.remaining() != 0 {
            return Err(CompleteLevelFileError::TrailingBytes(input.remaining()));
        }
        Ok(Self(Level {
            number,
            header: LevelHeader { legacy, expanded },
            layer1,
            layer2,
            layer3,
            sprites,
            entrances,
            screen_exits,
            secondary_exits,
            map16_overrides,
            unknown_extensions,
        }))
    }
}

fn validate_map16_override_keys(
    overrides: &[(u32, Map16Tile)],
) -> Result<(), CompleteLevelFileError> {
    let mut keys = BTreeSet::new();
    for (index, _) in overrides {
        if !keys.insert(*index) {
            return Err(CompleteLevelFileError::DuplicateMap16Override(*index));
        }
    }
    Ok(())
}

#[path = "complete_file_size.rs"]
mod size;

#[cfg(test)]
use size::checked_file_add;
use size::encoded_file_len;

#[path = "complete_file_records.rs"]
mod records;

use records::{
    decode_blobs, decode_count, decode_entrances, decode_layer, decode_optional_layer3,
    encode_blobs, encode_count, encode_entrances, encode_layer, push_u16, push_u32,
};

#[cfg(test)]
#[path = "complete_file_tests.rs"]
mod tests;
