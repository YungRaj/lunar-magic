use super::*;
use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "lm-world-app-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn file() -> SpriteAppearanceFile {
    SpriteAppearanceFile {
        definitions: vec![SpriteAppearanceDefinition {
            sprite_id: 1,
            parts: vec![SpriteAppearancePart {
                tile_index: 2,
                palette_index: 3,
                x_offset: 4,
                y_offset: 5,
                x_flip: false,
                y_flip: false,
            }],
        }],
    }
}

#[test]
fn real_file_lifecycle_preserves_keyed_definitions_and_part_order() {
    let document = path("Sprites 日本語.lmowapp");
    let script = path("edits.txt");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(&script, "LMOWAED1\ndefinition insert 1 10\npart insert 10 0 123 4 -8 16 1 0\npart insert 10 1 124 5 9 -10 0 1\npart move 10 0 end\ndefinition move 10 1\n").unwrap();
    let mut session = None;
    open(&mut session, &document).unwrap();
    edit(&mut session, &script).unwrap();
    assert!(close(&mut session, false).is_err());
    save(&mut session).unwrap();
    close(&mut session, false).unwrap();
    let saved = SpriteAppearanceFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.definitions[0].sprite_id, 0x10);
    assert_eq!(saved.definitions[0].parts.len(), 2);
    assert_eq!(saved.definitions[0].parts[0].x_offset, 9);
    assert_eq!(saved.definitions[0].parts[1].x_offset, -8);
    fs::remove_file(document).unwrap();
    fs::remove_file(script).unwrap();
}

#[test]
fn failed_open_and_dirty_discard_preserve_file() {
    let document = path("sprites.lmowapp");
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
            &[lm_app::OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id: 1 }],
        )
        .unwrap();
    close(&mut session, true).unwrap();
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_file(document).unwrap();
}
