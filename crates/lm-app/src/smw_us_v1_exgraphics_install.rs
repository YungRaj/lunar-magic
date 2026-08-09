use crate::PreparedRomCommit;
use lm_profile::{
    SmwUsV1ExGraphicsEncoding, SmwUsV1ExGraphicsError, SmwUsV1ExGraphicsRuntimeState,
    SmwUsV1ExpandedExAnimationRuntimeGeneration, has_smw_us_v1_4bpp_graphics_prerequisite,
    probe_smw_us_v1_exgraphics_runtime_for_mapper,
    probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper,
    smw_us_v1_exgraphics_installation_plan_for_mapper, smw_us_v1_exgraphics_pointer_for_mapper,
    smw_us_v1_exgraphics_pointer_in_rom, smw_us_v1_expanded_exanimation_runtime_installation_plan,
    smw_us_v1_gfx_expanded_settings_installation_plan,
    smw_us_v1_sa1_exgraphics_runtime_installation_plan,
    smw_us_v1_sa1_expanded_settings_installation_plan,
};
use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable, Project, RomMutation};
use lm_rats::ProtectedRange;
use lm_rom::{Mapper, RomImage};

/// Prepares first-time or subsequent native ExGFX insertion as one application commit.
///
/// A post-4bpp-GFX ROM receives the exact zero-filled expanded-settings prerequisite first. Files
/// `$60..$63` and `$80..$FFF` then use their independent native allocation passes, but only the
/// final combined mutation is published. Every inserted pointer is reopened through its RATS block
/// and its raw bytes are compared before returning.
///
/// # Errors
///
/// Rejects an unsupported runtime family, malformed files, failed prerequisite/allocation writes,
/// or any semantic reopen disagreement without mutating application state.
pub fn prepare_smw_us_v1_exgraphics_install(
    expected_revision: u64,
    image: RomImage,
    files: &[(u16, Vec<u8>)],
) -> Result<PreparedRomCommit, String> {
    if files.is_empty() {
        return Err("ExGFX insertion requires at least one file".into());
    }
    let before = image.logical_bytes().to_vec();
    let mapper = lm_rom::detect_identity(&image)
        .map(|identity| identity.mapper)
        .unwrap_or(Mapper::LoRom);
    let mut project = Project::new(image);
    match probe_smw_us_v1_exgraphics_runtime_for_mapper(&project.rom, mapper) {
        Ok(state) => {
            if mapper == Mapper::Sa1 && state != SmwUsV1ExGraphicsRuntimeState::Expanded {
                return Err(
                    "SA-1 ready/reserved ExGFX runtime migration is not yet available".into(),
                );
            }
        }
        Err(SmwUsV1ExGraphicsError::UnsupportedRuntimeHook) => {
            if !matches!(mapper, Mapper::LoRom | Mapper::Sa1) {
                return Err(format!(
                    "first-time ExGFX prerequisite installation is not yet available for {mapper:?}"
                ));
            }
            if !has_smw_us_v1_4bpp_graphics_prerequisite(&project.rom) {
                return Err(
                    "SMW US v1 ExGFX insertion requires regular GFX to be inserted as 4bpp first"
                        .into(),
                );
            }
            let settings = if mapper == Mapper::Sa1 {
                smw_us_v1_sa1_expanded_settings_installation_plan()
            } else {
                smw_us_v1_gfx_expanded_settings_installation_plan()
            }
            .map_err(|error| error.to_string())?;
            project
                .install_relocatable_patch(&settings)
                .map_err(|error| error.to_string())?;
            if mapper == Mapper::Sa1 {
                let first_domain_files =
                    files.iter().map(|(number, _)| *number).collect::<Vec<_>>();
                let runtime = smw_us_v1_sa1_exgraphics_runtime_installation_plan(
                    &project.rom,
                    &first_domain_files,
                )
                .map_err(|error| error.to_string())?;
                project
                    .install_relocatable_patch(&runtime)
                    .map_err(|error| error.to_string())?;
            }
            probe_smw_us_v1_exgraphics_runtime_for_mapper(&project.rom, mapper)
                .map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    if mapper != Mapper::Sa1 {
        let mapper_runtime = lm_profile::smw_us_v1_expanded_exanimation_uses_mapper_runtime(
            project.rom.logical_bytes(),
            mapper,
        )
        .map_err(|error| error.to_string())?;
        let mut generation = probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(
            project.rom.logical_bytes(),
            mapper,
            mapper_runtime,
        );
        if generation.is_err()
            && mapper != Mapper::LoRom
            && mapper_runtime
            && matches!(
                probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(
                    project.rom.logical_bytes(),
                    mapper,
                    false,
                ),
                Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::Current)
            )
        {
            migrate_relocated_lorom_exanimation_runtime(&mut project, mapper)?;
            generation = probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(
                project.rom.logical_bytes(),
                mapper,
                mapper_runtime,
            );
        }
        match generation.map_err(|error| error.to_string())? {
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Absent => {
                let mut plan = if mapper == Mapper::LoRom {
                    smw_us_v1_expanded_exanimation_runtime_installation_plan()
                        .map_err(|error| error.to_string())?
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
                    )
                    .map_err(|error| error.to_string())?
                };
                plan.allocation.fill_bytes = vec![0x00, 0xff];
                let extended =
                    mapper_rom_offset(mapper, lm_profile::SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET);
                plan.allocation
                    .protected
                    .push(ProtectedRange(extended..extended + 0xf00 * 3));
                project
                    .install_relocatable_patch(&plan)
                    .map_err(|error| error.to_string())?;
            }
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current => {}
            SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyPointerHooks => {
                if mapper != Mapper::LoRom {
                    return Err(format!(
                        "legacy pointer-hook ExAnimation migration is not valid for {mapper:?}"
                    ));
                }
                let migration = lm_profile::smw_us_v1_legacy_exanimation_hook_migration(
                    project.rom.logical_bytes(),
                )
                .map_err(|error| error.to_string())?;
                project
                    .install_relocatable_patch(&migration.plan)
                    .map_err(|error| error.to_string())?;
            }
            SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable => {
                if mapper != Mapper::LoRom {
                    return Err(format!(
                        "legacy global-table ExAnimation migration is not valid for {mapper:?}"
                    ));
                }
                crate::revision_patch_state::migrate_legacy_global_exanimations(&mut project)
                    .map_err(|error| error.to_string())?;
            }
        }
    }

    let mut reserved = Vec::new();
    let mut compressed = Vec::new();
    let mut extended = Vec::new();
    for file in files.iter().cloned() {
        match smw_us_v1_exgraphics_pointer_for_mapper(file.0, mapper)
            .map_err(|error| error.to_string())?
            .encoding
        {
            SmwUsV1ExGraphicsEncoding::Raw2048 => reserved.push(file),
            SmwUsV1ExGraphicsEncoding::Lz2 if mapper == Mapper::Sa1 && file.0 >= 0x100 => {
                extended.push(file);
            }
            SmwUsV1ExGraphicsEncoding::Lz2 => compressed.push(file),
        }
    }
    let compressed_groups = if mapper == Mapper::Sa1 && reserved.is_empty() {
        [&extended, &compressed]
    } else {
        [&compressed, &extended]
    };
    for group in std::iter::once(&reserved).chain(compressed_groups) {
        if group.is_empty() {
            continue;
        }
        let plan = smw_us_v1_exgraphics_installation_plan_for_mapper(&project.rom, group, mapper)
            .map_err(|error| error.to_string())?;
        project
            .install_relocatable_patch(&plan)
            .map_err(|error| error.to_string())?;
    }

    for (file_number, expected) in files {
        let route = smw_us_v1_exgraphics_pointer_in_rom(&project.rom, *file_number, mapper)
            .map_err(|error| error.to_string())?;
        let actual = reopen_exgraphics_file(&project, route, mapper)
            .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?;
        if actual != *expected {
            return Err(format!(
                "ExGFX{file_number:02X}: reopened bytes differ after insertion"
            ));
        }
    }
    let final_state = probe_smw_us_v1_exgraphics_runtime_for_mapper(&project.rom, mapper)
        .map_err(|error| error.to_string())?;
    let expected_state = if mapper == Mapper::Sa1 {
        SmwUsV1ExGraphicsRuntimeState::Expanded
    } else if compressed.is_empty() {
        SmwUsV1ExGraphicsRuntimeState::ReservedOnly
    } else {
        SmwUsV1ExGraphicsRuntimeState::Expanded
    };
    if final_state != expected_state {
        return Err(format!(
            "ExGFX runtime reopened as {final_state:?}, expected {expected_state:?}"
        ));
    }
    let mutation = RomMutation::between(mapper, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision,
        description: "Insert native SMW US ExGFX files".into(),
        mutation,
    })
}

fn reopen_exgraphics_file(
    project: &Project,
    route: lm_profile::SmwUsV1ExGraphicsPointer,
    mapper: Mapper,
) -> Result<Vec<u8>, String> {
    match route.encoding {
        SmwUsV1ExGraphicsEncoding::Raw2048 => project
            .load_tagged_payload(route.pointer_offset, mapper)
            .map(|loaded| loaded.bytes)
            .map_err(|error| error.to_string()),
        SmwUsV1ExGraphicsEncoding::Lz2 => project
            .load_decompressed_graphics_file(
                0,
                GraphicsRomLayout {
                    mapper,
                    pointers: LevelPointerTable {
                        offset: route.pointer_offset,
                        entries: 1,
                        stride: 3,
                    },
                    split_pointer_planes: None,
                    compression: GraphicsCompression::Lz2,
                    maximum_compressed_len: 0x8000,
                    maximum_decompressed_len: 0x1000,
                },
            )
            .map_err(|error| error.to_string()),
    }
}

fn mapper_rom_offset(mapper: Mapper, lorom_offset: usize) -> usize {
    if mapper == Mapper::ExLoRom {
        0x40_0000 + lorom_offset
    } else {
        lorom_offset
    }
}

fn migrate_relocated_lorom_exanimation_runtime(
    project: &mut Project,
    mapper: Mapper,
) -> Result<(), String> {
    let runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
        project.rom.logical_bytes(),
        mapper,
        false,
    )
    .map_err(|error| error.to_string())?;
    let pointer = lm_rom::SnesPointer24::decode(
        &project.rom.logical_bytes()[runtime.payload.start + 0xea..runtime.payload.start + 0xed],
    )
    .map_err(|error| error.to_string())?
    .to_pc(mapper)
    .map_err(|error| error.to_string())?;
    let pointer_table = project
        .rom
        .read(pointer, lm_profile::EXPANDED_EXANIMATION_POINTER_TABLE_LEN)
        .map_err(|error| error.to_string())?
        .to_vec();
    let search = if mapper == Mapper::ExLoRom {
        0x10_0000..0x40_0000
    } else {
        0x40_0000..project.rom.logical_len()
    };
    let mut plan = lm_profile::smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
        mapper,
        lm_rats::AllocationPolicy::lorom(search),
        true,
    )
    .map_err(|error| error.to_string())?;
    plan.allocation.fill_bytes = vec![0x00, 0xff];
    plan.payloads[1].bytes = pointer_table;
    for write in &mut plan.writes {
        write.expected = project
            .rom
            .read(write.offset, write.replacement.len())
            .map_err(|error| error.to_string())?
            .to_vec();
    }
    project
        .install_relocatable_patch(&plan)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::{
        SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET, SMW_US_V1_EXGFX_RUNTIME_HOOK,
        SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET, SMW_US_V1_EXGFX_TABLE_BASE_OPERAND,
        SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET, probe_smw_us_v1_exgraphics_runtime,
        probe_smw_us_v1_expanded_exanimation_runtime_generation, smw_us_v1_exgraphics_pointer,
    };

    fn ready_image() -> RomImage {
        let mut bytes = vec![0xff; 0x10_0000];
        let core = smw_us_v1_expanded_exanimation_runtime_installation_plan().unwrap();
        for write in core.writes {
            bytes[write.offset..write.offset + write.expected.len()]
                .copy_from_slice(&write.expected);
        }
        bytes[SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET..SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET + 5]
            .copy_from_slice(&SMW_US_V1_EXGFX_RUNTIME_HOOK);
        bytes[SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET
            ..SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET + 3]
            .copy_from_slice(&SMW_US_V1_EXGFX_TABLE_BASE_OPERAND);
        bytes[SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET + 6] = 0x1f;
        bytes[lm_profile::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET
            ..lm_profile::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET + 2]
            .copy_from_slice(&lm_profile::SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER);
        bytes[0x7fd7] = 0x0a;
        bytes[0x080000..0x080008].copy_from_slice(b"STAR\x1f\0\xe0\xff");
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn mixed_native_domains_prepare_one_reopenable_revision_bound_mutation() {
        let image = ready_image();
        let before = image.logical_bytes().to_vec();
        let files = [(0x60, vec![0x5a; 0x800]), (0x80, vec![0xa5; 0xc00])];
        let prepared = prepare_smw_us_v1_exgraphics_install(41, image, &files).unwrap();
        assert_eq!(prepared.expected_revision, 41);
        let mut reopened = Project::new(RomImage::from_bytes(before).unwrap());
        reopened
            .apply_mutation("insert native ExGFX", &prepared.mutation)
            .unwrap();
        assert_eq!(
            probe_smw_us_v1_exgraphics_runtime(&reopened.rom).unwrap(),
            SmwUsV1ExGraphicsRuntimeState::Expanded
        );
        for (file_number, expected) in files {
            let route = smw_us_v1_exgraphics_pointer(file_number).unwrap();
            let actual = reopen_exgraphics_file(&reopened, route, Mapper::LoRom).unwrap();
            assert_eq!(actual, expected);
        }
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(reopened.rom.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
    }

    fn assert_authentic_sa1_first_exgfx_is_byte_exact(file_number: u16, after_variable: &str) {
        let before = RomImage::from_bytes(
            std::fs::read(std::env::var_os("LM_SA1_EXGFX_BEFORE").expect("LM_SA1_EXGFX_BEFORE"))
                .unwrap(),
        )
        .unwrap();
        let oracle =
            RomImage::from_bytes(
                std::fs::read(std::env::var_os(after_variable).unwrap_or_else(|| {
                    panic!("{after_variable} must name an authentic after image")
                }))
                .unwrap(),
            )
            .unwrap();
        let bytes = (0..0x800_usize)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        let prepared =
            prepare_smw_us_v1_exgraphics_install(0, before.clone(), &[(file_number, bytes)])
                .unwrap();
        let mut project = Project::new(before);
        project
            .apply_mutation(&prepared.description, &prepared.mutation)
            .unwrap();
        let actual = project.rom.logical_bytes();
        let expected = oracle.logical_bytes();
        let mismatches = actual
            .iter()
            .zip(expected)
            .enumerate()
            .filter_map(|(offset, (actual, expected))| {
                (actual != expected).then_some((offset, *actual, *expected))
            })
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len());
        assert!(
            mismatches.is_empty(),
            "{} mismatches; first: {:02X?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(64)]
        );
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack before/after first-ExGFX oracle images"]
    fn authentic_sa1_first_exgfx60_install_is_byte_exact() {
        assert_authentic_sa1_first_exgfx_is_byte_exact(0x60, "LM_SA1_EXGFX60_AFTER");
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack before/after first-ExGFX oracle images"]
    fn authentic_sa1_first_exgfx80_install_is_byte_exact() {
        assert_authentic_sa1_first_exgfx_is_byte_exact(0x80, "LM_SA1_EXGFX_AFTER");
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack before/after first-ExGFX oracle images"]
    fn authentic_sa1_first_exgfx100_install_is_byte_exact() {
        assert_authentic_sa1_first_exgfx_is_byte_exact(0x100, "LM_SA1_EXGFX100_AFTER");
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack before/after mixed first-ExGFX oracle images"]
    fn authentic_sa1_first_mixed_exgfx_domains_are_byte_exact() {
        let before = RomImage::from_bytes(
            std::fs::read(std::env::var_os("LM_SA1_EXGFX_BEFORE").expect("LM_SA1_EXGFX_BEFORE"))
                .unwrap(),
        )
        .unwrap();
        let bytes = (0..0x800_usize)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        for (variable, numbers) in [
            ("LM_SA1_EXGFX60_80_AFTER", &[0x60, 0x80][..]),
            ("LM_SA1_EXGFX60_100_AFTER", &[0x60, 0x100][..]),
            ("LM_SA1_EXGFX80_100_AFTER", &[0x80, 0x100][..]),
            ("LM_SA1_EXGFX_MIXED_AFTER", &[0x60, 0x80, 0x100][..]),
        ] {
            let oracle = RomImage::from_bytes(
                std::fs::read(
                    std::env::var_os(variable)
                        .unwrap_or_else(|| panic!("{variable} must name an authentic after image")),
                )
                .unwrap(),
            )
            .unwrap();
            let files = numbers
                .iter()
                .map(|number| (*number, bytes.clone()))
                .collect::<Vec<_>>();
            let prepared = prepare_smw_us_v1_exgraphics_install(0, before.clone(), &files).unwrap();
            let mut project = Project::new(before.clone());
            project
                .apply_mutation(&prepared.description, &prepared.mutation)
                .unwrap();
            let actual = project.rom.logical_bytes();
            let expected = oracle.logical_bytes();
            let mismatches = actual
                .iter()
                .zip(expected)
                .enumerate()
                .filter_map(|(offset, (actual, expected))| {
                    (actual != expected).then_some((offset, *actual, *expected))
                })
                .collect::<Vec<_>>();
            assert_eq!(actual.len(), expected.len(), "{variable}");
            assert!(
                mismatches.is_empty(),
                "{variable}: {} mismatches; first: {:02X?}",
                mismatches.len(),
                &mismatches[..mismatches.len().min(64)]
            );
        }
    }

    #[test]
    fn insertion_migrates_authenticated_legacy_pointer_hooks_in_the_same_commit() {
        let mut project = Project::new(ready_image());
        let first = prepare_smw_us_v1_exgraphics_install(
            0,
            project.rom.clone(),
            &[(0x80, vec![0x80; 0x800])],
        )
        .unwrap();
        project
            .apply_mutation(&first.description, &first.mutation)
            .unwrap();
        let runtime = lm_profile::detect_smw_us_v1_current_expanded_exanimation_runtime(
            project.rom.logical_bytes(),
        )
        .unwrap();
        project
            .rom
            .write(runtime.payload.start + 0x92, &[0x0f])
            .unwrap();
        project
            .rom
            .write(runtime.payload.start + 0x118, &[0x0f])
            .unwrap();
        project
            .rom
            .write(runtime.payload.start + 0x169, &[0x4c, 0x4d, 0x00, 0x01])
            .unwrap();
        project.rom.update_snes_checksum(0x7fdc).unwrap();
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(project.rom.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyPointerHooks
        );

        let before = project.rom.as_file_bytes().to_vec();
        let second = prepare_smw_us_v1_exgraphics_install(
            9,
            project.rom.clone(),
            &[(0x81, vec![0x81; 0x800])],
        )
        .unwrap();
        assert_eq!(second.expected_revision, 9);
        assert_eq!(
            project.rom.as_file_bytes(),
            before,
            "preparation mutated the caller's image"
        );
        project
            .apply_mutation(&second.description, &second.mutation)
            .unwrap();
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(project.rom.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
        assert_eq!(
            reopen_exgraphics_file(
                &project,
                smw_us_v1_exgraphics_pointer(0x80).unwrap(),
                Mapper::LoRom,
            )
            .unwrap(),
            vec![0x80; 0x800]
        );
        assert_eq!(
            reopen_exgraphics_file(
                &project,
                smw_us_v1_exgraphics_pointer(0x81).unwrap(),
                Mapper::LoRom,
            )
            .unwrap(),
            vec![0x81; 0x800]
        );
    }

    #[test]
    fn converted_exlorom_inserts_and_reopens_through_relocated_tables() {
        let mut source = ready_image();
        let pristine =
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        source
            .write(0x7fc0, &pristine.logical_bytes()[0x7fc0..0x8000])
            .unwrap();
        source.write(0x7fd7, &[0x0a]).unwrap();
        source.update_snes_checksum(0x7fdc).unwrap();
        assert_eq!(
            lm_rom::detect_identity(&source).unwrap().mapper,
            Mapper::LoRom
        );
        let first =
            prepare_smw_us_v1_exgraphics_install(0, source.clone(), &[(0x80, vec![0x80; 0x800])])
                .unwrap();
        let mut installed = Project::open_supported(source).unwrap();
        installed
            .apply_mutation(&first.description, &first.mutation)
            .unwrap();
        installed.convert_to_64_mbit_exlorom().unwrap();
        assert_eq!(installed.identity.as_ref().unwrap().mapper, Mapper::ExLoRom);
        assert_eq!(
            probe_smw_us_v1_exgraphics_runtime_for_mapper(&installed.rom, Mapper::ExLoRom).unwrap(),
            SmwUsV1ExGraphicsRuntimeState::Expanded
        );

        let converted_before = installed.rom.as_file_bytes().to_vec();
        let second = prepare_smw_us_v1_exgraphics_install(
            12,
            installed.rom.clone(),
            &[(0x81, vec![0x81; 0x800])],
        )
        .unwrap();
        assert_eq!(installed.rom.as_file_bytes(), converted_before);
        installed
            .apply_mutation(&second.description, &second.mutation)
            .unwrap();
        assert_eq!(second.expected_revision, 12);
        for (file, expected) in [(0x80, 0x80), (0x81, 0x81)] {
            let route = smw_us_v1_exgraphics_pointer_for_mapper(file, Mapper::ExLoRom).unwrap();
            assert_eq!(
                reopen_exgraphics_file(&installed, route, Mapper::ExLoRom).unwrap(),
                vec![expected; 0x800]
            );
        }
        assert!(
            lm_rom::detect_identity(&installed.rom)
                .unwrap()
                .checksum_matches()
        );
    }
}
