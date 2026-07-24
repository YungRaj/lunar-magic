use crate::{CompleteOverworldData, OverworldLayers};
use lm_graphics::{CompactExAnimation, ExAnimationError, Palette};
use lm_overworld::{
    EventRevealTable, EventTableError, FixedTableEncodingError, OverworldEndpoint, OverworldLayer,
    OverworldLayerEncodingError, OverworldMessage, OverworldSprite, OverworldSpriteError,
};
use std::fmt;

mod validation;

use validation::{
    read_u16, section_lengths, shape_values, to_u16, validate_data_shape, validate_modes,
    validate_section_len, validate_shape,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteOverworldShape {
    pub width: usize,
    pub height: usize,
    pub event_reveals: usize,
    pub endpoints: usize,
    pub messages: usize,
    pub sprites: usize,
    pub sprite_record_len: usize,
    pub palette_colors: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteOverworldFile {
    pub source_slot: u16,
    pub shape: CompleteOverworldShape,
    pub data: CompleteOverworldData,
}

impl CompleteOverworldFile {
    pub const MAGIC: [u8; 8] = *b"LMOWFULL";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 40;
    pub const MAX_SECTION_LEN: usize = 0x100_0000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + 9 * Self::MAX_SECTION_LEN;

    /// Encodes all nine modeled overworld payloads in deterministic domain order.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldFileError`] for shape mismatches, invalid records, modes, or
    /// lengths that are not representable by the format.
    pub fn encode(
        &self,
        double_size_modes: &[bool],
    ) -> Result<Vec<u8>, CompleteOverworldFileError> {
        validate_modes(double_size_modes)?;
        validate_data_shape(&self.data, self.shape)?;
        let sections = encode_sections(&self.data, self.shape, double_size_modes)?;
        let animation_len = u32::try_from(sections.animation.len())
            .map_err(|_| CompleteOverworldFileError::Overflow)?;
        let encoded_len = sections
            .all()
            .iter()
            .try_fold(Self::HEADER_LEN, |total, section| {
                total
                    .checked_add(section.len())
                    .ok_or(CompleteOverworldFileError::Overflow)
            })?;
        if encoded_len > Self::MAX_FILE_LEN {
            return Err(CompleteOverworldFileError::FileTooLarge(encoded_len));
        }
        let mut output = Vec::with_capacity(encoded_len);
        output.extend_from_slice(&Self::MAGIC);
        output.extend_from_slice(&Self::VERSION.to_le_bytes());
        output.extend_from_slice(&self.source_slot.to_le_bytes());
        for value in shape_values(self.shape) {
            output.extend_from_slice(&to_u16(value)?.to_le_bytes());
        }
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&animation_len.to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        for section in sections.all() {
            output.extend_from_slice(section);
        }
        debug_assert_eq!(output.len(), encoded_len);
        Ok(output)
    }

    /// Decodes all modeled overworld domains and requires exact file consumption.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldFileError`] for invalid framing, dimensions, records, animation,
    /// overflow, truncation, or trailing bytes.
    pub fn decode(
        bytes: &[u8],
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, CompleteOverworldFileError> {
        validate_modes(double_size_modes)?;
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(CompleteOverworldFileError::FileTooLarge(bytes.len()));
        }
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(CompleteOverworldFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(CompleteOverworldFileError::WrongMagic);
        }
        let version = read_u16(header, 8);
        if version != Self::VERSION {
            return Err(CompleteOverworldFileError::UnsupportedVersion(version));
        }
        if header[36..40] != [0; 4] {
            return Err(CompleteOverworldFileError::ReservedBytes);
        }
        let source_slot = read_u16(header, 10);
        let shape = CompleteOverworldShape {
            width: usize::from(read_u16(header, 12)),
            height: usize::from(read_u16(header, 14)),
            event_reveals: usize::from(read_u16(header, 16)),
            endpoints: usize::from(read_u16(header, 18)),
            messages: usize::from(read_u16(header, 20)),
            sprites: usize::from(read_u16(header, 22)),
            sprite_record_len: usize::from(read_u16(header, 24)),
            palette_colors: usize::from(read_u16(header, 26)),
        };
        validate_shape(shape)?;
        if header[28..32] != [0; 4] {
            return Err(CompleteOverworldFileError::ReservedBytes);
        }
        let animation_len = usize::try_from(u32::from_le_bytes([
            header[32], header[33], header[34], header[35],
        ]))
        .map_err(|_| CompleteOverworldFileError::Overflow)?;
        validate_section_len(animation_len)?;
        let lengths = section_lengths(shape, animation_len)?;
        let expected = lengths.iter().try_fold(Self::HEADER_LEN, |total, length| {
            total
                .checked_add(*length)
                .ok_or(CompleteOverworldFileError::Overflow)
        })?;
        if bytes.len() != expected {
            return Err(CompleteOverworldFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut cursor = Self::HEADER_LEN;
        let mut take = |len: usize| {
            let start = cursor;
            cursor += len;
            &bytes[start..cursor]
        };
        let layer1 = OverworldLayer::decode_le(shape.width, shape.height, take(lengths[0]))
            .map_err(CompleteOverworldFileError::Layer)?;
        let layer2 = OverworldLayer::decode_le(shape.width, shape.height, take(lengths[1]))
            .map_err(CompleteOverworldFileError::Layer)?;
        let event_reveals = EventRevealTable::decode(take(lengths[2]), take(lengths[3]))?;
        let endpoints = OverworldEndpoint::decode_all(take(lengths[4]))
            .map_err(CompleteOverworldFileError::Endpoints)?;
        let messages = OverworldMessage::decode_all(take(lengths[5]))
            .map_err(CompleteOverworldFileError::Messages)?;
        let sprites = OverworldSprite::decode_all(take(lengths[6]), shape.sprite_record_len)?;
        let palette =
            Palette::decode_snes(take(lengths[7])).map_err(CompleteOverworldFileError::Palette)?;
        let animation_bytes = take(lengths[8]);
        let (animation, consumed) = CompactExAnimation::decode(
            animation_bytes,
            maximum_animation_records,
            double_size_modes,
        )?;
        if consumed != animation_bytes.len() {
            return Err(CompleteOverworldFileError::UnconsumedAnimation {
                consumed,
                actual: animation_bytes.len(),
            });
        }
        Ok(Self {
            source_slot,
            shape,
            data: CompleteOverworldData {
                layers: OverworldLayers { layer1, layer2 },
                event_reveals,
                endpoints,
                messages,
                sprites,
                palette,
                animation,
            },
        })
    }
}

struct EncodedSections {
    layer1: Vec<u8>,
    layer2: Vec<u8>,
    event_sources: Vec<u8>,
    event_destinations: Vec<u8>,
    endpoints: Vec<u8>,
    messages: Vec<u8>,
    sprites: Vec<u8>,
    palette: Vec<u8>,
    animation: Vec<u8>,
}

impl EncodedSections {
    fn all(&self) -> [&[u8]; 9] {
        [
            &self.layer1,
            &self.layer2,
            &self.event_sources,
            &self.event_destinations,
            &self.endpoints,
            &self.messages,
            &self.sprites,
            &self.palette,
            &self.animation,
        ]
    }
}

fn encode_sections(
    data: &CompleteOverworldData,
    shape: CompleteOverworldShape,
    modes: &[bool],
) -> Result<EncodedSections, CompleteOverworldFileError> {
    let (event_sources, event_destinations) = data.event_reveals.encode()?;
    let sections = EncodedSections {
        layer1: data.layers.layer1.encode_le()?,
        layer2: data.layers.layer2.encode_le()?,
        event_sources,
        event_destinations,
        endpoints: OverworldEndpoint::encode_all(&data.endpoints)?,
        messages: OverworldMessage::encode_all(&data.messages)?,
        sprites: OverworldSprite::encode_all(&data.sprites, shape.sprite_record_len)?,
        palette: data
            .palette
            .encode_snes()
            .map_err(|_| CompleteOverworldFileError::Overflow)?,
        animation: data.animation.encode(modes)?,
    };
    for section in sections.all() {
        validate_section_len(section.len())?;
    }
    Ok(sections)
}

#[derive(Debug)]
pub enum CompleteOverworldFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    EmptyDimensions,
    WrongSizeModeCount(usize),
    TooManyEventReveals(usize),
    FieldTooLarge(usize),
    SectionTooLarge(usize),
    FileTooLarge(usize),
    WrongLength {
        expected: usize,
        actual: usize,
    },
    ShapeMismatch {
        expected: Box<CompleteOverworldShape>,
        actual: Box<CompleteOverworldShape>,
    },
    UnconsumedAnimation {
        consumed: usize,
        actual: usize,
    },
    Overflow,
    Layer(Vec<u8>),
    LayerEncoding(OverworldLayerEncodingError),
    Events(EventTableError),
    Endpoints(usize),
    Messages(usize),
    FixedTableEncoding(FixedTableEncodingError),
    Sprites(OverworldSpriteError),
    Palette(usize),
    Animation(ExAnimationError),
}

impl fmt::Display for CompleteOverworldFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid complete overworld file: {self:?}")
    }
}

impl std::error::Error for CompleteOverworldFileError {}

impl From<EventTableError> for CompleteOverworldFileError {
    fn from(value: EventTableError) -> Self {
        Self::Events(value)
    }
}

impl From<OverworldLayerEncodingError> for CompleteOverworldFileError {
    fn from(value: OverworldLayerEncodingError) -> Self {
        Self::LayerEncoding(value)
    }
}

impl From<FixedTableEncodingError> for CompleteOverworldFileError {
    fn from(value: FixedTableEncodingError) -> Self {
        Self::FixedTableEncoding(value)
    }
}

impl From<OverworldSpriteError> for CompleteOverworldFileError {
    fn from(value: OverworldSpriteError) -> Self {
        Self::Sprites(value)
    }
}

impl From<ExAnimationError> for CompleteOverworldFileError {
    fn from(value: ExAnimationError) -> Self {
        Self::Animation(value)
    }
}

#[cfg(test)]
#[path = "overworld_file_tests.rs"]
mod tests;
