use super::{CustomSpriteEntry, CustomSpriteLibraryError, MAX_CUSTOM_SPRITE_SIDECAR_LEN};
use crate::{DescriptionFormat, LineEnding, SpriteLengthTable, SpriteRecord};

const MAX_DESCRIPTION_LEN: usize = 1024;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

pub(super) fn decode_placements(
    data: &[u8],
    lengths: &SpriteLengthTable,
) -> Result<(u8, Vec<Vec<SpriteRecord>>), CustomSpriteLibraryError> {
    let &header = data
        .first()
        .ok_or(CustomSpriteLibraryError::MissingHeader)?;
    let mut offset = 1usize;
    let mut placements: Vec<Vec<SpriteRecord>> = Vec::new();
    loop {
        let first = *data
            .get(offset)
            .ok_or(CustomSpriteLibraryError::MissingTerminator)?;
        if first == 0xff {
            if offset + 1 != data.len() {
                return Err(CustomSpriteLibraryError::TrailingData(
                    data.len() - offset - 1,
                ));
            }
            return Ok((header, placements));
        }
        let len = lengths
            .record_len(&data[offset..])
            .ok_or(CustomSpriteLibraryError::MalformedSprite { offset })?;
        let end = offset
            .checked_add(len)
            .ok_or(CustomSpriteLibraryError::MalformedSprite { offset })?;
        let encoded = data
            .get(offset..end)
            .ok_or(CustomSpriteLibraryError::MalformedSprite { offset })?
            .to_vec();
        if placements.is_empty() || first & 1 != 0 {
            placements.push(Vec::new());
        }
        let placement = placements
            .last_mut()
            .ok_or(CustomSpriteLibraryError::MalformedSprite { offset })?;
        placement.push(SpriteRecord { encoded });
        offset = end;
    }
}

pub(super) fn decode_descriptions(
    bytes: &[u8],
    expected: usize,
) -> Result<(Vec<String>, DescriptionFormat), CustomSpriteLibraryError> {
    let (bytes, utf8_bom) = bytes
        .strip_prefix(UTF8_BOM)
        .map_or((bytes, false), |text| (text, true));
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CustomSpriteLibraryError::InvalidDescriptionEncoding)?;
    if text.contains('\0') || text.contains('\r') && !text.contains("\r\n") {
        return Err(CustomSpriteLibraryError::InvalidDescription);
    }
    let has_crlf = text.contains("\r\n");
    let stripped = text.replace("\r\n", "");
    if has_crlf && stripped.contains('\n') || stripped.contains('\r') {
        return Err(CustomSpriteLibraryError::MixedLineEndings);
    }
    let (line_ending, separator) = if has_crlf {
        (LineEnding::CrLf, "\r\n")
    } else {
        (LineEnding::Lf, "\n")
    };
    let mut values: Vec<String> = if text.is_empty() {
        if expected == 1 {
            vec![String::new()]
        } else {
            Vec::new()
        }
    } else {
        text.split(separator).map(str::to_owned).collect()
    };
    let trailing = values.len() == expected + 1 && values.last().is_some_and(String::is_empty);
    if trailing {
        values.pop();
    }
    Ok((
        values,
        DescriptionFormat {
            utf8_bom,
            line_ending,
            trailing_line_ending: trailing,
        },
    ))
}

pub(super) fn validate_entry(
    sprites: &[SpriteRecord],
    description: &str,
) -> Result<(), CustomSpriteLibraryError> {
    if sprites.is_empty() {
        return Err(CustomSpriteLibraryError::EmptyPlacement);
    }
    if sprites.iter().any(|sprite| sprite.encoded.len() < 3) {
        return Err(CustomSpriteLibraryError::MalformedSprite { offset: 0 });
    }
    if sprites
        .iter()
        .skip(1)
        .any(|sprite| sprite.encoded[0] & 1 != 0)
    {
        return Err(CustomSpriteLibraryError::UnexpectedPlacementBoundary);
    }
    if description.len() > MAX_DESCRIPTION_LEN || description.contains(['\0', '\r', '\n']) {
        return Err(CustomSpriteLibraryError::InvalidDescription);
    }
    Ok(())
}

fn line_ending(format: DescriptionFormat) -> &'static [u8] {
    match format.line_ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    }
}

pub(super) fn encoded_data_len(
    entries: &[CustomSpriteEntry],
) -> Result<usize, CustomSpriteLibraryError> {
    entries.iter().try_fold(2usize, |total, entry| {
        entry.sprites.iter().try_fold(total, |total, sprite| {
            total
                .checked_add(sprite.encoded.len())
                .filter(|total| *total <= MAX_CUSTOM_SPRITE_SIDECAR_LEN)
                .ok_or(CustomSpriteLibraryError::DataTooLarge)
        })
    })
}

pub(super) fn encoded_description_len(
    entries: &[CustomSpriteEntry],
    format: DescriptionFormat,
) -> Result<usize, CustomSpriteLibraryError> {
    let mut total = usize::from(format.utf8_bom) * UTF8_BOM.len();
    for (index, entry) in entries.iter().enumerate() {
        total = total
            .checked_add(entry.description.len())
            .ok_or(CustomSpriteLibraryError::DescriptionsTooLarge)?;
        if index + 1 < entries.len() || format.trailing_line_ending {
            total = total
                .checked_add(line_ending(format).len())
                .ok_or(CustomSpriteLibraryError::DescriptionsTooLarge)?;
        }
        if total > MAX_CUSTOM_SPRITE_SIDECAR_LEN {
            return Err(CustomSpriteLibraryError::DescriptionsTooLarge);
        }
    }
    Ok(total)
}

pub(super) fn encode_descriptions(
    entries: &[CustomSpriteEntry],
    format: DescriptionFormat,
) -> Result<Vec<u8>, CustomSpriteLibraryError> {
    let mut bytes = Vec::with_capacity(encoded_description_len(entries, format)?);
    if format.utf8_bom {
        bytes.extend_from_slice(UTF8_BOM);
    }
    for (index, entry) in entries.iter().enumerate() {
        bytes.extend_from_slice(entry.description.as_bytes());
        if index + 1 < entries.len() || format.trailing_line_ending {
            bytes.extend_from_slice(line_ending(format));
        }
    }
    Ok(bytes)
}

pub(super) fn validate_sizes(
    entries: &[CustomSpriteEntry],
    format: DescriptionFormat,
) -> Result<(), CustomSpriteLibraryError> {
    for (index, entry) in entries.iter().enumerate() {
        validate_entry(&entry.sprites, &entry.description)?;
        if index != 0 && entry.sprites[0].encoded[0] & 1 == 0 {
            return Err(CustomSpriteLibraryError::MissingPlacementBoundary);
        }
    }
    encoded_data_len(entries)?;
    encoded_description_len(entries, format)?;
    Ok(())
}
