use crate::{AppError, AppState, FrontendEffect};
use lm_profile::RevisionPatchTemplate;
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, InstalledExAnimationRomLayout,
    LegacyExAnimationMigrationLayout, LegacyExAnimationRomLayout, LevelPointerTable,
    LoadedLegacyExAnimationSlot, RomMutation,
};
use lm_rats::ProtectedRange;
use lm_rom::{Mapper, Region, SnesPointer24, SupportedGame};
use std::ops::Range;

impl AppState {
    pub(crate) fn install_lz2_speed_runtime(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::GraphicsCompressionRuntimeIdentityMismatch);
        }
        if !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(&project.rom) {
            return Err(AppError::GraphicsCompressionRuntimeUnavailable);
        }
        let source_mode = lm_profile::detect_smw_us_v1_graphics_compression_mode(&project.rom)?;
        let logical_len = project.rom.logical_len();
        let checksum_field = identity.internal_header_offset + 0x1c;
        let allocation_range = match source_mode {
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original => 0x80_000..logical_len,
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz3 => 0x80_000..logical_len,
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed => {
                return Err(AppError::GraphicsCompressionRuntimeUnavailable);
            }
        };
        let mut allocation = lm_rats::AllocationPolicy::lorom(allocation_range);
        allocation.protected.extend([
            lm_rats::ProtectedRange(
                lm_profile::SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET
                    ..lm_profile::SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET + 5,
            ),
            lm_rats::ProtectedRange(
                lm_profile::SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET
                    ..lm_profile::SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET + 1,
            ),
            lm_rats::ProtectedRange(checksum_field..checksum_field + 4),
        ]);
        let plan = match source_mode {
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original => {
                Some(lm_profile::smw_us_v1_lz2_speed_installation_plan(
                    &project.rom,
                    allocation,
                    checksum_field,
                )?)
            }
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz3 => None,
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed => unreachable!(),
        };
        if source_mode == lm_profile::SmwUsV1GraphicsCompressionMode::Lz3 {
            let replacement = lm_profile::smw_us_v1_compact_graphics_compression_migration_plan(
                &project.rom,
                checksum_field,
                lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed,
            )?;
            project.replace_relocatable_patch_with_kind(
                &replacement.plan,
                &replacement.obsolete,
                0xff,
                lm_project::EditKind::GraphicsCompressionMigration {
                    source: lm_project::GraphicsCompression::Lz3,
                    target: lm_project::GraphicsCompression::Lz2,
                },
            )?;
            if let Some(profile) = self.revision_profile.as_mut() {
                profile.graphics.compression = lm_project::GraphicsCompression::Lz2;
            }
        } else {
            project.install_relocatable_patch(
                plan.as_ref()
                    .expect("LZ2 Orig selects the runtime-only installation plan"),
            )?;
        }
        self.advance_project_revision()?;
        let description = "Install SMW US LZ2 Speed graphics runtime".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install(
        &mut self,
        expected_revision: u64,
        layer3: bool,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if layer3 {
            self.install_layer3(expected_revision)
        } else {
            self.install_settings(expected_revision)
        }
    }

    pub(crate) fn install_layer3(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Layer3IdentityMismatch);
        }
        let settings_installed = lm_profile::load_smw_us_v1_overworld_settings(project)
            .map_err(|error| AppError::NativeOverworldSettingsStorage(error.to_string()))?
            .installed;
        let plans = if settings_installed {
            vec![
                lm_profile::smw_us_v1_complete_layer3_installation_plan()
                    .map_err(lm_profile::CompleteLayer3BuildError::Runtime)?,
            ]
        } else {
            lm_profile::smw_us_v1_complete_layer3_feature_plans()?
        };
        project
            .install_relocatable_patch_group("install complete SMW US Layer 3 feature", &plans)?;
        self.advance_project_revision()?;
        let description = "Install complete SMW US Layer 3 runtime".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_settings(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || !matches!(identity.mapper, Mapper::LoRom | Mapper::Sa1)
        {
            return Err(AppError::ExpandedSettingsIdentityMismatch);
        }
        if identity.mapper == Mapper::Sa1 {
            let ram_remap = project
                .load_lunar_magic_rom_metadata(
                    lm_profile::smw_us_v1_lunar_magic_metadata_layout_for_mapper(Mapper::Sa1),
                )?
                .is_some_and(|metadata| metadata.sa1_ram_remap());
            let plan =
                lm_profile::smw_us_v1_sa1_expanded_settings_installation_plan_with_ram_remap(
                    ram_remap,
                )?;
            project.install_relocatable_patch(&plan)?;
        } else {
            let plan =
                lm_profile::smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom)?;
            project.install_relocatable_patch_with_expansion_retry(
                &plan,
                lm_profile::SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN,
            )?;
        }
        self.advance_project_revision()?;
        let description = "Install SMW US expanded level settings".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_lfix3(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Lfix3IdentityMismatch);
        }
        let description =
            match lm_profile::probe_smw_us_v1_lfix3_generation(project.rom.logical_bytes())? {
                lm_profile::SmwUsV1Lfix3Generation::Absent => {
                    let plan = lm_profile::smw_us_v1_builtin_lfix3_installation_plan()?;
                    project.install_relocatable_patch(&plan)?;
                    "Install SMW US Lfix3 core runtime"
                }
                lm_profile::SmwUsV1Lfix3Generation::Generation3Current => {
                    return Err(AppError::Lfix3AlreadyInstalled);
                }
                lm_profile::SmwUsV1Lfix3Generation::Generation1 => {
                    let plan = lm_profile::smw_us_v1_generation_1_lfix3_migration(
                        project.rom.logical_bytes(),
                    )?;
                    project.install_relocatable_patch(&plan)?;
                    "Migrate SMW US Lfix3 generation 1"
                }
                lm_profile::SmwUsV1Lfix3Generation::Generation2 => {
                    let migration = lm_profile::smw_us_v1_generation_2_lfix3_migration(
                        project.rom.logical_bytes(),
                    )?;
                    project.replace_relocatable_patch(
                        &migration.plan,
                        &lm_project::RatsOwnershipManifest {
                            owned: vec![migration.previous_runtime],
                            retained: Vec::new(),
                        },
                        0xff,
                    )?;
                    "Migrate SMW US Lfix3 generation 2"
                }
            };
        self.advance_project_revision()?;
        let description = description.to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_map16_runtime(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Map16RuntimeIdentityMismatch);
        }
        let description = match lm_profile::probe_smw_us_v1_map16_runtime_generation(
            project.rom.logical_bytes(),
        )? {
            lm_profile::SmwUsV1Map16RuntimeGeneration::Absent => {
                let plan = lm_profile::smw_us_v1_builtin_map16_runtime_installation_plan(
                    project.rom.logical_bytes(),
                )?;
                project.install_relocatable_patch(&plan)?;
                "Install SMW US Map16 runtime"
            }
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageOneLegacy
            | lm_profile::SmwUsV1Map16RuntimeGeneration::StageTwoLegacy
            | lm_profile::SmwUsV1Map16RuntimeGeneration::StageThreeLegacy => {
                let plan = lm_profile::smw_us_v1_legacy_map16_runtime_migration(
                    project.rom.logical_bytes(),
                )?;
                project.install_relocatable_patch(&plan)?;
                "Migrate legacy SMW US Map16 runtime"
            }
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageFourCurrent => {
                return Err(AppError::Map16RuntimeAlreadyInstalled);
            }
        };
        self.advance_project_revision()?;
        let description = description.to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_expanded_exanimation_runtime(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
        {
            return Err(AppError::ExpandedExAnimationRuntimeIdentityMismatch);
        }
        let mapper = identity.mapper;
        let mapper_runtime = lm_profile::smw_us_v1_expanded_exanimation_uses_mapper_runtime(
            project.rom.logical_bytes(),
            mapper,
        )?;
        let description =
            match lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(
                project.rom.logical_bytes(),
                mapper,
                mapper_runtime,
            )? {
                lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Absent => {
                    let plan = if mapper == Mapper::LoRom {
                        lm_profile::smw_us_v1_expanded_exanimation_runtime_installation_plan()?
                    } else {
                        let search = if mapper == Mapper::ExLoRom {
                            0x10_0000..0x40_0000
                        } else {
                            0x40_0000..project.rom.logical_len()
                        };
                        lm_profile::smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
                            mapper,
                            lm_rats::AllocationPolicy::lorom(search),
                            mapper_runtime,
                        )?
                    };
                    project.install_relocatable_patch(&plan)?;
                    "Install SMW US expanded ExAnimation runtime"
                }
                lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyPointerHooks => {
                    if mapper != Mapper::LoRom {
                        return Err(AppError::ExpandedExAnimationRuntimeIdentityMismatch);
                    }
                    let migration = lm_profile::smw_us_v1_legacy_exanimation_hook_migration(
                        project.rom.logical_bytes(),
                    )?;
                    project.install_relocatable_patch(&migration.plan)?;
                    "Migrate legacy SMW US ExAnimation pointer hooks"
                }
                lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable => {
                    if mapper != Mapper::LoRom {
                        return Err(AppError::ExpandedExAnimationRuntimeIdentityMismatch);
                    }
                    migrate_legacy_global_exanimations(project)?;
                    "Migrate legacy SMW US global ExAnimation table"
                }
                lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current => {
                    return Err(AppError::ExpandedExAnimationRuntimeAlreadyInstalled);
                }
            };
        self.advance_project_revision()?;
        let description = description.to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_layer2_runtime(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Layer2RuntimeIdentityMismatch);
        }
        let generation = lm_profile::probe_smw_us_v1_layer2_runtime_generation(&project.rom)?;
        let (plan, description) = match generation {
            lm_profile::SmwUsV1Layer2RuntimeGeneration::Format100Legacy => (
                lm_profile::smw_us_v1_layer2_format_100_migration(project.rom.logical_bytes())?,
                "Migrate SMW US Layer 2 runtime format $100 to $103",
            ),
            lm_profile::SmwUsV1Layer2RuntimeGeneration::Format101Legacy => (
                lm_profile::smw_us_v1_layer2_format_101_migration(project.rom.logical_bytes())?,
                "Migrate SMW US Layer 2 runtime format $101 to $103",
            ),
            lm_profile::SmwUsV1Layer2RuntimeGeneration::Format102Legacy => (
                lm_profile::smw_us_v1_layer2_format_102_migration(project.rom.logical_bytes())?,
                "Migrate SMW US Layer 2 runtime format $102 to $103",
            ),
            lm_profile::SmwUsV1Layer2RuntimeGeneration::Format103Current => {
                return Err(AppError::Layer2RuntimeAlreadyInstalled);
            }
            absent @ lm_profile::SmwUsV1Layer2RuntimeGeneration::Absent => {
                return Err(AppError::Layer2RuntimeLegacyMigrationRequired(absent));
            }
        };
        project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = description.to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_sprite19_fix(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Sprite19FixIdentityMismatch);
        }
        if lm_profile::detect_smw_us_v1_sprite19_fix(project.rom.logical_bytes())?
            == lm_profile::SmwUsV1Sprite19FixState::Installed
        {
            return Err(AppError::Sprite19FixAlreadyInstalled);
        }
        let plan =
            lm_profile::smw_us_v1_sprite19_fix_installation_plan(project.rom.logical_bytes())?;
        project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = "Install SMW US sprite 19 ASM fix".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_support_patch_b(
        &mut self,
        expected_revision: u64,
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
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::SupportPatchBIdentityMismatch);
        }
        if lm_profile::detect_smw_us_v1_support_patch_b(project.rom.logical_bytes())?
            == lm_profile::SmwUsV1SupportPatchBState::Installed
        {
            return Err(AppError::SupportPatchBAlreadyInstalled);
        }
        let plan =
            lm_profile::smw_us_v1_support_patch_b_installation_plan(project.rom.logical_bytes())?;
        project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = "Install SMW US level support patch B".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_revision_patch(
        &mut self,
        expected_revision: u64,
        template: &RevisionPatchTemplate,
        search: Range<usize>,
        fill: u8,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let profile = self
            .revision_profile
            .as_ref()
            .ok_or(AppError::NoRevisionProfile)?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        let plan = template.installation_plan(
            profile,
            &project.rom,
            search,
            identity.internal_header_offset,
            identity.internal_header_offset + 0x1c,
            fill,
        )?;
        let result = project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = format!(
            "Install revision patch {} ({} payloads)",
            template.name,
            result.blocks.len()
        );
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

pub(crate) fn migrate_legacy_global_exanimations(
    project: &mut lm_project::Project,
) -> Result<(), AppError> {
    let detected = lm_profile::detect_smw_us_v1_legacy_global_exanimation_runtime(
        project.rom.logical_bytes(),
    )?;
    let legacy = LegacyExAnimationRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: detected.pointer_table.start,
            entries: 0x200,
            stride: 3,
        },
    };
    let loaded = project.load_all_legacy_exanimations(legacy)?;
    let mut protected = vec![
        ProtectedRange(detected.pointer_table.clone()),
        ProtectedRange(detected.auxiliary_table.clone()),
    ];
    for slot in &loaded {
        if let LoadedLegacyExAnimationSlot::Present {
            payload_offset,
            record_count,
            ..
        } = slot
        {
            let end = payload_offset.checked_add(1 + record_count * 0x23).ok_or(
                lm_project::LegacyExAnimationIoError::PayloadOffsetOverflow(*payload_offset),
            )?;
            protected.push(ProtectedRange(*payload_offset..end));
        }
    }

    let original = project.rom.logical_bytes().to_vec();
    let mut staged = project.clone();
    let mut install = lm_profile::smw_us_v1_expanded_exanimation_runtime_installation_plan()?;
    for write in &mut install.writes {
        write.expected = original[write.offset..write.offset + write.replacement.len()].to_vec();
    }
    install.allocation.protected.extend(protected);
    staged.install_relocatable_patch(&install)?;

    let current_runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime(
        staged.rom.logical_bytes(),
    )?;
    let operand = &staged.rom.logical_bytes()
        [current_runtime.payload.start + 0xea..current_runtime.payload.start + 0xed];
    let current_pointer_table = SnesPointer24::decode(operand)
        .expect("an exact three-byte runtime operand is a 24-bit pointer")
        .to_pc(Mapper::LoRom)?;
    let current = InstalledExAnimationRomLayout {
        payload: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: current_pointer_table,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 0x40,
            maximum_encoded_len: 0x8000,
        },
        pointer_presence_mask: 0x00ff_0000,
        pointer_locator: None,
    };
    let mut allocation = lm_rats::AllocationPolicy::lorom(
        lm_profile::SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_START..staged.rom.logical_len(),
    );
    allocation.protected.extend(
        lm_rats::scan(staged.rom.logical_bytes())
            .into_iter()
            .map(|block| ProtectedRange(block.full_range())),
    );
    let options = ExAnimationSaveOptions {
        allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let mut double_size_modes = [false; 256];
    double_size_modes[1..=3].fill(true);
    staged.migrate_legacy_exanimations(
        &LegacyExAnimationMigrationLayout {
            legacy,
            current,
            legacy_auxiliary: detected.auxiliary_table,
        },
        &double_size_modes,
        &options,
        lm_profile::SMW_US_V1_CHECKSUM_FIELD,
    )?;
    let mutation = RomMutation::between(Mapper::LoRom, &original, staged.rom.logical_bytes())?;
    project.apply_mutation("migrate legacy SMW US global ExAnimation table", &mutation)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_profile::RevisionProfile;
    use lm_project::{PatchFixup, PatchPayload, PatchWrite, Project};
    use lm_rats::{HEADER_LEN, make_header};
    use lm_rom::{LunarMagicRomMetadata, Mapper, RomImage, pc_to_snes};
    use std::{fs, path::PathBuf};

    fn write_pointer(bytes: &mut [u8], offset: usize, mapper: Mapper, target: usize) {
        bytes[offset..offset + 3]
            .copy_from_slice(&pc_to_snes(mapper, target).unwrap().to_le_bytes()[..3]);
    }

    fn legacy_global_exanimation_fixture() -> Vec<u8> {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(pristine).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        bytes.resize(0x10_0000, 0xff);
        let runtime_header = 0x8_0000;
        let runtime_len = 0x200;
        bytes[runtime_header..runtime_header + HEADER_LEN]
            .copy_from_slice(&make_header(runtime_len).unwrap());
        let runtime = runtime_header + HEADER_LEN;
        bytes[runtime..runtime + runtime_len].fill(0xea);
        let pointer_table = 0x8_2000;
        let auxiliary_table = 0x8_3000;
        let legacy_payload = 0x8_4000;
        bytes[pointer_table..pointer_table + 0x600].fill(0);
        bytes[auxiliary_table..auxiliary_table + 0x140].fill(0x5a);
        bytes[legacy_payload] = 1;
        let record = &mut bytes[legacy_payload + 1..legacy_payload + 1 + 0x23];
        record[0] = 0x21;
        record[1..3].copy_from_slice(&0x4321_u16.to_le_bytes());
        for word in record[3..].chunks_exact_mut(2) {
            word.copy_from_slice(&0x1234_u16.to_le_bytes());
        }
        write_pointer(&mut bytes, 0x283ae, Mapper::LoRom, runtime);
        bytes[0x283ad] = 0x22;
        write_pointer(&mut bytes, runtime + 0x1a, Mapper::LoRom, pointer_table);
        bytes[0x2418] = 0x22;
        write_pointer(&mut bytes, 0x2419, Mapper::LoRom, auxiliary_table);
        write_pointer(
            &mut bytes,
            pointer_table + 7 * 3,
            Mapper::LoRom,
            legacy_payload,
        );
        bytes
    }

    fn mapper_exanimation_fixture(mapper: Mapper, mapper_runtime: bool, headered: bool) -> Vec<u8> {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let mut logical = match mapper {
            Mapper::ExLoRom => {
                let mut project =
                    Project::open_supported(RomImage::from_bytes(pristine).unwrap()).unwrap();
                project.convert_to_64_mbit_exlorom().unwrap();
                project.rom.logical_bytes().to_vec()
            }
            Mapper::Sa1 => {
                let mut image = RomImage::from_bytes(pristine).unwrap();
                image.write(0x7fd5, &[0x23, 0x34]).unwrap();
                image.update_snes_checksum(0x7fdc).unwrap();
                let mut project = Project::open_supported(image).unwrap();
                project.expand_sa1_rom(lm_project::SA1_6_MIB_LEN).unwrap();
                project.rom.logical_bytes().to_vec()
            }
            Mapper::LoRom => unreachable!("mapper fixture covers expanded mapper families"),
        };
        let metadata_base = if mapper == Mapper::ExLoRom {
            0x40_0000
        } else {
            0
        };
        let attribution_offset = metadata_base + lm_profile::SMW_US_V1_LM_ATTRIBUTION_OFFSET;
        logical[attribution_offset
            ..attribution_offset + lm_rom::LunarMagicRomMetadata::ATTRIBUTION_LEN]
            .fill(b' ');
        logical[attribution_offset
            ..attribution_offset + lm_rom::LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(lm_rom::LunarMagicRomMetadata::SIGNATURE);
        logical[metadata_base + lm_profile::SMW_US_V1_LM_VRAM_VERSION_OFFSET] = 1;
        let feature_offset = metadata_base + lm_profile::SMW_US_V1_LM_FEATURE_RECORD_OFFSET;
        logical[feature_offset..feature_offset + lm_rom::LunarMagicRomMetadata::FEATURE_LEN]
            .fill(0);
        let (declaration, enabled) = match mapper {
            Mapper::ExLoRom => (1 << 1, 1 << 17),
            Mapper::Sa1 => (1 << 2, 1 << 18),
            Mapper::LoRom => unreachable!("handled above"),
        };
        let bits: u32 = declaration | if mapper_runtime { enabled } else { 0 };
        logical[feature_offset..feature_offset + 4].copy_from_slice(&bits.to_le_bytes());
        let mut image = RomImage::from_bytes(logical).unwrap();
        image.update_snes_checksum(0x7fdc).unwrap();
        let logical = image.logical_bytes().to_vec();
        if !headered {
            return logical;
        }
        let mapper_byte = match mapper {
            Mapper::ExLoRom => 0x32,
            Mapper::Sa1 => 0x23,
            Mapper::LoRom => unreachable!("handled above"),
        };
        let mut physical =
            lm_profile::lunar_magic_copier_header(logical.len(), mapper_byte).to_vec();
        physical.extend(logical);
        physical
    }

    fn fixture(profile: &RevisionProfile) -> Vec<u8> {
        let mut bytes = vec![0xff; 0x4_0000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = match profile.mapper {
            Mapper::LoRom => 0x20,
            Mapper::ExLoRom => 0x32,
            Mapper::Sa1 => 0x23,
        };
        bytes[0x7fd9] = 1;
        bytes[0x7fdb] = profile.revision;
        for table in [
            profile.level.layer1,
            profile.level.sprites.low_or_contiguous_table(),
            profile.map16.graphics,
            profile.map16.acts_like,
            profile.graphics.pointers,
            profile.palette.pointers,
            profile.exanimation.pointers,
            profile.overworld.layers.layer1,
            profile.overworld.layers.layer2,
            profile.overworld.event_reveals.sources,
            profile.overworld.event_reveals.destinations,
            profile.overworld.endpoints.pointers,
            profile.overworld.messages.pointers,
            profile.overworld.sprites.pointers,
            profile.overworld.palette.pointers,
            profile.overworld.animation.pointers,
        ] {
            for index in 0..table.entries {
                write_pointer(
                    &mut bytes,
                    table.offset + index * table.stride,
                    profile.mapper,
                    0x1_0000,
                );
            }
        }
        bytes
    }

    fn template(profile: &RevisionProfile) -> RevisionPatchTemplate {
        RevisionPatchTemplate {
            name: "test runtime".into(),
            game: profile.game,
            region: profile.region,
            revision: profile.revision,
            mapper: profile.mapper,
            payloads: vec![PatchPayload {
                bytes: vec![0xaa; 8],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 0x80,
                expected: vec![0xff; 4],
                replacement: vec![0x22, 0, 0, 0],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: lm_project::PatchFixupEncoding::Long24,
                }],
            }],
        }
    }

    #[test]
    fn application_install_is_revision_checked_undoable_and_failure_atomic() {
        let profile = lm_profile::test_support::profile();
        let mut app = AppState::default();
        app.load_rom(fixture(&profile)).unwrap();
        app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
            .unwrap();
        let revision = app.controller_snapshot().unwrap().revision;

        let effects = app
            .dispatch(Command::InstallRevisionPatch {
                expected_revision: revision,
                template: Box::new(template(&profile)),
                search: 0x3_0000..0x4_0000,
                fill: 0xff,
            })
            .unwrap();

        assert_eq!(effects.len(), 1);
        assert_eq!(app.project().unwrap().rom.read(0x80, 1).unwrap(), &[0x22]);
        let installed_revision = app.controller_snapshot().unwrap().revision;
        assert!(installed_revision > revision);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.read(0x80, 4).unwrap(),
            &[0xff; 4]
        );
        let before = app.project().unwrap().rom.logical_bytes().to_vec();
        assert!(
            app.dispatch(Command::InstallRevisionPatch {
                expected_revision: revision,
                template: Box::new(template(&profile)),
                search: 0x3_0000..0x4_0000,
                fill: 0xff,
            })
            .is_err()
        );
        assert_eq!(app.project().unwrap().rom.logical_bytes(), before);
    }

    #[test]
    fn application_rejects_patch_without_project_or_profile() {
        let profile = lm_profile::test_support::profile();
        let mut app = AppState::default();
        assert!(
            app.dispatch(Command::InstallRevisionPatch {
                expected_revision: 0,
                template: Box::new(template(&profile)),
                search: 0x8000..0x1_0000,
                fill: 0xff,
            })
            .is_err()
        );
        assert!(app.project().is_none());
    }

    #[test]
    fn lfix3_install_migrates_generation_1_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = RomImage::from_bytes(original).unwrap();
        image
            .replace_exact(
                0x0002_d7ce,
                &[0xf0, 0x02, 0xa9, 0x01],
                &[0x22, 0x50, 0xdc, 0x05],
            )
            .unwrap();
        image
            .replace_exact(
                0x0002_dc50,
                &[0xff; 0x30],
                &[
                    0xbd, 0xd8, 0x19, 0x89, 0x04, 0xf0, 0x1a, 0x48, 0x48, 0x29, 0x02, 0x4a, 0x8d,
                    0x93, 0x1b, 0x68, 0x29, 0x08, 0x0a, 0x0a, 0x0a, 0x8d, 0x2a, 0x19, 0x68, 0x4a,
                    0x08, 0x4a, 0x4a, 0x4a, 0x28, 0x2a, 0x6b, 0x9c, 0x2a, 0x19, 0xad, 0xbf, 0x13,
                    0xc9, 0x25, 0xa9, 0x00, 0x2a, 0x6b, 0xff, 0xff, 0xff,
                ],
            )
            .unwrap();
        let legacy = image.as_file_bytes().to_vec();
        let mut app = AppState::default();
        app.load_rom(legacy.clone()).unwrap();
        app.dispatch(Command::InstallLfix3 { rev: 0 }).unwrap();
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert!(
            lm_profile::detect_smw_us_v1_current_lfix3_runtime(
                app.project().unwrap().rom.logical_bytes()
            )
            .unwrap()
            .is_some()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), legacy);
    }

    #[test]
    fn expanded_exanimation_install_reopens_rejects_duplicate_and_undoes_exactly() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();

        let effects = app
            .dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                app.project().unwrap().rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
        assert!(
            lm_rom::SnesChecksum::decode(app.project().unwrap().rom.logical_bytes(), 0x7fdc)
                .unwrap()
                .is_complementary()
        );

        let revision = app.project_revision();
        assert!(matches!(
            app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: revision }),
            Err(AppError::ExpandedExAnimationRuntimeAlreadyInstalled)
        ));
        assert_eq!(app.project().unwrap().history.undo_len(), 1);

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn mapper_expanded_exanimation_install_reopens_preserves_header_and_undoes_exactly() {
        for mapper in [Mapper::ExLoRom, Mapper::Sa1] {
            for mapper_runtime in [false, true] {
                for headered in [false, true] {
                    let original = mapper_exanimation_fixture(mapper, mapper_runtime, headered);
                    let original_header = RomImage::from_bytes(original.clone())
                        .unwrap()
                        .copier_header_bytes()
                        .map(<[u8]>::to_vec);
                    let mut app = AppState::default();
                    app.load_rom(original.clone()).unwrap();
                    assert_eq!(
                        lm_profile::smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                            app.project().unwrap().rom.logical_bytes(),
                            mapper,
                        )
                        .unwrap(),
                        mapper_runtime
                    );

                    app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
                        .unwrap();
                    let project = app.project().unwrap();
                    assert_eq!(project.history.undo_len(), 1);
                    let runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
                            project.rom.logical_bytes(),
                            mapper,
                            mapper_runtime,
                        )
                        .unwrap();
                    assert_eq!(
                        runtime.payload.len(),
                        if mapper_runtime { 0xc50 } else { 0xc30 }
                    );
                    assert_eq!(
                        project.rom.copier_header_bytes().map(<[u8]>::to_vec),
                        original_header
                    );
                    let saved = project.save_snapshot();
                    let mut reopened = AppState::default();
                    reopened.load_rom(saved).unwrap();
                    assert_eq!(
                        reopened
                            .project()
                            .unwrap()
                            .identity
                            .as_ref()
                            .unwrap()
                            .mapper,
                        mapper
                    );

                    let revision = app.project_revision();
                    assert!(matches!(
                        app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: revision }),
                        Err(AppError::ExpandedExAnimationRuntimeAlreadyInstalled)
                    ));
                    app.dispatch(Command::Undo).unwrap();
                    assert_eq!(app.project().unwrap().save_snapshot(), original);
                }
            }
        }
    }

    #[test]
    fn expanded_exanimation_legacy_global_table_migrates_as_one_exact_undo() {
        let original = legacy_global_exanimation_fixture();
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(&original).unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable
        );
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
            .unwrap();
        let project = app.project().unwrap();
        assert_eq!(project.history.undo_len(), 1);
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                project.rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
        let runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime(
            project.rom.logical_bytes(),
        )
        .unwrap();
        let pointer = SnesPointer24::decode(
            &project.rom.logical_bytes()
                [runtime.payload.start + 0xea..runtime.payload.start + 0xed],
        )
        .unwrap()
        .to_pc(Mapper::LoRom)
        .unwrap();
        let layout = ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: pointer,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 0x40,
            maximum_encoded_len: 0x8000,
        };
        let mut modes = [false; 256];
        modes[1..=3].fill(true);
        let migrated = project.load_exanimation(7, layout, &modes).unwrap();
        assert_eq!(migrated.records.len(), 1);
        assert_eq!(migrated.records[0].kind(), 1);
        assert_eq!(migrated.records[0].destination(), 0x4321);
        assert_eq!(migrated.records[0].frame_bytes(false), &[0x34, 0x12]);
        assert!(
            lm_rom::SnesChecksum::decode(
                project.rom.logical_bytes(),
                lm_profile::SMW_US_V1_CHECKSUM_FIELD
            )
            .unwrap()
            .is_complementary()
        );

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    fn exgfx_insertion_migrates_legacy_global_exanimations_atomically() {
        let mut image = RomImage::from_bytes(legacy_global_exanimation_fixture()).unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET,
                &lm_profile::SMW_US_V1_EXGFX_RUNTIME_HOOK,
            )
            .unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET,
                &lm_profile::SMW_US_V1_EXGFX_TABLE_BASE_OPERAND,
            )
            .unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET,
                &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x1f],
            )
            .unwrap();
        image
            .write(
                lm_profile::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                &lm_profile::SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER,
            )
            .unwrap();
        for offset in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS {
            image
                .write(offset, &[lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER])
                .unwrap();
        }
        image.update_snes_checksum(0x7fdc).unwrap();
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                image.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable
        );
        let before = image.as_file_bytes().to_vec();
        let prepared =
            crate::prepare_smw_us_v1_exgraphics_install(17, image, &[(0x80, vec![0x80; 0x800])])
                .unwrap();
        let mut project = Project::new(RomImage::from_bytes(before.clone()).unwrap());
        project
            .apply_mutation(&prepared.description, &prepared.mutation)
            .unwrap();
        assert_eq!(prepared.expected_revision, 17);
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                project.rom.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
        assert_eq!(
            lm_profile::probe_smw_us_v1_exgraphics_runtime(&project.rom).unwrap(),
            lm_profile::SmwUsV1ExGraphicsRuntimeState::Expanded
        );
        let route = lm_profile::smw_us_v1_exgraphics_pointer(0x80).unwrap();
        assert_eq!(
            project
                .load_decompressed_graphics_file(
                    0,
                    lm_project::GraphicsRomLayout {
                        mapper: Mapper::LoRom,
                        pointers: LevelPointerTable {
                            offset: route.pointer_offset,
                            entries: 1,
                            stride: 3,
                        },
                        split_pointer_planes: None,
                        compression: lm_project::GraphicsCompression::Lz2,
                        maximum_compressed_len: 0x8000,
                        maximum_decompressed_len: 0x1000,
                    },
                )
                .unwrap(),
            vec![0x80; 0x800]
        );
        assert_ne!(project.rom.as_file_bytes(), before);
    }

    #[test]
    fn legacy_global_migration_preserves_copier_framing_and_rejects_bad_source_atomically() {
        let logical = legacy_global_exanimation_fixture();
        let mut headered = vec![0xa5; 0x200];
        headered.extend_from_slice(&logical);
        let mut app = AppState::default();
        app.load_rom(headered.clone()).unwrap();
        app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
            .unwrap();
        assert_eq!(
            app.project().unwrap().rom.copier_header_bytes(),
            Some(&headered[..0x200])
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), headered);

        let mut malformed = logical;
        malformed[0x8_2000 + 7 * 3 + 2] = 0x7e;
        let mut app = AppState::default();
        app.load_rom(malformed.clone()).unwrap();
        assert!(
            app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
                .is_err()
        );
        assert_eq!(app.project().unwrap().save_snapshot(), malformed);
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    fn external_lunar_magic_165_global_exanimations_migrate_reciprocally_when_supplied() {
        let (Ok(before_path), Ok(after_path)) = (
            std::env::var("LM_EXPANDED_EXANIMATION_LEGACY_GLOBAL_BEFORE"),
            std::env::var("LM_EXPANDED_EXANIMATION_LEGACY_GLOBAL_AFTER"),
        ) else {
            return;
        };
        let before = fs::read(before_path).unwrap();
        let after = RomImage::from_bytes(fs::read(after_path).unwrap()).unwrap();
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                RomImage::from_bytes(before.clone())
                    .unwrap()
                    .logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable
        );
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                after.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );

        let mut app = AppState::default();
        app.load_rom(before.clone()).unwrap();
        app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
            .unwrap();
        let rust = app.project().unwrap();
        assert_eq!(rust.history.undo_len(), 1);
        let rust_runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime(
            rust.rom.logical_bytes(),
        )
        .unwrap();
        let lunar_runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime(
            after.logical_bytes(),
        )
        .unwrap();
        let layout = |image: &RomImage, runtime: &lm_rats::RatsBlock| {
            let pointer = SnesPointer24::decode(
                &image.logical_bytes()[runtime.payload.start + 0xea..runtime.payload.start + 0xed],
            )
            .unwrap()
            .to_pc(Mapper::LoRom)
            .unwrap();
            ExAnimationRomLayout {
                mapper: Mapper::LoRom,
                pointers: LevelPointerTable {
                    offset: pointer,
                    entries: 0x200,
                    stride: 3,
                },
                maximum_records: 0x40,
                maximum_encoded_len: 0x8000,
            }
        };
        let rust_layout = layout(&rust.rom, &rust_runtime);
        let lunar_layout = layout(&after, &lunar_runtime);
        let lunar = Project::new(after.clone());
        let mut modes = [false; 256];
        modes[1..=3].fill(true);
        let mut populated = 0usize;
        for slot in 0..0x200 {
            let load = |project: &Project, layout: ExAnimationRomLayout| {
                let pointer = layout.pointers.pointer_offset(slot).unwrap();
                if project.rom.logical_bytes()[pointer + 2] == 0 {
                    None
                } else {
                    Some(project.load_exanimation(slot, layout, &modes).unwrap())
                }
            };
            let rust_animation = load(rust, rust_layout);
            let lunar_animation = load(&lunar, lunar_layout);
            assert_eq!(rust_animation, lunar_animation, "migrated slot {slot:#05x}");
            populated += usize::from(rust_animation.is_some());
        }
        assert!(
            populated > 0,
            "legacy oracle has no populated ExAnimation slots"
        );
        assert!(
            lm_rom::detect_identity(&rust.rom)
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), before);
    }

    #[test]
    fn external_lunar_magic_170_pointer_hooks_migrate_reciprocally_when_supplied() {
        let (Ok(before_path), Ok(after_path)) = (
            std::env::var("LM_EXPANDED_EXANIMATION_LEGACY_POINTER_BEFORE"),
            std::env::var("LM_EXPANDED_EXANIMATION_LEGACY_POINTER_AFTER"),
        ) else {
            return;
        };
        let before = fs::read(before_path).unwrap();
        let before_image = RomImage::from_bytes(before.clone()).unwrap();
        let after = RomImage::from_bytes(fs::read(after_path).unwrap()).unwrap();
        assert_eq!(
            lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
                before_image.logical_bytes()
            )
            .unwrap(),
            lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyPointerHooks
        );
        let migration =
            lm_profile::smw_us_v1_legacy_exanimation_hook_migration(before_image.logical_bytes())
                .unwrap();
        let runtime = migration.runtime.payload.start;
        for (relative, len, write_index) in [(0x92, 1, 0), (0x118, 1, 1), (0x169, 4, 2)] {
            assert_eq!(
                &after.logical_bytes()[runtime + relative..runtime + relative + len],
                &migration.plan.writes[write_index].replacement,
                "Lunar Magic pointer-hook fragment at runtime +{relative:#x}"
            );
        }

        let mut app = AppState::default();
        app.load_rom(before.clone()).unwrap();
        app.dispatch(Command::InstallExpandedExAnimationRuntime { rev: 0 })
            .unwrap();
        let rust = app.project().unwrap();
        assert_eq!(rust.history.undo_len(), 1);
        for (relative, len) in [(0x92, 1), (0x118, 1), (0x169, 4)] {
            assert_eq!(
                &rust.rom.logical_bytes()[runtime + relative..runtime + relative + len],
                &after.logical_bytes()[runtime + relative..runtime + relative + len],
                "Rust and Lunar Magic pointer-hook fragment at runtime +{relative:#x}"
            );
        }
        assert!(matches!(
            lm_profile::smw_us_v1_legacy_exanimation_hook_migration(rust.rom.logical_bytes()),
            Err(lm_profile::SmwUsV1LegacyExAnimationHookMigrationError::MarkerMismatch)
        ));
        assert!(
            lm_rom::detect_identity(&rust.rom)
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), before);
    }

    #[test]
    fn application_installs_complete_layer3_family_as_one_revision() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let mut app = AppState::default();
        app.load_rom(before).unwrap();
        let original = app.project().unwrap().rom.logical_bytes().to_vec();
        let revision = app.controller_snapshot().unwrap().revision;
        let effects = app
            .dispatch(Command::InstallLayer3 { rev: revision })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert!(app.controller_snapshot().unwrap().revision > revision);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), original);
    }

    #[test]
    fn layer3_reuses_an_already_installed_expanded_settings_prerequisite() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();
        let settings_snapshot = app.project().unwrap().save_snapshot();
        app.dispatch(Command::InstallLayer3 { rev: 1 }).unwrap();
        assert_eq!(app.project().unwrap().history.undo_len(), 2);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), settings_snapshot);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn application_installs_settings_into_preexpanded_headered_rom_and_undoes_exactly() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let pristine = RomImage::from_bytes(pristine).unwrap();
        let mut file = (0..lm_rom::COPIER_HEADER_LEN)
            .map(|index| (index as u8).wrapping_mul(19))
            .collect::<Vec<_>>();
        file.extend_from_slice(pristine.logical_bytes());
        let mut image = RomImage::from_bytes(file).unwrap();
        image.expand(Mapper::LoRom, 0x20_0000, 0x11).unwrap();
        image.write(0x18_0000, &vec![0xff; 0x8000]).unwrap();
        image
            .update_snes_checksum(lm_profile::SMW_US_V1_CHECKSUM_FIELD)
            .unwrap();
        let original = image.as_file_bytes().to_vec();

        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let effects = app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();

        assert_eq!(effects.len(), 1);
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        let layout =
            lm_profile::smw_us_v1_installed_expanded_settings_layout(app.project().unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(layout.table_offset, 0x18_2d08);
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x20_0000);
        assert_eq!(
            app.project().unwrap().rom.copier_header_bytes(),
            Some(&original[..lm_rom::COPIER_HEADER_LEN])
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn application_installs_sa1_settings_reopens_and_undoes_exactly() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = RomImage::from_bytes(pristine).unwrap();
        image.expand(Mapper::Sa1, 0x10_0000, 0xff).unwrap();
        let plan = lm_profile::smw_us_v1_sa1_expanded_settings_installation_plan().unwrap();
        for write in &plan.writes {
            image.write(write.offset, &write.expected).unwrap();
        }
        image.write(0x007fd5, &[0x23]).unwrap();
        image
            .update_snes_checksum(lm_profile::SMW_US_V1_CHECKSUM_FIELD)
            .unwrap();
        assert_eq!(lm_rom::detect_identity(&image).unwrap().mapper, Mapper::Sa1);
        let original = image.as_file_bytes().to_vec();

        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let effects = app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();

        assert_eq!(effects.len(), 1);
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert_eq!(
            app.project().unwrap().identity.as_ref().unwrap().mapper,
            Mapper::Sa1
        );
        assert_eq!(
            app.project().unwrap().rom.read(0x087ff8, 4).unwrap(),
            b"STAR"
        );
        assert!(
            lm_rom::detect_identity(&app.project().unwrap().rom)
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn application_honors_sa1_ram_remap_metadata_during_settings_install() {
        const REMAPPED_BYTES: &[(usize, u8)] = &[
            (0x07f192, 0x61),
            (0x07f7a3, 0x61),
            (0x07f82f, 0x79),
            (0x07f9c7, 0x73),
            (0x07f9e2, 0x61),
            (0x07faf2, 0x7f),
            (0x07faf5, 0x7f),
            (0x07fafc, 0x61),
            (0x07fb20, 0x3b),
            (0x07fb21, 0xeb),
            (0x07fb45, 0x67),
            (0x07fb48, 0x68),
            (0x07fb4b, 0x6d),
            (0x07fb4e, 0x7f),
            (0x07fc9b, 0x67),
            (0x07fd90, 0x74),
            (0x07fddf, 0x74),
        ];
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = RomImage::from_bytes(pristine).unwrap();
        image.expand(Mapper::Sa1, 0x10_0000, 0xff).unwrap();
        let plan = lm_profile::smw_us_v1_sa1_expanded_settings_installation_plan().unwrap();
        for write in &plan.writes {
            image.write(write.offset, &write.expected).unwrap();
        }
        image.write(0x007fd5, &[0x23]).unwrap();

        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let metadata = LunarMagicRomMetadata::from_parts(
            &attribution,
            1,
            &[0; LunarMagicRomMetadata::FEATURE_LEN],
        )
        .unwrap()
        .with_sa1_ram_remap(true);
        let layout = lm_profile::smw_us_v1_lunar_magic_metadata_layout_for_mapper(Mapper::Sa1);
        image
            .write(layout.attribution, metadata.attribution())
            .unwrap();
        image
            .write(layout.vram_version, &[metadata.vram_version()])
            .unwrap();
        image
            .write(layout.feature_record, metadata.feature_record())
            .unwrap();
        image
            .update_snes_checksum(lm_profile::SMW_US_V1_CHECKSUM_FIELD)
            .unwrap();
        let original = image.as_file_bytes().to_vec();

        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();
        for &(offset, replacement) in REMAPPED_BYTES {
            assert_eq!(
                app.project().unwrap().rom.read(offset, 1).unwrap(),
                &[replacement]
            );
        }
        for (offset, replacement) in [(0x07f9f7, 0x60), (0x07fbd6, 0x38)] {
            assert_eq!(
                app.project().unwrap().rom.read(offset, 1).unwrap(),
                &[replacement]
            );
        }
        assert!(
            lm_rom::detect_identity(&app.project().unwrap().rom)
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
