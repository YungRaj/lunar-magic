use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyLevelHeader {
    bytes: [u8; Self::ENCODED_LEN],
}

/// The original two-bit Layer 1 camera-scroll mode stored in level-header byte 4.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Layer1VerticalScrollMode {
    #[default]
    None = 0,
    AtWill = 1,
    NoScrollAtBottomUnlessFlying = 2,
    NoneVerticalOrHorizontal = 3,
}

impl Layer1VerticalScrollMode {
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 3 {
            0 => Self::None,
            1 => Self::AtWill,
            2 => Self::NoScrollAtBottomUnlessFlying,
            _ => Self::NoneVerticalOrHorizontal,
        }
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        self as u8
    }
}

impl LegacyLevelHeader {
    pub const ENCODED_LEN: usize = 5;

    /// Decodes the five bytes preceding the Layer 1 object stream.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless it is exactly five bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        Ok(Self {
            bytes: bytes.try_into().map_err(|_| bytes.len())?,
        })
    }

    #[must_use]
    pub const fn encoded(self) -> [u8; Self::ENCODED_LEN] {
        self.bytes
    }

    #[must_use]
    pub const fn background_palette(self) -> u8 {
        self.bytes[0] >> 5
    }

    #[must_use]
    pub const fn level_mode(self) -> u8 {
        self.bytes[1] & 0x1f
    }

    #[must_use]
    pub const fn background_color(self) -> u8 {
        self.bytes[1] >> 5
    }

    /// Returns the native four-bit sprite GFX set selected by header byte 2.
    #[must_use]
    pub const fn sprite_tileset(self) -> u8 {
        self.bytes[2] & 0x0f
    }

    /// Returns the three-bit selector used by SMW's default level-music table.
    ///
    /// Lunar Magic uses the high nibble of header byte 2 and masks it to three bits whenever no
    /// explicit music control command is present.
    #[must_use]
    pub const fn default_music_selector(self) -> u8 {
        self.bytes[2] >> 4 & 7
    }

    /// Whether Lunar Magic splits low- and high-priority Layer 3 tiles across painter slots.
    ///
    /// This is the high bit of legacy-header byte 2, adjacent to the default-music selector.
    #[must_use]
    pub const fn split_layer3_priority(self) -> bool {
        self.bytes[2] & 0x80 != 0
    }

    /// Replaces the three-bit default-music selector while preserving the sprite tileset and
    /// unrelated high bit.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_default_music_selector(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[2], value, 0x70, 4)
    }

    #[must_use]
    pub const fn sprite_palette(self) -> u8 {
        self.bytes[3] >> 3 & 7
    }

    /// Returns the two-bit selector used by SMW's original level time-limit table.
    #[must_use]
    pub const fn time_limit_selector(self) -> u8 {
        self.bytes[3] >> 6
    }

    #[must_use]
    pub const fn foreground_palette(self) -> u8 {
        self.bytes[3] & 7
    }

    #[must_use]
    pub const fn object_tileset(self) -> u8 {
        self.bytes[4] & 0x0f
    }

    /// Returns Lunar Magic's four-way Layer 1 vertical-scroll selection.
    #[must_use]
    pub const fn layer1_vertical_scroll(self) -> Layer1VerticalScrollMode {
        Layer1VerticalScrollMode::from_raw(self.bytes[4] >> 4)
    }

    /// Preserves every bit except the proven three-bit background-palette field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_background_palette(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[0], value, 0xe0, 5)
    }

    /// Preserves the background-color field while replacing the five-bit level mode.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than 31.
    pub fn set_level_mode(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[1], value, 0x1f, 0)
    }

    /// Preserves the low five bits while replacing the proven background-color field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_background_color(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[1], value, 0xe0, 5)
    }

    /// Preserves byte 2 outside the recovered four-bit sprite GFX set.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than 15.
    pub fn set_sprite_tileset(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[2], value, 0x0f, 0)
    }

    /// Preserves byte 3 outside the proven sprite-palette field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_sprite_palette(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[3], value, 0x38, 3)
    }

    /// Replaces the two-bit time-limit selector while preserving both palette fields.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than three.
    pub fn set_time_limit_selector(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[3], value, 0xc0, 6)
    }

    /// Preserves byte 3 outside the proven foreground-palette field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_foreground_palette(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[3], value, 7, 0)
    }

    /// Preserves byte 4's upper nibble while replacing the proven object-tileset field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than 15.
    pub fn set_object_tileset(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[4], value, 0x0f, 0)
    }

    /// Changes only bits 4-5, preserving the object tileset and both unrelated high bits.
    pub fn set_layer1_vertical_scroll(&mut self, mode: Layer1VerticalScrollMode) {
        self.bytes[4] = self.bytes[4] & !0x30 | mode.raw() << 4;
    }
}

fn set_bits(byte: &mut u8, value: u8, mask: u8, shift: u8) -> Result<(), HeaderValueError> {
    if value > mask >> shift {
        return Err(HeaderValueError {
            value,
            maximum: mask >> shift,
        });
    }
    *byte = *byte & !mask | value << shift;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeaderValueError {
    pub value: u8,
    pub maximum: u8,
}

impl fmt::Display for HeaderValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "level-header value {} exceeds {}",
            self.value, self.maximum
        )
    }
}

impl std::error::Error for HeaderValueError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedLevelHeader {
    pub fields: [u16; Self::FIELD_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuperGraphicsBypass {
    pub enabled: bool,
    pub foreground_background: [u16; 6],
    pub sprites: [u16; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphicsFileValueError {
    pub slot: usize,
    pub value: u16,
}

impl fmt::Display for GraphicsFileValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "graphics file value {:X} in slot {} exceeds FFF",
            self.value, self.slot
        )
    }
}

impl std::error::Error for GraphicsFileValueError {}

impl ExpandedLevelHeader {
    pub const FIELD_COUNT: usize = 16;
    pub const ENCODED_LEN: usize = Self::FIELD_COUNT * 2;

    /// Decodes the exact 0x20-byte expanded per-level record.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless it is exactly 32 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(bytes.len());
        }
        let mut fields = [0; Self::FIELD_COUNT];
        for (field, pair) in fields.iter_mut().zip(bytes.chunks_exact(2)) {
            *field = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Ok(Self { fields })
    }

    #[must_use]
    pub fn encode(self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        for (pair, field) in bytes.chunks_exact_mut(2).zip(self.fields) {
            pair.copy_from_slice(&field.to_le_bytes());
        }
        bytes
    }

    /// Decodes Lunar Magic's expanded Super GFX bypass fields.
    ///
    /// The two recovered expanded-header loaders prove the reversed word order. Only the low
    /// twelve bits name a GFX/ExGFX file; unknown high bits remain owned by the raw record.
    #[must_use]
    pub fn super_graphics_bypass(self) -> SuperGraphicsBypass {
        SuperGraphicsBypass {
            enabled: self.fields[0] & 0x8000 != 0,
            foreground_background: std::array::from_fn(|slot| self.fields[7 - slot] & 0x0fff),
            sprites: std::array::from_fn(|slot| self.fields[11 - slot] & 0x0fff),
        }
    }

    /// Replaces the proven Super GFX bypass fields while preserving unrelated bits and words.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsFileValueError`] if any file number exceeds the native 12-bit range.
    pub fn set_super_graphics_bypass(
        &mut self,
        value: SuperGraphicsBypass,
    ) -> Result<(), GraphicsFileValueError> {
        for (slot, file) in value
            .foreground_background
            .iter()
            .chain(&value.sprites)
            .copied()
            .enumerate()
        {
            if file > 0x0fff {
                return Err(GraphicsFileValueError { slot, value: file });
            }
        }
        self.fields[0] = self.fields[0] & !0x8000 | u16::from(value.enabled) << 15;
        for (slot, file) in value.foreground_background.into_iter().enumerate() {
            let field = &mut self.fields[7 - slot];
            *field = *field & 0xf000 | file;
        }
        for (slot, file) in value.sprites.into_iter().enumerate() {
            let field = &mut self.fields[11 - slot];
            *field = *field & 0xf000 | file;
        }
        Ok(())
    }
}

impl Default for ExpandedLevelHeader {
    fn default() -> Self {
        let mut fields = [0x7f7f; Self::FIELD_COUNT];
        fields[8] = 0xffff;
        fields[12..].copy_from_slice(&[0x2b, 0x2a, 0x29, 0x28]);
        Self { fields }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LevelHeader {
    pub legacy: LegacyLevelHeader,
    pub expanded: Option<ExpandedLevelHeader>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proven_legacy_fields_preserve_unowned_bits() {
        let original = [0b1011_0011, 0b0111_1010, 0xaa, 0b1101_0110, 0x55];
        let mut header = LegacyLevelHeader::decode(&original).unwrap();
        header.set_background_palette(2).unwrap();
        header.set_level_mode(3).unwrap();
        header.set_background_color(7).unwrap();
        header.set_sprite_tileset(6).unwrap();
        header.set_default_music_selector(5).unwrap();
        header.set_sprite_palette(1).unwrap();
        header.set_time_limit_selector(2).unwrap();
        header.set_foreground_palette(5).unwrap();
        header.set_object_tileset(9).unwrap();
        let encoded = header.encoded();
        assert_eq!(encoded[0] & 0x1f, original[0] & 0x1f);
        assert_eq!(encoded[1] & 0x1f, 3);
        assert_eq!(encoded[1] >> 5, 7);
        assert_eq!(encoded[2] & 0x80, original[2] & 0x80);
        assert!(header.split_layer3_priority());
        assert_eq!(header.sprite_tileset(), 6);
        assert_eq!(header.default_music_selector(), 5);
        assert_eq!(header.time_limit_selector(), 2);
        assert_eq!(header.sprite_palette(), 1);
        assert_eq!(header.foreground_palette(), 5);
        assert_eq!(encoded[4] & 0xf0, original[4] & 0xf0);
        assert_eq!(header.object_tileset(), 9);
    }

    #[test]
    fn time_limit_selector_rejects_out_of_range_values_atomically() {
        let mut header = LegacyLevelHeader::decode(&[0x12, 0x34, 0x56, 0x9a, 0xbc]).unwrap();
        let before = header;
        assert_eq!(
            header.set_time_limit_selector(4),
            Err(HeaderValueError {
                value: 4,
                maximum: 3,
            })
        );
        assert_eq!(header, before);
    }

    #[test]
    fn layer1_vertical_scroll_uses_only_header_byte_four_bits_four_and_five() {
        for raw in 0..4 {
            let original = [0x12, 0x34, 0x56, 0x78, 0xca];
            let mut header = LegacyLevelHeader::decode(&original).unwrap();
            let mode = Layer1VerticalScrollMode::from_raw(raw);
            header.set_layer1_vertical_scroll(mode);
            assert_eq!(header.layer1_vertical_scroll(), mode);
            assert_eq!(header.encoded()[4] & 0x30, raw << 4);
            assert_eq!(header.encoded()[4] & !0x30, original[4] & !0x30);
            assert_eq!(&header.encoded()[..4], &original[..4]);
        }
    }

    #[test]
    fn expanded_default_matches_recovered_initializer() {
        let header = ExpandedLevelHeader::default();
        assert_eq!(header.fields[..8], [0x7f7f; 8]);
        assert_eq!(header.fields[8], 0xffff);
        assert_eq!(header.fields[12..], [0x2b, 0x2a, 0x29, 0x28]);
        assert_eq!(
            ExpandedLevelHeader::decode(&header.encode()).unwrap(),
            header
        );
    }

    #[test]
    fn super_graphics_bypass_uses_reversed_words_and_preserves_unknown_bits() {
        let mut header = ExpandedLevelHeader {
            fields: std::array::from_fn(|index| 0xa000 | u16::try_from(index).unwrap()),
        };
        let before = header.fields;
        let bypass = SuperGraphicsBypass {
            enabled: true,
            foreground_background: [0x101, 0x202, 0x303, 0x404, 0x505, 0x606],
            sprites: [0x707, 0x808, 0x909, 0xa0a],
        };
        header.set_super_graphics_bypass(bypass).unwrap();
        assert_eq!(header.super_graphics_bypass(), bypass);
        assert_eq!(header.fields[7] & 0x0fff, 0x101);
        assert_eq!(header.fields[2] & 0x0fff, 0x606);
        assert_eq!(header.fields[11] & 0x0fff, 0x707);
        assert_eq!(header.fields[8] & 0x0fff, 0xa0a);
        for (after, before) in header.fields.iter().zip(before) {
            assert_eq!(*after & 0x7000, before & 0x7000);
        }
        assert_eq!(header.fields[1], before[1]);
        assert_eq!(header.fields[12..], before[12..]);
    }

    #[test]
    fn super_graphics_bypass_rejects_out_of_range_files_atomically() {
        let mut header = ExpandedLevelHeader::default();
        let before = header;
        let mut bypass = header.super_graphics_bypass();
        bypass.sprites[2] = 0x1000;
        assert!(header.set_super_graphics_bypass(bypass).is_err());
        assert_eq!(header, before);
    }
}
