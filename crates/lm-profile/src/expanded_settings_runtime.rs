//! Recovered SMW US revision-0 expanded-settings runtime relocation contracts.
//!
//! Lunar Magic's ROM-layout descriptor contains logical PC offsets. The installer uses those
//! offsets as code/data sites and publishes mapped SNES addresses into their operands. This module
//! records the verified relationships without pretending that the complete installer is available.

use lm_snes::CodeBuilderError;

mod builders;
mod contracts;

pub use builders::*;
#[cfg(test)]
pub(crate) use contracts::runtime_block;
pub use contracts::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsRelocationTarget {
    AllocationBase,
    AllocationBaseAddend(usize),
    RecordTable,
    SpecialRecord,
    DescriptorEntry { index: usize, addend: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsRelocation {
    pub site_descriptor_index: usize,
    pub site_addend: usize,
    pub target: ExpandedSettingsRelocationTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeMutableSpan {
    pub offset: usize,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsRuntimeBlock {
    pub descriptor_index: usize,
    pub embedded_template_va: u32,
    pub len: usize,
    /// Bytes changed after the embedded template is copied. Spans include complete operands even
    /// when a particular fixture changes only one byte of that operand.
    pub mutable_spans: &'static [RuntimeMutableSpan],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBlockVerificationError {
    WrongTemplateLength {
        expected: usize,
        actual: usize,
    },
    WrongInstalledLength {
        expected: usize,
        actual: usize,
    },
    MutableSpanOutOfBounds {
        offset: usize,
        len: usize,
    },
    UnexpectedDifference {
        offset: usize,
        template: u8,
        installed: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsRuntimeBuildError {
    Code(CodeBuilderError),
    AddressOutOfRange(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsEntryContinuation {
    Return,
    Continue,
}

impl std::fmt::Display for ExpandedSettingsRuntimeBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings runtime construction failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsRuntimeBuildError {}

impl From<CodeBuilderError> for ExpandedSettingsRuntimeBuildError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

impl std::fmt::Display for RuntimeBlockVerificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded-settings runtime block error: {self:?}")
    }
}

impl std::error::Error for RuntimeBlockVerificationError {}

#[cfg(test)]
mod tests;
