//! Complete transactional SMW US v1 Layer 3 runtime installation.

use crate::{
    ExpandedSettingsInstallPlanError, Layer3CompatibilityBuildError, SMW_US_V1_CHECKSUM_FIELD,
    SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_END, SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START,
    smw_us_v1_expanded_settings_installation_plan, smw_us_v1_layer3_auxiliary_payload,
    smw_us_v1_layer3_compatibility_payload, smw_us_v1_layer3_extended_runtime_payload,
    smw_us_v1_layer3_extended_runtime_writes, smw_us_v1_layer3_main_patch_payload,
    smw_us_v1_layer3_main_patch_writes, smw_us_v1_layer3_main_runtime_allocation_hooks,
    smw_us_v1_layer3_main_runtime_payload, smw_us_v1_layer3_main_runtime_verified_fixed_writes,
};
use lm_project::{PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompleteLayer3BuildError {
    Runtime(Layer3CompatibilityBuildError),
    ExpandedSettings(ExpandedSettingsInstallPlanError),
}

impl std::fmt::Display for CompleteLayer3BuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "complete Layer 3 construction failed: {self:?}")
    }
}

impl std::error::Error for CompleteLayer3BuildError {}

impl From<Layer3CompatibilityBuildError> for CompleteLayer3BuildError {
    fn from(value: Layer3CompatibilityBuildError) -> Self {
        Self::Runtime(value)
    }
}

impl From<ExpandedSettingsInstallPlanError> for CompleteLayer3BuildError {
    fn from(value: ExpandedSettingsInstallPlanError) -> Self {
        Self::ExpandedSettings(value)
    }
}

/// Builds the separately allocated plans required by the complete Layer 3 feature.
///
/// The runtime family and expanded-settings table deliberately retain different recovered free-space
/// policies and must be installed with
/// [`lm_project::Project::install_relocatable_patch_group`].
///
/// # Errors
///
/// Propagates runtime generation or expanded-settings construction failures.
pub fn smw_us_v1_complete_layer3_feature_plans()
-> Result<Vec<RelocatablePatchPlan>, CompleteLayer3BuildError> {
    Ok(vec![
        smw_us_v1_complete_layer3_installation_plan()?,
        smw_us_v1_expanded_settings_installation_plan()?,
    ])
}

/// Builds one failure-atomic installation containing all five recovered Layer 3 allocations.
///
/// # Errors
///
/// Propagates deterministic compatibility/auxiliary code-generation failures.
pub fn smw_us_v1_complete_layer3_installation_plan()
-> Result<RelocatablePatchPlan, Layer3CompatibilityBuildError> {
    let components = [
        (
            vec![smw_us_v1_layer3_main_patch_payload()],
            smw_us_v1_layer3_main_patch_writes(),
        ),
        (vec![smw_us_v1_layer3_main_runtime_payload()], {
            let mut writes = smw_us_v1_layer3_main_runtime_allocation_hooks();
            writes.extend(smw_us_v1_layer3_main_runtime_verified_fixed_writes());
            writes
        }),
        (
            vec![
                smw_us_v1_layer3_compatibility_payload()?,
                smw_us_v1_layer3_auxiliary_payload()?,
            ],
            compatibility_writes(),
        ),
        (
            vec![smw_us_v1_layer3_extended_runtime_payload()],
            smw_us_v1_layer3_extended_runtime_writes(),
        ),
    ];
    let mut payloads = Vec::new();
    let mut writes = Vec::new();
    for (component_payloads, component_writes) in components {
        append_component(
            &mut payloads,
            &mut writes,
            component_payloads,
            component_writes,
        );
    }
    Ok(RelocatablePatchPlan {
        description: "install complete SMW US Layer 3 runtime".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START
                ..SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_END,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads,
        writes,
    })
}

fn append_component(
    payloads: &mut Vec<PatchPayload>,
    writes: &mut Vec<PatchWrite>,
    mut component_payloads: Vec<PatchPayload>,
    mut component_writes: Vec<PatchWrite>,
) {
    let payload_base = payloads.len();
    for payload in &mut component_payloads {
        for fixup in &mut payload.fixups {
            fixup.target_payload += payload_base;
        }
    }
    for write in &mut component_writes {
        for fixup in &mut write.fixups {
            fixup.target_payload += payload_base;
        }
    }
    payloads.extend(component_payloads);
    writes.extend(component_writes);
}

fn compatibility_writes() -> Vec<PatchWrite> {
    crate::smw_us_v1_layer3_compatibility_installation_plan()
        .expect("the same generated payloads were already validated")
        .writes
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchError};
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    fn pristine() -> RomImage {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn complete_family_is_one_checksum_valid_undo_step() {
        let before = pristine();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_complete_layer3_installation_plan().unwrap();
        assert_eq!(plan.payloads.len(), 5);
        assert_eq!(plan.writes.len(), 55);
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks.len(), 5);
        assert!(
            result
                .blocks
                .windows(2)
                .all(|pair| pair[0].payload.end <= pair[1].header_offset)
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn final_extended_hook_failure_rolls_back_every_family() {
        let mut before = pristine();
        before.write(0x0001_b86c, &[0xff]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_complete_layer3_installation_plan().unwrap();
        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch {
                index: 54,
                offset: 0x0001_b86c
            })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn complete_feature_keeps_the_expanded_table_bank_alignment() {
        let before = pristine();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plans = smw_us_v1_complete_layer3_feature_plans().unwrap();
        let results = project
            .install_relocatable_patch_group("install complete Layer 3 feature", &plans)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].blocks.len(), 5);
        assert_eq!(results[1].blocks.len(), 1);
        assert_eq!(results[1].blocks[0].header_offset, 0x0008_7ff8);
        assert_eq!(results[1].blocks[0].payload.start, 0x0008_8000);
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }
}
