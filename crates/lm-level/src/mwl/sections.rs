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
    /// Lunar Magic marks the data as compressed tilemap storage, retains the existing layout bit,
    /// records the resulting bank, and clears the legacy/direct-pointer flag. This method is
    /// intentionally distinct from [`Self::with_active_bank`], which is lossless outside the bank
    /// field.
    ///
    /// # Errors
    ///
    /// Rejects banks outside Lunar Magic's accepted `$0..$7` range.
    pub const fn after_native_remap(self, bank: u8) -> Result<Self, MwlLayer2DescriptorError> {
        if bank >= 8 {
            return Err(MwlLayer2DescriptorError::ActiveBank(bank));
        }
        Ok(Self(
            (self.0 & 0x05) | Self::COMPRESSED_TILEMAP | ((bank as u32) << 4),
        ))
    }

    /// Applies Lunar Magic's descriptor-byte transition when a level mode changes from an
    /// object-backed Layer 2 mode to a compressed-tilemap mode.
    ///
    /// `ChangeLevelModeDialogProc` clears bits 0 and 2, then sets bits 1, 3, and 4. Higher bits
    /// and the remaining opaque bytes are retained exactly.
    #[must_use]
    pub const fn after_object_to_tilemap_mode_change(self) -> Self {
        Self((self.0 & !0xff) | (((self.0 as u8) & 0xfa | 0x1a) as u32))
    }

    /// Applies Lunar Magic's descriptor-byte transition when a level mode changes from a
    /// compressed-tilemap mode to an object-backed Layer 2 mode.
    ///
    /// The recovered dialog clears low-byte bits 0–4 while preserving bits 5–7 and every higher
    /// opaque byte.
    #[must_use]
    pub const fn after_tilemap_to_object_mode_change(self) -> Self {
        Self((self.0 & !0xff) | (((self.0 as u8) & 0xe0) as u32))
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

/// Lunar Magic's Layer 2 camera-scroll selection.
///
/// Original settings select one of SMW's sixteen paired rate presets. Separate settings use the
/// installed 5-bit horizontal and vertical selectors independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer2ScrollSettings {
    Original { table_index: u8 },
    Separate { horizontal: u8, vertical: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer2ScrollSettingsError {
    pub field: &'static str,
    pub value: u8,
    pub maximum: u8,
}

impl std::fmt::Display for Layer2ScrollSettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Layer 2 {} scroll selector {} exceeds {}",
            self.field, self.value, self.maximum
        )
    }
}

impl std::error::Error for Layer2ScrollSettingsError {}

const VANILLA_LAYER2_VERTICAL_SCROLL: [u8; 16] = [3, 1, 1, 0, 0, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0];
const VANILLA_LAYER2_HORIZONTAL_SCROLL: [u8; 16] = [2, 2, 1, 0, 1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];

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

    /// Decodes the exact original-preset or separate-rate selection used by Lunar Magic 3.63.
    #[must_use]
    pub const fn layer2_scroll_settings(&self) -> Layer2ScrollSettings {
        let position = self.0[2];
        let extension = self.0[17];
        if extension & 0x80 == 0 {
            Layer2ScrollSettings::Original {
                table_index: position >> 4,
            }
        } else {
            Layer2ScrollSettings::Separate {
                horizontal: extension & 0x1f,
                vertical: position >> 4 | (extension >> 2 & 0x10),
            }
        }
    }

    /// Applies Lunar Magic's canonical Layer 2 scroll encoding while preserving unowned bits.
    ///
    /// # Errors
    ///
    /// Rejects original indices above 15 and separate selectors above 31 without mutation.
    pub fn set_layer2_scroll_settings(
        &mut self,
        settings: Layer2ScrollSettings,
    ) -> Result<(), Layer2ScrollSettingsError> {
        let (position_high, extension) = match settings {
            Layer2ScrollSettings::Original { table_index } => {
                if table_index > 0x0f {
                    return Err(Layer2ScrollSettingsError {
                        field: "original table",
                        value: table_index,
                        maximum: 0x0f,
                    });
                }
                (
                    table_index,
                    self.0[17] & 0x20 | VANILLA_LAYER2_HORIZONTAL_SCROLL[usize::from(table_index)],
                )
            }
            Layer2ScrollSettings::Separate {
                horizontal,
                vertical,
            } => {
                if horizontal > 0x1f {
                    return Err(Layer2ScrollSettingsError {
                        field: "horizontal",
                        value: horizontal,
                        maximum: 0x1f,
                    });
                }
                if vertical > 0x1f {
                    return Err(Layer2ScrollSettingsError {
                        field: "vertical",
                        value: vertical,
                        maximum: 0x1f,
                    });
                }
                (
                    vertical & 0x0f,
                    self.0[17] & 0x20 | 0x80 | (vertical & 0x10) << 2 | horizontal,
                )
            }
        };
        self.0[2] = self.0[2] & 0x0f | position_high << 4;
        self.0[17] = extension;
        Ok(())
    }

    /// Resolves the effective horizontal and vertical selector pair.
    #[must_use]
    pub const fn layer2_scroll_selectors(&self) -> (u8, u8) {
        match self.layer2_scroll_settings() {
            Layer2ScrollSettings::Original { table_index } => (
                VANILLA_LAYER2_HORIZONTAL_SCROLL[table_index as usize],
                VANILLA_LAYER2_VERTICAL_SCROLL[table_index as usize],
            ),
            Layer2ScrollSettings::Separate {
                horizontal,
                vertical,
            } => (horizontal, vertical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_mode_storage_transitions_preserve_only_the_recovered_descriptor_bits() {
        let source = MwlLayer2Descriptor::from_raw(0xa1b2_c3d5);
        assert_eq!(
            source.after_object_to_tilemap_mode_change().raw(),
            0xa1b2_c3da
        );
        assert_eq!(
            source.after_tilemap_to_object_mode_change().raw(),
            0xa1b2_c3c0
        );
    }

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

    #[test]
    fn layer2_original_and_separate_scroll_encodings_match_lunar_magic() {
        let source = std::array::from_fn(|index| u8::try_from(index).unwrap());
        let mut header = MwlLevelHeaderSection(source);
        header
            .set_layer2_scroll_settings(Layer2ScrollSettings::Separate {
                horizontal: 0x1b,
                vertical: 0x12,
            })
            .unwrap();
        assert_eq!(header.0[2], 0x22);
        assert_eq!(header.0[17], 0xdb);
        assert_eq!(
            header.layer2_scroll_settings(),
            Layer2ScrollSettings::Separate {
                horizontal: 0x1b,
                vertical: 0x12,
            }
        );
        assert_eq!(header.layer2_scroll_selectors(), (0x1b, 0x12));
        for (index, original) in source.into_iter().enumerate() {
            if ![2, 17].contains(&index) {
                assert_eq!(header.0[index], original);
            }
        }

        header
            .set_layer2_scroll_settings(Layer2ScrollSettings::Original { table_index: 5 })
            .unwrap();
        assert_eq!(header.0[2], 0x52);
        assert_eq!(header.0[17], 0x02);
        assert_eq!(
            header.layer2_scroll_settings(),
            Layer2ScrollSettings::Original { table_index: 5 }
        );
        assert_eq!(header.layer2_scroll_selectors(), (2, 2));
    }

    #[test]
    fn layer2_scroll_rejects_out_of_range_selectors_atomically() {
        for settings in [
            Layer2ScrollSettings::Original { table_index: 16 },
            Layer2ScrollSettings::Separate {
                horizontal: 32,
                vertical: 0,
            },
            Layer2ScrollSettings::Separate {
                horizontal: 0,
                vertical: 32,
            },
        ] {
            let mut header = MwlLevelHeaderSection([0x5a; 0x40]);
            let before = header.clone();
            assert!(header.set_layer2_scroll_settings(settings).is_err());
            assert_eq!(header, before);
        }
    }
}
