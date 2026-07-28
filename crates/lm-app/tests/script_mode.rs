use lm_graphics::{
    Bgr555, CompactExAnimation, ExAnimationRecord, GraphicsFile4bpp, GraphicsInterchangeFile,
    IndexedTile, Palette, PaletteInterchangeFile,
};
use lm_level::{
    AppearanceSource, CompleteLevelFile, CustomObjectLibrary, EntityAppearanceFile,
    EntityAppearanceRecord, ExpandedLevelSettingsRecord, Layer3Data, Layer3File, Layer3Settings,
    LayerData, Level, LevelObjectData, Map16Page, Map16PageFile, Map16Set, Map16SetFile, Map16Tile,
    MwlFile, MwlLevelHeaderSection, MwlSection, MwlSectionKind, NativeLevelFile,
    NativeSpriteStream, S16Sidecar, SpriteLengthTable,
};
use lm_overworld::{
    EventNumberMap, EventReveal, EventRevealTable, EventTilemapBuffers, OverworldLevelName,
    OverworldMessage, OverworldMetadata, OverworldPathGraph, PathDirection, PathEdge, PathNode,
    SpecialEventRevealTable, SpriteAppearanceDefinition, SpriteAppearanceFile,
    SpriteAppearancePart, Submap, decode_native_overworld_message_file,
    encode_native_overworld_message_file,
};
use lm_profile::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, smw_us_v1_event_tilemap_locator,
    smw_us_v1_overworld_event_number_map_locator, smw_us_v1_overworld_event_reveal_locator,
    smw_us_v1_overworld_message_patch_locator, smw_us_v1_special_event_reveal_locator,
};
use lm_project::{EventTilemapCompression, MwlOptionalLevelAssets, Project};
use lm_rats::{parse_at, scan};
use lm_rom::{RomImage, detect_identity};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SCRIPT: AtomicU64 = AtomicU64::new(0);
const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

fn pristine_smw_us_rom_bytes() -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in [
        "Super Mario World (USA).sfc",
        "SMW-working.sfc",
        "sysLMRestore/smwOrig.smc",
    ] {
        let Ok(bytes) = fs::read(root.join(relative)) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes.clone()) else {
            continue;
        };
        if lm_oracle::sha256_hex(image.logical_bytes()) == PRISTINE_SMW_US_SHA256 {
            return bytes;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

fn script_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "lm-app-script-{}-{}.txt",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn renderable_level() -> Level {
    Level {
        layer1: LayerData {
            raw_tilemap: vec![0],
            ..LayerData::default()
        },
        layer2: LayerData {
            raw_tilemap: vec![0],
            ..LayerData::default()
        },
        ..Level::default()
    }
}

fn write_render_assets(directory: &std::path::Path) -> PathBuf {
    fs::write(
        directory.join("map16.lm16set"),
        Map16SetFile {
            set: Map16Set {
                pages: vec![
                    Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
                ],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("graphics.lmgfx"),
        GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT])],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("palette.lmpal"),
        PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0); 128],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let spec = directory.join("render spec.txt");
    fs::write(&spec, "LMBNDR1\nmap16 map16.lm16set\ngraphics graphics.lmgfx\npalette palette.lmpal\noutput rendered preview.png\nlayer1-width 1\nlayer1-height 1\nlayer2-width 1\nlayer2-height 1\n").unwrap();
    spec
}

#[test]
fn command_script_drives_the_real_binary_and_propagates_errors() {
    let valid = script_path();
    fs::write(&valid, "status\nquit\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&valid)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("No ROM open"));

    let elevated = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--allow-in-place-rom-write")
        .arg("--script")
        .arg(&valid)
        .output()
        .unwrap();
    assert!(elevated.status.success());
    assert!(
        String::from_utf8_lossy(&elevated.stdout)
            .contains("in-place ROM replacement is explicitly enabled")
    );
    let duplicate = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--allow-in-place-rom-write")
        .arg("--allow-in-place-rom-write")
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("Duplicate"));
    fs::remove_file(valid).unwrap();

    let invalid = script_path();
    fs::write(&invalid, "open\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&invalid)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid shell command"));
    fs::remove_file(invalid).unwrap();
}

#[test]
fn command_script_installs_expanded_settings_and_saves_a_new_rom() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!(
        "lm-app-expanded-settings-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.smc");
    let output = directory.join("installed output.smc");
    let commands = directory.join("commands.txt");
    let original = fs::read(
        root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
    )
    .unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\nexpanded-settings-install\nsave-as {}\nquit\n",
            input.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    parse_at(
        image.logical_bytes(),
        SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
    )
    .unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_complete_layer3_runtime_and_reopens_checksum_valid() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-layer3-runtime-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.smc");
    let output = directory.join("installed output.smc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\nlayer3-install\nundo\nredo\nsave-as {}\nquit\n",
            input.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert_eq!(image.logical_len(), 0x10_0000);
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let blocks = scan(image.logical_bytes());
    assert_eq!(blocks.len(), 6);
    assert_eq!(
        blocks
            .iter()
            .map(|block| block.payload.len())
            .collect::<Vec<_>>(),
        [
            0x4c0,
            0x3d0,
            0x20,
            0x20,
            0x370,
            SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN
        ]
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_exports_and_saves_native_overworld_messages() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-overworld-messages-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.sfc");
    let artifact = directory.join("messages input.lmowmsg");
    let exported = directory.join("messages exported.lmowmsg");
    let output = directory.join("installed output.sfc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    let messages: Vec<_> = (0_usize..200)
        .map(|index| {
            let mut bytes = [0x1f; OverworldMessage::ENCODED_LEN];
            bytes[0] = u8::try_from(index % 0xfd).unwrap();
            OverworldMessage(bytes)
        })
        .collect();
    fs::write(
        &artifact,
        encode_native_overworld_message_file(&messages).unwrap(),
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\noverworld-native-message-import {}\nundo\nredo\noverworld-native-message-export {}\nsave-as {}\nquit\n",
            input.display(),
            artifact.display(),
            exported.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(
        decode_native_overworld_message_file(&fs::read(exported).unwrap()).unwrap(),
        messages
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(output).unwrap()).unwrap()).unwrap();
    assert_eq!(
        project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .unwrap()
            .messages,
        messages
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_exports_and_saves_native_overworld_event_reveals() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-overworld-events-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.sfc");
    let artifact = directory.join("events input.lmowevt");
    let exported = directory.join("events exported.lmowevt");
    let output = directory.join("installed output.sfc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    let table = EventRevealTable {
        entries: (0_u16..200)
            .map(|index| EventReveal {
                source_tile: index,
                destination_tile: index | 0x0200,
            })
            .collect(),
    };
    fs::write(&artifact, table.encode_native_event_file().unwrap()).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\noverworld-native-event-import {}\nundo\nredo\noverworld-native-event-export {}\nsave-as {}\nquit\n",
            input.display(),
            artifact.display(),
            exported.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(
        EventRevealTable::decode_native_event_file(&fs::read(exported).unwrap()).unwrap(),
        table
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(output).unwrap()).unwrap()).unwrap();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    assert_eq!(
        project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap()
            .table,
        table
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_exports_and_saves_native_overworld_event_map() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-overworld-event-map-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.sfc");
    let artifact = directory.join("event map input.lmowmap");
    let exported = directory.join("event map exported.lmowmap");
    let output = directory.join("installed output.sfc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    let mut map = EventNumberMap::decode_legacy_pairs(&[
        0x28, 3, 0x4d, 1, 0x52, 1, 0x53, 1, 0x5b, 8, 0x5c, 2, 0x57, 4, 0x30, 1,
    ])
    .unwrap();
    map.set(0xff, 0x7e);
    fs::write(&artifact, map.encode_native_file().unwrap()).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\noverworld-native-event-map-import {}\nundo\nredo\noverworld-native-event-map-export {}\nsave-as {}\nquit\n",
            input.display(),
            artifact.display(),
            exported.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(
        EventNumberMap::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        map
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(output).unwrap()).unwrap()).unwrap();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    assert_eq!(
        project
            .load_overworld_event_number_map_detected(
                smw_us_v1_overworld_event_number_map_locator()
            )
            .unwrap()
            .map,
        map
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_exports_and_saves_native_special_events() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-special-events-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.sfc");
    let artifact = directory.join("special events input.lmowspc");
    let exported = directory.join("special events exported.lmowspc");
    let output = directory.join("installed output.sfc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    let mut table = SpecialEventRevealTable::default();
    for index in 0_u16..24 {
        table.reveals[usize::from(index)] = EventReveal {
            source_tile: index + 0x100,
            destination_tile: index + 0x300,
        };
        table.directions[usize::from(index)] = index.to_le_bytes()[0];
    }
    fs::write(&artifact, table.encode_native_file().unwrap()).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\noverworld-native-special-event-import {}\nundo\nredo\noverworld-native-special-event-export {}\nsave-as {}\nquit\n",
            input.display(),
            artifact.display(),
            exported.display(),
            output.display()
        ),
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(
        SpecialEventRevealTable::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        table
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(output).unwrap()).unwrap()).unwrap();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    assert_eq!(
        project
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table,
        table
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_installs_exports_and_saves_native_event_tilemaps() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-event-tilemaps-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.sfc");
    let artifact = directory.join("event tilemaps input.lmowtil");
    let exported = directory.join("event tilemaps exported.lmowtil");
    let output = directory.join("installed output.sfc");
    let commands = directory.join("commands.txt");
    let original = pristine_smw_us_rom_bytes();
    fs::write(&input, &original).unwrap();
    let mut buffers = EventTilemapBuffers::default();
    buffers.primary_bytes_mut()[7] = 0x12;
    buffers.primary_bytes_mut()[0x807] = 0x34;
    buffers.secondary_high_bytes_mut()[9] = 0xab;
    fs::write(&artifact, buffers.encode_native_file()).unwrap();
    fs::write(
        &commands,
        format!(
            "open {}\noverworld-native-event-tilemap-import {}\nundo\nredo\noverworld-native-event-tilemap-export {}\nsave-as {}\nquit\n",
            input.display(),
            artifact.display(),
            exported.display(),
            output.display()
        ),
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(
        EventTilemapBuffers::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        buffers
    );
    let project =
        Project::open_supported(RomImage::from_bytes(fs::read(output).unwrap()).unwrap()).unwrap();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    assert_eq!(
        project
            .load_event_tilemap_buffers_detected(
                smw_us_v1_event_tilemap_locator(),
                EventTilemapCompression::Lz2,
            )
            .unwrap()
            .buffers,
        buffers
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_paired_custom_object_history() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-custom-object-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let data = directory.join("Objects 日本語.mw0");
    let descriptions = directory.join("Objects 日本語.mw0t");
    let edits = directory.join("Object edits.txt");
    let commands = directory.join("Commands.txt");
    fs::write(&data, [1, 0, 3, 0xff]).unwrap();
    fs::write(&descriptions, b"Original\n").unwrap();
    fs::write(
        &edits,
        "LMCUSED1\nreplace 0 020004 4368616e676564\nformat bom crlf trailing\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "custom-open {}\ncustom-edit {}\ncustom-undo\ncustom-redo\ncustom-status\ncustom-save\ncustom-close\nquit\n",
            data.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("custom-object undo: applied"));
    assert!(stdout.contains("custom-object redo: applied"));
    let saved =
        CustomObjectLibrary::decode(&fs::read(&data).unwrap(), &fs::read(&descriptions).unwrap())
            .unwrap();
    assert_eq!(saved.entries()[0].description, "Changed");
    assert!(saved.description_format().utf8_bom);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_complete_level_document_lifecycle() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-bundle-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level.lmlevel");
    let edits = directory.join("edits.lmedit");
    let commands = directory.join("commands.txt");
    fs::write(
        &document,
        CompleteLevelFile(renderable_level()).encode().unwrap(),
    )
    .unwrap();
    fs::write(
        &edits,
        "LMAUXED1\nscreen-exit-insert 0 0x1234\nmap16-upsert 0x20 1 2 3 4 5\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "bundle-open {}\nbundle-edit-file {}\nbundle-undo\nbundle-redo\nbundle-render-file {}\nbundle-save\nbundle-close\nquit\n",
            document.display(),
            edits.display(),
            write_render_assets(&directory).display()
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("complete level undo: applied"));
    assert!(stdout.contains("complete level redo: applied"));
    let saved = CompleteLevelFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.0.screen_exits[0].encoded, 0x1234);
    assert_eq!(saved.0.map16_overrides[0].0, 0x20);
    assert!(directory.join("rendered preview.png").is_file());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_renders_complete_level_with_dsc_context() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-bundle-dsc-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level 日本語.lmlevel");
    let dsc = directory.join("custom display 日本語.dsc");
    let spec = write_render_assets(&directory);
    let commands = directory.join("commands.txt");
    fs::write(
        &document,
        CompleteLevelFile(renderable_level()).encode().unwrap(),
    )
    .unwrap();
    fs::write(&dsc, b"0\t8\tblended\n").unwrap();
    fs::write(
        &spec,
        "LMBNDR1\nmap16 map16.lm16set\ngraphics graphics.lmgfx\npalette palette.lmpal\ndsc custom display 日本語.dsc\ndsc-custom-display 1\ndsc-special-markers 1\ndsc-first-feature 0\ndsc-first-suppressed 0\ndsc-second-feature 0\ndsc-level-mode 0\noutput rendered DSC preview.png\nlayer1-width 1\nlayer1-height 1\nlayer2-width 1\nlayer2-height 1\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "bundle-open {}\nbundle-render-file {}\nbundle-close\nquit\n",
            document.display(),
            spec.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(directory.join("rendered DSC preview.png").is_file());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_paired_custom_sprite_lifecycle() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-custom-sprite-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let data = directory.join("Sprite placements 日本語.mw2");
    let descriptions = directory.join("Sprite placements 日本語.mwt");
    let lengths = directory.join("Sprite lengths.bin");
    let spec = directory.join("Open sprites.txt");
    let edits = directory.join("Sprite edits.txt");
    let commands = directory.join("Sprite commands.txt");
    fs::write(&data, [0x5a, 1, 2, 3, 5, 4, 5, 0xff]).unwrap();
    fs::write(&descriptions, b"First\nSecond\n").unwrap();
    fs::write(&lengths, [3; SpriteLengthTable::ENCODED_LEN]).unwrap();
    fs::write(
        &spec,
        "LMSPDOC1\ndata Sprite placements 日本語.mw2\nsprite-lengths Sprite lengths.bin\n",
    )
    .unwrap();
    fs::write(
        &edits,
        "LMSPRED1\nreplace 0 010809+000a0b 50616972\nheader 44\nformat no-bom lf trailing\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "custom-sprite-open {}\ncustom-sprite-edit {}\ncustom-sprite-undo\ncustom-sprite-redo\ncustom-sprite-status\ncustom-sprite-save\ncustom-sprite-close\nquit\n",
            spec.display(),
            edits.display()
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("custom-sprite undo: applied"));
    assert!(stdout.contains("custom-sprite redo: applied"));
    let decoded = lm_level::CustomSpriteLibrary::decode(
        &fs::read(&data).unwrap(),
        &fs::read(&descriptions).unwrap(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(decoded.header(), 0x44);
    assert_eq!(decoded.entries()[0].sprites.len(), 2);
    assert_eq!(decoded.entries()[0].description, "Pair");
    assert!(!decoded.description_format().utf8_bom);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_edits_and_canonically_saves_native_s16() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-native-s16-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Sprites 日本語.s16");
    let spec = directory.join("Open sidecar.txt");
    let edits = directory.join("Sidecar edits.txt");
    let commands = directory.join("Sidecar commands.txt");
    fs::write(&document, [0; S16Sidecar::BLOCK_LEN]).unwrap();
    fs::write(&spec, "LMN16DC1\nkind s16\nfile Sprites 日本語.s16\n").unwrap();
    fs::write(&edits, "LMN16ED1\nset 0 44332211\nset 200 2\n").unwrap();
    fs::write(
        &commands,
        format!(
            "native-sidecar-open {}\nnative-sidecar-edit {}\nnative-sidecar-undo\nnative-sidecar-redo\nnative-sidecar-status\nnative-sidecar-save\nnative-sidecar-close\nquit\n",
            spec.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("native Map16 sidecar undo: applied"));
    assert!(stdout.contains("native Map16 sidecar redo: applied"));
    let bytes = fs::read(&document).unwrap();
    assert_eq!(bytes.len(), 0x1000);
    let decoded = S16Sidecar::decode(&bytes).unwrap();
    assert_eq!(decoded.entry(0), Some(0x4433_2211));
    assert_eq!(decoded.entry(0x200), Some(2));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_replaces_and_losslessly_saves_dsc_sidecar() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-dsc-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Display names 日本語.dsc");
    let replacement = directory.join("Replacement 日本語.dsc");
    let commands = directory.join("DSC commands.txt");
    fs::write(&document, b"10\t0\tOld\r\n").unwrap();
    let replacement_bytes = b"\xef\xbb\xbf10\t28\tNew\\nName\\b112233\r\n11\t2\t1234\r\n";
    fs::write(&replacement, replacement_bytes).unwrap();
    fs::write(
        &commands,
        format!(
            "dsc-open {}\ndsc-replace {}\ndsc-undo\ndsc-redo\ndsc-status\ndsc-save\ndsc-close\nquit\n",
            document.display(),
            replacement.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DSC undo: applied"));
    assert!(stdout.contains("DSC redo: applied"));
    assert_eq!(fs::read(&document).unwrap(), replacement_bytes);
    let decoded = lm_level::DscSidecar::decode(replacement_bytes).unwrap();
    assert_eq!(decoded.entries().len(), 2);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn dirty_portable_document_blocks_eof_and_requires_explicit_quit_discard() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-dirty-bundle-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level.lmlevel");
    let edits = directory.join("edits.lmedit");
    let blocked = directory.join("blocked.txt");
    let discarded = directory.join("discarded.txt");
    let original = CompleteLevelFile(Level::default()).encode().unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(&edits, "LMAUXED1\nscreen-exit-insert 0 0x1234\n").unwrap();
    let edit_commands = format!(
        "bundle-open {}\nbundle-edit-file {}\n",
        document.display(),
        edits.display()
    );
    fs::write(&blocked, &edit_commands).unwrap();
    fs::write(&discarded, format!("{edit_commands}quit\nyes\n")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&blocked)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsaved portable documents"));

    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&discarded)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}

fn mwl_file() -> MwlFile {
    let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
        std::array::from_fn(|_| MwlSection::default());
    sections[MwlSectionKind::LevelHeader as usize].bytes =
        vec![0; MwlLevelHeaderSection::ENCODED_LEN];
    sections[MwlSectionKind::Layer1 as usize].bytes = vec![1, 2, 3];
    MwlFile {
        version: MwlFile::CURRENT_VERSION,
        flags: 0,
        attribution: [0x20; MwlFile::ATTRIBUTION_LEN],
        sections,
    }
}

#[test]
fn command_script_imports_typed_mwl_optional_assets_as_one_edit() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-mwl-optional-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("target level.mwl");
    let source = directory.join("source 日本語.mwl");
    let modes = directory.join("size modes.bin");
    let spec = directory.join("import optional assets.txt");
    let edits = directory.join("semantic optional edits.txt");
    let edit_spec = directory.join("semantic edit options.txt");
    let commands = directory.join("commands.txt");
    let assets = MwlOptionalLevelAssets {
        palette_metadata: [7, 0x10_8031],
        palette: Palette {
            colors: (0_u16..257).map(Bgr555).collect(),
        },
        exanimation_metadata: [0, 0x10_97e9],
        exanimation: Some(CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap()],
        }),
    };
    let mut source_file = MwlFile::default();
    assets
        .install_into(&mut source_file, &[false; 256])
        .unwrap();
    fs::write(&document, mwl_file().encode().unwrap()).unwrap();
    fs::write(&source, source_file.encode().unwrap()).unwrap();
    fs::write(&modes, [0; 256]).unwrap();
    fs::write(
        &spec,
        "LMMWLOPT1\nsource source 日本語.mwl\nsize-modes size modes.bin\nmaximum-records 32\n",
    )
    .unwrap();
    fs::write(
        &edits,
        "LMMWLOE1\npalette-color 256 1234\nexanimation-globals 09 0000000A\ntrigger 3 07\nframe-replace 0 0 1234\n",
    )
    .unwrap();
    fs::write(
        &edit_spec,
        "LMMWLOES1\nedits semantic optional edits.txt\nsize-modes size modes.bin\nmaximum-records 32\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "mwl-open {}\nmwl-import-optional-assets-file {}\nmwl-edit-optional-assets-file {}\nmwl-undo\nmwl-redo\nmwl-save\nmwl-close\nquit\n",
            document.display(),
            spec.display(),
            edit_spec.display()
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("MWL undo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("MWL redo: applied"));
    let saved = MwlFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.section(MwlSectionKind::Layer1), &[1, 2, 3]);
    let edited = MwlOptionalLevelAssets::decode(&saved, 32, &[false; 256]).unwrap();
    assert_eq!(edited.palette.colors[256], Bgr555(0x1234));
    assert_eq!(edited.exanimation.as_ref().unwrap().setting, 9);
    assert_eq!(edited.exanimation.as_ref().unwrap().header_value, 10);
    assert_eq!(edited.exanimation.as_ref().unwrap().trigger_values[3], 7);
    assert_eq!(
        edited.exanimation.as_ref().unwrap().records[0].frame_bytes(false),
        [0x34, 0x12]
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_mwl_lifecycle_and_dirty_eof_policy() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-mwl-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level with spaces.mwl");
    let edits = directory.join("MWL edits.txt");
    let save_commands = directory.join("save commands.txt");
    let dirty_commands = directory.join("dirty commands.txt");
    fs::write(&document, mwl_file().encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LMWLEDT1\nflags 12345678\nlevel 1ab\nsection layer1 aabbccdd\n",
    )
    .unwrap();
    let edit_commands = format!(
        "mwl-open {}\nmwl-edit-file {}\nmwl-undo\nmwl-redo\nmwl-status\n",
        document.display(),
        edits.display()
    );
    fs::write(
        &save_commands,
        format!("{edit_commands}mwl-save\nmwl-close\nquit\n"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&save_commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("MWL document saved"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("MWL undo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("MWL redo: applied"));
    let saved = MwlFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.flags, 0x1234_5678);
    assert_eq!(
        saved.sections[MwlSectionKind::Layer1 as usize].bytes,
        [0xaa, 0xbb, 0xcc, 0xdd]
    );
    assert_eq!(
        MwlLevelHeaderSection::decode(&saved.sections[MwlSectionKind::LevelHeader as usize].bytes)
            .unwrap()
            .level_number(),
        0x01ab
    );

    fs::write(&edits, "LMWLEDT1\nflags fedcba98\n").unwrap();
    fs::write(&dirty_commands, &edit_commands).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&dirty_commands)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MWL level"));
    assert_eq!(
        MwlFile::decode(&fs::read(&document).unwrap())
            .unwrap()
            .flags,
        0x1234_5678
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_interpretation_bound_native_level_document() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-native-level-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let lengths = SpriteLengthTable::standard();
    let value = NativeLevelFile {
        source_level: 0x105,
        layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
        sprites: NativeSpriteStream::parse(&[0x10, 0, 0x20, 1, 0xff], false, &lengths).unwrap(),
    };
    let document = directory.join("Native Level 日本語.lmlvl");
    let spec = directory.join("Open Native Level.txt");
    let edits = directory.join("Edit Native Level.txt");
    let commands = directory.join("Commands.txt");
    fs::write(&document, value.encode().unwrap()).unwrap();
    fs::write(
        &spec,
        "LMNLDOC1\nlevel Native Level 日本語.lmlvl\nsprite-lengths standard\n",
    )
    .unwrap();
    fs::write(
        &edits,
        "LMLEDIT1\nheader mode 1f\nsprite-header 44\nobject insert 1 030405\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "native-level-open {}\nnative-level-edit-file {}\nnative-level-undo\nnative-level-redo\nnative-level-status\nnative-level-save\nnative-level-close\nquit\n",
            spec.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-level document saved"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-level undo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-level redo: applied"));
    let saved = NativeLevelFile::decode(&fs::read(&document).unwrap(), &lengths).unwrap();
    assert_eq!(saved.source_level, 0x105);
    assert_eq!(saved.layer1.header.level_mode(), 0x1f);
    assert_eq!(saved.layer1.objects.records.len(), 2);
    assert_eq!(saved.sprites.header, 0x44);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_standalone_map16_page_document() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-map16-page-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let value = Map16PageFile {
        source_page: 0x12,
        page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
    };
    let document = directory.join("Map16 Page 日本語.map16");
    let edits = directory.join("Map16 Page Edits.txt");
    let render_spec = directory.join("Map16 Page Render.txt");
    let commands = directory.join("Commands.txt");
    fs::write(&document, value.encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LMPGEDT1\ntile 01 0 0 0 0 abcd\nsubtile 01 br 8000\nacts-like 02 ffff\n",
    )
    .unwrap();
    write_render_assets(&directory);
    fs::write(
        &render_spec,
        "LMPGDR1\ngraphics graphics.lmgfx\npalette palette.lmpal\noutput Map16 Page Preview.png\nviewport-origin-x -1\nviewport-origin-y 0\nviewport-width 18\nviewport-height 19\nzoom-numerator 2\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "map16-page-open {}\nmap16-page-edit-file {}\nmap16-page-undo\nmap16-page-redo\nmap16-page-render-file {}\nmap16-page-status\nmap16-page-save\nmap16-page-close\nquit\n",
            document.display(),
            edits.display(),
            render_spec.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Map16 page document saved"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Map16 page undo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Map16 page redo: applied"));
    let saved = Map16PageFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.source_page, 0x12);
    assert_eq!(saved.page.tiles[1].bottom_right.0, 0x8000);
    assert_eq!(saved.page.tiles[2].acts_like, 0xffff);
    let preview = fs::read(directory.join("Map16 Page Preview.png")).unwrap();
    assert_eq!(preview.get(16..20), Some(18_u32.to_be_bytes().as_slice()));
    assert_eq!(preview.get(20..24), Some(19_u32.to_be_bytes().as_slice()));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_layer3_history_and_save() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-layer3-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Layer 3 日本語.lmlayer3");
    let edits = directory.join("Layer 3 edits.txt");
    let commands = directory.join("Commands.txt");
    let value = Layer3File(Layer3Data {
        settings: Layer3Settings::default(),
        tilemap: vec![0, 1, 2, 3],
        remap_commands: vec![0xfe, 7],
    });
    fs::write(&document, value.encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LML3EDT1\nstart 2a\nflags 80\ntilemap-range 1 aabb\nremap fe0708\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "layer3-open {}\nlayer3-edit-file {}\nlayer3-undo\nlayer3-redo\nlayer3-status\nlayer3-save\nlayer3-close\nquit\n",
            document.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Layer 3 undo applied"));
    assert!(stdout.contains("Layer 3 redo applied"));
    assert!(stdout.contains("Layer 3 document saved"));
    let saved = Layer3File::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.0.settings.start_position, 0x2a);
    assert_eq!(saved.0.tilemap, [0, 0xaa, 0xbb, 3]);
    assert_eq!(saved.0.remap_commands, [0xfe, 7, 8]);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_expanded_settings_history_and_save() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-expanded-settings-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Expanded Settings 日本語.bin");
    let edits = directory.join("Expanded Settings edits.txt");
    let commands = directory.join("Commands.txt");
    fs::write(&document, [0; ExpandedLevelSettingsRecord::ENCODED_LEN]).unwrap();
    fs::write(&edits, "LMXSETED1\nword 2 1234\nword f abcd\n").unwrap();
    fs::write(
        &commands,
        format!(
            "expanded-settings-open {}\nexpanded-settings-edit-file {}\nexpanded-settings-undo\nexpanded-settings-redo\nexpanded-settings-status\nexpanded-settings-save\nexpanded-settings-close\nquit\n",
            document.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("expanded-settings undo: applied"));
    assert!(stdout.contains("expanded-settings redo: applied"));
    assert!(stdout.contains("expanded-settings document saved"));
    let saved = ExpandedLevelSettingsRecord::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.word(2).unwrap(), 0x1234);
    assert_eq!(saved.word(0xf).unwrap(), 0xabcd);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_entity_appearance_document() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-entity-script-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Entity 日本語.lmentapp");
    let edits = directory.join("Entity edits.txt");
    let commands = directory.join("Commands.txt");
    let value = EntityAppearanceFile {
        appearances: vec![EntityAppearanceRecord {
            source: AppearanceSource::Sprite(1),
            tile_index: 2,
            palette_index: 3,
            x: 4,
            y: 5,
            x_flip: false,
            y_flip: false,
        }],
    };
    fs::write(&document, value.encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LMENTED1\nreplace 0 layer1 10 20 4 -8 9 1 0\ninsert 1 sprite 11 21 5 10 11 0 1\n",
    )
    .unwrap();
    fs::write(&commands, format!("entity-app-open {}\nentity-app-edit-file {}\nentity-app-undo\nentity-app-redo\nentity-app-status\nentity-app-save\nentity-app-close\nquit\n", document.display(), edits.display())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("entity appearance undo: applied"));
    assert!(stdout.contains("entity appearance redo: applied"));
    assert!(stdout.contains("entity appearance document saved"));
    let saved = EntityAppearanceFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.appearances.len(), 2);
    assert_eq!(saved.appearances[0].x, -8);
    assert_eq!(saved.appearances[1].source, AppearanceSource::Sprite(0x11));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_overworld_appearance_document() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-world-appearance-script-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Sprite Appearances.lmowapp");
    let edits = directory.join("Sprite edits.txt");
    let commands = directory.join("Commands.txt");
    let value = SpriteAppearanceFile {
        definitions: vec![SpriteAppearanceDefinition {
            sprite_id: 1,
            parts: vec![SpriteAppearancePart {
                tile_index: 2,
                palette_index: 3,
                x_offset: 4,
                y_offset: 5,
                x_flip: false,
                y_flip: false,
            }],
        }],
    };
    fs::write(&document, value.encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LMOWAED1\ndefinition insert 1 10\npart insert 10 0 123 4 -8 16 1 0\npart insert 10 1 124 5 9 -10 0 1\ndefinition move 10 1\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "world-app-open {}\nworld-app-edit-file {}\nworld-app-undo\nworld-app-redo\nworld-app-status\nworld-app-save\nworld-app-close\nquit\n",
            document.display(),
            edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("overworld appearance undo: applied"));
    assert!(stdout.contains("overworld appearance redo: applied"));
    assert!(stdout.contains("overworld appearance document saved"));
    let saved = SpriteAppearanceFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.definitions.len(), 2);
    assert_eq!(saved.definitions[0].sprite_id, 0x10);
    assert_eq!(saved.definitions[0].parts.len(), 2);
    assert_eq!(saved.definitions[0].parts[0].x_offset, -8);
    assert_eq!(saved.definitions[0].parts[1].y_offset, -10);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_script_drives_path_and_metadata_history() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-overworld-support-日本語-{}-{}",
        std::process::id(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path_file = directory.join("Paths 日本語.lmowpath");
    let path_edits = directory.join("Path edits.txt");
    let metadata_file = directory.join("Metadata 日本語.lmowmeta");
    let metadata_edits = directory.join("Metadata edits.txt");
    let commands = directory.join("Commands.txt");
    let mut edge = PathEdge {
        from: 1,
        to: 2,
        direction: PathDirection::Right,
        exit_index: None,
        raw_flags: 0,
    };
    edge.set_one_way(true);
    let paths = OverworldPathGraph {
        nodes: vec![
            PathNode {
                id: 1,
                x: 1,
                y: 2,
                submap: Submap::Main,
                level: Some(0x105),
                raw_flags: 0x80,
            },
            PathNode {
                id: 2,
                x: 3,
                y: 4,
                submap: Submap::Main,
                level: None,
                raw_flags: 0x40,
            },
        ],
        edges: vec![edge],
    };
    let metadata = OverworldMetadata {
        level_names: vec![OverworldLevelName {
            level: 0x105,
            tiles: [0x12; OverworldLevelName::TILE_COUNT],
            raw_flags: 0x80,
        }],
        ..OverworldMetadata::default()
    };
    fs::write(&path_file, paths.encode_file().unwrap()).unwrap();
    fs::write(&path_edits, "LMOPEDT1\nnode upsert 1 9 a 0 105 80\n").unwrap();
    fs::write(&metadata_file, metadata.encode_file().unwrap()).unwrap();
    fs::write(&metadata_edits, "LMOMEDT1\nname remove 105\n").unwrap();
    fs::write(
        &commands,
        format!(
            "path-open {}\npath-edit {}\npath-undo\npath-redo\npath-save\npath-close\nmetadata-open {}\nmetadata-edit {}\nmetadata-undo\nmetadata-redo\nmetadata-save\nmetadata-close\nquit\n",
            path_file.display(),
            path_edits.display(),
            metadata_file.display(),
            metadata_edits.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("overworld path undo: applied"));
    assert!(stdout.contains("overworld path redo: applied"));
    assert!(stdout.contains("overworld metadata undo: applied"));
    assert!(stdout.contains("overworld metadata redo: applied"));
    let saved_paths = OverworldPathGraph::decode_file(&fs::read(&path_file).unwrap()).unwrap();
    assert_eq!((saved_paths.nodes[0].x, saved_paths.nodes[0].y), (9, 10));
    let saved_metadata =
        OverworldMetadata::decode_file(&fs::read(&metadata_file).unwrap()).unwrap();
    assert!(saved_metadata.level_names.is_empty());
    fs::remove_dir_all(directory).unwrap();
}
