//! Lunar Magic's permanent level-access restriction transaction.

use crate::{CopierHeaderEdit, Edit, EditBatch, EditKind, Project, TransactionError};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, parse_at};
use lm_rom::{
    COPIER_HEADER_LEN, Mapper, RomError, SnesChecksum, compute_snes_checksum, pc_to_snes,
};

const PER_SAVE_TEMPLATE: [u8; 32] = [
    0xa6, 0x67, 0xe0, 0x10, 0x90, 0x02, 0x49, 0, 0x85, 0x0a, 0xc8, 0xb7, 0x65, 0xe0, 0x10, 0x90,
    0x02, 0x49, 0, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];
const BULK_SAVE_TEMPLATE: [u8; 32] = [
    0x08, 0xc2, 0x30, 0xa5, 0x8a, 0x49, 0, 0, 0x85, 0x8a, 0x28, 0xc2, 0x10, 0xa0, 0x00, 0x00, 0x6b,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// Revision-specific locations used by Lunar Magic's restriction migration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelAccessRestrictionLayout {
    pub mapper: Mapper,
    pub per_save_hook: usize,
    pub per_save_code: usize,
    pub per_save_completion_marker: usize,
    pub bulk_save_hook: usize,
    pub bulk_save_code: usize,
    pub graphics_pointer_low: usize,
    pub graphics_pointer_high: usize,
    pub graphics_pointer_entries: usize,
    pub graphics_integrity_words: [usize; 2],
    pub protected_pointer_words: [usize; 9],
    pub metadata_compensation_fill: usize,
    pub metadata_compensation_len: usize,
    pub metadata_compensation_byte: usize,
    pub restriction_marker: usize,
    pub restriction_marker_mirror: Option<usize>,
    pub title: usize,
    pub title_mirror: Option<usize>,
    pub version: usize,
    pub version_mirror: Option<usize>,
    pub checksum_field: usize,
    pub exlorom_bulk_save: Option<ExLoRomRestrictionBulkSaveLayout>,
}

/// Descriptor-backed allocation migration performed while Lunar Magic bulk-resaves ExLoROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExLoRomRestrictionBulkSaveLayout {
    pub protected_owner: usize,
    pub auxiliary_owner: usize,
    pub allocation_start: usize,
    pub allocation_end: usize,
    pub protected_pointer: usize,
    pub auxiliary_pointer_low: usize,
    pub auxiliary_pointer_bank: usize,
    pub allocation_cursor: usize,
}

/// Randomized material embedded by one restriction run.
///
/// Keeping this explicit makes the irreversible transaction reproducible in tests while allowing
/// the application boundary to obtain fresh values for a real operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelAccessRestrictionKeys {
    pub per_save_low: u8,
    pub per_save_high: u8,
    pub graphics: u16,
}

#[derive(Debug)]
pub enum LevelAccessRestrictionError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    RequiresInstalledRom,
    AlreadyRestricted,
    TitleTooLong(usize),
    NonAsciiTitle,
    InvalidLayout,
    NoChecksumCompensation,
    Allocation(AllocationError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for LevelAccessRestrictionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot restrict level access: {self:?}")
    }
}

impl std::error::Error for LevelAccessRestrictionError {}

impl From<RomError> for LevelAccessRestrictionError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for LevelAccessRestrictionError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<AllocationError> for LevelAccessRestrictionError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl Project {
    /// Permanently applies Lunar Magic's level-access restriction as one undoable Rust transaction.
    ///
    /// The original tool closes the ROM after this operation and warns that it has no reversal
    /// command. Rust still records one exact undo batch so a failed publication or immediate user
    /// mistake cannot strand the only in-memory copy.
    ///
    /// # Errors
    ///
    /// Rejects mapper/layout disagreement, an uninstalled or already restricted ROM, malformed
    /// title text, missing checksum compensation, stale ranges, or transaction failures.
    pub fn restrict_level_access(
        &mut self,
        title: &str,
        keys: LevelAccessRestrictionKeys,
        layout: LevelAccessRestrictionLayout,
    ) -> Result<bool, LevelAccessRestrictionError> {
        validate(self, title, layout)?;
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = original.clone();

        let per_save_mask = u16::from(keys.per_save_low) | u16::from(keys.per_save_high) << 8;
        if let Some(migration) = layout.exlorom_bulk_save {
            migrate_exlorom_bulk_save_owners(&mut staged, migration, per_save_mask)?;
        } else {
            for offset in layout.protected_pointer_words {
                xor_word(&mut staged, offset, per_save_mask)?;
            }
        }

        let mut per_save = PER_SAVE_TEMPLATE;
        if layout.mapper == Mapper::ExLoRom {
            // The original derives this bank byte from descriptor entry $31 after mapper
            // conversion. The 64-Mbit SMW-US descriptor resolves it to bank $81.
            per_save[3] = 0x81;
            per_save[14] = 0x81;
        }
        per_save[7] = keys.per_save_low;
        per_save[18] = keys.per_save_high;
        copy(&mut staged, layout.per_save_code, &per_save)?;
        copy(
            &mut staged,
            layout.per_save_hook,
            &long_call(layout.mapper, layout.per_save_code)?,
        )?;
        copy(&mut staged, layout.per_save_completion_marker, &[0xfe])?;

        let mut bulk_save = BULK_SAVE_TEMPLATE;
        bulk_save[6..8].copy_from_slice(&keys.graphics.to_le_bytes());
        copy(&mut staged, layout.bulk_save_code, &bulk_save)?;
        copy(
            &mut staged,
            layout.bulk_save_hook,
            &long_call(layout.mapper, layout.bulk_save_code)?,
        )?;

        let [graphics_low, graphics_high] = keys.graphics.to_le_bytes();
        xor_bytes(
            &mut staged,
            layout.graphics_pointer_low,
            layout.graphics_pointer_entries,
            graphics_low,
        )?;
        xor_bytes(
            &mut staged,
            layout.graphics_pointer_high,
            layout.graphics_pointer_entries,
            graphics_high,
        )?;
        for offset in layout.graphics_integrity_words {
            xor_word(&mut staged, offset, keys.graphics)?;
        }

        copy(&mut staged, layout.restriction_marker, b"B")?;
        if let Some(offset) = layout.restriction_marker_mirror {
            copy(&mut staged, offset, b"B")?;
        }
        let mut padded_title = [b' '; 21];
        padded_title[..title.len()].copy_from_slice(title.as_bytes());
        copy(&mut staged, layout.title, &padded_title)?;
        if let Some(offset) = layout.title_mirror {
            copy(&mut staged, offset, &padded_title)?;
        }
        copy(&mut staged, layout.version, &[5])?;
        if let Some(offset) = layout.version_mirror {
            copy(&mut staged, offset, &[5])?;
        }

        copy(
            &mut staged,
            layout.metadata_compensation_fill,
            &vec![0xff; layout.metadata_compensation_len],
        )?;
        copy(&mut staged, layout.metadata_compensation_byte, &[0])?;
        install_checksum_compensation(&mut staged, layout)?;

        let mut replacement_header = self.rom.copier_header_bytes().map(<[u8]>::to_vec);
        if let Some(header) = replacement_header.as_mut() {
            header[COPIER_HEADER_LEN - 1] = 1;
        }
        commit_complete_restriction(self, layout.mapper, &original, &staged, replacement_header)?;
        Ok(true)
    }
}

fn migrate_exlorom_bulk_save_owners(
    staged: &mut [u8],
    layout: ExLoRomRestrictionBulkSaveLayout,
    per_save_mask: u16,
) -> Result<(), LevelAccessRestrictionError> {
    let protected = parse_at(staged, layout.protected_owner)
        .map_err(|_| LevelAccessRestrictionError::InvalidLayout)?;
    let auxiliary = parse_at(staged, layout.auxiliary_owner)
        .map_err(|_| LevelAccessRestrictionError::InvalidLayout)?;
    if protected.payload.len() != 0x21 || auxiliary.payload.len() != 5 {
        return Err(LevelAccessRestrictionError::InvalidLayout);
    }

    let mut protected_payload = staged[protected.payload.clone()].to_vec();
    for relative in [5usize, 8, 11, 14, 17, 20, 23, 26, 29] {
        xor_word(&mut protected_payload, relative, per_save_mask)?;
    }
    let auxiliary_payload = staged[auxiliary.payload.clone()].to_vec();
    let policy = AllocationPolicy::lorom(layout.allocation_start..layout.allocation_end);
    let (new_protected, new_auxiliary) = {
        let mut allocator = FreeSpaceAllocator::new(staged, policy);
        let new_protected = allocator.allocate(&protected_payload)?;
        let new_auxiliary = allocator.allocate(&auxiliary_payload)?;
        (new_protected, new_auxiliary)
    };

    staged[protected.full_range()].fill(0);
    staged[auxiliary.full_range()].fill(0);
    let protected_pointer = pc_to_snes(Mapper::ExLoRom, new_protected.payload.start)?;
    copy(
        staged,
        layout.protected_pointer,
        &protected_pointer.to_le_bytes()[..3],
    )?;
    let auxiliary_pointer = pc_to_snes(Mapper::ExLoRom, new_auxiliary.payload.start)?;
    copy(
        staged,
        layout.auxiliary_pointer_low,
        &auxiliary_pointer.to_le_bytes()[..2],
    )?;
    copy(
        staged,
        layout.auxiliary_pointer_bank,
        &[auxiliary_pointer.to_le_bytes()[2]],
    )?;
    let physical_cursor = u32::try_from(new_auxiliary.full_range().end + COPIER_HEADER_LEN)
        .map_err(|_| LevelAccessRestrictionError::InvalidLayout)?;
    copy(
        staged,
        layout.allocation_cursor,
        &physical_cursor.to_le_bytes()[..3],
    )?;
    Ok(())
}

fn validate(
    project: &Project,
    title: &str,
    layout: LevelAccessRestrictionLayout,
) -> Result<(), LevelAccessRestrictionError> {
    if let Some(identity) = &project.identity
        && identity.mapper != layout.mapper
    {
        return Err(LevelAccessRestrictionError::MapperMismatch {
            expected: identity.mapper,
            actual: layout.mapper,
        });
    }
    if title.len() > 21 {
        return Err(LevelAccessRestrictionError::TitleTooLong(title.len()));
    }
    if !title.is_ascii() {
        return Err(LevelAccessRestrictionError::NonAsciiTitle);
    }
    if project.rom.read(layout.restriction_marker, 1)? == b"B" {
        return Err(LevelAccessRestrictionError::AlreadyRestricted);
    }
    if project.rom.read(layout.per_save_hook, 5)? != [0x85, 0x0a, 0xc8, 0xb7, 0x65]
        || project.rom.read(layout.bulk_save_hook, 5)? != [0xc2, 0x10, 0xa0, 0x00, 0x00]
        || !project
            .rom
            .read(layout.per_save_code, PER_SAVE_TEMPLATE.len())?
            .iter()
            .all(|byte| *byte == 0xff)
        || !project
            .rom
            .read(layout.bulk_save_code, BULK_SAVE_TEMPLATE.len())?
            .iter()
            .all(|byte| *byte == 0xff)
    {
        return Err(LevelAccessRestrictionError::RequiresInstalledRom);
    }
    if layout.graphics_pointer_entries == 0
        || layout.metadata_compensation_len == 0
        || layout.metadata_compensation_fill + layout.metadata_compensation_len
            > project.rom.logical_len()
    {
        return Err(LevelAccessRestrictionError::InvalidLayout);
    }
    Ok(())
}

fn install_checksum_compensation(
    staged: &mut [u8],
    layout: LevelAccessRestrictionLayout,
) -> Result<(), LevelAccessRestrictionError> {
    let stored = SnesChecksum::decode(staged, layout.checksum_field)?;
    for value in 0..=u8::MAX {
        *staged
            .get_mut(layout.metadata_compensation_byte)
            .ok_or(LevelAccessRestrictionError::InvalidLayout)? = value;
        if compute_snes_checksum(staged, layout.checksum_field)? == stored {
            return Ok(());
        }
    }
    let auxiliary_offset = layout
        .metadata_compensation_byte
        .checked_sub(1)
        .filter(|offset| *offset >= layout.metadata_compensation_fill)
        .ok_or(LevelAccessRestrictionError::NoChecksumCompensation)?;
    staged[auxiliary_offset] = 0;
    staged[layout.metadata_compensation_byte] = 0;
    let base = compute_snes_checksum(staged, layout.checksum_field)?.checksum;
    staged[auxiliary_offset] = 1;
    let auxiliary_weight = compute_snes_checksum(staged, layout.checksum_field)?
        .checksum
        .wrapping_sub(base);
    staged[auxiliary_offset] = 0;
    staged[layout.metadata_compensation_byte] = 1;
    let primary_weight = compute_snes_checksum(staged, layout.checksum_field)?
        .checksum
        .wrapping_sub(base);
    staged[layout.metadata_compensation_byte] = 0;
    for auxiliary in 0..=u8::MAX {
        for primary in 0..=u8::MAX {
            if base
                .wrapping_add(auxiliary_weight.wrapping_mul(u16::from(auxiliary)))
                .wrapping_add(primary_weight.wrapping_mul(u16::from(primary)))
                == stored.checksum
            {
                staged[auxiliary_offset] = auxiliary;
                staged[layout.metadata_compensation_byte] = primary;
                return Ok(());
            }
        }
    }
    let compensation_end = layout
        .metadata_compensation_byte
        .checked_add(1)
        .ok_or(LevelAccessRestrictionError::NoChecksumCompensation)?;
    staged[layout.metadata_compensation_fill..compensation_end].fill(0);
    let base = compute_snes_checksum(staged, layout.checksum_field)?.checksum;
    staged[layout.metadata_compensation_fill] = 1;
    let common_weight = compute_snes_checksum(staged, layout.checksum_field)?
        .checksum
        .wrapping_sub(base);
    staged[layout.metadata_compensation_fill] = 0;
    let byte_count = compensation_end - layout.metadata_compensation_fill;
    for total in 0..=byte_count * usize::from(u8::MAX) {
        if base.wrapping_add(common_weight.wrapping_mul(u16::try_from(total).unwrap_or(u16::MAX)))
            == stored.checksum
        {
            let mut remaining = total;
            for byte in &mut staged[layout.metadata_compensation_fill..compensation_end] {
                let value = remaining.min(usize::from(u8::MAX));
                *byte = u8::try_from(value).unwrap_or(u8::MAX);
                remaining -= value;
            }
            return Ok(());
        }
    }
    Err(LevelAccessRestrictionError::NoChecksumCompensation)
}

fn long_call(mapper: Mapper, target: usize) -> Result<[u8; 5], LevelAccessRestrictionError> {
    // Lunar Magic deliberately emits the low-bank LoROM mirror here (`$0D:F100`, not the
    // canonical `$8D:F100` returned by the shared pointer encoder).
    let address = pc_to_snes(mapper, target)?;
    let address = if mapper == Mapper::LoRom {
        address & 0x007f_ffff
    } else {
        address
    };
    let pointer = address.to_le_bytes();
    Ok([0x22, pointer[0], pointer[1], pointer[2], 0xea])
}

fn xor_word(bytes: &mut [u8], offset: usize, mask: u16) -> Result<(), LevelAccessRestrictionError> {
    let end = offset
        .checked_add(2)
        .ok_or(LevelAccessRestrictionError::InvalidLayout)?;
    let source = bytes
        .get(offset..end)
        .ok_or(LevelAccessRestrictionError::InvalidLayout)?;
    let value = u16::from_le_bytes([source[0], source[1]]) ^ mask;
    copy(bytes, offset, &value.to_le_bytes())
}

fn xor_bytes(
    bytes: &mut [u8],
    offset: usize,
    len: usize,
    mask: u8,
) -> Result<(), LevelAccessRestrictionError> {
    let target = bytes
        .get_mut(offset..offset.saturating_add(len))
        .ok_or(LevelAccessRestrictionError::InvalidLayout)?;
    for byte in target {
        *byte ^= mask;
    }
    Ok(())
}

fn copy(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), LevelAccessRestrictionError> {
    let target = bytes
        .get_mut(offset..offset.saturating_add(value.len()))
        .ok_or(LevelAccessRestrictionError::InvalidLayout)?;
    target.copy_from_slice(value);
    Ok(())
}

fn commit_complete_restriction(
    project: &mut Project,
    mapper: Mapper,
    original: &[u8],
    staged: &[u8],
    replacement_header: Option<Vec<u8>>,
) -> Result<(), LevelAccessRestrictionError> {
    let mutation = crate::RomMutation::between(mapper, original, staged)?;
    let description = "restrict level access".to_owned();
    let edits = mutation
        .writes
        .into_iter()
        .map(|write| {
            Ok(Edit {
                offset: write.offset,
                before: project.rom.read(write.offset, write.bytes.len())?.to_vec(),
                after: write.bytes,
                description: description.clone(),
            })
        })
        .collect::<Result<Vec<_>, RomError>>()?;
    let before_header = project.rom.copier_header_bytes().map(<[u8]>::to_vec);
    let copier_header = (before_header != replacement_header).then_some(CopierHeaderEdit {
        before: before_header,
        after: replacement_header,
    });
    let batch = EditBatch {
        description,
        edits,
        kind: EditKind::Ordinary,
        copier_header,
    };
    batch.apply(&mut project.rom)?;
    project.history.push_batch(batch);
    project.synchronize_identity_checksums();
    Ok(())
}
