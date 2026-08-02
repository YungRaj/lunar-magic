use std::fmt;

mod palette;
mod sections;

pub use palette::{MwlPaletteSection, MwlPaletteSectionError};
pub use sections::{
    Layer2ScrollSettings, Layer2ScrollSettingsError, MwlLayer2Descriptor, MwlLayer2DescriptorError,
    MwlLayer2Section, MwlLevelHeaderSection, MwlMainEntranceSettings, MwlMidwayEntranceSettings,
    MwlPayloadSection,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum MwlSectionKind {
    LevelHeader = 0,
    Layer1 = 1,
    Layer2 = 2,
    Sprites = 3,
    Palette = 4,
    SecondaryExits = 5,
    ExAnimation = 6,
    ExpandedHeader = 7,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MwlSection {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlFile {
    pub version: u16,
    pub flags: u32,
    pub attribution: [u8; Self::ATTRIBUTION_LEN],
    pub sections: [MwlSection; Self::SECTION_COUNT],
}

impl MwlFile {
    pub const SIGNATURE: [u8; 2] = *b"LM";
    pub const CURRENT_VERSION: u16 = 0x0363;
    pub const DIRECTORY_OFFSET: usize = 0x40;
    pub const DIRECTORY_LEN: usize = 0x40;
    pub const DATA_OFFSET: usize = 0x80;
    pub const ATTRIBUTION_LEN: usize = 0x30;
    pub const SECTION_COUNT: usize = 8;
    pub const MAX_SECTION_BYTES: usize = 16 * 1024 * 1024;
    pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

    /// Parses the current eight-section binary MWL container.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError`] for malformed headers, unsupported directory shapes, integer overflow,
    /// overlapping header data, or out-of-bounds sections.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        if bytes.len() > Self::MAX_FILE_BYTES {
            return Err(MwlError::FileTooLarge(bytes.len()));
        }
        let fixed = bytes.get(..Self::DATA_OFFSET).ok_or(MwlError::Truncated {
            offset: 0,
            needed: Self::DATA_OFFSET,
        })?;
        if fixed[..2] != Self::SIGNATURE {
            return Err(MwlError::Signature);
        }
        let version = u16::from_le_bytes([fixed[2], fixed[3]]);
        let directory_offset = read_u32(fixed, 4)?;
        let directory_len = read_u32(fixed, 8)?;
        let directory_offset = usize::try_from(directory_offset).map_err(|_| MwlError::Overflow)?;
        let directory_len = usize::try_from(directory_len).map_err(|_| MwlError::Overflow)?;
        if directory_offset != Self::DIRECTORY_OFFSET || directory_len != Self::DIRECTORY_LEN {
            return Err(MwlError::DirectoryShape {
                offset: directory_offset,
                len: directory_len,
            });
        }
        let flags = read_u32(fixed, 12)?;
        let attribution =
            fixed[16..Self::DIRECTORY_OFFSET]
                .try_into()
                .map_err(|_| MwlError::Truncated {
                    offset: 16,
                    needed: Self::ATTRIBUTION_LEN,
                })?;
        let mut sections: [MwlSection; Self::SECTION_COUNT] =
            std::array::from_fn(|_| MwlSection::default());
        let mut ranges = Vec::new();
        for (index, section) in sections.iter_mut().enumerate() {
            let entry = directory_offset + index * 8;
            let offset =
                usize::try_from(read_u32(fixed, entry)?).map_err(|_| MwlError::Overflow)?;
            let len =
                usize::try_from(read_u32(fixed, entry + 4)?).map_err(|_| MwlError::Overflow)?;
            if len == 0 {
                if offset != 0 {
                    return Err(MwlError::EmptySectionOffset { index, offset });
                }
                continue;
            }
            if len > Self::MAX_SECTION_BYTES {
                return Err(MwlError::SectionTooLarge { index, bytes: len });
            }
            if offset < Self::DATA_OFFSET {
                return Err(MwlError::SectionOverlapsHeader { index, offset });
            }
            let end = offset.checked_add(len).ok_or(MwlError::Overflow)?;
            section.bytes = bytes
                .get(offset..end)
                .ok_or(MwlError::Truncated {
                    offset,
                    needed: len,
                })?
                .to_vec();
            if let Some((first, _, _)) = ranges
                .iter()
                .find(|(_, start, previous_end)| offset < *previous_end && *start < end)
            {
                return Err(MwlError::SectionOverlap {
                    first: *first,
                    second: index,
                });
            }
            ranges.push((index, offset, end));
        }
        Ok(Self {
            version,
            flags,
            attribution,
            sections,
        })
    }

    /// Encodes a canonical contiguous MWL container and backfills all section entries.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if the file cannot be represented by 32-bit directory fields.
    pub fn encode(&self) -> Result<Vec<u8>, MwlError> {
        for (index, section) in self.sections.iter().enumerate() {
            if section.bytes.len() > Self::MAX_SECTION_BYTES {
                return Err(MwlError::SectionTooLarge {
                    index,
                    bytes: section.bytes.len(),
                });
            }
        }
        let encoded_len = self
            .sections
            .iter()
            .try_fold(Self::DATA_OFFSET, |total, section| {
                total
                    .checked_add(section.bytes.len())
                    .ok_or(MwlError::Overflow)
            })?;
        if encoded_len > Self::MAX_FILE_BYTES {
            return Err(MwlError::FileTooLarge(encoded_len));
        }
        let mut output = Vec::with_capacity(encoded_len);
        output.extend_from_slice(&Self::SIGNATURE);
        output.extend_from_slice(&self.version.to_le_bytes());
        output.extend_from_slice(
            &u32::try_from(Self::DIRECTORY_OFFSET)
                .map_err(|_| MwlError::Overflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(
            &u32::try_from(Self::DIRECTORY_LEN)
                .map_err(|_| MwlError::Overflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(&self.flags.to_le_bytes());
        output.extend_from_slice(&self.attribution);
        output.resize(Self::DATA_OFFSET, 0);
        for (index, section) in self.sections.iter().enumerate() {
            if section.bytes.is_empty() {
                continue;
            }
            let offset = u32::try_from(output.len()).map_err(|_| MwlError::Overflow)?;
            let len = u32::try_from(section.bytes.len()).map_err(|_| MwlError::Overflow)?;
            let entry = Self::DIRECTORY_OFFSET + index * 8;
            output[entry..entry + 4].copy_from_slice(&offset.to_le_bytes());
            output[entry + 4..entry + 8].copy_from_slice(&len.to_le_bytes());
            output.extend_from_slice(&section.bytes);
        }
        Ok(output)
    }

    #[must_use]
    pub fn section(&self, kind: MwlSectionKind) -> &[u8] {
        &self.sections[kind as usize].bytes
    }

    pub fn set_section(&mut self, kind: MwlSectionKind, bytes: Vec<u8>) {
        self.sections[kind as usize].bytes = bytes;
    }

    /// Decodes a section using the common two-word payload prefix.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError`] when the selected section is too short.
    pub fn payload_section(&self, kind: MwlSectionKind) -> Result<MwlPayloadSection, MwlError> {
        MwlPayloadSection::decode(self.section(kind))
    }

    /// Encodes and installs a common-prefix payload section.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if the encoded section length cannot be represented.
    pub fn set_payload_section(
        &mut self,
        kind: MwlSectionKind,
        section: &MwlPayloadSection,
    ) -> Result<(), MwlError> {
        self.set_section(kind, section.encode()?);
        Ok(())
    }

    /// Decodes the Layer 2 section with its lossless descriptor and source-address metadata.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError`] when the section is shorter than its two-word metadata prefix.
    pub fn layer2_section(&self) -> Result<MwlLayer2Section, MwlError> {
        MwlLayer2Section::decode(self.section(MwlSectionKind::Layer2))
    }

    /// Encodes and installs a typed Layer 2 section without changing unknown descriptor bits.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if the encoded section length cannot be represented.
    pub fn set_layer2_section(&mut self, section: &MwlLayer2Section) -> Result<(), MwlError> {
        self.set_section(MwlSectionKind::Layer2, section.encode()?);
        Ok(())
    }

    /// Decodes the exact 257-color palette section exported by Lunar Magic.
    ///
    /// # Errors
    ///
    /// Returns [`MwlPaletteSectionError`] unless the section has the recovered `0x20a`-byte
    /// shape.
    pub fn palette_section(&self) -> Result<MwlPaletteSection, MwlPaletteSectionError> {
        MwlPaletteSection::decode(self.section(MwlSectionKind::Palette))
    }

    /// Encodes and installs an exact typed palette section.
    pub fn set_palette_section(&mut self, section: &MwlPaletteSection) {
        self.set_section(MwlSectionKind::Palette, section.encode());
    }

    /// Decodes the exact installed expanded-settings record carried by the MWL expanded-header
    /// section.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ExpandedLevelSettingsError`] unless the section is exactly 32 bytes.
    pub fn expanded_settings_section(
        &self,
    ) -> Result<crate::ExpandedLevelSettingsRecord, crate::ExpandedLevelSettingsError> {
        crate::ExpandedLevelSettingsRecord::decode(self.section(MwlSectionKind::ExpandedHeader))
    }

    /// Installs one exact expanded-settings record without interpreting or normalizing unknown
    /// words.
    pub fn set_expanded_settings_section(&mut self, record: &crate::ExpandedLevelSettingsRecord) {
        self.set_section(MwlSectionKind::ExpandedHeader, record.encoded().to_vec());
    }
}

fn payload_section_len(payload_len: usize) -> Result<usize, MwlError> {
    MwlPayloadSection::METADATA_LEN
        .checked_add(payload_len)
        .ok_or(MwlError::Overflow)
}

pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MwlError> {
    let end = offset.checked_add(4).ok_or(MwlError::Overflow)?;
    let value = bytes
        .get(offset..end)
        .ok_or(MwlError::Truncated { offset, needed: 4 })?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlError {
    Signature,
    DirectoryShape { offset: usize, len: usize },
    SectionOverlapsHeader { index: usize, offset: usize },
    EmptySectionOffset { index: usize, offset: usize },
    SectionOverlap { first: usize, second: usize },
    SectionTooLarge { index: usize, bytes: usize },
    FileTooLarge(usize),
    Truncated { offset: usize, needed: usize },
    Overflow,
    Layer2Scroll(Layer2ScrollSettingsError),
}

impl From<Layer2ScrollSettingsError> for MwlError {
    fn from(value: Layer2ScrollSettingsError) -> Self {
        Self::Layer2Scroll(value)
    }
}

impl fmt::Display for MwlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MWL level file: {self:?}")
    }
}

impl std::error::Error for MwlError {}

impl Default for MwlFile {
    fn default() -> Self {
        let mut attribution = [b' '; Self::ATTRIBUTION_LEN];
        let label = b"Lunar Magic compatible Rust MWL";
        attribution[..label.len()].copy_from_slice(label);
        Self {
            version: Self::CURRENT_VERSION,
            flags: 0,
            attribution,
            sections: std::array::from_fn(|_| MwlSection::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_section_container_round_trips() {
        let mut file = MwlFile {
            flags: 1,
            ..MwlFile::default()
        };
        file.set_section(MwlSectionKind::LevelHeader, vec![1; 0x40]);
        file.set_section(MwlSectionKind::Layer1, vec![2, 3, 4]);
        file.set_section(MwlSectionKind::ExpandedHeader, vec![8; 0x20]);
        let encoded = file.encode().unwrap();
        assert_eq!(&encoded[..4], &[b'L', b'M', 0x63, 0x03]);
        assert_eq!(MwlFile::decode(&encoded).unwrap(), file);
    }

    #[test]
    fn section_cannot_point_into_directory() {
        let mut bytes = MwlFile::default().encode().unwrap();
        bytes[0x40..0x44].copy_from_slice(&0x40_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&1_u32.to_le_bytes());
        assert!(matches!(
            MwlFile::decode(&bytes),
            Err(MwlError::SectionOverlapsHeader { .. })
        ));
    }

    #[test]
    fn common_payload_prefix_round_trips() {
        let section = MwlPayloadSection {
            metadata: [0x1234_5678, 0x90ab_cdef],
            payload: vec![1, 2, 3],
        };
        assert_eq!(
            MwlPayloadSection::decode(&section.encode().unwrap()).unwrap(),
            section
        );
        let mut header = MwlLevelHeaderSection([0; 0x40]);
        header.set_level_number(0x105);
        assert_eq!(header.level_number(), 0x105);
    }

    #[test]
    fn typed_layer2_section_preserves_unknown_metadata_and_models_native_bank_transition() {
        let source = MwlLayer2Section {
            descriptor: MwlLayer2Descriptor::from_raw(0xdead_be8d),
            source_address: 0x00ff_d900,
            payload: vec![0xf1; 0x800],
        };
        let mut file = MwlFile::default();
        file.set_layer2_section(&source).unwrap();
        assert_eq!(file.layer2_section().unwrap(), source);

        let changed = source.descriptor.with_active_bank(5).unwrap();
        assert_eq!(changed.raw(), 0xdead_bedd);
        assert_eq!(changed.active_bank(), 5);
        assert!(!changed.uses_compressed_tilemap());
        assert!(changed.uses_split_planes());

        let normalized = source.descriptor.after_native_remap(3).unwrap();
        assert_eq!(normalized.raw(), 0x37);
        assert_eq!(normalized.active_bank(), 3);
        assert!(normalized.uses_compressed_tilemap());
        assert!(normalized.uses_split_planes());
        let normalized_legacy = MwlLayer2Descriptor::from_raw(0x08)
            .after_native_remap(2)
            .unwrap();
        assert_eq!(normalized_legacy.raw(), 0x22);
        assert!(!normalized_legacy.uses_split_planes());
        assert_eq!(
            source.descriptor.with_active_bank(8),
            Err(MwlLayer2DescriptorError::ActiveBank(8))
        );
        assert_eq!(
            source.descriptor.after_native_remap(8),
            Err(MwlLayer2DescriptorError::ActiveBank(8))
        );
    }

    #[test]
    fn exact_expanded_settings_section_round_trips() {
        let record = crate::ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        let mut file = MwlFile::default();
        file.set_expanded_settings_section(&record);
        assert_eq!(file.expanded_settings_section().unwrap(), record);
        file.set_section(MwlSectionKind::ExpandedHeader, vec![0; 31]);
        assert!(file.expanded_settings_section().is_err());
    }

    #[test]
    fn payload_section_length_overflow_is_typed() {
        assert_eq!(
            payload_section_len(0).unwrap(),
            MwlPayloadSection::METADATA_LEN
        );
        assert_eq!(payload_section_len(usize::MAX), Err(MwlError::Overflow));
    }

    #[test]
    fn directory_ranges_must_be_disjoint_and_empty_entries_canonical() {
        let mut bytes = MwlFile::default().encode().unwrap();
        bytes.push(7);
        for entry in [0x40, 0x48] {
            bytes[entry..entry + 4].copy_from_slice(&0x80_u32.to_le_bytes());
            bytes[entry + 4..entry + 8].copy_from_slice(&1_u32.to_le_bytes());
        }
        assert_eq!(
            MwlFile::decode(&bytes),
            Err(MwlError::SectionOverlap {
                first: 0,
                second: 1
            })
        );
        let mut empty_offset = MwlFile::default().encode().unwrap();
        empty_offset[0x40..0x44].copy_from_slice(&0x80_u32.to_le_bytes());
        assert_eq!(
            MwlFile::decode(&empty_offset),
            Err(MwlError::EmptySectionOffset {
                index: 0,
                offset: 0x80
            })
        );
    }

    #[test]
    fn every_canonical_prefix_and_excessive_declared_section_fail() {
        let bytes = MwlFile::default().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(MwlFile::decode(&bytes[..end]).is_err());
        }
        let mut excessive = bytes;
        excessive[0x40..0x44].copy_from_slice(&0x80_u32.to_le_bytes());
        excessive[0x44..0x48].copy_from_slice(
            &u32::try_from(MwlFile::MAX_SECTION_BYTES + 1)
                .unwrap()
                .to_le_bytes(),
        );
        assert!(matches!(
            MwlFile::decode(&excessive),
            Err(MwlError::SectionTooLarge { index: 0, .. })
        ));
    }
}
