//! One fully decoded semantic view of Lunar Magic's eight-section MWL container.

use crate::{MwlExAnimationSectionError, MwlOptionalLevelAssets, MwlOptionalLevelAssetsError};
use lm_graphics::{CompactExAnimation, Palette};
use lm_level::{
    ExpandedLevelSettingsError, ExpandedLevelSettingsRecord, LevelEditError, LevelObjectData,
    MwlError, MwlFile, MwlLayer2Descriptor, MwlLayer2Section, MwlLevelHeaderSection,
    MwlPayloadSection, MwlSecondaryExit, MwlSecondaryExitDecodeError, MwlSectionKind,
    NativeLayer2Data, NativeLayer2Error, NativeSpriteEncodingError, NativeSpriteStream,
    ObjectStreamError, SecondaryExitEncodingError, SpriteLengthTable, SpriteStreamError,
};
use std::fmt;

/// All modeled content in one binary MWL file, including lossless provenance words.
///
/// Keeping the section metadata beside the semantic payloads lets callers retarget the level
/// number while retaining every opaque address/descriptor word emitted by Lunar Magic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlNativeLevel {
    pub version: u16,
    pub flags: u32,
    pub attribution: [u8; MwlFile::ATTRIBUTION_LEN],
    pub header: MwlLevelHeaderSection,
    pub layer1_metadata: [u32; 2],
    pub layer1: LevelObjectData,
    pub layer2_descriptor: MwlLayer2Descriptor,
    pub layer2_source_address: u32,
    pub layer2: NativeLayer2Data,
    pub sprite_metadata: [u32; 2],
    pub sprites: NativeSpriteStream,
    pub palette_metadata: [u32; 2],
    pub palette: Palette,
    pub secondary_exit_metadata: [u32; 2],
    pub secondary_exits: Vec<MwlSecondaryExit>,
    pub exanimation_metadata: [u32; 2],
    pub exanimation: Option<CompactExAnimation>,
    pub expanded_settings: Option<ExpandedLevelSettingsRecord>,
}

impl MwlNativeLevel {
    /// Retargets the file to another native level slot.
    ///
    /// MWL secondary-exit records implicitly target the imported level, so their semantic
    /// destinations move with the header rather than retaining the source slot.
    ///
    /// # Errors
    ///
    /// Rejects targets outside Lunar Magic's native nine-bit level namespace.
    pub fn retarget(&mut self, level_number: u16) -> Result<(), MwlNativeLevelError> {
        if level_number > 0x01ff {
            return Err(MwlSecondaryExitDecodeError::TargetLevelOutOfRange(level_number).into());
        }
        self.header.set_level_number(level_number);
        for record in &mut self.secondary_exits {
            record.exit.destination_level = level_number;
        }
        Ok(())
    }

    /// Decodes and validates every MWL section before returning any semantic state.
    ///
    /// Sprite parsing follows bit `$20` in the sprite-stream header, while Layer 2 parsing follows
    /// the level mode embedded in Layer 1 exactly as Lunar Magic does. The container flags remain
    /// opaque: Lunar Magic exports expanded sprite streams while leaving those flags zero.
    ///
    /// # Errors
    ///
    /// Returns the first section framing, semantic payload, or revision-table decoding error.
    pub fn decode(
        file: &MwlFile,
        sprite_lengths: &SpriteLengthTable,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, MwlNativeLevelError> {
        let header = MwlLevelHeaderSection::decode(file.section(MwlSectionKind::LevelHeader))?;
        let level_number = header.level_number();
        let layer1 = file.payload_section(MwlSectionKind::Layer1)?;
        let layer1_data = LevelObjectData::parse(&layer1.payload)?;
        let layer2 = file.layer2_section()?;
        let layer2_data =
            NativeLayer2Data::decode_mwl(layer1_data.header.level_mode(), &layer2.payload)?;
        let sprites = file.payload_section(MwlSectionKind::Sprites)?;
        let expanded_sprites = sprites
            .payload
            .first()
            .is_some_and(|header| NativeSpriteStream::header_uses_expanded_framing(*header));
        let sprite_data =
            NativeSpriteStream::parse(&sprites.payload, expanded_sprites, sprite_lengths)?;
        let optional =
            MwlOptionalLevelAssets::decode(file, maximum_animation_records, double_size_modes)?;
        let secondary = file.payload_section(MwlSectionKind::SecondaryExits)?;
        let secondary_exits = MwlSecondaryExit::decode_all(&secondary.payload, level_number)?;
        let expanded_settings = match file.section(MwlSectionKind::ExpandedHeader) {
            [] => None,
            bytes => Some(ExpandedLevelSettingsRecord::decode(bytes)?),
        };
        Ok(Self {
            version: file.version,
            flags: file.flags,
            attribution: file.attribution,
            header,
            layer1_metadata: layer1.metadata,
            layer1: layer1_data,
            layer2_descriptor: layer2.descriptor,
            layer2_source_address: layer2.source_address,
            layer2: layer2_data,
            sprite_metadata: sprites.metadata,
            sprites: sprite_data,
            palette_metadata: optional.palette_metadata,
            palette: optional.palette,
            secondary_exit_metadata: secondary.metadata,
            secondary_exits,
            exanimation_metadata: optional.exanimation_metadata,
            exanimation: optional.exanimation,
            expanded_settings,
        })
    }

    /// Encodes a canonical eight-section MWL file without discarding opaque metadata.
    ///
    /// # Errors
    ///
    /// Returns a typed error if any semantic payload cannot be represented canonically.
    pub fn encode(
        &self,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
    ) -> Result<MwlFile, MwlNativeLevelError> {
        let mut canonical_sprites = self.sprites.clone();
        canonical_sprites.canonicalize_for_orientation(self.layer1.header.is_vertical())?;
        let mut file = MwlFile {
            version: self.version,
            flags: self.flags,
            attribution: self.attribution,
            sections: std::array::from_fn(|_| lm_level::MwlSection::default()),
        };
        let level_number = self.header.level_number();
        file.set_section(MwlSectionKind::LevelHeader, self.header.0.to_vec());
        file.set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: self.layer1_metadata,
                payload: self.layer1.encode()?,
            },
        )?;
        file.set_layer2_section(&MwlLayer2Section {
            descriptor: self.layer2_descriptor,
            source_address: self.layer2_source_address,
            payload: self.layer2.encode_mwl()?,
        })?;
        file.set_payload_section(
            MwlSectionKind::Sprites,
            &MwlPayloadSection {
                metadata: self.sprite_metadata,
                payload: canonical_sprites.encode_for_table(sprite_lengths)?,
            },
        )?;
        MwlOptionalLevelAssets {
            palette_metadata: self.palette_metadata,
            palette: self.palette.clone(),
            exanimation_metadata: self.exanimation_metadata,
            exanimation: self.exanimation.clone(),
        }
        .install_into(&mut file, double_size_modes)?;
        let records = self
            .secondary_exits
            .iter()
            .copied()
            .map(|mut record| {
                record.exit.destination_level = level_number;
                record
            })
            .collect::<Vec<_>>();
        file.set_payload_section(
            MwlSectionKind::SecondaryExits,
            &MwlPayloadSection {
                metadata: self.secondary_exit_metadata,
                payload: MwlSecondaryExit::encode_all(&records)?,
            },
        )?;
        if let Some(settings) = &self.expanded_settings {
            file.set_expanded_settings_section(settings);
        }
        Ok(file)
    }
}

#[derive(Debug)]
pub enum MwlNativeLevelError {
    Mwl(MwlError),
    Objects(ObjectStreamError),
    Layer2(NativeLayer2Error),
    Sprites(SpriteStreamError),
    SpriteEncoding(NativeSpriteEncodingError),
    OptionalAssets(MwlOptionalLevelAssetsError),
    ExAnimation(MwlExAnimationSectionError),
    SecondaryExitDecode(MwlSecondaryExitDecodeError),
    SecondaryExitEncode(SecondaryExitEncodingError),
    ExpandedSettings(ExpandedLevelSettingsError),
    SpriteCanonicalization(LevelEditError),
}

impl fmt::Display for MwlNativeLevelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid semantic MWL level: {self:?}")
    }
}

impl std::error::Error for MwlNativeLevelError {}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for MwlNativeLevelError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_error!(MwlError, Mwl);
from_error!(ObjectStreamError, Objects);
from_error!(NativeLayer2Error, Layer2);
from_error!(SpriteStreamError, Sprites);
from_error!(NativeSpriteEncodingError, SpriteEncoding);
from_error!(MwlOptionalLevelAssetsError, OptionalAssets);
from_error!(MwlExAnimationSectionError, ExAnimation);
from_error!(MwlSecondaryExitDecodeError, SecondaryExitDecode);
from_error!(SecondaryExitEncodingError, SecondaryExitEncode);
from_error!(ExpandedLevelSettingsError, ExpandedSettings);
from_error!(LevelEditError, SpriteCanonicalization);

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Bgr555;
    use lm_level::{MwlSecondaryExit, NATIVE_LAYER2_TILEMAP_LEN, SecondaryExit};

    fn semantic_level() -> MwlNativeLevel {
        let mut header = MwlLevelHeaderSection([0; MwlLevelHeaderSection::ENCODED_LEN]);
        header.set_level_number(0x105);
        MwlNativeLevel {
            version: MwlFile::CURRENT_VERSION,
            flags: 0xa0,
            attribution: [0x5a; MwlFile::ATTRIBUTION_LEN],
            header,
            layer1_metadata: [3, 0x06_88dd],
            layer1: LevelObjectData::parse(&[0, 0, 0, 0, 0, 0xff]).unwrap(),
            layer2_descriptor: MwlLayer2Descriptor::from_raw(0x22),
            layer2_source_address: 0xff_d900,
            layer2: NativeLayer2Data::Tilemap(vec![0x34; NATIVE_LAYER2_TILEMAP_LEN]),
            sprite_metadata: [7, 0x07_c4ca],
            sprites: NativeSpriteStream::parse(
                &[0x10, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
            palette_metadata: [9, 0x10_8031],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            secondary_exit_metadata: [0, 0],
            secondary_exits: vec![MwlSecondaryExit {
                index: 0x123,
                exit: SecondaryExit {
                    destination_level: 0x105,
                    position_and_method: 2,
                    screen: 3,
                    x: 4,
                    y: 5,
                    destination_flags: 0x20,
                    x_and_overworld_flags: 0x80,
                    additional_flags: 6,
                },
                reserved: 0xaa,
            }],
            exanimation_metadata: [0, 0],
            exanimation: None,
            expanded_settings: Some(ExpandedLevelSettingsRecord::from_encoded([0x77; 0x20])),
        }
    }

    #[test]
    fn all_eight_sections_round_trip_semantically_and_preserve_metadata() {
        let lengths = SpriteLengthTable::standard();
        let modes = [false; 256];
        let expected = semantic_level();
        let file = expected.encode(&lengths, &modes).unwrap();
        let actual = MwlNativeLevel::decode(&file, &lengths, 32, &modes).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(file.flags, 0xa0);
        assert!(
            file.sections
                .iter()
                .all(|section| !section.bytes.is_empty())
        );
    }

    #[test]
    fn sprite_header_is_the_framing_authority_and_container_flags_stay_opaque() {
        let lengths = SpriteLengthTable::standard();
        let modes = [false; 256];
        let mut expected = semantic_level();
        expected.flags = 1;
        expected.sprites = NativeSpriteStream::parse(
            &[0x20, 0xff, 1, 0x00, 0x00, 0x01, 0xff, 0xfe],
            true,
            &lengths,
        )
        .unwrap();

        let file = expected.encode(&lengths, &modes).unwrap();
        assert_eq!(file.flags, 1);
        assert!(NativeSpriteStream::header_uses_expanded_framing(
            file.payload_section(MwlSectionKind::Sprites)
                .unwrap()
                .payload[0]
        ));
        assert_eq!(
            MwlNativeLevel::decode(&file, &lengths, 32, &modes).unwrap(),
            expected
        );

        let mut legacy = semantic_level().encode(&lengths, &modes).unwrap();
        legacy.flags = 1;
        assert!(
            !MwlNativeLevel::decode(&legacy, &lengths, 32, &modes)
                .unwrap()
                .sprites
                .expanded
        );
    }

    #[test]
    fn retarget_moves_header_and_implicit_secondary_exit_destinations_only() {
        let mut level = semantic_level();
        let before = level.clone();
        level.retarget(0x1ab).unwrap();
        assert_eq!(level.header.level_number(), 0x1ab);
        assert_eq!(level.secondary_exits[0].exit.destination_level, 0x1ab);
        level.header.set_level_number(before.header.level_number());
        level.secondary_exits[0].exit.destination_level =
            before.secondary_exits[0].exit.destination_level;
        assert_eq!(level, before);
        assert!(level.retarget(0x200).is_err());
    }
}
