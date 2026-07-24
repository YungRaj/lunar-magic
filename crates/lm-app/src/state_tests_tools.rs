use super::*;

fn emulator_tool(id: &str) -> ExternalTool {
    ExternalTool {
        id: id.into(),
        name: "Test Emulator".into(),
        executable: "/Applications/Test Emulator".into(),
        arguments: vec!["{rom}".into(), "--level={level_hex}".into()],
        working_directory: Some("{project_dir}".into()),
        subscriptions: vec![ToolEvent::ProjectSaved],
    }
}

#[test]
fn external_tool_commands_expand_to_frontend_effects_without_spawning() {
    let mut app = AppState::default();
    app.load_rom_at(test_rom(), Some("/tmp/My Hack/game.smc".into()))
        .unwrap();
    app.set_external_tools(vec![emulator_tool("emu")]).unwrap();

    let expected = FrontendEffect::LaunchExternalTool(ToolInvocation {
        tool_id: "emu".into(),
        executable: "/Applications/Test Emulator".into(),
        arguments: vec!["/tmp/My Hack/game.smc".into(), "--level=105".into()],
        working_directory: Some("/tmp/My Hack".into()),
    });
    assert_eq!(
        app.dispatch(Command::RunExternalTool("emu".into()))
            .unwrap(),
        vec![expected.clone()]
    );
    assert_eq!(
        app.external_tool_event(ToolEvent::ProjectSaved).unwrap(),
        vec![expected]
    );
    assert!(
        app.external_tool_event(ToolEvent::ProjectOpened)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_tool_replacement_is_atomic_and_unknown_ids_are_structured() {
    let mut app = AppState::default();
    app.set_external_tools(vec![emulator_tool("emu")]).unwrap();
    assert!(matches!(
        app.set_external_tools(vec![emulator_tool("same"), emulator_tool("same")]),
        Err(ExternalToolError::DuplicateId(id)) if id == "same"
    ));
    assert_eq!(app.external_tools(), &[emulator_tool("emu")]);
    let mut duplicate_event = emulator_tool("other");
    duplicate_event.subscriptions.push(ToolEvent::ProjectSaved);
    assert!(matches!(
        app.set_external_tools(vec![duplicate_event]),
        Err(ExternalToolError::DuplicateSubscription {
            tool_id,
            event: ToolEvent::ProjectSaved,
        }) if tool_id == "other"
    ));
    assert_eq!(app.external_tools(), &[emulator_tool("emu")]);
    assert!(matches!(
        app.dispatch(Command::RunExternalTool("missing".into())),
        Err(AppError::ExternalTool(ExternalToolError::UnknownTool(id))) if id == "missing"
    ));
}

#[test]
fn open_save_and_level_transitions_emit_subscribed_tool_effects() {
    let mut tool = emulator_tool("events");
    tool.subscriptions = vec![
        ToolEvent::ProjectOpened,
        ToolEvent::ProjectSaved,
        ToolEvent::LevelChanged,
    ];
    let mut app = AppState::default();
    app.set_external_tools(vec![tool]).unwrap();

    let request = match app.dispatch(Command::Open).unwrap().as_slice() {
        [FrontendEffect::ChooseRom { request_id }] => *request_id,
        effects => panic!("unexpected open effects: {effects:?}"),
    };
    let opened = app
        .complete_open(
            request,
            test_rom(),
            Some("/tmp/My Hack 日本語/game.smc".into()),
        )
        .unwrap();
    assert!(matches!(
        opened.as_slice(),
        [FrontendEffect::LaunchExternalTool(ToolInvocation { arguments, .. })]
            if arguments[1] == "--level=105"
    ));

    let changed = app.dispatch(Command::SelectLevel(0x106)).unwrap();
    assert!(matches!(
        changed.as_slice(),
        [FrontendEffect::ViewChanged(EditorMode::Level(0x106)),
         FrontendEffect::LaunchExternalTool(ToolInvocation { arguments, .. })]
            if arguments[1] == "--level=106"
    ));
    assert_eq!(
        app.dispatch(Command::SelectLevel(0x106)).unwrap(),
        vec![FrontendEffect::ViewChanged(EditorMode::Level(0x106))]
    );

    app.dispatch(Command::Save).unwrap();
    let saved = app
        .confirm_saved(app.pending_save_request_id().unwrap())
        .unwrap();
    assert!(matches!(
        saved.as_slice(),
        [FrontendEffect::LaunchExternalTool(ToolInvocation { arguments, .. })]
            if arguments[0] == "/tmp/My Hack 日本語/game.smc"
    ));
}

#[test]
fn broken_event_template_reports_a_diagnostic_without_blocking_navigation() {
    let mut tool = emulator_tool("broken");
    tool.arguments = vec!["{unknown}".into()];
    tool.subscriptions = vec![ToolEvent::LevelChanged];
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.set_external_tools(vec![tool]).unwrap();

    let effects = app.dispatch(Command::SelectLevel(0x106)).unwrap();
    assert_eq!(app.mode, EditorMode::Level(0x106));
    assert!(matches!(
        effects.as_slice(),
        [FrontendEffect::ViewChanged(EditorMode::Level(0x106)),
         FrontendEffect::ExternalToolFailed { tool_id, error: ExternalToolError::UnknownPlaceholder(key) }]
            if tool_id == "broken" && key == "unknown"
    ));
}

#[test]
fn level_navigation_history_moves_backward_forward_and_branches() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::SelectLevel(0x106)).unwrap();
    app.dispatch(Command::SelectLevel(0x107)).unwrap();

    app.dispatch(Command::NavigateLevel(
        crate::LevelNavigationDirection::Back,
    ))
    .unwrap();
    assert_eq!(app.mode, EditorMode::Level(0x106));
    app.dispatch(Command::NavigateLevel(
        crate::LevelNavigationDirection::Forward,
    ))
    .unwrap();
    assert_eq!(app.mode, EditorMode::Level(0x107));

    app.dispatch(Command::NavigateLevel(
        crate::LevelNavigationDirection::Back,
    ))
    .unwrap();
    app.dispatch(Command::SelectLevel(0x108)).unwrap();
    assert!(
        app.dispatch(Command::NavigateLevel(
            crate::LevelNavigationDirection::Forward,
        ))
        .unwrap()
        .is_empty()
    );
    assert_eq!(app.mode, EditorMode::Level(0x108));
    assert_eq!(app.status, "No later level");
}

#[test]
fn level_navigation_restores_world_origin_and_exact_zoom() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let viewport = crate::LevelViewport::new(lm_render::Point { x: 96, y: -24 }, 5, 2).unwrap();
    assert_eq!(
        app.dispatch(Command::SetLevelViewport(viewport)).unwrap(),
        vec![FrontendEffect::LevelViewportChanged(viewport)]
    );
    app.dispatch(Command::SelectLevel(0x106)).unwrap();
    let effects = app
        .dispatch(Command::NavigateLevel(
            crate::LevelNavigationDirection::Back,
        ))
        .unwrap();
    assert_eq!(
        effects,
        vec![
            FrontendEffect::ViewChanged(EditorMode::Level(0x105)),
            FrontendEffect::LevelViewportChanged(viewport),
        ]
    );
    assert_eq!(viewport.zoom(), (5, 2));
}

#[test]
fn viewport_updates_require_an_active_level_view() {
    let mut app = AppState::default();
    let viewport = crate::LevelViewport::default();
    assert!(matches!(
        app.dispatch(Command::SetLevelViewport(viewport)),
        Err(AppError::NoProject)
    ));
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    assert!(matches!(
        app.dispatch(Command::SetLevelViewport(viewport)),
        Err(AppError::NoLevelView)
    ));
}
