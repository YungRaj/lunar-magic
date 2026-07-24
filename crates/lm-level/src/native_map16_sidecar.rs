use std::fmt;

/// Fixed custom-object Map16 sidecar loaded and saved as exactly `0x2000` bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M16Sidecar {
    bytes: Vec<u8>,
}

impl M16Sidecar {
    pub const ENCODED_LEN: usize = 0x2000;
    pub const ENTRY_COUNT: usize = Self::ENCODED_LEN / 4;

    /// Decodes the exact native `.m16` buffer.
    ///
    /// # Errors
    ///
    /// Returns the actual length unless it is exactly `0x2000` bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeMap16SidecarError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(NativeMap16SidecarError::M16Length(bytes.len()));
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    #[must_use]
    pub fn entry(&self, index: usize) -> Option<u32> {
        read_entry(&self.bytes, index)
    }

    /// Replaces one raw little-endian 32-bit entry.
    ///
    /// # Errors
    ///
    /// Rejects an index outside the recovered 2,048-entry buffer.
    pub fn set_entry(&mut self, index: usize, value: u32) -> Result<(), NativeMap16SidecarError> {
        write_entry(&mut self.bytes, index, value, Self::ENTRY_COUNT)
    }
}

/// Sprite Map16 sidecar represented as Lunar Magic's complete zero-filled working buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct S16Sidecar {
    bytes: Vec<u8>,
    loaded_len: usize,
}

impl S16Sidecar {
    pub const CAPACITY: usize = 0x1c000;
    pub const ENTRY_COUNT: usize = Self::CAPACITY / 4;
    pub const BLOCK_LEN: usize = 0x800;

    /// Loads an `.s16` prefix into a zeroed native-capacity buffer.
    ///
    /// This deliberately accepts non-block and non-dword-aligned inputs because the recovered
    /// loader reads any prefix up to capacity. Canonical saving resolves them to whole blocks.
    ///
    /// # Errors
    ///
    /// Rejects inputs exceeding the recovered `0x1c000`-byte working buffer.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeMap16SidecarError> {
        if bytes.len() > Self::CAPACITY {
            return Err(NativeMap16SidecarError::S16TooLarge(bytes.len()));
        }
        let mut buffer = vec![0; Self::CAPACITY];
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: buffer,
            loaded_len: bytes.len(),
        })
    }

    #[must_use]
    pub const fn loaded_len(&self) -> usize {
        self.loaded_len
    }

    #[must_use]
    pub fn entry(&self, index: usize) -> Option<u32> {
        read_entry(&self.bytes, index)
    }

    /// Replaces one raw entry in the complete working buffer.
    ///
    /// # Errors
    ///
    /// Rejects an index outside the recovered 28,672-entry capacity.
    pub fn set_entry(&mut self, index: usize, value: u32) -> Result<(), NativeMap16SidecarError> {
        write_entry(&mut self.bytes, index, value, Self::ENTRY_COUNT)
    }

    /// Returns the exact recovered save length: through the last nonzero dword, rounded upward to
    /// `0x800`, with one block emitted for an all-zero buffer.
    #[must_use]
    pub fn canonical_len(&self) -> usize {
        let used = self
            .bytes
            .chunks_exact(4)
            .rposition(|entry| entry != [0, 0, 0, 0])
            .map_or(0, |index| (index + 1) * 4);
        used.max(1).div_ceil(Self::BLOCK_LEN) * Self::BLOCK_LEN
    }

    #[must_use]
    pub fn encode_canonical(&self) -> Vec<u8> {
        self.bytes[..self.canonical_len()].to_vec()
    }
}

fn read_entry(bytes: &[u8], index: usize) -> Option<u32> {
    let offset = index.checked_mul(4)?;
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn write_entry(
    bytes: &mut [u8],
    index: usize,
    value: u32,
    count: usize,
) -> Result<(), NativeMap16SidecarError> {
    if index >= count {
        return Err(NativeMap16SidecarError::InvalidEntry(index));
    }
    let offset = index * 4;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMap16SidecarError {
    M16Length(usize),
    S16TooLarge(usize),
    InvalidEntry(usize),
}

impl fmt::Display for NativeMap16SidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native Map16 sidecar: {self:?}")
    }
}

impl std::error::Error for NativeMap16SidecarError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m16_is_exact_and_entries_are_little_endian() {
        let mut bytes = vec![0; M16Sidecar::ENCODED_LEN];
        bytes[4..8].copy_from_slice(&0x4433_2211_u32.to_le_bytes());
        let mut sidecar = M16Sidecar::decode(&bytes).unwrap();
        assert_eq!(sidecar.entry(1), Some(0x4433_2211));
        sidecar
            .set_entry(M16Sidecar::ENTRY_COUNT - 1, 0xaabb_ccdd)
            .unwrap();
        assert_eq!(&sidecar.encode()[0x1ffc..], &[0xdd, 0xcc, 0xbb, 0xaa]);
        assert_eq!(
            M16Sidecar::decode(&bytes[..0x1fff]),
            Err(NativeMap16SidecarError::M16Length(0x1fff))
        );
        assert_eq!(
            sidecar.set_entry(M16Sidecar::ENTRY_COUNT, 0),
            Err(NativeMap16SidecarError::InvalidEntry(
                M16Sidecar::ENTRY_COUNT
            ))
        );
    }

    #[test]
    fn s16_zero_fills_arbitrary_loaded_prefixes() {
        let sidecar = S16Sidecar::decode(&[1, 2, 3]).unwrap();
        assert_eq!(sidecar.loaded_len(), 3);
        assert_eq!(sidecar.entry(0), Some(0x0003_0201));
        assert_eq!(sidecar.entry(1), Some(0));
        assert_eq!(sidecar.canonical_len(), S16Sidecar::BLOCK_LEN);
        assert_eq!(sidecar.encode_canonical().len(), S16Sidecar::BLOCK_LEN);
    }

    #[test]
    fn s16_rounds_last_nonzero_dword_to_native_blocks() {
        let mut sidecar = S16Sidecar::decode(&[]).unwrap();
        assert_eq!(sidecar.canonical_len(), 0x800);
        sidecar.set_entry(0x1ff, 1).unwrap();
        assert_eq!(sidecar.canonical_len(), 0x800);
        sidecar.set_entry(0x200, 1).unwrap();
        assert_eq!(sidecar.canonical_len(), 0x1000);
        sidecar.set_entry(S16Sidecar::ENTRY_COUNT - 1, 1).unwrap();
        assert_eq!(sidecar.canonical_len(), S16Sidecar::CAPACITY);
    }

    #[test]
    fn s16_limits_and_edits_are_checked() {
        assert_eq!(
            S16Sidecar::decode(&vec![0; S16Sidecar::CAPACITY + 1]),
            Err(NativeMap16SidecarError::S16TooLarge(
                S16Sidecar::CAPACITY + 1
            ))
        );
        let mut sidecar = S16Sidecar::decode(&[]).unwrap();
        assert_eq!(
            sidecar.set_entry(S16Sidecar::ENTRY_COUNT, 1),
            Err(NativeMap16SidecarError::InvalidEntry(
                S16Sidecar::ENTRY_COUNT
            ))
        );
    }
}
