//! Lunar Magic title-screen playback runtime and movement-data persistence.

use crate::{Project, payload::staging::commit_staged};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};
use lm_title::{TitleScreenRecording, TitleScreenRecordingError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleRecordingExpansionWrite {
    pub offset: usize,
    pub bytes: &'static [u8],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleRecordingPatchLocator {
    pub mapper: Mapper,
    pub hook: usize,
    pub pristine_hook: [u8; Self::HOOK_LEN],
    pub hook_template: [u8; Self::HOOK_LEN],
    pub runtime_template: [u8; Self::RUNTIME_LEN],
    /// Optional internal-header ROM-size byte updated when allocation expands the image.
    pub rom_size_field: Option<usize>,
    /// Fixed bytes initialized by Lunar Magic's confirmed vanilla-ROM expansion path.
    pub expansion_writes: &'static [TitleRecordingExpansionWrite],
    /// Optional Lunar Magic checksum-compensation run used to preserve the stored checksum.
    pub checksum_compensation: Option<std::ops::Range<usize>>,
}

impl TitleRecordingPatchLocator {
    pub const HOOK_LEN: usize = 0x11;
    pub const RUNTIME_LEN: usize = 0x60;
    pub const HOOK_RUNTIME_POINTER: usize = 1;
    pub const TIMER_POINTER: usize = 0x23;
    pub const FIRST_INPUT_POINTER: usize = 0x36;
    pub const SECOND_INPUT_POINTER: usize = 0x44;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TitleRecordingStorage {
    Absent,
    Installed {
        runtime: RatsBlock,
        recording: RatsBlock,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedTitleRecording {
    pub recording: Option<TitleScreenRecording>,
    pub storage: TitleRecordingStorage,
}

#[derive(Debug)]
pub enum TitleRecordingPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    HookSignature,
    RuntimeSignature,
    MissingRuntimeOwnership,
    RuntimeLength(usize),
    MissingRecordingOwnership,
    RecordingPointersDisagree,
    Rom(RomError),
    Recording(TitleScreenRecordingError),
    Allocation(AllocationError),
    Commit(crate::PayloadSaveError),
    CompensationOverflow { required: usize, available: usize },
    CompensationMismatch { expected: u16, actual: u16 },
    ReopenMismatch,
}

impl std::fmt::Display for TitleRecordingPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "title-screen recording patch failed: {self:?}")
    }
}

impl std::error::Error for TitleRecordingPatchError {}

impl From<RomError> for TitleRecordingPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TitleScreenRecordingError> for TitleRecordingPatchError {
    fn from(value: TitleScreenRecordingError) -> Self {
        Self::Recording(value)
    }
}

impl From<AllocationError> for TitleRecordingPatchError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<crate::PayloadSaveError> for TitleRecordingPatchError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Detects pristine SMW or Lunar Magic's exact playback runtime and owned recording.
    ///
    /// # Errors
    ///
    /// Rejects altered hooks, unowned runtimes/data, fixed-code disagreement, malformed movement
    /// payloads, and disagreement among the runtime's three biased data pointers.
    pub fn load_title_recording_detected(
        &self,
        locator: &TitleRecordingPatchLocator,
    ) -> Result<LoadedTitleRecording, TitleRecordingPatchError> {
        validate_mapper(self, locator.mapper)?;
        let hook = self
            .rom
            .read(locator.hook, TitleRecordingPatchLocator::HOOK_LEN)?;
        if hook[0] != 0x22 {
            if hook != locator.pristine_hook {
                return Err(TitleRecordingPatchError::HookSignature);
            }
            return Ok(LoadedTitleRecording {
                recording: None,
                storage: TitleRecordingStorage::Absent,
            });
        }
        let hook_pointer = TitleRecordingPatchLocator::HOOK_RUNTIME_POINTER..4;
        validate_fixed_bytes(
            hook,
            &locator.hook_template,
            std::slice::from_ref(&hook_pointer),
        )
        .map_err(|()| TitleRecordingPatchError::HookSignature)?;
        let runtime_start = read_pointer(
            self.rom.logical_bytes(),
            locator.hook + TitleRecordingPatchLocator::HOOK_RUNTIME_POINTER,
            locator.mapper,
        )?;
        let runtime = owned_block(
            self.rom.logical_bytes(),
            runtime_start,
            OwnershipKind::Runtime,
        )?;
        if runtime.payload.len() != TitleRecordingPatchLocator::RUNTIME_LEN {
            return Err(TitleRecordingPatchError::RuntimeLength(
                runtime.payload.len(),
            ));
        }
        let runtime_bytes = &self.rom.logical_bytes()[runtime.payload.clone()];
        validate_fixed_bytes(
            runtime_bytes,
            &locator.runtime_template,
            &[
                TitleRecordingPatchLocator::TIMER_POINTER
                    ..TitleRecordingPatchLocator::TIMER_POINTER + 3,
                TitleRecordingPatchLocator::FIRST_INPUT_POINTER
                    ..TitleRecordingPatchLocator::FIRST_INPUT_POINTER + 3,
                TitleRecordingPatchLocator::SECOND_INPUT_POINTER
                    ..TitleRecordingPatchLocator::SECOND_INPUT_POINTER + 3,
            ],
        )
        .map_err(|()| TitleRecordingPatchError::RuntimeSignature)?;
        let timer = read_pointer(
            self.rom.logical_bytes(),
            runtime.payload.start + TitleRecordingPatchLocator::TIMER_POINTER,
            locator.mapper,
        )?;
        let recording_start = timer
            .checked_sub(2)
            .ok_or(TitleRecordingPatchError::MissingRecordingOwnership)?;
        for (operand, bias) in [
            (TitleRecordingPatchLocator::FIRST_INPUT_POINTER, 3),
            (TitleRecordingPatchLocator::SECOND_INPUT_POINTER, 2),
        ] {
            let observed = read_pointer(
                self.rom.logical_bytes(),
                runtime.payload.start + operand,
                locator.mapper,
            )?;
            if observed.checked_add(bias) != Some(recording_start) {
                return Err(TitleRecordingPatchError::RecordingPointersDisagree);
            }
        }
        let recording = owned_block(
            self.rom.logical_bytes(),
            recording_start,
            OwnershipKind::Recording,
        )?;
        let value = TitleScreenRecording::from_bytes(
            self.rom.logical_bytes()[recording.payload.clone()].to_vec(),
        )?;
        Ok(LoadedTitleRecording {
            recording: Some(value),
            storage: TitleRecordingStorage::Installed { runtime, recording },
        })
    }

    /// Installs or replaces title-screen playback as one checksum-valid undo operation.
    ///
    /// # Errors
    ///
    /// Rejects foreign current state, unsafe allocation, mapping failures, stale ownership, and
    /// semantic disagreement after reopen. Failure leaves both ROM and history unchanged.
    pub fn save_title_recording_detected(
        &mut self,
        recording: &TitleScreenRecording,
        locator: &TitleRecordingPatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, TitleRecordingPatchError> {
        let loaded = self.load_title_recording_detected(locator)?;
        if loaded.recording.as_ref() == Some(recording) {
            return Ok(false);
        }
        stage_recording(
            self,
            recording,
            locator,
            &loaded.storage,
            allocation,
            checksum_field,
            fill,
        )?;
        if self
            .load_title_recording_detected(locator)?
            .recording
            .as_ref()
            != Some(recording)
        {
            return Err(TitleRecordingPatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn stage_recording(
    project: &mut Project,
    recording: &TitleScreenRecording,
    locator: &TitleRecordingPatchLocator,
    storage: &TitleRecordingStorage,
    allocation: &AllocationPolicy,
    checksum_field: usize,
    fill: u8,
) -> Result<(), TitleRecordingPatchError> {
    let original = project.rom.logical_bytes().to_vec();
    let mut image = RomImage::from_bytes(original.clone())?;
    let mut staged = image.logical_bytes().to_vec();
    let mut policy = allocation.clone();
    policy.protected.extend([
        ProtectedRange(locator.hook..locator.hook + TitleRecordingPatchLocator::HOOK_LEN),
        ProtectedRange(checksum_field..checksum_field + 4),
    ]);
    if let Some(compensation) = &locator.checksum_compensation {
        policy.protected.push(ProtectedRange(compensation.clone()));
    }
    let requested_end = policy.search.end;
    let initial_end = requested_end.min(staged.len());
    let initial_attempt = if policy.search.start < initial_end {
        let mut bounded = policy.clone();
        bounded.search.end = initial_end;
        let mut candidate = staged.clone();
        match allocate_title_blocks(&mut candidate, locator, storage, &bounded, recording, fill) {
            Ok(blocks) => Some(Ok((candidate, blocks))),
            Err(TitleRecordingPatchError::Allocation(AllocationError::NoSpace { .. })) => None,
            Err(error) => Some(Err(error)),
        }
    } else {
        None
    };
    let (mut staged, (runtime, recording_block)) = match initial_attempt {
        Some(result) => result?,
        None if requested_end > staged.len() => {
            let expansion_fill = *policy
                .fill_bytes
                .first()
                .ok_or(AllocationError::InvalidPolicy)?;
            image.expand(locator.mapper, requested_end, expansion_fill)?;
            staged = image.logical_bytes().to_vec();
            if let Some(offset) = locator.rom_size_field {
                let rom_size = u8::try_from(requested_end.ilog2().saturating_sub(10))
                    .map_err(|_| RomError::InvalidExpansionSize(requested_end))?;
                let image_len = staged.len();
                *staged.get_mut(offset).ok_or(RomError::RangeOutOfBounds {
                    offset,
                    len: 1,
                    image_len,
                })? = rom_size;
            }
            for write in locator.expansion_writes {
                let image_len = staged.len();
                let end = write.offset.checked_add(write.bytes.len()).ok_or(
                    RomError::RangeOutOfBounds {
                        offset: write.offset,
                        len: write.bytes.len(),
                        image_len,
                    },
                )?;
                let destination =
                    staged
                        .get_mut(write.offset..end)
                        .ok_or(RomError::RangeOutOfBounds {
                            offset: write.offset,
                            len: write.bytes.len(),
                            image_len,
                        })?;
                destination.copy_from_slice(write.bytes);
            }
            policy.validate(staged.len())?;
            let blocks =
                allocate_title_blocks(&mut staged, locator, storage, &policy, recording, fill)?;
            (staged, blocks)
        }
        None => {
            return Err(AllocationError::NoSpace {
                required: lm_rats::HEADER_LEN + recording.bytes().len(),
            }
            .into());
        }
    };
    let runtime_bytes = &mut staged[runtime.payload.clone()];
    if matches!(storage, TitleRecordingStorage::Absent) {
        runtime_bytes.copy_from_slice(&locator.runtime_template);
    }
    write_pointer(
        runtime_bytes,
        TitleRecordingPatchLocator::TIMER_POINTER,
        locator.mapper,
        recording_block.payload.start + 2,
    )?;
    write_pointer(
        runtime_bytes,
        TitleRecordingPatchLocator::FIRST_INPUT_POINTER,
        locator.mapper,
        recording_block.payload.start - 3,
    )?;
    write_pointer(
        runtime_bytes,
        TitleRecordingPatchLocator::SECOND_INPUT_POINTER,
        locator.mapper,
        recording_block.payload.start - 2,
    )?;
    let mut hook = locator.hook_template;
    hook[TitleRecordingPatchLocator::HOOK_RUNTIME_POINTER..4]
        .copy_from_slice(&low_bank_pointer(locator.mapper, runtime.payload.start)?);
    staged[locator.hook..locator.hook + TitleRecordingPatchLocator::HOOK_LEN]
        .copy_from_slice(&hook);
    if let Some(compensation) = &locator.checksum_compensation {
        preserve_stored_checksum(&mut staged, &original, checksum_field, compensation.clone())?;
    } else {
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
    }
    commit_staged(
        project,
        "replace title-screen recording".into(),
        &original,
        &staged,
    )?;
    Ok(())
}

fn allocate_title_blocks(
    staged: &mut [u8],
    locator: &TitleRecordingPatchLocator,
    storage: &TitleRecordingStorage,
    policy: &AllocationPolicy,
    recording: &TitleScreenRecording,
    fill: u8,
) -> Result<(RatsBlock, RatsBlock), TitleRecordingPatchError> {
    policy.validate(staged.len())?;
    let runtime = match storage {
        TitleRecordingStorage::Absent => None,
        TitleRecordingStorage::Installed { runtime, recording } => {
            validate_owned(
                staged,
                runtime,
                TitleRecordingPatchError::MissingRuntimeOwnership,
            )?;
            validate_owned(
                staged,
                recording,
                TitleRecordingPatchError::MissingRecordingOwnership,
            )?;
            FreeSpaceAllocator::new(&mut *staged, policy.clone()).erase(recording, fill)?;
            Some(runtime.clone())
        }
    };
    let recording_block =
        FreeSpaceAllocator::new(&mut *staged, policy.clone()).allocate(recording.bytes())?;
    let runtime = match runtime {
        Some(runtime) => runtime,
        None => FreeSpaceAllocator::new(&mut *staged, policy.clone())
            .allocate(&locator.runtime_template)?,
    };
    Ok((runtime, recording_block))
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), TitleRecordingPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(TitleRecordingPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn validate_fixed_bytes(
    actual: &[u8],
    expected: &[u8],
    mutable: &[std::ops::Range<usize>],
) -> Result<(), ()> {
    if actual.len() != expected.len() {
        return Err(());
    }
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        if !mutable.iter().any(|range| range.contains(&index)) && actual != expected {
            return Err(());
        }
    }
    Ok(())
}

fn owned_block(
    bytes: &[u8],
    payload: usize,
    kind: OwnershipKind,
) -> Result<RatsBlock, TitleRecordingPatchError> {
    let error = || match kind {
        OwnershipKind::Runtime => TitleRecordingPatchError::MissingRuntimeOwnership,
        OwnershipKind::Recording => TitleRecordingPatchError::MissingRecordingOwnership,
    };
    let header = payload.checked_sub(lm_rats::HEADER_LEN).ok_or_else(error)?;
    let block = parse_at(bytes, header).map_err(|_| error())?;
    if block.payload.start != payload {
        return Err(error());
    }
    Ok(block)
}

#[derive(Clone, Copy)]
enum OwnershipKind {
    Runtime,
    Recording,
}

fn validate_owned(
    bytes: &[u8],
    expected: &RatsBlock,
    error: TitleRecordingPatchError,
) -> Result<(), TitleRecordingPatchError> {
    if parse_at(bytes, expected.header_offset).ok().as_ref() != Some(expected) {
        return Err(error);
    }
    Ok(())
}

fn read_pointer(bytes: &[u8], offset: usize, mapper: Mapper) -> Result<usize, RomError> {
    let raw = &bytes[offset..offset + 3];
    snes_to_pc(
        mapper,
        u32::from(raw[0]) | u32::from(raw[1]) << 8 | u32::from(raw[2]) << 16,
    )
}

fn write_pointer(
    bytes: &mut [u8],
    offset: usize,
    mapper: Mapper,
    pc: usize,
) -> Result<(), RomError> {
    bytes[offset..offset + 3].copy_from_slice(&low_bank_pointer(mapper, pc)?);
    Ok(())
}

fn low_bank_pointer(mapper: Mapper, pc: usize) -> Result<[u8; 3], RomError> {
    let mut bytes: [u8; 3] = pc_to_snes(mapper, pc)?.to_le_bytes()[..3]
        .try_into()
        .expect("three-byte slice");
    // Only LoROM has an equivalent low-bank mirror. ExLoROM bit 23 selects the ROM half, and
    // SA-1 uses the complete bank value as part of its mapped address.
    if mapper == Mapper::LoRom {
        bytes[2] &= 0x7f;
    }
    Ok(bytes)
}

fn preserve_stored_checksum(
    staged: &mut [u8],
    original: &[u8],
    checksum_field: usize,
    compensation: std::ops::Range<usize>,
) -> Result<(), TitleRecordingPatchError> {
    let fields =
        original
            .get(checksum_field..checksum_field + 4)
            .ok_or(RomError::RangeOutOfBounds {
                offset: checksum_field,
                len: 4,
                image_len: original.len(),
            })?;
    staged[checksum_field..checksum_field + 4].copy_from_slice(fields);
    let staged_len = staged.len();
    let target = staged
        .get_mut(compensation.clone())
        .ok_or(RomError::RangeOutOfBounds {
            offset: compensation.start,
            len: compensation.len(),
            image_len: staged_len,
        })?;
    target.fill(0);
    let expected = u16::from_le_bytes([fields[2], fields[3]]);
    let current = compute_snes_checksum(staged, checksum_field)?.checksum;
    let difference = usize::from(expected.wrapping_sub(current));
    let available = compensation.len() * usize::from(u8::MAX);
    if difference > available {
        return Err(TitleRecordingPatchError::CompensationOverflow {
            required: difference,
            available,
        });
    }
    let full = difference / usize::from(u8::MAX);
    let remainder = difference % usize::from(u8::MAX);
    staged[compensation.start..compensation.start + full].fill(u8::MAX);
    if remainder != 0 {
        staged[compensation.start + full] = remainder as u8;
    }
    let actual = compute_snes_checksum(staged, checksum_field)?.checksum;
    if actual != expected {
        return Err(TitleRecordingPatchError::CompensationMismatch { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_and_recording_pointers_preserve_mapper_significant_high_banks() {
        for (mapper, pc) in [
            (Mapper::LoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x42_0000),
            (Mapper::Sa1, 0x2_0000),
            (Mapper::Sa1, 0x42_0000),
        ] {
            let pointer = low_bank_pointer(mapper, pc).unwrap();
            assert_eq!(read_pointer(&pointer, 0, mapper).unwrap(), pc);
        }
    }
}
