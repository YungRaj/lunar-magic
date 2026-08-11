use super::{ExAnimationController, ExAnimationControllerError, ExAnimationControllerTarget};
use crate::PreparedRomCommit;
use lm_project::{
    ExAnimationSaveOptions, PayloadReclamation, PayloadSaveResult, Project, RatsOwnershipManifest,
    RomMutation,
};
use lm_rom::RomImage;

impl ExAnimationController {
    /// Saves the edited semantic animation into an already evolving staging project.
    pub fn save_to_project(
        &self,
        project: &mut Project,
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationControllerError> {
        match self.target {
            ExAnimationControllerTarget::Level(slot) => project.save_exanimation_with_checksum(
                slot,
                &self.animation,
                self.layout,
                &self.double_size_modes,
                self.checksum_field_offset,
                options,
            ),
            ExAnimationControllerTarget::Global(installation) => project
                .save_installed_global_exanimation_with_checksum(
                    &self.animation,
                    installation,
                    &self.double_size_modes,
                    self.checksum_field_offset,
                    options,
                ),
        }
        .map_err(ExAnimationControllerError::Io)
    }

    /// Encodes and allocates the compact payload on a private project, repairs the checksum, and
    /// returns a revision-bound prepared mutation.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError`] for source, encoding, size-mode, allocation, checksum,
    /// or mutation-preparation failures.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &ExAnimationSaveOptions,
    ) -> Result<PreparedRomCommit, ExAnimationControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(ExAnimationControllerError::Rom)?;
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
        match self.target {
            ExAnimationControllerTarget::Level(slot) => project
                .save_exanimation_with_checksum(
                    slot,
                    &self.animation,
                    self.layout,
                    &self.double_size_modes,
                    self.checksum_field_offset,
                    options,
                )
                .map(|_| ()),
            ExAnimationControllerTarget::Global(installation) => project
                .save_installed_global_exanimation_with_checksum(
                    &self.animation,
                    installation,
                    &self.double_size_modes,
                    self.checksum_field_offset,
                    options,
                )
                .map(|_| ()),
        }
        .map_err(ExAnimationControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(ExAnimationControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Prepares one snapshot-bound `ExAnimation` relocation and reclamation mutation.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError`] for an untagged/stale source, non-exact ownership,
    /// size-mode/encoding failure, unsafe overlap, allocation/checksum failure, or mutation failure.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &ExAnimationSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, ExAnimationControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(ExAnimationControllerError::Rom)?;
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
        let reclamation = PayloadReclamation {
            checksum_field: self.checksum_field_offset,
            manifest,
        };
        match self.target {
            ExAnimationControllerTarget::Level(slot) => project
                .save_exanimation_with_checksum_and_reclamation(
                    slot,
                    &self.animation,
                    self.layout,
                    &self.double_size_modes,
                    &options,
                    reclamation,
                )
                .map(|_| ()),
            ExAnimationControllerTarget::Global(installation) => project
                .save_installed_global_exanimation_with_checksum_and_reclamation(
                    &self.animation,
                    installation,
                    &self.double_size_modes,
                    &options,
                    reclamation,
                )
                .map(|_| ()),
        }
        .map_err(ExAnimationControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(ExAnimationControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
