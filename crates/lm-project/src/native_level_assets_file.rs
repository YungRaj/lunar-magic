//! Canonical aggregate interchange for all modeled native per-level assets.

use crate::LoadedNativeLevelAssets;
use lm_graphics::{
    CompactExAnimationFile, CompactExAnimationFileError, PaletteInterchangeError,
    PaletteInterchangeFile,
};
use lm_level::{
    ExpandedLevelSettingsError, ExpandedLevelSettingsRecord, NativeLevelFile, NativeLevelFileError,
    SpriteLengthTable,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsFile {
    pub source_slot: u16,
    pub assets: LoadedNativeLevelAssets,
}

impl NativeLevelAssetsFile {
    pub const MAGIC: [u8; 8] = *b"LMNATAS1";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 32;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN
        + NativeLevelFile::MAX_FILE_LEN
        + PaletteInterchangeFile::MAX_FILE_LEN
        + CompactExAnimationFile::MAX_FILE_LEN
        + ExpandedLevelSettingsRecord::ENCODED_LEN;

    /// Encodes all nested resources canonically and requires their source slot to agree.
    ///
    /// # Errors
    ///
    /// Returns a nested serialization, slot, length, or arithmetic error.
    pub fn encode(
        &self,
        double_size_modes: &[bool],
    ) -> Result<Vec<u8>, NativeLevelAssetsFileError> {
        self.validate_slot()?;
        let level = NativeLevelFile {
            source_level: self.source_slot,
            layer1: self.assets.level.layer1.clone(),
            sprites: self.assets.level.sprites.clone(),
        }
        .encode()?;
        let palette = PaletteInterchangeFile {
            source_palette: self.source_slot,
            palette: self.assets.palette.clone(),
        }
        .encode()?;
        let animation = CompactExAnimationFile {
            source_slot: self.source_slot,
            animation: self.assets.exanimation.clone(),
        }
        .encode(double_size_modes)?;
        let settings = self
            .assets
            .expanded_settings
            .as_ref()
            .map(ExpandedLevelSettingsRecord::encoded);
        let lengths = [
            level.len(),
            palette.len(),
            animation.len(),
            settings.as_ref().map_or(0, |bytes| bytes.len()),
        ];
        let total = lengths.iter().try_fold(Self::HEADER_LEN, |total, len| {
            total
                .checked_add(*len)
                .ok_or(NativeLevelAssetsFileError::Overflow)
        })?;
        if total > Self::MAX_FILE_LEN {
            return Err(NativeLevelAssetsFileError::FileTooLarge(total));
        }
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&u16::from(settings.is_some()).to_le_bytes());
        bytes.extend_from_slice(&self.source_slot.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        for len in lengths {
            bytes.extend_from_slice(
                &u32::try_from(len)
                    .map_err(|_| NativeLevelAssetsFileError::Overflow)?
                    .to_le_bytes(),
            );
        }
        bytes.extend_from_slice(&level);
        bytes.extend_from_slice(&palette);
        bytes.extend_from_slice(&animation);
        if let Some(settings) = settings {
            bytes.extend_from_slice(settings);
        }
        Ok(bytes)
    }

    /// Decodes all nested resources with explicit revision interpretation tables.
    ///
    /// # Errors
    ///
    /// Rejects malformed framing, unsupported flags, excessive or inconsistent section lengths,
    /// noncanonical nested resources, source-slot disagreement, or invalid expanded settings.
    pub fn decode(
        bytes: &[u8],
        sprite_lengths: &SpriteLengthTable,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, NativeLevelAssetsFileError> {
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(NativeLevelAssetsFileError::FileTooLarge(bytes.len()));
        }
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(NativeLevelAssetsFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(NativeLevelAssetsFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(NativeLevelAssetsFileError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([header[10], header[11]]);
        if flags & !1 != 0 {
            return Err(NativeLevelAssetsFileError::UnknownFlags(flags));
        }
        if header[14] != 0 || header[15] != 0 {
            return Err(NativeLevelAssetsFileError::ReservedBytes);
        }
        let source_slot = u16::from_le_bytes([header[12], header[13]]);
        let lengths = std::array::from_fn::<_, 4, _>(|index| {
            let offset = 16 + index * 4;
            usize::try_from(u32::from_le_bytes([
                header[offset],
                header[offset + 1],
                header[offset + 2],
                header[offset + 3],
            ]))
            .map_err(|_| NativeLevelAssetsFileError::Overflow)
        });
        let lengths = lengths.into_iter().collect::<Result<Vec<_>, _>>()?;
        if lengths[3] != if flags & 1 != 0 { 32 } else { 0 } {
            return Err(NativeLevelAssetsFileError::SettingsLength(lengths[3]));
        }
        let expected = lengths.iter().try_fold(Self::HEADER_LEN, |total, len| {
            total
                .checked_add(*len)
                .ok_or(NativeLevelAssetsFileError::Overflow)
        })?;
        if bytes.len() != expected {
            return Err(NativeLevelAssetsFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut offset = Self::HEADER_LEN;
        let mut take = |len: usize| {
            let section = &bytes[offset..offset + len];
            offset += len;
            section
        };
        let level = NativeLevelFile::decode(take(lengths[0]), sprite_lengths)?;
        let palette = PaletteInterchangeFile::decode(take(lengths[1]))?;
        let animation = CompactExAnimationFile::decode(
            take(lengths[2]),
            maximum_animation_records,
            double_size_modes,
        )?;
        let settings = if flags & 1 != 0 {
            Some(ExpandedLevelSettingsRecord::decode(take(lengths[3]))?)
        } else {
            None
        };
        for (domain, actual) in [
            ("level", level.source_level),
            ("palette", palette.source_palette),
            ("ExAnimation", animation.source_slot),
        ] {
            if actual != source_slot {
                return Err(NativeLevelAssetsFileError::SourceSlotMismatch {
                    domain,
                    expected: source_slot,
                    actual,
                });
            }
        }
        let file = Self {
            source_slot,
            assets: LoadedNativeLevelAssets {
                level: crate::LoadedLevelSlot {
                    number: usize::from(source_slot),
                    layer1: level.layer1,
                    sprites: level.sprites,
                },
                palette: palette.palette,
                exanimation: animation.animation,
                expanded_settings: settings,
            },
        };
        file.validate_slot()?;
        Ok(file)
    }

    fn validate_slot(&self) -> Result<(), NativeLevelAssetsFileError> {
        if self.assets.level.number == usize::from(self.source_slot) {
            Ok(())
        } else {
            Err(NativeLevelAssetsFileError::SourceSlotMismatch {
                domain: "assets",
                expected: self.source_slot,
                actual: u16::try_from(self.assets.level.number).unwrap_or(u16::MAX),
            })
        }
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    UnknownFlags(u16),
    ReservedBytes,
    SettingsLength(usize),
    WrongLength {
        expected: usize,
        actual: usize,
    },
    FileTooLarge(usize),
    SourceSlotMismatch {
        domain: &'static str,
        expected: u16,
        actual: u16,
    },
    Overflow,
    Level(NativeLevelFileError),
    Palette(PaletteInterchangeError),
    ExAnimation(CompactExAnimationFileError),
    ExpandedSettings(ExpandedLevelSettingsError),
}

impl fmt::Display for NativeLevelAssetsFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid aggregate native level-assets file: {self:?}"
        )
    }
}

impl std::error::Error for NativeLevelAssetsFileError {}

impl From<NativeLevelFileError> for NativeLevelAssetsFileError {
    fn from(value: NativeLevelFileError) -> Self {
        Self::Level(value)
    }
}
impl From<PaletteInterchangeError> for NativeLevelAssetsFileError {
    fn from(value: PaletteInterchangeError) -> Self {
        Self::Palette(value)
    }
}
impl From<CompactExAnimationFileError> for NativeLevelAssetsFileError {
    fn from(value: CompactExAnimationFileError) -> Self {
        Self::ExAnimation(value)
    }
}
impl From<ExpandedLevelSettingsError> for NativeLevelAssetsFileError {
    fn from(value: ExpandedLevelSettingsError) -> Self {
        Self::ExpandedSettings(value)
    }
}

#[cfg(test)]
#[path = "native_level_assets_file_tests.rs"]
mod tests;
