use crate::PreparedRomCommit;
use lm_profile::{
    SmwUsV1ExGraphicsEncoding, SmwUsV1ExGraphicsError, SmwUsV1ExGraphicsRuntimeState,
    has_smw_us_v1_4bpp_graphics_prerequisite, probe_smw_us_v1_exgraphics_runtime,
    smw_us_v1_exgraphics_installation_plan, smw_us_v1_exgraphics_pointer,
    smw_us_v1_gfx_expanded_settings_installation_plan,
};
use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable, Project, RomMutation};
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
    let mut project = Project::new(image);
    match probe_smw_us_v1_exgraphics_runtime(&project.rom) {
        Ok(_) => {}
        Err(SmwUsV1ExGraphicsError::UnsupportedRuntimeHook) => {
            if !has_smw_us_v1_4bpp_graphics_prerequisite(&project.rom) {
                return Err(
                    "SMW US v1 ExGFX insertion requires regular GFX to be inserted as 4bpp first"
                        .into(),
                );
            }
            project
                .install_relocatable_patch(
                    &smw_us_v1_gfx_expanded_settings_installation_plan()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            probe_smw_us_v1_exgraphics_runtime(&project.rom).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }

    let mut reserved = Vec::new();
    let mut compressed = Vec::new();
    for file in files.iter().cloned() {
        match smw_us_v1_exgraphics_pointer(file.0)
            .map_err(|error| error.to_string())?
            .encoding
        {
            SmwUsV1ExGraphicsEncoding::Raw2048 => reserved.push(file),
            SmwUsV1ExGraphicsEncoding::Lz2 => compressed.push(file),
        }
    }
    for group in [&reserved, &compressed] {
        if group.is_empty() {
            continue;
        }
        let plan = smw_us_v1_exgraphics_installation_plan(&project.rom, group)
            .map_err(|error| error.to_string())?;
        project
            .install_relocatable_patch(&plan)
            .map_err(|error| error.to_string())?;
    }

    for (file_number, expected) in files {
        let route =
            smw_us_v1_exgraphics_pointer(*file_number).map_err(|error| error.to_string())?;
        let actual = reopen_exgraphics_file(&project, route)
            .map_err(|error| format!("ExGFX{file_number:02X}: {error}"))?;
        if actual != *expected {
            return Err(format!(
                "ExGFX{file_number:02X}: reopened bytes differ after insertion"
            ));
        }
    }
    let final_state =
        probe_smw_us_v1_exgraphics_runtime(&project.rom).map_err(|error| error.to_string())?;
    let expected_state = if compressed.is_empty() {
        SmwUsV1ExGraphicsRuntimeState::ReservedOnly
    } else {
        SmwUsV1ExGraphicsRuntimeState::Expanded
    };
    if final_state != expected_state {
        return Err(format!(
            "ExGFX runtime reopened as {final_state:?}, expected {expected_state:?}"
        ));
    }
    let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
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
) -> Result<Vec<u8>, String> {
    match route.encoding {
        SmwUsV1ExGraphicsEncoding::Raw2048 => project
            .load_tagged_payload(route.pointer_offset, Mapper::LoRom)
            .map(|loaded| loaded.bytes)
            .map_err(|error| error.to_string()),
        SmwUsV1ExGraphicsEncoding::Lz2 => project
            .load_decompressed_graphics_file(
                0,
                GraphicsRomLayout {
                    mapper: Mapper::LoRom,
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::{
        SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET, SMW_US_V1_EXGFX_RUNTIME_HOOK,
        SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET, SMW_US_V1_EXGFX_TABLE_BASE_OPERAND,
        SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET,
    };

    fn ready_image() -> RomImage {
        let mut bytes = vec![0xff; 0x10_0000];
        bytes[SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET..SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET + 5]
            .copy_from_slice(&SMW_US_V1_EXGFX_RUNTIME_HOOK);
        bytes[SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET
            ..SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET + 3]
            .copy_from_slice(&SMW_US_V1_EXGFX_TABLE_BASE_OPERAND);
        bytes[SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET + 6] = 0x1f;
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
            let actual = reopen_exgraphics_file(&reopened, route).unwrap();
            assert_eq!(actual, expected);
        }
    }
}
