use super::{Command, RomOverworldEditor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::{CompleteOverworldSaveOptions, Project, RomMutation};
use lm_rats::{AllocationPolicy, ProtectedRange};

impl RomOverworldEditor {
    pub(super) fn prepare_animation_runtime_install(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        if workspace.controller.is_modified()
            || workspace.native_sprites.is_modified()
            || workspace.assets.animation_options != workspace.baseline_animation_options
        {
            return Err(
                "commit or discard staged overworld edits before installing the animation runtime"
                    .into(),
            );
        }
        let mapper = workspace.profiled.snapshot.identity.mapper;
        let mapper_runtime = lm_profile::smw_us_v1_expanded_exanimation_uses_mapper_runtime(
            workspace.image.logical_bytes(),
            mapper,
        )
        .map_err(|error| error.to_string())?;
        match lm_profile::detect_smw_us_v1_overworld_animation_runtime_for_mapper(
            workspace.image.logical_bytes(),
            mapper,
            mapper_runtime,
        )
        .map_err(|error| error.to_string())?
        {
            Some(_) => {
                return Err("the authenticated overworld animation runtime is installed".into());
            }
            None => {}
        }
        let before = workspace.image.logical_bytes().to_vec();
        let mut project = Project::new(workspace.image.clone());
        let allocation = self.overworld_allocation_policy(workspace)?;
        project
            .install_relocatable_patch(
                &lm_profile::smw_us_v1_overworld_animation_runtime_installation_plan_for_mapper(
                    mapper,
                    allocation,
                    mapper_runtime,
                )
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        Ok(lm_app::PreparedRomCommit {
            expected_revision: workspace.controller.revision(),
            description: "Install SMW US v1 overworld animation runtime".into(),
            mutation: RomMutation::between(
                workspace.profiled.snapshot.identity.mapper,
                &before,
                project.rom.logical_bytes(),
            )
            .map_err(|error| error.to_string())?,
        }
        .into_command())
    }

    pub(super) fn prepare_main_layer2_commit(&self) -> Result<Command, String> {
        let workspace = self
            .main_layer2_workspace
            .as_ref()
            .ok_or("playable Layer 2 workspace is closed")?;
        let range = parse_search_range(&self.search_start, &self.search_end)?;
        let mut prepared = workspace
            .controller
            .prepare_commit(
                "Edit playable SMW main-overworld Layer 2",
                AllocationPolicy {
                    search: range,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff, 0x00],
                    protected: vec![
                        ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD + 2,
                        ),
                        ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK + 1,
                        ),
                        ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD + 2,
                        ),
                        ProtectedRange(
                            lm_profile::SMW_US_V1_CHECKSUM_FIELD
                                ..lm_profile::SMW_US_V1_CHECKSUM_FIELD + 4,
                        ),
                    ],
                },
            )
            .map_err(|error| error.to_string())?;
        let layer1_modified = workspace.layer1 != workspace.baseline_layer1;
        let paths_modified = workspace.paths != workspace.original_paths;
        if layer1_modified || paths_modified {
            let before = workspace.image.logical_bytes().to_vec();
            let mut project = Project::new(workspace.image.clone());
            project
                .apply_mutation("stage playable Layer 2", &prepared.mutation)
                .map_err(|error| error.to_string())?;
            if layer1_modified {
                let mut encoded = vec![0_u8; 0x800];
                for plane in 0..2 {
                    for y in 0..32 {
                        for x in 0..32 {
                            let screen = (y / 16) * 2 + x / 16;
                            let within = (y % 16) * 16 + x % 16;
                            encoded[plane * 0x400 + screen * 0x100 + within] =
                                u8::try_from(workspace.layer1.tiles[y * 64 + plane * 32 + x])
                                    .map_err(|_| "vanilla overworld Layer 1 tile exceeds $FF")?;
                        }
                    }
                }
                project
                    .rom
                    .write(0x06_77df, &encoded)
                    .map_err(|error| error.to_string())?;
            }
            if paths_modified {
                project
                    .save_overworld_path_links(
                        &workspace.paths,
                        lm_profile::smw_us_v1_overworld_path_link_layout(),
                        lm_profile::SMW_US_V1_CHECKSUM_FIELD,
                    )
                    .map_err(|error| error.to_string())?;
            }
            project
                .rom
                .update_snes_checksum(lm_profile::SMW_US_V1_CHECKSUM_FIELD)
                .map_err(|error| error.to_string())?;
            prepared.description = "Edit composed playable SMW overworld and routes".into();
            prepared.mutation =
                RomMutation::between(lm_rom::Mapper::LoRom, &before, project.rom.logical_bytes())
                    .map_err(|error| error.to_string())?;
        }
        Ok(prepared.into_command())
    }

    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let prepared = if workspace.controller.is_modified() {
            let options = self.save_options(workspace)?;
            workspace
                .controller
                .prepare_commit(
                    format!("Edit complete native overworld slot {:03X}", workspace.slot),
                    &options,
                )
                .map_err(|error| error.to_string())?
        } else {
            lm_app::PreparedRomCommit {
                expected_revision: workspace.controller.revision(),
                description: format!(
                    "Edit overworld animation options for slot {:03X}",
                    workspace.slot
                ),
                mutation: RomMutation::unchanged(
                    workspace.profiled.snapshot.identity.mapper,
                    workspace.image.logical_bytes().len(),
                ),
            }
        };
        self.finish_combined_commit(workspace, prepared)
    }

    pub(super) fn prepare_commit_owned(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        let prepared = workspace
            .controller
            .prepare_commit_with_reclamation(
                format!(
                    "Edit and reclaim complete native overworld slot {:03X}",
                    workspace.slot
                ),
                &options,
                manifest,
            )
            .map_err(|error| error.to_string())?;
        self.finish_combined_commit(workspace, prepared)
    }

    fn finish_combined_commit(
        &self,
        workspace: &Workspace,
        prepared: lm_app::PreparedRomCommit,
    ) -> Result<Command, String> {
        let prepared = self.merge_animation_option_commit(workspace, prepared)?;
        let prepared = if workspace.native_sprites.is_modified() {
            workspace
                .native_sprites
                .merge_into_commit(prepared, &self.native_sprite_save_options(workspace)?)
                .map_err(|error| error.to_string())?
        } else {
            prepared
        };
        Ok(prepared.into_command())
    }

    fn merge_animation_option_commit(
        &self,
        workspace: &Workspace,
        mut prepared: lm_app::PreparedRomCommit,
    ) -> Result<lm_app::PreparedRomCommit, String> {
        if workspace.assets.animation_options == workspace.baseline_animation_options {
            return Ok(prepared);
        }
        merge_animation_option_mutation(
            &workspace.image,
            workspace.profiled.snapshot.identity.mapper,
            workspace.profiled.snapshot.identity.internal_header_offset + 0x1c,
            workspace.slot,
            workspace.assets.animation_options,
            workspace.assets.animation_lightning_unused_low_bit,
            &mut prepared,
        )
        .map_err(|error| error.to_string())?;
        Ok(prepared)
    }

    fn save_options(&self, workspace: &Workspace) -> Result<CompleteOverworldSaveOptions, String> {
        self.overworld_allocation_policy(workspace)
            .map(CompleteOverworldSaveOptions::uniform_allocation)
    }

    fn native_sprite_save_options(
        &self,
        workspace: &Workspace,
    ) -> Result<lm_project::NativeCustomOverworldSpriteSaveOptions, String> {
        let mut allocation = self.overworld_allocation_policy(workspace)?;
        if let Some(block) = workspace.native_sprites.owned_block() {
            let owned = ProtectedRange(block.full_range());
            allocation.protected.retain(|range| range != &owned);
        }
        Ok(lm_project::NativeCustomOverworldSpriteSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        })
    }

    fn overworld_allocation_policy(
        &self,
        workspace: &Workspace,
    ) -> Result<AllocationPolicy, String> {
        let range = parse_search_range(&self.search_start, &self.search_end)?;
        let mut allocation = workspace
            .profiled
            .profile
            .allocation_policy_for_rom(
                range,
                &workspace.image,
                workspace.profiled.snapshot.identity.internal_header_offset,
            )
            .map_err(|error| error.to_string())?;
        for protected in [
            ProtectedRange(
                workspace.native_sprite_layout.stream.pointer_offset
                    ..workspace.native_sprite_layout.stream.pointer_offset + 3,
            ),
            ProtectedRange(
                workspace.native_sprite_layout.record_size_pointer_offset
                    ..workspace.native_sprite_layout.record_size_pointer_offset + 4,
            ),
        ] {
            if !allocation.protected.contains(&protected) {
                allocation.protected.push(protected);
            }
        }
        if let Some(block) = &workspace.native_sprite_layout.record_size_block {
            let protected = ProtectedRange(block.full_range());
            if !allocation.protected.contains(&protected) {
                allocation.protected.push(protected);
            }
        }
        if let Some(block) = workspace.native_sprites.owned_block() {
            let protected = ProtectedRange(block.full_range());
            if !allocation.protected.contains(&protected) {
                allocation.protected.push(protected);
            }
        }
        Ok(allocation)
    }
}

fn merge_animation_option_mutation(
    image: &lm_rom::RomImage,
    mapper: lm_rom::Mapper,
    checksum_field: usize,
    slot: u16,
    options: [crate::overworld_editor_render::OverworldAnimationOptions; 7],
    lightning_unused_low_bit: bool,
    prepared: &mut lm_app::PreparedRomCommit,
) -> Result<(), Box<dyn std::error::Error>> {
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image.clone());
    project.apply_mutation("stage complete overworld payloads", &prepared.mutation)?;
    let (features, lightning) = crate::overworld_editor_render::encode_overworld_animation_options(
        options,
        lightning_unused_low_bit,
    );
    project.save_installed_overworld_animation_options(
        features,
        lightning,
        lm_profile::smw_us_v1_overworld_animation_options_layout_for_mapper(mapper),
        checksum_field,
    )?;
    prepared.description =
        format!("Edit complete native overworld slot {slot:03X} and animation options");
    prepared.mutation = RomMutation::between(mapper, &before, project.rom.logical_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{Mapper, RomImage, compute_snes_checksum, pc_to_snes};

    fn write_pointer(image: &mut RomImage, operand: usize, target: usize) {
        let address = pc_to_snes(Mapper::LoRom, target).unwrap();
        image.write(operand, &address.to_le_bytes()[..3]).unwrap();
    }

    #[test]
    fn map_options_merge_with_payload_edit_and_reopen_from_one_mutation() {
        const RUNTIME: usize = 0x30000;
        const TABLE: usize = 0x31000;
        const CHECKSUM: usize = 0x7fdc;
        let mut image = RomImage::from_bytes(vec![0xff; 0x40000]).unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_MARKER,
                &[0x22],
            )
            .unwrap();
        write_pointer(
            &mut image,
            lm_profile::SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_OPERAND,
            RUNTIME,
        );
        write_pointer(
            &mut image,
            RUNTIME
                + usize::try_from(
                    lm_profile::SMW_US_V1_OVERWORLD_ANIMATION_FEATURE_OPERAND_DISPLACEMENT,
                )
                .unwrap(),
            TABLE,
        );
        image.write(TABLE, &[0; 7]).unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_OVERWORLD_LIGHTNING_DISABLE_MASK,
                &[0xf7],
            )
            .unwrap();
        let before = image.logical_bytes().to_vec();
        let mut options =
            crate::overworld_editor_render::decode_overworld_animation_options([0; 7], 0xf7);
        options[2]
            .features
            .set_enabled(lm_graphics::ExAnimationFeature::GlobalExAnimation, false);
        options[2].original_lightning = true;
        let mut prepared = lm_app::PreparedRomCommit {
            expected_revision: 9,
            description: "payload edit".into(),
            mutation: RomMutation {
                mapper: Mapper::LoRom,
                expected_len: before.len(),
                appended: Vec::new(),
                writes: vec![lm_project::RomWrite {
                    offset: 0x1234,
                    bytes: vec![0x5a],
                }],
            },
        };
        merge_animation_option_mutation(
            &image,
            Mapper::LoRom,
            CHECKSUM,
            0x101,
            options,
            true,
            &mut prepared,
        )
        .unwrap();

        let mut reopened = Project::new(image);
        reopened
            .apply_mutation("combined commit", &prepared.mutation)
            .unwrap();
        assert_eq!(reopened.rom.read(0x1234, 1).unwrap(), [0x5a]);
        let loaded = reopened
            .load_installed_overworld_animation_options(
                lm_profile::smw_us_v1_overworld_animation_options_layout(),
            )
            .unwrap();
        assert_eq!(loaded.feature_bytes[2], 0x20);
        assert_eq!(loaded.lightning_disable_mask, 0xd7);
        let checksum = compute_snes_checksum(reopened.rom.logical_bytes(), CHECKSUM).unwrap();
        assert_eq!(reopened.rom.read(CHECKSUM, 4).unwrap(), checksum.encoded());
        assert_eq!(prepared.expected_revision, 9);
    }
}
