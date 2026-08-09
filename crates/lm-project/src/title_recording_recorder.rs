//! Temporary Lunar Magic joypad-recorder runtime installation.

use crate::{Project, payload::staging::commit_staged};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleRecordingRecorderLocator {
    pub mapper: Mapper,
    pub first_hook: usize,
    pub pristine_first_hook: Vec<u8>,
    pub installed_first_hook: Vec<u8>,
    pub first_hook_pointer: usize,
    pub second_hook: usize,
    pub pristine_second_hook: Vec<u8>,
    pub installed_second_hook: Vec<u8>,
    pub second_hook_pointer: usize,
    pub second_runtime_offset: usize,
    pub runtime_template: Vec<u8>,
    pub compensation: usize,
    pub compensation_len: usize,
    pub checksum_field: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TitleRecordingRecorderState {
    Absent,
    Installed { runtime: RatsBlock },
}

#[derive(Debug)]
pub enum TitleRecordingRecorderError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    InvalidLocator,
    HookSignature,
    RuntimePointer,
    RuntimeOwnership,
    RuntimeSignature,
    CompensationOverflow { required: usize, available: usize },
    CompensationMismatch { expected: u16, actual: u16 },
    Rom(RomError),
    Allocation(AllocationError),
    Commit(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for TitleRecordingRecorderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "title-recording recorder patch failed: {self:?}")
    }
}

impl std::error::Error for TitleRecordingRecorderError {}

impl From<RomError> for TitleRecordingRecorderError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<AllocationError> for TitleRecordingRecorderError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<crate::PayloadSaveError> for TitleRecordingRecorderError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Authenticates either the pristine hooks or Lunar Magic's complete temporary recorder.
    pub fn load_title_recording_recorder_detected(
        &self,
        locator: &TitleRecordingRecorderLocator,
    ) -> Result<TitleRecordingRecorderState, TitleRecordingRecorderError> {
        validate_locator(self, locator)?;
        let first = self
            .rom
            .read(locator.first_hook, locator.pristine_first_hook.len())?;
        let second = self
            .rom
            .read(locator.second_hook, locator.pristine_second_hook.len())?;
        if first == locator.pristine_first_hook && second == locator.pristine_second_hook {
            return Ok(TitleRecordingRecorderState::Absent);
        }
        validate_hook(
            first,
            &locator.installed_first_hook,
            locator.first_hook_pointer,
        )?;
        validate_hook(
            second,
            &locator.installed_second_hook,
            locator.second_hook_pointer,
        )?;
        let runtime_start = read_pointer(
            self.rom.logical_bytes(),
            locator.first_hook + locator.first_hook_pointer,
            locator.mapper,
        )?;
        let second_start = read_pointer(
            self.rom.logical_bytes(),
            locator.second_hook + locator.second_hook_pointer,
            locator.mapper,
        )?;
        if second_start != runtime_start + locator.second_runtime_offset {
            return Err(TitleRecordingRecorderError::RuntimePointer);
        }
        let header = runtime_start
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(TitleRecordingRecorderError::RuntimeOwnership)?;
        let runtime = parse_at(self.rom.logical_bytes(), header)
            .map_err(|_| TitleRecordingRecorderError::RuntimeOwnership)?;
        if runtime.payload.start != runtime_start
            || runtime.payload.len() != locator.runtime_template.len()
        {
            return Err(TitleRecordingRecorderError::RuntimeOwnership);
        }
        if self.rom.logical_bytes()[runtime.payload.clone()] != locator.runtime_template {
            return Err(TitleRecordingRecorderError::RuntimeSignature);
        }
        Ok(TitleRecordingRecorderState::Installed { runtime })
    }

    /// Installs Lunar Magic's temporary level joypad recorder as one exact undoable transaction.
    pub fn install_title_recording_recorder(
        &mut self,
        locator: &TitleRecordingRecorderLocator,
        allocation: &AllocationPolicy,
    ) -> Result<bool, TitleRecordingRecorderError> {
        if matches!(
            self.load_title_recording_recorder_detected(locator)?,
            TitleRecordingRecorderState::Installed { .. }
        ) {
            return Ok(false);
        }
        let original = self.rom.logical_bytes().to_vec();
        let mut image = RomImage::from_bytes(original.clone())?;
        if allocation.search.end > image.logical_len() {
            image.expand(locator.mapper, allocation.search.end, 0)?;
        }
        let mut staged = image.logical_bytes().to_vec();
        let mut policy = allocation.clone();
        protect_locator(&mut policy, locator);
        policy.validate(staged.len())?;
        let runtime =
            FreeSpaceAllocator::new(&mut staged, policy).allocate(&locator.runtime_template)?;
        write_installed_hook(
            &mut staged,
            locator.first_hook,
            &locator.installed_first_hook,
            locator.first_hook_pointer,
            locator.mapper,
            runtime.payload.start,
        )?;
        write_installed_hook(
            &mut staged,
            locator.second_hook,
            &locator.installed_second_hook,
            locator.second_hook_pointer,
            locator.mapper,
            runtime.payload.start + locator.second_runtime_offset,
        )?;
        preserve_stored_checksum(&mut staged, &original, locator)?;
        commit_staged(
            self,
            "install title-recording joypad recorder".into(),
            &original,
            &staged,
        )?;
        if !matches!(
            self.load_title_recording_recorder_detected(locator)?,
            TitleRecordingRecorderState::Installed { .. }
        ) {
            return Err(TitleRecordingRecorderError::ReopenMismatch);
        }
        Ok(true)
    }

    /// Removes only an authenticated recorder and reconstructs the original checksum run.
    pub fn uninstall_title_recording_recorder(
        &mut self,
        locator: &TitleRecordingRecorderLocator,
        allocation: &AllocationPolicy,
    ) -> Result<bool, TitleRecordingRecorderError> {
        let TitleRecordingRecorderState::Installed { runtime } =
            self.load_title_recording_recorder_detected(locator)?
        else {
            return Ok(false);
        };
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = original.clone();
        let mut policy = allocation.clone();
        protect_locator(&mut policy, locator);
        policy.validate(staged.len())?;
        FreeSpaceAllocator::new(&mut staged, policy).erase(&runtime, 0)?;
        staged[locator.first_hook..locator.first_hook + locator.pristine_first_hook.len()]
            .copy_from_slice(&locator.pristine_first_hook);
        staged[locator.second_hook..locator.second_hook + locator.pristine_second_hook.len()]
            .copy_from_slice(&locator.pristine_second_hook);
        preserve_stored_checksum(&mut staged, &original, locator)?;
        commit_staged(
            self,
            "uninstall title-recording joypad recorder".into(),
            &original,
            &staged,
        )?;
        if self.load_title_recording_recorder_detected(locator)?
            != TitleRecordingRecorderState::Absent
        {
            return Err(TitleRecordingRecorderError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn validate_locator(
    project: &Project,
    locator: &TitleRecordingRecorderLocator,
) -> Result<(), TitleRecordingRecorderError> {
    if let Some(identity) = &project.identity
        && identity.mapper != locator.mapper
    {
        return Err(TitleRecordingRecorderError::MapperMismatch {
            expected: locator.mapper,
            actual: identity.mapper,
        });
    }
    let valid_hook = |pristine: &[u8], installed: &[u8], pointer: usize| {
        !pristine.is_empty()
            && pristine.len() == installed.len()
            && pointer
                .checked_add(3)
                .is_some_and(|end| end <= installed.len())
    };
    if !valid_hook(
        &locator.pristine_first_hook,
        &locator.installed_first_hook,
        locator.first_hook_pointer,
    ) || !valid_hook(
        &locator.pristine_second_hook,
        &locator.installed_second_hook,
        locator.second_hook_pointer,
    ) || locator.runtime_template.is_empty()
        || locator.second_runtime_offset >= locator.runtime_template.len()
        || locator.compensation_len == 0
        || locator.compensation + locator.compensation_len > project.rom.logical_len()
        || locator.checksum_field + 4 > project.rom.logical_len()
    {
        return Err(TitleRecordingRecorderError::InvalidLocator);
    }
    Ok(())
}

fn protect_locator(policy: &mut AllocationPolicy, locator: &TitleRecordingRecorderLocator) {
    policy.protected.extend([
        ProtectedRange(locator.first_hook..locator.first_hook + locator.pristine_first_hook.len()),
        ProtectedRange(
            locator.second_hook..locator.second_hook + locator.pristine_second_hook.len(),
        ),
        ProtectedRange(locator.compensation..locator.compensation + locator.compensation_len),
        ProtectedRange(locator.checksum_field..locator.checksum_field + 4),
    ]);
}

fn validate_hook(
    actual: &[u8],
    expected: &[u8],
    pointer: usize,
) -> Result<(), TitleRecordingRecorderError> {
    if actual.len() != expected.len()
        || actual
            .iter()
            .zip(expected)
            .enumerate()
            .any(|(index, (a, b))| !(pointer..pointer + 3).contains(&index) && a != b)
    {
        return Err(TitleRecordingRecorderError::HookSignature);
    }
    Ok(())
}

fn write_installed_hook(
    staged: &mut [u8],
    offset: usize,
    template: &[u8],
    pointer: usize,
    mapper: Mapper,
    target: usize,
) -> Result<(), TitleRecordingRecorderError> {
    let end = offset + template.len();
    staged[offset..end].copy_from_slice(template);
    let mut encoded = pc_to_snes(mapper, target)?.to_le_bytes();
    if mapper == Mapper::LoRom {
        encoded[2] &= 0x7f;
    }
    staged[offset + pointer..offset + pointer + 3].copy_from_slice(&encoded[..3]);
    Ok(())
}

fn read_pointer(
    bytes: &[u8],
    offset: usize,
    mapper: Mapper,
) -> Result<usize, TitleRecordingRecorderError> {
    let value = bytes
        .get(offset..offset + 3)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: 3,
            image_len: bytes.len(),
        })?;
    let address = u32::from(value[0]) | u32::from(value[1]) << 8 | u32::from(value[2]) << 16;
    snes_to_pc(mapper, address).map_err(|_| TitleRecordingRecorderError::RuntimePointer)
}

fn preserve_stored_checksum(
    staged: &mut [u8],
    original: &[u8],
    locator: &TitleRecordingRecorderLocator,
) -> Result<(), TitleRecordingRecorderError> {
    let fields = &original[locator.checksum_field..locator.checksum_field + 4];
    staged[locator.checksum_field..locator.checksum_field + 4].copy_from_slice(fields);
    staged[locator.compensation..locator.compensation + locator.compensation_len].fill(0);
    let expected = u16::from_le_bytes([fields[2], fields[3]]);
    let current = compute_snes_checksum(staged, locator.checksum_field)?.checksum;
    let difference = usize::from(expected.wrapping_sub(current));
    let available = locator.compensation_len * usize::from(u8::MAX);
    if difference > available {
        return Err(TitleRecordingRecorderError::CompensationOverflow {
            required: difference,
            available,
        });
    }
    let full = difference / usize::from(u8::MAX);
    let remainder = difference % usize::from(u8::MAX);
    staged[locator.compensation..locator.compensation + full].fill(u8::MAX);
    if remainder != 0 {
        staged[locator.compensation + full] = remainder as u8;
    }
    let actual = compute_snes_checksum(staged, locator.checksum_field)?.checksum;
    if actual != expected {
        return Err(TitleRecordingRecorderError::CompensationMismatch { expected, actual });
    }
    Ok(())
}
