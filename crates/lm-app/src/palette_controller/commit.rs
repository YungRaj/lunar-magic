use super::{PaletteController, PaletteControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    PaletteSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
};
use lm_rom::RomImage;

impl PaletteController {
    /// Allocates and repoints the exact palette on a private project, repairs the checksum, and
    /// returns one compact revision-bound mutation.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteControllerError`] for source, shape, allocation, mapping, checksum, or
    /// mutation-preparation failure.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &PaletteSaveOptions,
    ) -> Result<PreparedRomCommit, PaletteControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(PaletteControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut project = Project::new(image);
        project
            .save_palette_with_checksum(
                self.palette_number,
                &self.palette,
                self.layout,
                self.checksum_field_offset,
                options,
            )
            .map_err(PaletteControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(PaletteControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Prepares one snapshot-bound palette relocation and reclamation mutation.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteControllerError`] for an untagged/stale source, non-exact ownership,
    /// unsafe overlap, palette/allocation/checksum failure, or mutation preparation failure.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &PaletteSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, PaletteControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(PaletteControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut options = options.clone();
        options.previous_block.clone_from(&self.previous_block);
        let mut project = Project::new(image);
        project
            .save_palette_with_checksum_and_reclamation(
                self.palette_number,
                &self.palette,
                self.layout,
                &options,
                PayloadReclamation {
                    checksum_field: self.checksum_field_offset,
                    manifest,
                },
            )
            .map_err(PaletteControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(PaletteControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
