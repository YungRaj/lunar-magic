use lm_project::{NativeLevelAssetsFile, NativeLevelAssetsLayout, Project};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, pc_to_snes};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn pointer(bytes: &mut [u8], offset: usize, mapper: Mapper, target: usize) {
    let value = pc_to_snes(mapper, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&value[..3]);
}

fn fixture(profile: &lm_profile::RevisionProfile) -> Vec<u8> {
    let mut bytes = vec![0xff; 0x40_8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x32;
    bytes[0x7fd6] = 2;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
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
            pointer(
                &mut bytes,
                table.offset + index * table.stride,
                profile.mapper,
                0x1_0000,
            );
        }
    }
    let slot = 0x105;
    pointer(
        &mut bytes,
        profile.level.layer1.pointer_offset(slot).unwrap(),
        profile.mapper,
        0x1_0000,
    );
    pointer(
        &mut bytes,
        profile
            .level
            .sprites
            .low_or_contiguous_table()
            .pointer_offset(slot)
            .unwrap(),
        profile.mapper,
        0x1_0100,
    );
    pointer(
        &mut bytes,
        profile.palette.pointers.pointer_offset(slot).unwrap(),
        profile.mapper,
        0x1_0200,
    );
    pointer(
        &mut bytes,
        profile.exanimation.pointers.pointer_offset(slot).unwrap(),
        profile.mapper,
        0x1_0400,
    );
    bytes[0x1_0000..0x1_0009].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]);
    bytes[0x1_0100..0x1_0106].copy_from_slice(&[0x10, 0, 1, 2, 0xff, 0xfe]);
    bytes[0x1_0200..0x1_0400].fill(0);
    bytes[0x1_0400..0x1_0427].fill(0);
    let settings = profile.expanded_settings.unwrap();
    let settings_offset = settings.table_offset + slot * settings.stride;
    bytes[settings_offset..settings_offset + 32].fill(0x5a);
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

#[test]
fn built_cli_profile_export_and_import_round_trip_complete_native_assets() {
    let directory = std::env::temp_dir().join(format!(
        "lm-native-profile-process-{}-{}-日本語",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source game.smc");
    let profile_path = directory.join("revision profile.lmrev");
    let assets = directory.join("exported level assets.lmna");
    let imported = directory.join("imported game.smc");
    let profile = lm_profile::test_support::profile();
    fs::write(&source, fixture(&profile)).unwrap();
    fs::write(&profile_path, profile.encode()).unwrap();

    let export = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args([
            "profile-export",
            "native-assets",
            source.to_str().unwrap(),
            profile_path.to_str().unwrap(),
            "105",
            assets.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let exported = NativeLevelAssetsFile::decode(
        &fs::read(&assets).unwrap(),
        &profile.sprite_lengths,
        profile.exanimation.maximum_records,
        &profile.exanimation_double_size_modes,
    )
    .unwrap();

    let import = || {
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args([
                "profile-import",
                "native-assets",
                source.to_str().unwrap(),
                imported.to_str().unwrap(),
                profile_path.to_str().unwrap(),
                "105",
                assets.to_str().unwrap(),
                "30000",
                "40000",
            ])
            .output()
            .unwrap()
    };
    let first = import();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(&imported).unwrap()).unwrap())
            .unwrap();
    let reopened = project
        .load_native_level_assets(
            0x105,
            NativeLevelAssetsLayout {
                level: profile.level,
                palette: profile.palette,
                exanimation: profile.exanimation,
                expanded_settings: profile.expanded_settings,
            },
            &profile.sprite_lengths,
            &profile.exanimation_double_size_modes,
        )
        .unwrap();
    assert_eq!(reopened, exported.assets);
    assert!(project.identity.unwrap().checksum_matches());
    let preserved = fs::read(&imported).unwrap();
    assert!(!import().status.success());
    assert_eq!(fs::read(&imported).unwrap(), preserved);
    fs::remove_dir_all(directory).unwrap();
}
