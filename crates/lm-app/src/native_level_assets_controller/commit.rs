use super::{NativeLevelAssetsController, NativeLevelAssetsControllerError};
use crate::PreparedRomCommit;
use lm_project::{
    LevelLayer2SaveOptions, Lfix3LevelFields, NativeLevelAssetsLayer2,
    NativeLevelAssetsLayer2Layout, NativeLevelAssetsLayer2SaveOptions,
    NativeLevelAssetsSaveOptions, PayloadReclamation, Project, RatsOwnershipManifest, RomMutation,
    SecondaryExitStorage, VanillaMainEntrance,
};
use lm_rom::RomImage;

impl NativeLevelAssetsController {
    /// Prepares a complete MWL import for an SMW-US ROM with current Lunar Magic runtimes.
    ///
    /// The returned mutation includes every modeled MWL payload, all eight recovered main-entrance
    /// fields, the enabled separate-midway record, and replacement of secondary exits targeting
    /// the selected level. All private-project subtransactions collapse into one revision-checked
    /// live application mutation and therefore one undo step.
    ///
    /// # Errors
    ///
    /// Rejects missing current Lfix3/secondary-exit/separate-midway installations, invalid exit
    /// indexes, duplicate MWL exit records, serialization/allocation failures, or reopen mismatch
    /// without touching the live application project.
    pub fn prepare_smw_us_v1_installed_mwl_import(
        &self,
        source: &lm_project::MwlNativeLevel,
        options: &NativeLevelAssetsSaveOptions,
        layer2_options: &LevelLayer2SaveOptions,
    ) -> Result<PreparedRomCommit, NativeLevelAssetsControllerError> {
        let mut staged_controller = self.clone();
        staged_controller.replace_modeled_assets_from_mwl(source)?;
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(NativeLevelAssetsControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        if !image
            .read(0x26cc, 4)
            .is_ok_and(|hook| hook == [0x22, 0x00, 0xdd, 0x05])
        {
            return Err(NativeLevelAssetsControllerError::MwlLfix3Unavailable);
        }
        let mut project = Project::new(image);
        let (effective, effective_layer2) =
            staged_controller.effective_save_options(options, Some(layer2_options), false);
        staged_controller.save_private_project(&mut project, effective, effective_layer2, None)?;

        let level = staged_controller.assets.level.number;
        save_mwl_entrances(
            &mut project,
            source,
            level,
            staged_controller.checksum_field,
        )?;
        save_mwl_secondary_exits(
            &mut project,
            source,
            level,
            staged_controller.checksum_field,
        )?;

        let mutation = RomMutation::between(
            staged_controller.layout.level.mapper,
            &before,
            project.rom.logical_bytes(),
        )
        .map_err(NativeLevelAssetsControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: staged_controller.revision,
            description: format!("Import MWL level {level:03X}"),
            mutation,
        })
    }

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
        if let Some(features) = self.features {
            project
                .save_installed_exanimation_features(
                    self.assets.level.number,
                    features.options,
                    self.feature_installation,
                    self.checksum_field,
                )
                .map_err(NativeLevelAssetsControllerError::ExAnimationFeatures)?;
        }
        if let (Some(fields), Some(layout)) = (self.lfix3_fields, self.lfix3_layout) {
            project
                .save_lfix3_level_fields(
                    self.assets.level.number,
                    fields,
                    layout,
                    self.checksum_field,
                )
                .map_err(NativeLevelAssetsControllerError::Lfix3Fields)?;
        }
        Ok(())
    }
}

fn save_mwl_entrances(
    project: &mut Project,
    source: &lm_project::MwlNativeLevel,
    level: usize,
    checksum_field: usize,
) -> Result<(), NativeLevelAssetsControllerError> {
    let main = source.header.main_entrance();
    project
        .save_vanilla_main_entrance(
            level,
            VanillaMainEntrance {
                position: main.position,
                vertical_settings: main.vertical_settings,
                screen_and_method: main.screen_and_method,
                level_mode_and_screen: main.level_mode_and_screen,
            },
            lm_profile::smw_us_v1_vanilla_entrance_layout(),
            checksum_field,
        )
        .map_err(NativeLevelAssetsControllerError::MwlVanillaEntrance)?;
    project
        .save_lfix3_level_fields(
            level,
            Lfix3LevelFields {
                flags: main.flags,
                high_position: main.high_position,
                additional_flags: main.additional_flags,
                runtime_flags: source.header.0[17],
            },
            lm_profile::smw_us_v1_lfix3_level_fields_layout(),
            checksum_field,
        )
        .map_err(NativeLevelAssetsControllerError::MwlLfix3Fields)?;
    project
        .save_expanded_level_mode(
            level,
            source.header.0[16],
            lm_profile::smw_us_v1_expanded_level_mode_locator(),
            checksum_field,
        )
        .map_err(NativeLevelAssetsControllerError::MwlExpandedLevelMode)?;

    if main.flags & 0x20 != 0 {
        save_mwl_separate_midway(project, source, level, checksum_field)?;
    }
    Ok(())
}

fn save_mwl_separate_midway(
    project: &mut Project,
    source: &lm_project::MwlNativeLevel,
    level: usize,
    checksum_field: usize,
) -> Result<(), NativeLevelAssetsControllerError> {
    let locator = lm_profile::smw_us_v1_separate_midway_locator();
    let mut table = project
        .load_separate_midway_table(locator)
        .map_err(NativeLevelAssetsControllerError::MwlSeparateMidway)?
        .table;
    let midway = source.header.midway_entrance();
    table.entries[level] = lm_level::SeparateMidwayEntrance {
        flags: midway.flags,
        position: midway.position,
        additional_flags: midway.additional_flags,
        high_position: midway.high_position,
    };
    project
        .save_separate_midway_table(&table, locator, checksum_field)
        .map(|_| ())
        .map_err(NativeLevelAssetsControllerError::MwlSeparateMidway)
}

fn save_mwl_secondary_exits(
    project: &mut Project,
    source: &lm_project::MwlNativeLevel,
    level: usize,
    checksum_field: usize,
) -> Result<(), NativeLevelAssetsControllerError> {
    let locator = lm_profile::smw_us_v1_secondary_exit_locator();
    let loaded = project
        .load_secondary_exit_table_detected(locator)
        .map_err(NativeLevelAssetsControllerError::MwlSecondaryExits)?;
    if !matches!(loaded.storage, SecondaryExitStorage::Installed { .. }) {
        return Err(NativeLevelAssetsControllerError::MwlSecondaryExits(
            lm_project::SecondaryExitPatchError::InstallationRequired,
        ));
    }
    let mut table = loaded.table;
    for exit in &mut table.entries {
        if usize::from(exit.destination_level) == level {
            *exit = lm_level::SecondaryExit::default();
        }
    }
    overlay_mwl_secondary_exits(&mut table.entries, &source.secondary_exits);
    let allocation =
        lm_profile::smw_us_v1_secondary_exit_allocation_policy(project.rom.logical_len());
    project
        .save_installed_secondary_exit_table(&table, locator, &allocation, checksum_field, 0xff)
        .map(|_| ())
        .map_err(NativeLevelAssetsControllerError::MwlSecondaryExits)
}

fn overlay_mwl_secondary_exits(
    entries: &mut [lm_level::SecondaryExit],
    records: &[lm_level::MwlSecondaryExit],
) {
    for record in records
        .iter()
        .take(lm_level::SecondaryExitTable::ENTRY_COUNT)
    {
        let index = usize::from(record.index);
        if let Some(entry) = entries.get_mut(index) {
            *entry = record.exit;
        }
    }
}

#[cfg(test)]
mod secondary_exit_import_tests {
    use super::overlay_mwl_secondary_exits;
    use lm_level::{MwlSecondaryExit, SecondaryExit, SecondaryExitTable};

    #[test]
    fn binary_mwl_secondary_exit_overlay_skips_bad_keys_and_uses_last_duplicate() {
        let mut entries = vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
        let first = SecondaryExit {
            position_and_method: 1,
            ..SecondaryExit::default()
        };
        let last = SecondaryExit {
            position_and_method: 2,
            ..SecondaryExit::default()
        };
        overlay_mwl_secondary_exits(
            &mut entries,
            &[
                MwlSecondaryExit {
                    index: 7,
                    exit: first,
                    reserved: 0xaa,
                },
                MwlSecondaryExit {
                    index: 0x2000,
                    exit: SecondaryExit {
                        position_and_method: 3,
                        ..SecondaryExit::default()
                    },
                    reserved: 0xbb,
                },
                MwlSecondaryExit {
                    index: 7,
                    exit: last,
                    reserved: 0xcc,
                },
            ],
        );
        assert_eq!(entries[7], last);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| **entry != SecondaryExit::default())
                .count(),
            1
        );
    }

    #[test]
    fn binary_mwl_secondary_exit_overlay_obeys_original_record_count_cap() {
        let mut entries = vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
        let mut records = vec![MwlSecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
        records.push(MwlSecondaryExit {
            index: 9,
            exit: SecondaryExit {
                position_and_method: 1,
                ..SecondaryExit::default()
            },
            reserved: 0,
        });
        overlay_mwl_secondary_exits(&mut entries, &records);
        assert_eq!(entries[9], SecondaryExit::default());
    }
}
