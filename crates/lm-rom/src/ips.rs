//! Bounded International Patching System (IPS) patches.

use std::fmt;

const MAGIC: &[u8; 5] = b"PATCH";
const EOF: &[u8; 3] = b"EOF";
const MAX_RECORD_LEN: usize = u16::MAX as usize;
const RESERVED_OFFSET: usize = 0x45_4f_46;

pub const MAX_IPS_IMAGE_LEN: usize = 0x100_0000;
pub const MAX_IPS_PATCH_LEN: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IpsError {
    ImageTooLarge(usize),
    PatchTooLarge(usize),
    Truncated,
    WrongMagic,
    ZeroLengthRle,
    InvalidTrailer(usize),
    OffsetOverflow,
    OutputTooLarge(usize),
}

impl fmt::Display for IpsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "IPS error: {self:?}")
    }
}

impl std::error::Error for IpsError {}

/// Applies one exactly consumed IPS patch in record order.
///
/// # Errors
///
/// Returns [`IpsError`] for invalid framing, truncated records, zero-length RLE, arithmetic
/// overflow, or inputs and results outside the bounded IPS address space.
pub fn apply_ips(source: &[u8], patch: &[u8]) -> Result<Vec<u8>, IpsError> {
    check_image(source.len())?;
    if patch.len() > MAX_IPS_PATCH_LEN {
        return Err(IpsError::PatchTooLarge(patch.len()));
    }
    if patch.get(..MAGIC.len()) != Some(MAGIC) {
        return Err(if patch.len() < MAGIC.len() {
            IpsError::Truncated
        } else {
            IpsError::WrongMagic
        });
    }
    let mut output = source.to_vec();
    let mut cursor = MAGIC.len();
    loop {
        let marker = patch.get(cursor..cursor + 3).ok_or(IpsError::Truncated)?;
        cursor += 3;
        if marker == EOF {
            match patch.len() - cursor {
                0 => return Ok(output),
                3 => {
                    let length = read_u24(&patch[cursor..cursor + 3]);
                    check_image(length)?;
                    output.resize(length, 0);
                    return Ok(output);
                }
                trailing => return Err(IpsError::InvalidTrailer(trailing)),
            }
        }
        let offset = read_u24(marker);
        let size_bytes = patch.get(cursor..cursor + 2).ok_or(IpsError::Truncated)?;
        cursor += 2;
        let size = usize::from(u16::from_be_bytes([size_bytes[0], size_bytes[1]]));
        if size == 0 {
            let run_bytes = patch.get(cursor..cursor + 2).ok_or(IpsError::Truncated)?;
            cursor += 2;
            let run = usize::from(u16::from_be_bytes([run_bytes[0], run_bytes[1]]));
            if run == 0 {
                return Err(IpsError::ZeroLengthRle);
            }
            let value = *patch.get(cursor).ok_or(IpsError::Truncated)?;
            cursor += 1;
            write_record(&mut output, offset, run, |target| target.fill(value))?;
        } else {
            let data = patch
                .get(cursor..cursor + size)
                .ok_or(IpsError::Truncated)?;
            cursor += size;
            write_record(&mut output, offset, size, |target| {
                target.copy_from_slice(data);
            })?;
        }
    }
}

/// Creates a deterministic IPS patch transforming `source` into `target`.
///
/// # Errors
///
/// Returns [`IpsError`] if either image exceeds the IPS address space, a record cannot be
/// represented, or the resulting patch exceeds the workspace patch bound.
pub fn create_ips(source: &[u8], target: &[u8]) -> Result<Vec<u8>, IpsError> {
    check_image(source.len())?;
    check_image(target.len())?;
    let mut patch = Vec::new();
    patch.extend_from_slice(MAGIC);
    let mut cursor = 0;
    while cursor < target.len() {
        if source.get(cursor) == Some(&target[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        while cursor < target.len() && source.get(cursor) != Some(&target[cursor]) {
            cursor += 1;
        }
        encode_changed_range(&mut patch, target, start, cursor)?;
        if patch.len() > MAX_IPS_PATCH_LEN {
            return Err(IpsError::PatchTooLarge(patch.len()));
        }
    }
    patch.extend_from_slice(EOF);
    if target.len() < source.len() {
        write_u24(&mut patch, target.len())?;
    }
    if patch.len() > MAX_IPS_PATCH_LEN {
        return Err(IpsError::PatchTooLarge(patch.len()));
    }
    Ok(patch)
}

fn encode_changed_range(
    patch: &mut Vec<u8>,
    target: &[u8],
    mut start: usize,
    end: usize,
) -> Result<(), IpsError> {
    while start < end {
        let mut record_start = start;
        let mut record_end = end.min(start + MAX_RECORD_LEN);
        if record_start == RESERVED_OFFSET {
            record_start -= 1;
        }
        if record_end - record_start > MAX_RECORD_LEN {
            record_end = record_start + MAX_RECORD_LEN;
        }
        let data = &target[record_start..record_end];
        write_u24(patch, record_start)?;
        if data.len() >= 4 && data.iter().all(|value| *value == data[0]) {
            patch.extend_from_slice(&0_u16.to_be_bytes());
            patch.extend_from_slice(
                &u16::try_from(data.len())
                    .map_err(|_| IpsError::OffsetOverflow)?
                    .to_be_bytes(),
            );
            patch.push(data[0]);
        } else {
            patch.extend_from_slice(
                &u16::try_from(data.len())
                    .map_err(|_| IpsError::OffsetOverflow)?
                    .to_be_bytes(),
            );
            patch.extend_from_slice(data);
        }
        start = record_end;
    }
    Ok(())
}

fn write_record(
    output: &mut Vec<u8>,
    offset: usize,
    len: usize,
    write: impl FnOnce(&mut [u8]),
) -> Result<(), IpsError> {
    let end = offset.checked_add(len).ok_or(IpsError::OffsetOverflow)?;
    if end > MAX_IPS_IMAGE_LEN {
        return Err(IpsError::OutputTooLarge(end));
    }
    if output.len() < end {
        output.resize(end, 0);
    }
    write(&mut output[offset..end]);
    Ok(())
}

fn check_image(len: usize) -> Result<(), IpsError> {
    if len > MAX_IPS_IMAGE_LEN {
        Err(IpsError::ImageTooLarge(len))
    } else {
        Ok(())
    }
}

fn read_u24(bytes: &[u8]) -> usize {
    usize::from(bytes[0]) << 16 | usize::from(bytes[1]) << 8 | usize::from(bytes[2])
}

fn write_u24(output: &mut Vec<u8>, value: usize) -> Result<(), IpsError> {
    if value >= MAX_IPS_IMAGE_LEN {
        return Err(IpsError::OffsetOverflow);
    }
    output.extend_from_slice(&[
        u8::try_from(value >> 16).map_err(|_| IpsError::OffsetOverflow)?,
        u8::try_from((value >> 8) & 0xff).map_err(|_| IpsError::OffsetOverflow)?,
        u8::try_from(value & 0xff).map_err(|_| IpsError::OffsetOverflow)?,
    ]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_rle_overlap_and_truncate_apply_in_order() {
        let patch = b"PATCH\x00\x00\x01\x00\x03abc\x00\x00\x02\x00\x00\x00\x04xEOF\x00\x00\x05";
        assert_eq!(apply_ips(b"0123456789", patch).unwrap(), b"0axxx");
    }

    #[test]
    fn deterministic_creation_round_trips_growth_shrink_and_rle() {
        for (source, target) in [
            (b"abcdef".as_slice(), b"abZZZZefghi".as_slice()),
            (b"abcdefgh".as_slice(), b"abX".as_slice()),
            (b"same".as_slice(), b"same".as_slice()),
        ] {
            let patch = create_ips(source, target).unwrap();
            assert_eq!(apply_ips(source, &patch).unwrap(), target);
            assert_eq!(create_ips(source, target).unwrap(), patch);
        }
    }

    #[test]
    fn reserved_eof_offset_is_encoded_by_extending_backward() {
        let source = vec![0; RESERVED_OFFSET + 2];
        let mut target = source.clone();
        target[RESERVED_OFFSET] = 7;
        let patch = create_ips(&source, &target).unwrap();
        assert_ne!(&patch[5..8], EOF);
        assert_eq!(apply_ips(&source, &patch).unwrap(), target);
    }

    #[test]
    fn malformed_framing_and_limits_are_rejected() {
        assert_eq!(apply_ips(b"", b"PAT"), Err(IpsError::Truncated));
        assert_eq!(apply_ips(b"", b"WRONG"), Err(IpsError::WrongMagic));
        assert_eq!(
            apply_ips(b"", b"PATCHEOFx"),
            Err(IpsError::InvalidTrailer(1))
        );
        assert_eq!(
            apply_ips(b"", b"PATCH\x00\x00\x00\x00\x00\x00\x00xEOF"),
            Err(IpsError::ZeroLengthRle)
        );
        assert_eq!(
            apply_ips(b"", b"PATCH\xff\xff\xff\x00\x02abEOF"),
            Err(IpsError::OutputTooLarge(MAX_IPS_IMAGE_LEN + 1))
        );
        assert_eq!(
            create_ips(&vec![0; MAX_IPS_IMAGE_LEN + 1], b""),
            Err(IpsError::ImageTooLarge(MAX_IPS_IMAGE_LEN + 1))
        );
    }
}
