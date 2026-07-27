use super::{NativeLevelAssetsController, NativeLevelAssetsControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    LevelLayer2SaveOptions, NativeLevelAssetsLayer2, NativeLevelAssetsLayer2Layout,
    NativeLevelAssetsLayer2SaveOptions, NativeLevelAssetsSaveOptions, PayloadReclamation, Project,
    RatsOwnershipManifest, RomMutation,
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
        self.prepare_commit_inner(description, options, None, None)
    }

    /// Serializes the complete aggregate including the profile-described native Layer 2 payload.
    ///
    /// # Errors
    ///
    /// Rejects a controller without Layer 2, stale/malformed data, or any grouped allocation and
    /// semantic-reopen failure without touching the live project.
    pub fn prepare_commit_with_layer2(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
        layer2_options: &LevelLayer2SaveOptions,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        self.prepare_commit_inner(description, options, Some(layer2_options), None)
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
        self.prepare_commit_inner(description, options, None, Some(manifest))
    }

    /// Serializes the five-domain aggregate and reclaims exact manifest-owned prior payloads.
    ///
    /// # Errors
    ///
    /// Returns typed ownership, Layer 2, aggregate, allocation, or checksum errors atomically.
    pub fn prepare_commit_with_layer2_and_reclamation(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
        layer2_options: &LevelLayer2SaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        self.prepare_commit_inner(description, options, Some(layer2_options), Some(manifest))
    }

    fn prepare_commit_inner(
        &self,
        description: impl Into<String>,
        options: &NativeLevelAssetsSaveOptions,
        layer2_options: Option<&LevelLayer2SaveOptions>,
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
        let (effective, effective_layer2) =
            self.effective_save_options(options, layer2_options, manifest.is_some());
        let mut project = Project::new(image);
        if self.layer2.is_some() && layer2_options.is_none() {
            return Err(NativeLevelAssetsControllerError::Layer2SaveOptionsRequired);
        }
        self.save_private_project(&mut project, effective, effective_layer2, manifest)?;
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

    fn effective_save_options(
        &self,
        options: &NativeLevelAssetsSaveOptions,
        layer2_options: Option<&LevelLayer2SaveOptions>,
        reclaim: bool,
    ) -> (NativeLevelAssetsSaveOptions, Option<LevelLayer2SaveOptions>) {
        let mut effective = options.clone();
        let mut effective_layer2 = layer2_options.cloned();
        if reclaim {
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
            if let Some(layer2) = effective_layer2.as_mut() {
                layer2.previous_block.clone_from(&self.previous_blocks[4]);
            }
        }
        (effective, effective_layer2)
    }

    fn save_private_project(
        &self,
        project: &mut Project,
        effective: NativeLevelAssetsSaveOptions,
        effective_layer2: Option<LevelLayer2SaveOptions>,
        manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<(), NativeLevelAssetsControllerError> {
        if let (Some(layer2), Some(layer2_layout), Some(layer2_options)) =
            (&self.layer2, self.layer2_layout, effective_layer2)
        {
            let assets = NativeLevelAssetsLayer2 {
                core: self.assets.as_save_assets(),
                layer2,
                layer2_descriptor: self.layer2_descriptor,
            };
            let layout = NativeLevelAssetsLayer2Layout {
                core: self.layout,
                layer2: layer2_layout,
            };
            let options = NativeLevelAssetsLayer2SaveOptions {
                core: effective,
                layer2: layer2_options,
            };
            if let Some(manifest) = manifest {
                project
                    .save_native_level_assets_with_layer2_and_reclamation(
                        assets,
                        layout,
                        &self.sprite_lengths,
                        &self.double_size_modes,
                        &options,
                        PayloadReclamation {
                            checksum_field: self.checksum_field,
                            manifest,
                        },
                    )
                    .map_err(NativeLevelAssetsControllerError::Layer2Save)?;
            } else {
                project
                    .save_native_level_assets_with_layer2(
                        assets,
                        layout,
                        &self.sprite_lengths,
                        &self.double_size_modes,
                        self.checksum_field,
                        &options,
                    )
                    .map_err(NativeLevelAssetsControllerError::Layer2Save)?;
            }
        } else if let Some(manifest) = manifest {
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
        Ok(())
    }
}
