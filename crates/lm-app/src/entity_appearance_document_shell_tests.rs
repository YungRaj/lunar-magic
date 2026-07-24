use super::*;
use lm_level::{AppearanceSource, EntityAppearanceRecord};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "lm-entity-app-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn file() -> EntityAppearanceFile {
    EntityAppearanceFile {
        appearances: vec![EntityAppearanceRecord {
            source: AppearanceSource::Sprite(1),
            tile_index: 2,
            palette_index: 3,
            x: 4,
            y: 5,
            x_flip: false,
            y_flip: false,
        }],
    }
}

#[test]
fn real_file_open_edit_save_close_round_trip() {
    let document = path("entities 日本語.lmentapp");
    let script = path("edits.txt");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(
        &script,
        "LMENTED1\nreplace 0 layer1 10 20 4 -8 9 1 0\ninsert 1 sprite 11 21 5 10 11 0 1\n",
    )
    .unwrap();
    let mut session = None;
    open(&mut session, &document).unwrap();
    edit(&mut session, &script).unwrap();
    assert!(close(&mut session, false).is_err());
    save(&mut session).unwrap();
    close(&mut session, false).unwrap();
    let saved = EntityAppearanceFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.appearances.len(), 2);
    assert_eq!(saved.appearances[0].x, -8);
    assert_eq!(saved.appearances[1].source, AppearanceSource::Sprite(0x11));
    fs::remove_file(document).unwrap();
    fs::remove_file(script).unwrap();
}

#[test]
fn failed_open_and_dirty_discard_are_nonmutating() {
    let document = path("entities.lmentapp");
    fs::write(&document, b"bad").unwrap();
    let mut session = None;
    assert!(open(&mut session, &document).is_err());
    let original = file().encode().unwrap();
    fs::write(&document, &original).unwrap();
    open(&mut session, &document).unwrap();
    session
        .as_mut()
        .unwrap()
        .apply_edits(
            0,
            &[lm_app::EntityAppearanceDocumentEdit::Remove { index: 0 }],
        )
        .unwrap();
    close(&mut session, true).unwrap();
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_file(document).unwrap();
}
