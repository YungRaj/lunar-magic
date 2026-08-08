use super::{
    CUSTOM_OBJECT_HEADER_LEN, CustomObjectEntry, CustomObjectLibraryError, DescriptionFormat,
    LineEnding, MAX_CUSTOM_OBJECT_SIDECAR_LEN, UTF8_BOM,
};
use crate::{ObjectRecord, encoded_record_length};

pub(super) const MAX_DESCRIPTION_LEN: usize = 1024;

pub(super) fn decode_objects(
    data: &[u8],
) -> Result<([u8; CUSTOM_OBJECT_HEADER_LEN], Vec<Vec<ObjectRecord>>), CustomObjectLibraryError> {
    // Early Rust builds emitted a headerless pair. Accept the only unambiguous short form so
    // those documents can be opened and canonically saved into Lunar Magic's native framing.
    let (header, mut offset) = if data.len() < CUSTOM_OBJECT_HEADER_LEN + 1 {
        if data.len() < 4 || data.last() != Some(&0xff) {
            return Err(CustomObjectLibraryError::MissingHeader);
        }
        ([0; CUSTOM_OBJECT_HEADER_LEN], 0)
    } else {
        (
            data[..CUSTOM_OBJECT_HEADER_LEN]
                .try_into()
                .expect("bounded header slice"),
            CUSTOM_OBJECT_HEADER_LEN,
        )
    };
    let mut groups: Vec<Vec<ObjectRecord>> = Vec::new();
    loop {
        let first = *data
            .get(offset)
            .ok_or(CustomObjectLibraryError::MissingTerminator)?;
        if first == 0xff {
            if offset + 1 != data.len() {
                return Err(CustomObjectLibraryError::TrailingObjectBytes(
                    data.len() - offset - 1,
                ));
            }
            return Ok((header, groups));
        }
        let length = encoded_record_length(&data[offset..])
            .ok_or(CustomObjectLibraryError::MalformedObject { offset })?;
        let end = offset
            .checked_add(length)
            .ok_or(CustomObjectLibraryError::MalformedObject { offset })?;
        let bytes = data
            .get(offset..end)
            .ok_or(CustomObjectLibraryError::MalformedObject { offset })?;
        let mut object = ObjectRecord::new(bytes.to_vec())
            .map_err(|_| CustomObjectLibraryError::MalformedObject { offset })?;
        let starts_group = object.advances_screen();
        if groups.is_empty() || starts_group {
            groups.push(Vec::new());
        }
        if starts_group {
            object
                .set_raw_advances_screen(false)
                .map_err(|_| CustomObjectLibraryError::InvalidGroupBoundary)?;
        }
        groups.last_mut().expect("group inserted").push(object);
        offset = end;
    }
}

pub(super) fn decode_descriptions(
    text: &str,
    expected_count: usize,
) -> Result<(Vec<String>, LineEnding, bool), CustomObjectLibraryError> {
    if text.contains('\0') || text.contains('\r') && !text.contains("\r\n") {
        return Err(CustomObjectLibraryError::InvalidDescription);
    }
    let has_crlf = text.contains("\r\n");
    let without_crlf = text.replace("\r\n", "");
    if has_crlf && without_crlf.contains('\n') || without_crlf.contains('\r') {
        return Err(CustomObjectLibraryError::MixedLineEndings);
    }
    let line_ending = if has_crlf {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };
    let separator = std::str::from_utf8(line_ending.bytes()).expect("ASCII line ending");
    let mut values = if text.is_empty() {
        if expected_count == 1 {
            vec![String::new()]
        } else {
            Vec::new()
        }
    } else {
        text.split(separator).map(str::to_owned).collect()
    };
    let trailing =
        if values.len() == expected_count + 1 && values.last().is_some_and(String::is_empty) {
            values.pop();
            true
        } else {
            false
        };
    Ok((values, line_ending, trailing))
}

pub(super) fn encoded_data_len(
    entries: &[CustomObjectEntry],
) -> Result<usize, CustomObjectLibraryError> {
    entries
        .iter()
        .try_fold(CUSTOM_OBJECT_HEADER_LEN + 1, |length, entry| {
            entry.objects().try_fold(length, |length, object| {
                length
                    .checked_add(object.encoded().len())
                    .filter(|length| *length <= MAX_CUSTOM_OBJECT_SIDECAR_LEN)
                    .ok_or(CustomObjectLibraryError::DataTooLarge)
            })
        })
}

pub(super) fn encoded_description_len(
    entries: &[CustomObjectEntry],
    format: DescriptionFormat,
) -> Result<usize, CustomObjectLibraryError> {
    let mut length = usize::from(format.utf8_bom) * UTF8_BOM.len();
    for (index, entry) in entries.iter().enumerate() {
        length = length
            .checked_add(entry.description.len())
            .ok_or(CustomObjectLibraryError::DescriptionsTooLarge)?;
        if index + 1 < entries.len() || format.trailing_line_ending {
            length = length
                .checked_add(format.line_ending.bytes().len())
                .ok_or(CustomObjectLibraryError::DescriptionsTooLarge)?;
        }
        if length > MAX_CUSTOM_OBJECT_SIDECAR_LEN {
            return Err(CustomObjectLibraryError::DescriptionsTooLarge);
        }
    }
    Ok(length)
}

pub(super) fn validate_encoded_sizes(
    entries: &[CustomObjectEntry],
    format: DescriptionFormat,
) -> Result<(), CustomObjectLibraryError> {
    encoded_data_len(entries)?;
    encoded_description_len(entries, format)?;
    Ok(())
}
