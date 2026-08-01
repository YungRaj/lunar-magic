//! Clean-room installation of Lunar Magic's user-requested sprite `$19` ASM fix.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;
use std::fmt;

pub const SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET: usize = 0x0000_e762;
pub const SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET: usize = 0x0001_bca0;
pub const SMW_US_V1_SPRITE19_FIX_BRANCH_OFFSET: usize = 0x0000_20a0;

const HOOK_EXPECTED: [u8; 6] = [0x8d, 0x11, 0x1f, 0x8d, 0xb8, 0x1f];
const HOOK_INSTALLED: [u8; 6] = [0xea, 0x22, 0xa0, 0xbc, 0x03, 0xea];
const RUNTIME_EXPECTED: [u8; 0x20] = [0xff; 0x20];
const RUNTIME_INSTALLED: [u8; 0x20] = [
    0xad, 0x09, 0x01, 0xf0, 0x10, 0xaf, 0xf0, 0x9e, 0x00, 0x8d, 0x11, 0x1f, 0x8d, 0xb8, 0x1f, 0x6b,
    0x22, 0xc9, 0x9b, 0x00, 0xfa, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x11, 0x01,
];
const BRANCH_EXPECTED: [u8; 3] = [0x9c, 0x11, 0x1f];
const BRANCH_INSTALLED: [u8; 3] = [0xea; 3];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1Sprite19FixState {
    Pristine,
    SharedRuntimeInstalled,
    Installed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1Sprite19FixDetectError {
    Truncated { offset: usize, needed: usize },
    InconsistentRuntime,
}

impl fmt::Display for SmwUsV1Sprite19FixDetectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot detect SMW-US sprite 19 ASM fix: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Sprite19FixDetectError {}

/// Authenticates the pristine, shared-runtime-only, or complete current fix shape.
///
/// # Errors
///
/// Rejects truncated ROMs and every partial or modified combination.
pub fn detect_smw_us_v1_sprite19_fix(
    bytes: &[u8],
) -> Result<SmwUsV1Sprite19FixState, SmwUsV1Sprite19FixDetectError> {
    let hook = exact(
        bytes,
        SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET,
        HOOK_EXPECTED.len(),
    )?;
    let runtime = exact(
        bytes,
        SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET,
        RUNTIME_EXPECTED.len(),
    )?;
    let branch = exact(
        bytes,
        SMW_US_V1_SPRITE19_FIX_BRANCH_OFFSET,
        BRANCH_EXPECTED.len(),
    )?;
    let pristine_support = hook == HOOK_EXPECTED && runtime == RUNTIME_EXPECTED;
    let installed_support = hook == HOOK_INSTALLED && runtime == RUNTIME_INSTALLED;
    if pristine_support && branch == BRANCH_EXPECTED {
        Ok(SmwUsV1Sprite19FixState::Pristine)
    } else if installed_support && branch == BRANCH_EXPECTED {
        Ok(SmwUsV1Sprite19FixState::SharedRuntimeInstalled)
    } else if installed_support && branch == BRANCH_INSTALLED {
        Ok(SmwUsV1Sprite19FixState::Installed)
    } else {
        Err(SmwUsV1Sprite19FixDetectError::InconsistentRuntime)
    }
}

/// Builds the exact fixed-location transaction needed for the detected source state.
///
/// The shared `$20`-byte runtime is omitted when Lunar Magic has already installed and
/// authenticated it for another feature.
///
/// # Errors
///
/// Rejects truncated, modified, partially installed, and already-complete inputs.
pub fn smw_us_v1_sprite19_fix_installation_plan(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Sprite19FixInstallError> {
    let state =
        detect_smw_us_v1_sprite19_fix(bytes).map_err(SmwUsV1Sprite19FixInstallError::Detect)?;
    if state == SmwUsV1Sprite19FixState::Installed {
        return Err(SmwUsV1Sprite19FixInstallError::AlreadyInstalled);
    }
    let mut writes = Vec::with_capacity(3);
    if state == SmwUsV1Sprite19FixState::Pristine {
        writes.push(write(
            SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET,
            &HOOK_EXPECTED,
            &HOOK_INSTALLED,
        ));
        writes.push(write(
            SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET,
            &RUNTIME_EXPECTED,
            &RUNTIME_INSTALLED,
        ));
    }
    writes.push(write(
        SMW_US_V1_SPRITE19_FIX_BRANCH_OFFSET,
        &BRANCH_EXPECTED,
        &BRANCH_INSTALLED,
    ));
    Ok(RelocatablePatchPlan {
        description: "install SMW US sprite 19 ASM fix".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(0..bytes.len()),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1Sprite19FixInstallError {
    Detect(SmwUsV1Sprite19FixDetectError),
    AlreadyInstalled,
}

impl fmt::Display for SmwUsV1Sprite19FixInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot install SMW-US sprite 19 ASM fix: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Sprite19FixInstallError {}

fn exact(
    bytes: &[u8],
    offset: usize,
    needed: usize,
) -> Result<&[u8], SmwUsV1Sprite19FixDetectError> {
    bytes
        .get(offset..offset + needed)
        .ok_or(SmwUsV1Sprite19FixDetectError::Truncated { offset, needed })
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
    fn pristine_and_shared_runtime_paths_install_exactly_and_reject_corruption() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        assert_eq!(
            detect_smw_us_v1_sprite19_fix(&original).unwrap(),
            SmwUsV1Sprite19FixState::Pristine
        );
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        project
            .install_relocatable_patch(
                &smw_us_v1_sprite19_fix_installation_plan(project.rom.logical_bytes()).unwrap(),
            )
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_sprite19_fix(project.rom.logical_bytes()).unwrap(),
            SmwUsV1Sprite19FixState::Installed
        );
        assert_eq!(
            &project.rom.logical_bytes()
                [SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET..SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET + 6],
            &HOOK_INSTALLED
        );
        assert_eq!(
            &project.rom.logical_bytes()[SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET
                ..SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET + 0x20],
            &RUNTIME_INSTALLED
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);

        let mut shared = original.clone();
        shared[SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET..SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET + 6]
            .copy_from_slice(&HOOK_INSTALLED);
        shared[SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET..SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET + 0x20]
            .copy_from_slice(&RUNTIME_INSTALLED);
        assert_eq!(
            detect_smw_us_v1_sprite19_fix(&shared).unwrap(),
            SmwUsV1Sprite19FixState::SharedRuntimeInstalled
        );
        assert_eq!(
            smw_us_v1_sprite19_fix_installation_plan(&shared)
                .unwrap()
                .writes
                .len(),
            1
        );
        shared[SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET] ^= 1;
        assert_eq!(
            detect_smw_us_v1_sprite19_fix(&shared),
            Err(SmwUsV1Sprite19FixDetectError::InconsistentRuntime)
        );
    }
}
