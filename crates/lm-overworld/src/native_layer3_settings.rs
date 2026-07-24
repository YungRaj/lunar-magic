//! Exact 32-byte editor records used by Lunar Magic's seven overworld maps.

pub const OVERWORLD_LAYER3_MAP_COUNT: usize = 7;
pub const OVERWORLD_LAYER3_LAYOUT_WORDS: usize = 8;
pub const OVERWORLD_LAYER3_GFX_SLOTS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldLayer3SettingsRecord {
    bytes: [u8; Self::ENCODED_LEN],
}

impl Default for OverworldLayer3SettingsRecord {
    fn default() -> Self {
        Self {
            bytes: [0; Self::ENCODED_LEN],
        }
    }
}

impl OverworldLayer3SettingsRecord {
    pub const ENCODED_LEN: usize = 0x20;
    pub const CUSTOM_TILEMAP_FLAG: u16 = 0x2000;
    pub const CUSTOM_GRAPHICS_FLAG: u16 = 0x4000;

    const FLAGS: usize = 0;
    const TILEMAP: usize = 2;
    const LAYOUT: usize = 4;
    const PRESERVED: usize = 0x14;
    const GRAPHICS: usize = 0x18;

    #[must_use]
    pub const fn from_bytes(bytes: [u8; Self::ENCODED_LEN]) -> Self {
        Self { bytes }
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.bytes
    }

    #[must_use]
    pub fn feature_flags(&self) -> u16 {
        self.word(Self::FLAGS)
    }

    pub fn set_feature_flags(&mut self, value: u16) {
        self.set_word(Self::FLAGS, value);
    }

    #[must_use]
    pub fn uses_custom_tilemap(&self) -> bool {
        self.feature_flags() & Self::CUSTOM_TILEMAP_FLAG != 0
    }

    pub fn set_uses_custom_tilemap(&mut self, enabled: bool) {
        self.update_flag(Self::CUSTOM_TILEMAP_FLAG, enabled);
    }

    #[must_use]
    pub fn uses_custom_graphics(&self) -> bool {
        self.feature_flags() & Self::CUSTOM_GRAPHICS_FLAG != 0
    }

    pub fn set_uses_custom_graphics(&mut self, enabled: bool) {
        self.update_flag(Self::CUSTOM_GRAPHICS_FLAG, enabled);
    }

    #[must_use]
    pub fn tilemap_file(&self) -> u16 {
        self.word(Self::TILEMAP) & 0x0fff
    }

    /// Replaces the low 12-bit external tilemap file index.
    ///
    /// # Errors
    ///
    /// Rejects values larger than `$FFF`.
    pub fn set_tilemap_file(&mut self, value: u16) -> Result<(), OverworldLayer3SettingsError> {
        check_12_bit(value)?;
        self.set_masked_word(Self::TILEMAP, 0x0fff, value);
        Ok(())
    }

    #[must_use]
    pub fn tilemap_size(&self) -> u8 {
        ((self.word(Self::TILEMAP) >> 12) & 3) as u8
    }

    /// Replaces the packed two-bit tilemap size selector.
    ///
    /// # Errors
    ///
    /// Rejects values larger than three.
    pub fn set_tilemap_size(&mut self, value: u8) -> Result<(), OverworldLayer3SettingsError> {
        check_2_bit(value)?;
        self.set_masked_word(Self::TILEMAP, 0x3000, u16::from(value) << 12);
        Ok(())
    }

    #[must_use]
    pub fn tilemap_position(&self) -> u8 {
        (self.word(Self::TILEMAP) >> 14) as u8
    }

    /// Replaces the packed two-bit Layer 3 placement selector.
    ///
    /// # Errors
    ///
    /// Rejects values larger than three.
    pub fn set_tilemap_position(&mut self, value: u8) -> Result<(), OverworldLayer3SettingsError> {
        check_2_bit(value)?;
        self.set_masked_word(Self::TILEMAP, 0xc000, u16::from(value) << 14);
        Ok(())
    }

    #[must_use]
    pub fn address_layout_word(&self, index: usize) -> Option<u16> {
        (index < OVERWORLD_LAYER3_LAYOUT_WORDS).then(|| self.word(Self::LAYOUT + index * 2))
    }

    /// Replaces one of the eight words shifted by Lunar Magic's address-layout converter.
    ///
    /// # Errors
    ///
    /// Rejects indexes outside the eight-word region.
    pub fn set_address_layout_word(
        &mut self,
        index: usize,
        value: u16,
    ) -> Result<(), OverworldLayer3SettingsError> {
        if index >= OVERWORLD_LAYER3_LAYOUT_WORDS {
            return Err(OverworldLayer3SettingsError::LayoutIndex(index));
        }
        self.set_word(Self::LAYOUT + index * 2, value);
        Ok(())
    }

    #[must_use]
    pub fn preserved_bytes(&self) -> &[u8] {
        &self.bytes[Self::PRESERVED..Self::GRAPHICS]
    }

    #[must_use]
    pub fn graphics_file(&self, slot: usize) -> Option<u16> {
        (slot < OVERWORLD_LAYER3_GFX_SLOTS).then(|| self.word(Self::GRAPHICS + slot * 2) & 0x0fff)
    }

    /// Replaces the low 12 bits of one custom Layer 3 graphics file word.
    ///
    /// # Errors
    ///
    /// Rejects slots outside zero through three or values larger than `$FFF`.
    pub fn set_graphics_file(
        &mut self,
        slot: usize,
        value: u16,
    ) -> Result<(), OverworldLayer3SettingsError> {
        if slot >= OVERWORLD_LAYER3_GFX_SLOTS {
            return Err(OverworldLayer3SettingsError::GraphicsSlot(slot));
        }
        check_12_bit(value)?;
        self.set_masked_word(Self::GRAPHICS + slot * 2, 0x0fff, value);
        Ok(())
    }

    fn word(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]])
    }

    fn set_word(&mut self, offset: usize, value: u16) {
        self.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_masked_word(&mut self, offset: usize, mask: u16, value: u16) {
        self.set_word(offset, self.word(offset) & !mask | value & mask);
    }

    fn update_flag(&mut self, flag: u16, enabled: bool) {
        let value = if enabled {
            self.feature_flags() | flag
        } else {
            self.feature_flags() & !flag
        };
        self.set_feature_flags(value);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldLayer3SettingsTable {
    pub maps: [OverworldLayer3SettingsRecord; OVERWORLD_LAYER3_MAP_COUNT],
}

impl OverworldLayer3SettingsTable {
    pub const ENCODED_LEN: usize =
        OVERWORLD_LAYER3_MAP_COUNT * OverworldLayer3SettingsRecord::ENCODED_LEN;

    /// Decodes exactly seven contiguous 32-byte records.
    ///
    /// # Errors
    ///
    /// Rejects any input whose length is not exactly 224 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, OverworldLayer3SettingsError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(OverworldLayer3SettingsError::Length(bytes.len()));
        }
        let mut maps = std::array::from_fn(|_| OverworldLayer3SettingsRecord::default());
        for (record, chunk) in maps
            .iter_mut()
            .zip(bytes.chunks_exact(OverworldLayer3SettingsRecord::ENCODED_LEN))
        {
            record.bytes.copy_from_slice(chunk);
        }
        Ok(Self { maps })
    }

    #[must_use]
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        for (map, record) in self.maps.iter().enumerate() {
            let start = map * OverworldLayer3SettingsRecord::ENCODED_LEN;
            bytes[start..start + OverworldLayer3SettingsRecord::ENCODED_LEN]
                .copy_from_slice(record.encoded());
        }
        bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldLayer3SettingsError {
    Length(usize),
    Value12Bit(u16),
    Value2Bit(u8),
    LayoutIndex(usize),
    GraphicsSlot(usize),
}

impl std::fmt::Display for OverworldLayer3SettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld Layer 3 settings: {self:?}")
    }
}

impl std::error::Error for OverworldLayer3SettingsError {}

fn check_12_bit(value: u16) -> Result<(), OverworldLayer3SettingsError> {
    if value <= 0x0fff {
        Ok(())
    } else {
        Err(OverworldLayer3SettingsError::Value12Bit(value))
    }
}

fn check_2_bit(value: u8) -> Result<(), OverworldLayer3SettingsError> {
    if value <= 3 {
        Ok(())
    } else {
        Err(OverworldLayer3SettingsError::Value2Bit(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_edits_preserve_every_unowned_bit() {
        let original = std::array::from_fn(|index| u8::try_from(index).unwrap() ^ 0xa5);
        let mut record = OverworldLayer3SettingsRecord::from_bytes(original);
        record.set_uses_custom_tilemap(true);
        record.set_uses_custom_graphics(false);
        record.set_tilemap_file(0xabc).unwrap();
        record.set_tilemap_size(3).unwrap();
        record.set_tilemap_position(2).unwrap();
        record.set_graphics_file(2, 0x456).unwrap();
        assert_eq!(record.preserved_bytes(), &original[0x14..0x18]);
        assert_eq!(record.encoded()[0x1d] & 0xf0, original[0x1d] & 0xf0);
        assert_eq!(record.tilemap_file(), 0xabc);
        assert_eq!(record.tilemap_size(), 3);
        assert_eq!(record.tilemap_position(), 2);
        assert_eq!(record.graphics_file(2), Some(0x456));
    }

    #[test]
    fn exact_table_roundtrip_is_lossless() {
        let bytes = std::array::from_fn(|index| u8::try_from(index).unwrap());
        let table = OverworldLayer3SettingsTable::decode(&bytes).unwrap();
        assert_eq!(table.encode(), bytes);
    }
}
