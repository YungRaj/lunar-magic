use super::{NativeLevelAssetsController, NativeLevelAssetsControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    NativeLevelAssetsSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
};
use lm_rom::RomImage;

impl NativeLevelAssetsController {
    /// Serializes the complete aggregate on a private image and returns one revision-bound commit.
    ///
    /// # Errors
    ///
    /// Returns an error when the source image cannot be reopened, an edited domain cannot be
    /// serialized, or the resulting ROM mutation cannot be constructed.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        self.prepare_commit_inner(description, options, None)
    }

    /// Serializes the aggregate while reclaiming only snapshot blocks proven by `manifest`.
    ///
    /// # Errors
    ///
    /// Returns an error for stale/non-exact ownership, domain serialization, allocation, protected
    /// direct-write, checksum, or resulting mutation failure without touching the live project.
    pub fn prepare_commit_with_reclamation(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        self.prepare_commit_inner(description, options, Some(manifest))
    }

    fn prepare_commit_inner(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
        manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(NativeLevelAssetsControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.level.mapper, before.len()),
            });
        }
        let mut effective = options.clone();
        if manifest.is_some() {
            effective
                .level
                .previous_layer1
                .clone_from(&self.previous_blocks[0]);
            effective
                .level
                .previous_sprites
                .clone_from(&self.previous_blocks[1]);
            effective
                .palette
                .previous_block
                .clone_from(&self.previous_blocks[2]);
            effective
                .exanimation
                .previous_block
                .clone_from(&self.previous_blocks[3]);
        }
        let mut project = Project::new(image);
        if let Some(manifest) = manifest {
            project
                .save_native_level_assets_with_reclamation(
                    self.assets.as_save_assets(),
                    self.layout,
                    &self.sprite_lengths,
                    &self.double_size_modes,
                    &effective,
                    PayloadReclamation {
                        checksum_field: self.checksum_field,
                        manifest,
                    },
                )
                .map_err(NativeLevelAssetsControllerError::Save)?;
        } else {
            project
                .save_native_level_assets(
                    self.assets.as_save_assets(),
                    self.layout,
                    &self.sprite_lengths,
                    &self.double_size_modes,
                    self.checksum_field,
                    &effective,
                )
                .map_err(NativeLevelAssetsControllerError::Save)?;
        }
        let mutation = RomMutation::between(
            self.layout.level.mapper,
            &before,
            project.rom.logical_bytes(),
        )
        .map_err(NativeLevelAssetsControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}
