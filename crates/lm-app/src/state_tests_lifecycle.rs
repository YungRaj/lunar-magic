use super::*;

#[test]
fn save_is_not_clean_until_frontend_confirms_persistence() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit",
            &[lm_project::RomWrite {
                offset: 1,
                bytes: vec![9],
            }],
        )
        .unwrap();
    assert!(app.project.as_ref().unwrap().is_modified());
    app.dispatch(Command::Save).unwrap();
    assert!(app.project.as_ref().unwrap().is_modified());
    app.confirm_saved(app.pending_save_request_id().unwrap())
        .unwrap();
    assert!(!app.project.as_ref().unwrap().is_modified());
}

#[test]
fn modified_project_requires_confirmation_before_close() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit",
            &[lm_project::RomWrite {
                offset: 7,
                bytes: vec![1],
            }],
        )
        .unwrap();
    assert_eq!(
        app.dispatch(Command::Quit).unwrap(),
        vec![FrontendEffect::ConfirmDiscardChanges { quit_after: true }]
    );
    assert!(app.project.is_some());
    assert_eq!(
        app.discard_and_close(true),
        vec![
            FrontendEffect::ProjectClosed,
            FrontendEffect::QuitApplication
        ]
    );
    assert_eq!(app.mode, EditorMode::NoProject);
}

#[test]
fn opening_over_a_modified_project_requires_explicit_confirmation() {
    let mut app = AppState::default();
    app.load_rom_at(test_rom(), Some("old.smc".into())).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit",
            &[lm_project::RomWrite {
                offset: 7,
                bytes: vec![1],
            }],
        )
        .unwrap();
    assert_eq!(
        app.dispatch(Command::Open).unwrap(),
        vec![FrontendEffect::ConfirmDiscardAndOpen]
    );
    assert!(app.project.is_some());
    assert_eq!(app.document_path, Some("old.smc".into()));
    assert!(matches!(
        app.discard_and_request_open().unwrap().as_slice(),
        [
            FrontendEffect::ProjectClosed,
            FrontendEffect::ChooseRom { .. }
        ]
    ));
    assert!(app.project.is_none());
    assert!(app.document_path.is_none());
}

#[test]
fn stale_save_acknowledgement_does_not_clear_newer_edits() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "first edit",
            &[lm_project::RomWrite {
                offset: 1,
                bytes: vec![1],
            }],
        )
        .unwrap();
    app.dispatch(Command::Save).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit while save is pending",
            &[lm_project::RomWrite {
                offset: 2,
                bytes: vec![2],
            }],
        )
        .unwrap();
    assert!(matches!(
        app.confirm_saved(app.pending_save_request_id().unwrap()),
        Err(AppError::StaleSaveAcknowledgement)
    ));
    assert!(app.project.as_ref().unwrap().is_modified());
    assert_eq!(app.status, "Save completed, but newer edits remain unsaved");
}

#[test]
fn stale_save_as_still_adopts_the_written_destination() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::SaveAs).unwrap();
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "newer edit",
            &[lm_project::RomWrite {
                offset: 2,
                bytes: vec![2],
            }],
        )
        .unwrap();
    assert!(matches!(
        app.confirm_saved_at(app.pending_save_request_id().unwrap(), "snapshot.smc"),
        Err(AppError::StaleSaveAcknowledgement)
    ));
    assert_eq!(app.document_path, Some("snapshot.smc".into()));
    assert!(app.project.as_ref().unwrap().is_modified());
}

#[test]
fn save_acknowledgement_requires_a_pending_snapshot() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    assert!(matches!(
        app.confirm_saved(99),
        Err(AppError::NoPendingSave)
    ));
}

#[test]
fn capabilities_follow_history_dirty_and_pending_save_state() {
    let mut app = AppState::default();
    assert_eq!(app.capabilities(), AppCapabilities::default());
    app.load_rom(test_rom()).unwrap();
    assert_eq!(
        app.capabilities(),
        AppCapabilities {
            project: ProjectStatus::OpenClean,
            ..AppCapabilities::default()
        }
    );
    app.project
        .as_mut()
        .unwrap()
        .apply_writes(
            "edit",
            &[lm_project::RomWrite {
                offset: 3,
                bytes: vec![7],
            }],
        )
        .unwrap();
    assert_eq!(app.capabilities().project, ProjectStatus::OpenModified);
    assert!(app.capabilities().can_save());
    assert!(app.capabilities().history.undo);
    app.dispatch(Command::Save).unwrap();
    assert_eq!(app.capabilities().save, SaveStatus::Pending);
    assert!(!app.capabilities().can_save());
    app.save_failed(app.pending_save_request_id().unwrap(), "disk full")
        .unwrap();
    assert!(app.capabilities().can_save());
    assert_eq!(app.status, "Save failed: disk full");
}

#[test]
fn asset_editor_modes_are_frontend_neutral() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    for (command, mode, status) in [
        (
            Command::ShowGraphics(0x32),
            EditorMode::Graphics(0x32),
            "Graphics 32",
        ),
        (
            Command::ShowPalette(5),
            EditorMode::Palette(5),
            "Palette 005",
        ),
        (
            Command::ShowExAnimation(0x105),
            EditorMode::ExAnimation(0x105),
            "ExAnimation 105",
        ),
        (
            Command::ShowLayer3(0x106),
            EditorMode::Layer3(0x106),
            "Layer 3 106",
        ),
    ] {
        assert_eq!(
            app.dispatch(command).unwrap(),
            vec![FrontendEffect::ViewChanged(mode)]
        );
        assert_eq!(app.mode, mode);
        assert_eq!(app.status, status);
    }
}

#[test]
fn copy_and_cut_are_typed_frontend_effects() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let selection = EditorSelection::new(ClipboardKind::Map16Tiles, vec![7]).unwrap();
    app.dispatch(Command::SetSelection(selection.clone()))
        .unwrap();
    assert!(app.capabilities().selection.copy);
    let payload = ClipboardPayload::from_map16_tiles(&[lm_level::Map16Tile::default()]);
    let copy = app.dispatch(Command::Copy(payload.clone())).unwrap();
    assert!(
        matches!(copy.as_slice(), [FrontendEffect::WriteClipboard(bytes)]
            if ClipboardPayload::decode(bytes).unwrap() == payload)
    );
    assert_eq!(
        app.dispatch(Command::Cut(payload)).unwrap(),
        vec![FrontendEffect::CutSelection {
            selection,
            clipboard: ClipboardPayload::from_map16_tiles(&[lm_level::Map16Tile::default()])
                .encode()
                .unwrap(),
        }]
    );
}

#[test]
fn layer_three_mode_accepts_only_its_lossless_clipboard_domains() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowLayer3(0x105)).unwrap();
    let selection = EditorSelection::new(ClipboardKind::Layer3RemapBytes, vec![1, 2]).unwrap();
    app.dispatch(Command::SetSelection(selection)).unwrap();
    let remap = ClipboardPayload::from_layer3_remap_bytes(&[0x80, 7]);
    assert!(matches!(
        app.dispatch(Command::Copy(remap)).unwrap().as_slice(),
        [FrontendEffect::WriteClipboard(_)]
    ));
    let tilemap = ClipboardPayload::from_layer3_tilemap_bytes(&[1, 2]);
    assert!(matches!(
        app.dispatch(Command::Paste(tilemap.encode().unwrap()))
            .unwrap()
            .as_slice(),
        [FrontendEffect::ApplyClipboard(payload)]
            if payload.kind == ClipboardKind::Layer3TilemapBytes
    ));
    let wrong = ClipboardPayload::from_map16_tiles(&[lm_level::Map16Tile::default()]);
    assert!(matches!(
        app.dispatch(Command::Paste(wrong.encode().unwrap())),
        Err(AppError::SelectionWrongMode {
            mode: EditorMode::Layer3(0x105),
            kind: ClipboardKind::Map16Tiles,
        })
    ));
}

#[test]
fn exanimation_mode_accepts_frames_without_confusing_other_domains() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(0x105)).unwrap();
    let selection = EditorSelection::new(ClipboardKind::ExAnimationFrames, vec![1, 2]).unwrap();
    app.dispatch(Command::SetSelection(selection)).unwrap();
    let frames = ClipboardPayload::from_exanimation_frames(&[
        lm_graphics::ExAnimationFrame {
            source_words: vec![0x1234],
        },
        lm_graphics::ExAnimationFrame {
            source_words: vec![0x5678],
        },
    ])
    .unwrap();
    assert!(matches!(
        app.dispatch(Command::Copy(frames.clone()))
            .unwrap()
            .as_slice(),
        [FrontendEffect::WriteClipboard(_)]
    ));
    assert!(matches!(
        app.dispatch(Command::Paste(frames.encode().unwrap()))
            .unwrap()
            .as_slice(),
        [FrontendEffect::ApplyClipboard(payload)]
            if payload.kind == ClipboardKind::ExAnimationFrames
    ));
    let wrong = ClipboardPayload::from_palette_colors(&[lm_graphics::Bgr555(1)]);
    assert!(matches!(
        app.dispatch(Command::Paste(wrong.encode().unwrap())),
        Err(AppError::SelectionWrongMode {
            mode: EditorMode::ExAnimation(0x105),
            kind: ClipboardKind::PaletteColors,
        })
    ));
}

#[test]
fn paste_is_decoded_and_rejected_in_the_wrong_editor() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowPalette(0)).unwrap();
    let payload = ClipboardPayload::from_palette_colors(&[lm_graphics::Bgr555(7)]);
    assert_eq!(
        app.dispatch(Command::Paste(payload.encode().unwrap()))
            .unwrap(),
        vec![FrontendEffect::ApplyClipboard(payload)]
    );
    let map16 = ClipboardPayload::from_map16_tiles(&[lm_level::Map16Tile::default()]);
    assert!(matches!(
        app.dispatch(Command::Paste(map16.encode().unwrap())),
        Err(AppError::SelectionWrongMode {
            mode: EditorMode::Palette(0),
            kind: ClipboardKind::Map16Tiles,
        })
    ));
    assert!(matches!(
        app.dispatch(Command::Paste(vec![1, 2, 3])),
        Err(AppError::Clipboard(ClipboardError::Truncated))
    ));
}

#[test]
fn selection_is_cleared_when_editor_context_changes() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(1)).unwrap();
    app.dispatch(Command::SetSelection(
        EditorSelection::new(ClipboardKind::GraphicsTiles, vec![2]).unwrap(),
    ))
    .unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    assert!(app.selection.is_none());
    assert!(!app.capabilities().selection.copy);
}
