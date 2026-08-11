use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::OverworldWarpLinkTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_warp_installation_plan,
    smw_us_v1_overworld_warp_link_layout, smw_us_v1_overworld_warp_patch_locator,
    smw_us_v1_overworld_warp_runtime_template, smw_us_v1_overworld_warp_update_policy,
};
use lm_project::{OverworldWarpLinkStorage, OverworldWarpPatchMigrationOptions};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_warp_links(
        &mut self,
        expected_revision: u64,
        table: &OverworldWarpLinkTable,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        if !replace_native_warp_links_in_project(project, table)? {
            return Ok(Vec::new());
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld warp links".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

pub(crate) fn replace_native_warp_links_in_project(
    project: &mut lm_project::Project,
    table: &OverworldWarpLinkTable,
) -> Result<bool, AppError> {
    let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err(AppError::NativeOverworldWarpIdentityMismatch);
    }
    let loaded =
        project.load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?;
    let changed = match loaded.storage {
        OverworldWarpLinkStorage::Fixed if table.links.len() == 27 => project
            .save_overworld_warp_links(
                table,
                smw_us_v1_overworld_warp_link_layout(),
                SMW_US_V1_CHECKSUM_FIELD,
            )?,
        OverworldWarpLinkStorage::Fixed => {
            project
                .install_relocatable_patch(&smw_us_v1_overworld_warp_installation_plan(table)?)?;
            true
        }
        storage @ OverworldWarpLinkStorage::CurrentPatch { .. } => {
            let allocation = smw_us_v1_overworld_warp_update_policy(project.rom.logical_len());
            project.save_installed_overworld_warp_links(
                table,
                storage,
                &allocation,
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )?
        }
        storage @ OverworldWarpLinkStorage::LegacyPatch { .. } => {
            let allocation = smw_us_v1_overworld_warp_update_policy(project.rom.logical_len());
            let runtime = smw_us_v1_overworld_warp_runtime_template();
            project.migrate_legacy_overworld_warp_patch(
                table,
                storage,
                OverworldWarpPatchMigrationOptions {
                    locator: smw_us_v1_overworld_warp_patch_locator(),
                    current_runtime: &runtime,
                    allocation: &allocation,
                    checksum_field: SMW_US_V1_CHECKSUM_FIELD,
                    fill: 0xff,
                },
            )?
        }
    };
    if !changed {
        return Ok(false);
    }
    let reopened =
        project.load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?;
    if reopened.table != *table {
        return Err(AppError::NativeOverworldWarpReopenMismatch);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink};
    use lm_profile::{
        SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET, SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET,
    };
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator};
    use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity, pc_to_snes};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn native_warp_replacement_is_revisioned_and_undoable() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        let before = app.project().unwrap().save_snapshot();
        let mut table = app
            .project()
            .unwrap()
            .load_overworld_warp_links(smw_us_v1_overworld_warp_link_layout())
            .unwrap();
        table.links[0].destination.horizontal_tile ^= 1;
        app.dispatch(Command::ReplaceNativeOverworldWarpLinks {
            rev: 0,
            table: Box::new(table),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), before);
    }

    #[test]
    fn expanded_warp_install_is_one_application_revision_and_undo_step() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes.clone()).unwrap();
        let table = OverworldWarpLinkTable {
            links: (0_u16..30)
                .map(|value| OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: value,
                        horizontal_tile: value + 1,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: value + 2,
                        horizontal_tile: value + 3,
                    },
                })
                .collect(),
        };
        app.dispatch(Command::ReplaceNativeOverworldWarpLinks {
            rev: 0,
            table: Box::new(table.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
                .unwrap()
                .table,
            table
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), bytes);
    }

    #[test]
    fn legacy_warp_migration_is_one_application_revision_and_undo_step() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let old_table = expanded_table(30);
        let legacy = legacy_rom(original, &old_table);
        let mut app = AppState::default();
        app.load_rom(legacy.clone()).unwrap();
        let replacement = expanded_table(35);
        app.dispatch(Command::ReplaceNativeOverworldWarpLinks {
            rev: 0,
            table: Box::new(replacement.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        let loaded = app
            .project()
            .unwrap()
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap();
        assert_eq!(loaded.table, replacement);
        assert!(matches!(
            loaded.storage,
            OverworldWarpLinkStorage::CurrentPatch { .. }
        ));
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), legacy);
    }

    #[test]
    fn authentic_navigation_edits_match_across_copier_header_variants() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc");
        let physical = fs::read(fixture).unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let variants = [physical, physical_image.logical_bytes().to_vec()];
        let mut logical_results = Vec::new();

        for original in variants {
            let original_image = RomImage::from_bytes(original.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            let project = app.project().unwrap();
            let mut paths = project
                .load_overworld_path_links_detected(
                    lm_profile::smw_us_v1_overworld_path_patch_locator(),
                )
                .unwrap()
                .table;
            let original_paths = paths.clone();
            let mut warps = project
                .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
                .unwrap()
                .table;
            let original_warps = warps.clone();
            assert!(!paths.links.is_empty());
            assert_eq!(warps.links.len(), 27);
            paths.links[0].target.x_tile ^= 1;
            paths.links[0].destination.x ^= 8;
            warps.links[0].destination.horizontal_tile ^= 1;
            warps.links[0].destination.packed_vertical ^= 0x10;

            app.dispatch(Command::ReplaceNativeOverworldPathLinks {
                rev: 0,
                table: Box::new(paths.clone()),
            })
            .unwrap();
            app.dispatch(Command::ReplaceNativeOverworldWarpLinks {
                rev: 1,
                table: Box::new(warps.clone()),
            })
            .unwrap();

            let project = app.project().unwrap();
            assert_eq!(
                project
                    .load_overworld_path_links_detected(
                        lm_profile::smw_us_v1_overworld_path_patch_locator(),
                    )
                    .unwrap()
                    .table,
                paths
            );
            assert_eq!(
                project
                    .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator(),)
                    .unwrap()
                    .table,
                warps
            );
            let result = RomImage::from_bytes(project.save_snapshot()).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&result).unwrap().checksum_matches());
            logical_results.push(result.logical_bytes().to_vec());

            app.dispatch(Command::Undo).unwrap();
            assert_eq!(
                app.project()
                    .unwrap()
                    .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator(),)
                    .unwrap()
                    .table,
                original_warps
            );
            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original);
            assert_eq!(
                app.project()
                    .unwrap()
                    .load_overworld_path_links_detected(
                        lm_profile::smw_us_v1_overworld_path_patch_locator(),
                    )
                    .unwrap()
                    .table,
                original_paths
            );
            app.dispatch(Command::Redo).unwrap();
            app.dispatch(Command::Redo).unwrap();
            assert_eq!(
                app.project().unwrap().rom.logical_bytes(),
                logical_results.last().unwrap()
            );
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }

    fn expanded_table(count: u16) -> OverworldWarpLinkTable {
        OverworldWarpLinkTable {
            links: (0..count)
                .map(|value| OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: value,
                        horizontal_tile: value + 1,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: value + 2,
                        horizontal_tile: value + 3,
                    },
                })
                .collect(),
        }
    }

    fn legacy_rom(original: Vec<u8>, table: &OverworldWarpLinkTable) -> Vec<u8> {
        let mut image = RomImage::from_bytes(original).unwrap();
        image.expand(Mapper::LoRom, 0x90_000, 0xff).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        let policy = AllocationPolicy::lorom(0x80_000..0x90_000);
        let runtime = FreeSpaceAllocator::new(&mut bytes, policy.clone())
            .allocate(&[0xff; 0x80])
            .unwrap();
        let planes = table.encode_planes().unwrap();
        let plane_len = planes.source_vertical.len();
        let mut payload = planes.source_vertical;
        payload.extend_from_slice(&planes.source_horizontal);
        payload.extend_from_slice(&planes.destination_vertical);
        payload.extend_from_slice(&planes.destination_horizontal);
        let data = FreeSpaceAllocator::new(&mut bytes, policy)
            .allocate(&payload)
            .unwrap();
        let patch = runtime.payload.start;
        bytes[patch + 0x10] = u8::try_from(table.links.len()).unwrap();
        for (operand, addend) in
            [0x14, 0x24, 0x47, 0x59]
                .into_iter()
                .zip([0, plane_len, plane_len * 2, plane_len * 3])
        {
            bytes[patch + operand..patch + operand + 3].copy_from_slice(
                &pc_to_snes(Mapper::LoRom, data.payload.start + addend)
                    .unwrap()
                    .to_le_bytes()[..3],
            );
        }
        let entry = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
        let return_target = pc_to_snes(Mapper::LoRom, patch + 0x40)
            .unwrap()
            .to_le_bytes();
        bytes[SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET
            ..SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET + 5]
            .copy_from_slice(&[0x22, entry[0], entry[1], entry[2], 0x60]);
        bytes[SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET
            ..SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET + 4]
            .copy_from_slice(&[0x22, return_target[0], return_target[1], return_target[2]]);
        let checksum = compute_snes_checksum(&bytes, SMW_US_V1_CHECKSUM_FIELD).unwrap();
        bytes[SMW_US_V1_CHECKSUM_FIELD..SMW_US_V1_CHECKSUM_FIELD + 4]
            .copy_from_slice(&checksum.encoded());
        bytes
    }
}
