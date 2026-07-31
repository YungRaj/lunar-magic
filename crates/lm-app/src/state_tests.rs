use super::*;
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
