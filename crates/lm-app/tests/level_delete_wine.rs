use lm_project::{ExAnimationRomLayout, LevelPointerTable, NativeLevelAssetsLayout, Project};
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lm-delete-level-oracle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_range_eq(rust: &RomImage, oracle: &RomImage, offset: usize, len: usize) {
    assert_eq!(
        rust.read(offset, len).unwrap(),
        oracle.read(offset, len).unwrap(),
        "oracle mismatch at ${offset:06X}..${:06X}",
        offset + len
    );
}

#[test]
#[ignore = "requires Wine, Lunar Magic 3.63, and the retained legally obtained ROM fixture"]
fn aggregate_delete_matches_lunar_magic_for_every_modeled_level_asset() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::env::var_os("LM_DELETE_SOURCE_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc")
        });
    let lunar_magic = std::env::var_os("LUNAR_MAGIC_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("lm363/Lunar Magic.exe"));
    let wine = std::env::var_os("WINE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wine"));
    let directory = TemporaryDirectory::create();
    fs::copy(&source, directory.0.join("oracle.smc")).unwrap();
    let output = Command::new(wine)
        .arg(lunar_magic)
        .args(["-DeleteLevels", "oracle.smc", "-LevelList", "0"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Deleted 1 level."));

    let source_image = RomImage::from_bytes(fs::read(source).unwrap()).unwrap();
    let oracle = RomImage::from_bytes(fs::read(directory.0.join("oracle.smc")).unwrap()).unwrap();
    let mut project = Project::new(source_image.clone());
    let mut level = lm_profile::smw_us_v1_vanilla_level_layout();
    level.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&source_image).unwrap();
    let layer2 = lm_profile::smw_us_v1_layer2_layout(&source_image).unwrap();
    let expanded = lm_profile::smw_us_v1_installed_expanded_settings_layout(&project)
        .unwrap()
        .or(Some(lm_profile::smw_us_v1_expanded_settings_layout()));
    let layout = NativeLevelAssetsLayout {
        level,
        palette: lm_profile::smw_us_v1_custom_palette_layout(),
        exanimation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x8138b,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        },
        expanded_settings: expanded,
    };
    let deleted = project
        .delete_native_level_assets_to_original_source(
            "delete native level 000",
            layout,
            Some(layer2),
            Some(lm_profile::smw_us_v1_vanilla_entrance_layout()),
            Some(lm_profile::smw_us_v1_lfix3_level_fields_layout()),
            0,
            0x19,
            0x7fdc,
            0x00,
        )
        .unwrap();

    assert_range_eq(
        &project.rom,
        &oracle,
        level.layer1.pointer_offset(0).unwrap(),
        3,
    );
    let (sprite_low, sprite_bank) = level.sprites.pointer_ranges(0).unwrap();
    assert_range_eq(&project.rom, &oracle, sprite_low.start, sprite_low.len());
    if let Some(bank) = sprite_bank {
        assert_range_eq(&project.rom, &oracle, bank.start, bank.len());
    }
    for table in [
        layout.palette.pointers,
        layout.exanimation.pointers,
        layer2.pointers,
    ] {
        assert_range_eq(&project.rom, &oracle, table.pointer_offset(0).unwrap(), 3);
    }
    if let Some(descriptors) = layer2.descriptor_table {
        assert_range_eq(&project.rom, &oracle, descriptors.offset, 1);
    }
    if let Some(settings) = expanded {
        assert_range_eq(
            &project.rom,
            &oracle,
            settings.table_offset,
            lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN,
        );
    }
    for offset in [
        lm_profile::SMW_US_V1_ENTRANCE_POSITION_OFFSET,
        lm_profile::SMW_US_V1_ENTRANCE_VERTICAL_SETTINGS_OFFSET,
        lm_profile::SMW_US_V1_ENTRANCE_SCREEN_AND_METHOD_OFFSET,
        lm_profile::SMW_US_V1_ENTRANCE_LEVEL_MODE_AND_SCREEN_OFFSET,
        lm_profile::SMW_US_V1_LFIX3_FLAGS_OFFSET,
        lm_profile::SMW_US_V1_LFIX3_HIGH_POSITION_OFFSET,
        lm_profile::SMW_US_V1_LFIX3_ADDITIONAL_FLAGS_OFFSET,
        lm_profile::SMW_US_V1_LFIX3_RUNTIME_FLAGS_OFFSET,
    ] {
        assert_range_eq(&project.rom, &oracle, offset, 1);
    }
    let mut oracle_reservations = Vec::new();
    for block in deleted.reclaimed {
        let rust = project
            .rom
            .read(block.header_offset, block.full_range().len())
            .unwrap();
        let original = oracle
            .read(block.header_offset, block.full_range().len())
            .unwrap();
        assert!(rust.iter().all(|byte| *byte == 0));
        if rust != original {
            assert_eq!(&original[..8], b"STAR\xFD\x01\x02\xFE");
            assert!(original[8..].iter().all(|byte| *byte == 0));
            oracle_reservations.push(block.header_offset);
        }
    }
    assert_eq!(oracle_reservations, [0x80000, 0x81ecd]);
}
