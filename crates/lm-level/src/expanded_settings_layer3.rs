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
const MODE_LOW_NIBBLE_WORDS: [usize; 4] = [12, 13, 14, 15];
const MODE_HIGH_NIBBLE_WORDS: [usize; 4] = [8, 9, 10, 11];
const MODE_ENABLED_MASK: u32 = 1;
const MODE_ROW_SIGN_BIT: u32 = 0x0400;
const ORDINARY_EDITOR_ROW_BIAS: i16 = 12;

/// The recovered Layer 3 editor-row adjustment derived from expanded level settings.
///
/// `offset` is measured in Lunar Magic's 16-pixel editor rows. Some packed mode types clamp
/// source rows beyond row 30 instead of applying the ordinary mode's twelve-row bias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3ExpandedEditorRow {
    pub offset: i16,
    pub clamp_at_30: bool,
}

/// Proven Layer 3 composition state derived by Lunar Magic's slot dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3ExpandedComposition {
    /// Packed bit 31 moves Layer 3 from the primary mode mask to the alternate source mask.
    pub alternate_source_route: bool,
    /// Whether nontransparent Layer 3 pixels add to the current destination.
    pub additive: bool,
    /// Whether each Layer 3 source channel is halved before opaque or additive composition.
    pub half_color: bool,
}

/// Lunar Magic's exact packed high nibbles from expanded-settings words 8–15.
///
/// The enable bit, editor-row behavior, source route, and composition-mask transformation have
/// authenticated meanings here. The remaining bits are retained in `packed()` because they also
/// feed a larger unresolved slot-assignment and painter dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3ExpandedModeFlags(u32);

impl Layer3ExpandedModeFlags {
    #[must_use]
    pub const fn from_packed(packed: u32) -> Self {
        Self(packed)
    }

    #[must_use]
    pub const fn packed(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.0 & MODE_ENABLED_MASK != 0
    }

    /// Returns packed bit 31's recovered source-mask route while expanded mode is enabled.
    #[must_use]
    pub const fn alternate_layer3_source_route(self) -> Option<bool> {
        if self.enabled() {
            Some(self.0 & 0x8000_0000 != 0)
        } else {
            None
        }
    }

    /// Returns packed bit 30's primary-route Layer 3 additive input.
    ///
    /// The input is not consumed on the alternate source route, so that state returns `None`.
    #[must_use]
    pub const fn primary_layer3_additive_input(self) -> Option<bool> {
        if !self.enabled() || self.0 & 0x8000_0000 != 0 {
            None
        } else {
            Some(self.0 & 0x4000_0000 != 0)
        }
    }

    /// Resolves the exact Layer 3 color-composition fields consumed by the slot dispatcher.
    ///
    /// `base_composition_mask` is Lunar Magic's byte from the active level-mode table. Packed bit
    /// 30 replaces its Layer 3 additive-input bit before the dispatcher evaluates sign and
    /// half-color masks. Use `lunar_magic_level_layer_slots` when the painter position is needed.
    #[must_use]
    pub const fn layer3_composition(
        self,
        base_composition_mask: u8,
    ) -> Option<Layer3ExpandedComposition> {
        if !self.enabled() {
            return None;
        }
        let alternate_source_route = self.0 & 0x8000_0000 != 0;
        let composition_mask = if self.0 & 0x4000_0000 == 0 {
            base_composition_mask & !4
        } else {
            base_composition_mask | 4
        };
        let additive =
            !alternate_source_route && composition_mask & 4 != 0 && composition_mask & 0x80 == 0;
        let half_color = if alternate_source_route {
            composition_mask & 0x60 == 0x60
        } else {
            additive && composition_mask & 0x44 == 0x44
        };
        Some(Layer3ExpandedComposition {
            alternate_source_route,
            additive,
            half_color,
        })
    }

    /// Resolves the proven editor-row behavior for the active level configuration.
    ///
    /// Lunar Magic applies this state to Layer 3 setting 1, or setting 2 for every object tileset
    /// except tileset 1. Other settings ignore the packed row fields.
    #[must_use]
    pub fn editor_row(
        self,
        layer3_setting: u8,
        object_tileset: u8,
    ) -> Option<Layer3ExpandedEditorRow> {
        if !self.enabled() || !(layer3_setting == 1 || (layer3_setting == 2 && object_tileset != 1))
        {
            return None;
        }

        let bytes = self.0.to_le_bytes();
        let mode_type = (bytes[1] >> 4) | ((bytes[3] & 0x04) << 2);
        let encoded =
            u16::from(bytes[0] >> 3) | (u16::from((bytes[2] >> 4) | ((bytes[3] & 0x03) << 4)) << 5);
        let negative = u32::from(encoded) & MODE_ROW_SIGN_BIT != 0;
        let encoded = i16::from_le_bytes(encoded.to_le_bytes());
        let signed = if negative { encoded - 0x0800 } else { encoded };
        let clamp_at_30 = mode_type == 1 || (6..=0x11).contains(&mode_type);
        Some(Layer3ExpandedEditorRow {
            offset: if clamp_at_30 {
                signed
            } else {
                signed - ORDINARY_EDITOR_ROW_BIAS
            },
            clamp_at_30,
        })
    }
}

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
    /// Packs the high nibbles of words 12–15 followed by words 8–11 exactly as Lunar Magic does.
    #[must_use]
    pub fn layer3_expanded_mode_flags(&self) -> Layer3ExpandedModeFlags {
        let encoded = self.encoded();
        let mut packed = 0_u32;
        let mut nibble = 0_u32;
        while nibble < 4 {
            let low_word = MODE_LOW_NIBBLE_WORDS[nibble as usize];
            let high_word = MODE_HIGH_NIBBLE_WORDS[nibble as usize];
            let low = u16::from_le_bytes([encoded[low_word * 2], encoded[low_word * 2 + 1]]);
            let high = u16::from_le_bytes([encoded[high_word * 2], encoded[high_word * 2 + 1]]);
            packed |= u32::from(low >> 12) << (nibble * 4);
            packed |= u32::from(high >> 12) << ((nibble + 4) * 4);
            nibble += 1;
        }
        Layer3ExpandedModeFlags::from_packed(packed)
    }

    /// Replaces only the eight recovered high nibbles that form Lunar Magic's packed Layer 3
    /// expanded-mode value. Every adjacent low twelve-bit field remains byte-exact.
    ///
    /// # Errors
    ///
    /// Propagates an internal fixed-record word-access error.
    pub fn set_layer3_expanded_mode_flags(
        &mut self,
        flags: Layer3ExpandedModeFlags,
    ) -> Result<(), ExpandedLevelSettingsError> {
        let packed = flags.packed();
        for nibble in 0_usize..4 {
            let low_word = MODE_LOW_NIBBLE_WORDS[nibble];
            let high_word = MODE_HIGH_NIBBLE_WORDS[nibble];
            let low = self.word(low_word)? & 0x0fff;
            let high = self.word(high_word)? & 0x0fff;
            self.set_word(
                low_word,
                low | (((packed >> (nibble * 4)) as u16 & 0x000f) << 12),
            )?;
            self.set_word(
                high_word,
                high | (((packed >> ((nibble + 4) * 4)) as u16 & 0x000f) << 12),
            )?;
        }
        Ok(())
    }

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

    #[test]
    fn packed_mode_setter_preserves_every_adjacent_low_twelve_bit_field() {
        let source = std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap());
        let mut record = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        let before = record.clone();
        let flags = Layer3ExpandedModeFlags::from_packed(0x89ab_cdef);
        record.set_layer3_expanded_mode_flags(flags).unwrap();
        assert_eq!(record.layer3_expanded_mode_flags(), flags);
        for word in 0..ExpandedLevelSettingsRecord::WORD_COUNT {
            if (8..16).contains(&word) {
                assert_eq!(
                    record.word(word).unwrap() & 0x0fff,
                    before.word(word).unwrap() & 0x0fff
                );
            } else {
                assert_eq!(record.word(word).unwrap(), before.word(word).unwrap());
            }
        }
    }

    fn record_with_mode_flags(packed: u32) -> ExpandedLevelSettingsRecord {
        let mut record = ExpandedLevelSettingsRecord::decode(&[0; 32]).unwrap();
        for (nibble, word) in MODE_LOW_NIBBLE_WORDS.into_iter().enumerate() {
            record
                .set_word(word, (((packed >> (nibble * 4)) & 0x0f) as u16) << 12)
                .unwrap();
        }
        for (nibble, word) in MODE_HIGH_NIBBLE_WORDS.into_iter().enumerate() {
            record
                .set_word(word, (((packed >> ((nibble + 4) * 4)) & 0x0f) as u16) << 12)
                .unwrap();
        }
        record
    }

    fn packed_row(encoded_row: u16, mode_type: u8, enabled: bool) -> u32 {
        let mut packed = (u32::from(encoded_row) & 0x1f) << 3;
        packed |= ((u32::from(encoded_row) >> 5) & 0x3f) << 20;
        packed |= (u32::from(mode_type) & 0x0f) << 12;
        packed |= (u32::from(mode_type) & 0x10) << 22;
        if enabled {
            packed |= MODE_ENABLED_MASK;
        }
        packed
    }

    #[test]
    fn expanded_mode_pack_uses_exact_word_nibbles_and_preserves_low_bits() {
        let expected = 0x89ab_cdef;
        let mut record = record_with_mode_flags(expected);
        for word in MODE_LOW_NIBBLE_WORDS
            .into_iter()
            .chain(MODE_HIGH_NIBBLE_WORDS)
        {
            record
                .set_word(word, record.word(word).unwrap() | 0x0a5b)
                .unwrap();
        }
        assert_eq!(record.layer3_expanded_mode_flags().packed(), expected);
    }

    #[test]
    fn expanded_editor_row_obeys_enable_and_level_configuration_gates() {
        let flags = Layer3ExpandedModeFlags::from_packed(packed_row(20, 1, true));
        assert_eq!(
            flags.editor_row(1, 1),
            Some(Layer3ExpandedEditorRow {
                offset: 20,
                clamp_at_30: true,
            })
        );
        assert_eq!(flags.editor_row(2, 0), flags.editor_row(1, 1));
        assert_eq!(flags.editor_row(2, 1), None);
        assert_eq!(flags.editor_row(0, 0), None);
        assert_eq!(flags.editor_row(3, 0), None);
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(packed_row(20, 1, false)).editor_row(1, 0),
            None
        );
    }

    #[test]
    fn expanded_editor_row_sign_extends_and_applies_type_specific_bias() {
        let negative_five = 0x07fb;
        let clamped = Layer3ExpandedModeFlags::from_packed(packed_row(negative_five, 0x11, true));
        assert_eq!(
            clamped.editor_row(1, 0),
            Some(Layer3ExpandedEditorRow {
                offset: -5,
                clamp_at_30: true,
            })
        );
        let ordinary = Layer3ExpandedModeFlags::from_packed(packed_row(negative_five, 2, true));
        assert_eq!(
            ordinary.editor_row(1, 0),
            Some(Layer3ExpandedEditorRow {
                offset: -17,
                clamp_at_30: false,
            })
        );
        for mode_type in [0, 2, 3, 4, 5, 0x12, 0x1f] {
            assert!(
                !Layer3ExpandedModeFlags::from_packed(packed_row(0, mode_type, true))
                    .editor_row(1, 0)
                    .unwrap()
                    .clamp_at_30
            );
        }
        for mode_type in 6..=0x11 {
            assert!(
                Layer3ExpandedModeFlags::from_packed(packed_row(0, mode_type, true))
                    .editor_row(1, 0)
                    .unwrap()
                    .clamp_at_30
            );
        }
    }

    #[test]
    fn expanded_composition_matches_primary_and_alternate_dispatch_routes() {
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0x4000_0001).layer3_composition(0x40),
            Some(Layer3ExpandedComposition {
                alternate_source_route: false,
                additive: true,
                half_color: true,
            })
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0x4000_0001).layer3_composition(0xc0),
            Some(Layer3ExpandedComposition {
                alternate_source_route: false,
                additive: false,
                half_color: false,
            })
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(1).layer3_composition(0x44),
            Some(Layer3ExpandedComposition {
                alternate_source_route: false,
                additive: false,
                half_color: false,
            })
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0xc000_0001).layer3_composition(0x60),
            Some(Layer3ExpandedComposition {
                alternate_source_route: true,
                additive: false,
                half_color: true,
            })
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0).layer3_composition(0xff),
            None
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0x4000_0001).primary_layer3_additive_input(),
            Some(true)
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0x8000_0001).primary_layer3_additive_input(),
            None
        );
        assert_eq!(
            Layer3ExpandedModeFlags::from_packed(0).alternate_layer3_source_route(),
            None
        );
    }
}
