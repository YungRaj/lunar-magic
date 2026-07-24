//! Clean-room implementation of the SMW US v1 Layer 3 compatibility family.
//!
//! Lunar Magic installs two consecutive `$20`-byte RATS blocks: a bridge for the legacy Layer 3
//! position path and an auxiliary comparison dispatcher used by two other engine sites.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;
use lm_snes::{BranchCondition, CodeBuilder, CodeBuilderError};

pub const SMW_US_V1_LAYER3_COMPATIBILITY_PAYLOAD_LEN: usize = 0x20;
pub const SMW_US_V1_LAYER3_AUXILIARY_PAYLOAD_LEN: usize = 0x20;
pub const SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_START: usize = 0x0008_5a56;
pub const SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_END: usize = 0x0009_0000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3CompatibilityBuildError {
    Code(CodeBuilderError),
    UnexpectedGeneratedLength {
        component: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl std::fmt::Display for Layer3CompatibilityBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Layer 3 compatibility construction failed: {self:?}"
        )
    }
}

impl std::error::Error for Layer3CompatibilityBuildError {}

impl From<CodeBuilderError> for Layer3CompatibilityBuildError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

/// Generates the legacy-coordinate bridge and its preserved Lunar Magic ownership marker.
///
/// # Errors
///
/// Rejects code-construction failure or a generated instruction stream whose size no longer leaves
/// the recovered marker at its verified location.
pub fn smw_us_v1_layer3_compatibility_payload()
-> Result<PatchPayload, Layer3CompatibilityBuildError> {
    let mut code = CodeBuilder::new();
    let layer_one = code.label()?;
    code.sta_direct_page(0x99);
    code.ldy_direct_page(0x0c);
    code.cpy_immediate16(0x0200);
    code.branch(BranchCondition::CarrySet, layer_one);
    code.jml_absolute(0x00_00bef1);
    code.bind(layer_one)?;
    code.jml_absolute(0x00_00bebb);
    let assembled = code.finish()?;
    if assembled.bytes.len() != 0x11 {
        return Err(Layer3CompatibilityBuildError::UnexpectedGeneratedLength {
            component: "bridge code",
            expected: 0x11,
            actual: assembled.bytes.len(),
        });
    }
    let mut bytes = assembled.bytes;
    bytes.extend_from_slice(b"           LM\0\x01");
    debug_assert_eq!(bytes.len(), SMW_US_V1_LAYER3_COMPATIBILITY_PAYLOAD_LEN);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Generates both comparison entries used by the auxiliary compatibility hooks.
///
/// Entry zero compares `$09` with `$13D8`, then `$01` with `$13D7`. Entry `$0D` performs the same
/// operation after adding zero to A, preserving the carry-sensitive instruction replaced at its
/// caller.
///
/// # Errors
///
/// Rejects code-construction failure or unexpected instruction-stream size.
pub fn smw_us_v1_layer3_auxiliary_payload() -> Result<PatchPayload, Layer3CompatibilityBuildError> {
    let mut code = CodeBuilder::new();
    let first_done = code.label()?;
    let second_done = code.label()?;
    code.lda_direct_page(0x09);
    code.cmp_absolute(0x13d8);
    code.branch(BranchCondition::NotEqual, first_done);
    code.ldy_direct_page(0x01);
    code.cpy_absolute(0x13d7);
    code.bind(first_done)?;
    code.rtl();
    code.adc_immediate8(0);
    code.cmp_absolute(0x13d8);
    code.branch(BranchCondition::NotEqual, second_done);
    code.ldy_direct_page(0);
    code.cpy_absolute(0x13d7);
    code.bind(second_done)?;
    code.rtl();
    let assembled = code.finish()?;
    if assembled.bytes.len() != 0x1a {
        return Err(Layer3CompatibilityBuildError::UnexpectedGeneratedLength {
            component: "auxiliary code",
            expected: 0x1a,
            actual: assembled.bytes.len(),
        });
    }
    let mut bytes = assembled.bytes;
    bytes.extend_from_slice(b"  LM\x01\x01");
    debug_assert_eq!(bytes.len(), SMW_US_V1_LAYER3_AUXILIARY_PAYLOAD_LEN);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Builds the complete failure-atomic compatibility-family installation.
///
/// Payload 0 is the bridge and payload 1 is the auxiliary dispatcher. The narrow allocation range
/// reproduces the retained Wine installation while still deriving all five mapped operands from
/// allocator results.
///
/// # Errors
///
/// Propagates deterministic payload-construction failures.
pub fn smw_us_v1_layer3_compatibility_installation_plan()
-> Result<RelocatablePatchPlan, Layer3CompatibilityBuildError> {
    Ok(RelocatablePatchPlan {
        description: "install SMW US Layer 3 compatibility bridge".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_START
                ..SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_END,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            smw_us_v1_layer3_compatibility_payload()?,
            smw_us_v1_layer3_auxiliary_payload()?,
        ],
        writes: compatibility_writes(),
    })
}

fn compatibility_writes() -> Vec<PatchWrite> {
    vec![
        fixed_write(
            0x0000_3eec,
            &[0xc0, 0, 2, 0xb0, 0xca],
            &[0xcc, 0xd7, 0x13, 0xb0, 0xca],
        ),
        allocation_hook(0x0000_3ee8, &[0x85, 0x99, 0xa4, 0x0c], 0x5c, 0, 0),
        fixed_write(
            0x0000_3f81,
            &[
                0xa5, 0x1a, 0x38, 0xe9, 0x80, 0, 0xaa, 0xa4, 0x1c, 0xad, 0x33, 0x19, 0xf0, 0x23,
                0xa6, 0x1e, 0xa5, 0x20, 0x38, 0xe9, 0x80, 0, 0xa8, 0x4c, 0xb2, 0xbf,
            ],
            &[
                0xad, 0x33, 0x19, 0x0a, 0x0a, 0xaa, 0xb5, 0x1c, 0x38, 0xe9, 0x80, 0, 0xa8, 0xb5,
                0x1a, 0x38, 0xe9, 0x80, 0, 0xaa, 0x80, 0x1b, 0xea, 0xea, 0xea, 0xea,
            ],
        ),
        fixed_write(0x0000_4117, &[0x26], &[0x0f]),
        fixed_write(0x0000_4123, &[0x1a], &[0x03]),
        allocation_hook(0x0001_3a4e, &[0xa5, 0x09, 0xc9, 0x02], 0x22, 1, 0),
        allocation_hook(0x0001_5158, &[0x69, 0, 0xc9, 2], 0x22, 1, 0x0d),
    ]
}

fn fixed_write(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn allocation_hook(
    offset: usize,
    expected: &[u8],
    opcode: u8,
    target_payload: usize,
    target_addend: usize,
) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: vec![opcode, 0, 0, 0],
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchError};
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    fn fixtures() -> (RomImage, RomImage) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        (
            RomImage::from_bytes(fs::read(fixture.join("before.smc")).unwrap()).unwrap(),
            RomImage::from_bytes(fs::read(fixture.join("after.smc")).unwrap()).unwrap(),
        )
    }

    #[test]
    fn generated_payloads_match_retained_wine_blocks() {
        let (_, after) = fixtures();
        assert_eq!(
            smw_us_v1_layer3_compatibility_payload().unwrap().bytes,
            after
                .read(0x0008_5a5e, SMW_US_V1_LAYER3_COMPATIBILITY_PAYLOAD_LEN)
                .unwrap()
        );
        assert_eq!(
            smw_us_v1_layer3_auxiliary_payload().unwrap().bytes,
            after
                .read(0x0008_5a86, SMW_US_V1_LAYER3_AUXILIARY_PAYLOAD_LEN)
                .unwrap()
        );
    }

    #[test]
    fn plan_installs_all_hooks_reopens_and_undoes_as_one_edit() {
        let (before, after) = fixtures();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_compatibility_installation_plan().unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].header_offset, 0x0008_5a56);
        assert_eq!(result.blocks[1].header_offset, 0x0008_5a7e);
        for (block, expected_payload) in result.blocks.iter().zip([
            smw_us_v1_layer3_compatibility_payload().unwrap(),
            smw_us_v1_layer3_auxiliary_payload().unwrap(),
        ]) {
            assert_eq!(
                project
                    .rom
                    .read(block.payload.start, block.payload.len())
                    .unwrap(),
                expected_payload.bytes
            );
        }
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
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn late_auxiliary_hook_failure_rolls_back_allocations_and_earlier_writes() {
        let (mut before, _) = fixtures();
        before.write(0x0001_5158, &[0xff]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_compatibility_installation_plan().unwrap();

        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch {
                index: 6,
                offset: 0x0001_5158,
            })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }
}
