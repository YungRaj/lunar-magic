use super::{LevelController, LevelControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    LevelLayer2SaveOptions, LevelSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest,
    RomMutation,
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
        self.prepare_commit_internal(description.into(), options, None, false)
    }

    /// Prepares a commit that may relocate a growing pristine shared-bank sprite stream.
    ///
    /// The caller must provide a sprite allocation policy confined to the bank named by the
    /// shared pointer. Allocation and pointer validation fail atomically if that authority is
    /// incorrect or the bank has insufficient free space.
    ///
    /// # Errors
    ///
    /// Returns the same typed errors as [`Self::prepare_commit`], plus allocation or shared-bank
    /// mismatch errors when relocation is required.
    pub fn prepare_commit_with_shared_bank_sprite_relocation(
        &self,
        description: impl Into<String>,
        options: &LevelSaveOptions,
    ) -> Result<PreparedRomCommit, LevelControllerError> {
        self.prepare_commit_internal(description.into(), options, None, true)
    }

    /// Prepares one atomic mutation for Layer 1, sprites, and the optional staged Layer 2 stream.
    ///
    /// # Errors
    ///
    /// Returns a typed controller error for serialization, allocation, or snapshot failures.
    pub fn prepare_commit_with_layer2(
        &self,
        description: impl Into<String>,
        options: &LevelSaveOptions,
        layer2_options: &LevelLayer2SaveOptions,
        relocate_growing_shared_bank_sprites: bool,
    ) -> Result<PreparedRomCommit, LevelControllerError> {
        self.prepare_commit_internal(
            description.into(),
            options,
            Some(layer2_options),
            relocate_growing_shared_bank_sprites,
        )
    }

    fn prepare_commit_internal(
        &self,
        description: String,
        options: &LevelSaveOptions,
        layer2_options: Option<&LevelLayer2SaveOptions>,
        relocate_growing_shared_bank_sprites: bool,
    ) -> Result<PreparedRomCommit, LevelControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(LevelControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut project = Project::new(image);
        let level_changed = self.level != self.baseline;
        self.save_staged_level(
            &mut project,
            options,
            level_changed,
            relocate_growing_shared_bank_sprites,
        )?;
        self.save_staged_layer2(&mut project, layer2_options)?;
        self.validate_semantic_reopen(&project, level_changed)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(LevelControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    fn save_staged_level(
        &self,
        project: &mut Project,
        options: &LevelSaveOptions,
        level_changed: bool,
        relocate_growing_shared_bank_sprites: bool,
    ) -> Result<(), LevelControllerError> {
        let sprites_changed = self.level.sprites != self.baseline.sprites;
        let shared_bank_sprites = matches!(
            self.layout.sprites,
            lm_project::SpritePointerTable::SplitSharedBank { .. }
        );
        if level_changed && !sprites_changed {
            project
                .save_level_layer1_with_checksum(
                    self.layout,
                    &self.level,
                    self.checksum_field_offset,
                    options,
                )
                .map_err(LevelControllerError::Save)?;
        } else if level_changed && shared_bank_sprites {
            if self.level.layer1 != self.baseline.layer1 {
                project
                    .save_level_layer1_with_checksum(
                        self.layout,
                        &self.level,
                        self.checksum_field_offset,
                        options,
                    )
                    .map_err(LevelControllerError::Save)?;
            }
            let in_place = project.save_level_sprites_in_place_with_checksum(
                self.layout,
                &self.baseline,
                &self.level,
                &self.sprite_lengths,
                self.checksum_field_offset,
            );
            match in_place {
                Ok(_) => {}
                Err(lm_project::LevelSaveError::InPlaceSpriteGrowth { .. })
                    if relocate_growing_shared_bank_sprites =>
                {
                    project
                        .relocate_level_sprites_with_checksum(
                            self.layout,
                            &self.level,
                            &self.sprite_lengths,
                            self.checksum_field_offset,
                            options,
                        )
                        .map_err(LevelControllerError::Save)?;
                }
                Err(error) => return Err(LevelControllerError::Save(error)),
            }
        } else if level_changed {
            if self.level.layer1 != self.baseline.layer1 {
                project
                    .save_level_layer1_with_checksum(
                        self.layout,
                        &self.level,
                        self.checksum_field_offset,
                        options,
                    )
                    .map_err(LevelControllerError::Save)?;
            }
            if sprites_changed {
                project
                    .relocate_level_sprites_with_checksum(
                        self.layout,
                        &self.level,
                        &self.sprite_lengths,
                        self.checksum_field_offset,
                        options,
                    )
                    .map_err(LevelControllerError::Save)?;
            }
        }
        Ok(())
    }

    fn save_staged_layer2(
        &self,
        project: &mut Project,
        layer2_options: Option<&LevelLayer2SaveOptions>,
    ) -> Result<(), LevelControllerError> {
        if self.layer2 != self.baseline_layer2 {
            let layout = self
                .layer2_layout
                .ok_or(LevelControllerError::Layer2Unavailable)?;
            let layer2 = self
                .layer2
                .as_ref()
                .ok_or(LevelControllerError::Layer2Unavailable)?;
            let layer2_options = layer2_options.ok_or(LevelControllerError::Layer2Unavailable)?;
            project
                .save_level_layer2_with_descriptor_and_checksum(
                    self.level.number,
                    self.level.layer1.header.level_mode(),
                    &lm_project::LoadedLevelLayer2 {
                        data: layer2.clone(),
                        descriptor: self.layer2_descriptor,
                    },
                    layout,
                    layer2_options,
                    self.checksum_field_offset,
                )
                .map_err(LevelControllerError::Layer2Load)?;
        }
        Ok(())
    }

    fn validate_semantic_reopen(
        &self,
        project: &Project,
        level_changed: bool,
    ) -> Result<(), LevelControllerError> {
        if level_changed {
            let mut reopen_layout = self.layout;
            reopen_layout.expanded_sprites = self.level.sprites.expanded;
            let reopened = project
                .load_level_slot(self.level.number, reopen_layout, &self.sprite_lengths)
                .map_err(LevelControllerError::Load)?;
            if reopened != self.level {
                return Err(LevelControllerError::NonCanonicalLevelEncoding);
            }
        }
        if self.layer2 != self.baseline_layer2 {
            let reopened = project
                .load_level_layer2_with_descriptor(
                    self.level.number,
                    self.level.layer1.header.level_mode(),
                    self.layer2_layout
                        .ok_or(LevelControllerError::Layer2Unavailable)?,
                )
                .map_err(LevelControllerError::Layer2Load)?;
            if Some(reopened.data) != self.layer2 || reopened.descriptor != self.layer2_descriptor {
                return Err(LevelControllerError::NonCanonicalLayer2Encoding);
            }
        }
        Ok(())
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
