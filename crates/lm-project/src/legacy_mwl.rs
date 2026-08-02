use lm_graphics::{Palette, PaletteEncodingError};
use lm_level::{
    ExpandedLevelSettingsRecord, Layer2Storage, LegacyMwlError, LegacyMwlManifest,
    LegacyMwlSecondaryExit, LegacyMwlSidecar, LevelEditError, LevelObjectData, MwlFile,
    MwlLayer2Descriptor, MwlLevelHeaderSection, MwlSecondaryExit, NativeLayer2Data,
    NativeLayer2Error, NativeSpriteEncodingError, NativeSpriteStream, ObjectStreamError,
    SecondaryExit, SpriteLengthTable, SpriteStreamError, level_mode_layer2_storage,
};
use std::fmt;

use crate::MwlNativeLevel;

/// Lunar Magic 3.63's four-file legacy level representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyMwlBundle {
    pub manifest: LegacyMwlManifest,
    pub layer1: Vec<u8>,
    pub layer2: Vec<u8>,
    pub sprites: Vec<u8>,
    pub palette: Option<Vec<u8>>,
}

impl LegacyMwlBundle {
    pub const ATTRIBUTION: &str = "©2025 FuSoYa, Defender of Relm";
    pub const MAX_SIDECAR_BYTES: usize = 0x8000;
    pub const PALETTE_BYTES: usize = 0x202;

    /// Projects a native MWL level into Lunar Magic's deliberately lossy legacy format.
    ///
    /// Legacy files cannot carry `ExAnimation`, expanded settings, midway-only fields, or modern
    /// secondary-exit planes. The original exporter omits those domains too.
    ///
    /// # Errors
    ///
    /// Rejects unsafe filenames, oversized or unencodable streams, invalid exits, and a palette
    /// shape other than Lunar Magic's exact 257 words.
    pub fn from_native(
        source: &MwlNativeLevel,
        base_name: &str,
        sprite_lengths: &SpriteLengthTable,
    ) -> Result<Self, LegacyMwlBundleError> {
        validate_base_name(base_name)?;
        validate_source_address("Layer 1", source.layer1_metadata[1])?;
        validate_source_address("Layer 2", source.layer2_source_address)?;
        validate_source_address("sprites", source.sprite_metadata[1])?;
        let layer1 = source.layer1.encode()?;
        let layer2 = source.layer2.encode_mwl()?;
        let mut canonical_sprites = source.sprites.clone();
        canonical_sprites.canonicalize_for_orientation(source.layer1.header.is_vertical())?;
        let sprites = canonical_sprites.encode_for_table(sprite_lengths)?;
        validate_payloads(&layer1, &layer2, &sprites)?;
        let palette = if source.layer1_metadata[0] & 1 != 0 {
            let bytes = source.palette.encode_snes()?;
            if bytes.len() != Self::PALETTE_BYTES {
                return Err(LegacyMwlBundleError::PaletteShape(bytes.len()));
            }
            Some(bytes)
        } else {
            None
        };
        let manifest = LegacyMwlManifest {
            version: LegacyMwlManifest::CURRENT_VERSION,
            attribution: Self::ATTRIBUTION.into(),
            level_number: source.header.level_number(),
            header: [
                source.header.0[2],
                source.header.0[3],
                source.header.0[4],
                source.header.0[5],
                source.header.0[6],
            ],
            layer1: LegacyMwlSidecar {
                flags: low_byte(source.layer1_metadata[0]),
                source_address: source.layer1_metadata[1],
                file_name: format!("{base_name}.mw0"),
            },
            layer2: LegacyMwlSidecar {
                flags: low_byte(source.layer2_descriptor.raw()),
                source_address: source.layer2_source_address,
                file_name: format!("{base_name}.mw1"),
            },
            sprites: LegacyMwlSidecar {
                flags: (low_byte(source.sprite_metadata[0]) & !1)
                    | u8::from(canonical_sprites.expanded),
                source_address: source.sprite_metadata[1],
                file_name: format!("{base_name}.mw2"),
            },
            secondary_exits: source
                .secondary_exits
                .iter()
                .map(legacy_secondary_exit)
                .collect::<Result<_, _>>()?,
        };
        manifest.encode()?;
        Ok(Self {
            manifest,
            layer1,
            layer2,
            sprites,
            palette,
        })
    }

    /// Reconstructs the semantic aggregate initialized by Lunar Magic's legacy importer.
    ///
    /// A manifest without a custom palette retains the destination palette. Installed expanded
    /// settings are reset to Lunar Magic's recovered default record.
    ///
    /// # Errors
    ///
    /// Rejects malformed or oversized sidecars, invalid streams, and palette shape mismatches.
    pub fn decode_native(
        &self,
        sprite_lengths: &SpriteLengthTable,
        destination_palette: &Palette,
        expanded_settings_installed: bool,
    ) -> Result<MwlNativeLevel, LegacyMwlBundleError> {
        self.manifest.validate()?;
        validate_payloads(&self.layer1, &self.layer2, &self.sprites)?;
        let layer1 = LevelObjectData::parse(&self.layer1)?;
        let layer2 = NativeLayer2Data::decode_mwl(layer1.header.level_mode(), &self.layer2)?;
        let expanded_sprites = self.manifest.sprites.flags & 1 != 0;
        let sprites = NativeSpriteStream::parse(&self.sprites, expanded_sprites, sprite_lengths)?;
        let palette = if self.manifest.layer1.flags & 1 != 0 {
            let bytes = self
                .palette
                .as_deref()
                .ok_or(LegacyMwlBundleError::MissingPalette)?;
            if bytes.len() != Self::PALETTE_BYTES {
                return Err(LegacyMwlBundleError::PaletteShape(bytes.len()));
            }
            Palette::decode_snes(bytes).map_err(LegacyMwlBundleError::PaletteShape)?
        } else {
            let mut palette = destination_palette.clone();
            palette.colors.rotate_left(1);
            palette
        };
        if palette.colors.len() != 257 {
            return Err(LegacyMwlBundleError::PaletteShape(palette.colors.len() * 2));
        }
        let mut header = MwlLevelHeaderSection([0; MwlLevelHeaderSection::ENCODED_LEN]);
        header.set_level_number(self.manifest.level_number);
        header.0[2..7].copy_from_slice(&self.manifest.header);
        let secondary_exits = self
            .manifest
            .secondary_exits
            .iter()
            .copied()
            .map(|exit| modern_secondary_exit(exit, &self.manifest))
            .collect();
        let layer2_descriptor = normalized_layer2_descriptor(
            self.manifest.version,
            self.manifest.layer2.flags,
            layer1.header.level_mode(),
        );
        Ok(MwlNativeLevel {
            version: self.manifest.version,
            // The legacy manifest's sprite flag controls only its standalone `.mw2` stream.
            // Binary MWL container flags are a separate opaque field and have no legacy source.
            flags: MwlFile::default().flags,
            attribution: binary_attribution(&self.manifest.attribution),
            header,
            layer1_metadata: [
                u32::from(self.manifest.layer1.flags),
                self.manifest.layer1.source_address,
            ],
            layer1,
            layer2_descriptor,
            layer2_source_address: self.manifest.layer2.source_address,
            layer2,
            sprite_metadata: [
                u32::from(self.manifest.sprites.flags),
                self.manifest.sprites.source_address,
            ],
            sprites,
            palette_metadata: [0; 2],
            palette,
            secondary_exit_metadata: [0; 2],
            secondary_exits,
            exanimation_metadata: [0; 2],
            exanimation: None,
            expanded_settings: expanded_settings_installed.then(default_expanded_settings),
        })
    }
}

fn validate_source_address(kind: &'static str, address: u32) -> Result<(), LegacyMwlBundleError> {
    if address > 0x00ff_ffff {
        return Err(LegacyMwlBundleError::SourceAddress { kind, address });
    }
    Ok(())
}

fn normalized_layer2_descriptor(version: u16, flags: u8, level_mode: u8) -> MwlLayer2Descriptor {
    let flags = if version >= 0x0341 {
        flags
    } else {
        match level_mode_layer2_storage(level_mode) {
            Layer2Storage::Objects => 0,
            Layer2Storage::CompressedTilemap => {
                let normalized = flags & 0xf6;
                if flags & 2 == 0 {
                    normalized | 8
                } else {
                    normalized
                }
            }
        }
    };
    MwlLayer2Descriptor::from_raw(u32::from(flags))
}

fn validate_payloads(
    layer1: &[u8],
    layer2: &[u8],
    sprites: &[u8],
) -> Result<(), LegacyMwlBundleError> {
    for (kind, bytes) in [
        ("Layer 1", layer1),
        ("Layer 2", layer2),
        ("sprites", sprites),
    ] {
        if bytes.len() > LegacyMwlBundle::MAX_SIDECAR_BYTES {
            return Err(LegacyMwlBundleError::SidecarTooLarge {
                kind,
                bytes: bytes.len(),
            });
        }
    }
    Ok(())
}

fn low_byte(value: u32) -> u8 {
    value.to_le_bytes()[0]
}

fn validate_base_name(base_name: &str) -> Result<(), LegacyMwlBundleError> {
    let path = std::path::Path::new(base_name);
    if base_name.is_empty()
        || base_name.contains(['\r', '\n', '\0'])
        || path.is_absolute()
        || path.components().count() != 1
        || base_name.len() + 4 > LegacyMwlManifest::MAX_LINE_BYTES
    {
        return Err(LegacyMwlBundleError::UnsafeBaseName(base_name.into()));
    }
    Ok(())
}

fn legacy_secondary_exit(
    source: &MwlSecondaryExit,
) -> Result<LegacyMwlSecondaryExit, LegacyMwlBundleError> {
    let encoded = source.encode()?;
    Ok(LegacyMwlSecondaryExit {
        index: source.index,
        position_and_method: encoded[2],
        screen_and_y: encoded[3],
        destination_high_and_flags: encoded[4],
    })
}

fn modern_secondary_exit(
    source: LegacyMwlSecondaryExit,
    manifest: &LegacyMwlManifest,
) -> MwlSecondaryExit {
    let mut destination_flags = source.destination_high_and_flags & !8;
    let mut x = 0;
    if manifest.header[3] & 0x20 == 0 {
        x = destination_flags >> 5 & 1;
        destination_flags &= !0x20;
    }
    MwlSecondaryExit {
        index: source.index,
        exit: SecondaryExit {
            destination_level: manifest.level_number,
            position_and_method: source.position_and_method,
            screen: source.screen_and_y >> 4,
            y: source.screen_and_y & 0x0f,
            x,
            destination_flags,
            x_and_overworld_flags: 0,
            additional_flags: 0,
        },
        reserved: 0,
    }
}

fn binary_attribution(attribution: &str) -> [u8; MwlFile::ATTRIBUTION_LEN] {
    let mut bytes = [b' '; MwlFile::ATTRIBUTION_LEN];
    let source = attribution.as_bytes();
    let length = source.len().min(bytes.len());
    bytes[..length].copy_from_slice(&source[..length]);
    bytes
}

fn default_expanded_settings() -> ExpandedLevelSettingsRecord {
    let mut bytes = [0x7f; ExpandedLevelSettingsRecord::ENCODED_LEN];
    bytes[16..18].copy_from_slice(&u16::MAX.to_le_bytes());
    for (offset, value) in [(24, 0x2b_u16), (26, 0x2a), (28, 0x29), (30, 0x28)] {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    ExpandedLevelSettingsRecord::from_encoded(bytes)
}

#[derive(Debug)]
pub enum LegacyMwlBundleError {
    Manifest(LegacyMwlError),
    Objects(ObjectStreamError),
    Layer2(NativeLayer2Error),
    Sprites(SpriteStreamError),
    SpriteEncoding(NativeSpriteEncodingError),
    SpriteCanonicalization(LevelEditError),
    Palette(PaletteEncodingError),
    SecondaryExit(lm_level::SecondaryExitEncodingError),
    UnsafeBaseName(String),
    SourceAddress { kind: &'static str, address: u32 },
    SidecarTooLarge { kind: &'static str, bytes: usize },
    MissingPalette,
    PaletteShape(usize),
}

impl fmt::Display for LegacyMwlBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid legacy MWL bundle: {self:?}")
    }
}

impl std::error::Error for LegacyMwlBundleError {}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for LegacyMwlBundleError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_error!(LegacyMwlError, Manifest);
from_error!(ObjectStreamError, Objects);
from_error!(NativeLayer2Error, Layer2);
from_error!(SpriteStreamError, Sprites);
from_error!(NativeSpriteEncodingError, SpriteEncoding);
from_error!(LevelEditError, SpriteCanonicalization);
from_error!(PaletteEncodingError, Palette);
from_error!(lm_level::SecondaryExitEncodingError, SecondaryExit);

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn level_105() -> MwlNativeLevel {
        let bytes =
            std::fs::read(root().join("oracle-work/lm363/pristine-us/levels/Level 105.mwl"))
                .unwrap();
        let file = MwlFile::decode(&bytes).unwrap();
        MwlNativeLevel::decode(&file, &SpriteLengthTable::standard(), 32, &[false; 256]).unwrap()
    }

    #[test]
    fn binary_level_projects_to_exact_live_lunar_magic_legacy_files() {
        let bundle =
            LegacyMwlBundle::from_native(&level_105(), "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        let fixture = root().join("oracle-work/lm363/pristine-us/legacy-level-105");
        assert_eq!(
            bundle.manifest.encode().unwrap(),
            std::fs::read(fixture.join("Level 105.mwl")).unwrap()
        );
        assert_eq!(
            bundle.layer1,
            std::fs::read(fixture.join("Level 105.mw0")).unwrap()
        );
        assert_eq!(
            bundle.layer2,
            std::fs::read(fixture.join("Level 105.mw1")).unwrap()
        );
        assert_eq!(
            bundle.sprites,
            std::fs::read(fixture.join("Level 105.mw2")).unwrap()
        );
        assert!(bundle.palette.is_none());
    }

    #[test]
    fn legacy_decode_reconstructs_current_semantics_and_defaults() {
        let source = level_105();
        let bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        let mut destination_palette = source.palette.clone();
        destination_palette.colors.rotate_right(1);
        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination_palette, true)
            .unwrap();
        assert_eq!(decoded.header.0[..7], source.header.0[..7]);
        assert_eq!(decoded.layer1, source.layer1);
        assert_eq!(decoded.layer2, source.layer2);
        assert_eq!(decoded.sprites, source.sprites);
        assert_eq!(decoded.palette, source.palette);
        assert_eq!(
            decoded.expanded_settings.unwrap(),
            default_expanded_settings()
        );
        assert_eq!(decoded.secondary_exits[0].index, 0x1cb);
    }

    #[test]
    fn pre_341_layer2_flags_follow_recovered_import_normalization() {
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xff, 1).raw(), 0);
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xff, 0).raw(), 0xf6);
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xf5, 0).raw(), 0xfc);
        assert_eq!(normalized_layer2_descriptor(0x0341, 0xff, 1).raw(), 0xff);
    }

    #[test]
    fn legacy_export_rejects_addresses_the_format_cannot_represent() {
        let mut source = level_105();
        source.layer1_metadata[1] = 0x0100_0000;
        assert!(matches!(
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard()),
            Err(LegacyMwlBundleError::SourceAddress {
                kind: "Layer 1",
                address: 0x0100_0000
            })
        ));
    }
}
