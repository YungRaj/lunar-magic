use super::{
    ExpandedSettingsRelocation, ExpandedSettingsRelocationTarget, ExpandedSettingsRuntimeBlock,
    RuntimeBlockVerificationError, RuntimeMutableSpan,
};
use crate::{
    SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN, SMW_US_V1_EXPANDED_SETTINGS_SPECIAL_RECORD_OFFSET,
};

const TABLE_BLOCK_MUTABLE: &[RuntimeMutableSpan] = &[
    RuntimeMutableSpan {
        offset: 0x0f,
        len: 3,
    },
    RuntimeMutableSpan {
        offset: 0x16,
        len: 10,
    },
];
const BASE_LOAD_MUTABLE: &[RuntimeMutableSpan] = &[RuntimeMutableSpan {
    offset: 0x33,
    len: 3,
}];
const MAPPED_CALL_MUTABLE: &[RuntimeMutableSpan] = &[RuntimeMutableSpan {
    offset: 0x27,
    len: 3,
}];
const BASE_PAIR_MUTABLE: &[RuntimeMutableSpan] = &[
    RuntimeMutableSpan {
        offset: 0x37,
        len: 3,
    },
    RuntimeMutableSpan {
        offset: 0x3d,
        len: 3,
    },
];
const CONDITIONAL_OPCODE_MUTABLE: &[RuntimeMutableSpan] = &[RuntimeMutableSpan {
    offset: 0x17,
    len: 1,
}];
const SPECIAL_RECORD_MUTABLE: &[RuntimeMutableSpan] = &[
    RuntimeMutableSpan {
        offset: 0x09,
        len: 2,
    },
    RuntimeMutableSpan {
        offset: 0x12,
        len: 1,
    },
];
const LARGE_RUNTIME_MUTABLE: &[RuntimeMutableSpan] = &[
    RuntimeMutableSpan {
        offset: 0x39,
        len: 2,
    },
    RuntimeMutableSpan {
        offset: 0x42,
        len: 1,
    },
    RuntimeMutableSpan {
        offset: 0xb6,
        len: 1,
    },
    RuntimeMutableSpan {
        offset: 0x1bb,
        len: 3,
    },
];

/// Every embedded runtime range copied by `InstallExpandedLevelHeaderRuntime`.
///
/// Virtual addresses identify evidence inside Lunar Magic 3.63 and are not dereferenced by the
/// reimplementation. No proprietary payload bytes are embedded here.
pub const SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS: [ExpandedSettingsRuntimeBlock; 12] = [
    runtime_block(0x69, 0x005b_5880, 0x90, &[]),
    runtime_block(0x72, 0x005b_5918, 0x60, &[]),
    runtime_block(0x172, 0x005b_5980, 0x50, TABLE_BLOCK_MUTABLE),
    runtime_block(0x173, 0x005b_59d8, 0x50, BASE_LOAD_MUTABLE),
    runtime_block(0x19f, 0x005b_5a30, 0x50, MAPPED_CALL_MUTABLE),
    runtime_block(0x1db, 0x005b_5a88, 0x70, BASE_PAIR_MUTABLE),
    runtime_block(0x213, 0x005b_5afc, 0x20, &[]),
    runtime_block(0x215, 0x005b_5b20, 0xd0, CONDITIONAL_OPCODE_MUTABLE),
    runtime_block(0x216, 0x005b_5bf8, 0x40, SPECIAL_RECORD_MUTABLE),
    runtime_block(0x219, 0x005b_5c3c, 0x30, &[]),
    runtime_block(0x21c, 0x005b_5c70, 0x220, LARGE_RUNTIME_MUTABLE),
    runtime_block(0x220, 0x005b_5e98, 0x150, &[]),
];

/// Descriptor blocks currently emitted from independent semantic Rust builders.
pub const SMW_US_V1_GENERATED_EXPANDED_SETTINGS_RUNTIME_BLOCKS: [usize; 12] = [
    0x69, 0x72, 0x172, 0x173, 0x19f, 0x1db, 0x213, 0x215, 0x216, 0x219, 0x21c, 0x220,
];

pub(crate) const fn runtime_block(
    descriptor_index: usize,
    embedded_template_va: u32,
    len: usize,
    mutable_spans: &'static [RuntimeMutableSpan],
) -> ExpandedSettingsRuntimeBlock {
    ExpandedSettingsRuntimeBlock {
        descriptor_index,
        embedded_template_va,
        len,
        mutable_spans,
    }
}

/// Confirms that an installed block differs from its embedded template only at recovered mutable
/// operands and configuration bytes.
///
/// # Errors
///
/// Rejects incorrect slice lengths, invalid span metadata, or any unexplained byte difference.
pub fn verify_expanded_settings_runtime_block(
    block: ExpandedSettingsRuntimeBlock,
    template: &[u8],
    installed: &[u8],
) -> Result<(), RuntimeBlockVerificationError> {
    if template.len() != block.len {
        return Err(RuntimeBlockVerificationError::WrongTemplateLength {
            expected: block.len,
            actual: template.len(),
        });
    }
    if installed.len() != block.len {
        return Err(RuntimeBlockVerificationError::WrongInstalledLength {
            expected: block.len,
            actual: installed.len(),
        });
    }
    for span in block.mutable_spans {
        if span
            .offset
            .checked_add(span.len)
            .is_none_or(|end| end > block.len)
        {
            return Err(RuntimeBlockVerificationError::MutableSpanOutOfBounds {
                offset: span.offset,
                len: span.len,
            });
        }
    }
    for (offset, (&template, &installed)) in template.iter().zip(installed).enumerate() {
        let mutable = block
            .mutable_spans
            .iter()
            .any(|span| (span.offset..span.offset + span.len).contains(&offset));
        if !mutable && template != installed {
            return Err(RuntimeBlockVerificationError::UnexpectedDifference {
                offset,
                template,
                installed,
            });
        }
    }
    Ok(())
}

/// Five fixed-runtime operands patched by `PatchExpandedLevelHeaderTablePointers`.
///
/// Each site is descriptor entry `$35..$39` plus `$6C`; each receives the mapped address of
/// descriptor entry `$70`. Entry `$70` is a separately installed runtime payload, not the
/// dynamically allocated `$6E00` settings block.
pub const SMW_US_V1_EXPANDED_HEADER_FIXED_RUNTIME_RELOCATIONS: [ExpandedSettingsRelocation; 5] = [
    fixed_runtime_relocation(0x35),
    fixed_runtime_relocation(0x36),
    fixed_runtime_relocation(0x37),
    fixed_runtime_relocation(0x38),
    fixed_runtime_relocation(0x39),
];

const fn fixed_runtime_relocation(site_descriptor_index: usize) -> ExpandedSettingsRelocation {
    ExpandedSettingsRelocation {
        site_descriptor_index,
        site_addend: 0x6c,
        target: ExpandedSettingsRelocationTarget::DescriptorEntry {
            index: 0x70,
            addend: 0,
        },
    }
}

/// Allocation-dependent publications recovered from `InstallExpandedLevelHeaderRuntime`.
///
/// These are the direct mapped-scalar writes visible in the installer after allocation. Additional
/// range-copy operations and fixed-runtime links are intentionally represented elsewhere.
pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_RELOCATIONS: [ExpandedSettingsRelocation; 4] = [
    ExpandedSettingsRelocation {
        site_descriptor_index: 0x172,
        site_addend: 0x0f,
        target: ExpandedSettingsRelocationTarget::RecordTable,
    },
    ExpandedSettingsRelocation {
        site_descriptor_index: 0x1db,
        site_addend: 0x37,
        target: ExpandedSettingsRelocationTarget::AllocationBase,
    },
    ExpandedSettingsRelocation {
        site_descriptor_index: 0x1db,
        site_addend: 0x3d,
        target: ExpandedSettingsRelocationTarget::AllocationBaseAddend(1),
    },
    ExpandedSettingsRelocation {
        site_descriptor_index: 0x173,
        site_addend: 0x33,
        target: ExpandedSettingsRelocationTarget::AllocationBase,
    },
];

impl ExpandedSettingsRelocationTarget {
    /// Resolves allocation-relative targets. Descriptor-relative targets require the active
    /// revision descriptor and therefore return `None`.
    #[must_use]
    pub const fn allocation_addend(self) -> Option<usize> {
        match self {
            Self::AllocationBase => Some(0),
            Self::AllocationBaseAddend(addend) => Some(addend),
            Self::RecordTable => Some(SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN),
            Self::SpecialRecord => Some(SMW_US_V1_EXPANDED_SETTINGS_SPECIAL_RECORD_OFFSET),
            Self::DescriptorEntry { .. } => None,
        }
    }
}
