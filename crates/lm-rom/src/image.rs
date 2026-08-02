use crate::{
    COPIER_HEADER_LEN, CopierHeader, Mapper, RomError, detect_copier_header,
    mapper_supports_image_len,
};
use std::ops::Range;

const MINIMUM_ROM_LEN: usize = 0x8000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeRange {
    pub range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct RomImage {
    bytes: Vec<u8>,
    original: Vec<u8>,
    header: CopierHeader,
    original_header: CopierHeader,
}

impl RomImage {
    /// Creates an image and detects its optional 512-byte copier header.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::ImageTooSmall`] when fewer than one `LoROM` bank is supplied.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RomError> {
        if bytes.len() < MINIMUM_ROM_LEN {
            return Err(RomError::ImageTooSmall);
        }
        let header = detect_copier_header(bytes.len());
        Ok(Self {
            original: bytes.clone(),
            bytes,
            header,
            original_header: header,
        })
    }

    /// Reconstructs an image whose accepted baseline and current file bytes differ.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::ImageTooSmall`] if either physical image is too small.
    pub fn from_recovery(original: Vec<u8>, current: Vec<u8>) -> Result<Self, RomError> {
        let mut image = Self::from_bytes(original)?;
        let current = Self::from_bytes(current)?;
        image.bytes = current.bytes;
        image.header = current.header;
        Ok(image)
    }

    #[must_use]
    pub const fn copier_header(&self) -> CopierHeader {
        self.header
    }
    #[must_use]
    pub fn logical_len(&self) -> usize {
        self.bytes.len() - self.header.byte_len()
    }
    #[must_use]
    pub fn as_file_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact physical bytes of the last accepted save baseline.
    #[must_use]
    pub fn original_file_bytes(&self) -> &[u8] {
        &self.original
    }
    #[must_use]
    pub fn logical_bytes(&self) -> &[u8] {
        &self.bytes[self.header.byte_len()..]
    }

    /// Returns the exact optional 512-byte copier header.
    #[must_use]
    pub fn copier_header_bytes(&self) -> Option<&[u8]> {
        (self.header == CopierHeader::Present).then(|| &self.bytes[..COPIER_HEADER_LEN])
    }

    /// Reports whether any physical file byte or copier-header state differs from the accepted
    /// baseline.
    #[must_use]
    pub fn has_file_changes(&self) -> bool {
        self.header != self.original_header || self.bytes != self.original
    }

    /// Adds or removes the 512-byte copier header without changing any logical ROM byte.
    ///
    /// Newly added headers are filled with `fill`. Removing a header discards only those 512 file
    /// bytes. Returns `false` when the requested state is already active.
    pub fn set_copier_header(&mut self, header: CopierHeader, fill: u8) -> bool {
        if self.header == header {
            return false;
        }
        match header {
            CopierHeader::Present => {
                self.bytes
                    .splice(..0, std::iter::repeat_n(fill, COPIER_HEADER_LEN));
            }
            CopierHeader::Absent => {
                self.bytes.drain(..COPIER_HEADER_LEN);
            }
        }
        self.header = header;
        true
    }

    /// Compare-replaces the exact optional copier header without touching logical ROM bytes.
    ///
    /// `None` denotes an absent header; a present header must contain exactly 512 bytes. The
    /// current state and bytes must match `expected`, making this suitable for stale-safe history.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::BytesMismatch`] when either shape is invalid or the current header does
    /// not exactly match `expected`.
    pub fn replace_copier_header_exact(
        &mut self,
        expected: Option<&[u8]>,
        replacement: Option<&[u8]>,
    ) -> Result<(), RomError> {
        if expected.is_some_and(|bytes| bytes.len() != COPIER_HEADER_LEN)
            || replacement.is_some_and(|bytes| bytes.len() != COPIER_HEADER_LEN)
            || self.copier_header_bytes() != expected
        {
            return Err(RomError::BytesMismatch {
                offset: 0,
                len: COPIER_HEADER_LEN,
            });
        }
        if let Some(bytes) = replacement {
            if self.header == CopierHeader::Present {
                self.bytes[..COPIER_HEADER_LEN].copy_from_slice(bytes);
            } else {
                self.bytes.splice(..0, bytes.iter().copied());
            }
            self.header = CopierHeader::Present;
        } else {
            if self.header == CopierHeader::Present {
                self.bytes.drain(..COPIER_HEADER_LEN);
            }
            self.header = CopierHeader::Absent;
        }
        Ok(())
    }

    /// Reads a range using logical, headerless offsets.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::RangeOutOfBounds`] if any byte falls outside the image.
    pub fn read(&self, offset: usize, len: usize) -> Result<&[u8], RomError> {
        Ok(&self.bytes[self.file_range(offset, len)?])
    }

    /// Writes a range using logical, headerless offsets.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::RangeOutOfBounds`] if any byte falls outside the image.
    pub fn write(&mut self, offset: usize, value: &[u8]) -> Result<(), RomError> {
        let range = self.file_range(offset, value.len())?;
        self.bytes[range].copy_from_slice(value);
        Ok(())
    }

    /// Replaces a fixed-size logical range only when its current bytes exactly match `expected`.
    ///
    /// This compare-and-replace primitive prevents stale undo, redo, and prepared edits from
    /// overwriting a newer mutation that happened outside their history sequence.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::RangeOutOfBounds`] if the range is outside the image,
    /// [`RomError::BytesMismatch`] if its contents changed, or that same mismatch error when the
    /// expected and replacement lengths differ.
    pub fn replace_exact(
        &mut self,
        offset: usize,
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<(), RomError> {
        if expected.len() != replacement.len() {
            return Err(RomError::BytesMismatch {
                offset,
                len: expected.len().max(replacement.len()),
            });
        }
        let range = self.file_range(offset, expected.len())?;
        if self.bytes[range.clone()] != *expected {
            return Err(RomError::BytesMismatch {
                offset,
                len: expected.len(),
            });
        }
        self.bytes[range].copy_from_slice(replacement);
        Ok(())
    }

    /// Replaces the complete logical tail after verifying its current bytes exactly.
    ///
    /// This is the reversible primitive used by transaction history for ROM expansion. Requiring
    /// the complete expected tail prevents a stale resize edit from truncating unrelated data.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::TailMismatch`] if `expected` is not the complete current tail, or
    /// [`RomError::ImageTooSmall`] if replacement would produce an undersized ROM.
    pub fn replace_logical_tail(
        &mut self,
        offset: usize,
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<(), RomError> {
        let current = self.logical_bytes();
        if current.get(offset..) != Some(expected) {
            return Err(RomError::TailMismatch { offset });
        }
        let new_logical_len =
            offset
                .checked_add(replacement.len())
                .ok_or(RomError::RangeOutOfBounds {
                    offset,
                    len: replacement.len(),
                    image_len: current.len(),
                })?;
        if new_logical_len < MINIMUM_ROM_LEN {
            return Err(RomError::ImageTooSmall);
        }
        let file_offset = self.header.byte_len() + offset;
        self.bytes.truncate(file_offset);
        self.bytes.extend_from_slice(replacement);
        Ok(())
    }

    /// Expands the logical image to a complete `LoROM` bank using `fill` for new bytes.
    ///
    /// Existing bytes and an optional copier header remain at their original file positions. The
    /// target must be representable by `mapper`; shrinking is deliberately unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::CannotShrink`] for a smaller target or
    /// [`RomError::InvalidExpansionSize`] for unaligned/unrepresentable sizes.
    pub fn expand(
        &mut self,
        mapper: Mapper,
        target_logical_len: usize,
        fill: u8,
    ) -> Result<bool, RomError> {
        let current = self.logical_len();
        if target_logical_len < current {
            return Err(RomError::CannotShrink {
                current,
                requested: target_logical_len,
            });
        }
        if target_logical_len == current {
            return Ok(false);
        }
        if !mapper_supports_image_len(mapper, target_logical_len) {
            return Err(RomError::InvalidExpansionSize(target_logical_len));
        }
        let file_len = self
            .header
            .byte_len()
            .checked_add(target_logical_len)
            .ok_or(RomError::InvalidExpansionSize(target_logical_len))?;
        self.bytes.resize(file_len, fill);
        Ok(true)
    }

    pub fn restore_original(&mut self) {
        self.bytes.clone_from(&self.original);
        self.header = self.original_header;
    }
    pub fn accept_changes(&mut self) {
        self.original.clone_from(&self.bytes);
        self.original_header = self.header;
    }

    #[must_use]
    pub fn changed_ranges(&self) -> Vec<ChangeRange> {
        let mut result = Vec::new();
        let mut start = None;
        let original = &self.original[self.original_header.byte_len()..];
        let current = &self.bytes[self.header.byte_len()..];
        for index in 0..original.len().max(current.len()) {
            let before = original.get(index);
            let after = current.get(index);
            if before != after {
                start.get_or_insert(index);
            } else if let Some(begin) = start.take() {
                result.push(ChangeRange {
                    range: begin..index,
                });
            }
        }
        if let Some(begin) = start {
            result.push(ChangeRange {
                range: begin..original.len().max(current.len()),
            });
        }
        result
    }

    fn file_range(&self, offset: usize, len: usize) -> Result<Range<usize>, RomError> {
        let end = offset.checked_add(len).ok_or(RomError::RangeOutOfBounds {
            offset,
            len,
            image_len: self.logical_len(),
        })?;
        if end > self.logical_len() {
            return Err(RomError::RangeOutOfBounds {
                offset,
                len,
                image_len: self.logical_len(),
            });
        }
        let base = self.header.byte_len();
        Ok(base + offset..base + end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_is_transparent() {
        let mut bytes = vec![0; 0x80200];
        bytes[0x200] = 7;
        assert_eq!(
            RomImage::from_bytes(bytes).unwrap().read(0, 1).unwrap(),
            &[7]
        );
    }
    #[test]
    fn changes_coalesce() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        rom.write(3, &[1, 2]).unwrap();
        assert_eq!(rom.changed_ranges(), vec![ChangeRange { range: 3..5 }]);
    }

    #[test]
    fn copier_header_conversion_preserves_logical_bytes_and_original_state() {
        let logical = vec![0x5a; 0x8000];
        let mut rom = RomImage::from_bytes(logical.clone()).unwrap();
        assert!(!rom.has_file_changes());
        assert!(rom.set_copier_header(CopierHeader::Present, 0xa5));
        assert!(rom.has_file_changes());
        assert_eq!(rom.copier_header(), CopierHeader::Present);
        assert_eq!(rom.logical_bytes(), logical);
        assert!(
            rom.as_file_bytes()[..COPIER_HEADER_LEN]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        assert!(rom.changed_ranges().is_empty());
        assert!(!rom.set_copier_header(CopierHeader::Present, 0));
        rom.restore_original();
        assert!(!rom.has_file_changes());
        assert_eq!(rom.copier_header(), CopierHeader::Absent);
        assert_eq!(rom.as_file_bytes(), logical);

        rom.set_copier_header(CopierHeader::Present, 0x11);
        rom.accept_changes();
        rom.set_copier_header(CopierHeader::Absent, 0);
        rom.restore_original();
        assert_eq!(rom.copier_header(), CopierHeader::Present);
        assert_eq!(rom.logical_bytes(), logical);
    }

    #[test]
    fn exact_copier_header_replacement_is_shape_and_content_guarded() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let first = vec![0x12; COPIER_HEADER_LEN];
        let second = vec![0x34; COPIER_HEADER_LEN];
        rom.replace_copier_header_exact(None, Some(&first)).unwrap();
        assert_eq!(rom.copier_header_bytes(), Some(first.as_slice()));
        assert!(matches!(
            rom.replace_copier_header_exact(None, Some(&second)),
            Err(RomError::BytesMismatch { .. })
        ));
        assert!(matches!(
            rom.replace_copier_header_exact(Some(&first[..COPIER_HEADER_LEN - 1]), None),
            Err(RomError::BytesMismatch { .. })
        ));
        rom.replace_copier_header_exact(Some(&first), Some(&second))
            .unwrap();
        rom.replace_copier_header_exact(Some(&second), None)
            .unwrap();
        assert_eq!(rom.copier_header(), CopierHeader::Absent);
    }

    #[test]
    fn exact_replacement_is_header_transparent_and_compare_guarded() {
        let mut bytes = vec![0x55; 0x8200];
        bytes[0x200 + 4..0x200 + 7].copy_from_slice(&[1, 2, 3]);
        let mut rom = RomImage::from_bytes(bytes).unwrap();
        rom.replace_exact(4, &[1, 2, 3], &[7, 8, 9]).unwrap();
        assert_eq!(rom.read(4, 3).unwrap(), [7, 8, 9]);
        assert_eq!(
            rom.replace_exact(4, &[1, 2, 3], &[0, 0, 0]),
            Err(RomError::BytesMismatch { offset: 4, len: 3 })
        );
        assert_eq!(rom.read(4, 3).unwrap(), [7, 8, 9]);
        assert_eq!(
            rom.replace_exact(4, &[7, 8], &[1]),
            Err(RomError::BytesMismatch { offset: 4, len: 2 })
        );
    }

    #[test]
    fn expansion_preserves_header_and_tracks_the_new_tail() {
        let mut bytes = vec![0xaa; 0x80200];
        bytes[..0x200].fill(0x55);
        let mut rom = RomImage::from_bytes(bytes).unwrap();
        assert!(rom.expand(Mapper::LoRom, 0x0010_0000, 0xff).unwrap());
        assert_eq!(rom.copier_header(), CopierHeader::Present);
        assert!(
            rom.as_file_bytes()[..0x200]
                .iter()
                .all(|byte| *byte == 0x55)
        );
        assert!(
            rom.logical_bytes()[0x80000..]
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert_eq!(
            rom.changed_ranges(),
            vec![ChangeRange {
                range: 0x80000..0x0010_0000
            }]
        );
        rom.restore_original();
        assert_eq!(rom.logical_len(), 0x80000);
    }

    #[test]
    fn expansion_rejects_shrink_alignment_and_mapper_overflow() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        assert!(matches!(
            rom.expand(Mapper::LoRom, 0x4000, 0xff),
            Err(RomError::CannotShrink { .. })
        ));
        assert!(matches!(
            rom.expand(Mapper::LoRom, 0x8001, 0xff),
            Err(RomError::InvalidExpansionSize(0x8001))
        ));
        assert!(matches!(
            rom.expand(Mapper::LoRom, 0x0040_8000, 0xff),
            Err(RomError::InvalidExpansionSize(0x0040_8000))
        ));
        assert_eq!(rom.logical_len(), 0x8000);
    }

    #[test]
    fn exact_tail_replacement_is_reversible_and_header_transparent() {
        let mut bytes = vec![0x55; 0x200];
        bytes.extend(vec![0; 0x8000]);
        let mut rom = RomImage::from_bytes(bytes).unwrap();
        let appended = vec![1; 0x8000];
        let wrong = vec![2; 0x8000];
        rom.replace_logical_tail(0x8000, &[], &appended).unwrap();
        assert_eq!(rom.logical_len(), 0x10000);
        assert!(
            rom.as_file_bytes()[..0x200]
                .iter()
                .all(|byte| *byte == 0x55)
        );
        assert_eq!(
            rom.replace_logical_tail(0x8000, &wrong, &[]),
            Err(RomError::TailMismatch { offset: 0x8000 })
        );
        rom.replace_logical_tail(0x8000, &appended, &[]).unwrap();
        assert_eq!(rom.logical_len(), 0x8000);
    }

    #[test]
    fn recovery_retains_baseline_across_header_and_length_changes() {
        let original = vec![0x11; 0x8000];
        let mut current = vec![0x22; COPIER_HEADER_LEN];
        current.extend(vec![0x11; 0x1_0000]);
        let mut recovered = RomImage::from_recovery(original.clone(), current.clone()).unwrap();
        assert_eq!(recovered.original_file_bytes(), original);
        assert_eq!(recovered.as_file_bytes(), current);
        assert_eq!(recovered.copier_header(), CopierHeader::Present);
        assert!(recovered.has_file_changes());

        recovered.restore_original();
        assert_eq!(recovered.as_file_bytes(), original);
        assert_eq!(recovered.copier_header(), CopierHeader::Absent);
        assert!(!recovered.has_file_changes());
    }
}
