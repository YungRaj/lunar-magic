use lm_graphics::{Palette, PaletteEncodingError};
use lm_level::{
    ExpandedLevelSettingsRecord, Layer2Storage, LegacyMwlError, LegacyMwlManifest,
    LegacyMwlSecondaryExit, LegacyMwlSidecar, LevelEditError, LevelObjectData, MwlFile,
    MwlLayer2Descriptor, MwlLevelHeaderSection, MwlSecondaryExit, NATIVE_LAYER2_TILEMAP_LEN,
    NativeLayer2Data, NativeLayer2Error, NativeSpriteEncodingError, NativeSpriteStream,
    ObjectStreamError, SecondaryExit, SpriteLengthTable, SpriteStreamError,
    level_mode_layer2_storage,
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
                flags: low_byte(source.sprite_metadata[0]),
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
        let layer1 = match LevelObjectData::parse(&self.layer1) {
            Ok(layer1) => layer1,
            Err(ObjectStreamError::MissingTerminator) => {
                // Lunar Magic's legacy file reader supplies an end marker after a complete
                // unterminated `.mw0` stream. Retry only this exact boundary: a genuinely
                // truncated object record remains an error instead of borrowing the marker as
                // record data.
                let mut terminated = self.layer1.clone();
                terminated.push(0xff);
                LevelObjectData::parse(&terminated)?
            }
            Err(error) => return Err(error.into()),
        };
        let layer2 = if level_mode_layer2_storage(layer1.header.level_mode())
            == Layer2Storage::CompressedTilemap
            && self.layer2.len() != NATIVE_LAYER2_TILEMAP_LEN
        {
            // The legacy importer clears its fixed 0x800-byte background workspace before
            // reading at most that many `.mw1` bytes. A short read leaves the unread suffix
            // zeroed, while bytes after the workspace are ignored. Binary MWL sections remain
            // exact-length validated by `NativeLayer2Data::decode_mwl`; this recovery belongs
            // only to the legacy sidecar.
            let mut padded = vec![0; NATIVE_LAYER2_TILEMAP_LEN];
            let imported = self.layer2.len().min(NATIVE_LAYER2_TILEMAP_LEN);
            padded[..imported].copy_from_slice(&self.layer2[..imported]);
            NativeLayer2Data::decode_mwl(layer1.header.level_mode(), &padded)?
        } else {
            NativeLayer2Data::decode_mwl(layer1.header.level_mode(), &self.layer2)?
        };
        let expanded_sprites = self
            .sprites
            .first()
            .is_some_and(|header| NativeSpriteStream::header_uses_expanded_framing(*header));
        let sprites =
            match NativeSpriteStream::parse(&self.sprites, expanded_sprites, sprite_lengths) {
                Ok(sprites) => sprites,
                Err(SpriteStreamError::MissingTerminator) if !expanded_sprites => {
                    // The legacy reader supplies the one-byte terminator after a complete standard
                    // sprite stream. Expanded `$FF $FE` recovery is deliberately not inferred from
                    // this standard-stream observation.
                    let mut terminated = self.sprites.clone();
                    terminated.push(0xff);
                    NativeSpriteStream::parse(&terminated, false, sprite_lengths)?
                }
                Err(error) => return Err(error.into()),
            };
        let requested_custom_palette = self.manifest.layer1.flags & 1 != 0;
        let imported_custom_palette = requested_custom_palette && self.palette.is_some();
        let mut palette = destination_palette.clone();
        palette.colors.rotate_left(1);
        if let Some(bytes) = self.palette.as_deref().filter(|_| requested_custom_palette) {
            if bytes.len() > Self::MAX_SIDECAR_BYTES {
                return Err(LegacyMwlBundleError::SidecarTooLarge {
                    kind: "palette",
                    bytes: bytes.len(),
                });
            }
            let mut merged = palette.encode_snes()?;
            if merged.len() != Self::PALETTE_BYTES {
                return Err(LegacyMwlBundleError::PaletteShape(merged.len()));
            }
            let imported = bytes.len().min(Self::PALETTE_BYTES);
            merged[..imported].copy_from_slice(&bytes[..imported]);
            palette = Palette::decode_snes(&merged).map_err(LegacyMwlBundleError::PaletteShape)?;
        }
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
        let layer1_flags = if imported_custom_palette {
            self.manifest.layer1.flags
        } else {
            self.manifest.layer1.flags & !1
        };
        let layer2_descriptor = normalized_layer2_descriptor(
            self.manifest.version,
            self.manifest.layer2.flags,
            layer1.header.level_mode(),
        );
        Ok(MwlNativeLevel {
            version: self.manifest.version,
            // The legacy manifest's sprite flag is opaque metadata. Framing authority comes from
            // bit $20 of the standalone `.mw2` header, just as it does in binary MWL payloads.
            // Binary MWL container flags remain a separate opaque field with no legacy source.
            flags: MwlFile::default().flags,
            attribution: binary_attribution(&self.manifest.attribution),
            header,
            layer1_metadata: [u32::from(layer1_flags), self.manifest.layer1.source_address],
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

    fn level_000() -> MwlNativeLevel {
        let bytes =
            std::fs::read(root().join(
                "oracle-work/lm363/pristine-us/palette-install-positive/exported/Level 000.mwl",
            ))
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
    fn authentic_custom_palette_legacy_bundle_matches_binary_level_semantics() {
        let root = root();
        let binary =
            std::fs::read(root.join(
                "oracle-work/lm363/pristine-us/palette-install-positive/exported/Level 000.mwl",
            ))
            .unwrap();
        let file = MwlFile::decode(&binary).unwrap();
        let source =
            MwlNativeLevel::decode(&file, &SpriteLengthTable::standard(), 32, &[false; 256])
                .unwrap();
        let projected =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        let fixture = root.join("oracle-work/lm363/pristine-us/legacy-level-000-custom-palette");
        let manifest_bytes = std::fs::read(fixture.join("Level 000.mwl")).unwrap();
        let report = LegacyMwlManifest::decode_with_diagnostics(&manifest_bytes).unwrap();
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.manifest.secondary_exits.len(), 0x1ef3);
        assert_eq!(
            report.manifest.secondary_exits.last().unwrap().index,
            0x1fff
        );
        let authentic = LegacyMwlBundle {
            manifest: report.manifest,
            layer1: std::fs::read(fixture.join("Level 000.mw0")).unwrap(),
            layer2: std::fs::read(fixture.join("Level 000.mw1")).unwrap(),
            sprites: std::fs::read(fixture.join("Level 000.mw2")).unwrap(),
            palette: Some(std::fs::read(fixture.join("Level 000.mw3")).unwrap()),
        };
        let mut destination = source.palette.clone();
        destination.colors.rotate_left(31);
        let imported = authentic
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();

        assert_eq!(imported.layer1, source.layer1);
        assert_eq!(imported.layer2, source.layer2);
        assert_eq!(imported.sprites, source.sprites);
        assert_eq!(imported.palette, source.palette);
        assert_ne!(imported.palette, destination);
        assert_eq!(
            projected.layer1,
            std::fs::read(fixture.join("Level 000.mw0")).unwrap()
        );
        assert_eq!(
            projected.layer2,
            std::fs::read(fixture.join("Level 000.mw1")).unwrap()
        );
        assert_eq!(
            projected.sprites,
            std::fs::read(fixture.join("Level 000.mw2")).unwrap()
        );
        assert_eq!(
            projected.palette.unwrap(),
            std::fs::read(fixture.join("Level 000.mw3")).unwrap()
        );
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
    fn custom_palette_legacy_bundle_round_trips_the_exact_mw3_payload() {
        let mut source = level_105();
        source.layer1_metadata[0] |= 1;
        source.palette.colors.rotate_left(17);
        let bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        assert_eq!(bundle.manifest.layer1.flags & 1, 1);
        assert_eq!(
            bundle.manifest.palette_file_name().unwrap(),
            "Level 105.mw3"
        );
        assert_eq!(
            bundle.palette.as_ref().unwrap().len(),
            LegacyMwlBundle::PALETTE_BYTES
        );
        assert_eq!(
            bundle.palette.as_ref().unwrap(),
            &source.palette.encode_snes().unwrap()
        );

        let mut destination = source.palette.clone();
        destination.colors.rotate_right(43);
        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();
        assert_eq!(decoded.palette, source.palette);
        assert_ne!(decoded.palette, destination);
    }

    #[test]
    fn missing_declared_palette_falls_back_to_shared_palette_and_clears_custom_flag() {
        let mut source = level_105();
        source.layer1_metadata[0] |= 1;
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        bundle.palette = None;
        let mut destination = source.palette.clone();
        destination.colors.rotate_right(29);
        let mut expected = destination.clone();
        expected.colors.rotate_left(1);

        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();
        assert_eq!(decoded.layer1_metadata[0] & 1, 0);
        assert_eq!(decoded.palette, expected);
    }

    #[test]
    fn present_partial_palette_overlays_destination_bytes_and_keeps_custom_flag() {
        let mut source = level_105();
        source.layer1_metadata[0] |= 1;
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        let mut destination = source.palette.clone();
        destination.colors.rotate_right(11);
        let mut expected = destination.clone();
        expected.colors.rotate_left(1);
        let mut expected_bytes = expected.encode_snes().unwrap();
        expected_bytes[..3].copy_from_slice(&[0x34, 0x12, 0x56]);
        expected = Palette::decode_snes(&expected_bytes).unwrap();
        bundle.palette = Some(vec![0x34, 0x12, 0x56]);

        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();
        assert_eq!(decoded.layer1_metadata[0] & 1, 1);
        assert_eq!(decoded.palette, expected);
    }

    #[test]
    fn present_empty_and_trailing_palette_payloads_match_lunar_magic_reads() {
        let mut source = level_105();
        source.layer1_metadata[0] |= 1;
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        let full = bundle.palette.clone().unwrap();
        let mut destination = source.palette.clone();
        destination.colors.rotate_right(7);

        bundle.palette = Some(Vec::new());
        let empty = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();
        let mut expected_empty = destination.clone();
        expected_empty.colors.rotate_left(1);
        assert_eq!(empty.layer1_metadata[0] & 1, 1);
        assert_eq!(empty.palette, expected_empty);

        bundle.palette = Some([full.as_slice(), &[0xaa, 0xbb]].concat());
        let trailing = bundle
            .decode_native(&SpriteLengthTable::standard(), &destination, true)
            .unwrap();
        assert_eq!(trailing.layer1_metadata[0] & 1, 1);
        assert_eq!(trailing.palette, source.palette);
    }

    #[test]
    fn short_legacy_layer2_tilemap_keeps_prefix_and_zero_fills_unread_workspace() {
        let source = level_000();
        assert_eq!(
            level_mode_layer2_storage(source.layer1.header.level_mode()),
            Layer2Storage::CompressedTilemap
        );
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        bundle.layer2 = vec![0xf1, 0x00];

        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        let NativeLayer2Data::Tilemap(tilemap) = decoded.layer2 else {
            panic!("legacy background sidecar decoded as objects");
        };
        assert_eq!(tilemap.len(), NATIVE_LAYER2_TILEMAP_LEN);
        assert_eq!(&tilemap[..2], &[0xf1, 0x00]);
        assert!(tilemap[2..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn overlong_legacy_layer2_tilemap_ignores_bytes_after_fixed_workspace() {
        let source = level_000();
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        let authentic = bundle.layer2.clone();
        bundle.layer2.extend_from_slice(&[0xaa, 0xbb]);

        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert_eq!(decoded.layer2, NativeLayer2Data::Tilemap(authentic));
    }

    #[test]
    fn legacy_layer1_accepts_trailing_bytes_and_supplies_only_a_missing_terminator() {
        let source = level_000();
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        let terminated = bundle.layer1.clone();

        bundle.layer1.extend_from_slice(&[0xaa, 0xbb]);
        let trailing = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert_eq!(trailing.layer1, source.layer1);

        bundle.layer1 = terminated[..terminated.len() - 1].to_vec();
        let unterminated = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert_eq!(unterminated.layer1, source.layer1);

        bundle.layer1 = terminated[..terminated.len() - 2].to_vec();
        assert!(
            bundle
                .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
                .is_err(),
            "a partial final object must not borrow the synthesized terminator as record data"
        );
    }

    #[test]
    fn legacy_standard_sprites_accept_trailing_bytes_and_supply_a_missing_terminator() {
        let source = level_000();
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        assert_eq!(bundle.manifest.sprites.flags & 1, 0);
        let terminated = bundle.sprites.clone();

        bundle.sprites.extend_from_slice(&[0xaa, 0xbb]);
        let trailing = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert_eq!(trailing.sprites, source.sprites);

        bundle.sprites = terminated[..terminated.len() - 1].to_vec();
        let unterminated = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert_eq!(unterminated.sprites, source.sprites);

        bundle.sprites = terminated[..terminated.len() - 2].to_vec();
        assert!(
            bundle
                .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
                .is_err(),
            "a partial final sprite must not borrow the synthesized terminator as record data"
        );
    }

    #[test]
    fn legacy_sprite_manifest_flag_is_opaque_and_stream_header_owns_framing() {
        let source = level_000();
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        let standard_bytes = bundle.sprites.clone();
        bundle.manifest.sprites.flags |= 1;

        let decoded = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, true)
            .unwrap();
        assert!(!decoded.sprites.expanded);
        assert_eq!(decoded.sprite_metadata[0] & 1, 1);

        let reexport =
            LegacyMwlBundle::from_native(&decoded, "Level 000", &SpriteLengthTable::standard())
                .unwrap();
        assert_eq!(reexport.manifest.sprites.flags & 1, 1);
        assert_eq!(reexport.sprites, standard_bytes);
    }

    #[test]
    fn pre_341_layer2_flags_follow_recovered_import_normalization() {
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xff, 1).raw(), 0);
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xff, 0).raw(), 0xf6);
        assert_eq!(normalized_layer2_descriptor(0x0340, 0xf5, 0).raw(), 0xfc);
        assert_eq!(normalized_layer2_descriptor(0x0341, 0xff, 1).raw(), 0xff);
    }

    #[test]
    fn historical_layer2_version_boundary_flows_through_complete_bundle_decode() {
        let source = level_105();
        assert_eq!(source.layer1.header.level_mode(), 0);
        let mut bundle =
            LegacyMwlBundle::from_native(&source, "Level 105", &SpriteLengthTable::standard())
                .unwrap();
        bundle.manifest.layer2.flags = 0xff;
        bundle.manifest.version = 0x0340;
        let old = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, false)
            .unwrap();
        assert_eq!(old.version, 0x0340);
        assert_eq!(old.layer2_descriptor.raw(), 0xf6);

        bundle.manifest.version = 0x0341;
        let current = bundle
            .decode_native(&SpriteLengthTable::standard(), &source.palette, false)
            .unwrap();
        assert_eq!(current.version, 0x0341);
        assert_eq!(current.layer2_descriptor.raw(), 0xff);
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
