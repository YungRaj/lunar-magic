use super::{Map16Controller, Map16ControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    Map16SetSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
};
use lm_rom::RomImage;

impl Map16Controller {
    /// Saves the staged complete Map16 set onto an evolving recovery project.
    ///
    /// # Errors
    ///
    /// Returns the same table, allocation, mapper, and checksum failures as ordinary publication.
    pub fn save_to_project(
        &self,
        project: &mut Project,
        options: &Map16SetSaveOptions,
    ) -> Result<(), Map16ControllerError> {
        project
            .save_map16_set_with_checksum(
                &self.set,
                self.layout,
                self.checksum_field_offset,
                options,
            )
            .map(|_| ())
            .map_err(Map16ControllerError::Io)
    }

    /// Serializes all page pairs through the transactional allocator on a private project, repairs
    /// the checksum, and returns a compact application mutation.
    ///
    /// # Errors
    ///
    /// Returns [`Map16ControllerError`] for malformed source bytes, table/allocation failure, or an
    /// unexpected shrinking result. The application project is never touched here.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &Map16SetSaveOptions,
    ) -> Result<PreparedRomCommit, Map16ControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(Map16ControllerError::Rom)?;
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
            .save_map16_set_with_checksum(
                &self.set,
                self.layout,
                self.checksum_field_offset,
                options,
            )
            .map_err(Map16ControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(Map16ControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Serializes the complete set while reclaiming only snapshot-captured blocks proven by the
    /// supplied ownership manifest.
    ///
    /// # Errors
    ///
    /// Returns [`Map16ControllerError`] for malformed source bytes, non-exact ownership evidence,
    /// allocation/table failure, or an invalid resulting mutation. No application state changes.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &Map16SetSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, Map16ControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(Map16ControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut effective_options = options.clone();
        effective_options
            .previous_graphics
            .clone_from(&self.previous_graphics);
        effective_options
            .previous_acts_like
            .clone_from(&self.previous_acts_like);
        let mut project = Project::new(image);
        project
            .save_map16_set_with_checksum_and_reclamation(
                &self.set,
                self.layout,
                &effective_options,
                PayloadReclamation {
                    checksum_field: self.checksum_field_offset,
                    manifest,
                },
            )
            .map_err(Map16ControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(Map16ControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
