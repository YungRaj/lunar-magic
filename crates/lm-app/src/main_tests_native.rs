use super::*;
use crate::editor_shell::{execute_native_assets_script, execute_owned_editor_script};
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
