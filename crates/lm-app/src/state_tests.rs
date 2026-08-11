use super::*;
use crate::RecoverySnapshot;
use crate::ToolInvocation;

fn localization(locale: &str) -> LocalizationCatalog {
    LocalizationCatalog::new(
        locale,
        crate::UiTextKey::ALL.map(|key| (key, format!("{locale}-{key:?}"))),
    )
    .unwrap()
}

#[test]
fn frontend_configuration_replacement_is_validated_and_atomic() {
    let mut app = AppState::default();
    app.set_localization(localization("en-US")).unwrap();
    let mut invalid_catalog = localization("fr-FR");
    invalid_catalog.locale.clear();
    assert!(app.set_localization(invalid_catalog).is_err());
    assert_eq!(app.localization().unwrap().locale, "en-US");

    let toolbar = ToolbarConfig {
        items: vec![crate::ToolbarItem::Action {
            id: "file.open".into(),
            action: crate::ToolbarAction::Open,
            label: crate::UiTextKey::FileOpen,
        }],
    };
    app.set_toolbar(toolbar.clone()).unwrap();
    assert!(
        app.set_toolbar(ToolbarConfig {
            items: vec![crate::ToolbarItem::Separator],
        })
        .is_err()
    );
    assert_eq!(app.toolbar(), Some(&toolbar));

    let gesture = ShortcutGesture {
        modifiers: crate::ShortcutModifiers::PRIMARY,
        key: crate::ShortcutKey::Character('s'),
    };
    let shortcuts = ShortcutConfig {
        bindings: vec![crate::ShortcutBinding {
            gesture,
            action: ToolbarAction::Save,
        }],
    };
    app.set_shortcuts(shortcuts.clone()).unwrap();
    let mut invalid = shortcuts.clone();
    invalid.bindings.push(invalid.bindings[0]);
    assert!(app.set_shortcuts(invalid).is_err());
    assert_eq!(app.shortcuts(), Some(&shortcuts));
    assert_eq!(app.shortcut_action(gesture), Some(ToolbarAction::Save));

    let aggregate = FrontendConfig {
        localization: localization("de-DE"),
        toolbar: toolbar.clone(),
        shortcuts: shortcuts.clone(),
    };
    app.set_frontend_config(aggregate).unwrap();
    let mut invalid_localization = localization("ja-JP");
    invalid_localization.locale.clear();
    assert!(
        app.set_frontend_config(FrontendConfig {
            localization: invalid_localization,
            toolbar: ToolbarConfig {
                items: vec![crate::ToolbarItem::Action {
                    id: "different".into(),
                    action: ToolbarAction::Undo,
                    label: crate::UiTextKey::EditUndo,
                }],
            },
            shortcuts: ShortcutConfig::default(),
        })
        .is_err()
    );
    assert_eq!(app.localization().unwrap().locale, "de-DE");
    assert_eq!(app.toolbar(), Some(&toolbar));
    assert_eq!(app.shortcuts(), Some(&shortcuts));

    app.clear_toolbar();
    assert_eq!(app.toolbar(), None);
    assert_eq!(app.shortcuts(), Some(&shortcuts));
    app.clear_localization();
    assert_eq!(app.localization(), None);
    assert_eq!(app.shortcuts(), Some(&shortcuts));
}

#[test]
fn toolbar_and_shortcut_activation_share_authoritative_enablement() {
    let mut app = AppState::default();
    assert_eq!(
        app.activate_toolbar_action(ToolbarAction::Open),
        Some(ToolbarActivation::command(Command::Open))
    );
    assert_eq!(app.activate_toolbar_action(ToolbarAction::ShowMap16), None);
    app.load_rom(test_rom()).unwrap();
    assert!(!app.toolbar_action_enabled(ToolbarAction::LevelBack));
    app.dispatch(Command::SelectLevel(0x106)).unwrap();
    assert!(app.toolbar_action_enabled(ToolbarAction::LevelBack));
    assert!(!app.toolbar_action_enabled(ToolbarAction::LevelForward));
    app.dispatch(Command::NavigateLevel(
        crate::LevelNavigationDirection::Back,
    ))
    .unwrap();
    assert!(app.toolbar_action_enabled(ToolbarAction::LevelForward));
    assert_eq!(
        app.activate_toolbar_action(ToolbarAction::LevelForward),
        Some(ToolbarActivation::command(Command::NavigateLevel(
            crate::LevelNavigationDirection::Forward
        )))
    );
    assert_eq!(
        app.activate_toolbar_action(ToolbarAction::ShowMap16),
        Some(ToolbarActivation::command(Command::ShowMap16))
    );
    assert_eq!(app.activate_toolbar_action(ToolbarAction::Copy), None);
    app.dispatch(Command::ShowMap16).unwrap();
    app.dispatch(Command::SetSelection(
        EditorSelection::new(ClipboardKind::Map16Tiles, vec![0]).unwrap(),
    ))
    .unwrap();
    assert_eq!(
        app.activate_toolbar_action(ToolbarAction::Copy),
        Some(ToolbarActivation::RequestCopyPayload)
    );
    assert_eq!(
        app.activate_toolbar_action(ToolbarAction::Paste),
        Some(ToolbarActivation::RequestClipboardBytes)
    );

    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit",
            &[lm_project::RomWrite {
                offset: 2,
                bytes: vec![9],
            }],
        )
        .unwrap();
    assert!(app.toolbar_action_enabled(ToolbarAction::Save));
    assert!(app.toolbar_action_enabled(ToolbarAction::Undo));
    app.dispatch(Command::Save).unwrap();
    assert!(!app.toolbar_action_enabled(ToolbarAction::Open));
    assert!(!app.toolbar_action_enabled(ToolbarAction::SaveAs));
}

fn test_rom() -> Vec<u8> {
    let mut bytes = vec![0; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    let profile = test_profile();
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
    let pointer = lm_rom::pc_to_snes(lm_rom::Mapper::LoRom, 0x6000)
        .unwrap()
        .to_le_bytes();
    for table in tables {
        for index in 0..table.entries {
            let offset = table.offset + index * table.stride;
            bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
        }
    }
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

#[test]
fn lunar_magic_snapshot_limit_persists_across_projects_and_obeys_disabled_boundary() {
    let mut app = AppState::default();
    assert_eq!(app.undo_snapshot_limit(), 33);
    assert_eq!(app.undo_operation_limit(), 32);
    app.set_undo_snapshot_limit(3).unwrap();
    app.load_rom(test_rom()).unwrap();
    assert_eq!(app.project().unwrap().history.limit(), 2);

    for (revision, offset) in [(0, 2), (1, 3), (2, 4)] {
        app.dispatch(Command::CommitRomWrites {
            expected_revision: revision,
            description: format!("history edit {revision}"),
            writes: vec![lm_project::RomWrite {
                offset,
                bytes: vec![revision as u8 + 1],
            }],
        })
        .unwrap();
    }
    assert_eq!(app.project().unwrap().history.undo_len(), 2);

    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().history.redo_len(), 1);
    app.set_undo_snapshot_limit(1).unwrap();
    assert_eq!(app.project().unwrap().history.undo_len(), 0);
    assert_eq!(app.project().unwrap().history.redo_len(), 0);
    assert_eq!(app.project().unwrap().history.limit(), 0);

    app.dispatch(Command::CommitRomWrites {
        expected_revision: 4,
        description: "disabled history edit".into(),
        writes: vec![lm_project::RomWrite {
            offset: 5,
            bytes: vec![9],
        }],
    })
    .unwrap();
    assert_eq!(app.project().unwrap().history.undo_len(), 0);

    let before = app.undo_snapshot_limit();
    assert!(app.set_undo_snapshot_limit(52).is_err());
    assert_eq!(app.undo_snapshot_limit(), before);
    assert_eq!(app.project().unwrap().history.limit(), 0);
}

#[test]
fn crash_recovery_restores_exact_dirty_bytes_level_and_saved_baseline() {
    let original = test_rom();
    let mut app = AppState::default();
    app.load_rom_at(original.clone(), Some("source.smc".into()))
        .unwrap();
    app.dispatch(Command::SelectLevel(0x1ab)).unwrap();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "unsaved recovery edit".into(),
        writes: vec![lm_project::RomWrite {
            offset: 4,
            bytes: vec![0xa5],
        }],
    })
    .unwrap();
    let snapshot = app.recovery_snapshot().unwrap();
    assert_eq!(snapshot.saved_baseline, original);
    assert_ne!(snapshot.current_rom, snapshot.saved_baseline);
    assert_eq!(snapshot.level, Some(0x1ab));

    let mut recovered = AppState::default();
    recovered.load_recovery(snapshot.clone()).unwrap();
    assert_eq!(recovered.document_path, None);
    assert_eq!(recovered.mode, EditorMode::Level(0x1ab));
    assert_eq!(
        recovered.controller_snapshot().unwrap().rom_bytes,
        snapshot.current_rom
    );
    assert_eq!(
        recovered.recovery_snapshot().unwrap().saved_baseline,
        snapshot.saved_baseline
    );
    assert_eq!(
        recovered.capabilities().project,
        ProjectStatus::OpenModified
    );
}

#[test]
fn crash_recovery_composes_a_staged_editor_mutation_without_changing_live_state() {
    let original = test_rom();
    let mut app = AppState::default();
    app.load_rom(original.clone()).unwrap();
    let project = app.project().unwrap();
    let mapper = project.identity.as_ref().unwrap().mapper;
    let logical_len = project.rom.logical_len();
    let mutation = lm_project::RomMutation {
        mapper,
        expected_len: logical_len,
        appended: Vec::new(),
        writes: vec![lm_project::RomWrite {
            offset: 4,
            bytes: vec![0xa5],
        }],
    };

    let snapshot = app
        .recovery_snapshot_with_mutation(&mutation, Some(0x105))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.saved_baseline, original);
    assert_ne!(snapshot.current_rom, snapshot.saved_baseline);
    assert_eq!(snapshot.level, Some(0x105));
    assert_eq!(app.capabilities().project, ProjectStatus::OpenClean);
    assert_eq!(app.project().unwrap().history.undo_len(), 0);
    assert_eq!(app.project().unwrap().save_snapshot(), original);
}

#[test]
fn crash_recovery_rejects_clean_records_and_open_project_replacement() {
    let original = test_rom();
    let clean = RecoverySnapshot {
        revision: 7,
        level: Some(0x105),
        saved_baseline: original.clone(),
        current_rom: original.clone(),
    };
    let mut app = AppState::default();
    assert!(matches!(
        app.load_recovery(clean),
        Err(AppError::Recovery(_))
    ));

    app.load_rom(original).unwrap();
    let dirty = RecoverySnapshot {
        revision: 8,
        level: None,
        saved_baseline: test_rom(),
        current_rom: {
            let mut bytes = test_rom();
            bytes[3] = 1;
            bytes
        },
    };
    assert!(matches!(
        app.load_recovery(dirty),
        Err(AppError::ProjectAlreadyOpen)
    ));
}

#[test]
fn compatibility_report_is_path_free_and_classifies_pristine_runtime_state() {
    let mut app = AppState::default();
    app.load_rom_at(
        crate::test_support::pristine_smw_us_rom_bytes(),
        Some("/secret/projects/private-hack.sfc".into()),
    )
    .unwrap();
    let report = app.rom_compatibility_report();
    for field in [
        "Game: Super Mario World",
        "Region: North America",
        "Revision: 0",
        "Mapper: LoROM",
        "Current identity: valid",
        "Copier header: absent",
        "Logical bytes: 524288",
        "Checksum status: valid",
        "Dirty: false",
        "Revision profile: not-installed",
        "Layer 2 runtime: absent",
        "Map16 runtime: absent",
        "Lfix3 runtime: absent",
        "Compatibility warnings: 0",
    ] {
        assert!(
            report.text.contains(field),
            "missing report field {field:?}"
        );
    }
    assert_eq!(report.warnings, 0);
    assert!(report.text.len() < 8192);
    assert!(!report.text.contains("secret"));
    assert!(!report.text.contains("private-hack"));
}

#[test]
fn compatibility_report_surfaces_checksum_and_partial_runtime_corruption() {
    let mut app = AppState::default();
    app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
        .unwrap();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "corrupt runtime marker for diagnostic".into(),
        writes: vec![lm_project::RomWrite {
            offset: lm_profile::SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET,
            bytes: b"LM\xff\xff".to_vec(),
        }],
    })
    .unwrap();
    let report = app.rom_compatibility_report();
    assert!(report.text.contains("Checksum status: mismatch"));
    assert!(report.text.contains("Dirty: true"));
    assert!(report.text.contains("Layer 2 runtime: probe-failed"));
    assert!(report.text.contains("Warning: stored SNES checksum"));
    assert!(
        report
            .text
            .contains("Warning: Layer 2 runtime probe failed")
    );
    assert_eq!(report.warnings, 2);
}

#[test]
fn compatibility_report_distinguishes_headered_and_sa1_variants() {
    let pristine = crate::test_support::pristine_smw_us_rom_bytes();
    let mut headered = vec![0xa5; lm_rom::COPIER_HEADER_LEN];
    headered.extend_from_slice(&pristine);
    let mut app = AppState::default();
    app.load_rom(headered).unwrap();
    let report = app.rom_compatibility_report();
    assert!(report.text.contains("Copier header: present"));
    assert!(report.text.contains("Physical bytes: 524800"));
    assert!(report.text.contains("Logical bytes: 524288"));
    assert_eq!(report.warnings, 0);

    let mut sa1 = test_rom();
    sa1[0x7fd5] = 0x23;
    let checksum = lm_rom::compute_snes_checksum(&sa1, 0x7fdc).unwrap();
    sa1[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut app = AppState::default();
    app.load_rom(sa1).unwrap();
    let report = app.rom_compatibility_report();
    assert!(report.text.contains("Mapper: SA-1"));
    assert!(report.text.contains("Layer 2 runtime: not-applicable"));
    assert!(report.text.contains("Map16 runtime: not-applicable"));
    assert!(report.text.contains("Lfix3 runtime: not-applicable"));
    assert_eq!(report.warnings, 0);
}

#[test]
fn compatibility_report_detects_when_current_bytes_break_opened_identity() {
    let mut app = AppState::default();
    app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
        .unwrap();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "break internal title".into(),
        writes: vec![lm_project::RomWrite {
            offset: 0x7fc0,
            bytes: vec![b'X'],
        }],
    })
    .unwrap();
    let report = app.rom_compatibility_report();
    assert!(report.text.contains("Current identity: invalid"));
    assert!(
        report
            .text
            .contains("Warning: current ROM identity validation failed")
    );
    assert_eq!(report.warnings, 2);
}

#[test]
fn compatibility_report_reaudits_an_installed_revision_profile() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(test_profile())))
        .unwrap();
    let report = app.rom_compatibility_report();
    assert!(report.text.contains("Revision profile: audited ("));
    assert!(report.text.contains("pointer entries)"));
}

fn test_profile() -> RevisionProfile {
    let mut profile = lm_profile::test_support::profile();
    profile.mapper = lm_rom::Mapper::LoRom;
    profile.level.mapper = lm_rom::Mapper::LoRom;
    profile.map16.mapper = lm_rom::Mapper::LoRom;
    profile.graphics.mapper = lm_rom::Mapper::LoRom;
    profile.palette.mapper = lm_rom::Mapper::LoRom;
    profile.exanimation.mapper = lm_rom::Mapper::LoRom;
    profile.palette_installation = lm_project::InstalledLayout::Unconditional(profile.palette);
    profile.exanimation_installation =
        lm_project::InstalledLayout::Unconditional(lm_project::InstalledExAnimationRomLayout {
            payload: profile.exanimation,
            pointer_presence_mask: 0x00ff_0000,
            pointer_locator: None,
        });
    let expanded = profile.expanded_settings.as_mut().unwrap();
    expanded.mapper = lm_rom::Mapper::LoRom;
    expanded.table_offset = 0x5600;
    expanded.entries = 1;
    profile.overworld.layers.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.event_reveals.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.endpoints.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.messages.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.sprites.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.palette.mapper = lm_rom::Mapper::LoRom;
    profile.overworld.animation.mapper = lm_rom::Mapper::LoRom;
    profile
}

#[path = "state_tests_lifecycle.rs"]
mod lifecycle;
#[path = "state_tests_profile.rs"]
mod profile;
#[path = "state_tests_tools.rs"]
mod tools;
#[path = "state_tests_transactions.rs"]
mod transactions;
