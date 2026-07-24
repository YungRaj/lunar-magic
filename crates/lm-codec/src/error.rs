use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodecError {
    InvalidDecodedLength(usize),
    UnexpectedEnd,
    MissingTerminator,
    InvalidBackReference { offset: usize, produced: usize },
    OutputLimitExceeded { limit: usize },
    TrailingCompressedData(usize),
    UnsupportedLz2Command(u8),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDecodedLength(length) => {
                write!(f, "invalid decoded byte length {length}")
            }
            Self::UnexpectedEnd => write!(f, "compressed stream ended unexpectedly"),
            Self::MissingTerminator => write!(f, "compressed stream has no terminator"),
            Self::InvalidBackReference { offset, produced } => write!(
                f,
                "back-reference {offset:#x} is outside {produced:#x} produced bytes"
            ),
            Self::OutputLimitExceeded { limit } => {
                write!(f, "decoded output exceeds limit {limit:#x}")
            }
            Self::TrailingCompressedData(bytes) => {
                write!(f, "compressed stream has {bytes} trailing bytes")
            }
            Self::UnsupportedLz2Command(command) => {
                write!(f, "LZ2 command {command} is reserved or invalid")
            }
        }
    }
}

impl std::error::Error for CodecError {}
