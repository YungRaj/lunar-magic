//! Exact-guarded base hooks for the SMW US revision-0 expanded-settings runtime.

use crate::ExpandedSettingsRuntimeLayout;
use lm_project::PatchWrite;
use lm_rom::{Mapper, RomError, pc_to_snes};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsHook {
    pub site_descriptor_index: usize,
    pub target_descriptor_index: usize,
    pub offset: usize,
    pub expected: &'static [u8],
    pub trailing_nop: bool,
}

/// Three direct hooks emitted immediately after the twelve runtime copies.
pub const SMW_US_V1_EXPANDED_SETTINGS_DIRECT_HOOKS: [ExpandedSettingsHook; 3] = [
    ExpandedSettingsHook {
        site_descriptor_index: 0x174,
        target_descriptor_index: 0x172,
        offset: 0x283b8,
        expected: &[0xad, 0x25, 0x19, 0xc9, 0x09],
        trailing_nop: true,
    },
    ExpandedSettingsHook {
        site_descriptor_index: 0x214,
        target_descriptor_index: 0x213,
        offset: 0x1471,
        expected: &[0xae, 0xc6, 0x13, 0xa9, 0x18],
        trailing_nop: true,
    },
    ExpandedSettingsHook {
        site_descriptor_index: 0x217,
        target_descriptor_index: 0x216,
        offset: 0x2140,
        expected: &[0x85, 0x20, 0xe2, 0x20],
        trailing_nop: false,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsOperandRelocation {
    pub site_descriptor_index: usize,
    pub site_addend: usize,
    pub target_descriptor_index: usize,
    pub offset: usize,
    pub expected: [u8; 3],
}

/// Retargets the existing `JSL` at descriptor `$21A` to runtime entry `$219`.
pub const SMW_US_V1_EXPANDED_SETTINGS_OPERAND_RELOCATION: ExpandedSettingsOperandRelocation =
    ExpandedSettingsOperandRelocation {
        site_descriptor_index: 0x21a,
        site_addend: 1,
        target_descriptor_index: 0x219,
        offset: 0x21dfe,
        expected: [0xf2, 0xdb, 0x05],
    };

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsHookError {
    MissingTargetDescriptor {
        descriptor_index: usize,
    },
    InvalidTargetAddress {
        descriptor_index: usize,
        destination_offset: usize,
        source: RomError,
    },
    WrongExpectedLength {
        descriptor_index: usize,
        expected: usize,
        replacement: usize,
    },
}

impl std::fmt::Display for ExpandedSettingsHookError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded-settings hook failed: {self:?}")
    }
}

impl std::error::Error for ExpandedSettingsHookError {}

/// Generates the three direct `JSL` hook writes from resolved runtime destinations.
///
/// # Errors
///
/// Rejects missing targets, destinations that cannot map through `LoROM`, or inconsistent recovered
/// expected-byte metadata.
pub fn smw_us_v1_expanded_settings_direct_hook_writes(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<Vec<PatchWrite>, ExpandedSettingsHookError> {
    SMW_US_V1_EXPANDED_SETTINGS_DIRECT_HOOKS
        .iter()
        .map(|hook| {
            let target_slot = crate::SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
                .iter()
                .position(|block| block.descriptor_index == hook.target_descriptor_index)
                .ok_or(ExpandedSettingsHookError::MissingTargetDescriptor {
                    descriptor_index: hook.target_descriptor_index,
                })?;
            let destination_offset = layout.destination_offsets[target_slot];
            let target = pc_to_snes(Mapper::LoRom, destination_offset).map_err(|source| {
                ExpandedSettingsHookError::InvalidTargetAddress {
                    descriptor_index: hook.target_descriptor_index,
                    destination_offset,
                    source,
                }
            })?;
            let address = target.to_le_bytes();
            // Lunar Magic publishes the equivalent low-bank LoROM mirror for these hooks.
            let mut replacement = vec![0x22, address[0], address[1], address[2] & 0x7f];
            if hook.trailing_nop {
                replacement.push(0xea);
            }
            if replacement.len() != hook.expected.len() {
                return Err(ExpandedSettingsHookError::WrongExpectedLength {
                    descriptor_index: hook.site_descriptor_index,
                    expected: hook.expected.len(),
                    replacement: replacement.len(),
                });
            }
            Ok(PatchWrite {
                offset: hook.offset,
                expected: hook.expected.to_vec(),
                replacement,
                fixups: Vec::new(),
            })
        })
        .collect()
}

/// Generates the exact-guarded operand-only relocation at descriptor `$21A+1`.
///
/// # Errors
///
/// Rejects a missing or unmappable `$219` runtime destination.
pub fn smw_us_v1_expanded_settings_operand_relocation_write(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<PatchWrite, ExpandedSettingsHookError> {
    let relocation = SMW_US_V1_EXPANDED_SETTINGS_OPERAND_RELOCATION;
    let target_slot = crate::SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .position(|block| block.descriptor_index == relocation.target_descriptor_index)
        .ok_or(ExpandedSettingsHookError::MissingTargetDescriptor {
            descriptor_index: relocation.target_descriptor_index,
        })?;
    let destination_offset = layout.destination_offsets[target_slot];
    let target = pc_to_snes(Mapper::LoRom, destination_offset).map_err(|source| {
        ExpandedSettingsHookError::InvalidTargetAddress {
            descriptor_index: relocation.target_descriptor_index,
            destination_offset,
            source,
        }
    })?;
    let mut replacement = target.to_le_bytes()[..3].to_vec();
    replacement[2] &= 0x7f;
    Ok(PatchWrite {
        offset: relocation.offset,
        expected: relocation.expected.to_vec(),
        replacement,
        fixups: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExpandedSettingsEntryContinuation;
    use std::{fs, path::PathBuf};

    #[test]
    fn direct_hooks_match_pristine_and_wine_installed_oracles() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let writes = smw_us_v1_expanded_settings_direct_hook_writes(
            ExpandedSettingsRuntimeLayout::smw_us_v1(
                0x11_8000,
                ExpandedSettingsEntryContinuation::Continue,
            ),
        )
        .unwrap();
        assert_eq!(writes.len(), 3);
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
    fn unmappable_runtime_destination_is_rejected() {
        let mut layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
            0x11_8000,
            ExpandedSettingsEntryContinuation::Continue,
        );
        layout.destination_offsets[2] = usize::MAX;
        assert!(matches!(
            smw_us_v1_expanded_settings_direct_hook_writes(layout),
            Err(ExpandedSettingsHookError::InvalidTargetAddress {
                descriptor_index: 0x172,
                ..
            })
        ));
    }

    #[test]
    fn operand_only_relocation_matches_both_rom_oracles() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let write = smw_us_v1_expanded_settings_operand_relocation_write(
            ExpandedSettingsRuntimeLayout::smw_us_v1(
                0x11_8000,
                ExpandedSettingsEntryContinuation::Continue,
            ),
        )
        .unwrap();
        let physical = write.offset + 0x200;
        assert_eq!(write.expected, before[physical..physical + 3]);
        assert_eq!(write.replacement, after[physical..physical + 3]);
        assert_eq!(write.replacement, [0xf0, 0xfa, 0x0f]);
    }
}
