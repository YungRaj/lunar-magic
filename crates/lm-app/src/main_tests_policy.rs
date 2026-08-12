use super::*;

fn frontend_fixture() -> FrontendConfig {
    FrontendConfig {
        localization: lm_app::LocalizationCatalog::new(
            "en-US",
            UiTextKey::ALL.map(|key| (key, format!("{key:?}"))),
        )
        .unwrap(),
        toolbar: ToolbarConfig {
            items: vec![
                ToolbarItem::Action {
                    id: "view.map16".into(),
                    action: ToolbarAction::ShowMap16,
                    label: UiTextKey::ViewMap16,
                },
                ToolbarItem::Separator,
                ToolbarItem::Action {
                    id: "file.save".into(),
                    action: ToolbarAction::Save,
                    label: UiTextKey::FileSave,
                },
            ],
        },
        shortcuts: ShortcutConfig {
            bindings: vec![ShortcutBinding {
                gesture: ShortcutGesture {
                    modifiers: ShortcutModifiers::PRIMARY,
                    key: ShortcutKey::Character('m'),
                },
                action: ToolbarAction::ShowMap16,
            }],
        },
    }
}

fn tool_fixture() -> ToolConfig {
    ToolConfig {
        tools: vec![ExternalTool {
            id: "emu".into(),
            name: "Émulateur".into(),
            executable: "/Applications/Emulator App".into(),
            arguments: vec!["--rom".into(), "{rom}".into(), "--level={level_hex}".into()],
            working_directory: Some("{project_dir}".into()),
            subscriptions: vec![ToolEvent::ProjectSaved],
            replace_tile_editor_palette: false,
        }],
    }
}

#[test]
fn external_tool_configuration_installs_and_expands_unicode_context_atomically() {
    let directory = std::env::temp_dir().join(format!("lm-app-tools-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let valid = directory.join("Tools 日本語.lmtools");
    let invalid = directory.join("invalid.lmtools");
    fs::write(&valid, tool_fixture().encode().unwrap()).unwrap();
    fs::write(&invalid, b"LMTOOLS1").unwrap();

    let mut app = AppState::default();
    app.load_rom_at(
        profiled_rom(&lm_profile::test_support::profile()),
        Some(directory.join("My Hack 日本語.smc")),
    )
    .unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    execute_tool_command(&mut app, shell_command::ToolCommand::Install(valid)).unwrap();
    let effects = app
        .dispatch(Command::RunExternalTool("emu".into()))
        .unwrap();
    let FrontendEffect::LaunchExternalTool(invocation) = &effects[0] else {
        panic!("expected typed external-tool invocation");
    };
    assert_eq!(
        invocation.arguments[1],
        directory.join("My Hack 日本語.smc").display().to_string()
    );
    assert_eq!(invocation.arguments[2], "--level=105");

    assert!(execute_tool_command(&mut app, shell_command::ToolCommand::Install(invalid)).is_err());
    assert_eq!(app.external_tools(), tool_fixture().tools);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn frontend_configuration_installs_and_routes_toolbar_and_shortcut_actions() {
    let directory = std::env::temp_dir().join(format!("lm-app-ui-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let path = directory.join("Frontend config.lmuicfg");
    fs::write(&path, frontend_fixture().encode().unwrap()).unwrap();
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    install_ui_config(&mut app, &path).unwrap();
    assert_eq!(app.localization().unwrap().locale(), "en-US");
    execute_ui_command(
        &mut app,
        shell_command::UiCommand::Action("map16".into()),
        false,
    )
    .unwrap();
    assert_eq!(app.mode, lm_app::EditorMode::Map16);
    app.dispatch(Command::SelectLevel(0x106)).unwrap();
    execute_ui_command(
        &mut app,
        shell_command::UiCommand::Action("level-back".into()),
        false,
    )
    .unwrap();
    assert_eq!(app.mode, lm_app::EditorMode::Level(0x105));
    execute_ui_command(
        &mut app,
        shell_command::UiCommand::Action("level-forward".into()),
        false,
    )
    .unwrap();
    assert_eq!(app.mode, lm_app::EditorMode::Level(0x106));
    execute_ui_command(
        &mut app,
        shell_command::UiCommand::Shortcut("primary+m".into()),
        false,
    )
    .unwrap();
    assert_eq!(app.mode, lm_app::EditorMode::Map16);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn in_place_rom_save_requires_explicit_shell_capability() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-in-place-policy-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let path = directory.join("source.smc");
    let profile = lm_profile::test_support::profile();
    let bytes = profiled_rom(&profile);
    fs::write(&path, &bytes).unwrap();
    let mut app = AppState::default();
    app.load_rom_at(bytes.clone(), Some(path.clone())).unwrap();

    let error = save(&mut app, false).unwrap_err().to_string();
    assert!(error.contains("in-place ROM replacement is disabled"));
    assert_eq!(fs::read(&path).unwrap(), bytes);

    save(&mut app, true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn malformed_frontend_bundle_cannot_replace_active_configuration() {
    let directory = std::env::temp_dir().join(format!("lm-app-ui-atomic-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let valid = directory.join("valid.lmuicfg");
    let invalid = directory.join("invalid.lmuicfg");
    fs::write(&valid, frontend_fixture().encode().unwrap()).unwrap();
    fs::write(&invalid, b"LMUICFG1").unwrap();
    let mut app = AppState::default();
    install_ui_config(&mut app, &valid).unwrap();
    let localization = app.localization().unwrap().clone();
    let toolbar = app.toolbar().unwrap().clone();
    let shortcuts = app.shortcuts().unwrap().clone();
    assert!(install_ui_config(&mut app, &invalid).is_err());
    assert_eq!(app.localization(), Some(&localization));
    assert_eq!(app.toolbar(), Some(&toolbar));
    assert_eq!(app.shortcuts(), Some(&shortcuts));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn save_as_publishes_then_adopts_the_document_path_and_clean_baseline() {
    let directory = std::env::temp_dir().join(format!("lm-app-save-as-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let destination = directory.join("My Hack 日本語.smc");
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&lm_profile::test_support::profile()))
        .unwrap();

    save_as(&mut app, &destination).unwrap();

    assert!(!app.project().unwrap().is_modified());
    assert_eq!(
        fs::read(&destination).unwrap(),
        app.project().unwrap().save_snapshot()
    );
    assert_eq!(app.recent_documents().paths().first(), Some(&destination));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn protected_graphics_tile_and_wrong_ownership_shape_roll_back_whole_script() {
    let profile = lm_profile::test_support::profile();
    let mut app = AppState::default();
    app.load_rom(profiled_rom(&profile)).unwrap();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
        .unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let revision = app.project_revision();
    let before = app.project().unwrap().save_snapshot();
    let protected = graphics_edit_script::parse(&format!(
        "LMGFXED1\nowners 3 editable\nowner 0 fixed\nset 1 {}\nset 0 {}\n",
        "a".repeat(64),
        "b".repeat(64)
    ))
    .unwrap();
    assert!(commit_graphics_edits(&mut app, &protected, 0x1_0000..0x2_0000).is_err());
    let wrong_shape = graphics_edit_script::parse("LMGFXED1\nowners 2 editable\n").unwrap();
    assert!(commit_graphics_edits(&mut app, &wrong_shape, 0x1_0000..0x2_0000).is_err());
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project().unwrap().save_snapshot(), before);
}
