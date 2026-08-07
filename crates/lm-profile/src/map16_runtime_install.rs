//! Authenticated pristine SMW-US installation of Lunar Magic's complete Map16 runtime.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, ProtectedRange, RatsBlock, parse_at};
use lm_rom::{IpsError, Mapper, RomError, apply_ips, snes_to_pc};
use std::{fmt, io::Read};

const FIXED_PATCH_BASE64: &str = include_str!("assets/map16_runtime_fixed.ips.b64");
#[cfg(test)]
const STAGE3_TO_STAGE4_ORACLE_IPS_BASE64: &str =
    include_str!("assets/map16_stage3_to_stage4.ips.b64");
const AUXILIARY_PAYLOAD_GZIP_BASE64: &str = include_str!("assets/map16_auxiliary.bin.gz.b64");
const PRISTINE_LOGICAL_LEN: usize = 0x80_000;
const EXPANDED_LOGICAL_LEN: usize = 0x100_000;
const AUXILIARY_PAYLOAD_LEN: usize = 0x8000;
const AUXILIARY_BANK_OPERAND: usize = 0x37_626;
const STAGED_HOOK_BASE_OFFSET: usize = 0x37_600;
const STAGE_MARKER_OFFSET: usize = 0x37_65c;
const STAGE_FOUR_HOOK_OFFSET: usize = 0x37_7a0;
const STAGE_TWO_COMPARE_HOOK_OFFSET: usize = STAGED_HOOK_BASE_OFFSET + 0x10b;
const STAGE_ONE_HOOK_BASE: [u8; 0x19] = [
    0xea, 0xea, 0x98, 0x5c, 0x45, 0xf5, 0x00, 0xea, 0xac, 0x9b, 0x0d, 0x10, 0x0c, 0x7a, 0x7a, 0x80,
    0xf2, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea, 0xeb,
];
const STAGED_HOOK_BASE: [u8; 0x44] = [
    0xea, 0xea, 0x98, 0x5c, 0x45, 0xf5, 0x00, 0xea, 0xac, 0x9b, 0x0d, 0x10, 0x0a, 0x7a, 0x7a, 0x80,
    0xf2, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea, 0xeb, 0xad, 0x93, 0x16, 0xda, 0xc2, 0x30, 0xa8, 0x0a,
    0xaa, 0x30, 0x16, 0xbf, 0x00, 0x80, 0x11, 0xc9, 0x00, 0x02, 0xb0, 0xf2, 0x84, 0x03, 0xe2, 0x30,
    0xfa, 0x8d, 0x93, 0x16, 0xeb, 0xa8, 0xa3, 0x08, 0x60, 0xbf, 0x00, 0x80, 0xff, 0xc9, 0x00, 0x02,
    0xb0, 0xdc, 0x80, 0xe8,
];
const STAGE_THREE_MARKER: [u8; 4] = [0x4c, 0x4d, 0x11, 0x01];
const STAGE_FOUR_MARKER: [u8; 4] = [0x4c, 0x4d, 0x12, 0x01];
const STAGE_ONE_MARKER: [u8; 4] = [0x4c, 0x4d, 0x00, 0x01];
const STAGE_TWO_MARKER: [u8; 4] = [0x4c, 0x4d, 0x01, 0x01];
const STAGE_TWO_COMPARE_HOOK: [u8; 0x0f] = [
    0xc9, 0xd2, 0xf0, 0x11, 0xc9, 0x93, 0xf0, 0x1d, 0xc9, 0x7f, 0xf0, 0x19, 0x4c, 0x02, 0xf6,
];
const STAGE_THREE_HOOK: [u8; 0x14] = [
    0x20, 0x08, 0xf6, 0xc9, 0xda, 0xf0, 0x19, 0x4c, 0x02, 0xf6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
];
const STAGE_FOUR_HOOK: [u8; 0x14] = [
    0x20, 0x08, 0xf6, 0xa5, 0x0f, 0xf0, 0x04, 0xa3, 0x0a, 0x80, 0x02, 0xa3, 0x06, 0xc9, 0xda, 0xf0,
    0x0f, 0x4c, 0x02, 0xf6,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1Map16RuntimeGeneration {
    Absent,
    StageOneLegacy,
    StageTwoLegacy,
    StageThreeLegacy,
    StageFourCurrent,
}

/// Builds the authenticated pristine-ROM Map16 runtime plan with Lunar Magic's recovered
/// 512-KiB-to-1-MiB allocation window.
///
/// # Errors
///
/// Rejects any source that is not the exact pristine logical ROM shape, or malformed embedded
/// runtime evidence.
pub fn smw_us_v1_builtin_map16_runtime_installation_plan(
    source: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Map16RuntimeInstallBuildError> {
    let search_end = match source.len() {
        PRISTINE_LOGICAL_LEN => EXPANDED_LOGICAL_LEN,
        EXPANDED_LOGICAL_LEN => 0x200_000,
        length => return Err(SmwUsV1Map16RuntimeInstallBuildError::PristineLength(length)),
    };
    smw_us_v1_map16_runtime_installation_plan(
        source,
        AllocationPolicy {
            search: PRISTINE_LOGICAL_LEN..search_end,
            bank_size: Some(0x8000),
            fill_bytes: vec![0x00, 0xff],
            protected: vec![ProtectedRange(0x7fc0..0x8000)],
        },
        crate::SMW_US_V1_CHECKSUM_FIELD,
    )
}

#[derive(Debug)]
pub enum SmwUsV1Map16RuntimeInstallBuildError {
    PristineLength(usize),
    InvalidEmbeddedBase64,
    AuxiliaryIo(std::io::Error),
    AuxiliaryLength(usize),
    Ips(IpsError),
    PatchedLength(usize),
    MissingAuxiliaryBankOperand,
    MissingAlignedAuxiliarySpace,
}

#[derive(Debug)]
pub enum SmwUsV1Map16RuntimeDetectError {
    Embedded(SmwUsV1Map16RuntimeInstallBuildError),
    FixedByteMismatch {
        offset: usize,
        expected: u8,
        actual: u8,
    },
    AuxiliaryAddress(RomError),
    AuxiliaryBeforeHeader(usize),
    AuxiliaryHeader(HeaderError),
    AuxiliaryOwnership {
        expected: usize,
        actual: usize,
    },
    AuxiliaryLength(usize),
    AuxiliaryPayloadMismatch,
}

#[derive(Debug)]
pub enum SmwUsV1Map16StageThreeMigrationBuildError {
    Detect(SmwUsV1Map16RuntimeDetectError),
    MissingStageThree,
}

#[derive(Debug)]
pub enum SmwUsV1Map16LegacyMigrationBuildError {
    Detect(SmwUsV1Map16RuntimeDetectError),
    MissingLegacy,
}

impl fmt::Display for SmwUsV1Map16LegacyMigrationBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot build SMW-US legacy Map16 migration: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Map16LegacyMigrationBuildError {}

impl fmt::Display for SmwUsV1Map16StageThreeMigrationBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot build SMW-US Map16 stage-three migration: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Map16StageThreeMigrationBuildError {}

impl fmt::Display for SmwUsV1Map16RuntimeDetectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot authenticate current SMW-US Map16 runtime: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Map16RuntimeDetectError {}

/// Authenticates the current Map16 runtime and its complete auxiliary allocation.
///
/// Returns `Ok(None)` only when the recovered runtime marker is absent. A present marker upgrades
/// the operation to strict authentication: every fixed IPS byte except the checksum and typed bank
/// relocation must match, and that relocation must resolve to an exactly owned `$8000`-byte RATS
/// payload equal to the recovered auxiliary table.
///
/// # Errors
///
/// Rejects malformed embedded evidence, truncated or modified fixed patches, invalid relocated
/// addresses, malformed/non-exact RATS ownership, and any auxiliary payload difference.
pub fn detect_smw_us_v1_current_map16_runtime(
    bytes: &[u8],
) -> Result<Option<RatsBlock>, SmwUsV1Map16RuntimeDetectError> {
    if bytes
        .get(super::native_map16_secondary::SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        != Some(0x22)
    {
        return Ok(None);
    }
    authenticate_current_map16_runtime(bytes).map(Some)
}

/// Classifies the retained stage-three runtime emitted by Lunar Magic 3.01 and the current
/// stage-four runtime. Every non-absent result is authenticated across the complete staged hook
/// network, including the currently unused stage-four destination.
///
/// # Errors
///
/// Rejects a partial or modified legacy/current runtime candidate.
pub fn probe_smw_us_v1_map16_runtime_generation(
    bytes: &[u8],
) -> Result<SmwUsV1Map16RuntimeGeneration, SmwUsV1Map16RuntimeDetectError> {
    match bytes.get(STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4) {
        Some(marker) if marker == STAGE_ONE_MARKER => {
            authenticate_stage_one_map16_hooks(bytes)?;
            Ok(SmwUsV1Map16RuntimeGeneration::StageOneLegacy)
        }
        Some(marker) if marker == STAGE_TWO_MARKER => {
            authenticate_stage_two_map16_hooks(bytes)?;
            Ok(SmwUsV1Map16RuntimeGeneration::StageTwoLegacy)
        }
        Some(marker) if marker == STAGE_THREE_MARKER => {
            detect_smw_us_v1_stage_three_map16_runtime(bytes)?;
            Ok(SmwUsV1Map16RuntimeGeneration::StageThreeLegacy)
        }
        Some(marker) if marker == STAGE_FOUR_MARKER => {
            authenticate_staged_map16_hooks(bytes, &STAGE_FOUR_MARKER, &STAGE_FOUR_HOOK)?;
            Ok(SmwUsV1Map16RuntimeGeneration::StageFourCurrent)
        }
        Some(marker) if marker.starts_with(b"LM") => {
            Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                offset: STAGE_MARKER_OFFSET + 2,
                expected: STAGE_FOUR_MARKER[2],
                actual: marker[2],
            })
        }
        _ => Ok(SmwUsV1Map16RuntimeGeneration::Absent),
    }
}

/// Authenticates Lunar Magic's retained stage-one Map16 auxiliary hook.
///
/// Returns `Ok(false)` only when its exact stage marker is absent.
///
/// # Errors
///
/// Rejects a modified auxiliary prologue or final legacy hook.
pub fn detect_smw_us_v1_stage_one_map16_runtime(
    bytes: &[u8],
) -> Result<bool, SmwUsV1Map16RuntimeDetectError> {
    if bytes.get(STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4) != Some(STAGE_ONE_MARKER.as_slice())
    {
        return Ok(false);
    }
    authenticate_stage_one_map16_hooks(bytes)?;
    Ok(true)
}

/// Authenticates Lunar Magic's retained stage-two Map16 auxiliary and compare hooks.
///
/// Returns `Ok(false)` only when its exact stage marker is absent.
///
/// # Errors
///
/// Rejects a modified auxiliary prologue, compare hook, or final legacy hook.
pub fn detect_smw_us_v1_stage_two_map16_runtime(
    bytes: &[u8],
) -> Result<bool, SmwUsV1Map16RuntimeDetectError> {
    if bytes.get(STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4) != Some(STAGE_TWO_MARKER.as_slice())
    {
        return Ok(false);
    }
    authenticate_stage_two_map16_hooks(bytes)?;
    Ok(true)
}

/// Authenticates Lunar Magic 3.01's retained stage-three Map16 runtime.
///
/// Returns `Ok(false)` only when its exact stage marker is absent.
///
/// # Errors
///
/// Rejects any staged hook or destination mismatch.
pub fn detect_smw_us_v1_stage_three_map16_runtime(
    bytes: &[u8],
) -> Result<bool, SmwUsV1Map16RuntimeDetectError> {
    if bytes.get(STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4)
        != Some(STAGE_THREE_MARKER.as_slice())
    {
        return Ok(false);
    }
    authenticate_staged_map16_hooks(bytes, &STAGE_THREE_MARKER, &STAGE_THREE_HOOK)?;
    Ok(true)
}

/// Builds Lunar Magic's exact stage-three-to-stage-four compatibility upgrade while leaving all
/// Map16 data and allocations untouched. No editor-version stamp is written.
///
/// The returned plan carries a no-op precondition for the full staged hook base, so a change after
/// planning rejects the transaction rather than upgrading hooks authenticated from an older
/// snapshot.
///
/// # Errors
///
/// Rejects a missing or modified stage-three runtime.
pub fn smw_us_v1_stage_three_map16_runtime_migration(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Map16StageThreeMigrationBuildError> {
    if !detect_smw_us_v1_stage_three_map16_runtime(bytes)
        .map_err(SmwUsV1Map16StageThreeMigrationBuildError::Detect)?
    {
        return Err(SmwUsV1Map16StageThreeMigrationBuildError::MissingStageThree);
    }
    let writes = vec![
        PatchWrite {
            offset: STAGED_HOOK_BASE_OFFSET,
            expected: STAGED_HOOK_BASE.to_vec(),
            replacement: STAGED_HOOK_BASE.to_vec(),
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: STAGE_MARKER_OFFSET,
            expected: STAGE_THREE_MARKER.to_vec(),
            replacement: STAGE_FOUR_MARKER.to_vec(),
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: STAGE_FOUR_HOOK_OFFSET,
            expected: STAGE_THREE_HOOK.to_vec(),
            replacement: STAGE_FOUR_HOOK.to_vec(),
            fixups: Vec::new(),
        },
    ];
    Ok(RelocatablePatchPlan {
        description: "Migrate Lunar Magic Map16 runtime stage 3 to stage 4".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: 0..bytes.len(),
            bank_size: None,
            fill_bytes: vec![0, 0xff],
            protected: Vec::new(),
        },
        checksum_field: crate::SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes,
    })
}

/// Builds the exact retained stage-one/two/three-to-stage-four compatibility upgrade.
///
/// All generation-owned bytes are authenticated before planning. Stage one's compare hook is
/// repaired exactly as Lunar Magic does when its discriminator byte is not yet current. The plan
/// keeps no-op preconditions for every authenticated range so changes after planning fail
/// atomically.
///
/// # Errors
///
/// Rejects an absent, current, or modified legacy runtime.
pub fn smw_us_v1_legacy_map16_runtime_migration(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Map16LegacyMigrationBuildError> {
    let generation = probe_smw_us_v1_map16_runtime_generation(bytes)
        .map_err(SmwUsV1Map16LegacyMigrationBuildError::Detect)?;
    let (old_base, old_marker) = match generation {
        SmwUsV1Map16RuntimeGeneration::StageOneLegacy
        | SmwUsV1Map16RuntimeGeneration::StageTwoLegacy => {
            let base = bytes
                .get(STAGED_HOOK_BASE_OFFSET..STAGED_HOOK_BASE_OFFSET + STAGED_HOOK_BASE.len())
                .ok_or_else(|| {
                    SmwUsV1Map16LegacyMigrationBuildError::Detect(
                        SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                            offset: STAGED_HOOK_BASE_OFFSET + STAGED_HOOK_BASE.len() - 1,
                            expected: STAGED_HOOK_BASE[STAGED_HOOK_BASE.len() - 1],
                            actual: 0,
                        },
                    )
                })?;
            let marker = if generation == SmwUsV1Map16RuntimeGeneration::StageOneLegacy {
                STAGE_ONE_MARKER
            } else {
                STAGE_TWO_MARKER
            };
            (base, marker)
        }
        SmwUsV1Map16RuntimeGeneration::StageThreeLegacy => {
            (STAGED_HOOK_BASE.as_slice(), STAGE_THREE_MARKER)
        }
        SmwUsV1Map16RuntimeGeneration::Absent | SmwUsV1Map16RuntimeGeneration::StageFourCurrent => {
            return Err(SmwUsV1Map16LegacyMigrationBuildError::MissingLegacy);
        }
    };
    let mut writes = vec![
        PatchWrite {
            offset: STAGED_HOOK_BASE_OFFSET,
            expected: old_base.to_vec(),
            replacement: STAGED_HOOK_BASE.to_vec(),
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: STAGE_MARKER_OFFSET,
            expected: old_marker.to_vec(),
            replacement: STAGE_FOUR_MARKER.to_vec(),
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: STAGE_FOUR_HOOK_OFFSET,
            expected: STAGE_THREE_HOOK.to_vec(),
            replacement: STAGE_FOUR_HOOK.to_vec(),
            fixups: Vec::new(),
        },
    ];
    if generation == SmwUsV1Map16RuntimeGeneration::StageOneLegacy {
        let expected = bytes
            .get(STAGE_TWO_COMPARE_HOOK_OFFSET..STAGE_TWO_COMPARE_HOOK_OFFSET + 0x0f)
            .ok_or_else(|| {
                SmwUsV1Map16LegacyMigrationBuildError::Detect(
                    SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                        offset: STAGE_TWO_COMPARE_HOOK_OFFSET + 0x0e,
                        expected: STAGE_TWO_COMPARE_HOOK[0x0e],
                        actual: 0,
                    },
                )
            })?
            .to_vec();
        if expected.get(8).copied() != Some(0xc9) {
            writes.push(PatchWrite {
                offset: STAGE_TWO_COMPARE_HOOK_OFFSET,
                expected,
                replacement: STAGE_TWO_COMPARE_HOOK.to_vec(),
                fixups: Vec::new(),
            });
        }
    }
    Ok(RelocatablePatchPlan {
        description: format!("Migrate Lunar Magic Map16 runtime {generation:?} to stage 4"),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: 0..bytes.len(),
            bank_size: None,
            fill_bytes: vec![0, 0xff],
            protected: Vec::new(),
        },
        checksum_field: crate::SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes,
    })
}

fn authenticate_current_map16_runtime(
    bytes: &[u8],
) -> Result<RatsBlock, SmwUsV1Map16RuntimeDetectError> {
    for (offset, current) in
        fixed_patch_replacements().map_err(SmwUsV1Map16RuntimeDetectError::Embedded)?
    {
        if (crate::SMW_US_V1_CHECKSUM_FIELD..crate::SMW_US_V1_CHECKSUM_FIELD + 4).contains(&offset)
            || offset == AUXILIARY_BANK_OPERAND
        {
            continue;
        }
        let expected = current;
        let actual = bytes.get(offset).copied().ok_or(
            SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                offset,
                expected,
                actual: 0,
            },
        )?;
        if actual != expected {
            return Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                offset,
                expected,
                actual,
            });
        }
    }
    let bank = bytes.get(AUXILIARY_BANK_OPERAND).copied().ok_or(
        SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
            offset: AUXILIARY_BANK_OPERAND,
            expected: 0,
            actual: 0,
        },
    )?;
    let payload_offset = snes_to_pc(Mapper::LoRom, u32::from(bank) << 16 | 0x8000)
        .map_err(SmwUsV1Map16RuntimeDetectError::AuxiliaryAddress)?;
    let header_offset = payload_offset.checked_sub(HEADER_LEN).ok_or(
        SmwUsV1Map16RuntimeDetectError::AuxiliaryBeforeHeader(payload_offset),
    )?;
    let block =
        parse_at(bytes, header_offset).map_err(SmwUsV1Map16RuntimeDetectError::AuxiliaryHeader)?;
    if block.payload.start != payload_offset {
        return Err(SmwUsV1Map16RuntimeDetectError::AuxiliaryOwnership {
            expected: payload_offset,
            actual: block.payload.start,
        });
    }
    if block.payload.len() != AUXILIARY_PAYLOAD_LEN {
        return Err(SmwUsV1Map16RuntimeDetectError::AuxiliaryLength(
            block.payload.len(),
        ));
    }
    let expected = decode_auxiliary_payload().map_err(SmwUsV1Map16RuntimeDetectError::Embedded)?;
    if bytes.get(block.payload.clone()) != Some(expected.as_slice()) {
        return Err(SmwUsV1Map16RuntimeDetectError::AuxiliaryPayloadMismatch);
    }
    Ok(block)
}

fn authenticate_staged_map16_hooks(
    bytes: &[u8],
    marker: &[u8],
    final_hook: &[u8],
) -> Result<(), SmwUsV1Map16RuntimeDetectError> {
    authenticate_exact_range(bytes, STAGED_HOOK_BASE_OFFSET, &STAGED_HOOK_BASE)?;
    authenticate_exact_range(bytes, STAGE_MARKER_OFFSET, marker)?;
    authenticate_exact_range(
        bytes,
        STAGE_TWO_COMPARE_HOOK_OFFSET,
        &STAGE_TWO_COMPARE_HOOK,
    )?;
    authenticate_exact_range(bytes, STAGE_FOUR_HOOK_OFFSET, final_hook)
}

fn authenticate_stage_one_map16_hooks(bytes: &[u8]) -> Result<(), SmwUsV1Map16RuntimeDetectError> {
    authenticate_exact_range(bytes, STAGED_HOOK_BASE_OFFSET, &STAGE_ONE_HOOK_BASE)?;
    authenticate_exact_range(bytes, STAGE_MARKER_OFFSET, &STAGE_ONE_MARKER)?;
    if bytes.get(STAGE_TWO_COMPARE_HOOK_OFFSET + 8).copied() == Some(0xc9) {
        authenticate_exact_range(
            bytes,
            STAGE_TWO_COMPARE_HOOK_OFFSET,
            &STAGE_TWO_COMPARE_HOOK,
        )?;
    }
    authenticate_exact_range(bytes, STAGE_FOUR_HOOK_OFFSET, &STAGE_THREE_HOOK)
}

fn authenticate_stage_two_map16_hooks(bytes: &[u8]) -> Result<(), SmwUsV1Map16RuntimeDetectError> {
    authenticate_exact_range(bytes, STAGED_HOOK_BASE_OFFSET, &STAGE_ONE_HOOK_BASE)?;
    authenticate_exact_range(bytes, STAGE_MARKER_OFFSET, &STAGE_TWO_MARKER)?;
    authenticate_exact_range(
        bytes,
        STAGE_TWO_COMPARE_HOOK_OFFSET,
        &STAGE_TWO_COMPARE_HOOK,
    )?;
    authenticate_exact_range(bytes, STAGE_FOUR_HOOK_OFFSET, &STAGE_THREE_HOOK)
}

fn authenticate_exact_range(
    bytes: &[u8],
    offset: usize,
    expected: &[u8],
) -> Result<(), SmwUsV1Map16RuntimeDetectError> {
    for (index, expected) in expected.iter().copied().enumerate() {
        let actual = bytes.get(offset + index).copied().unwrap_or(0);
        if actual != expected {
            return Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch {
                offset: offset + index,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

impl fmt::Display for SmwUsV1Map16RuntimeInstallBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot build SMW-US Map16 runtime installation: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Map16RuntimeInstallBuildError {}

impl From<IpsError> for SmwUsV1Map16RuntimeInstallBuildError {
    fn from(value: IpsError) -> Self {
        Self::Ips(value)
    }
}

/// Builds the exact Lunar Magic 3.63 runtime transformation recovered from an unchanged complete
/// Map16 import into authenticated pristine SMW-US.
///
/// The bundled IPS data describes only fixed revision bytes within the original 512 KiB image.
/// The `$8000`-byte auxiliary table is allocated independently, and its one varying bank operand
/// is emitted as a typed relocation. The caller remains responsible for authenticating `pristine`
/// as SMW-US revision 0 before offering this operation.
///
/// # Errors
///
/// Rejects a source other than the authenticated 512-KiB or 1-MiB shapes, malformed embedded
/// patch data, an unexpected patched length, or loss of the recovered auxiliary relocation site.
pub fn smw_us_v1_map16_runtime_installation_plan(
    source: &[u8],
    mut allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1Map16RuntimeInstallBuildError> {
    if !matches!(source.len(), PRISTINE_LOGICAL_LEN | EXPANDED_LOGICAL_LEN) {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::PristineLength(
            source.len(),
        ));
    }
    let patch = decode_base64(FIXED_PATCH_BASE64)?;
    let patched = apply_ips(source, &patch)?;
    if patched.len() != source.len() {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::PatchedLength(
            patched.len(),
        ));
    }
    let auxiliary = decode_auxiliary_payload()?;
    let writes = changed_patch_writes(source, &patched, checksum_field)?;
    // Lunar Magic places the eight-byte RATS header immediately before a complete `$8000`-byte
    // payload bank. The generic allocator's bank constraint includes the header, so reserve each
    // preceding partial bank explicitly and allocate this exceptional cross-boundary block without
    // its ordinary same-bank rule. Find the first complete virtual/source fill run explicitly;
    // protecting only the prefix before that run avoids also protecting an earlier payload bank.
    let search = allocation.search.clone();
    let first = search
        .start
        .checked_add(0x8000)
        .ok_or(SmwUsV1Map16RuntimeInstallBuildError::MissingAlignedAuxiliarySpace)?;
    let last = search
        .end
        .checked_sub(AUXILIARY_PAYLOAD_LEN)
        .ok_or(SmwUsV1Map16RuntimeInstallBuildError::MissingAlignedAuxiliarySpace)?;
    let payload_start = (first..=last)
        .step_by(0x8000)
        .find(|payload_start| {
            (payload_start - lm_rats::HEADER_LEN..payload_start + AUXILIARY_PAYLOAD_LEN).all(
                |offset| {
                    source
                        .get(offset)
                        .copied()
                        .is_none_or(|byte| allocation.fill_bytes.contains(&byte))
                },
            )
        })
        .ok_or(SmwUsV1Map16RuntimeInstallBuildError::MissingAlignedAuxiliarySpace)?;
    allocation.protected.push(ProtectedRange(
        search.start..payload_start - lm_rats::HEADER_LEN,
    ));
    allocation.bank_size = None;
    Ok(RelocatablePatchPlan {
        description: "Install Lunar Magic Map16 runtime".into(),
        mapper: Mapper::LoRom,
        allocation,
        checksum_field,
        expansion_fill: 0,
        payloads: vec![PatchPayload {
            bytes: auxiliary,
            fixups: Vec::new(),
        }],
        writes,
    })
}

fn decode_auxiliary_payload() -> Result<Vec<u8>, SmwUsV1Map16RuntimeInstallBuildError> {
    let compressed = decode_base64(AUXILIARY_PAYLOAD_GZIP_BASE64)?;
    let mut decoder = flate2::read::GzDecoder::new(compressed.as_slice()).take(32_769);
    let mut payload = Vec::with_capacity(AUXILIARY_PAYLOAD_LEN);
    decoder
        .read_to_end(&mut payload)
        .map_err(SmwUsV1Map16RuntimeInstallBuildError::AuxiliaryIo)?;
    if payload.len() != AUXILIARY_PAYLOAD_LEN {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::AuxiliaryLength(
            payload.len(),
        ));
    }
    Ok(payload)
}

fn fixed_patch_replacements() -> Result<Vec<(usize, u8)>, SmwUsV1Map16RuntimeInstallBuildError> {
    let patch = decode_base64(FIXED_PATCH_BASE64)?;
    let zero = apply_ips(&vec![0; PRISTINE_LOGICAL_LEN], &patch)?;
    let ones = apply_ips(&vec![0xff; PRISTINE_LOGICAL_LEN], &patch)?;
    if zero.len() != PRISTINE_LOGICAL_LEN || ones.len() != PRISTINE_LOGICAL_LEN {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::PatchedLength(
            zero.len().max(ones.len()),
        ));
    }
    Ok(zero
        .into_iter()
        .zip(ones)
        .enumerate()
        .filter_map(|(offset, (zero, ones))| (zero == ones).then_some((offset, zero)))
        .collect())
}

fn changed_patch_writes(
    pristine: &[u8],
    patched: &[u8],
    checksum_field: usize,
) -> Result<Vec<PatchWrite>, SmwUsV1Map16RuntimeInstallBuildError> {
    let checksum = checksum_field..checksum_field + 4;
    let mut writes = Vec::new();
    let mut cursor = 0;
    let mut found_auxiliary_fixup = false;
    while cursor < pristine.len() {
        if pristine[cursor] == patched[cursor] || checksum.contains(&cursor) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < pristine.len()
            && pristine[cursor] != patched[cursor]
            && !checksum.contains(&cursor)
        {
            cursor += 1;
        }
        let mut fixups = Vec::new();
        if (start..cursor).contains(&AUXILIARY_BANK_OPERAND) {
            found_auxiliary_fixup = true;
            fixups.push(PatchFixup {
                offset: AUXILIARY_BANK_OPERAND - start,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Bank8LowBank,
            });
        }
        writes.push(PatchWrite {
            offset: start,
            expected: pristine[start..cursor].to_vec(),
            replacement: patched[start..cursor].to_vec(),
            fixups,
        });
    }
    if !found_auxiliary_fixup {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::MissingAuxiliaryBankOperand);
    }
    Ok(writes)
}

fn decode_base64(text: &str) -> Result<Vec<u8>, SmwUsV1Map16RuntimeInstallBuildError> {
    let symbols = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if symbols.is_empty() || symbols.len() % 4 != 0 {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64);
    }
    let mut decoded = Vec::with_capacity(symbols.len() / 4 * 3);
    for quartet in symbols.chunks_exact(4) {
        let padding = usize::from(quartet[3] == b'=') + usize::from(quartet[2] == b'=');
        let mut value = 0_u32;
        for (index, symbol) in quartet.iter().copied().enumerate() {
            let sextet = if symbol == b'=' {
                if index < 2 {
                    return Err(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64);
                }
                0
            } else {
                u32::from(
                    base64_sextet(symbol)
                        .ok_or(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64)?,
                )
            };
            value = value << 6 | sextet;
        }
        let bytes = value.to_be_bytes();
        decoded.extend_from_slice(&bytes[1..4 - padding]);
    }
    Ok(decoded)
}

const fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum, snes_to_pc};

    fn allocation() -> AllocationPolicy {
        AllocationPolicy {
            search: 0x80_000..0x10_0000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0, 0xff],
            protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
        }
    }

    #[test]
    fn embedded_patch_decodes_and_installs_the_relocated_wine_shape() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let plan =
            smw_us_v1_map16_runtime_installation_plan(&original, allocation(), 0x7fdc).unwrap();
        assert_eq!(plan.payloads[0].bytes.len(), 0x8000);
        let relocated = plan
            .writes
            .iter()
            .filter_map(|write| write.fixups.first().map(|fixup| (write, fixup)))
            .collect::<Vec<_>>();
        assert_eq!(relocated.len(), 1);
        assert_eq!(relocated[0].0.offset + relocated[0].1.offset, 0x37_626);
        assert_eq!(relocated[0].1.target_payload, 0);
        assert_eq!(relocated[0].1.encoding, PatchFixupEncoding::Bank8LowBank);

        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks[0].payload, 0x88_000..0x90_000);
        assert_eq!(
            snes_to_pc(Mapper::LoRom, result.snes_addresses[0]).unwrap(),
            0x88_000
        );
        assert_eq!(
            project.rom.logical_bytes()[super::super::native_map16_secondary::SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET],
            0x22
        );
        let secondary =
            super::super::native_map16_secondary::load_smw_us_v1_secondary_map16(&project).unwrap();
        assert!(secondary.installed);
        assert!(secondary.blocks.iter().all(Option::is_none));
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            checksum.encoded()
        );
        assert_eq!(
            detect_smw_us_v1_current_map16_runtime(project.rom.logical_bytes())
                .unwrap()
                .unwrap()
                .payload,
            0x88_000..0x90_000
        );
    }

    #[test]
    fn malformed_source_is_rejected_before_a_plan_exists() {
        assert!(matches!(
            smw_us_v1_map16_runtime_installation_plan(&[0; 1], allocation(), 0x7fdc),
            Err(SmwUsV1Map16RuntimeInstallBuildError::PristineLength(1))
        ));
    }

    #[test]
    fn builtin_plan_uses_the_recovered_expansion_window() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let plan = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        assert_eq!(plan.allocation.search, 0x80_000..0x10_0000);
        assert_eq!(plan.checksum_field, crate::SMW_US_V1_CHECKSUM_FIELD);
        assert_eq!(plan.payloads[0].bytes.len(), AUXILIARY_PAYLOAD_LEN);
    }

    #[test]
    fn occupied_one_megabyte_source_expands_to_two_and_uses_the_next_auxiliary_bank() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = RomImage::from_bytes(original).unwrap();
        image
            .expand(Mapper::LoRom, EXPANDED_LOGICAL_LEN, 0xa5)
            .unwrap();
        image.update_snes_checksum(0x7fdc).unwrap();
        let expanded = image.logical_bytes().to_vec();

        let plan = smw_us_v1_builtin_map16_runtime_installation_plan(&expanded).unwrap();
        assert_eq!(plan.allocation.search, 0x80_000..0x200_000);
        let mut project = Project::new(RomImage::from_bytes(expanded.clone()).unwrap());
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(project.rom.logical_len(), 0x200_000);
        assert_eq!(result.blocks[0].header_offset, 0x107ff8);
        assert_eq!(result.blocks[0].payload, 0x108000..0x110000);
        assert_eq!(
            detect_smw_us_v1_current_map16_runtime(project.rom.logical_bytes())
                .unwrap()
                .unwrap()
                .payload,
            0x108000..0x110000
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), expanded);
    }

    #[test]
    fn current_detector_rejects_marker_only_fixed_and_payload_modifications() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        assert!(
            detect_smw_us_v1_current_map16_runtime(&original)
                .unwrap()
                .is_none()
        );

        let mut marker_only = original.clone();
        marker_only[super::super::native_map16_secondary::SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET] = 0x22;
        assert!(matches!(
            detect_smw_us_v1_current_map16_runtime(&marker_only),
            Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch { .. })
        ));

        let plan = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let installed = project.install_relocatable_patch(&plan).unwrap();
        let fixed_offset = plan
            .writes
            .iter()
            .find(|write| {
                let range = write.offset..write.offset + write.replacement.len();
                !range
                    .contains(&super::super::native_map16_secondary::SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET)
                    && !range.contains(&AUXILIARY_BANK_OPERAND)
            })
            .unwrap()
            .offset;
        let mut modified = project.rom.logical_bytes().to_vec();
        modified[fixed_offset] ^= 1;
        assert!(matches!(
            detect_smw_us_v1_current_map16_runtime(&modified),
            Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch { offset, .. })
                if offset == fixed_offset
        ));
        let mut modified = project.rom.logical_bytes().to_vec();
        modified[installed.blocks[0].payload.start] ^= 1;
        assert!(matches!(
            detect_smw_us_v1_current_map16_runtime(&modified),
            Err(SmwUsV1Map16RuntimeDetectError::AuxiliaryPayloadMismatch)
        ));
    }

    fn legacy_map16_fixture(generation: SmwUsV1Map16RuntimeGeneration) -> Vec<u8> {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let install = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        let mut installed = Project::new(RomImage::from_bytes(original).unwrap());
        installed.install_relocatable_patch(&install).unwrap();
        let mut bytes = installed.save_snapshot();
        bytes[STAGED_HOOK_BASE_OFFSET..STAGED_HOOK_BASE_OFFSET + STAGE_ONE_HOOK_BASE.len()]
            .copy_from_slice(&STAGE_ONE_HOOK_BASE);
        bytes[STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_THREE_HOOK.len()]
            .copy_from_slice(&STAGE_THREE_HOOK);
        match generation {
            SmwUsV1Map16RuntimeGeneration::StageOneLegacy => {
                bytes[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4]
                    .copy_from_slice(&STAGE_ONE_MARKER);
                bytes[STAGE_TWO_COMPARE_HOOK_OFFSET + 8] = 0xff;
            }
            SmwUsV1Map16RuntimeGeneration::StageTwoLegacy => {
                bytes[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + 4]
                    .copy_from_slice(&STAGE_TWO_MARKER);
                bytes[STAGE_TWO_COMPARE_HOOK_OFFSET
                    ..STAGE_TWO_COMPARE_HOOK_OFFSET + STAGE_TWO_COMPARE_HOOK.len()]
                    .copy_from_slice(&STAGE_TWO_COMPARE_HOOK);
            }
            _ => panic!("fixture helper requires stage one or two"),
        }
        let checksum = compute_snes_checksum(&bytes, crate::SMW_US_V1_CHECKSUM_FIELD).unwrap();
        bytes[crate::SMW_US_V1_CHECKSUM_FIELD..crate::SMW_US_V1_CHECKSUM_FIELD + 4]
            .copy_from_slice(&checksum.encoded());
        bytes
    }

    #[test]
    fn stage_one_and_two_runtimes_are_authenticated_migrated_and_undo_exactly() {
        for generation in [
            SmwUsV1Map16RuntimeGeneration::StageOneLegacy,
            SmwUsV1Map16RuntimeGeneration::StageTwoLegacy,
        ] {
            let legacy = legacy_map16_fixture(generation);
            assert_eq!(
                probe_smw_us_v1_map16_runtime_generation(&legacy).unwrap(),
                generation
            );
            assert_eq!(
                detect_smw_us_v1_stage_one_map16_runtime(&legacy).unwrap(),
                generation == SmwUsV1Map16RuntimeGeneration::StageOneLegacy
            );
            assert_eq!(
                detect_smw_us_v1_stage_two_map16_runtime(&legacy).unwrap(),
                generation == SmwUsV1Map16RuntimeGeneration::StageTwoLegacy
            );

            let before = legacy.clone();
            let plan = smw_us_v1_legacy_map16_runtime_migration(&legacy).unwrap();
            assert!(plan.payloads.is_empty());
            let mut project = Project::new(RomImage::from_bytes(legacy).unwrap());
            project.install_relocatable_patch(&plan).unwrap();
            assert_eq!(
                probe_smw_us_v1_map16_runtime_generation(project.rom.logical_bytes()).unwrap(),
                SmwUsV1Map16RuntimeGeneration::StageFourCurrent
            );
            assert_eq!(
                &project.rom.logical_bytes()[STAGE_TWO_COMPARE_HOOK_OFFSET
                    ..STAGE_TWO_COMPARE_HOOK_OFFSET + STAGE_TWO_COMPARE_HOOK.len()],
                STAGE_TWO_COMPARE_HOOK
            );
            assert_eq!(project.history.undo_len(), 1);
            project.undo().unwrap();
            assert_eq!(project.save_snapshot(), before);
        }
    }

    #[test]
    fn early_stage_detectors_reject_owned_hook_corruption() {
        let mut stage_one = legacy_map16_fixture(SmwUsV1Map16RuntimeGeneration::StageOneLegacy);
        stage_one[STAGED_HOOK_BASE_OFFSET + 3] ^= 1;
        assert!(matches!(
            probe_smw_us_v1_map16_runtime_generation(&stage_one),
            Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch { offset, .. })
                if offset == STAGED_HOOK_BASE_OFFSET + 3
        ));

        let mut stage_two = legacy_map16_fixture(SmwUsV1Map16RuntimeGeneration::StageTwoLegacy);
        stage_two[STAGE_TWO_COMPARE_HOOK_OFFSET + 4] ^= 1;
        assert!(matches!(
            probe_smw_us_v1_map16_runtime_generation(&stage_two),
            Err(SmwUsV1Map16RuntimeDetectError::FixedByteMismatch { offset, .. })
                if offset == STAGE_TWO_COMPARE_HOOK_OFFSET + 4
        ));
    }

    #[test]
    fn early_stage_migration_rejects_changes_after_planning() {
        let legacy = legacy_map16_fixture(SmwUsV1Map16RuntimeGeneration::StageOneLegacy);
        let plan = smw_us_v1_legacy_map16_runtime_migration(&legacy).unwrap();
        for offset in [
            STAGED_HOOK_BASE_OFFSET + STAGE_ONE_HOOK_BASE.len(),
            STAGE_TWO_COMPARE_HOOK_OFFSET,
            STAGE_FOUR_HOOK_OFFSET,
        ] {
            let mut changed = legacy.clone();
            changed[offset] ^= 1;
            let snapshot = changed.clone();
            let mut project = Project::new(RomImage::from_bytes(changed).unwrap());
            assert!(project.install_relocatable_patch(&plan).is_err());
            assert_eq!(project.history.undo_len(), 0);
            assert_eq!(project.save_snapshot(), snapshot);
        }
    }

    #[test]
    fn stage_three_runtime_is_authenticated_migrated_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let install = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        let mut installed = Project::new(RomImage::from_bytes(original).unwrap());
        installed.install_relocatable_patch(&install).unwrap();
        let mut stage_three = installed.save_snapshot();
        stage_three[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + STAGE_THREE_MARKER.len()]
            .copy_from_slice(&STAGE_THREE_MARKER);
        stage_three[STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_THREE_HOOK.len()]
            .copy_from_slice(&STAGE_THREE_HOOK);
        let checksum =
            compute_snes_checksum(&stage_three, crate::SMW_US_V1_CHECKSUM_FIELD).unwrap();
        stage_three[crate::SMW_US_V1_CHECKSUM_FIELD..crate::SMW_US_V1_CHECKSUM_FIELD + 4]
            .copy_from_slice(&checksum.encoded());

        assert_eq!(
            probe_smw_us_v1_map16_runtime_generation(&stage_three).unwrap(),
            SmwUsV1Map16RuntimeGeneration::StageThreeLegacy
        );
        assert!(detect_smw_us_v1_stage_three_map16_runtime(&stage_three).unwrap());
        let before = stage_three.clone();
        let plan = smw_us_v1_stage_three_map16_runtime_migration(&stage_three).unwrap();
        assert!(plan.payloads.is_empty());
        assert!(plan.writes.iter().any(|write| {
            write.offset <= STAGE_MARKER_OFFSET
                && STAGE_MARKER_OFFSET < write.offset + write.replacement.len()
        }));
        assert!(plan.writes.iter().any(|write| {
            write.offset <= STAGE_FOUR_HOOK_OFFSET
                && STAGE_FOUR_HOOK_OFFSET < write.offset + write.replacement.len()
        }));

        let mut project = Project::new(RomImage::from_bytes(stage_three).unwrap());
        project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            probe_smw_us_v1_map16_runtime_generation(project.rom.logical_bytes()).unwrap(),
            SmwUsV1Map16RuntimeGeneration::StageFourCurrent
        );
        assert_eq!(
            &project.rom.logical_bytes()
                [STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + STAGE_FOUR_MARKER.len()],
            STAGE_FOUR_MARKER
        );
        assert_eq!(
            &project.rom.logical_bytes()
                [STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_FOUR_HOOK.len()],
            STAGE_FOUR_HOOK
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), before);
    }

    #[test]
    fn stage_three_migration_rejects_staged_hooks_changed_after_planning() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let install = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        let mut installed = Project::new(RomImage::from_bytes(original).unwrap());
        installed.install_relocatable_patch(&install).unwrap();
        let mut stage_three = installed.save_snapshot();
        stage_three[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + STAGE_THREE_MARKER.len()]
            .copy_from_slice(&STAGE_THREE_MARKER);
        stage_three[STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_THREE_HOOK.len()]
            .copy_from_slice(&STAGE_THREE_HOOK);
        let plan = smw_us_v1_stage_three_map16_runtime_migration(&stage_three).unwrap();

        for offset in [STAGED_HOOK_BASE_OFFSET, STAGE_FOUR_HOOK_OFFSET] {
            let mut changed = stage_three.clone();
            changed[offset] ^= 1;
            let snapshot = changed.clone();
            let mut project = Project::new(RomImage::from_bytes(changed).unwrap());
            assert!(project.install_relocatable_patch(&plan).is_err());
            assert_eq!(project.history.undo_len(), 0);
            assert_eq!(project.save_snapshot(), snapshot);
        }
    }

    #[test]
    fn external_stage_three_oracle_matches_lunar_magic_upgrade_bytes() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let install = smw_us_v1_builtin_map16_runtime_installation_plan(&original).unwrap();
        let mut installed = Project::new(RomImage::from_bytes(original).unwrap());
        installed.install_relocatable_patch(&install).unwrap();
        let mut stage_three = installed.save_snapshot();
        stage_three[STAGE_MARKER_OFFSET..STAGE_MARKER_OFFSET + STAGE_THREE_MARKER.len()]
            .copy_from_slice(&STAGE_THREE_MARKER);
        stage_three[STAGE_FOUR_HOOK_OFFSET..STAGE_FOUR_HOOK_OFFSET + STAGE_THREE_HOOK.len()]
            .copy_from_slice(&STAGE_THREE_HOOK);
        let checksum =
            compute_snes_checksum(&stage_three, crate::SMW_US_V1_CHECKSUM_FIELD).unwrap();
        stage_three[crate::SMW_US_V1_CHECKSUM_FIELD..crate::SMW_US_V1_CHECKSUM_FIELD + 4]
            .copy_from_slice(&checksum.encoded());

        let before = RomImage::from_bytes(stage_three.clone()).unwrap();
        assert_eq!(
            probe_smw_us_v1_map16_runtime_generation(before.logical_bytes()).unwrap(),
            SmwUsV1Map16RuntimeGeneration::StageThreeLegacy
        );
        let mut physical_before = vec![0; 0x200];
        physical_before.extend_from_slice(&stage_three);
        let oracle_patch = decode_base64(STAGE3_TO_STAGE4_ORACLE_IPS_BASE64).unwrap();
        let oracle_after = apply_ips(&physical_before, &oracle_patch).unwrap();
        let oracle_after = RomImage::from_bytes(oracle_after).unwrap();
        assert_eq!(
            probe_smw_us_v1_map16_runtime_generation(oracle_after.logical_bytes()).unwrap(),
            SmwUsV1Map16RuntimeGeneration::StageFourCurrent
        );

        let plan = smw_us_v1_stage_three_map16_runtime_migration(before.logical_bytes()).unwrap();
        let mut project = Project::new(before);
        project.install_relocatable_patch(&plan).unwrap();

        let editor_only = [
            0x7fdc..0x7fe0,
            0x7e_ff8..0x7e_fff,
            0x7f_0b6..0x7f_0b8,
            0x7f_0c3..0x7f_0c5,
        ];
        for (offset, (&actual, &expected)) in project
            .rom
            .logical_bytes()
            .iter()
            .zip(oracle_after.logical_bytes())
            .enumerate()
        {
            if !editor_only.iter().any(|range| range.contains(&offset)) {
                assert_eq!(actual, expected, "oracle mismatch at logical {offset:#x}");
            }
        }
    }
}
