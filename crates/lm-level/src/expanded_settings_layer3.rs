use crate::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};

const LAYER3_ENABLE_WORD: usize = 0;
const LAYER3_ENABLE_MASK: u16 = 0x2000;
const LAYER3_TILEMAP_DESCRIPTOR_WORD: usize = 1;
const FILE_MASK: u16 = 0x0fff;
const LENGTH_SHIFT: u16 = 12;
const OFFSET_SHIFT: u16 = 14;
const SELECTOR_MASK: u16 = 3;
const DESTINATION_WORD_OFFSETS: [u16; 4] = [0, 0, 0, 0x0800];
const REQUESTED_BYTE_LENGTHS: [u16; 4] = [0x2000, 0x1000, 0x0800, 0];
const TILEMAP_WORD_CAPACITY: u16 = 0x1000;

/// Lunar Magic's packed per-level Layer 3 tilemap graphics descriptor.
///
/// The low twelve bits select GFX/ExGFX. Bits 12–13 select requested byte length and bits 14–15
/// select the destination word offset. Selectors are retained exactly because multiple selector
/// values currently map to offset zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3TilemapGraphicsDescriptor(u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3TilemapGraphicsDescriptorError {
    FileOutOfRange(u16),
    LengthSelector(u8),
    OffsetSelector(u8),
}

impl std::fmt::Display for Layer3TilemapGraphicsDescriptorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Layer 3 tilemap graphics descriptor error: {self:?}"
        )
    }
}

impl std::error::Error for Layer3TilemapGraphicsDescriptorError {}

impl Layer3TilemapGraphicsDescriptor {
    #[must_use]
    pub const fn from_packed(packed: u16) -> Self {
        Self(packed)
    }

    /// Constructs one exact descriptor from its recovered fields.
    ///
    /// # Errors
    ///
    /// Rejects graphics identifiers above twelve bits and selectors above three.
    pub fn new(
        file: u16,
        length_selector: u8,
        offset_selector: u8,
    ) -> Result<Self, Layer3TilemapGraphicsDescriptorError> {
        if file > FILE_MASK {
            return Err(Layer3TilemapGraphicsDescriptorError::FileOutOfRange(file));
        }
        if usize::from(length_selector) >= REQUESTED_BYTE_LENGTHS.len() {
            return Err(Layer3TilemapGraphicsDescriptorError::LengthSelector(
                length_selector,
            ));
        }
        if usize::from(offset_selector) >= DESTINATION_WORD_OFFSETS.len() {
            return Err(Layer3TilemapGraphicsDescriptorError::OffsetSelector(
                offset_selector,
            ));
        }
        Ok(Self(
            file | (u16::from(length_selector) << LENGTH_SHIFT)
                | (u16::from(offset_selector) << OFFSET_SHIFT),
        ))
    }

    #[must_use]
    pub const fn packed(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn file(self) -> u16 {
        self.0 & FILE_MASK
    }

    #[must_use]
    pub const fn length_selector(self) -> u8 {
        ((self.0 >> LENGTH_SHIFT) & SELECTOR_MASK) as u8
    }

    #[must_use]
    pub const fn offset_selector(self) -> u8 {
        ((self.0 >> OFFSET_SHIFT) & SELECTOR_MASK) as u8
    }

    #[must_use]
    pub const fn destination_word_offset(self) -> u16 {
        DESTINATION_WORD_OFFSETS[self.offset_selector() as usize]
    }

    #[must_use]
    pub const fn destination_byte_offset(self) -> u16 {
        self.destination_word_offset() * 2
    }

    /// Returns Lunar Magic's requested length clipped to the remaining 0x2000-byte workspace.
    #[must_use]
    pub const fn effective_byte_length(self) -> u16 {
        let requested = REQUESTED_BYTE_LENGTHS[self.length_selector() as usize];
        let remaining_words = TILEMAP_WORD_CAPACITY - self.destination_word_offset();
        let remaining_bytes = remaining_words * 2;
        if requested < remaining_bytes {
            requested
        } else {
            remaining_bytes
        }
    }
}

impl ExpandedLevelSettingsRecord {
    #[must_use]
    pub fn layer3_tilemap_enabled(&self) -> bool {
        u16::from_le_bytes([self.encoded()[0], self.encoded()[1]]) & LAYER3_ENABLE_MASK != 0
    }

    /// Changes only the recovered Layer 3 enable bit in word 0.
    ///
    /// # Errors
    ///
    /// Propagates an internal fixed-record word-access error.
    pub fn set_layer3_tilemap_enabled(
        &mut self,
        enabled: bool,
    ) -> Result<(), ExpandedLevelSettingsError> {
        let word = self.word(LAYER3_ENABLE_WORD)?;
        self.set_word(
            LAYER3_ENABLE_WORD,
            if enabled {
                word | LAYER3_ENABLE_MASK
            } else {
                word & !LAYER3_ENABLE_MASK
            },
        )
    }

    /// Reads the exact packed descriptor from native word 1.
    ///
    /// # Errors
    ///
    /// Propagates an internal fixed-record word-access error.
    pub fn layer3_tilemap_graphics_descriptor(
        &self,
    ) -> Result<Layer3TilemapGraphicsDescriptor, ExpandedLevelSettingsError> {
        self.word(LAYER3_TILEMAP_DESCRIPTOR_WORD)
            .map(Layer3TilemapGraphicsDescriptor::from_packed)
    }

    /// Replaces only native word 1 with the exact packed descriptor.
    ///
    /// # Errors
    ///
    /// Propagates an internal fixed-record word-access error.
    pub fn set_layer3_tilemap_graphics_descriptor(
        &mut self,
        descriptor: Layer3TilemapGraphicsDescriptor,
    ) -> Result<(), ExpandedLevelSettingsError> {
        self.set_word(LAYER3_TILEMAP_DESCRIPTOR_WORD, descriptor.packed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_selector_tables_and_clipping_match_lunar_magic() {
        let full = Layer3TilemapGraphicsDescriptor::new(0xabc, 0, 0).unwrap();
        assert_eq!(full.destination_byte_offset(), 0);
        assert_eq!(full.effective_byte_length(), 0x2000);
        let clipped = Layer3TilemapGraphicsDescriptor::new(0x123, 0, 3).unwrap();
        assert_eq!(clipped.destination_byte_offset(), 0x1000);
        assert_eq!(clipped.effective_byte_length(), 0x1000);
        let empty = Layer3TilemapGraphicsDescriptor::new(0x7f, 3, 0).unwrap();
        assert_eq!(empty.effective_byte_length(), 0);
    }

    #[test]
    fn exact_selectors_round_trip_even_when_offsets_alias() {
        for length in 0..4 {
            for offset in 0..4 {
                let value = Layer3TilemapGraphicsDescriptor::new(0x456, length, offset).unwrap();
                assert_eq!(value.file(), 0x456);
                assert_eq!(value.length_selector(), length);
                assert_eq!(value.offset_selector(), offset);
                assert_eq!(
                    Layer3TilemapGraphicsDescriptor::from_packed(value.packed()),
                    value
                );
            }
        }
    }

    #[test]
    fn record_accessors_preserve_every_unrelated_bit_and_word() {
        let source = std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap());
        let mut record = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        let original_word_zero = record.word(0).unwrap();
        record.set_layer3_tilemap_enabled(true).unwrap();
        assert_eq!(
            record.word(0).unwrap(),
            original_word_zero | LAYER3_ENABLE_MASK
        );
        record.set_layer3_tilemap_enabled(false).unwrap();
        assert_eq!(
            record.word(0).unwrap(),
            original_word_zero & !LAYER3_ENABLE_MASK
        );
        let descriptor = Layer3TilemapGraphicsDescriptor::new(0xabc, 2, 3).unwrap();
        record
            .set_layer3_tilemap_graphics_descriptor(descriptor)
            .unwrap();
        assert_eq!(
            record.layer3_tilemap_graphics_descriptor().unwrap(),
            descriptor
        );
        assert_eq!(&record.encoded()[4..], &source[4..]);
    }
}
