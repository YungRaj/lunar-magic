use super::{CompleteOverworldFile, CompleteOverworldFileError, CompleteOverworldShape};
use crate::CompleteOverworldData;
use lm_overworld::{
    EventRevealTable, OverworldEndpoint, OverworldMessage, OverworldSprite, OverworldSpriteError,
};

pub(super) fn validate_data_shape(
    data: &CompleteOverworldData,
    shape: CompleteOverworldShape,
) -> Result<(), CompleteOverworldFileError> {
    validate_shape(shape)?;
    let actual = CompleteOverworldShape {
        width: data.layers.layer1.width,
        height: data.layers.layer1.height,
        event_reveals: data.event_reveals.entries.len(),
        endpoints: data.endpoints.len(),
        messages: data.messages.len(),
        sprites: data.sprites.len(),
        sprite_record_len: shape.sprite_record_len,
        palette_colors: data.palette.colors.len(),
    };
    let tile_count = shape.width.checked_mul(shape.height);
    if actual != shape
        || data.layers.layer2.width != shape.width
        || data.layers.layer2.height != shape.height
        || Some(data.layers.layer1.tiles.len()) != tile_count
        || Some(data.layers.layer2.tiles.len()) != tile_count
    {
        return Err(CompleteOverworldFileError::ShapeMismatch {
            expected: Box::new(shape),
            actual: Box::new(actual),
        });
    }
    Ok(())
}

pub(super) fn validate_shape(
    shape: CompleteOverworldShape,
) -> Result<(), CompleteOverworldFileError> {
    if shape.width == 0 || shape.height == 0 {
        return Err(CompleteOverworldFileError::EmptyDimensions);
    }
    if shape.event_reveals > EventRevealTable::MAX_ENTRIES {
        return Err(CompleteOverworldFileError::TooManyEventReveals(
            shape.event_reveals,
        ));
    }
    if shape.sprite_record_len < OverworldSprite::OWNED_LEN {
        return Err(CompleteOverworldFileError::Sprites(
            OverworldSpriteError::RecordTooShort(shape.sprite_record_len),
        ));
    }
    for value in shape_values(shape) {
        to_u16(value)?;
    }
    Ok(())
}

pub(super) fn section_lengths(
    shape: CompleteOverworldShape,
    animation_len: usize,
) -> Result<[usize; 9], CompleteOverworldFileError> {
    let tiles = shape
        .width
        .checked_mul(shape.height)
        .and_then(|value| value.checked_mul(2))
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let events = shape
        .event_reveals
        .checked_mul(2)
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let endpoints = shape
        .endpoints
        .checked_mul(OverworldEndpoint::ENCODED_LEN)
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let messages = shape
        .messages
        .checked_mul(OverworldMessage::ENCODED_LEN)
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let sprites = shape
        .sprites
        .checked_mul(shape.sprite_record_len)
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let palette = shape
        .palette_colors
        .checked_mul(2)
        .ok_or(CompleteOverworldFileError::Overflow)?;
    let lengths = [
        tiles,
        tiles,
        events,
        events,
        endpoints,
        messages,
        sprites,
        palette,
        animation_len,
    ];
    for len in lengths {
        validate_section_len(len)?;
    }
    Ok(lengths)
}

pub(super) fn validate_section_len(len: usize) -> Result<(), CompleteOverworldFileError> {
    if len > CompleteOverworldFile::MAX_SECTION_LEN {
        Err(CompleteOverworldFileError::SectionTooLarge(len))
    } else {
        Ok(())
    }
}

pub(super) fn shape_values(shape: CompleteOverworldShape) -> [usize; 8] {
    [
        shape.width,
        shape.height,
        shape.event_reveals,
        shape.endpoints,
        shape.messages,
        shape.sprites,
        shape.sprite_record_len,
        shape.palette_colors,
    ]
}

pub(super) fn to_u16(value: usize) -> Result<u16, CompleteOverworldFileError> {
    u16::try_from(value).map_err(|_| CompleteOverworldFileError::FieldTooLarge(value))
}

pub(super) fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub(super) fn validate_modes(modes: &[bool]) -> Result<(), CompleteOverworldFileError> {
    if modes.len() == 256 {
        Ok(())
    } else {
        Err(CompleteOverworldFileError::WrongSizeModeCount(modes.len()))
    }
}
