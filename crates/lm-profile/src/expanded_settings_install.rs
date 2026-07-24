//! Transactional SMW US revision-0 expanded-settings installation plan.

use crate::{
    ExpandedSettingsAllocationFixupEncoding, ExpandedSettingsEntryContinuation,
    ExpandedSettingsRuntimeBundleError, ExpandedSettingsRuntimeLayout,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS, SmwUsV1ExpandedSettingsAllocation,
    smw_us_v1_expanded_settings_fixed_writes,
};
use lm_level::ExpandedOverworldSettings;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START: usize = 0x08_7ff8;
pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END: usize = 0x09_0000;
pub const SMW_US_V1_CHECKSUM_FIELD: usize = 0x00_7fdc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsInstallPlanError {
    Runtime(ExpandedSettingsRuntimeBundleError),
    MissingRuntimeWrite {
        descriptor_index: usize,
        destination_offset: usize,
    },
    MissingFixupDescriptor {
        descriptor_index: usize,
    },
    UnexpectedRuntimeFixups {
        descriptor_index: usize,
    },
    SpecialRecordIndex(usize),
}

impl std::fmt::Display for ExpandedSettingsInstallPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings installation plan failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsInstallPlanError {}

impl From<ExpandedSettingsRuntimeBundleError> for ExpandedSettingsInstallPlanError {
    fn from(value: ExpandedSettingsRuntimeBundleError) -> Self {
        Self::Runtime(value)
    }
}

/// Builds the complete failure-atomic installation plan.
///
/// The allocation policy reproduces Lunar Magic's retained placement: the eight-byte RATS header
/// starts at `$087FF8`, and the `$6E00` payload begins at `$088000` (`$11:8000`). Runtime operands
/// remain typed relocations, so the transaction still derives their values from the allocator's
/// result rather than embedding that address.
///
/// # Errors
///
/// Rejects runtime generation failures or disagreement between the recovered descriptor/fixup
/// catalog and the generated fixed-write family.
pub fn smw_us_v1_expanded_settings_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(None)
}

/// Builds the complete installation plan with optional exact records for submaps zero through six.
///
/// # Errors
///
/// Propagates the same recovered runtime/fixup validation as the default installation plan.
pub fn smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(
    overworld: Option<&ExpandedOverworldSettings>,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    // The builder needs well-formed placeholder addresses; every allocation-dependent byte is
    // replaced by a typed transaction fixup before publication.
    let layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
        0x00_8000,
        ExpandedSettingsEntryContinuation::Continue,
    );
    let mut writes = smw_us_v1_expanded_settings_fixed_writes(layout)?;
    for (slot, block) in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .copied()
        .enumerate()
    {
        let destination_offset = layout.destination_offsets[slot];
        let write = writes
            .iter()
            .find(|write| write.offset == destination_offset)
            .ok_or(ExpandedSettingsInstallPlanError::MissingRuntimeWrite {
                descriptor_index: block.descriptor_index,
                destination_offset,
            })?;
        if !write.fixups.is_empty() {
            return Err(ExpandedSettingsInstallPlanError::UnexpectedRuntimeFixups {
                descriptor_index: block.descriptor_index,
            });
        }
    }
    for recovered in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS {
        let slot = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
            .iter()
            .position(|block| block.descriptor_index == recovered.descriptor_index)
            .ok_or(ExpandedSettingsInstallPlanError::MissingFixupDescriptor {
                descriptor_index: recovered.descriptor_index,
            })?;
        let block = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[slot];
        let destination_offset = layout.destination_offsets[slot];
        let write = writes
            .iter_mut()
            .find(|write| write.offset == destination_offset)
            .ok_or(ExpandedSettingsInstallPlanError::MissingRuntimeWrite {
                descriptor_index: block.descriptor_index,
                destination_offset,
            })?;
        write.fixups.push(PatchFixup {
            offset: recovered.offset,
            target_payload: 0,
            target_addend: recovered.target_addend,
            encoding: match recovered.encoding {
                ExpandedSettingsAllocationFixupEncoding::Long24 => {
                    PatchFixupEncoding::Long24LowBank
                }
                ExpandedSettingsAllocationFixupEncoding::Low16 => PatchFixupEncoding::Low16,
                ExpandedSettingsAllocationFixupEncoding::Bank8 => PatchFixupEncoding::Bank8LowBank,
            },
        });
    }

    let mut allocation = SmwUsV1ExpandedSettingsAllocation::new_default();
    if let Some(overworld) = overworld {
        for (index, record) in overworld.records.iter().cloned().enumerate() {
            allocation
                .set_record(0x200 + index, record)
                .map_err(|_| ExpandedSettingsInstallPlanError::SpecialRecordIndex(index))?;
        }
    }
    Ok(RelocatablePatchPlan {
        description: "install SMW US expanded level settings".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START
                ..SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END,
            // Lunar Magic places the tag in the preceding bank's final eight bytes.
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: allocation.encode(),
            fixups: Vec::new(),
        }],
        writes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN, SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT};
    use lm_project::{ExpandedLevelSettingsLayout, Project, RelocatablePatchError};
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    fn fixtures() -> (Vec<u8>, RomImage) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        (after, RomImage::from_bytes(before).unwrap())
    }

    #[test]
    fn plan_installs_all_owned_bytes_reopens_semantically_and_undoes_exactly() {
        let (after_file, before_image) = fixtures();
        let after = RomImage::from_bytes(after_file).unwrap();
        let original = before_image.logical_bytes().to_vec();
        let mut project = Project::new(before_image);
        let plan = smw_us_v1_expanded_settings_installation_plan().unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks.len(), 1);
        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START
        );
        assert_eq!(result.blocks[0].payload.start, 0x08_8000);
        // The retained oracle imported level 000 after installing the table. Its tag, fill prefix,
        // and every other record are nevertheless exact installation evidence.
        let installed = project
            .rom
            .read(result.blocks[0].header_offset, 0x6e08)
            .unwrap();
        let oracle = after.read(result.blocks[0].header_offset, 0x6e08).unwrap();
        let record_zero = 8 + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN;
        assert_eq!(&installed[..record_zero], &oracle[..record_zero]);
        assert_eq!(
            &installed[record_zero + 0x20..],
            &oracle[record_zero + 0x20..]
        );
        for write in &plan.writes {
            assert_eq!(
                project
                    .rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                after.read(write.offset, write.replacement.len()).unwrap()
            );
        }
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        let settings_layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: result.blocks[0].payload.start + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
            entries: SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
            stride: 0x20,
        };
        assert_eq!(
            project
                .load_expanded_level_settings(0x207, settings_layout)
                .unwrap(),
            crate::smw_us_v1_default_expanded_settings_record()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn late_hook_precondition_failure_preserves_rom_and_history() {
        let (_, mut before) = fixtures();
        before.write(0x1471, &[0]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_expanded_settings_installation_plan().unwrap();
        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch { .. })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }
}
