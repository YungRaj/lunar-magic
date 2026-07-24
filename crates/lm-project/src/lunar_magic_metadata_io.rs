//! Fixed-location Lunar Magic ROM attribution and feature metadata persistence.

use crate::{Project, payload::staging::commit_staged};
use lm_rom::{
    LunarMagicRomMetadata, LunarMagicRomMetadataError, Mapper, RomError, compute_snes_checksum,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LunarMagicRomMetadataLayout {
    pub mapper: Mapper,
    pub attribution: usize,
    pub vram_version: usize,
    pub feature_record: usize,
}

#[derive(Debug)]
pub enum LunarMagicRomMetadataIoError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    PartialInstallation,
    Rom(RomError),
    Metadata(LunarMagicRomMetadataError),
    Commit(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for LunarMagicRomMetadataIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Lunar Magic ROM metadata operation failed: {self:?}"
        )
    }
}

impl std::error::Error for LunarMagicRomMetadataIoError {}

impl From<RomError> for LunarMagicRomMetadataIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<LunarMagicRomMetadataError> for LunarMagicRomMetadataIoError {
    fn from(value: LunarMagicRomMetadataError) -> Self {
        Self::Metadata(value)
    }
}

impl From<crate::PayloadSaveError> for LunarMagicRomMetadataIoError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Loads pristine absence or the complete fixed-location Lunar Magic metadata snapshot.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, out-of-range fields, partial installations, invalid
    /// attribution signatures, and noncanonical checksum-status reserved bits.
    pub fn load_lunar_magic_rom_metadata(
        &self,
        layout: LunarMagicRomMetadataLayout,
    ) -> Result<Option<LunarMagicRomMetadata>, LunarMagicRomMetadataIoError> {
        validate_mapper(self, layout.mapper)?;
        let attribution = self
            .rom
            .read(layout.attribution, LunarMagicRomMetadata::ATTRIBUTION_LEN)?;
        let vram = self.rom.read(layout.vram_version, 1)?[0];
        let feature = self
            .rom
            .read(layout.feature_record, LunarMagicRomMetadata::FEATURE_LEN)?;
        let attribution_absent = attribution.iter().all(|byte| *byte == 0xff);
        let record_absent = vram == 0xff && feature.iter().all(|byte| *byte == 0xff);
        if attribution_absent && record_absent {
            return Ok(None);
        }
        if attribution_absent || record_absent {
            return Err(LunarMagicRomMetadataIoError::PartialInstallation);
        }
        Ok(Some(LunarMagicRomMetadata::from_parts(
            attribution,
            vram,
            feature,
        )?))
    }

    /// Writes all fixed metadata fields and checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects foreign/partial current metadata, mapper disagreement, invalid ranges/checksum, or
    /// semantic disagreement after reopen. Failure leaves ROM and history unchanged.
    pub fn save_lunar_magic_rom_metadata(
        &mut self,
        metadata: &LunarMagicRomMetadata,
        layout: LunarMagicRomMetadataLayout,
        checksum_field: usize,
    ) -> Result<bool, LunarMagicRomMetadataIoError> {
        let current = self.load_lunar_magic_rom_metadata(layout)?;
        if current.as_ref() == Some(metadata) {
            return Ok(false);
        }
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = original.clone();
        copy_checked(&mut staged, layout.attribution, metadata.attribution())?;
        copy_checked(&mut staged, layout.vram_version, &[metadata.vram_version()])?;
        copy_checked(
            &mut staged,
            layout.feature_record,
            metadata.feature_record(),
        )?;
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        copy_checked(&mut staged, checksum_field, &checksum.encoded())?;
        commit_staged(
            self,
            "replace Lunar Magic ROM metadata".into(),
            &original,
            &staged,
        )?;
        if self.load_lunar_magic_rom_metadata(layout)?.as_ref() != Some(metadata) {
            return Err(LunarMagicRomMetadataIoError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), LunarMagicRomMetadataIoError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(LunarMagicRomMetadataIoError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn copy_checked(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), RomError> {
    let image_len = bytes.len();
    let target = bytes
        .get_mut(offset..offset.saturating_add(value.len()))
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: value.len(),
            image_len,
        })?;
    target.copy_from_slice(value);
    Ok(())
}
