//! Generated base helper and fixed pointer publications for expanded level settings.

use crate::{ExpandedSettingsRuntimeLayout, SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS};
use lm_project::PatchWrite;
use lm_rom::{Mapper, RomError, pc_to_snes};
use lm_snes::{CodeBuilder, CodeBuilderError};

/// Logical destination of descriptor `$70` in the SMW US revision-0 layout.
pub const SMW_US_V1_EXPANDED_SETTINGS_BASE_HELPER_OFFSET: usize = 0x06_f0f0;

/// Five descriptor `$35..$39` operands patched to descriptor `$70`.
pub const SMW_US_V1_EXPANDED_SETTINGS_BASE_POINTER_OFFSETS: [usize; 5] =
    [0x06_a4c1, 0x06_c206, 0x06_ce06, 0x06_da06, 0x06_e906];

/// Persistent descriptor `$71` hook that enters the final descriptor `$72` runtime.
pub const SMW_US_V1_EXPANDED_SETTINGS_BASE_HOOK_OFFSET: usize = 0x00_2a50;

const BASE_POINTER_EXPECTED: [u8; 3] = [0xe3, 0xb3, 0x0d];
const BASE_HOOK_EXPECTED: [u8; 4] = [0xa2, 0x03, 0xb5, 0x04];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsBaseError {
    Code(CodeBuilderError),
    MissingTargetDescriptor {
        descriptor_index: usize,
    },
    InvalidTargetAddress {
        descriptor_index: usize,
        destination_offset: usize,
        source: RomError,
    },
}

impl std::fmt::Display for ExpandedSettingsBaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings base installation failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsBaseError {}

impl From<CodeBuilderError> for ExpandedSettingsBaseError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

/// Independently generates descriptor `$70`.
///
/// The helper snapshots direct-page words `$57/$59` into `$FA/$FB` and returns. Lunar Magic
/// reserves 16 bytes for the entry, so the unused tail remains `$FF`.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_base_helper() -> Result<Vec<u8>, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    code.lda_direct_page(0x57);
    code.sta_direct_page(0xfa);
    code.lda_direct_page(0x59);
    code.sta_direct_page(0xfb);
    code.rts();
    let mut bytes = code.finish()?.bytes;
    bytes.resize(0x10, 0xff);
    Ok(bytes)
}

/// Produces descriptor `$70`, all five fixed pointer operands, and the descriptor `$71` hook.
///
/// Every write carries the exact pristine-ROM bytes. Target addresses are derived from the
/// revision layout and published through Lunar Magic's low-bank `LoROM` mirror.
///
/// # Errors
///
/// Rejects an unmappable `$70` destination, a missing `$72` component, or an unmappable `$72`
/// destination.
pub fn smw_us_v1_expanded_settings_base_writes(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<Vec<PatchWrite>, ExpandedSettingsBaseError> {
    let base_target = low_bank_lorom_address(0x70, SMW_US_V1_EXPANDED_SETTINGS_BASE_HELPER_OFFSET)?;
    let mut writes = Vec::with_capacity(7);
    writes.push(PatchWrite {
        offset: SMW_US_V1_EXPANDED_SETTINGS_BASE_HELPER_OFFSET,
        expected: vec![0xff; 0x10],
        replacement: smw_us_v1_expanded_settings_base_helper()?,
        fixups: Vec::new(),
    });
    for offset in SMW_US_V1_EXPANDED_SETTINGS_BASE_POINTER_OFFSETS {
        writes.push(PatchWrite {
            offset,
            expected: BASE_POINTER_EXPECTED.to_vec(),
            replacement: base_target.to_vec(),
            fixups: Vec::new(),
        });
    }

    let target_descriptor_index = 0x72;
    let target_slot = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .position(|block| block.descriptor_index == target_descriptor_index)
        .ok_or(ExpandedSettingsBaseError::MissingTargetDescriptor {
            descriptor_index: target_descriptor_index,
        })?;
    let target = low_bank_lorom_address(
        target_descriptor_index,
        layout.destination_offsets[target_slot],
    )?;
    let mut hook = vec![0x22];
    hook.extend_from_slice(&target);
    writes.push(PatchWrite {
        offset: SMW_US_V1_EXPANDED_SETTINGS_BASE_HOOK_OFFSET,
        expected: BASE_HOOK_EXPECTED.to_vec(),
        replacement: hook,
        fixups: Vec::new(),
    });
    writes.sort_unstable_by_key(|write| write.offset);
    Ok(writes)
}

fn low_bank_lorom_address(
    descriptor_index: usize,
    destination_offset: usize,
) -> Result<[u8; 3], ExpandedSettingsBaseError> {
    let target = pc_to_snes(Mapper::LoRom, destination_offset).map_err(|source| {
        ExpandedSettingsBaseError::InvalidTargetAddress {
            descriptor_index,
            destination_offset,
            source,
        }
    })?;
    let bytes = target.to_le_bytes();
    Ok([bytes[0], bytes[1], bytes[2] & 0x7f])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExpandedSettingsEntryContinuation;
    use std::{fs, path::PathBuf};

    #[test]
    fn generated_helper_matches_lunar_magic_template() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let source = 0x005b_6880 - 0x0040_0000;
        assert_eq!(
            smw_us_v1_expanded_settings_base_helper().unwrap(),
            executable[source..source + 0x10]
        );
    }

    #[test]
    fn complete_base_family_matches_pristine_and_wine_roms() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let writes =
            smw_us_v1_expanded_settings_base_writes(ExpandedSettingsRuntimeLayout::smw_us_v1(
                0x11_8000,
                ExpandedSettingsEntryContinuation::Continue,
            ))
            .unwrap();
        assert_eq!(writes.len(), 7);
        for write in writes {
            let physical = write.offset + 0x200;
            assert_eq!(
                write.expected,
                before[physical..physical + write.expected.len()]
            );
            assert_eq!(
                write.replacement,
                after[physical..physical + write.replacement.len()]
            );
        }
    }

    #[test]
    fn unmappable_descriptor_72_destination_is_rejected() {
        let mut layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
            0x11_8000,
            ExpandedSettingsEntryContinuation::Continue,
        );
        let target_slot = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
            .iter()
            .position(|block| block.descriptor_index == 0x72)
            .unwrap();
        layout.destination_offsets[target_slot] = usize::MAX;
        assert!(matches!(
            smw_us_v1_expanded_settings_base_writes(layout),
            Err(ExpandedSettingsBaseError::InvalidTargetAddress {
                descriptor_index: 0x72,
                ..
            })
        ));
    }
}
