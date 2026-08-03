use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette};
use lm_level::{ExpandedLevelSettingsRecord, MwlSection, MwlSectionKind};
use lm_project::MwlOptionalLevelAssets;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn path(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lm-mwl-shell-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn file() -> MwlFile {
    let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
        std::array::from_fn(|_| MwlSection::default());
    sections[MwlSectionKind::LevelHeader as usize].bytes =
        vec![0; MwlLevelHeaderSection::ENCODED_LEN];
    MwlFile {
        version: MwlFile::CURRENT_VERSION,
        flags: 0,
        attribution: [0; MwlFile::ATTRIBUTION_LEN],
        sections,
    }
}

fn optional_assets() -> MwlOptionalLevelAssets {
    MwlOptionalLevelAssets {
        palette_metadata: [0, 0x10_8031],
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
    }
}

#[test]
fn layer3_settings_spec_participates_in_history_and_save() {
    let document = path("layer3-target.mwl");
    let spec = path("layer3-settings.txt");
    let mut target = file();
    let mut bytes = [0_u8; ExpandedLevelSettingsRecord::ENCODED_LEN];
    bytes[0] = 0x5a;
    bytes[1] = 0x81;
    target.set_expanded_settings_section(&ExpandedLevelSettingsRecord::decode(&bytes).unwrap());
    fs::write(&document, target.encode().unwrap()).unwrap();
    fs::write(
        &spec,
        "LMMWLL31\nenabled true\nfile 028\nlength-selector 2\noffset-selector 0\nexpanded-mode 89abcdef\n",
    )
    .unwrap();
    let mut session = None;
    open_mwl_document(&mut session, &document).unwrap();

    edit_layer3_settings(&mut session, &spec).unwrap();

    let controller = session.as_mut().unwrap();
    let edited = controller.value().expanded_settings_section().unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(edited.word(0).unwrap(), 0xa15a);
    assert_eq!(
        edited
            .layer3_tilemap_graphics_descriptor()
            .unwrap()
            .packed(),
        0x2028
    );
    assert_eq!(edited.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
    assert!(controller.undo(1).unwrap());
    assert_eq!(
        controller
            .value()
            .expanded_settings_section()
            .unwrap()
            .word(0)
            .unwrap(),
        0x815a
    );
    assert!(controller.redo(2).unwrap());
    save_mwl_document(&mut session).unwrap();
    let saved = MwlFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(
        saved
            .expanded_settings_section()
            .unwrap()
            .layer3_tilemap_graphics_descriptor()
            .unwrap()
            .packed(),
        0x2028
    );
    assert_eq!(
        saved
            .expanded_settings_section()
            .unwrap()
            .layer3_expanded_mode_flags()
            .packed(),
        0x89ab_cdef
    );
    fs::remove_file(document).unwrap();
    fs::remove_file(spec).unwrap();
}

#[test]
fn optional_assets_spec_import_participates_in_history_and_save() {
    let document = path("target.mwl");
    let source = path("source 日本語.mwl");
    let modes = path("modes.bin");
    let spec = path("import options.txt");
    let expected = optional_assets();
    let mut source_file = MwlFile::default();
    expected
        .install_into(&mut source_file, &[false; 256])
        .unwrap();
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(&source, source_file.encode().unwrap()).unwrap();
    fs::write(&modes, [0; 256]).unwrap();
    fs::write(
        &spec,
        format!(
            "LMMWLOPT1\nsource {}\nsize-modes {}\nmaximum-records 32\n",
            source.file_name().unwrap().to_string_lossy(),
            modes.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();
    let mut session = None;
    open_mwl_document(&mut session, &document).unwrap();

    import_optional_assets(&mut session, &spec).unwrap();

    let controller = session.as_mut().unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(
        MwlOptionalLevelAssets::decode(controller.value(), 32, &[false; 256]).unwrap(),
        expected
    );
    assert!(controller.undo(1).unwrap());
    assert!(controller.redo(2).unwrap());
    save_mwl_document(&mut session).unwrap();
    let saved = MwlFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(
        MwlOptionalLevelAssets::decode(&saved, 32, &[false; 256]).unwrap(),
        expected
    );
    for path in [document, source, modes, spec] {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn failed_optional_assets_import_does_not_dirty_open_document() {
    let document = path("target-failure.mwl");
    let source = path("source-failure.mwl");
    let modes = path("short-modes.bin");
    let spec = path("import-failure.txt");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(&source, MwlFile::default().encode().unwrap()).unwrap();
    fs::write(&modes, [0; 255]).unwrap();
    fs::write(
        &spec,
        format!(
            "LMMWLOPT1\nsource {}\nsize-modes {}\nmaximum-records 32\n",
            source.file_name().unwrap().to_string_lossy(),
            modes.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();
    let mut session = None;
    open_mwl_document(&mut session, &document).unwrap();

    assert!(import_optional_assets(&mut session, &spec).is_err());

    let controller = session.as_ref().unwrap();
    assert_eq!(controller.revision(), 0);
    assert!(!controller.is_modified());
    assert!(!controller.can_undo());
    for path in [document, source, modes, spec] {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn semantic_optional_edit_spec_is_atomic_undoable_and_saved() {
    let document = path("semantic-target.mwl");
    let modes = path("semantic-modes.bin");
    let edits = path("semantic-edits.txt");
    let spec = path("semantic-spec.txt");
    let expected = optional_assets();
    let mut target = file();
    expected.install_into(&mut target, &[false; 256]).unwrap();
    fs::write(&document, target.encode().unwrap()).unwrap();
    fs::write(&modes, [0; 256]).unwrap();
    fs::write(
        &edits,
        "LMMWLOE1\npalette-color 256 1234\nexanimation-globals 09 0000000A\ntrigger 3 07\n",
    )
    .unwrap();
    fs::write(
        &spec,
        format!(
            "LMMWLOES1\nedits {}\nsize-modes {}\nmaximum-records 32\n",
            edits.file_name().unwrap().to_string_lossy(),
            modes.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();
    let mut session = None;
    open_mwl_document(&mut session, &document).unwrap();

    edit_optional_assets(&mut session, &spec).unwrap();

    let controller = session.as_mut().unwrap();
    assert_eq!(controller.revision(), 1);
    let edited = MwlOptionalLevelAssets::decode(controller.value(), 32, &[false; 256]).unwrap();
    assert_eq!(edited.palette.colors[256], Bgr555(0x1234));
    assert_eq!(edited.exanimation.as_ref().unwrap().setting, 9);
    assert_eq!(edited.exanimation.as_ref().unwrap().trigger_values[3], 7);
    assert!(controller.undo(1).unwrap());
    assert!(controller.redo(2).unwrap());
    save_mwl_document(&mut session).unwrap();
    for path in [document, modes, edits, spec] {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn open_edit_save_close_round_trips_through_real_files() {
    let document = path("level.mwl");
    let script = path("edit.txt");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(
        &script,
        format!("{}\nflags 12345678\nlevel 1ab\n", mwl_edit_script::MAGIC),
    )
    .unwrap();
    let mut session = None;
    open_mwl_document(&mut session, &document).unwrap();
    edit_mwl_document(&mut session, &script).unwrap();
    assert!(session.as_ref().unwrap().is_modified());
    assert!(close_mwl_document(&mut session, false).is_err());
    save_mwl_document(&mut session).unwrap();
    close_mwl_document(&mut session, false).unwrap();
    let saved = MwlFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.flags, 0x1234_5678);
    assert_eq!(
        MwlLevelHeaderSection::decode(&saved.sections[MwlSectionKind::LevelHeader as usize].bytes)
            .unwrap()
            .level_number(),
        0x01ab
    );
    fs::remove_file(document).unwrap();
    fs::remove_file(script).unwrap();
}

#[test]
fn failed_open_and_dirty_discard_are_safe() {
    let document = path("bad.mwl");
    fs::write(&document, b"bad").unwrap();
    let mut session = None;
    assert!(open_mwl_document(&mut session, &document).is_err());
    assert!(session.is_none());
    fs::write(&document, file().encode().unwrap()).unwrap();
    open_mwl_document(&mut session, &document).unwrap();
    session
        .as_mut()
        .unwrap()
        .apply_edits(0, &[lm_app::MwlDocumentEdit::SetFlags(1)])
        .unwrap();
    close_mwl_document(&mut session, true).unwrap();
    assert!(session.is_none());
    assert_eq!(
        MwlFile::decode(&fs::read(&document).unwrap())
            .unwrap()
            .flags,
        0
    );
    fs::remove_file(document).unwrap();
}
