//! Lunar Magic title-screen playback runtime and movement-data persistence.

use crate::{Project, payload::staging::commit_staged};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};
use lm_title::{TitleScreenRecording, TitleScreenRecordingError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleRecordingPatchLocator {
    pub mapper: Mapper,
    pub hook: usize,
    pub pristine_hook: [u8; Self::HOOK_LEN],
    pub hook_template: [u8; Self::HOOK_LEN],
    pub runtime_template: [u8; Self::RUNTIME_LEN],
    /// Logical ROM target placed into the runtime's initial 16-bit continuation operand.
    pub continuation_target: usize,
}

impl TitleRecordingPatchLocator {
    pub const HOOK_LEN: usize = 0x11;
    pub const RUNTIME_LEN: usize = 0x60;
    pub const HOOK_RUNTIME_POINTER: usize = 1;
    pub const CONTINUATION_WORD: usize = 9;
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
    ContinuationMismatch,
    MissingRuntimeOwnership,
    RuntimeLength(usize),
    MissingRecordingOwnership,
    RecordingPointersDisagree,
    Rom(RomError),
    Recording(TitleScreenRecordingError),
    Allocation(AllocationError),
    Commit(crate::PayloadSaveError),
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
                TitleRecordingPatchLocator::CONTINUATION_WORD
                    ..TitleRecordingPatchLocator::CONTINUATION_WORD + 2,
                TitleRecordingPatchLocator::TIMER_POINTER
                    ..TitleRecordingPatchLocator::TIMER_POINTER + 3,
                TitleRecordingPatchLocator::FIRST_INPUT_POINTER
                    ..TitleRecordingPatchLocator::FIRST_INPUT_POINTER + 3,
                TitleRecordingPatchLocator::SECOND_INPUT_POINTER
                    ..TitleRecordingPatchLocator::SECOND_INPUT_POINTER + 3,
            ],
        )
        .map_err(|()| TitleRecordingPatchError::RuntimeSignature)?;
        let expected_continuation =
            pc_to_snes(locator.mapper, locator.continuation_target)?.to_le_bytes();
        if runtime_bytes[TitleRecordingPatchLocator::CONTINUATION_WORD
            ..TitleRecordingPatchLocator::CONTINUATION_WORD + 2]
            != expected_continuation[..2]
        {
            return Err(TitleRecordingPatchError::ContinuationMismatch);
        }
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
    if allocation.search.end > image.logical_len() {
        image.expand(locator.mapper, allocation.search.end, fill)?;
    }
    let mut staged = image.logical_bytes().to_vec();
    let mut policy = allocation.clone();
    policy.protected.extend([
        ProtectedRange(locator.hook..locator.hook + TitleRecordingPatchLocator::HOOK_LEN),
        ProtectedRange(checksum_field..checksum_field + 4),
    ]);
    policy.validate(staged.len())?;
    let runtime = match storage {
        TitleRecordingStorage::Absent => FreeSpaceAllocator::new(&mut staged, policy.clone())
            .allocate(&locator.runtime_template)?,
        TitleRecordingStorage::Installed { runtime, recording } => {
            validate_owned(
                &staged,
                runtime,
                TitleRecordingPatchError::MissingRuntimeOwnership,
            )?;
            validate_owned(
                &staged,
                recording,
                TitleRecordingPatchError::MissingRecordingOwnership,
            )?;
            FreeSpaceAllocator::new(&mut staged, policy.clone()).erase(recording, fill)?;
            runtime.clone()
        }
    };
    let recording_block =
        FreeSpaceAllocator::new(&mut staged, policy).allocate(recording.bytes())?;
    let runtime_bytes = &mut staged[runtime.payload.clone()];
    if matches!(storage, TitleRecordingStorage::Absent) {
        runtime_bytes.copy_from_slice(&locator.runtime_template);
    }
    runtime_bytes[TitleRecordingPatchLocator::CONTINUATION_WORD
        ..TitleRecordingPatchLocator::CONTINUATION_WORD + 2]
        .copy_from_slice(
            &pc_to_snes(locator.mapper, locator.continuation_target)?.to_le_bytes()[..2],
        );
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
    let checksum = compute_snes_checksum(&staged, checksum_field)?;
    staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
    commit_staged(
        project,
        "replace title-screen recording".into(),
        &original,
        &staged,
    )?;
    Ok(())
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
    bytes[2] &= 0x7f;
    Ok(bytes)
}
