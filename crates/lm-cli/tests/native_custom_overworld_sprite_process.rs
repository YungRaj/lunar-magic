use lm_oracle::Observation;
use lm_overworld::{
    CUSTOM_OVERWORLD_SPRITE_ID_COUNT, NativeCustomOverworldSprite, NativeCustomOverworldSpriteTable,
};
use lm_project::{
    NativeCustomOverworldSpriteRomLayout, NativeCustomOverworldSpriteSaveOptions, Project,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum};
use std::{fs, process::Command};

const POINTER: usize = 0x20;
const CHECKSUM: usize = 0x7fdc;

#[test]
fn built_cli_observes_a_transactionally_written_native_stream() {
    let mut bytes = vec![0xff; 0x8000];
    let checksum = compute_snes_checksum(&bytes, CHECKSUM).unwrap().encoded();
    bytes[CHECKSUM..CHECKSUM + checksum.len()].copy_from_slice(&checksum);
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let sizes = [4; CUSTOM_OVERWORLD_SPRITE_ID_COUNT];
    let table = NativeCustomOverworldSpriteTable {
        maps: std::array::from_fn(|map| {
            if map == 4 {
                vec![NativeCustomOverworldSprite {
                    id: 9,
                    x: 0x88,
                    y: 0x120,
                    screen: 0x18,
                    extra: vec![0xde],
                }]
            } else {
                Vec::new()
            }
        }),
    };
    project
        .save_native_custom_overworld_sprites(
            &table,
            &sizes,
            NativeCustomOverworldSpriteRomLayout {
                mapper: Mapper::LoRom,
                pointer_offset: POINTER,
                maximum_payload_len: 0x400,
            },
            &NativeCustomOverworldSpriteSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x100..CHECKSUM,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        ProtectedRange(POINTER..POINTER + 3),
                        ProtectedRange(CHECKSUM..CHECKSUM + 4),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();

    let directory = std::env::temp_dir().join(format!(
        "lm-native-custom-overworld-sprites-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("Custom Sprites.smc");
    let size_file = directory.join("Record Sizes.bin");
    let output = directory.join("Custom Sprites.obs");
    fs::write(&rom, project.save_snapshot()).unwrap();
    fs::write(&size_file, sizes).unwrap();

    let process = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("native-overworld-sprites")
        .arg(&rom)
        .arg("lorom")
        .arg(format!("{POINTER:x}"))
        .arg(&size_file)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        process.status.success(),
        "{}",
        String::from_utf8_lossy(&process.stderr)
    );
    let observed = Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(
        observed.get("overworld/custom-sprites/maps/4/00/id"),
        Some("09")
    );
    assert_eq!(
        observed.get("overworld/custom-sprites/maps/4/00/x"),
        Some("136")
    );
    assert_eq!(
        observed.get("overworld/custom-sprites/maps/4/00/extra"),
        Some("de")
    );
    fs::remove_dir_all(directory).unwrap();
}
