//! Authenticated migration of Lunar Magic's legacy expanded-ExAnimation pointer fragments.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};
use std::fmt;

const RUNTIME_HOOK_OFFSET: usize = 0x0002_83ad;
const LEGACY_BANK_OFFSETS: [usize; 2] = [0x92, 0x118];
const LEGACY_MARKER_OFFSET: usize = 0x169;
const LEGACY_MARKER: [u8; 4] = [0x4c, 0x4d, 0x00, 0x01];
const CURRENT_MARKER: [u8; 4] = [0x4c, 0x4d, 0x01, 0x01];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1LegacyExAnimationHookMigration {
    pub runtime: RatsBlock,
    pub plan: RelocatablePatchPlan,
}

#[derive(Debug)]
pub enum SmwUsV1LegacyExAnimationHookMigrationError {
    MissingHook,
    HookAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership { expected: usize, actual: usize },
    RuntimeTooShort { required: usize, actual: usize },
    MarkerMismatch,
}

impl fmt::Display for SmwUsV1LegacyExAnimationHookMigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "legacy ExAnimation hook migration failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1LegacyExAnimationHookMigrationError {}

/// Authenticates and plans `PatchLegacyExAnimationPointerHooks` (`0045E5F0`).
///
/// Lunar Magic resolves the old runtime through the long-call operand at `$0283AD`, requires the
/// marker `4C 4D 00 01` at runtime `+$169`, writes bank `$10` at `+$92` and `+$118`, and advances
/// the marker to `4C 4D 01 01`. Rust additionally requires the resolved payload to retain its RATS
/// owner so a coincidental marker cannot authorize writes into unrelated ROM bytes.
///
/// # Errors
///
/// Rejects an absent/non-JSL hook, unmappable or unowned runtime, undersized payload, or any marker
/// other than the exact authenticated legacy generation.
pub fn smw_us_v1_legacy_exanimation_hook_migration(
    bytes: &[u8],
) -> Result<SmwUsV1LegacyExAnimationHookMigration, SmwUsV1LegacyExAnimationHookMigrationError> {
    let hook = bytes
        .get(RUNTIME_HOOK_OFFSET..RUNTIME_HOOK_OFFSET + 4)
        .filter(|hook| hook[0] == 0x22)
        .ok_or(SmwUsV1LegacyExAnimationHookMigrationError::MissingHook)?;
    let address = u32::from(hook[1]) | u32::from(hook[2]) << 8 | u32::from(hook[3]) << 16;
    let runtime_offset = snes_to_pc(Mapper::LoRom, address)
        .map_err(SmwUsV1LegacyExAnimationHookMigrationError::HookAddress)?;
    let header_offset = runtime_offset
        .checked_sub(HEADER_LEN)
        .ok_or(SmwUsV1LegacyExAnimationHookMigrationError::RuntimeBeforeHeader(runtime_offset))?;
    let runtime = parse_at(bytes, header_offset)
        .map_err(SmwUsV1LegacyExAnimationHookMigrationError::RuntimeHeader)?;
    if runtime.payload.start != runtime_offset {
        return Err(
            SmwUsV1LegacyExAnimationHookMigrationError::RuntimeOwnership {
                expected: runtime_offset,
                actual: runtime.payload.start,
            },
        );
    }
    let required = LEGACY_MARKER_OFFSET + LEGACY_MARKER.len();
    if runtime.payload.len() < required {
        return Err(
            SmwUsV1LegacyExAnimationHookMigrationError::RuntimeTooShort {
                required,
                actual: runtime.payload.len(),
            },
        );
    }
    if bytes.get(runtime_offset + LEGACY_MARKER_OFFSET..runtime_offset + required)
        != Some(&LEGACY_MARKER)
    {
        return Err(SmwUsV1LegacyExAnimationHookMigrationError::MarkerMismatch);
    }

    let mut writes = LEGACY_BANK_OFFSETS
        .into_iter()
        .map(|relative| PatchWrite {
            offset: runtime_offset + relative,
            expected: vec![bytes[runtime_offset + relative]],
            replacement: vec![0x10],
            fixups: Vec::new(),
        })
        .collect::<Vec<_>>();
    writes.push(PatchWrite {
        offset: runtime_offset + LEGACY_MARKER_OFFSET,
        expected: LEGACY_MARKER.to_vec(),
        replacement: CURRENT_MARKER.to_vec(),
        fixups: Vec::new(),
    });
    Ok(SmwUsV1LegacyExAnimationHookMigration {
        runtime,
        plan: RelocatablePatchPlan {
            description: "migrate SMW US v1 legacy ExAnimation pointer hooks".into(),
            mapper: Mapper::LoRom,
            // This migration allocates nothing. A minimal in-image policy lets the shared guarded
            // write transaction validate without expanding or treating unrelated space as free.
            allocation: AllocationPolicy {
                search: 0..1,
                bank_size: None,
                fill_bytes: vec![0xff],
                protected: Vec::new(),
            },
            checksum_field: SMW_US_V1_CHECKSUM_FIELD,
            expansion_fill: 0xff,
            payloads: Vec::new(),
            writes,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchError};
    use lm_rats::make_header;
    use lm_rom::{RomImage, SnesChecksum, pc_to_snes};

    fn legacy_logical_rom() -> (Vec<u8>, usize) {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(pristine).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        bytes.resize(0x10_0000, 0xff);
        let header = 0x8_0000;
        let payload_len = 0x200;
        bytes[header..header + HEADER_LEN].copy_from_slice(&make_header(payload_len).unwrap());
        let runtime = header + HEADER_LEN;
        bytes[runtime..runtime + payload_len].fill(0x5a);
        bytes[runtime + 0x92] = 0x08;
        bytes[runtime + 0x118] = 0x08;
        bytes[runtime + LEGACY_MARKER_OFFSET..runtime + LEGACY_MARKER_OFFSET + 4]
            .copy_from_slice(&LEGACY_MARKER);
        let low_bank = pc_to_snes(Mapper::LoRom, runtime).unwrap() & 0x7f_ffff;
        bytes[RUNTIME_HOOK_OFFSET..RUNTIME_HOOK_OFFSET + 4].copy_from_slice(&[
            0x22,
            low_bank as u8,
            (low_bank >> 8) as u8,
            (low_bank >> 16) as u8,
        ]);
        (bytes, runtime)
    }

    #[test]
    fn authenticated_legacy_fragments_migrate_header_variants_and_undo_exactly() {
        let (logical, runtime) = legacy_logical_rom();
        let mut headered = vec![0; 0x200];
        headered.extend_from_slice(&logical);
        for original in [logical, headered] {
            let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
            let migration =
                smw_us_v1_legacy_exanimation_hook_migration(project.rom.logical_bytes()).unwrap();
            assert_eq!(migration.runtime.payload.start, runtime);
            let result = project.install_relocatable_patch(&migration.plan).unwrap();
            assert!(result.blocks.is_empty());
            assert_eq!(project.rom.read(runtime + 0x92, 1).unwrap(), &[0x10]);
            assert_eq!(project.rom.read(runtime + 0x118, 1).unwrap(), &[0x10]);
            assert_eq!(
                project.rom.read(runtime + LEGACY_MARKER_OFFSET, 4).unwrap(),
                CURRENT_MARKER
            );
            assert!(
                SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                    .unwrap()
                    .is_complementary()
            );
            project.undo().unwrap();
            assert_eq!(project.save_snapshot(), original);
        }
    }

    #[test]
    fn marker_and_late_fragment_changes_are_rejected_without_mutation() {
        let (mut wrong_marker, runtime) = legacy_logical_rom();
        wrong_marker[runtime + LEGACY_MARKER_OFFSET] ^= 1;
        assert!(matches!(
            smw_us_v1_legacy_exanimation_hook_migration(&wrong_marker),
            Err(SmwUsV1LegacyExAnimationHookMigrationError::MarkerMismatch)
        ));

        let (original, runtime) = legacy_logical_rom();
        let migration = smw_us_v1_legacy_exanimation_hook_migration(&original).unwrap();
        let mut changed = original.clone();
        changed[runtime + 0x118] ^= 1;
        let mut project = Project::new(RomImage::from_bytes(changed.clone()).unwrap());
        assert!(matches!(
            project.install_relocatable_patch(&migration.plan),
            Err(RelocatablePatchError::HookPreconditionMismatch { .. })
        ));
        assert_eq!(project.save_snapshot(), changed);
        assert!(!project.undo().unwrap());
    }
}
