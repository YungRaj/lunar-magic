use super::{MwlError, payload_section_len, read_u32};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MwlPayloadSection {
    pub metadata: [u32; 2],
    pub payload: Vec<u8>,
}

/// Lossless interpretation of Lunar Magic's first Layer 2 MWL metadata word.
///
/// The complete word is retained because several low-bit combinations describe historical
/// storage forms. Bits 4–6 are the active 4-KiB Map16 bank used by Lunar Magic's background
/// remapper. Accessors deliberately leave every unrelated bit unchanged.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MwlLayer2Descriptor(u32);

impl MwlLayer2Descriptor {
    pub const ACTIVE_BANK_MASK: u32 = 0x70;
    pub const COMPRESSED_TILEMAP: u32 = 0x02;
    pub const SPLIT_PLANES: u32 = 0x04;

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn active_bank(self) -> u8 {
        ((self.0 & Self::ACTIVE_BANK_MASK) >> 4) as u8
    }

    #[must_use]
    pub const fn uses_compressed_tilemap(self) -> bool {
        self.0 & Self::COMPRESSED_TILEMAP != 0
    }

    #[must_use]
    pub const fn uses_split_planes(self) -> bool {
        self.0 & Self::SPLIT_PLANES != 0
    }

    /// Replaces only the recovered three-bit active-bank field.
    ///
    /// # Errors
    ///
    /// Rejects banks outside Lunar Magic's accepted `$0..$7` range.
    pub const fn with_active_bank(self, bank: u8) -> Result<Self, MwlLayer2DescriptorError> {
        if bank >= 8 {
            return Err(MwlLayer2DescriptorError::ActiveBank(bank));
        }
        Ok(Self(
            (self.0 & !Self::ACTIVE_BANK_MASK) | ((bank as u32) << 4),
        ))
    }

    /// Applies the exact descriptor normalization performed after a native remap.
    ///
    /// Lunar Magic promotes the data to compressed split-plane tilemap storage, records the
    /// resulting bank, and clears the legacy/direct-pointer flag. This method is intentionally
    /// distinct from [`Self::with_active_bank`], which is lossless outside the bank field.
    ///
    /// # Errors
    ///
    /// Rejects banks outside Lunar Magic's accepted `$0..$7` range.
    pub const fn after_native_remap(self, bank: u8) -> Result<Self, MwlLayer2DescriptorError> {
        if bank >= 8 {
            return Err(MwlLayer2DescriptorError::ActiveBank(bank));
        }
        Ok(Self(
            (self.0 & 0x05) | Self::COMPRESSED_TILEMAP | Self::SPLIT_PLANES | ((bank as u32) << 4),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MwlLayer2DescriptorError {
    ActiveBank(u8),
}

impl std::fmt::Display for MwlLayer2DescriptorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid MWL Layer 2 descriptor: {self:?}")
    }
}

impl std::error::Error for MwlLayer2DescriptorError {}

/// Typed form of the common-prefix MWL Layer 2 section.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MwlLayer2Section {
    pub descriptor: MwlLayer2Descriptor,
    /// Lunar Magic's second metadata word, retained as an opaque source/storage address.
    pub source_address: u32,
    pub payload: Vec<u8>,
}

impl MwlLayer2Section {
    /// Decodes the exact two-word Layer 2 prefix without normalizing either word.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Truncated`] when fewer than eight bytes are available.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        let section = MwlPayloadSection::decode(bytes)?;
        Ok(Self {
            descriptor: MwlLayer2Descriptor::from_raw(section.metadata[0]),
            source_address: section.metadata[1],
            payload: section.payload,
        })
    }

    /// Encodes the descriptor, opaque source address, and payload exactly.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if the aggregate length cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, MwlError> {
        MwlPayloadSection {
            metadata: [self.descriptor.raw(), self.source_address],
            payload: self.payload.clone(),
        }
        .encode()
    }
}

impl MwlPayloadSection {
    pub const METADATA_LEN: usize = 8;

    /// Decodes the common two-word prefix used by Layer 1, Layer 2, sprites, palettes, secondary
    /// exits, and `ExAnimation` sections.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Truncated`] if the section is shorter than eight bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        if bytes.len() < Self::METADATA_LEN {
            return Err(MwlError::Truncated {
                offset: 0,
                needed: Self::METADATA_LEN,
            });
        }
        Ok(Self {
            metadata: [read_u32(bytes, 0)?, read_u32(bytes, 4)?],
            payload: bytes[Self::METADATA_LEN..].to_vec(),
        })
    }

    /// Encodes the common two-word prefix and payload after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if metadata and payload length cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, MwlError> {
        let mut bytes = Vec::with_capacity(payload_section_len(self.payload.len())?);
        bytes.extend_from_slice(&self.metadata[0].to_le_bytes());
        bytes.extend_from_slice(&self.metadata[1].to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlLevelHeaderSection(pub [u8; Self::ENCODED_LEN]);

/// The packed main or midway entrance fields stored in an MWL level-header section.
///
/// These bytes are deliberately exposed losslessly. Their individual bit meanings depend on the
/// level mode and on which Lunar Magic runtime patches are installed in the destination ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MwlMainEntranceSettings {
    pub position: u8,
    pub vertical_settings: u8,
    pub screen_and_method: u8,
    pub level_mode_and_screen: u8,
    pub flags: u8,
    pub high_position: u8,
    pub additional_flags: u8,
}

/// The four midway-specific packed fields stored in an MWL level-header section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MwlMidwayEntranceSettings {
    pub position: u8,
    pub flags: u8,
    pub high_position: u8,
    pub additional_flags: u8,
}

impl MwlLevelHeaderSection {
    pub const ENCODED_LEN: usize = 0x40;

    /// Decodes the fixed-size MWL level-settings section without interpreting unproven fields.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Truncated`] unless exactly 64 bytes are supplied.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        Ok(Self(bytes.try_into().map_err(|_| MwlError::Truncated {
            offset: 0,
            needed: Self::ENCODED_LEN,
        })?))
    }

    #[must_use]
    pub const fn level_number(&self) -> u16 {
        u16::from_le_bytes([self.0[0], self.0[1]])
    }

    pub fn set_level_number(&mut self, level: u16) {
        self.0[..2].copy_from_slice(&level.to_le_bytes());
    }

    /// Returns the exact seven packed main-entrance bytes emitted by Lunar Magic 3.63.
    #[must_use]
    pub const fn main_entrance(&self) -> MwlMainEntranceSettings {
        MwlMainEntranceSettings {
            position: self.0[2],
            vertical_settings: self.0[3],
            screen_and_method: self.0[4],
            level_mode_and_screen: self.0[5],
            flags: self.0[6],
            high_position: self.0[14],
            additional_flags: self.0[15],
        }
    }

    /// Replaces only the seven packed main-entrance bytes.
    pub fn set_main_entrance(&mut self, entrance: MwlMainEntranceSettings) {
        self.0[2] = entrance.position;
        self.0[3] = entrance.vertical_settings;
        self.0[4] = entrance.screen_and_method;
        self.0[5] = entrance.level_mode_and_screen;
        self.0[6] = entrance.flags;
        self.0[14] = entrance.high_position;
        self.0[15] = entrance.additional_flags;
    }

    /// Returns the exact seven packed midway-entrance bytes emitted by Lunar Magic 3.63.
    #[must_use]
    pub const fn midway_entrance(&self) -> MwlMidwayEntranceSettings {
        MwlMidwayEntranceSettings {
            position: self.0[10],
            flags: self.0[9],
            high_position: self.0[12],
            additional_flags: self.0[11],
        }
    }

    /// Replaces the packed midway-specific bytes while preserving the shared level-mode byte.
    ///
    /// Lunar Magic stores midway fields in four bytes. The duplicated fields in
    pub fn set_midway_entrance(&mut self, entrance: MwlMidwayEntranceSettings) {
        self.0[10] = entrance.position;
        self.0[9] = entrance.flags;
        self.0[12] = entrance.high_position;
        self.0[11] = entrance.additional_flags;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrance_accessors_touch_only_recovered_header_offsets() {
        let source = std::array::from_fn(|index| u8::try_from(index).unwrap());
        let mut header = MwlLevelHeaderSection(source);
        let main = MwlMainEntranceSettings {
            position: 0xa0,
            vertical_settings: 0xa1,
            screen_and_method: 0xa2,
            level_mode_and_screen: 0xa3,
            flags: 0xa4,
            high_position: 0xa5,
            additional_flags: 0xa6,
        };
        header.set_main_entrance(main);
        assert_eq!(header.main_entrance(), main);
        for (index, original) in source.into_iter().enumerate() {
            if ![2, 3, 4, 5, 6, 14, 15].contains(&index) {
                assert_eq!(header.0[index], original);
            }
        }

        let before_midway = header.0;
        let midway = MwlMidwayEntranceSettings {
            position: 0xb0,
            flags: 0xb1,
            high_position: 0xb2,
            additional_flags: 0xb3,
        };
        header.set_midway_entrance(midway);
        assert_eq!(header.midway_entrance().position, 0xb0);
        assert_eq!(header.midway_entrance().flags, 0xb1);
        assert_eq!(header.midway_entrance().high_position, 0xb2);
        assert_eq!(header.midway_entrance().additional_flags, 0xb3);
        for (index, original) in before_midway.into_iter().enumerate() {
            if ![9, 10, 11, 12].contains(&index) {
                assert_eq!(header.0[index], original);
            }
        }
    }
}
