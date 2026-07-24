use super::{OverworldController, OverworldControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    CompleteOverworldSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
};
use lm_rom::RomImage;

impl OverworldController {
    /// Saves all nine native payloads on a private project, repairs the checksum, and returns one
    /// expandable revision-bound application mutation.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldControllerError`] for any domain shape, encoding, allocation, mapper,
    /// checksum, or mutation-preparation failure.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &CompleteOverworldSaveOptions,
    ) -> Result<PreparedRomCommit, OverworldControllerError> {
        self.prepare_commit_inner(description, options, None)
    }

    /// Saves all nine payloads while reclaiming only snapshot blocks proven by `manifest`.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldControllerError`] for invalid ownership, model, allocation, mapper,
    /// checksum, or mutation preparation. The live application remains unchanged.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &CompleteOverworldSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, OverworldControllerError> {
        self.prepare_commit_inner(description, options, Some(manifest))
    }

    fn prepare_commit_inner(
        &self,
        description: impl Into<String>,
        options: &CompleteOverworldSaveOptions,
        manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<PreparedRomCommit, OverworldControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(OverworldControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.layers.mapper, before.len()),
            });
        }
        let mut effective = options.clone();
        if manifest.is_some() {
            effective
                .layers
                .previous_layer1
                .clone_from(&self.previous_blocks[0]);
            effective
                .layers
                .previous_layer2
                .clone_from(&self.previous_blocks[1]);
            effective
                .event_reveals
                .previous_sources
                .clone_from(&self.previous_blocks[2]);
            effective
                .event_reveals
                .previous_destinations
                .clone_from(&self.previous_blocks[3]);
            effective
                .endpoints
                .previous_block
                .clone_from(&self.previous_blocks[4]);
            effective
                .messages
                .previous_block
                .clone_from(&self.previous_blocks[5]);
            effective
                .sprites
                .previous_block
                .clone_from(&self.previous_blocks[6]);
            effective
                .palette
                .previous_block
                .clone_from(&self.previous_blocks[7]);
            effective
                .animation
                .previous_block
                .clone_from(&self.previous_blocks[8]);
        }
        let mut project = Project::new(image);
        if let Some(manifest) = manifest {
            project
                .save_complete_overworld_with_checksum_and_reclamation(
                    self.slot,
                    &self.data,
                    self.layout,
                    &effective,
                    &self.double_size_modes,
                    PayloadReclamation {
                        checksum_field: self.checksum_field_offset,
                        manifest,
                    },
                )
                .map_err(OverworldControllerError::Io)?;
        } else {
            project
                .save_complete_overworld_with_checksum(
                    self.slot,
                    &self.data,
                    self.layout,
                    &effective,
                    &self.double_size_modes,
                    self.checksum_field_offset,
                )
                .map_err(OverworldControllerError::Io)?;
        }
        let mutation = RomMutation::between(
            self.layout.layers.mapper,
            &before,
            project.rom.logical_bytes(),
        )
        .map_err(OverworldControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
