use crate::{
    EditKind, GraphicsCompression, GraphicsIoError, GraphicsRomLayout, PayloadReadPolicy,
    PayloadSaveError, PayloadSaveRequest, Project, RomMutation,
};
use lm_codec::{encode_lz2, encode_lz3};
use lm_rats::{AllocationPolicy, ProtectedRange};

/// Stable history marker used by application shells to synchronize effective codec metadata.
pub const GRAPHICS_COMPRESSION_MIGRATION_DESCRIPTION: &str = "change graphics compression";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsMigrationOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
    pub checksum_field: usize,
}

impl Project {
    /// Recompresses every declared native graphics slot as one atomic, undoable ROM mutation.
    ///
    /// All source files are decoded before staging begins. The staged image then allocates and
    /// repoints every target stream, repairs its checksum, and reopens every slot with the target
    /// codec before one compact mutation is applied to this project.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsIoError`] for any source decode, size, allocation, checksum, transaction,
    /// or semantic-reopen failure. The receiver and its history remain unchanged on failure.
    pub fn migrate_graphics_compression(
        &mut self,
        source: GraphicsRomLayout,
        target: GraphicsCompression,
        options: &GraphicsMigrationOptions,
    ) -> Result<bool, GraphicsIoError> {
        if source.compression == target {
            return Ok(false);
        }
        let entries = source
            .split_pointer_planes
            .map_or(source.pointers.entries, |planes| planes.entries);
        let files = (0..entries)
            .map(|slot| self.load_graphics_file(slot, source))
            .collect::<Result<Vec<_>, _>>()?;
        let previous_blocks = (0..entries)
            .map(|slot| {
                Ok(self
                    .load_payload_from_pointer(
                        source.read_pointer(self, slot)?,
                        source.mapper,
                        &PayloadReadPolicy::TaggedOrBounded {
                            maximum_len: source.maximum_compressed_len,
                            bank_size: None,
                        },
                    )?
                    .block)
            })
            .collect::<Result<Vec<_>, GraphicsIoError>>()?;

        let target_layout = GraphicsRomLayout {
            compression: target,
            ..source
        };
        let allocation = protected_policy(source, options)?;
        let requests = files
            .iter()
            .zip(previous_blocks)
            .enumerate()
            .map(|(slot, (graphics, previous_block))| {
                let raw = graphics.encode()?;
                if raw.len() > source.maximum_decompressed_len {
                    return Err(GraphicsIoError::DecompressedLimit {
                        actual: raw.len(),
                        maximum: source.maximum_decompressed_len,
                    });
                }
                let payload = match target {
                    GraphicsCompression::Lz2 => encode_lz2(&raw),
                    GraphicsCompression::Lz3 => encode_lz3(&raw),
                };
                if payload.len() > source.maximum_compressed_len {
                    return Err(GraphicsIoError::CompressedLimit {
                        actual: payload.len(),
                        maximum: source.maximum_compressed_len,
                    });
                }
                Ok(PayloadSaveRequest {
                    description: format!("recompress graphics file {slot:02x}"),
                    payload,
                    pointer: source.payload_pointer(slot)?,
                    mapper: source.mapper,
                    allocation_policy: allocation.clone(),
                    previous_block,
                    reuse_identical: options.reuse_identical,
                    maximum_payload_len: source.maximum_compressed_len,
                    erase_fill: options.erase_fill,
                })
            })
            .collect::<Result<Vec<_>, GraphicsIoError>>()?;

        let before = self.rom.logical_bytes().to_vec();
        let mut staged = self.clone();
        staged.save_tagged_payloads("recompress complete graphics set", &requests)?;
        staged
            .refresh_checksum(options.checksum_field)
            .map_err(PayloadSaveError::from)?;
        let after = staged.rom.logical_bytes().to_vec();
        let reopened = Project::new(
            lm_rom::RomImage::from_bytes(after.clone())
                .map_err(|error| GraphicsIoError::Save(PayloadSaveError::Rom(error)))?,
        );
        for (slot, expected) in files.iter().enumerate() {
            if reopened.load_graphics_file(slot, target_layout)? != *expected {
                return Err(GraphicsIoError::ReopenMismatch { slot });
            }
        }
        let mutation =
            RomMutation::between(source.mapper, &before, &after).map_err(PayloadSaveError::from)?;
        self.apply_mutation_with_kind(
            GRAPHICS_COMPRESSION_MIGRATION_DESCRIPTION,
            &mutation,
            EditKind::GraphicsCompressionMigration {
                source: source.compression,
                target,
            },
        )
        .map_err(PayloadSaveError::from)
        .map_err(GraphicsIoError::from)
    }
}

fn protected_policy(
    layout: GraphicsRomLayout,
    options: &GraphicsMigrationOptions,
) -> Result<AllocationPolicy, GraphicsIoError> {
    let mut policy = options.allocation.clone();
    let pointer_ranges = if let Some(planes) = layout.split_pointer_planes {
        let plane_len = planes
            .entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(planes.stride))
            .and_then(|last| last.checked_add(1))
            .ok_or(PayloadSaveError::PointerRangeOverflow {
                offset: planes.low_offset,
            })?;
        [planes.low_offset, planes.high_offset, planes.bank_offset]
            .into_iter()
            .map(|offset| {
                offset
                    .checked_add(plane_len)
                    .map(|end| ProtectedRange(offset..end))
                    .ok_or(PayloadSaveError::PointerRangeOverflow { offset })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let table_len = layout
            .pointers
            .entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(layout.pointers.stride))
            .and_then(|last| last.checked_add(3))
            .ok_or(PayloadSaveError::PointerRangeOverflow {
                offset: layout.pointers.offset,
            })?;
        let table_end = layout.pointers.offset.checked_add(table_len).ok_or(
            PayloadSaveError::PointerRangeOverflow {
                offset: layout.pointers.offset,
            },
        )?;
        vec![ProtectedRange(layout.pointers.offset..table_end)]
    };
    let checksum_end =
        options
            .checksum_field
            .checked_add(4)
            .ok_or(PayloadSaveError::PointerRangeOverflow {
                offset: options.checksum_field,
            })?;
    policy.protected.extend(pointer_ranges);
    policy
        .protected
        .push(ProtectedRange(options.checksum_field..checksum_end));
    Ok(policy)
}

#[cfg(test)]
#[path = "graphics_migration_tests.rs"]
mod tests;
