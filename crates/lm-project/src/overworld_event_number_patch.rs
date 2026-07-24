//! Detection and transactional installation of Lunar Magic's overworld event-number map.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::EventNumberMap;
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, compute_snes_checksum, pc_to_snes, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldEventNumberMapLocator {
    pub mapper: Mapper,
    pub legacy_probe_offset: usize,
    pub legacy_fixed_opcode: u8,
    pub legacy_pairs_offset: usize,
    pub legacy_pairs_len: usize,
    pub hook_offset: usize,
    pub pristine_hook: [u8; 4],
    pub runtime_offset: usize,
    pub runtime_template: [u8; 32],
    pub runtime_pointer_operand: usize,
    pub fixed_map_offset: usize,
    pub extended_map_offset: usize,
    /// Mask applied to the encoded SNES bank byte, preserving a producer's `LoROM` mirror choice.
    pub pointer_bank_mask: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEventNumberMapStorage {
    LegacyPairs,
    LegacyFixed,
    InstalledFixed,
    InstalledExtended,
    InstalledTagged(RatsBlock),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedOverworldEventNumberMap {
    pub map: EventNumberMap,
    pub storage: OverworldEventNumberMapStorage,
}

#[derive(Debug)]
pub enum OverworldEventNumberMapError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    Rom(RomError),
    LegacyPairs(usize),
    Hook([u8; 4]),
    Runtime,
    RuntimeReservation,
    Pointer(RomError),
    UnknownStorage(usize),
    TaggedLength(usize),
    MapLength(usize),
    Transaction(TransactionError),
    SemanticReopen,
}

impl std::fmt::Display for OverworldEventNumberMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld event-number map operation failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldEventNumberMapError {}

impl From<RomError> for OverworldEventNumberMapError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldEventNumberMapError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads the pristine pair/fixed representation or the version-1.10 installed runtime.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, malformed hooks/runtime bytes, unowned relocated pointers,
    /// excessive payloads, and ROM bounds.
    pub fn load_overworld_event_number_map_detected(
        &self,
        locator: OverworldEventNumberMapLocator,
    ) -> Result<LoadedOverworldEventNumberMap, OverworldEventNumberMapError> {
        validate_mapper(self, locator.mapper)?;
        let runtime_installed = runtime_is_installed(self, locator)?;
        let hook: [u8; 4] = self
            .rom
            .read(locator.hook_offset, 4)?
            .try_into()
            .map_err(|_| range_error(locator.hook_offset, 4, self.rom.logical_len()))?;
        let mut installed_hook = [0x22, 0, 0, 0];
        installed_hook[1..].copy_from_slice(&encode_pointer(
            locator.mapper,
            locator.runtime_offset,
            locator.pointer_bank_mask,
        )?);
        if runtime_installed || hook == installed_hook {
            if !runtime_installed {
                return Err(OverworldEventNumberMapError::Runtime);
            }
            return load_installed(self, locator);
        }
        if hook != locator.pristine_hook {
            return Err(OverworldEventNumberMapError::Hook(hook));
        }
        let probe = self.rom.read(locator.legacy_probe_offset, 1)?[0];
        if probe == locator.legacy_fixed_opcode {
            let bytes = self
                .rom
                .read(locator.fixed_map_offset, EventNumberMap::VANILLA_LEN)?;
            return Ok(LoadedOverworldEventNumberMap {
                map: EventNumberMap::decode(bytes)
                    .map_err(OverworldEventNumberMapError::MapLength)?,
                storage: OverworldEventNumberMapStorage::LegacyFixed,
            });
        }
        let pairs = self
            .rom
            .read(locator.legacy_pairs_offset, locator.legacy_pairs_len)?;
        Ok(LoadedOverworldEventNumberMap {
            map: EventNumberMap::decode_legacy_pairs(pairs)
                .map_err(OverworldEventNumberMapError::LegacyPairs)?,
            storage: OverworldEventNumberMapStorage::LegacyPairs,
        })
    }

    /// Installs or updates Lunar Magic's recovered version-1.10 mapping runtime and repairs the
    /// checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects malformed current state, unknown pristine hook/runtime bytes, invalid map lengths,
    /// mapper or ROM bounds, and any staged image that cannot semantically reopen.
    pub fn save_overworld_event_number_map_detected(
        &mut self,
        map: &EventNumberMap,
        locator: OverworldEventNumberMapLocator,
        checksum_field: usize,
    ) -> Result<bool, OverworldEventNumberMapError> {
        validate_mapper(self, locator.mapper)?;
        let loaded = self.load_overworld_event_number_map_detected(locator)?;
        if loaded.map == *map {
            return Ok(false);
        }
        let stored_len = map.stored_len();
        if !(EventNumberMap::VANILLA_LEN..=EventNumberMap::ENTRY_COUNT).contains(&stored_len) {
            return Err(OverworldEventNumberMapError::MapLength(stored_len));
        }
        let (table_offset, table_len) = if stored_len <= EventNumberMap::VANILLA_LEN {
            (locator.fixed_map_offset, EventNumberMap::VANILLA_LEN)
        } else {
            (locator.extended_map_offset, EventNumberMap::ENTRY_COUNT)
        };
        let mut table = vec![0; table_len];
        table[..stored_len].copy_from_slice(map.encode());
        let pointer = encode_pointer(locator.mapper, table_offset, locator.pointer_bank_mask)?;
        let mut writes = vec![RomWrite {
            offset: table_offset,
            bytes: table,
        }];
        if matches!(
            loaded.storage,
            OverworldEventNumberMapStorage::LegacyPairs
                | OverworldEventNumberMapStorage::LegacyFixed
        ) {
            let hook: [u8; 4] = self
                .rom
                .read(locator.hook_offset, 4)?
                .try_into()
                .map_err(|_| range_error(locator.hook_offset, 4, self.rom.logical_len()))?;
            if hook != locator.pristine_hook {
                return Err(OverworldEventNumberMapError::Hook(hook));
            }
            if self
                .rom
                .read(locator.runtime_offset, locator.runtime_template.len())?
                .iter()
                .any(|byte| *byte != 0xff)
            {
                return Err(OverworldEventNumberMapError::RuntimeReservation);
            }
            let mut runtime = locator.runtime_template;
            runtime[locator.runtime_pointer_operand..locator.runtime_pointer_operand + 3]
                .copy_from_slice(&pointer);
            let mut installed_hook = [0x22, 0, 0, 0];
            installed_hook[1..].copy_from_slice(&encode_pointer(
                locator.mapper,
                locator.runtime_offset,
                locator.pointer_bank_mask,
            )?);
            writes.push(RomWrite {
                offset: locator.runtime_offset,
                bytes: runtime.to_vec(),
            });
            writes.push(RomWrite {
                offset: locator.hook_offset,
                bytes: installed_hook.to_vec(),
            });
        } else {
            writes.push(RomWrite {
                offset: locator.runtime_offset + locator.runtime_pointer_operand,
                bytes: pointer.to_vec(),
            });
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        staged.write(checksum_field, &checksum.encoded())?;
        let reopened = Project::new(staged)
            .load_overworld_event_number_map_detected(locator)
            .map_err(|_| OverworldEventNumberMapError::SemanticReopen)?;
        if reopened.map != *map {
            return Err(OverworldEventNumberMapError::SemanticReopen);
        }
        Ok(self.apply_writes("save native overworld event-number map", &writes)?)
    }
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), OverworldEventNumberMapError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(OverworldEventNumberMapError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn runtime_is_installed(
    project: &Project,
    locator: OverworldEventNumberMapLocator,
) -> Result<bool, OverworldEventNumberMapError> {
    let runtime = project
        .rom
        .read(locator.runtime_offset, locator.runtime_template.len())?;
    let marker = &locator.runtime_template[28..32];
    if &runtime[28..32] != marker {
        return Ok(false);
    }
    for (index, (actual, expected)) in runtime.iter().zip(locator.runtime_template).enumerate() {
        if (locator.runtime_pointer_operand..locator.runtime_pointer_operand + 3).contains(&index) {
            continue;
        }
        if *actual != expected {
            return Err(OverworldEventNumberMapError::Runtime);
        }
    }
    Ok(true)
}

fn load_installed(
    project: &Project,
    locator: OverworldEventNumberMapLocator,
) -> Result<LoadedOverworldEventNumberMap, OverworldEventNumberMapError> {
    let hook: [u8; 4] = project
        .rom
        .read(locator.hook_offset, 4)?
        .try_into()
        .map_err(|_| range_error(locator.hook_offset, 4, project.rom.logical_len()))?;
    let expected_target = encode_pointer(
        locator.mapper,
        locator.runtime_offset,
        locator.pointer_bank_mask,
    )?;
    if hook[0] != 0x22 || hook[1..] != expected_target {
        return Err(OverworldEventNumberMapError::Hook(hook));
    }
    let operand = project
        .rom
        .read(locator.runtime_offset + locator.runtime_pointer_operand, 3)?;
    let address = u32::from(operand[0]) | u32::from(operand[1]) << 8 | u32::from(operand[2]) << 16;
    let offset =
        snes_to_pc(locator.mapper, address).map_err(OverworldEventNumberMapError::Pointer)?;
    let (length, storage) = if offset == locator.fixed_map_offset {
        (
            EventNumberMap::VANILLA_LEN,
            OverworldEventNumberMapStorage::InstalledFixed,
        )
    } else if offset == locator.extended_map_offset {
        (
            EventNumberMap::ENTRY_COUNT,
            OverworldEventNumberMapStorage::InstalledExtended,
        )
    } else {
        let block = exact_tagged_block(project.rom.logical_bytes(), offset)
            .ok_or(OverworldEventNumberMapError::UnknownStorage(offset))?;
        let length = block.payload.len();
        if length == 0 || length > EventNumberMap::ENTRY_COUNT {
            return Err(OverworldEventNumberMapError::TaggedLength(length));
        }
        (
            length,
            OverworldEventNumberMapStorage::InstalledTagged(block),
        )
    };
    let map = EventNumberMap::decode(project.rom.read(offset, length)?)
        .map_err(OverworldEventNumberMapError::MapLength)?;
    Ok(LoadedOverworldEventNumberMap { map, storage })
}

fn exact_tagged_block(bytes: &[u8], payload_offset: usize) -> Option<RatsBlock> {
    let header = payload_offset.checked_sub(lm_rats::HEADER_LEN)?;
    parse_at(bytes, header)
        .ok()
        .filter(|block| block.payload.start == payload_offset)
}

fn encode_pointer(
    mapper: Mapper,
    offset: usize,
    bank_mask: u8,
) -> Result<[u8; 3], OverworldEventNumberMapError> {
    let address = pc_to_snes(mapper, offset).map_err(OverworldEventNumberMapError::Pointer)?;
    let mut bank = address.to_le_bytes()[2];
    bank &= bank_mask;
    Ok([address.to_le_bytes()[0], address.to_le_bytes()[1], bank])
}

fn range_error(offset: usize, len: usize, image_len: usize) -> RomError {
    RomError::RangeOutOfBounds {
        offset,
        len,
        image_len,
    }
}
