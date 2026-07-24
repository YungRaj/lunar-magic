//! Small, deterministic 65C816 runtime-code construction primitives.

mod builder;
mod instruction;

pub use builder::{
    AssembledCode, CodeBuilder, CodeBuilderError, Label, LongAddressFixup, LongAddressTarget,
};
pub use instruction::{BranchCondition, IndexWidth, RegisterWidth};
