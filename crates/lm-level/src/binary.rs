use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BinaryError {
    UnexpectedEnd {
        offset: usize,
        needed: usize,
    },
    InvalidValue {
        offset: usize,
        description: &'static str,
    },
}

impl fmt::Display for BinaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd { offset, needed } => {
                write!(f, "need {needed} bytes at offset {offset:#x}")
            }
            Self::InvalidValue {
                offset,
                description,
            } => {
                write!(f, "invalid value at offset {offset:#x}: {description}")
            }
        }
    }
}

impl std::error::Error for BinaryError {}

#[derive(Clone, Debug)]
pub struct ByteCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    #[must_use]
    pub const fn position(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    /// Reads one byte.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEnd`] at end of input.
    pub fn u8(&mut self) -> Result<u8, BinaryError> {
        let value = *self
            .bytes
            .get(self.offset)
            .ok_or(BinaryError::UnexpectedEnd {
                offset: self.offset,
                needed: 1,
            })?;
        self.offset += 1;
        Ok(value)
    }

    /// Reads a little-endian 16-bit value.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEnd`] when fewer than two bytes remain.
    pub fn u16_le(&mut self) -> Result<u16, BinaryError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Reads a little-endian 32-bit value.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEnd`] when fewer than four bytes remain.
    pub fn u32_le(&mut self) -> Result<u32, BinaryError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads a bounded byte slice.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEnd`] when the requested range is unavailable.
    pub fn take(&mut self, len: usize) -> Result<&'a [u8], BinaryError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(BinaryError::UnexpectedEnd {
                offset: self.offset,
                needed: len,
            })?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(BinaryError::UnexpectedEnd {
                offset: self.offset,
                needed: len,
            })?;
        self.offset = end;
        Ok(value)
    }
}
