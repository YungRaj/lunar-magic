use super::*;
use crate::editor_shell::{
    execute_entrance_script, execute_native_assets_script, execute_owned_editor_script,
    execute_secondary_exit_script,
};
use lm_project::{
    PaletteSaveOptions, PayloadReadPolicy, RatsOwnershipManifest, RatsOwnershipManifestFile,
};

fn native_assets_test_app(profile: &lm_profile::RevisionProfile) -> AppState {
    native_assets_test_app_from_rom(profile, profiled_rom(profile))
}

fn native_assets_test_app_from_rom(
    profile: &lm_profile::RevisionProfile,
    rom: Vec<u8>,
) -> AppState {
    let mut source = Project::new(RomImage::from_bytes(rom).unwrap());
    let allocation = |search| AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: Vec::new(),
    };
    source
        .save_palette(
            0x105,
            &lm_graphics::Palette {
                colors: vec![lm_graphics::Bgr555(0xffff); profile.palette.colors_per_palette],
            },
            profile.palette,
            &PaletteSaveOptions {
                allocation: allocation(0x24_0000..0x25_0000),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    source
        .save_exanimation(
            0x105,
            &CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            profile.exanimation,
            &profile.exanimation_double_size_modes,
            &ExAnimationSaveOptions {
                allocation: allocation(0x25_0000..0x26_0000),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    let mut app = AppState::default();
    app.load_rom(source.save_snapshot()).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app
}

fn profile_and_rom_with_installed_animation_features() -> (lm_profile::RevisionProfile, Vec<u8>) {
    const FIRST_OPERAND: usize = 0x3_0000;
    const RUNTIME: usize = 0x3_0100;
    const FEATURE_TABLE: usize = 0x3_0201;
    let mut profile = lm_profile::test_support::profile();
    let lm_project::InstalledLayout::Unconditional(mut exanimation) =
        profile.exanimation_installation
    else {
        panic!("test profile uses an unconditional ExAnimation layout");
    };
    exanimation.pointer_locator = Some(lm_project::ChainedSnesPointerLocator {
        mapper: profile.mapper,
        first_operand_offset: FIRST_OPERAND,
        final_operand_displacement: 0,
    });
    profile.exanimation_installation = lm_project::InstalledLayout::Unconditional(exanimation);
    profile.exanimation_feature_installation = lm_project::InstalledLayout::Unconditional(
        lm_project::InstalledExAnimationFeatureRomLayout {
            table_locator: lm_project::ChainedSnesPointerLocator {
                mapper: profile.mapper,
                first_operand_offset: FIRST_OPERAND,
                final_operand_displacement: 3,
            },
        },
    );
    profile.validate().unwrap();

    let mut rom = profiled_rom(&profile);
    let pointer = |target: usize| pc_to_snes(profile.mapper, target).unwrap().to_le_bytes();
    rom[FIRST_OPERAND..FIRST_OPERAND + 3].copy_from_slice(&pointer(RUNTIME)[..3]);
    rom[RUNTIME..RUNTIME + 3].copy_from_slice(&pointer(profile.exanimation.pointers.offset)[..3]);
    rom[RUNTIME + 3..RUNTIME + 6].copy_from_slice(&pointer(FEATURE_TABLE)[..3]);
    rom[FEATURE_TABLE - 1] = 0;
    rom[FEATURE_TABLE..FEATURE_TABLE + lm_project::EXANIMATION_FEATURE_LEVEL_COUNT].fill(0);
    rom[FEATURE_TABLE + 0x105] = 0xa5;
    let checksum = compute_snes_checksum(&rom, 0x7fdc).unwrap();
    rom[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    (profile, rom)
}

#[test]
fn terminal_native_assets_spec_commits_all_domains_as_one_undoable_operation() {
    let (profile, rom) = profile_and_rom_with_installed_animation_features();
    let mut app = native_assets_test_app_from_rom(&profile, rom);
    let before = app.project().unwrap().save_snapshot();
    let directory =
        std::env::temp_dir().join(format!("lm-app-native-assets-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    fs::write(
        directory.join("Level edits.txt"),
        "LMLEDIT1\nheader mode 03\n",
    )
    .unwrap();
    fs::write(
        directory.join("Palette edits.txt"),
        "LMPALED1\nowners 100 editable\nset 1 1234\n",
    )
    .unwrap();
    fs::write(
        directory.join("Animation edits.txt"),
        "LMEXAED1\nsetting 07\n",
    )
    .unwrap();
    fs::write(
        directory.join("Animation feature edits.txt"),
        "LMEXFT1\nfeatures true false true false\n",
    )
    .unwrap();
    fs::write(directory.join("設定 edits.txt"), "LMXSETED1\nword 2 abcd\n").unwrap();
    let spec = directory.join("Aggregate edits.lmnat");
    fs::write(
        &spec,
        "LMNATED1\nlevel=Level edits.txt\npalette=Palette edits.txt\nexanimation=Animation edits.txt\nexanimation-features=Animation feature edits.txt\nexpanded-settings=設定 edits.txt\n",
    )
    .unwrap();

    execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).unwrap();
    let project = app.project().unwrap();
    let reopened_features = project
        .load_installed_exanimation_features(0x105, profile.exanimation_feature_installation)
        .unwrap();
    assert_eq!(reopened_features.options.encode(), 0x55);
    assert_eq!(reopened_features.options.preserved_low_nibble, 5);
    assert_eq!(
        project
            .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
            .unwrap()
            .layer1
            .header
            .level_mode(),
        3
    );
    assert_eq!(
        project.load_palette(0x105, profile.palette).unwrap().colors[1].0,
        0x1234
    );
    assert_eq!(
        project
            .load_exanimation(
                0x105,
                profile.exanimation,
                &profile.exanimation_double_size_modes,
            )
            .unwrap()
            .setting,
        7
    );
    assert_eq!(
        project
            .load_expanded_level_settings(0x105, profile.expanded_settings.unwrap())
            .unwrap()
            .word(2)
            .unwrap(),
        0xabcd
    );
    assert!(project.identity.as_ref().unwrap().checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(
        directory.join("Animation feature edits.txt"),
        "LMEXFT1\nfeatures true false invalid false\n",
    )
    .unwrap();
    assert!(execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_layer2_object_script_reopens_checksum_valid_and_rolls_back_atomically() {
    let mut profile = lm_profile::test_support::profile();
    profile.layer2 = Some(lm_project::LevelLayer2RomLayout {
        mapper: profile.mapper,
        pointers: lm_project::LevelPointerTable {
            offset: 0x3_0000,
            entries: 0x200,
            stride: 3,
        },
        background_bank_substitution: None,
        legacy_pointer_redirect: None,
        descriptor_table: None,
        maximum_compressed_len: 0x8000,
        tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::SplitPlanes,
    });
    let rom = profiled_rom(&profile);
    let mut app = native_assets_test_app_from_rom(&profile, rom);
    let before = app.project().unwrap().save_snapshot();
    let baseline_level = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let baseline_layer2 = app
        .project()
        .unwrap()
        .load_level_layer2(
            0x105,
            baseline_level.layer1.header.level_mode(),
            profile.layer2.unwrap(),
        )
        .unwrap();
    let lm_level::NativeLayer2Data::Objects(objects) = baseline_layer2 else {
        panic!("test level must use object-backed Layer 2");
    };
    let placement = objects
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            let record = &objects.objects.records[placement.record_index];
            record.encoded().len() == 3 && record.command_id() != 0 && record.command_id() != 0x27
        })
        .unwrap();
    let record = &objects.objects.records[placement.record_index];
    let field_command = record.command_id();
    let directory =
        std::env::temp_dir().join(format!("lm-app-layer2-objects-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Layer 2 objects.txt");
    let spec = directory.join("Layer 2 aggregate.lmnat");
    fs::write(&spec, "LMNATED1\nlayer2-objects=Layer 2 objects.txt\n").unwrap();
    fs::write(
        &script,
        format!(
            "LML2OBJ1\nobject fields {} {:02x} 55 1d 0c 0b true\n",
            placement.record_index, field_command,
        ),
    )
    .unwrap();
    execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).unwrap();
    let reopened = app
        .project()
        .unwrap()
        .load_level_layer2(
            0x105,
            baseline_level.layer1.header.level_mode(),
            profile.layer2.unwrap(),
        )
        .unwrap();
    let lm_level::NativeLayer2Data::Objects(reopened) = reopened else {
        unreachable!();
    };
    let edited = reopened
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            let record = &reopened.objects.records[placement.record_index];
            record.command_id() == field_command
                && record.parameter() == 0x55
                && placement.screen == 0x1d
                && placement.major == 0x1db
                && placement.minor == 0x1c
        })
        .unwrap();
    assert!(reopened.objects.records[edited.record_index].perpendicular_high_coordinate());
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        format!(
            "LML2OBJ1\nobject fields {} {:02x} 55 1d 0c 0b true\nobject remove 999\n",
            placement.record_index, field_command,
        ),
    )
    .unwrap();
    assert!(execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_layer2_tilemap_script_paints_remaps_reopens_and_rolls_back_atomically() {
    let mut profile = lm_profile::test_support::profile();
    profile.layer2 = Some(lm_project::LevelLayer2RomLayout {
        mapper: profile.mapper,
        pointers: lm_project::LevelPointerTable {
            offset: 0x3_0000,
            entries: 0x200,
            stride: 3,
        },
        background_bank_substitution: None,
        legacy_pointer_redirect: None,
        descriptor_table: None,
        maximum_compressed_len: 0x8000,
        tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::SplitPlanes,
    });
    let mut rom = profiled_rom(&profile);
    rom[0x6001] &= 0xe0;
    let tilemap = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
    let planes = lm_level::split_layer2_tilemap_planes(&tilemap).unwrap();
    let encoded = lm_codec::encode_terminated_rle(&planes);
    rom[0x5c00..0x5c00 + encoded.len()].copy_from_slice(&encoded);
    let checksum = compute_snes_checksum(&rom, 0x7fdc).unwrap();
    rom[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut app = native_assets_test_app_from_rom(&profile, rom);
    let before = app.project().unwrap().save_snapshot();
    let baseline_level = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(baseline_level.layer1.header.level_mode(), 0);

    let directory =
        std::env::temp_dir().join(format!("lm-app-layer2-tilemap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Layer 2 tilemap.txt");
    let spec = directory.join("Layer 2 aggregate.lmnat");
    fs::write(&spec, "LMNATED1\nlayer2-tilemap=Layer 2 tilemap.txt\n").unwrap();
    fs::write(
        &script,
        "LML2TIL1\nword 0 1234\nword 1023 abcd\nremap 0 0 8234,8235\n",
    )
    .unwrap();
    execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).unwrap();
    let reopened = app
        .project()
        .unwrap()
        .load_level_layer2(0x105, 0, profile.layer2.unwrap())
        .unwrap();
    let lm_level::NativeLayer2Data::Tilemap(bytes) = reopened else {
        panic!("mode zero must reopen compressed Layer 2");
    };
    assert_eq!(u16::from_le_bytes(bytes[0..2].try_into().unwrap()), 0x0235);
    assert_eq!(
        u16::from_le_bytes(bytes[0x7fe..0x800].try_into().unwrap()),
        0xabcd
    );
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(&script, "LML2TIL1\nword 0 2222\nremap 0 all 8000,9000\n").unwrap();
    assert!(execute_native_assets_script(&mut app, &spec, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn owned_native_assets_shell_reclaims_exact_blocks_and_rejects_stale_evidence() {
    let profile = lm_profile::test_support::profile();
    let mut app = native_assets_test_app(&profile);
    let before = app.project().unwrap().save_snapshot();
    let blocks = [profile.palette.pointers, profile.exanimation.pointers].map(|table| {
        let pointer = table.pointer_offset(0x105).unwrap();
        app.project()
            .unwrap()
            .load_payload(pointer, profile.mapper, &PayloadReadPolicy::Tagged)
            .unwrap()
            .block
            .unwrap()
    });
    let directory =
        std::env::temp_dir().join(format!("lm-app-owned-native-assets-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let palette = directory.join("Palette edits.txt");
    let animation = directory.join("Animation edits.txt");
    let spec = directory.join("Aggregate edits.lmnat");
    let manifest = directory.join("Ownership.lmrats");
    fs::write(&palette, "LMPALED1\nowners 100 editable\nset 1 1234\n").unwrap();
    fs::write(&animation, "LMEXAED1\nsetting 07\n").unwrap();
    fs::write(
        &spec,
        "LMNATED1\npalette=Palette edits.txt\nexanimation=Animation edits.txt\n",
    )
    .unwrap();
    fs::write(
        &manifest,
        RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: blocks.to_vec(),
            retained: Vec::new(),
        })
        .encode()
        .unwrap(),
    )
    .unwrap();

    execute_owned_editor_script(
        &mut app,
        shell_command::ScriptEditor::NativeAssets,
        &spec,
        &manifest,
        0x1_0000..0x1_8000,
    )
    .unwrap();
    for block in &blocks {
        assert!(
            app.project().unwrap().rom.logical_bytes()[block.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
    }
    let after = app.project().unwrap().save_snapshot();
    fs::write(&palette, "LMPALED1\nowners 100 editable\nset 1 4321\n").unwrap();
    assert!(execute_owned_editor_script(
        &mut app,
        shell_command::ScriptEditor::NativeAssets,
        &spec,
        &manifest,
        0x1_8000..0x2_0000,
    )
    .is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), after);
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_level_header_edit_commits_reloads_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let original_last_screen = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap()
        .layer1
        .header
        .last_screen();
    let revision_before_edit = app.project_revision();
    edit_level_header(
        &mut app,
        shell_command::LevelHeaderField::LastScreen,
        0x1d,
        0x1_0000..0x1_8000,
    )
    .unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(loaded.layer1.header.last_screen(), 0x1d);
    assert_eq!(app.project_revision(), revision_before_edit + 1);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(restored.layer1.header.last_screen(), original_last_screen);
}

#[test]
fn terminal_layer1_scroll_header_edit_preserves_adjacent_bits_reopens_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let original = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let original_byte = original.layer1.header.encoded()[4];

    edit_level_header(
        &mut app,
        shell_command::LevelHeaderField::Layer1VerticalScroll,
        2,
        0x1_0000..0x1_8000,
    )
    .unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(
        loaded.layer1.header.layer1_vertical_scroll(),
        lm_level::Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying
    );
    assert_eq!(
        loaded.layer1.header.encoded()[4] & !0x30,
        original_byte & !0x30
    );
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(
        app.project()
            .unwrap()
            .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
            .unwrap(),
        original
    );
    assert!(edit_level_header(
        &mut app,
        shell_command::LevelHeaderField::Layer1VerticalScroll,
        4,
        0x1_0000..0x1_8000,
    )
    .is_err());
    assert_eq!(
        app.project()
            .unwrap()
            .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
            .unwrap(),
        original
    );
}

#[test]
fn terminal_expanded_settings_batch_is_atomic_checksum_valid_and_undoable() {
    let profile = lm_profile::test_support::profile();
    let layout = profile.expanded_settings.unwrap();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    let baseline = app
        .project()
        .unwrap()
        .load_expanded_level_settings(0x105, layout)
        .unwrap();
    let directory = std::env::temp_dir().join(format!(
        "lm-app-expanded-settings-script-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("expanded settings 日本語.lmedit");
    fs::write(
        &script,
        "LMXSETED1\nlayer3-tilemap true abc 2 3\nsuper-gfx true 1 2 3 4 5 6 101 202 303 404\nlayer3-mode 89abcdef\n",
    )
    .unwrap();

    edit_expanded_settings(&mut app, &script).unwrap();
    let record = app
        .project()
        .unwrap()
        .load_expanded_level_settings(0x105, layout)
        .unwrap();
    assert_eq!(record.word(0).unwrap() & 0xa000, 0xa000);
    assert_eq!(
        record.word(0).unwrap() & !0xa000,
        baseline.word(0).unwrap() & !0xa000
    );
    assert_eq!(record.word(1).unwrap(), 0xeabc);
    assert_eq!(record.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
    assert_eq!(
        lm_level::ExpandedLevelHeader::from(&record).super_graphics_bypass(),
        lm_level::SuperGraphicsBypass {
            enabled: true,
            foreground_background: [1, 2, 3, 4, 5, 6],
            sprites: [0x101, 0x202, 0x303, 0x404],
        }
    );
    for word in 12..16 {
        assert_eq!(
            record.word(word).unwrap() & 0x0fff,
            baseline.word(word).unwrap() & 0x0fff
        );
    }
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(
        app.project()
            .unwrap()
            .load_expanded_level_settings(0x105, layout)
            .unwrap()
            .word(0)
            .unwrap(),
        baseline.word(0).unwrap()
    );

    fs::write(
        &script,
        "LMXSETED1\nlayer3-mode 89abcdef\nboundary-air true\n",
    )
    .unwrap();
    assert!(edit_expanded_settings(&mut app, &script).is_err());
    assert_eq!(
        app.project()
            .unwrap()
            .load_expanded_level_settings(0x105, layout)
            .unwrap()
            .word(1)
            .unwrap(),
        baseline.word(1).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_level_edit_script_covers_objects_and_native_sprite_tokens_atomically() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-level-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("level edits.lmedit");
    fs::write(
        &script,
        "LMLEDIT1\n\
         header background-palette 05\n\
         header layer1-scroll 02\n\
         object replace 0 020001\n\
         object insert 1 030001\n\
         object move 1 0\n\
         object remove 1\n\
         object command 0 01\n\
         object parameter 0 7f\n\
         object coordinates 0 0e 0d\n\
         object screen-advance 0 true\n\
         object insert 1 010201\n\
         object screen-jump-target 1 0a1b\n\
         sprite-header 20\n\
         sprite replace 0 record 000002\n\
         sprite insert 1 screen 12\n\
         sprite insert 2 control 90\n\
         sprite move 2 1\n\
         sprite remove 2\n",
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(loaded.layer1.header.background_palette(), 5);
    assert_eq!(
        loaded.layer1.header.layer1_vertical_scroll(),
        lm_level::Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying
    );
    assert_eq!(
        loaded.layer1.objects.records[0].encoded(),
        [0x8e, 0x1d, 0x7f]
    );
    assert_eq!(loaded.layer1.objects.records[1].encoded(), [0x1b, 0x0a, 1]);
    assert_eq!(loaded.sprites.header, 0);
    assert!(matches!(
        loaded.sprites.tokens.as_slice(),
        [lm_level::SpriteToken::Record(_)]
    ));
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert_eq!(restored.layer1.header.background_palette(), 0);
    assert_eq!(
        restored.layer1.header.layer1_vertical_scroll(),
        lm_level::Layer1VerticalScrollMode::None
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_screen_exit_script_canonicalizes_both_shapes_reopens_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let directory =
        std::env::temp_dir().join(format!("lm-app-screen-exit-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("screen exit edits.lmedit");

    for (requested, expected, encoded_len) in [(0x0000, 0x0400, 4), (0x1000, 0x1400, 5)] {
        fs::write(
            &script,
            format!(
                "LMLEDIT1\nobject insert 1 850a0034\nobject screen-exit 1 1f {requested:04x}\n"
            ),
        )
        .unwrap();
        execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
        let loaded = app
            .project()
            .unwrap()
            .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
            .unwrap();
        let record = &loaded.layer1.objects.records[1];
        assert_eq!(record.encoded().len(), encoded_len);
        assert_ne!(record.encoded()[0] & 0x80, 0);
        let exit = record.screen_exit().unwrap();
        assert_eq!(exit.screen, 0x1f);
        assert_eq!(exit.destination_and_flags, expected);
        assert!(app
            .project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches());
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), before);
    }

    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nobject screen-exit 0 01 0000\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(&script, "LMLEDIT1\nobject screen-exit 0 20 0000\n").unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_absolute_object_place_and_full_relocate_reopen_and_undo() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let baseline_level = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let field_object = baseline_level
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            let record = &baseline_level.layer1.objects.records[placement.record_index];
            record.encoded().len() == 3 && record.command_id() != 0x27
        })
        .unwrap();
    let field_command =
        baseline_level.layer1.objects.records[field_object.record_index].command_id();
    let directory =
        std::env::temp_dir().join(format!("lm-app-object-position-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("absolute object edits.lmedit");

    fs::write(
        &script,
        "LMLEDIT1\nobject place 090855 1f 0c 0b true\n",
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let placed = loaded
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            placement.screen == 0x1f && placement.major == 0x1fb && placement.minor == 0x1c
        })
        .unwrap();
    assert!(loaded.layer1.objects.records[placed.record_index].perpendicular_high_coordinate());
    assert!(loaded
        .layer1
        .objects
        .records
        .iter()
        .any(|record| record.screen_jump().is_some()));
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        format!(
            "LMLEDIT1\nobject fields {} {field_command:02x} 55 1d 0c 0b true\n",
            field_object.record_index
        ),
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let field_edited = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let placement = field_edited
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            let record = &field_edited.layer1.objects.records[placement.record_index];
            record.command_id() == field_command
                && record.parameter() == 0x55
                && placement.screen == 0x1d
                && placement.major == 0x1db
                && placement.minor == 0x1c
        })
        .unwrap();
    assert!(
        field_edited.layer1.objects.records[placement.record_index].perpendicular_high_coordinate()
    );
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        "LMLEDIT1\nobject relocate-position 0 1e 0a 09 true\n",
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let relocated = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert!(relocated
        .layer1
        .objects
        .native_placements()
        .iter()
        .any(|placement| {
            placement.screen == 0x1e && placement.major == 0x1e9 && placement.minor == 0x1a
        }));
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nobject relocate-position 999 00 00 00 false\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nobject fields 999 01 00 00 00 00 false\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(
        &script,
        "LMLEDIT1\nobject place 090855 20 00 00 false\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nobject place 020001 00 00 00 false\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_semantic_sprite_place_and_relocate_reopen_and_undo() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let baseline_level = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert!(baseline_level.sprites.expanded);
    let baseline_sprite = baseline_level.sprites.native_placements()[0].token_index;
    let field_sprite = baseline_level
        .sprites
        .tokens
        .iter()
        .position(|token| {
            matches!(token, lm_level::SpriteToken::Record(record) if record.encoded.len() == 3)
        })
        .unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-sprite-position-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("semantic sprite positions.lmedit");

    fs::write(&script, "LMLEDIT1\nsprite place 080047 1f 0c 009d\n").unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let placed = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let placement = placed
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| {
            placement.sprite_number == 0x47
                && placement.screen == 0x1f
                && placement.major == 0x1fc
                && placement.minor == 0x9d
        })
        .unwrap();
    let record = match &placed.sprites.tokens[placement.token_index] {
        lm_level::SpriteToken::Record(record) => record,
        lm_level::SpriteToken::Screen(_) | lm_level::SpriteToken::Control(_) => unreachable!(),
    };
    assert_eq!(record.native_fields().unwrap().extra_bits, 2);
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        format!("LMLEDIT1\nsprite fields {field_sprite} 1c 02 1d 0b 47\n"),
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let edited = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let placement = edited
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| {
            placement.sprite_number == 0x47
                && placement.screen == 0x1d
                && placement.major == 0x1db
                && placement.minor & 0x1f == 0x1c
        })
        .unwrap();
    let fields = match &edited.sprites.tokens[placement.token_index] {
        lm_level::SpriteToken::Record(record) => record.native_fields().unwrap(),
        lm_level::SpriteToken::Screen(_) | lm_level::SpriteToken::Control(_) => unreachable!(),
    };
    assert_eq!(fields.extra_bits, 2);
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        format!("LMLEDIT1\nsprite relocate-position {baseline_sprite} 1e 0a 008f\n"),
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let relocated = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert!(
        relocated
            .sprites
            .native_placements()
            .iter()
            .any(|placement| {
                placement.screen == 0x1e && placement.major == 0x1ea && placement.minor == 0x8f
            })
    );
    assert!(
        app.project()
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .checksum_matches()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nsprite relocate-position 999 00 00 0000\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::write(
        &script,
        "LMLEDIT1\nheader background-palette 05\nsprite place 080047 00 00 1000\n",
    )
    .unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_custom_time_script_is_orientation_aware_checksum_valid_and_undoable() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let directory = std::env::temp_dir().join(format!("lm-app-custom-time-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("custom time edits.lmedit");

    fs::write(&script, "LMLEDIT1\ncustom-time abc true\n").unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let horizontal = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let settings = lm_level::CustomTimeSettings::new(0xabc, true).unwrap();
    assert_eq!(horizontal.layer1.objects.custom_time(false), Some(settings));
    let horizontal_record = horizontal.layer1.objects.records.last().unwrap().encoded();
    assert_eq!(
        horizontal
            .layer1
            .objects
            .records
            .last()
            .unwrap()
            .command_id(),
        0x28
    );
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(&script, "LMLEDIT1\nheader mode 03\ncustom-time abc true\n").unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let vertical = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    assert!(vertical.layer1.header.is_vertical());
    assert_eq!(vertical.layer1.objects.custom_time(true), Some(settings));
    let vertical_record = vertical.layer1.objects.records.last().unwrap().encoded();
    assert_ne!(vertical_record, horizontal_record);
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(&script, "LMLEDIT1\ncustom-time 000 false\n").unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_sprite_properties_preserve_expanded_framing_reopen_and_undo() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let directory =
        std::env::temp_dir().join(format!("lm-app-sprite-properties-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("sprite properties.lmedit");

    fs::write(
        &script,
        "LMLEDIT1\nsprite-header 20\nsprite-properties 12 true false\nsprite insert 0 screen 12\n",
    )
    .unwrap();
    execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let header = lm_level::NativeSpriteHeader::from_raw(loaded.sprites.header);
    assert_eq!(header.memory(), 0x12);
    assert!(header.buoyancy_1());
    assert!(!header.buoyancy_2());
    assert_ne!(loaded.sprites.header & 0x20, 0);
    assert!(loaded.sprites.expanded);
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());

    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(&script, "LMLEDIT1\nsprite-properties 13 false false\n").unwrap();
    assert!(execute_level_script(&mut app, &script, 0x1_0000..0x1_8000).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_entrance_batch_preserves_scroll_nibble_reopens_checksum_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    let before = app.project().unwrap().save_snapshot();
    let mut layout = lm_profile::smw_us_v1_vanilla_entrance_layout();
    layout.mapper = app.project().unwrap().identity.as_ref().unwrap().mapper;

    let profiled = app.profiled_controller_snapshot().unwrap();
    let mut controller =
        lm_app::VanillaEntranceController::decode(&profiled.snapshot, layout).unwrap();
    let controller_before = controller.entrance();
    assert!(matches!(
        controller.apply_edits(&[
            lm_app::VanillaEntranceEdit::SetMain(lm_project::VanillaMainEntrance {
                position: 0x12,
                vertical_settings: 0x34,
                screen_and_method: 0x56,
                level_mode_and_screen: 0x78,
            }),
            lm_app::VanillaEntranceEdit::SetMidway(lm_level::SeparateMidwayEntrance {
                flags: 0x9a,
                position: 0xbc,
                additional_flags: 0xde,
                high_position: 0xf0,
            }),
        ]),
        Err(lm_app::VanillaEntranceControllerError::MidwayUnavailable { command: 1 })
    ));
    assert_eq!(controller.entrance(), controller_before);
    assert!(matches!(
        controller.apply_edits(&[
            lm_app::VanillaEntranceEdit::SetMain(lm_project::VanillaMainEntrance {
                position: 0x12,
                vertical_settings: 0x34,
                screen_and_method: 0x56,
                level_mode_and_screen: 0x78,
            }),
            lm_app::VanillaEntranceEdit::SetLayer2ScrollTable(0x10),
        ]),
        Err(lm_app::VanillaEntranceControllerError::InvalidLayer2ScrollTable {
            command: 1,
            value: 0x10,
        })
    ));
    assert_eq!(controller.entrance(), controller_before);

    let directory =
        std::env::temp_dir().join(format!("lm-app-entrance-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("entrance edits.lmentr");
    fs::write(
        &script,
        "LMENTR1\nmain 12 34 56 78\nlayer2-scroll 0a\n",
    )
    .unwrap();
    execute_entrance_script(&mut app, &script).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_vanilla_main_entrance(0x105, layout)
        .unwrap();
    assert_eq!(loaded.position, 0xa2);
    assert_eq!(loaded.vertical_settings, 0x34);
    assert_eq!(loaded.screen_and_method, 0x56);
    assert_eq!(loaded.level_mode_and_screen, 0x78);
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), before);

    fs::write(
        &script,
        "LMENTR1\nmain 12 34 56 78\nmidway 9a bc de f0\n",
    )
    .unwrap();
    assert!(execute_entrance_script(&mut app, &script).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_secondary_exit_set_clear_all_reopens_checksum_and_undoes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original =
        fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
    let mut app = AppState::default();
    app.load_rom(original.clone()).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-secondary-exit-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("secondary exit edits.lmsexed");
    fs::write(
        &script,
        "LMSEXED1\nclear-all\nset 0400 0105 02 03 04 05 20 80 07\nclear 0400\nset 0401 0106 02 03 04 05 20 80 07\n",
    )
    .unwrap();
    execute_secondary_exit_script(&mut app, &script).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .unwrap();
    assert_eq!(loaded.table.entries[0], lm_level::SecondaryExit::default());
    assert_eq!(loaded.table.entries[0x400], lm_level::SecondaryExit::default());
    assert_eq!(
        loaded.table.entries[0x401],
        lm_level::SecondaryExit {
            destination_level: 0x106,
            position_and_method: 2,
            screen: 3,
            x: 4,
            y: 5,
            destination_flags: 0x20,
            x_and_overworld_flags: 0x80,
            additional_flags: 7,
        }
    );
    assert!(app
        .project()
        .unwrap()
        .identity
        .as_ref()
        .unwrap()
        .checksum_matches());
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), original);

    fs::write(
        &script,
        "LMSEXED1\nclear-all\nset 0401 2000 02 03 04 05 20 80 07\n",
    )
    .unwrap();
    assert!(execute_secondary_exit_script(&mut app, &script).is_err());
    assert_eq!(app.project().unwrap().save_snapshot(), original);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_script_edit_failure_preserves_revision_and_native_level() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let revision = app.project_revision();
    let before = app
        .project()
        .unwrap()
        .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
        .unwrap();
    let edits = level_edit_script::parse("LMLEDIT1\nheader mode 03\nobject remove 999\n").unwrap();
    assert!(commit_level_edits(&mut app, &edits, 0x1_0000..0x1_8000, "failure").is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(
        app.project()
            .unwrap()
            .load_level_slot(0x105, profile.level, &profile.sprite_lengths)
            .unwrap(),
        before
    );
}

#[test]
fn terminal_map16_script_commits_all_edit_shapes_reloads_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-map16-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Map16 edits.lmedit");
    fs::write(
        &script,
        "LMM16ED1\n\
         tile 01 02 0001 0002 0003 0004 0000 10000\n\
         subtile 01 02 br 0005 10000\n\
         acts-like 01 02 0000 10000\n",
    )
    .unwrap();
    execute_map16_script(&mut app, &script, 0x1_0000..0x10_0000).unwrap();
    let set = app
        .project()
        .unwrap()
        .load_map16_set(profile.map16)
        .unwrap();
    let tile = set.pages[1].tiles[2];
    assert_eq!(tile.top_left.0, 1);
    assert_eq!(tile.bottom_right.0, 5);
    assert_eq!(tile.acts_like, 0);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_map16_set(profile.map16)
        .unwrap();
    assert_eq!(restored.pages[1].tiles[2], lm_level::Map16Tile::default());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_map16_cycle_failure_preserves_application_revision_and_rom() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let revision = app.project_revision();
    let before = app.project().unwrap().save_snapshot();
    let edits = map16_edit_script::parse(
        "LMM16ED1\n\
         acts-like 00 01 0002 10000\n\
         acts-like 00 02 0001 10000\n",
    )
    .unwrap();
    assert!(commit_map16_edits(&mut app, &edits, 0x1_0000..0x10_0000).is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project().unwrap().save_snapshot(), before);
}

#[test]
fn terminal_palette_script_preserves_ownership_commits_raw_words_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-palette-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Palette edits.lmedit");
    fs::write(
        &script,
        "LMPALED1\n\
         owners 100 editable\n\
         owner 00 fixed\n\
         owner 10 exanimation 0002\n\
         changes 01 1234 02 9234\n\
         range 03 8001 7fff\n",
    )
    .unwrap();
    execute_palette_script(&mut app, &script, 0x1_0000..0x2_0000).unwrap();
    let palette = app
        .project()
        .unwrap()
        .load_palette(1, profile.palette)
        .unwrap();
    assert_eq!(palette.colors[0].0, 0xffff);
    assert_eq!(palette.colors[1].0, 0x1234);
    assert_eq!(palette.colors[2].0, 0x9234);
    assert_eq!(palette.colors[3].0, 0x8001);
    assert_eq!(palette.colors[4].0, 0x7fff);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_palette(1, profile.palette)
        .unwrap();
    assert!(restored.colors.iter().all(|color| color.0 == 0xffff));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_protected_palette_edit_and_wrong_ownership_shape_preserve_application() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let revision = app.project_revision();
    let before = app.project().unwrap().save_snapshot();
    let protected = palette_edit_script::parse(
        "LMPALED1\n\
         owners 100 editable\n\
         owner 00 fixed\n\
         set 01 1234\n\
         set 00 5678\n",
    )
    .unwrap();
    assert!(commit_palette_edits(&mut app, &protected, 0x1_0000..0x2_0000).is_err());
    let wrong_shape = palette_edit_script::parse("LMPALED1\nowners ff editable\n").unwrap();
    assert!(commit_palette_edits(&mut app, &wrong_shape, 0x1_0000..0x2_0000).is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project().unwrap().save_snapshot(), before);
}

#[test]
fn terminal_graphics_script_commits_tiles_reloads_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-graphics-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Graphics edits.lmedit");
    fs::write(
        &script,
        format!(
            "LMGFXED1\nowners 3 editable\nowner 0 fixed\nowner 2 exanimation 7\nchanges 1 {}\nrange 1 {}\n",
            "a".repeat(64),
            "b".repeat(64)
        ),
    )
    .unwrap();
    execute_graphics_script(&mut app, &script, 0x1_0000..0x2_0000).unwrap();
    let graphics = app
        .project()
        .unwrap()
        .load_graphics_file(2, profile.graphics)
        .unwrap();
    assert_eq!(graphics.tiles[0].pixels(), &[0; 64]);
    assert_eq!(graphics.tiles[1].pixels(), &[0x0b; 64]);
    assert_eq!(graphics.tiles[2].pixels(), &[2; 64]);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_graphics_file(2, profile.graphics)
        .unwrap();
    assert_eq!(restored.tiles[1].pixels(), &[1; 64]);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn terminal_exanimation_script_commits_reloads_checksums_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-exanimation-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("ExAnimation edits.lmedit");
    fs::write(
        &script,
        "LMEXAED1\nsetting 05\nheader deadbeef\ntrigger 00 clear\ntrigger 02 aa\nframe replace 0 1 2222\nframe insert 0 2 3333\nrecord insert 1 02 00 2345 0 single 4444\nrecord move 1 0\n",
    )
    .unwrap();
    execute_exanimation_script(&mut app, &script, 0x23_0000..0x24_0000).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_exanimation(
            1,
            profile.exanimation,
            &profile.exanimation_double_size_modes,
        )
        .unwrap();
    assert_eq!(loaded.setting, 5);
    assert_eq!(loaded.header_value, 0xdead_beef);
    assert_eq!(loaded.trigger_mask, 4);
    assert_eq!(loaded.trigger_values[2], 0xaa);
    assert_eq!(loaded.records.len(), 2);
    assert_eq!(loaded.records[0].destination(), 0x2345);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_exanimation(
            1,
            profile.exanimation,
            &profile.exanimation_double_size_modes,
        )
        .unwrap();
    assert_eq!(restored.setting, 3);
    assert_eq!(restored.records.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_exanimation_frame_failure_preserves_application() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let revision = app.project_revision();
    let before = app.project().unwrap().save_snapshot();
    let edits =
        exanimation_edit_script::parse("LMEXAED1\nsetting 09\nframe replace 0 ff 1111\n").unwrap();
    assert!(commit_exanimation_edits(&mut app, &edits, 0x23_0000..0x24_0000).is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project().unwrap().save_snapshot(), before);
}

#[test]
fn terminal_overworld_script_commits_all_domains_reloads_and_undoes() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let directory =
        std::env::temp_dir().join(format!("lm-app-overworld-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let script = directory.join("Overworld edits.lmedit");
    fs::write(
        &script,
        "LMOWEDT1\nslot 0\npalette-owners 100 editable\npalette-owner 2 fixed\nlayer 2 1 2 1234\nevent 0 3 4\nendpoint 0 5 6 2\nmessage 0 1 2 44\nsprite 0 7 8 9 6 ccdd\npalette 3 9234\nanimation trigger 4 aa\nanimation frame replace 0 0 2222\n",
    )
    .unwrap();
    execute_overworld_script(&mut app, &script, 0x26_0000..0x28_0000).unwrap();
    let loaded = app
        .project()
        .unwrap()
        .load_complete_overworld(0, profile.overworld, &profile.exanimation_double_size_modes)
        .unwrap();
    assert_eq!(loaded.layers.layer2.tile(1, 2).unwrap(), 0x1234);
    assert_eq!(loaded.event_reveals.entries[0].source_tile, 3);
    assert_eq!(loaded.endpoints[0].x, 5);
    assert_eq!(loaded.messages[0].row(2).unwrap()[1], 0x44);
    assert_eq!(loaded.sprites[0].submap, Submap::StarWorld);
    assert_eq!(loaded.palette.colors[3], Bgr555(0x9234));
    assert_eq!(loaded.animation.trigger_values[4], 0xaa);
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    let restored = app
        .project()
        .unwrap()
        .load_complete_overworld(0, profile.overworld, &profile.exanimation_double_size_modes)
        .unwrap();
    assert_eq!(restored.layers.layer2.tile(1, 2).unwrap(), 2);
    assert_eq!(restored.palette.colors[3], Bgr555(3));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn late_protected_overworld_palette_edit_preserves_application() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let revision = app.project_revision();
    let before = app.project().unwrap().save_snapshot();
    let script = overworld_edit_script::parse(
        "LMOWEDT1\nslot 0\npalette-owners 100 editable\npalette-owner 2 fixed\nlayer 1 0 0 1234\npalette 2 9999\n",
    )
    .unwrap();
    assert!(commit_overworld_edits(&mut app, &script, 0x26_0000..0x28_0000).is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project().unwrap().save_snapshot(), before);
}
