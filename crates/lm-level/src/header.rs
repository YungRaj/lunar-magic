use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyLevelHeader {
    bytes: [u8; Self::ENCODED_LEN],
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

    #[must_use]
    pub const fn sprite_palette(self) -> u8 {
        self.bytes[3] >> 3 & 7
    }

    #[must_use]
    pub const fn foreground_palette(self) -> u8 {
        self.bytes[3] & 7
    }

    #[must_use]
    pub const fn object_tileset(self) -> u8 {
        self.bytes[4] & 0x0f
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

    /// Preserves byte 3 outside the proven sprite-palette field.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderValueError`] for values greater than seven.
    pub fn set_sprite_palette(&mut self, value: u8) -> Result<(), HeaderValueError> {
        set_bits(&mut self.bytes[3], value, 0x38, 3)
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
        header.set_sprite_palette(1).unwrap();
        header.set_foreground_palette(5).unwrap();
        header.set_object_tileset(9).unwrap();
        let encoded = header.encoded();
        assert_eq!(encoded[0] & 0x1f, original[0] & 0x1f);
        assert_eq!(encoded[1] & 0x1f, 3);
        assert_eq!(encoded[1] >> 5, 7);
        assert_eq!(encoded[3] & 0xc0, original[3] & 0xc0);
        assert_eq!(header.sprite_palette(), 1);
        assert_eq!(header.foreground_palette(), 5);
        assert_eq!(encoded[2], original[2]);
        assert_eq!(encoded[4] & 0xf0, original[4] & 0xf0);
        assert_eq!(header.object_tileset(), 9);
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
}
