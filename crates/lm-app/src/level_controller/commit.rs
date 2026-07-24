use super::{LevelController, LevelControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    LevelSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
};
use lm_rom::RomImage;

impl LevelController {
    /// Runs the native serializer/allocator against a temporary copy and returns its compact ROM
    /// mutation without changing the application project or this controller's source snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`LevelControllerError`] for invalid source bytes, serialization/allocation errors,
    /// or an unexpected shrinking result.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &LevelSaveOptions,
    ) -> Result<PreparedRomCommit, LevelControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(LevelControllerError::Rom)?;
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
        if self.level.sprites == self.baseline.sprites {
            project
                .save_level_layer1_with_checksum(
                    self.layout,
                    &self.level,
                    self.checksum_field_offset,
                    options,
                )
                .map_err(LevelControllerError::Save)?;
        } else {
            project
                .save_level_slot_with_checksum(
                    self.layout,
                    &self.level,
                    &self.sprite_lengths,
                    self.checksum_field_offset,
                    options,
                )
                .map_err(LevelControllerError::Save)?;
        }
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(LevelControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Prepares one snapshot-bound two-stream level relocation and reclamation mutation.
    ///
    /// # Errors
    ///
    /// Returns [`LevelControllerError`] for untagged/stale streams, non-exact ownership,
    /// serialization, unsafe overlap, allocation/checksum, or mutation-preparation failure.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &LevelSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, LevelControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(LevelControllerError::Rom)?;
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
        options.previous_layer1.clone_from(&self.previous_layer1);
        options.previous_sprites.clone_from(&self.previous_sprites);
        let mut project = Project::new(image);
        project
            .save_level_slot_with_checksum_and_reclamation(
                self.layout,
                &self.level,
                &self.sprite_lengths,
                &options,
                PayloadReclamation {
                    checksum_field: self.checksum_field_offset,
                    manifest,
                },
            )
            .map_err(LevelControllerError::Save)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(LevelControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
