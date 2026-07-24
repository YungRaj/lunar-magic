use super::*;

#[test]
fn profile_installation_is_atomic_revisioned_and_rom_clean() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let old_snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        app.profiled_controller_snapshot(),
        Err(AppError::NoRevisionProfile)
    ));
    let before = app.project().unwrap().rom.as_file_bytes().to_vec();
    let profile = test_profile();
    assert_eq!(
        app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
            .unwrap(),
        vec![FrontendEffect::RevisionProfileChanged {
            name: Some(profile.name.clone()),
            revision: 1,
        }]
    );
    assert_eq!(app.revision_profile(), Some(&profile));
    assert_eq!(app.capabilities().profile, ProfileStatus::Loaded);
    assert_eq!(
        app.profiled_controller_snapshot()
            .unwrap()
            .snapshot
            .revision,
        1
    );
    assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
    assert!(!app.project().unwrap().history.can_undo());

    assert!(matches!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: old_snapshot.revision,
            description: "stale old-profile edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 1,
                bytes: vec![1]
            }],
        }),
        Err(AppError::StaleProjectRevision { .. })
    ));
    assert!(
        app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
            .unwrap()
            .is_empty()
    );
    assert_eq!(app.project_revision(), 1);

    assert_eq!(
        app.dispatch(Command::ClearRevisionProfile).unwrap(),
        vec![FrontendEffect::RevisionProfileChanged {
            name: None,
            revision: 2
        }]
    );
    assert_eq!(app.capabilities().profile, ProfileStatus::Missing);
    assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
    assert!(
        app.dispatch(Command::ClearRevisionProfile)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_or_unrevisionable_profile_replacement_preserves_active_profile() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let valid = test_profile();
    app.dispatch(Command::InstallRevisionProfile(Box::new(valid.clone())))
        .unwrap();
    let revision = app.project_revision();

    let mut wrong = valid.clone();
    wrong.region = lm_rom::Region::Japan;
    assert!(matches!(
        app.dispatch(Command::InstallRevisionProfile(Box::new(wrong))),
        Err(AppError::RevisionProfile(
            RevisionProfileError::IdentityMismatch { .. }
        ))
    ));
    assert_eq!(app.revision_profile(), Some(&valid));
    assert_eq!(app.project_revision(), revision);

    let mut invalid_pointer = valid.clone();
    invalid_pointer.name = "Invalid pointer profile".into();
    invalid_pointer.level.layer1 = lm_project::LevelPointerTable {
        offset: 0x7000,
        entries: 1,
        stride: 3,
    };
    assert!(matches!(
        app.dispatch(Command::InstallRevisionProfile(Box::new(invalid_pointer))),
        Err(AppError::RevisionProfileAudit(
            RevisionProfileAuditError::InvalidTarget {
                domain: "level.layer1",
                index: 0,
                ..
            }
        ))
    ));
    assert_eq!(app.revision_profile(), Some(&valid));
    assert_eq!(app.project_revision(), revision);

    app.project_revision = u64::MAX;
    let mut replacement = valid.clone();
    replacement.name = "Another audited profile".into();
    assert!(matches!(
        app.dispatch(Command::InstallRevisionProfile(Box::new(replacement))),
        Err(AppError::ProjectRevisionOverflow)
    ));
    assert_eq!(app.revision_profile(), Some(&valid));
}

#[test]
fn document_replacement_clears_profiles_and_profile_changes_stale_open_requests() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let profile = test_profile();
    app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
        .unwrap();
    let request = match app.dispatch(Command::Open).unwrap().as_slice() {
        [FrontendEffect::ChooseRom { request_id }] => *request_id,
        effects => panic!("unexpected effects: {effects:?}"),
    };
    let mut replacement = profile;
    replacement.name = "Replacement profile".into();
    app.dispatch(Command::InstallRevisionProfile(Box::new(
        replacement.clone(),
    )))
    .unwrap();
    assert!(matches!(
        app.complete_open(request, test_rom(), Some("new.smc".into())),
        Err(AppError::OpenContextChanged)
    ));
    assert_eq!(app.revision_profile(), Some(&replacement));

    assert_eq!(
        app.dispatch(Command::Close).unwrap(),
        vec![FrontendEffect::ProjectClosed]
    );
    assert!(app.revision_profile().is_none());
    app.load_rom(test_rom()).unwrap();
    assert!(matches!(
        app.profiled_controller_snapshot(),
        Err(AppError::NoRevisionProfile)
    ));
}

#[test]
fn frontend_file_io_is_an_explicit_effect() {
    let mut app = AppState::default();
    let effects = app.dispatch(Command::Open).unwrap();
    let request_id = match effects.as_slice() {
        [FrontendEffect::ChooseRom { request_id }] => *request_id,
        _ => panic!("expected ROM chooser effect"),
    };
    app.complete_open(request_id, test_rom(), None).unwrap();
    assert_eq!(app.mode, EditorMode::Level(0x105));
    assert!(matches!(
        app.dispatch(Command::Save).unwrap().as_slice(),
        [FrontendEffect::ChooseSaveDestination { bytes, .. }] if bytes.len() == 0x8000
    ));
}

#[test]
fn path_aware_save_targets_the_existing_document() {
    let mut app = AppState::default();
    app.load_rom_at(test_rom(), Some("original.smc".into()))
        .unwrap();
    assert_eq!(app.document_path, Some("original.smc".into()));
    assert!(matches!(
        app.dispatch(Command::Save).unwrap().as_slice(),
        [FrontendEffect::PersistRomAt { path, bytes, .. }]
            if path == std::path::Path::new("original.smc") && bytes.len() == 0x8000
    ));
    app.confirm_saved(app.pending_save_request_id().unwrap())
        .unwrap();
    assert_eq!(app.document_path, Some("original.smc".into()));
}

#[test]
fn save_as_adopts_destination_and_future_save_reuses_it() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    assert!(matches!(
        app.dispatch(Command::SaveAs).unwrap().as_slice(),
        [FrontendEffect::ChooseSaveDestination { .. }]
    ));
    app.confirm_saved_at(app.pending_save_request_id().unwrap(), "copy.smc")
        .unwrap();
    assert_eq!(app.document_path, Some("copy.smc".into()));
    assert!(matches!(
        app.dispatch(Command::Save).unwrap().as_slice(),
        [FrontendEffect::PersistRomAt { path, .. }]
            if path == std::path::Path::new("copy.smc")
    ));
}

#[test]
fn overlapping_save_requests_cannot_replace_pending_snapshot() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::Save).unwrap();
    assert!(matches!(
        app.dispatch(Command::SaveAs),
        Err(AppError::SaveAlreadyPending)
    ));
    app.confirm_saved_at(app.pending_save_request_id().unwrap(), "first.smc")
        .unwrap();
    assert_eq!(app.document_path, Some("first.smc".into()));
}

#[test]
fn view_commands_require_a_project() {
    let mut app = AppState::default();
    assert!(matches!(
        app.dispatch(Command::ShowMap16),
        Err(AppError::NoProject)
    ));
    app.load_rom(test_rom()).unwrap();
    assert_eq!(
        app.dispatch(Command::ShowMap16).unwrap(),
        vec![FrontendEffect::ViewChanged(EditorMode::Map16)]
    );
}
