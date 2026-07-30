use super::*;
use lm_app::Command;
use lm_level::{Map16Address, Map16Quadrant, Subtile};
use lm_profile::load_smw_us_v1_complete_map16;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "lm-rom-map16-{name}-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn pristine_fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/before.smc");
    fs::read(path).unwrap()
}

#[test]
fn ordinary_rom_gui_route_edits_and_commits_native_map16() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    assert!(matches!(
        editor
            .workspace
            .as_ref()
            .map(|workspace| &workspace.controller),
        Some(Controller::Smw(_))
    ));
    let workspace = editor.workspace.as_ref().unwrap();
    assert_eq!(
        workspace.controller.set().pages.len(),
        lm_app::SMW_COMPLETE_MAP16_PAGES
    );
    let image = crate::vanilla_map16_preview::render_rom_map16_page(
        workspace.image.as_file_bytes().to_vec(),
        0x105,
        lm_level::LegacyLevelHeader::default(),
        &workspace.controller.set().pages[0],
    )
    .unwrap();
    assert_eq!(image.size, [256, 256]);
    let original_pixels = image.pixels;
    assert_eq!(editor.search_start, "80000");
    assert_eq!(editor.search_end, "100000");
    editor.apply(Map16ControllerEdit::SetSubtile {
        address: Map16Address { page: 0, tile: 0 },
        quadrant: Map16Quadrant::BottomRight,
        subtile: Subtile(0x2345),
        resolution_limit: 0x1_0000,
    });
    editor.apply(Map16ControllerEdit::SetSubtile {
        address: Map16Address {
            page: lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES,
            tile: 0,
        },
        quadrant: Map16Quadrant::TopLeft,
        subtile: Subtile(0x4567),
        resolution_limit: 0x1_0000,
    });
    assert!(editor.error.is_none());
    let workspace = editor.workspace.as_ref().unwrap();
    let edited_image = crate::vanilla_map16_preview::render_rom_map16_page(
        workspace.image.as_file_bytes().to_vec(),
        0x105,
        lm_level::LegacyLevelHeader::default(),
        &workspace.controller.set().pages[0],
    )
    .unwrap();
    assert_ne!(edited_image.pixels, original_pixels);
    let command = editor.prepare_commit().unwrap();
    app.dispatch(command).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10_0000);
    let reopened = load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap();
    assert_eq!(reopened.foreground.definitions[3], 0x2345);
    assert_eq!(reopened.background.definitions[0], 0x4567);
    assert_eq!(reopened.foreground.acts_like.len(), 0x8000);
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x80_000);
}

#[test]
fn complete_lunar_magic_file_import_commits_and_reopens_every_domain() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);

    let original = editor.workspace.as_ref().unwrap().controller.set().clone();
    let mut imported = original.clone();
    imported.pages[0].tiles[0].top_left = Subtile(0x7777);
    imported.pages[0].tiles[0].acts_like = 0x3456;
    imported.pages[2].tiles[0x34].bottom_right = Subtile(0x4567);
    imported.pages[2].tiles[0x34].acts_like = 0x0123;
    imported.pages[lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES + 1].tiles[0x23].top_right =
        Subtile(0x6789);

    let file = complete_file::export_file(&imported, None).unwrap();
    let replacements = complete_file::import_replacements(&file, &original).unwrap();
    editor
        .workspace
        .as_mut()
        .unwrap()
        .controller
        .apply_edits(&[Map16ControllerEdit::ReplaceTiles {
            replacements,
            resolution_limit: lm_app::SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
        }])
        .unwrap();

    let staged = editor.workspace.as_ref().unwrap().controller.set();
    assert_eq!(
        staged.pages[0].tiles[0].top_left,
        original.pages[0].tiles[0].top_left
    );
    assert_eq!(staged.pages[0].tiles[0].acts_like, 0x3456);
    assert_eq!(staged.pages[2].tiles[0x34].bottom_right, Subtile(0x4567));
    assert_eq!(staged.pages[2].tiles[0x34].acts_like, 0x0123);
    assert_eq!(
        staged.pages[lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES + 1].tiles[0x23].top_right,
        Subtile(0x6789)
    );
    assert_eq!(
        staged.pages[lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES + 1].tiles[0x23].acts_like,
        0
    );

    let command = editor.prepare_commit().unwrap();
    app.dispatch(command).unwrap();
    let reopened = load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap();
    assert_eq!(reopened.foreground.acts_like[0], 0x3456);
    let foreground_word = (2 * Map16Page::TILE_COUNT + 0x34) * 4 + 3;
    assert_eq!(reopened.foreground.definitions[foreground_word], 0x4567);
    assert_eq!(
        reopened.foreground.acts_like[2 * Map16Page::TILE_COUNT + 0x34],
        0x0123
    );
    let background_word = (Map16Page::TILE_COUNT + 0x23) * 4 + 1;
    assert_eq!(reopened.background.definitions[background_word], 0x6789);
}

#[test]
fn complete_file_workers_prevent_close_until_io_finishes() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();

    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("oracle-work/lm363/pristine-us/map16/all.map16");
    let mut loading = RomMap16Editor::default();
    loading.open(&app);
    loading
        .complete_loader
        .start(vec![crate::document_loader::BoundedRead::new(
            source,
            lm_level::Lm16Map16File::MAX_FILE_LEN as u64,
            "complete Map16 fixture",
        )])
        .unwrap();
    assert!(!loading.request_close(false));
    assert!(loading.error.as_deref().unwrap().contains("loading"));

    let output = temporary_path("save");
    let mut saving = RomMap16Editor::default();
    saving.open(&app);
    saving
        .complete_persistence
        .start(
            1,
            crate::persistence_worker::PersistenceTarget::Create(output.clone()),
            vec![1, 2, 3],
        )
        .unwrap();
    assert!(!saving.request_close(false));
    assert!(saving.error.as_deref().unwrap().contains("saving"));
    saving.complete_persistence.wait_for_test().result.unwrap();
    fs::remove_file(output).unwrap();
}
