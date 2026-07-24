use super::*;

#[test]
fn parses_native_level_inspection() {
    assert_eq!(
        parse_from(
            &vec![
                "level", "game.smc", "lorom", "105", "0x1000", "0x2000", "expanded",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::Level {
            rom: "game.smc".into(),
            mapper: Mapper::LoRom,
            number: 0x105,
            layer1_table: 0x1000,
            sprite_table: 0x2000,
            expanded_sprites: true,
        }
    );
}

#[test]
fn parses_split_bank_level_inspection() {
    let args = [
        "level-split-bank",
        "game.smc",
        "lorom",
        "0x105",
        "0x2e000",
        "0x2ec00",
        "0x77100",
        "legacy",
    ]
    .into_iter()
    .map(Into::into)
    .collect::<Vec<OsString>>();
    assert_eq!(
        parse_from(&args).unwrap(),
        Command::LevelSplitBank {
            rom: "game.smc".into(),
            mapper: Mapper::LoRom,
            number: 0x105,
            layer1_table: 0x2e000,
            sprite_low_table: 0x2ec00,
            sprite_bank_table: 0x77100,
            expanded_sprites: false,
        }
    );
}

#[test]
fn parses_layer2_export() {
    let args = [
        "level-layer2",
        "game.smc",
        "lorom",
        "0x105",
        "0x2e000",
        "0x2e600",
        "layer2.bin",
    ]
    .into_iter()
    .map(Into::into)
    .collect::<Vec<OsString>>();
    assert_eq!(
        parse_from(&args).unwrap(),
        Command::LevelLayer2 {
            rom: "game.smc".into(),
            mapper: Mapper::LoRom,
            number: 0x105,
            layer1_table: 0x2e000,
            layer2_table: 0x2e600,
            output: "layer2.bin".into(),
        }
    );
}

#[test]
fn parses_native_level_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "level-export",
            "game.smc",
            "lorom",
            "105",
            "1000",
            "2000",
            "expanded",
            "lengths.bin",
            "level.lmlvl"
        ]),
        Command::LevelTransfer(LevelTransferCommand::Export {
            level: 0x105,
            expanded_sprites: true,
            sprite_lengths: Some(path),
            ..
        }) if path == std::path::Path::new("lengths.bin")
    ));
    assert!(matches!(
        parse(vec![
            "level-import",
            "in.smc",
            "out.smc",
            "sa1",
            "105",
            "1000",
            "2000",
            "legacy",
            "standard",
            "level.lmlvl",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::LevelTransfer(LevelTransferCommand::Import {
            mapper: Mapper::Sa1,
            level: 0x105,
            expanded_sprites: false,
            sprite_lengths: None,
            checksum_field: 0x7fdc,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}
#[test]
fn parses_map16_inspection() {
    assert_eq!(
        parse_from(
            &vec!["map16", "game.smc", "sa1", "10", "0x300", "0x600"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::Map16 {
            rom: "game.smc".into(),
            mapper: Mapper::Sa1,
            page: 0x10,
            graphics_table: 0x300,
            acts_like_table: 0x600,
            observation: None,
        }
    );
    assert!(matches!(
        parse_from(
            &vec![
                "map16", "game.smc", "sa1", "10", "0x300", "0x600", "page.obs",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::Map16 {
            observation: Some(path),
            ..
        } if path == std::path::Path::new("page.obs")
    ));
}
#[test]
fn parses_map16_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "map16-export",
            "game.smc",
            "lorom",
            "10",
            "1000",
            "2000",
            "page.map16"
        ]),
        Command::Map16Transfer(Map16TransferCommand::Export { page: 0x10, .. })
    ));
    assert!(matches!(
        parse(vec![
            "map16-import",
            "in.smc",
            "out.smc",
            "sa1",
            "10",
            "1000",
            "2000",
            "page.map16",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::Map16Transfer(Map16TransferCommand::Import {
            mapper: Mapper::Sa1,
            page: 0x10,
            checksum_field: 0x7fdc,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}

#[test]
fn parses_graphics_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "graphics-export",
            "game.smc",
            "lorom",
            "32",
            "1000",
            "8000",
            "10000",
            "gfx.lmgfx"
        ]),
        Command::GraphicsTransfer(GraphicsTransferCommand::Export { slot: 0x32, .. })
    ));
    assert!(matches!(
        parse(vec![
            "graphics-import",
            "in.smc",
            "out.smc",
            "exlorom",
            "32",
            "1000",
            "8000",
            "10000",
            "gfx.lmgfx",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::GraphicsTransfer(GraphicsTransferCommand::Import {
            mapper: Mapper::ExLoRom,
            slot: 0x32,
            maximum_decompressed_len: 0x1_0000,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}

#[test]
fn parses_explicit_lz3_graphics_workflows() {
    assert!(matches!(
        parse_from(&[
            "graphics-export".into(),
            "game.smc".into(),
            "lorom".into(),
            "32".into(),
            "1000".into(),
            "8000".into(),
            "10000".into(),
            "lz3".into(),
            "gfx.lmgfx".into(),
        ])
        .unwrap(),
        Command::GraphicsTransfer(GraphicsTransferCommand::Export {
            compression: lm_project::GraphicsCompression::Lz3,
            ..
        })
    ));
    assert!(matches!(
        parse_from(&[
            "graphics-import".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "32".into(),
            "1000".into(),
            "8000".into(),
            "10000".into(),
            "lz3".into(),
            "gfx.lmgfx".into(),
            "7fdc".into(),
            "100000".into(),
            "200000".into(),
        ])
        .unwrap(),
        Command::GraphicsTransfer(GraphicsTransferCommand::Import {
            compression: lm_project::GraphicsCompression::Lz3,
            ..
        })
    ));
    assert!(matches!(
        parse_from(&[
            "graphics".into(),
            "game.smc".into(),
            "lorom".into(),
            "32".into(),
            "1000".into(),
            "8000".into(),
            "10000".into(),
            "lz3".into(),
        ])
        .unwrap(),
        Command::Asset(AssetCommand::Graphics {
            compression: lm_project::GraphicsCompression::Lz3,
            observation: None,
            ..
        })
    ));
}

#[test]
fn parses_ownership_backed_graphics_import() {
    assert!(matches!(
        parse_from(&[
            "graphics-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "32".into(),
            "1000".into(),
            "8000".into(),
            "10000".into(),
            "lz3".into(),
            "gfx.lmgfx".into(),
            "7fdc".into(),
            "100000".into(),
            "200000".into(),
            "ownership.lmrats".into(),
        ])
        .unwrap(),
        Command::GraphicsTransfer(GraphicsTransferCommand::Import {
            compression: lm_project::GraphicsCompression::Lz3,
            ownership_manifest: Some(_),
            ..
        })
    ));
}

#[test]
fn parses_ownership_backed_palette_and_exanimation_imports() {
    assert!(matches!(
        parse_from(&[
            "palette-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "5".into(),
            "3900".into(),
            "100".into(),
            "palette.lmpal".into(),
            "7fdc".into(),
            "300000".into(),
            "400000".into(),
            "palette.lmrats".into(),
        ])
        .unwrap(),
        Command::PaletteTransfer(PaletteTransferCommand::Import {
            ownership_manifest: Some(_),
            ..
        })
    ));
    assert!(matches!(
        parse_from(&[
            "exanimation-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "105".into(),
            "3c00".into(),
            "20".into(),
            "8000".into(),
            "modes.bin".into(),
            "animation.lmexan".into(),
            "7fdc".into(),
            "300000".into(),
            "400000".into(),
            "animation.lmrats".into(),
        ])
        .unwrap(),
        Command::ExAnimationTransfer(ExAnimationTransferCommand::Import {
            ownership_manifest: Some(_),
            ..
        })
    ));
}

#[test]
fn parses_ownership_backed_overworld_import() {
    assert!(matches!(
        parse_from(&[
            "overworld-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "1".into(),
            "overworld.layout".into(),
            "modes.bin".into(),
            "world.lmow".into(),
            "7fdc".into(),
            "10000".into(),
            "1f000".into(),
            "world.lmrats".into(),
        ])
        .unwrap(),
        Command::OverworldTransfer(OverworldTransferCommand::Import {
            ownership_manifest: Some(_),
            ..
        })
    ));
}

#[test]
fn parses_ownership_backed_level_and_map16_imports() {
    assert!(matches!(
        parse_from(&[
            "level-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "105".into(),
            "1000".into(),
            "2000".into(),
            "legacy".into(),
            "standard".into(),
            "level.lmlvl".into(),
            "7fdc".into(),
            "300000".into(),
            "400000".into(),
            "level.lmrats".into(),
        ])
        .unwrap(),
        Command::LevelTransfer(LevelTransferCommand::Import {
            ownership_manifest: Some(_),
            ..
        })
    ));
    assert!(matches!(
        parse_from(&[
            "map16-import-owned".into(),
            "in.smc".into(),
            "out.smc".into(),
            "lorom".into(),
            "10".into(),
            "3000".into(),
            "3300".into(),
            "page.map16".into(),
            "7fdc".into(),
            "300000".into(),
            "400000".into(),
            "page.lmrats".into(),
        ])
        .unwrap(),
        Command::Map16Transfer(Map16TransferCommand::Import {
            ownership_manifest: Some(_),
            ..
        })
    ));
}

#[test]
fn parses_atomic_graphics_recompression_workflow() {
    assert_eq!(
        parse_from(&[
            "graphics-recompress".into(),
            "input.smc".into(),
            "output.smc".into(),
            "lorom".into(),
            "200".into(),
            "3".into(),
            "8000".into(),
            "10000".into(),
            "lz2".into(),
            "lz3".into(),
            "7fdc".into(),
            "1000".into(),
            "7000".into(),
        ])
        .unwrap(),
        Command::GraphicsMigration(GraphicsMigrationCommand {
            input_rom: "input.smc".into(),
            output_rom: "output.smc".into(),
            mapper: Mapper::LoRom,
            pointer_table: 0x200,
            entries: 3,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
            source_compression: lm_project::GraphicsCompression::Lz2,
            target_compression: lm_project::GraphicsCompression::Lz3,
            checksum_field: 0x7fdc,
            search_start: 0x1000,
            search_end: 0x7000,
        })
    );
}

#[test]
fn parses_palette_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "palette-export",
            "game.smc",
            "lorom",
            "105",
            "1000",
            "100",
            "palette.lmpal"
        ]),
        Command::PaletteTransfer(PaletteTransferCommand::Export {
            palette: 0x105,
            colors: 0x100,
            ..
        })
    ));
    assert!(matches!(
        parse(vec![
            "palette-import",
            "in.smc",
            "out.smc",
            "exlorom",
            "105",
            "1000",
            "100",
            "palette.lmpal",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::PaletteTransfer(PaletteTransferCommand::Import {
            mapper: Mapper::ExLoRom,
            palette: 0x105,
            colors: 0x100,
            checksum_field: 0x7fdc,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}

#[test]
fn parses_exanimation_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "exanimation-export",
            "game.smc",
            "lorom",
            "105",
            "1000",
            "20",
            "8000",
            "modes.bin",
            "animation.lmexan"
        ]),
        Command::ExAnimationTransfer(ExAnimationTransferCommand::Export {
            slot: 0x105,
            maximum_records: 0x20,
            maximum_encoded_len: 0x8000,
            ..
        })
    ));
    assert!(matches!(
        parse(vec![
            "exanimation-import",
            "in.smc",
            "out.smc",
            "sa1",
            "105",
            "1000",
            "20",
            "8000",
            "modes.bin",
            "animation.lmexan",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::ExAnimationTransfer(ExAnimationTransferCommand::Import {
            mapper: Mapper::Sa1,
            slot: 0x105,
            checksum_field: 0x7fdc,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}

#[test]
fn parses_complete_overworld_export_and_safe_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "overworld-export",
            "game.smc",
            "lorom",
            "0",
            "overworld.layout",
            "modes.bin",
            "world.lmow"
        ]),
        Command::OverworldTransfer(OverworldTransferCommand::Export { slot: 0, .. })
    ));
    assert!(matches!(
        parse(vec![
            "overworld-import",
            "in.smc",
            "out.smc",
            "exlorom",
            "0",
            "overworld.layout",
            "modes.bin",
            "world.lmow",
            "7fdc",
            "300000",
            "400000"
        ]),
        Command::OverworldTransfer(OverworldTransferCommand::Import {
            mapper: Mapper::ExLoRom,
            slot: 0,
            checksum_field: 0x7fdc,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            ..
        })
    ));
}

#[test]
fn parses_graphics_and_palette_inspection() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert!(matches!(
        parse(vec![
            "graphics", "game.smc", "lorom", "32", "1000", "8000", "10000"
        ]),
        Command::Asset(AssetCommand::Graphics {
            file: 0x32,
            maximum_decompressed_len: 0x10000,
            ..
        })
    ));
    assert!(matches!(
        parse(vec![
            "overworld-sprites",
            "game.smc",
            "lorom",
            "0",
            "4000",
            "10",
            "9"
        ]),
        Command::Asset(AssetCommand::OverworldSprites {
            count: 0x10,
            record_len: 9,
            ..
        })
    ));
    assert_eq!(
        parse(vec![
            "native-overworld-sprites",
            "game.smc",
            "lorom",
            "7755b",
            "sizes.bin",
            "sprites.obs"
        ]),
        Command::Asset(AssetCommand::NativeCustomOverworldSprites {
            rom: "game.smc".into(),
            mapper: Mapper::LoRom,
            pointer: 0x7755b,
            record_sizes: "sizes.bin".into(),
            observation: "sprites.obs".into(),
        })
    );
    assert_eq!(
        parse(vec![
            "exanimation-slot-options",
            "game.smc",
            "lorom",
            "1234",
            "options.obs"
        ]),
        Command::ExAnimationSlotOptionsObserve {
            rom: "game.smc".into(),
            mapper: Mapper::LoRom,
            pointer: 0x1234,
            output: "options.obs".into(),
        }
    );
    assert!(matches!(
        parse(vec![
            "exanimation",
            "game.smc",
            "exlorom",
            "105",
            "4000",
            "20",
            "8000",
            "modes.bin"
        ]),
        Command::Asset(AssetCommand::ExAnimation {
            slot: 0x105,
            maximum_records: 0x20,
            ..
        })
    ));
    assert!(matches!(
        parse(vec!["palette", "game.smc", "sa1", "5", "2000", "100"]),
        Command::Asset(AssetCommand::Palette {
            index: 5,
            colors: 0x100,
            ..
        })
    ));
}

#[test]
fn parses_expanded_settings_export_and_import() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert_eq!(
        parse(vec![
            "expanded-settings-export",
            "game.smc",
            "exlorom",
            "105",
            "0x20000",
            "0x200",
            "0x20",
            "record.bin",
        ]),
        Command::ExpandedSettingsTransfer(ExpandedSettingsTransferCommand::Export {
            rom: "game.smc".into(),
            mapper: Mapper::ExLoRom,
            slot: 0x105,
            table_offset: 0x20000,
            entries: 0x200,
            stride: 0x20,
            output: "record.bin".into(),
        })
    );
    assert!(matches!(
        parse(vec![
            "expanded-settings-import",
            "game.smc",
            "changed.smc",
            "sa1",
            "1ff",
            "0x4000",
            "0x200",
            "0x20",
            "record.bin",
            "0x7fdc",
        ]),
        Command::ExpandedSettingsTransfer(ExpandedSettingsTransferCommand::Import {
            mapper: Mapper::Sa1,
            slot: 0x1ff,
            checksum_field: 0x7fdc,
            ..
        })
    ));
}
