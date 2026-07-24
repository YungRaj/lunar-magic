use super::AllocationError;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedRange(pub Range<usize>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationPolicy {
    pub search: Range<usize>,
    pub bank_size: Option<usize>,
    pub fill_bytes: Vec<u8>,
    pub protected: Vec<ProtectedRange>,
}

impl AllocationPolicy {
    #[must_use]
    pub fn lorom(search: Range<usize>) -> Self {
        Self {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0x00, 0xff],
            protected: Vec::new(),
        }
    }

    /// Reports whether `range` intersects bytes reserved from allocation or reuse.
    #[must_use]
    pub fn protects(&self, range: &Range<usize>) -> bool {
        self.protected
            .iter()
            .any(|protected| overlaps(range, &protected.0))
    }

    /// Reports whether the complete tagged allocation fits within one configured mapper bank.
    #[must_use]
    pub fn fits_bank(&self, range: &Range<usize>) -> bool {
        self.bank_size.is_none_or(|bank| {
            bank != 0 && !range.is_empty() && range.start / bank == (range.end - 1) / bank
        })
    }

    /// Reports whether a complete allocation is authorized for placement or duplicate reuse.
    #[must_use]
    pub fn permits_allocation(&self, range: &Range<usize>) -> bool {
        !range.is_empty()
            && range.start >= self.search.start
            && range.end <= self.search.end
            && self.fits_bank(range)
            && !self.protects(range)
    }

    /// Validates search, bank, fill-byte, and protected-range bounds for an image length.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError::InvalidPolicy`] for an empty/out-of-image search, empty fill
    /// set, zero bank size, or malformed protected range.
    pub fn validate(&self, image_len: usize) -> Result<(), AllocationError> {
        if self.search.start >= self.search.end
            || self.search.end > image_len
            || self.fill_bytes.is_empty()
            || self.bank_size == Some(0)
            || self
                .protected
                .iter()
                .any(|range| range.0.start > range.0.end || range.0.end > image_len)
        {
            Err(AllocationError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
