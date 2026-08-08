//! Lunar Magic expanded secondary-exit six-plane persistence.

use crate::{Project, payload::staging::commit_staged};
use lm_level::{SecondaryExitEncodingError, SecondaryExitTable};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, compute_snes_checksum, pc_to_snes, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecondaryExitPatchLocator {
    pub mapper: Mapper,
    pub first_reader: usize,
    pub second_reader: usize,
    pub fixed_planes: [usize; 4],
}

impl SecondaryExitPatchLocator {
    const FIRST_LEN: usize = 21;
    const SECOND_LEN: usize = 15;

    fn pointer_offsets(self) -> [usize; 6] {
        [
            self.first_reader + 1,
            self.first_reader + 8,
            self.first_reader + 15,
            self.second_reader + 1,
            self.second_reader + 6,
            self.second_reader + 11,
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecondaryExitStorage {
    Pristine,
    Installed {
        fixed_prefix_planes: usize,
        used_len: usize,
        tagged_planes: Vec<RatsBlock>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSecondaryExitTable {
    pub table: SecondaryExitTable,
    pub storage: SecondaryExitStorage,
}

#[derive(Debug)]
pub enum SecondaryExitPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    ReaderSignature,
    MixedStorage,
    FixedPointerMismatch { plane: usize },
    MissingOwnership { plane: usize },
    PlaneLength { plane: usize, len: usize },
    PlaneLengthsDisagree,
    InstallationRequired,
    Rom(RomError),
    Table(SecondaryExitEncodingError),
    Allocation(AllocationError),
    Commit(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for SecondaryExitPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "secondary-exit patch failed: {self:?}")
    }
}

impl std::error::Error for SecondaryExitPatchError {}

impl From<RomError> for SecondaryExitPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<SecondaryExitEncodingError> for SecondaryExitPatchError {
    fn from(value: SecondaryExitEncodingError) -> Self {
        Self::Table(value)
    }
}

impl From<AllocationError> for SecondaryExitPatchError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<crate::PayloadSaveError> for SecondaryExitPatchError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Loads pristine SMW's four 0x200-byte planes or Lunar Magic's current six-plane readers.
    ///
    /// Installed variable planes must have exact RATS ownership. Both Lunar Magic's compact
    /// fixed-prefix form and its all-tagged form are accepted.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, altered reader code, invalid or unowned pointers, inconsistent
    /// plane lengths, out-of-bounds fixed tables, and malformed packed records.
    pub fn load_secondary_exit_table_detected(
        &self,
        locator: SecondaryExitPatchLocator,
    ) -> Result<LoadedSecondaryExitTable, SecondaryExitPatchError> {
        validate_mapper(self, locator.mapper)?;
        let first = self
            .rom
            .read(locator.first_reader, SecondaryExitPatchLocator::FIRST_LEN)?;
        let second = self
            .rom
            .read(locator.second_reader, SecondaryExitPatchLocator::SECOND_LEN)?;
        if first.iter().all(|byte| *byte == 0xff) && second.iter().all(|byte| *byte == 0xff) {
            return load_pristine(self, locator);
        }
        validate_readers(first, second)?;
        load_installed(self, locator)
    }

    /// Replaces an installed table and reclaims only its proven tagged plane owners.
    ///
    /// Pristine installation is provided by the revision-profile installation plan; this method
    /// deliberately remains the lower-level owned-update path.
    ///
    /// # Errors
    ///
    /// Rejects pristine or foreign storage, unrepresentable records, stale ownership, invalid
    /// allocation/checksum state, and semantic disagreement after reopening the staged result.
    pub fn save_installed_secondary_exit_table(
        &mut self,
        table: &SecondaryExitTable,
        locator: SecondaryExitPatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        erase_fill: u8,
    ) -> Result<bool, SecondaryExitPatchError> {
        let loaded = self.load_secondary_exit_table_detected(locator)?;
        if &loaded.table == table {
            return Ok(false);
        }
        let SecondaryExitStorage::Installed { tagged_planes, .. } = loaded.storage else {
            return Err(SecondaryExitPatchError::InstallationRequired);
        };
        let encoded = table.encode()?;
        let used_len = used_plane_len(&encoded).max(1);
        let fixed_prefix = usize::from(used_len <= 0x200) * 4;
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = original.clone();
        {
            let mut allocator = FreeSpaceAllocator::new(&mut staged, allocation.clone());
            for block in &tagged_planes {
                allocator.erase(block, erase_fill)?;
            }
        }
        let mut blocks = Vec::with_capacity(SecondaryExitTable::PLANE_COUNT - fixed_prefix);
        for plane in 0..SecondaryExitTable::PLANE_COUNT {
            let plane_start = plane * SecondaryExitTable::ENTRY_COUNT;
            if plane < fixed_prefix {
                checked_copy(
                    &mut staged,
                    locator.fixed_planes[plane],
                    &encoded[plane_start..plane_start + 0x200],
                )?;
            } else {
                let payload = &encoded[plane_start..plane_start + used_len];
                let block =
                    FreeSpaceAllocator::new(&mut staged, allocation.clone()).allocate(payload)?;
                blocks.push(block);
            }
        }
        for (plane, offset) in locator.pointer_offsets().into_iter().enumerate() {
            let payload = if plane < fixed_prefix {
                locator.fixed_planes[plane]
            } else {
                blocks[plane - fixed_prefix].payload.start
            };
            write_low_bank_pointer(&mut staged, offset, locator.mapper, payload)?;
        }
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        checked_copy(&mut staged, checksum_field, &checksum.encoded())?;
        commit_staged(
            self,
            "replace native secondary-exit table".into(),
            &original,
            &staged,
        )?;
        if &self.load_secondary_exit_table_detected(locator)?.table != table {
            return Err(SecondaryExitPatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn load_pristine(
    project: &Project,
    locator: SecondaryExitPatchLocator,
) -> Result<LoadedSecondaryExitTable, SecondaryExitPatchError> {
    let mut encoded = vec![0; SecondaryExitTable::ENTRY_COUNT * SecondaryExitTable::PLANE_COUNT];
    for (plane, offset) in locator.fixed_planes.into_iter().enumerate() {
        let start = plane * SecondaryExitTable::ENTRY_COUNT;
        encoded[start..start + 0x200].copy_from_slice(project.rom.read(offset, 0x200)?);
    }
    Ok(LoadedSecondaryExitTable {
        table: SecondaryExitTable::decode(&encoded)
            .map_err(|len| SecondaryExitPatchError::PlaneLength { plane: 0, len })?,
        storage: SecondaryExitStorage::Pristine,
    })
}

fn load_installed(
    project: &Project,
    locator: SecondaryExitPatchLocator,
) -> Result<LoadedSecondaryExitTable, SecondaryExitPatchError> {
    let mut payloads = Vec::with_capacity(SecondaryExitTable::PLANE_COUNT);
    let mut blocks = Vec::new();
    let mut fixed_prefix = 0;
    let mut tagged_len = None;
    for (plane, offset) in locator.pointer_offsets().into_iter().enumerate() {
        let pc = read_pointer(project.rom.logical_bytes(), offset, locator.mapper)?;
        if plane < 4 && pc == locator.fixed_planes[plane] {
            if !blocks.is_empty() || fixed_prefix != plane {
                return Err(SecondaryExitPatchError::MixedStorage);
            }
            fixed_prefix += 1;
            payloads.push(project.rom.read(pc, 0x200)?.to_vec());
            continue;
        }
        if fixed_prefix != 0 && fixed_prefix != 4 {
            return Err(SecondaryExitPatchError::FixedPointerMismatch { plane });
        }
        let header = pc
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(SecondaryExitPatchError::MissingOwnership { plane })?;
        let block = parse_at(project.rom.logical_bytes(), header)
            .map_err(|_| SecondaryExitPatchError::MissingOwnership { plane })?;
        if block.payload.start != pc {
            return Err(SecondaryExitPatchError::MissingOwnership { plane });
        }
        let len = block.payload.len();
        if len == 0 || len > SecondaryExitTable::ENTRY_COUNT {
            return Err(SecondaryExitPatchError::PlaneLength { plane, len });
        }
        if tagged_len.is_some_and(|previous| previous != len) {
            return Err(SecondaryExitPatchError::PlaneLengthsDisagree);
        }
        tagged_len = Some(len);
        payloads.push(project.rom.logical_bytes()[block.payload.clone()].to_vec());
        blocks.push(block);
    }
    if fixed_prefix != 0 && fixed_prefix != 4 {
        return Err(SecondaryExitPatchError::MixedStorage);
    }
    let used_len = tagged_len.ok_or(SecondaryExitPatchError::MixedStorage)?;
    if fixed_prefix == 4 && used_len > 0x200 {
        return Err(SecondaryExitPatchError::PlaneLengthsDisagree);
    }
    let mut encoded = vec![0; SecondaryExitTable::ENTRY_COUNT * SecondaryExitTable::PLANE_COUNT];
    for (plane, payload) in payloads.iter().enumerate() {
        let start = plane * SecondaryExitTable::ENTRY_COUNT;
        encoded[start..start + payload.len()].copy_from_slice(payload);
    }
    Ok(LoadedSecondaryExitTable {
        table: SecondaryExitTable::decode(&encoded)
            .map_err(|len| SecondaryExitPatchError::PlaneLength { plane: 0, len })?,
        storage: SecondaryExitStorage::Installed {
            fixed_prefix_planes: fixed_prefix,
            used_len,
            tagged_planes: blocks,
        },
    })
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), SecondaryExitPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(SecondaryExitPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn validate_readers(first: &[u8], second: &[u8]) -> Result<(), SecondaryExitPatchError> {
    let first_template = [
        0xbf, 0, 0, 0, 0x85, 0x0e, 0x6b, 0xbf, 0, 0, 0, 0x85, 0x00, 0x6b, 0xbf, 0, 0, 0, 0x85,
        0x01, 0x6b,
    ];
    let second_template = [
        0xbf, 0, 0, 0, 0x6b, 0xbf, 0, 0, 0, 0x6b, 0xbf, 0, 0, 0, 0x6b,
    ];
    if !fixed_bytes_match(first, &first_template, &[1..4, 8..11, 15..18])
        || !fixed_bytes_match(second, &second_template, &[1..4, 6..9, 11..14])
    {
        return Err(SecondaryExitPatchError::ReaderSignature);
    }
    Ok(())
}

fn fixed_bytes_match(actual: &[u8], expected: &[u8], mutable: &[std::ops::Range<usize>]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .enumerate()
            .all(|(index, pair)| {
                mutable.iter().any(|range| range.contains(&index)) || pair.0 == pair.1
            })
}

fn used_plane_len(encoded: &[u8]) -> usize {
    (0..SecondaryExitTable::ENTRY_COUNT)
        .rev()
        .find(|index| {
            (0..SecondaryExitTable::PLANE_COUNT)
                .any(|plane| encoded[plane * SecondaryExitTable::ENTRY_COUNT + index] != 0)
        })
        .map_or(0, |index| index + 1)
}

fn read_pointer(bytes: &[u8], offset: usize, mapper: Mapper) -> Result<usize, RomError> {
    let raw = bytes
        .get(offset..offset + 3)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: 3,
            image_len: bytes.len(),
        })?;
    snes_to_pc(
        mapper,
        u32::from(raw[0]) | u32::from(raw[1]) << 8 | u32::from(raw[2]) << 16,
    )
}

fn write_low_bank_pointer(
    bytes: &mut [u8],
    offset: usize,
    mapper: Mapper,
    pc: usize,
) -> Result<(), RomError> {
    let mut pointer = pc_to_snes(mapper, pc)?.to_le_bytes();
    // Lunar Magic uses the low-bank mirror for LoROM. Bit 23 is not a mirror on ExLoROM (it
    // selects the lower versus upper 4 MiB half) or SA-1 (it participates in the mapped bank
    // range), so clearing it there redirects an otherwise valid freshly allocated plane.
    if mapper == Mapper::LoRom {
        pointer[2] &= 0x7f;
    }
    checked_copy(bytes, offset, &pointer[..3])
}

fn checked_copy(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), RomError> {
    let image_len = bytes.len();
    let target = bytes
        .get_mut(offset..offset.saturating_add(value.len()))
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: value.len(),
            image_len,
        })?;
    target.copy_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_plane_pointers_round_trip_without_losing_mapper_significant_banks() {
        for (mapper, pc) in [
            (Mapper::LoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x42_0000),
            (Mapper::Sa1, 0x2_0000),
            (Mapper::Sa1, 0x42_0000),
        ] {
            let mut bytes = [0_u8; 3];
            write_low_bank_pointer(&mut bytes, 0, mapper, pc).unwrap();
            assert_eq!(read_pointer(&bytes, 0, mapper).unwrap(), pc);
        }
    }
}
