use crate::Mapper;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RomError {
    ImageTooSmall,
    RangeOutOfBounds {
        offset: usize,
        len: usize,
        image_len: usize,
    },
    InvalidSnesAddress(u32),
    UnrepresentablePcOffset(usize),
    CannotShrink {
        current: usize,
        requested: usize,
    },
    InvalidExpansionSize(usize),
    TailMismatch {
        offset: usize,
    },
    BytesMismatch {
        offset: usize,
        len: usize,
    },
    UnsupportedMapper(Mapper),
}

impl fmt::Display for RomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImageTooSmall => write!(f, "ROM image is too small"),
            Self::RangeOutOfBounds {
                offset,
                len,
                image_len,
            } => write!(
                f,
                "ROM range {offset:#x}..{:#x} exceeds image length {image_len:#x}",
                offset.saturating_add(*len)
            ),
            Self::InvalidSnesAddress(address) => write!(f, "invalid SNES address {address:#08x}"),
            Self::UnrepresentablePcOffset(offset) => write!(
                f,
                "PC offset {offset:#x} is not representable by this mapper"
            ),
            Self::CannotShrink { current, requested } => write!(
                f,
                "cannot shrink ROM from {current:#x} bytes to {requested:#x} bytes"
            ),
            Self::InvalidExpansionSize(size) => write!(
                f,
                "ROM expansion size {size:#x} is not aligned or representable by the mapper"
            ),
            Self::TailMismatch { offset } => {
                write!(
                    f,
                    "ROM tail at {offset:#x} no longer matches the recorded edit"
                )
            }
            Self::BytesMismatch { offset, len } => write!(
                f,
                "ROM bytes at {offset:#x}..{:#x} no longer match the recorded edit",
                offset.saturating_add(*len)
            ),
            Self::UnsupportedMapper(mapper) => write!(f, "mapper {mapper:?} is not implemented"),
        }
    }
}

impl std::error::Error for RomError {}
