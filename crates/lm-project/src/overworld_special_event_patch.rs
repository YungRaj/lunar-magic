//! Detection and transactional updates for Lunar Magic's special-event reveal bundle.

use crate::{PatchFixupEncoding, Project, RelocatablePatchPlan};
use lm_overworld::{SpecialEventRevealError, SpecialEventRevealTable};
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpecialEventRevealPatchLocator {
    pub mapper: Mapper,
    pub source_operand: usize,
    pub destination_operand: usize,
    pub direction_operand: usize,
    pub fixed_source: usize,
    pub fixed_destination: usize,
    pub fixed_directions: usize,
    pub full_hook: usize,
    pub secondary_hook: usize,
    pub opcode_patch: usize,
    pub nop_patch: usize,
    pub inline_patch: usize,
    pub pointer_hooks: [usize; 2],
    pub helper_offset: usize,
    pub full_runtime_template: [u8; 64],
    pub pointer_runtime_template: [u8; 48],
    pub helper_template: [u8; 16],
    pub inline_template: [u8; 20],
    /// Mask applied to emitted/validated SNES bank bytes to retain the producer's `LoROM` mirror.
    pub pointer_bank_mask: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialEventRevealStorage {
    Fixed,
    Expanded {
        source: RatsBlock,
        destination: RatsBlock,
        directions: RatsBlock,
        full_runtime: RatsBlock,
        pointer_runtime: RatsBlock,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSpecialEventReveals {
    pub table: SpecialEventRevealTable,
    pub storage: SpecialEventRevealStorage,
}

#[derive(Debug)]
pub enum SpecialEventRevealPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    Rom(RomError),
    MixedStorage,
    UnknownStorage(usize),
    PayloadLength { offset: usize, actual: usize },
    Hook { offset: usize },
    Runtime { offset: usize },
    Table(SpecialEventRevealError),
}

impl std::fmt::Display for SpecialEventRevealPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native special-event reveal detection failed: {self:?}"
        )
    }
}

impl std::error::Error for SpecialEventRevealPatchError {}

impl From<RomError> for SpecialEventRevealPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<SpecialEventRevealError> for SpecialEventRevealPatchError {
    fn from(value: SpecialEventRevealError) -> Self {
        Self::Table(value)
    }
}

impl Project {
    /// Loads the three pristine fixed planes or a fully owned Lunar Magic patch installation.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, mixed storage, inexact RATS ownership, malformed runtime
    /// hooks/fixups, wrong payload lengths, ROM bounds, and lossy table data.
    pub fn load_special_event_reveals_detected(
        &self,
        locator: SpecialEventRevealPatchLocator,
    ) -> Result<LoadedSpecialEventReveals, SpecialEventRevealPatchError> {
        validate_mapper(self, locator.mapper)?;
        let source_offset = read_pointer(self, locator.source_operand, locator.mapper)?;
        let destination_offset = read_pointer(self, locator.destination_operand, locator.mapper)?;
        let direction_offset = read_pointer(self, locator.direction_operand, locator.mapper)?;
        if source_offset == locator.fixed_source
            && destination_offset == locator.fixed_destination
            && direction_offset == locator.fixed_directions
        {
            return Ok(LoadedSpecialEventReveals {
                table: read_table(self, source_offset, destination_offset, direction_offset)?,
                storage: SpecialEventRevealStorage::Fixed,
            });
        }
        let source = exact_block(self, source_offset, SpecialEventRevealTable::WORD_PLANE_LEN)?;
        let destination = exact_block(
            self,
            destination_offset,
            SpecialEventRevealTable::WORD_PLANE_LEN,
        )?;
        let directions = exact_block(self, direction_offset, SpecialEventRevealTable::ENTRY_COUNT)?;
        let full_runtime = runtime_from_hook(
            self,
            locator.full_hook,
            0,
            64,
            locator.mapper,
            locator.pointer_bank_mask,
        )?;
        let pointer_runtime = runtime_from_hook(
            self,
            locator.pointer_hooks[0],
            0,
            48,
            locator.mapper,
            locator.pointer_bank_mask,
        )?;
        validate_full_runtime(self, &locator, &full_runtime, source_offset)?;
        validate_pointer_runtime(self, &locator, &pointer_runtime)?;
        Ok(LoadedSpecialEventReveals {
            table: read_table(self, source_offset, destination_offset, direction_offset)?,
            storage: SpecialEventRevealStorage::Expanded {
                source,
                destination,
                directions,
                full_runtime,
                pointer_runtime,
            },
        })
    }
}

fn read_table(
    project: &Project,
    source: usize,
    destination: usize,
    directions: usize,
) -> Result<SpecialEventRevealTable, SpecialEventRevealPatchError> {
    Ok(SpecialEventRevealTable::decode(
        project
            .rom
            .read(source, SpecialEventRevealTable::WORD_PLANE_LEN)?,
        project
            .rom
            .read(destination, SpecialEventRevealTable::WORD_PLANE_LEN)?,
        project
            .rom
            .read(directions, SpecialEventRevealTable::ENTRY_COUNT)?,
    )?)
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), SpecialEventRevealPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(SpecialEventRevealPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn read_pointer(
    project: &Project,
    offset: usize,
    mapper: Mapper,
) -> Result<usize, SpecialEventRevealPatchError> {
    let bytes = project.rom.read(offset, 3)?;
    let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    Ok(snes_to_pc(mapper, address)?)
}

fn exact_block(
    project: &Project,
    payload_offset: usize,
    expected_len: usize,
) -> Result<RatsBlock, SpecialEventRevealPatchError> {
    let header = payload_offset
        .checked_sub(lm_rats::HEADER_LEN)
        .ok_or(SpecialEventRevealPatchError::UnknownStorage(payload_offset))?;
    let block = parse_at(project.rom.logical_bytes(), header)
        .map_err(|_| SpecialEventRevealPatchError::UnknownStorage(payload_offset))?;
    if block.payload.start != payload_offset {
        return Err(SpecialEventRevealPatchError::UnknownStorage(payload_offset));
    }
    if block.payload.len() != expected_len {
        return Err(SpecialEventRevealPatchError::PayloadLength {
            offset: payload_offset,
            actual: block.payload.len(),
        });
    }
    Ok(block)
}

fn runtime_from_hook(
    project: &Project,
    hook: usize,
    addend: usize,
    len: usize,
    mapper: Mapper,
    bank_mask: u8,
) -> Result<RatsBlock, SpecialEventRevealPatchError> {
    let bytes = project.rom.read(hook, 4)?;
    if bytes[0] != 0x22 {
        return Err(SpecialEventRevealPatchError::Hook { offset: hook });
    }
    let address = u32::from(bytes[1]) | u32::from(bytes[2]) << 8 | u32::from(bytes[3]) << 16;
    let target = snes_to_pc(mapper, address)?;
    let payload = target
        .checked_sub(addend)
        .ok_or(SpecialEventRevealPatchError::Hook { offset: hook })?;
    let expected_target = encode_pointer(mapper, target, bank_mask)?;
    if bytes[1..4] != expected_target {
        return Err(SpecialEventRevealPatchError::Hook { offset: hook });
    }
    exact_block(project, payload, len)
}

fn validate_full_runtime(
    project: &Project,
    locator: &SpecialEventRevealPatchLocator,
    runtime: &RatsBlock,
    source_offset: usize,
) -> Result<(), SpecialEventRevealPatchError> {
    let bytes = project.rom.read(runtime.payload.start, 64)?;
    let expected_self = encode_pointer(
        locator.mapper,
        runtime.payload.start,
        locator.pointer_bank_mask,
    )?;
    let expected_source = encode_pointer(locator.mapper, source_offset, locator.pointer_bank_mask)?;
    for (index, (actual, expected)) in bytes.iter().zip(locator.full_runtime_template).enumerate() {
        let mutable = (0x26..0x29).contains(&index) || (0x2e..0x31).contains(&index);
        if !mutable && *actual != expected {
            return Err(SpecialEventRevealPatchError::Runtime {
                offset: runtime.payload.start + index,
            });
        }
    }
    if bytes[0x26..0x29] != expected_source {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: runtime.payload.start + 0x26,
        });
    }
    if bytes[0x2e..0x31] != expected_self {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: runtime.payload.start + 0x2e,
        });
    }
    validate_hook_target(
        project,
        locator.secondary_hook,
        runtime.payload.start + 0x20,
        true,
        locator.mapper,
        locator.pointer_bank_mask,
    )?;
    if project.rom.read(locator.opcode_patch, 1)? != [0x5d]
        || project.rom.read(locator.nop_patch, 1)? != [0xea]
    {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: locator.opcode_patch,
        });
    }
    let inline = project.rom.read(locator.inline_patch, 20)?;
    let mut expected_inline = locator.inline_template;
    expected_inline[16..19].copy_from_slice(&expected_self);
    if inline != expected_inline {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: locator.inline_patch,
        });
    }
    if project.rom.read(locator.helper_offset, 16)? != locator.helper_template {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: locator.helper_offset,
        });
    }
    Ok(())
}

fn validate_pointer_runtime(
    project: &Project,
    locator: &SpecialEventRevealPatchLocator,
    runtime: &RatsBlock,
) -> Result<(), SpecialEventRevealPatchError> {
    if project.rom.read(runtime.payload.start, 48)? != locator.pointer_runtime_template {
        return Err(SpecialEventRevealPatchError::Runtime {
            offset: runtime.payload.start,
        });
    }
    validate_hook_target(
        project,
        locator.pointer_hooks[1],
        runtime.payload.start + 0x10,
        false,
        locator.mapper,
        locator.pointer_bank_mask,
    )
}

fn validate_hook_target(
    project: &Project,
    hook: usize,
    target: usize,
    trailing_nop: bool,
    mapper: Mapper,
    bank_mask: u8,
) -> Result<(), SpecialEventRevealPatchError> {
    let len = if trailing_nop { 5 } else { 4 };
    let bytes = project.rom.read(hook, len)?;
    let pointer = encode_pointer(mapper, target, bank_mask)?;
    if bytes[0] != 0x22 || bytes[1..4] != pointer || (trailing_nop && bytes.get(4) != Some(&0xea)) {
        return Err(SpecialEventRevealPatchError::Hook { offset: hook });
    }
    Ok(())
}

fn encode_pointer(
    mapper: Mapper,
    offset: usize,
    bank_mask: u8,
) -> Result<[u8; 3], SpecialEventRevealPatchError> {
    let bytes = pc_to_snes(mapper, offset)?.to_le_bytes();
    Ok([bytes[0], bytes[1], bytes[2] & bank_mask])
}

pub(crate) fn assert_special_event_install_plan_shape(
    plan: &RelocatablePatchPlan,
) -> Result<(), SpecialEventRevealPatchError> {
    if plan.payloads.len() != 5
        || plan.payloads[0].bytes.len() != SpecialEventRevealTable::WORD_PLANE_LEN
        || plan.payloads[1].bytes.len() != SpecialEventRevealTable::WORD_PLANE_LEN
        || plan.payloads[2].bytes.len() != SpecialEventRevealTable::ENTRY_COUNT
        || plan.payloads[3].bytes.len() != 64
        || plan.payloads[4].bytes.len() != 48
        || plan.payloads[3].fixups.iter().any(|fixup| {
            !matches!(
                fixup.encoding,
                PatchFixupEncoding::Long24 | PatchFixupEncoding::Long24LowBank
            )
        })
    {
        return Err(SpecialEventRevealPatchError::MixedStorage);
    }
    Ok(())
}
