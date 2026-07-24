use super::{Project, RatsOwnershipManifest, RatsReclamationError, RatsReclamationPlan};
use crate::RomWrite;
use lm_rom::{RomError, SnesChecksum, compute_snes_checksum};

impl Project {
    /// Reclaims proven-owned blocks and repairs the SNES checksum in one undoable operation.
    ///
    /// The checksum field may not intersect a reclaimed block. All erase writes are first applied
    /// to a private image so checksum or range failure cannot alter the project.
    ///
    /// # Errors
    ///
    /// Returns [`RatsReclamationError`] for invalid ownership proof, checksum overlap or bounds,
    /// ROM write failure, or transaction failure.
    pub fn reclaim_owned_rats_with_checksum(
        &mut self,
        description: impl Into<String>,
        manifest: &RatsOwnershipManifest,
        fill: u8,
        checksum_field: usize,
    ) -> Result<(RatsReclamationPlan, SnesChecksum), RatsReclamationError> {
        let plan = self.plan_rats_reclamation(manifest, fill)?;
        let checksum_end = checksum_field
            .checked_add(SnesChecksum::ENCODED_LEN)
            .ok_or(RomError::RangeOutOfBounds {
                offset: checksum_field,
                len: SnesChecksum::ENCODED_LEN,
                image_len: self.rom.logical_len(),
            })?;
        for (block, reclaimed) in plan.reclaimed.iter().enumerate() {
            let range = reclaimed.full_range();
            if range.start < checksum_end && checksum_field < range.end {
                return Err(RatsReclamationError::ChecksumFieldOverlap { block });
            }
        }
        let internal_header_start =
            checksum_field
                .checked_sub(0x1c)
                .ok_or(RomError::RangeOutOfBounds {
                    offset: checksum_field,
                    len: SnesChecksum::ENCODED_LEN,
                    image_len: self.rom.logical_len(),
                })?;
        let internal_header_end =
            internal_header_start
                .checked_add(0x40)
                .ok_or(RomError::RangeOutOfBounds {
                    offset: internal_header_start,
                    len: 0x40,
                    image_len: self.rom.logical_len(),
                })?;
        for (block, reclaimed) in plan.reclaimed.iter().enumerate() {
            let range = reclaimed.full_range();
            if range.start < internal_header_end && internal_header_start < range.end {
                return Err(RatsReclamationError::InternalHeaderOverlap { block });
            }
        }

        let mut staged = self.rom.clone();
        for write in &plan.writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        let mut writes = plan.writes.clone();
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        self.apply_writes(description, &writes)?;
        Ok((plan, checksum))
    }
}
