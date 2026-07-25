use super::{PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult};
use crate::{EditBatch, Project, RomTransaction, RomWrite};
use lm_rats::{AllocationError, FreeSpaceAllocator, ProtectedRange, find_duplicate, parse_at};
use lm_rom::{RomImage, pc_to_snes, snes_to_pc};
use std::ops::Range;

pub(super) fn expanded_staging_image(
    original: &[u8],
    requests: &[PayloadSaveRequest],
) -> Result<Vec<u8>, PayloadSaveError> {
    let target = requests
        .iter()
        .map(|request| request.allocation_policy.search.end)
        .max()
        .unwrap_or(original.len());
    if target <= original.len() {
        return Ok(original.to_vec());
    }
    let Some(first) = requests.first() else {
        return Ok(original.to_vec());
    };
    if requests
        .iter()
        .any(|request| request.mapper != first.mapper)
    {
        return Err(PayloadSaveError::MixedExpansionMappers);
    }
    let fill = first
        .allocation_policy
        .fill_bytes
        .iter()
        .copied()
        .find(|candidate| {
            requests
                .iter()
                .all(|request| request.allocation_policy.fill_bytes.contains(candidate))
        })
        .ok_or(AllocationError::InvalidPolicy)?;
    let mut image = RomImage::from_bytes(original.to_vec())?;
    image.expand(first.mapper, target, fill)?;
    Ok(image.logical_bytes().to_vec())
}

pub(crate) fn commit_staged(
    project: &mut Project,
    description: String,
    original: &[u8],
    staged: &[u8],
) -> Result<(), PayloadSaveError> {
    let writes = changed_writes(original, &staged[..original.len()]);
    let mut transaction = RomTransaction::new(&mut project.rom);
    for write in writes {
        transaction.write(write.offset, &write.bytes, description.clone())?;
    }
    transaction.append(&staged[original.len()..], description.clone())?;
    let edits = transaction.commit();
    if !edits.is_empty() {
        project.history.push_batch(EditBatch {
            description,
            edits,
            kind: crate::EditKind::Ordinary,
            copier_header: None,
        });
        project.synchronize_identity_checksums();
    }
    Ok(())
}

pub(super) fn stage_request(
    staged: &mut [u8],
    request: &PayloadSaveRequest,
    current_pointer_ranges: &[Range<usize>],
    all_pointer_ranges: &[Range<usize>],
) -> Result<PayloadSaveResult, PayloadSaveError> {
    let mut policy = request.allocation_policy.clone();
    policy
        .protected
        .extend(all_pointer_ranges.iter().cloned().map(ProtectedRange));
    policy.validate(staged.len())?;
    if let Some(previous) = &request.previous_block {
        match parse_at(staged, previous.header_offset) {
            Ok(actual) if &actual == previous => {}
            _ => return Err(AllocationError::InvalidBlock.into()),
        }
    }
    let duplicate = request
        .reuse_identical
        .then(|| {
            find_duplicate(
                staged,
                &request.payload,
                request.maximum_payload_len,
                &policy,
            )
        })
        .transpose()?
        .flatten();
    let (block, reused_existing) = if let Some(block) = duplicate {
        (block, true)
    } else {
        let mut allocator = FreeSpaceAllocator::new(&mut *staged, policy);
        // Native pointer tables may share byte-identical RATS payloads. Without a complete
        // revision-specific reference index, mutating or erasing `previous_block` could corrupt a
        // different slot. Allocate copy-on-write and leave reclamation to an explicit ownership
        // pass that has proved the block exclusive.
        let block = allocator.allocate(&request.payload)?;
        (block, false)
    };

    let snes_pointer = pc_to_snes(request.mapper, block.payload.start)?;
    let mut pointer = snes_pointer.to_le_bytes();
    if matches!(request.pointer, PayloadPointer::ContiguousLowBank { .. }) {
        pointer[2] &= 0x7f;
    }
    match request.pointer {
        PayloadPointer::Contiguous { .. } | PayloadPointer::ContiguousLowBank { .. } => {
            staged[current_pointer_ranges[0].clone()].copy_from_slice(&pointer[..3]);
        }
        PayloadPointer::Split {
            bank_offset,
            shared_bank,
            ..
        } => {
            staged[current_pointer_ranges[0].clone()].copy_from_slice(&pointer[..2]);
            if shared_bank {
                let existing = staged[bank_offset];
                let low_word = u32::from(pointer[0]) | (u32::from(pointer[1]) << 8);
                let existing_address = (u32::from(existing) << 16) | low_word;
                let required_address = (u32::from(pointer[2]) << 16) | low_word;
                let same_mapped_bank = matches!(
                    (
                        snes_to_pc(request.mapper, existing_address),
                        snes_to_pc(request.mapper, required_address)
                    ),
                    (Ok(existing_pc), Ok(required_pc)) if existing_pc == required_pc
                );
                if !same_mapped_bank {
                    return Err(PayloadSaveError::SharedPointerBankMismatch {
                        bank_offset,
                        existing,
                        required: pointer[2],
                    });
                }
            } else {
                staged[current_pointer_ranges[1].clone()].copy_from_slice(&pointer[2..3]);
            }
        }
        PayloadPointer::SplitBytes { .. } => {
            for (range, byte) in current_pointer_ranges.iter().zip(pointer) {
                staged[range.clone()].copy_from_slice(std::slice::from_ref(&byte));
            }
        }
    }

    Ok(PayloadSaveResult {
        block,
        snes_pointer,
        reused_existing,
    })
}

fn changed_writes(before: &[u8], after: &[u8]) -> Vec<RomWrite> {
    let mut writes = Vec::new();
    let mut start = None;
    for (index, (old, new)) in before.iter().zip(after).enumerate() {
        if old != new {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            writes.push(RomWrite {
                offset: begin,
                bytes: after[begin..index].to_vec(),
            });
        }
    }
    if let Some(begin) = start {
        writes.push(RomWrite {
            offset: begin,
            bytes: after[begin..].to_vec(),
        });
    }
    writes
}
