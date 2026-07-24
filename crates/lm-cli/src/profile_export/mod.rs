mod assets;
mod expanded_settings;
mod level_map16;
mod native_assets;
mod overworld;

use crate::args::ProfileExportKind;
use crate::oracle_input::read_rom;
use lm_profile::RevisionProfile;
use lm_project::Project;
use lm_rom::RomImage;
use std::fs;
use std::path::Path;

pub fn execute(
    kind: ProfileExportKind,
    rom: &Path,
    profile_path: &Path,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output || profile_path == output {
        return Err("refusing to overwrite an input file".into());
    }
    let profile = RevisionProfile::read_from(fs::File::open(profile_path)?)?;
    let project = Project::open_supported(RomImage::from_bytes(read_rom(rom)?)?)?;
    profile.ensure_identity(
        project
            .identity
            .as_ref()
            .ok_or("opened project has no detected identity")?,
    )?;
    profile.audit_rom(&project.rom)?;
    match kind {
        ProfileExportKind::NativeAssets => native_assets::export(&project, &profile, slot, output)?,
        ProfileExportKind::Level => level_map16::level(&project, &profile, slot, output)?,
        ProfileExportKind::Layer2 => level_map16::layer2(&project, &profile, slot, output)?,
        ProfileExportKind::Map16 => level_map16::map16(&project, &profile, slot, output)?,
        ProfileExportKind::Graphics => assets::graphics(&project, &profile, slot, output)?,
        ProfileExportKind::Palette => assets::palette(&project, &profile, slot, output)?,
        ProfileExportKind::ExAnimation => {
            assets::exanimation(&project, &profile, slot, output)?;
        }
        ProfileExportKind::ExpandedSettings => {
            expanded_settings::export(&project, &profile, slot, output)?;
        }
        ProfileExportKind::Overworld => overworld::export(&project, &profile, slot, output)?,
    }
    println!("profile: {}", profile.name);
    println!("output: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Map16PageFile, Map16Tile};
    use lm_rom::{Mapper, pc_to_snes};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lm-profile-export-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn set_pointer(bytes: &mut [u8], offset: usize, mapper: Mapper, target: usize) {
        let pointer = pc_to_snes(mapper, target).unwrap().to_le_bytes();
        bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
    }

    fn initialize_profile_tables(bytes: &mut [u8], profile: &lm_profile::RevisionProfile) {
        let tables = [
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
        ];
        for table in tables {
            for index in 0..table.entries {
                set_pointer(
                    bytes,
                    table.offset + index * table.stride,
                    profile.mapper,
                    0x1_0000,
                );
            }
        }
    }

    #[test]
    fn input_aliases_are_rejected_before_file_access() {
        let path = Path::new("same.file");
        assert!(
            execute(
                ProfileExportKind::Level,
                path,
                Path::new("profile"),
                0,
                path
            )
            .is_err()
        );
        assert!(execute(ProfileExportKind::Level, Path::new("rom"), path, 0, path).is_err());
    }

    #[test]
    fn profile_map16_export_uses_identity_checked_parallel_tables() {
        let rom_path = temporary("rom.smc");
        let profile_path = temporary("profile.lmrev");
        let output_path = temporary("page.lm16");
        let profile = lm_profile::test_support::profile();
        let mut rom = vec![0; 0x40_8000];
        rom[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        rom[0x7fd5] = 0x20;
        rom[0x7fd9] = 1;
        initialize_profile_tables(&mut rom, &profile);
        set_pointer(
            &mut rom,
            profile.map16.graphics.offset,
            profile.mapper,
            0x1_0000,
        );
        set_pointer(
            &mut rom,
            profile.map16.acts_like.offset,
            profile.mapper,
            0x1_0800,
        );
        fs::write(&rom_path, rom).unwrap();
        fs::write(&profile_path, profile.encode()).unwrap();

        execute(
            ProfileExportKind::Map16,
            &rom_path,
            &profile_path,
            0,
            &output_path,
        )
        .unwrap();
        let exported = Map16PageFile::decode(&fs::read(&output_path).unwrap()).unwrap();
        assert_eq!(exported.source_page, 0);
        assert!(
            exported
                .page
                .tiles
                .iter()
                .all(|tile| *tile == Map16Tile::default())
        );

        fs::remove_file(&output_path).unwrap();
        let mut malformed_rom = fs::read(&rom_path).unwrap();
        malformed_rom[profile.overworld.messages.pointers.offset
            ..profile.overworld.messages.pointers.offset + 3]
            .fill(0);
        fs::write(&rom_path, malformed_rom).unwrap();
        assert!(
            execute(
                ProfileExportKind::Map16,
                &rom_path,
                &profile_path,
                0,
                &output_path,
            )
            .is_err()
        );
        assert!(!output_path.exists());

        let mut incompatible = profile;
        incompatible.region = lm_rom::Region::Japan;
        fs::write(&profile_path, incompatible.encode()).unwrap();
        assert!(
            execute(
                ProfileExportKind::Map16,
                &rom_path,
                &profile_path,
                0,
                &output_path,
            )
            .is_err()
        );
        assert!(!output_path.exists());

        fs::remove_file(rom_path).unwrap();
        fs::remove_file(profile_path).unwrap();
    }
}
