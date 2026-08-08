//! Composition of the independently generated expanded-settings runtime family.

use crate::{
    ExpandedSettingsBaseError, ExpandedSettingsEntryContinuation, ExpandedSettingsHookError,
    ExpandedSettingsRuntimeBuildError, ExpandedSettingsTransferRuntimeError,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS, smw_us_v1_expanded_settings_allocation_load_block,
    smw_us_v1_expanded_settings_base_writes, smw_us_v1_expanded_settings_direct_hook_writes,
    smw_us_v1_expanded_settings_dma_block, smw_us_v1_expanded_settings_field_runtime_block,
    smw_us_v1_expanded_settings_index_restore_block,
    smw_us_v1_expanded_settings_indexed_scratch_block,
    smw_us_v1_expanded_settings_operand_relocation_write,
    smw_us_v1_expanded_settings_pointer_dispatch_block,
    smw_us_v1_expanded_settings_record_select_block, smw_us_v1_expanded_settings_reset_block,
    smw_us_v1_expanded_settings_selector_dispatch_block,
    smw_us_v1_expanded_settings_special_record_block,
    smw_us_v1_expanded_settings_state_compare_block,
    smw_us_v1_expanded_settings_transfer_runtime_block,
};
use lm_project::{PatchPayload, PatchWrite};
use lm_snes::CodeBuilderError;

/// Logical descriptor destinations recovered from the live SMW US revision-0 layout.
pub const SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_DESTINATIONS: [usize; 12] = [
    0x7f160, 0x7f780, 0x7f7f0, 0x7f840, 0x7f8a0, 0x7f900, 0x7f9c0, 0x7f9e0, 0x7fab0, 0x7faf0,
    0x7fb20, 0x7fd80,
];

/// Four-byte `LM` generation marker immediately preceding descriptor `$69`.
pub const SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET: usize = 0x07_f15c;
pub const SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER: [u8; 4] = [0x4c, 0x4d, 0x03, 0x01];

/// Resolved addresses and destinations required by the twelve-block runtime family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsRuntimeLayout {
    /// Logical, copier-header-transparent PC destinations in descriptor order.
    pub destination_offsets: [usize; 12],
    pub allocation_base_snes: u32,
    pub mapped_table_snes: u32,
    pub mapped_helper_snes: u32,
    pub continuation: ExpandedSettingsEntryContinuation,
}

/// One generated runtime block paired with its recovered descriptor identity and destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsRuntimeComponent {
    pub descriptor_index: usize,
    pub destination_offset: usize,
    pub payload: PatchPayload,
}

impl ExpandedSettingsRuntimeLayout {
    /// Constructs the recovered SMW US revision-0 layout.
    ///
    /// The mapped table/helper addresses are fixed runtime entries from the revision descriptor;
    /// only the separately allocated settings table and installer-selected continuation vary.
    #[must_use]
    pub const fn smw_us_v1(
        allocation_base_snes: u32,
        continuation: ExpandedSettingsEntryContinuation,
    ) -> Self {
        Self {
            destination_offsets: SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_DESTINATIONS,
            allocation_base_snes,
            mapped_table_snes: 0x0f_f200,
            mapped_helper_snes: 0x0f_f900,
            continuation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsAllocationFixupEncoding {
    Long24,
    Low16,
    Low8,
    Bank8,
}

/// One allocation-dependent operand inside a generated runtime component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsAllocationFixup {
    pub descriptor_index: usize,
    pub offset: usize,
    pub target_addend: usize,
    pub encoding: ExpandedSettingsAllocationFixupEncoding,
}

/// Every operand that must be rebound after allocating the `$6E00` settings payload.
pub const SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS: [ExpandedSettingsAllocationFixup;
    10] = [
    allocation_fixup(
        0x172,
        0x16,
        0x2d00,
        ExpandedSettingsAllocationFixupEncoding::Low16,
    ),
    allocation_fixup(
        0x172,
        0x0f,
        0x2d00,
        ExpandedSettingsAllocationFixupEncoding::Long24,
    ),
    allocation_fixup(
        0x172,
        0x1f,
        0x2d00,
        ExpandedSettingsAllocationFixupEncoding::Bank8,
    ),
    allocation_fixup(
        0x173,
        0x33,
        0,
        ExpandedSettingsAllocationFixupEncoding::Long24,
    ),
    allocation_fixup(
        0x1db,
        0x37,
        0,
        ExpandedSettingsAllocationFixupEncoding::Long24,
    ),
    allocation_fixup(
        0x1db,
        0x3d,
        1,
        ExpandedSettingsAllocationFixupEncoding::Long24,
    ),
    allocation_fixup(
        0x216,
        0x09,
        0x6d00,
        ExpandedSettingsAllocationFixupEncoding::Low16,
    ),
    allocation_fixup(
        0x216,
        0x12,
        0x6d00,
        ExpandedSettingsAllocationFixupEncoding::Bank8,
    ),
    allocation_fixup(
        0x21c,
        0x39,
        0x6d00,
        ExpandedSettingsAllocationFixupEncoding::Low16,
    ),
    allocation_fixup(
        0x21c,
        0x42,
        0x6d00,
        ExpandedSettingsAllocationFixupEncoding::Bank8,
    ),
];

const fn allocation_fixup(
    descriptor_index: usize,
    offset: usize,
    target_addend: usize,
    encoding: ExpandedSettingsAllocationFixupEncoding,
) -> ExpandedSettingsAllocationFixup {
    ExpandedSettingsAllocationFixup {
        descriptor_index,
        offset,
        target_addend,
        encoding,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsRuntimeBundleError {
    Code(CodeBuilderError),
    Runtime(ExpandedSettingsRuntimeBuildError),
    Transfer(ExpandedSettingsTransferRuntimeError),
    Hook(ExpandedSettingsHookError),
    Base(ExpandedSettingsBaseError),
    DestinationOverflow {
        descriptor_index: usize,
        destination_offset: usize,
        len: usize,
    },
    OverlappingDestinations {
        first_descriptor_index: usize,
        second_descriptor_index: usize,
    },
    MissingFixupComponent {
        descriptor_index: usize,
    },
    FixupOutOfBounds {
        descriptor_index: usize,
        offset: usize,
        len: usize,
    },
    FixupAddressOutOfRange {
        allocation_base_snes: u32,
        target_addend: usize,
    },
}

impl std::fmt::Display for ExpandedSettingsRuntimeBundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings runtime bundle failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsRuntimeBundleError {}

impl From<CodeBuilderError> for ExpandedSettingsRuntimeBundleError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

impl From<ExpandedSettingsRuntimeBuildError> for ExpandedSettingsRuntimeBundleError {
    fn from(value: ExpandedSettingsRuntimeBuildError) -> Self {
        Self::Runtime(value)
    }
}

impl From<ExpandedSettingsTransferRuntimeError> for ExpandedSettingsRuntimeBundleError {
    fn from(value: ExpandedSettingsTransferRuntimeError) -> Self {
        Self::Transfer(value)
    }
}

impl From<ExpandedSettingsHookError> for ExpandedSettingsRuntimeBundleError {
    fn from(value: ExpandedSettingsHookError) -> Self {
        Self::Hook(value)
    }
}

impl From<ExpandedSettingsBaseError> for ExpandedSettingsRuntimeBundleError {
    fn from(value: ExpandedSettingsBaseError) -> Self {
        Self::Base(value)
    }
}

/// Generates and resolves all twelve copied runtime blocks as one validated family.
///
/// This intentionally stops at resolved components. Turning them into clean-ROM writes additionally
/// requires revision-bound expected bytes for every destination and hook.
///
/// # Errors
///
/// Rejects invalid SNES parameters, assembler failures, overflowing destination ranges, or
/// overlapping descriptor destinations.
pub fn smw_us_v1_expanded_settings_runtime_bundle(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<Vec<ExpandedSettingsRuntimeComponent>, ExpandedSettingsRuntimeBundleError> {
    let special_record_snes = layout.allocation_base_snes.checked_add(0x6d00).ok_or(
        ExpandedSettingsRuntimeBuildError::AddressOutOfRange(layout.allocation_base_snes),
    )?;
    let record_table_snes = layout.allocation_base_snes.checked_add(0x2d00).ok_or(
        ExpandedSettingsRuntimeBuildError::AddressOutOfRange(layout.allocation_base_snes),
    )?;
    let payloads = [
        smw_us_v1_expanded_settings_selector_dispatch_block()?,
        smw_us_v1_expanded_settings_reset_block()?,
        smw_us_v1_expanded_settings_record_select_block(record_table_snes)?,
        smw_us_v1_expanded_settings_allocation_load_block(layout.allocation_base_snes)?,
        smw_us_v1_expanded_settings_indexed_scratch_block(
            layout.mapped_table_snes,
            layout.mapped_helper_snes,
        )?,
        smw_us_v1_expanded_settings_pointer_dispatch_block(layout.allocation_base_snes)?,
        smw_us_v1_expanded_settings_index_restore_block()?,
        smw_us_v1_expanded_settings_dma_block(layout.continuation)?,
        smw_us_v1_expanded_settings_special_record_block(special_record_snes)?,
        smw_us_v1_expanded_settings_state_compare_block()?,
        smw_us_v1_expanded_settings_transfer_runtime_block(
            special_record_snes,
            layout.mapped_helper_snes,
            layout.continuation,
        )?,
        smw_us_v1_expanded_settings_field_runtime_block()?,
    ];

    let mut components = Vec::with_capacity(payloads.len());
    for ((block, destination_offset), payload) in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .zip(layout.destination_offsets)
        .zip(payloads)
    {
        debug_assert_eq!(block.len, payload.bytes.len());
        components.push(ExpandedSettingsRuntimeComponent {
            descriptor_index: block.descriptor_index,
            destination_offset,
            payload,
        });
    }
    validate_destinations(&components)?;
    Ok(components)
}

/// Produces exact-guarded fixed-destination writes for the complete runtime family.
///
/// The pristine revision reserves every destination as `$FF`; each returned write therefore fails
/// safely if another patch already owns any byte. Allocation and hook writes remain separate.
///
/// # Errors
///
/// Propagates complete-family generation and destination validation failures.
pub fn smw_us_v1_expanded_settings_runtime_writes(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<Vec<PatchWrite>, ExpandedSettingsRuntimeBundleError> {
    smw_us_v1_expanded_settings_runtime_bundle(layout).map(|components| {
        components
            .into_iter()
            .map(|component| PatchWrite {
                offset: component.destination_offset,
                expected: vec![0xff; component.payload.bytes.len()],
                replacement: component.payload.bytes,
                fixups: component.payload.fixups,
            })
            .collect()
    })
}

/// Produces the complete recovered fixed-ROM family: twelve runtime bodies, descriptor `$70`,
/// five fixed pointer publications, and five persistent hooks/relocations.
///
/// # Errors
///
/// Propagates runtime-family or hook generation failures.
pub fn smw_us_v1_expanded_settings_fixed_writes(
    layout: ExpandedSettingsRuntimeLayout,
) -> Result<Vec<PatchWrite>, ExpandedSettingsRuntimeBundleError> {
    let mut writes = smw_us_v1_expanded_settings_runtime_writes(layout)?;
    writes.push(PatchWrite {
        offset: SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET,
        expected: vec![0xff; SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER.len()],
        replacement: SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER.to_vec(),
        fixups: Vec::new(),
    });
    writes.extend(smw_us_v1_expanded_settings_base_writes(layout)?);
    writes.extend(smw_us_v1_expanded_settings_direct_hook_writes(layout)?);
    writes.push(smw_us_v1_expanded_settings_operand_relocation_write(
        layout,
    )?);
    writes.sort_unstable_by_key(|write| write.offset);
    Ok(writes)
}

/// Rebinds every allocation-dependent operand after the allocator selects a SNES address.
///
/// This explicitly supports Lunar Magic's split low-word/bank publications in addition to ordinary
/// contiguous 24-bit operands.
///
/// # Errors
///
/// Rejects missing components, malformed fixup ranges, arithmetic overflow, or targets outside the
/// 24-bit SNES bus.
pub fn resolve_expanded_settings_runtime_allocation(
    components: &mut [ExpandedSettingsRuntimeComponent],
    allocation_base_snes: u32,
) -> Result<(), ExpandedSettingsRuntimeBundleError> {
    for fixup in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS {
        let component = components
            .iter_mut()
            .find(|component| component.descriptor_index == fixup.descriptor_index)
            .ok_or(ExpandedSettingsRuntimeBundleError::MissingFixupComponent {
                descriptor_index: fixup.descriptor_index,
            })?;
        let addend = u32::try_from(fixup.target_addend).map_err(|_| {
            ExpandedSettingsRuntimeBundleError::FixupAddressOutOfRange {
                allocation_base_snes,
                target_addend: fixup.target_addend,
            }
        })?;
        let target = allocation_base_snes
            .checked_add(addend)
            .filter(|value| *value <= 0x00ff_ffff)
            .ok_or(ExpandedSettingsRuntimeBundleError::FixupAddressOutOfRange {
                allocation_base_snes,
                target_addend: fixup.target_addend,
            })?;
        let encoded = target.to_le_bytes();
        let replacement: &[u8] = match fixup.encoding {
            ExpandedSettingsAllocationFixupEncoding::Long24 => &encoded[..3],
            ExpandedSettingsAllocationFixupEncoding::Low16 => &encoded[..2],
            ExpandedSettingsAllocationFixupEncoding::Low8 => &encoded[..1],
            ExpandedSettingsAllocationFixupEncoding::Bank8 => &encoded[2..3],
        };
        let end = fixup.offset.checked_add(replacement.len()).ok_or(
            ExpandedSettingsRuntimeBundleError::FixupOutOfBounds {
                descriptor_index: fixup.descriptor_index,
                offset: fixup.offset,
                len: replacement.len(),
            },
        )?;
        let destination = component.payload.bytes.get_mut(fixup.offset..end).ok_or(
            ExpandedSettingsRuntimeBundleError::FixupOutOfBounds {
                descriptor_index: fixup.descriptor_index,
                offset: fixup.offset,
                len: replacement.len(),
            },
        )?;
        destination.copy_from_slice(replacement);
    }
    Ok(())
}

fn validate_destinations(
    components: &[ExpandedSettingsRuntimeComponent],
) -> Result<(), ExpandedSettingsRuntimeBundleError> {
    let mut ranges = Vec::with_capacity(components.len());
    for component in components {
        let end = component
            .destination_offset
            .checked_add(component.payload.bytes.len())
            .ok_or(ExpandedSettingsRuntimeBundleError::DestinationOverflow {
                descriptor_index: component.descriptor_index,
                destination_offset: component.destination_offset,
                len: component.payload.bytes.len(),
            })?;
        ranges.push((
            component.destination_offset,
            end,
            component.descriptor_index,
        ));
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(
                ExpandedSettingsRuntimeBundleError::OverlappingDestinations {
                    first_descriptor_index: pair[0].2,
                    second_descriptor_index: pair[1].2,
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn wine_layout() -> ExpandedSettingsRuntimeLayout {
        ExpandedSettingsRuntimeLayout::smw_us_v1(
            0x11_8000,
            ExpandedSettingsEntryContinuation::Continue,
        )
    }

    #[test]
    fn guarded_writes_match_pristine_and_installed_roms() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let writes = smw_us_v1_expanded_settings_runtime_writes(wine_layout()).unwrap();
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
            assert!(write.fixups.is_empty());
        }
    }

    #[test]
    fn fixed_write_family_is_complete_nonoverlapping_and_matches_both_roms() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let writes = smw_us_v1_expanded_settings_fixed_writes(wine_layout()).unwrap();
        assert_eq!(writes.len(), 24);
        assert!(
            writes
                .windows(2)
                .all(|pair| pair[0].offset + pair[0].replacement.len() <= pair[1].offset)
        );
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
    fn two_phase_allocation_fixups_resolve_to_the_concrete_wine_bundle() {
        let mut template =
            smw_us_v1_expanded_settings_runtime_bundle(ExpandedSettingsRuntimeLayout::smw_us_v1(
                0x00_8000,
                ExpandedSettingsEntryContinuation::Continue,
            ))
            .unwrap();
        resolve_expanded_settings_runtime_allocation(&mut template, 0x11_8000).unwrap();
        let concrete = smw_us_v1_expanded_settings_runtime_bundle(wine_layout()).unwrap();
        assert_eq!(template, concrete);
    }

    #[test]
    fn allocation_fixups_reject_missing_components_and_bus_overflow() {
        let mut components = smw_us_v1_expanded_settings_runtime_bundle(wine_layout()).unwrap();
        components.retain(|component| component.descriptor_index != 0x172);
        assert_eq!(
            resolve_expanded_settings_runtime_allocation(&mut components, 0x11_8000),
            Err(ExpandedSettingsRuntimeBundleError::MissingFixupComponent {
                descriptor_index: 0x172
            })
        );

        let mut components = smw_us_v1_expanded_settings_runtime_bundle(wine_layout()).unwrap();
        assert!(matches!(
            resolve_expanded_settings_runtime_allocation(&mut components, 0xff_ff00),
            Err(ExpandedSettingsRuntimeBundleError::FixupAddressOutOfRange { .. })
        ));
    }

    #[test]
    fn complete_bundle_matches_every_wine_installed_runtime_block() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let installed = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let components = smw_us_v1_expanded_settings_runtime_bundle(wine_layout()).unwrap();
        assert_eq!(components.len(), 12);
        for component in components {
            let physical = component.destination_offset + 0x200;
            assert_eq!(
                component.payload.bytes,
                installed[physical..physical + component.payload.bytes.len()],
                "descriptor ${:X}",
                component.descriptor_index
            );
        }
    }

    #[test]
    fn overlapping_and_overflowing_destinations_are_rejected() {
        let mut overlapping = wine_layout();
        overlapping.destination_offsets[1] = overlapping.destination_offsets[0];
        assert!(matches!(
            smw_us_v1_expanded_settings_runtime_bundle(overlapping),
            Err(ExpandedSettingsRuntimeBundleError::OverlappingDestinations { .. })
        ));

        let mut overflowing = wine_layout();
        overflowing.destination_offsets[11] = usize::MAX;
        assert!(matches!(
            smw_us_v1_expanded_settings_runtime_bundle(overflowing),
            Err(ExpandedSettingsRuntimeBundleError::DestinationOverflow {
                descriptor_index: 0x220,
                ..
            })
        ));
    }
}
