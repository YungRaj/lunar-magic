use super::*;
use lm_level::{LevelObjectData, NativeSpriteStream};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn path(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lm-native-level-shell-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn file() -> NativeLevelFile {
    NativeLevelFile {
        source_level: 0x105,
        layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
        sprites: NativeSpriteStream::parse(
            &[0x10, 0x00, 0x20, 0x01, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap(),
    }
}

#[test]
fn open_edit_save_close_uses_bound_interpretation() {
    let directory = path("directory");
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level 日本語.lmlvl");
    let spec = directory.join("open spec.txt");
    let edits = directory.join("edit script.txt");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(
        &spec,
        "LMNLDOC1\nlevel level 日本語.lmlvl\nsprite-lengths standard\n",
    )
    .unwrap();
    fs::write(
        &edits,
        "LMLEDIT1\nheader mode 1f\nsprite-header 44\nobject insert 1 030405\n",
    )
    .unwrap();
    let mut session = None;
    open(&mut session, &spec).unwrap();
    edit(&mut session, &edits).unwrap();
    assert!(close(&mut session, false).is_err());
    save(&mut session).unwrap();
    close(&mut session, false).unwrap();
    let saved = NativeLevelFile::decode(
        &fs::read(&document).unwrap(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(saved.layer1.header.level_mode(), 0x1f);
    assert_eq!(saved.layer1.objects.records.len(), 2);
    assert_eq!(saved.sprites.header, 0x44);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn malformed_table_open_and_discard_leave_file_unchanged() {
    let directory = path("invalid");
    fs::create_dir(&directory).unwrap();
    let document = directory.join("level.lmlvl");
    let spec = directory.join("open.txt");
    let table = directory.join("lengths.bin");
    let original = file().encode().unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(&table, [3; 10]).unwrap();
    fs::write(
        &spec,
        "LMNLDOC1\nlevel level.lmlvl\nsprite-lengths lengths.bin\n",
    )
    .unwrap();
    let mut session = None;
    assert!(open(&mut session, &spec).is_err());
    assert!(session.is_none());
    fs::write(&table, [3; SpriteLengthTable::ENCODED_LEN]).unwrap();
    open(&mut session, &spec).unwrap();
    session
        .as_mut()
        .unwrap()
        .apply_edits(0, &[lm_app::NativeLevelEdit::SetSpriteHeader(7)])
        .unwrap();
    close(&mut session, true).unwrap();
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
