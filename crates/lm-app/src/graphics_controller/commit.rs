use super::{GraphicsController, GraphicsControllerError};
use crate::PreparedRomCommit;
use lm_project::{GraphicsSaveOptions, Project, RatsOwnershipManifest, RomMutation};
use lm_rom::RomImage;

impl GraphicsController {
    /// Compresses and allocates the edited file on a private project, repairs its checksum, and
    /// returns one compact revision-bound commit.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsControllerError`] for invalid source bytes, compression/size/allocation
    /// failure, or unexpected shrink preparation.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &GraphicsSaveOptions,
    ) -> Result<PreparedRomCommit, GraphicsControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(GraphicsControllerError::Rom)?;
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
            .save_graphics_file_with_checksum(
                self.file_number,
                &self.graphics,
                self.layout,
                self.checksum_field_offset,
                options,
            )
            .map_err(GraphicsControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(GraphicsControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Prepares an atomic allocation, repoint, owned-block reclamation, and checksum mutation.
    ///
    /// Ownership is bound to the exact tagged descriptor loaded into this controller; the caller
    /// cannot substitute a different previous block through `options`.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsControllerError`] for invalid source bytes, an untagged or stale source,
    /// non-exact ownership, unsafe overlap, compression/allocation failure, or mutation failure.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &GraphicsSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, GraphicsControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(GraphicsControllerError::Rom)?;
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
            .save_graphics_file_with_checksum_and_reclamation(
                self.file_number,
                &self.graphics,
                self.layout,
                self.checksum_field_offset,
                &options,
                manifest,
            )
            .map_err(GraphicsControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(GraphicsControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
