use super::{PayloadPointer, PayloadSaveError, PayloadSaveRequest};
use crate::{Project, RomWrite};
use lm_rom::mapper_supports_image_len;
use std::ops::Range;

pub(super) fn validate_checksum_protection(
    requests: &[PayloadSaveRequest],
    checksum_field: Option<usize>,
) -> Result<(), PayloadSaveError> {
    let Some(field) = checksum_field else {
        return Ok(());
    };
    let end = field
        .checked_add(4)
        .ok_or(PayloadSaveError::ChecksumUnprotected {
            checksum_field: field,
        })?;
    let checksum = field..end;
    if requests.iter().any(|request| {
        overlaps(&request.allocation_policy.search, &checksum)
            && !request.allocation_policy.protected.iter().any(|protected| {
                protected.0.start <= checksum.start && checksum.end <= protected.0.end
            })
    }) {
        return Err(PayloadSaveError::ChecksumUnprotected {
            checksum_field: field,
        });
    }
    Ok(())
}

pub(super) fn checked_pointer_ranges(
    requests: &[PayloadSaveRequest],
) -> Result<Vec<Vec<Range<usize>>>, PayloadSaveError> {
    let grouped = requests
        .iter()
        .map(|request| {
            let range = |offset: usize, len: usize| {
                offset
                    .checked_add(len)
                    .map(|end| offset..end)
                    .ok_or(PayloadSaveError::PointerRangeOverflow { offset })
            };
            match request.pointer {
                PayloadPointer::Contiguous { offset }
                | PayloadPointer::ContiguousLowBank { offset } => Ok(vec![range(offset, 3)?]),
                PayloadPointer::Split {
                    low_word_offset,
                    bank_offset,
                    ..
                } => Ok(vec![range(low_word_offset, 2)?, range(bank_offset, 1)?]),
                PayloadPointer::SplitBytes {
                    low_offset,
                    high_offset,
                    bank_offset,
                } => Ok(vec![
                    range(low_offset, 1)?,
                    range(high_offset, 1)?,
                    range(bank_offset, 1)?,
                ]),
            }
        })
        .collect::<Result<Vec<_>, PayloadSaveError>>()?;
    let ranges = grouped.iter().flatten().collect::<Vec<_>>();
    for (index, first) in ranges.iter().enumerate() {
        for second in &ranges[index + 1..] {
            if overlaps(first, second) {
                return Err(PayloadSaveError::OverlappingPointers {
                    first_offset: first.start,
                    second_offset: second.start,
                });
            }
        }
    }
    Ok(grouped)
}

pub(super) fn checked_extra_ranges(
    requests: &[PayloadSaveRequest],
    writes: &[RomWrite],
    pointers: &[Range<usize>],
    checksum_field: Option<usize>,
) -> Result<Vec<Range<usize>>, PayloadSaveError> {
    let ranges = writes
        .iter()
        .map(|write| {
            let end = write.offset.checked_add(write.bytes.len()).ok_or(
                PayloadSaveError::InvalidExtraWrite {
                    offset: write.offset,
                    len: write.bytes.len(),
                },
            )?;
            if write.bytes.is_empty() {
                return Err(PayloadSaveError::InvalidExtraWrite {
                    offset: write.offset,
                    len: 0,
                });
            }
            Ok(write.offset..end)
        })
        .collect::<Result<Vec<_>, PayloadSaveError>>()?;
    for (index, range) in ranges.iter().enumerate() {
        for second in &ranges[index + 1..] {
            if overlaps(range, second) {
                return Err(PayloadSaveError::OverlappingExtraWrites {
                    first_offset: range.start,
                    second_offset: second.start,
                });
            }
        }
        if let Some(pointer) = pointers.iter().find(|pointer| overlaps(range, pointer)) {
            return Err(PayloadSaveError::ExtraWriteOverlapsPointer {
                write_offset: range.start,
                pointer_offset: pointer.start,
            });
        }
        if let Some(field) = checksum_field {
            let checksum_end =
                field
                    .checked_add(4)
                    .ok_or(PayloadSaveError::ExtraWriteOverlapsChecksum {
                        offset: range.start,
                        checksum_field: field,
                    })?;
            if overlaps(range, &(field..checksum_end)) {
                return Err(PayloadSaveError::ExtraWriteOverlapsChecksum {
                    offset: range.start,
                    checksum_field: field,
                });
            }
        }
        if requests.iter().any(|request| {
            !request
                .allocation_policy
                .protected
                .iter()
                .any(|protected| protected.0.start <= range.start && range.end <= protected.0.end)
        }) {
            return Err(PayloadSaveError::ExtraWriteUnprotected {
                offset: range.start,
                len: range.end - range.start,
            });
        }
    }
    Ok(ranges)
}

pub(super) fn validate_request(
    project: &Project,
    request: &PayloadSaveRequest,
) -> Result<(), PayloadSaveError> {
    if !mapper_supports_image_len(request.mapper, project.rom.logical_len()) {
        return Err(PayloadSaveError::MapperCannotAddressImage {
            mapper: request.mapper,
            image_len: project.rom.logical_len(),
        });
    }
    if request.payload.len() > request.maximum_payload_len {
        return Err(PayloadSaveError::PayloadLimit {
            actual: request.payload.len(),
            maximum: request.maximum_payload_len,
        });
    }
    match request.pointer {
        PayloadPointer::Contiguous { offset } | PayloadPointer::ContiguousLowBank { offset } => {
            project.rom.read(offset, 3)?;
        }
        PayloadPointer::Split {
            low_word_offset,
            bank_offset,
            ..
        } => {
            project.rom.read(low_word_offset, 2)?;
            project.rom.read(bank_offset, 1)?;
        }
        PayloadPointer::SplitBytes {
            low_offset,
            high_offset,
            bank_offset,
        } => {
            project.rom.read(low_offset, 1)?;
            project.rom.read(high_offset, 1)?;
            project.rom.read(bank_offset, 1)?;
        }
    }
    Ok(())
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
