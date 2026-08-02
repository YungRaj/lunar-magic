//! Clean-room detection and installation of Lunar Magic's fixed-location level support patch B.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_SUPPORT_PATCH_B_RUNTIME_OFFSET: usize = 0x0006_f160;
pub const SMW_US_V1_SUPPORT_PATCH_B_HOOK_OFFSETS: [usize; 5] = [
    0x0006_a4ca,
    0x0006_c20f,
    0x0006_ce0f,
    0x0006_da0f,
    0x0006_e90f,
];

const HOOK_EXPECTED: [u8; 2] = [0xe3, 0xb3];
const HOOK_INSTALLED: [u8; 2] = [0x60, 0xf1];
const RUNTIME_EXPECTED: [u8; 0x30] = [0xff; 0x30];
const RUNTIME_INSTALLED: [u8; 0x30] = [
    0xa5, 0x59, 0x29, 0x80, 0xd0, 0x05, 0xad, 0x1a, 0x14, 0xd0, 0x17, 0xa5, 0x57, 0x29, 0x0f, 0x8d,
    0x33, 0x0f, 0xa5, 0x57, 0x4a, 0x4a, 0x4a, 0x4a, 0x8d, 0x32, 0x0f, 0xa5, 0x59, 0x29, 0x0f, 0x8d,
    0x31, 0x0f, 0x60, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1SupportPatchBState {
    Pristine,
    Installed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1SupportPatchBDetectError {
    Truncated { offset: usize, needed: usize },
    InconsistentRuntime,
}

impl std::fmt::Display for SmwUsV1SupportPatchBDetectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot detect SMW-US support patch B: {self:?}")
    }
}

impl std::error::Error for SmwUsV1SupportPatchBDetectError {}

/// Authenticates all five JSR operands and the complete fixed `$0D:F160` runtime reservation.
///
/// # Errors
///
/// Rejects truncated, partially installed, or modified combinations.
pub fn detect_smw_us_v1_support_patch_b(
    bytes: &[u8],
) -> Result<SmwUsV1SupportPatchBState, SmwUsV1SupportPatchBDetectError> {
    let runtime = exact(
        bytes,
        SMW_US_V1_SUPPORT_PATCH_B_RUNTIME_OFFSET,
        RUNTIME_EXPECTED.len(),
    )?;
    let hooks = SMW_US_V1_SUPPORT_PATCH_B_HOOK_OFFSETS
        .map(|offset| exact(bytes, offset, HOOK_EXPECTED.len()));
    let pristine = runtime == RUNTIME_EXPECTED
        && hooks
            .iter()
            .all(|hook| hook.as_ref().is_ok_and(|bytes| *bytes == HOOK_EXPECTED));
    let installed = runtime == RUNTIME_INSTALLED
        && hooks
            .iter()
            .all(|hook| hook.as_ref().is_ok_and(|bytes| *bytes == HOOK_INSTALLED));
    if pristine {
        Ok(SmwUsV1SupportPatchBState::Pristine)
    } else if installed {
        Ok(SmwUsV1SupportPatchBState::Installed)
    } else if let Some(Err(error)) = hooks.into_iter().find(Result::is_err) {
        Err(error)
    } else {
        Err(SmwUsV1SupportPatchBDetectError::InconsistentRuntime)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1SupportPatchBInstallError {
    Detect(SmwUsV1SupportPatchBDetectError),
    AlreadyInstalled,
}

impl std::fmt::Display for SmwUsV1SupportPatchBInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot install SMW-US support patch B: {self:?}")
    }
}

impl std::error::Error for SmwUsV1SupportPatchBInstallError {}

/// Builds the exact checksum-inclusive fixed-location installation transaction.
///
/// # Errors
///
/// Rejects malformed, partially installed, and already-installed ROMs.
pub fn smw_us_v1_support_patch_b_installation_plan(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1SupportPatchBInstallError> {
    match detect_smw_us_v1_support_patch_b(bytes)
        .map_err(SmwUsV1SupportPatchBInstallError::Detect)?
    {
        SmwUsV1SupportPatchBState::Installed => {
            return Err(SmwUsV1SupportPatchBInstallError::AlreadyInstalled);
        }
        SmwUsV1SupportPatchBState::Pristine => {}
    }
    let mut writes = Vec::with_capacity(6);
    for offset in SMW_US_V1_SUPPORT_PATCH_B_HOOK_OFFSETS {
        writes.push(write(offset, &HOOK_EXPECTED, &HOOK_INSTALLED));
    }
    writes.push(write(
        SMW_US_V1_SUPPORT_PATCH_B_RUNTIME_OFFSET,
        &RUNTIME_EXPECTED,
        &RUNTIME_INSTALLED,
    ));
    Ok(RelocatablePatchPlan {
        description: "install SMW US level support patch B".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(0..bytes.len()),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes,
    })
}

/// Pure semantic model of the runtime's conditional Layer 2 scroll-register update.
#[must_use]
pub const fn smw_us_v1_support_patch_b_scroll_registers(
    level_57: u8,
    level_59: u8,
    layer2_active_141a: u8,
) -> Option<[u8; 3]> {
    if level_59 & 0x80 != 0 || layer2_active_141a != 0 {
        None
    } else {
        Some([level_59 & 0x0f, level_57 >> 4, level_57 & 0x0f])
    }
}

fn exact(
    bytes: &[u8],
    offset: usize,
    needed: usize,
) -> Result<&[u8], SmwUsV1SupportPatchBDetectError> {
    bytes
        .get(offset..offset + needed)
        .ok_or(SmwUsV1SupportPatchBDetectError::Truncated { offset, needed })
}

fn write(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::RomImage;

    #[test]
    fn pristine_fixture_installs_exact_runtime_reopens_and_undoes() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        assert_eq!(
            detect_smw_us_v1_support_patch_b(&original).unwrap(),
            SmwUsV1SupportPatchBState::Pristine
        );
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        project
            .install_relocatable_patch(
                &smw_us_v1_support_patch_b_installation_plan(project.rom.logical_bytes()).unwrap(),
            )
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_support_patch_b(project.rom.logical_bytes()).unwrap(),
            SmwUsV1SupportPatchBState::Installed
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn partial_or_modified_shapes_are_rejected() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut partial = original.clone();
        partial[SMW_US_V1_SUPPORT_PATCH_B_HOOK_OFFSETS[0]..][..2].copy_from_slice(&HOOK_INSTALLED);
        assert_eq!(
            detect_smw_us_v1_support_patch_b(&partial),
            Err(SmwUsV1SupportPatchBDetectError::InconsistentRuntime)
        );
        let mut modified = original;
        modified[SMW_US_V1_SUPPORT_PATCH_B_RUNTIME_OFFSET] = 0;
        assert_eq!(
            detect_smw_us_v1_support_patch_b(&modified),
            Err(SmwUsV1SupportPatchBDetectError::InconsistentRuntime)
        );
    }

    #[test]
    fn semantic_register_model_matches_every_branch_and_nibble() {
        assert_eq!(
            smw_us_v1_support_patch_b_scroll_registers(0xab, 0x4c, 0),
            Some([0x0c, 0x0a, 0x0b])
        );
        assert_eq!(
            smw_us_v1_support_patch_b_scroll_registers(0xab, 0xcc, 0),
            None
        );
        assert_eq!(
            smw_us_v1_support_patch_b_scroll_registers(0xab, 0x4c, 1),
            None
        );
    }
}
