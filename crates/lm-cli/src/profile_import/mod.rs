mod assets;
mod common;
mod expanded_settings;
mod level_map16;
mod native_assets;
mod overworld;

use crate::args::ProfileImportKind;
use common::ImportContext;
use std::ops::Range;
use std::path::Path;

pub fn execute(
    kind: ProfileImportKind,
    input_rom: &Path,
    output_rom: &Path,
    profile: &Path,
    slot: usize,
    asset: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = ImportContext::load(input_rom, output_rom, profile, asset, search)?;
    match kind {
        ProfileImportKind::NativeAssets => native_assets::import(context, slot, asset, output_rom),
        ProfileImportKind::Level => level_map16::level(context, slot, asset, output_rom),
        ProfileImportKind::Map16 => level_map16::map16(context, slot, asset, output_rom),
        ProfileImportKind::Graphics => assets::graphics(context, slot, asset, output_rom),
        ProfileImportKind::Palette => assets::palette(context, slot, asset, output_rom),
        ProfileImportKind::ExAnimation => assets::exanimation(context, slot, asset, output_rom),
        ProfileImportKind::ExpandedSettings => {
            expanded_settings::import(context, slot, asset, output_rom)
        }
        ProfileImportKind::Overworld => overworld::import(context, slot, asset, output_rom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_level::{
        ExpandedLevelSettingsRecord, LevelObjectData, Map16Page, Map16PageFile, Map16Tile,
        NativeSpriteStream, Subtile,
    };
    use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets, NativeLevelAssetsFile, Project};
    use lm_rom::{Mapper, RomImage, pc_to_snes};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lm-profile-import-{name}-{}-{nonce}",
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
    fn output_aliases_are_rejected_before_file_access() {
        let same = Path::new("same");
        assert!(
            execute(
                ProfileImportKind::Map16,
                same,
                same,
                Path::new("profile"),
                0,
                Path::new("asset"),
                0..1,
            )
            .is_err()
        );
        assert!(
            execute(
                ProfileImportKind::Map16,
                Path::new("rom"),
                same,
                same,
                0,
                Path::new("asset"),
                0..1,
            )
            .is_err()
        );
    }

    #[test]
    fn profile_map16_import_allocates_checksums_and_semantically_reopens() {
        let input = temporary("before.smc");
        let output = temporary("after.smc");
        let profile_path = temporary("profile.lmrev");
        let asset = temporary("page.lm16");
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
        let mut page = Map16Page {
            tiles: vec![Map16Tile::default(); Map16Page::TILE_COUNT],
        };
        page.tiles[7] = Map16Tile {
            top_left: Subtile(0x4321),
            acts_like: 0x1234,
            ..Map16Tile::default()
        };
        fs::write(&input, rom).unwrap();
        fs::write(&profile_path, profile.encode()).unwrap();
        fs::write(
            &asset,
            Map16PageFile {
                source_page: 9,
                page: page.clone(),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();

        execute(
            ProfileImportKind::Map16,
            &input,
            &output,
            &profile_path,
            0,
            &asset,
            0x2_0000..0x3_0000,
        )
        .unwrap();
        let project =
            Project::open_supported(RomImage::from_bytes(fs::read(&output).unwrap()).unwrap())
                .unwrap();
        assert_eq!(project.load_map16_page(0, profile.map16).unwrap(), page);
        assert!(project.identity.unwrap().checksum_matches());

        for path in [input, output, profile_path, asset] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn profile_expanded_settings_import_repairs_checksum_and_reopens() {
        let input = temporary("expanded-before.smc");
        let output = temporary("expanded-after.smc");
        let profile_path = temporary("expanded-profile.lmrev");
        let asset = temporary("expanded-record.bin");
        let profile = lm_profile::test_support::profile();
        let mut rom = vec![0; 0x40_8000];
        rom[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        rom[0x7fd5] = 0x20;
        rom[0x7fd9] = 1;
        initialize_profile_tables(&mut rom, &profile);
        let record_bytes =
            std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap() ^ 0xa5);
        let record = ExpandedLevelSettingsRecord::decode(&record_bytes).unwrap();
        fs::write(&input, rom).unwrap();
        fs::write(&profile_path, profile.encode()).unwrap();
        fs::write(&asset, record.encoded()).unwrap();

        execute(
            ProfileImportKind::ExpandedSettings,
            &input,
            &output,
            &profile_path,
            0x105,
            &asset,
            0x3_0000..0x4_0000,
        )
        .unwrap();
        let project =
            Project::open_supported(RomImage::from_bytes(fs::read(&output).unwrap()).unwrap())
                .unwrap();
        assert_eq!(
            project
                .load_expanded_level_settings(0x105, profile.expanded_settings.unwrap())
                .unwrap(),
            record
        );
        assert!(project.identity.unwrap().checksum_matches());

        for path in [input, output, profile_path, asset] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn profile_native_assets_import_is_atomic_checksum_valid_and_semantically_reopens() {
        let input = temporary("native-assets-before.smc");
        let output = temporary("native-assets-after.smc");
        let profile_path = temporary("native-assets-profile.lmrev");
        let asset = temporary("native-assets.lmna");
        let profile = lm_profile::test_support::profile();
        let mut rom = vec![0; 0x40_8000];
        rom[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        rom[0x7fd5] = 0x20;
        rom[0x7fd9] = 1;
        initialize_profile_tables(&mut rom, &profile);
        let settings = ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        let assets = LoadedNativeLevelAssets {
            level: LoadedLevelSlot {
                number: 0x105,
                layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
                sprites: NativeSpriteStream::parse(
                    if profile.level.expanded_sprites {
                        &[0x10, 0, 1, 2, 0xff, 0xfe]
                    } else {
                        &[0x10, 0, 1, 2, 0xff]
                    },
                    profile.level.expanded_sprites,
                    &profile.sprite_lengths,
                )
                .unwrap(),
            },
            palette: Palette {
                colors: (0..profile.palette.colors_per_palette)
                    .map(|index| Bgr555(u16::try_from(index).unwrap()))
                    .collect(),
            },
            exanimation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            expanded_settings: Some(settings),
        };
        fs::write(&input, rom).unwrap();
        fs::write(&profile_path, profile.encode()).unwrap();
        fs::write(
            &asset,
            NativeLevelAssetsFile {
                source_slot: 0x105,
                assets: assets.clone(),
            }
            .encode(&profile.exanimation_double_size_modes)
            .unwrap(),
        )
        .unwrap();

        execute(
            ProfileImportKind::NativeAssets,
            &input,
            &output,
            &profile_path,
            0x105,
            &asset,
            0x3_0000..0x4_0000,
        )
        .unwrap();
        let project =
            Project::open_supported(RomImage::from_bytes(fs::read(&output).unwrap()).unwrap())
                .unwrap();
        let (layout, _) = profile
            .native_level_assets_save_plan(
                0x3_0000..0x4_0000,
                project.rom.logical_len(),
                project.identity.as_ref().unwrap().internal_header_offset,
            )
            .unwrap();
        assert_eq!(
            project
                .load_native_level_assets(
                    0x105,
                    layout,
                    &profile.sprite_lengths,
                    &profile.exanimation_double_size_modes,
                )
                .unwrap(),
            assets
        );
        assert!(project.identity.unwrap().checksum_matches());

        for path in [input, output, profile_path, asset] {
            fs::remove_file(path).unwrap();
        }
    }
}
