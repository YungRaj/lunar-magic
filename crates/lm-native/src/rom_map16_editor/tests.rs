use super::*;

#[test]
fn snes_tileset_options_default_to_optimized_and_survive_editor_reopen() {
    let mut app = AppState::default();
    app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
        .unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();

    editor.open(&app);
    assert!(editor.snes_tileset_deduplicate);
    assert!(!editor.snes_tileset_include_palette);
    assert_eq!(
        editor.snes_tileset_color_maps[9],
        std::array::from_fn(|index| index as u8)
    );

    editor.snes_tileset_include_palette = true;
    editor.snes_tileset_palette_row = 7;
    editor.snes_tileset_deduplicate = false;
    editor.snes_tileset_graphics_offset = 0x123;
    editor.snes_tileset_map_offset = 0x234;
    editor.snes_tileset_color_filter = true;
    editor.snes_tileset_color_filter_index = 9;
    editor.snes_tileset_color_maps[9][3] = 0x0e;
    assert!(editor.request_close(false));

    editor.open(&app);
    assert!(editor.snes_tileset_include_palette);
    assert_eq!(editor.snes_tileset_palette_row, 7);
    assert!(!editor.snes_tileset_deduplicate);
    assert_eq!(editor.snes_tileset_graphics_offset, 0x123);
    assert_eq!(editor.snes_tileset_map_offset, 0x234);
    assert!(editor.snes_tileset_color_filter);
    assert_eq!(editor.snes_tileset_color_filter_index, 9);
    assert_eq!(editor.snes_tileset_color_maps[9][3], 0x0e);
}
use lm_app::Command;
use lm_level::{Map16Address, Map16Quadrant, Subtile};
use lm_profile::load_smw_us_v1_complete_map16;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[test]
fn original_map16_commit_shortcut_requires_unmodified_f9() {
    fn observed(modifiers: egui::Modifiers) -> bool {
        let context = egui::Context::default();
        let mut taken = false;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F9,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    taken = take_map16_commit_shortcut(ui);
                });
            },
        );
        taken
    }

    assert!(observed(egui::Modifiers::NONE));
    for modifiers in [
        egui::Modifiers::CTRL,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT,
        egui::Modifiers::COMMAND,
    ] {
        assert!(!observed(modifiers));
    }
}

#[test]
fn original_map16_page_shortcuts_accept_up_and_down_with_all_modifiers() {
    fn observed(key: egui::Key, modifiers: egui::Modifiers) -> Option<Map16PageShortcut> {
        let context = egui::Context::default();
        let mut shortcut = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shortcut = take_map16_page_shortcut(ui);
                });
            },
        );
        shortcut
    }

    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT,
        egui::Modifiers::COMMAND,
    ] {
        assert_eq!(
            observed(egui::Key::ArrowUp, modifiers),
            Some(Map16PageShortcut::Previous)
        );
        assert_eq!(
            observed(egui::Key::ArrowDown, modifiers),
            Some(Map16PageShortcut::Next)
        );
    }

    assert_eq!(
        map16_page_after_shortcut(0, 256, Map16PageShortcut::Previous),
        0
    );
    assert_eq!(
        map16_page_after_shortcut(0x23, 256, Map16PageShortcut::Previous),
        0x22
    );
    assert_eq!(
        map16_page_after_shortcut(0x23, 256, Map16PageShortcut::Next),
        0x24
    );
    assert_eq!(
        map16_page_after_shortcut(255, 256, Map16PageShortcut::Next),
        255
    );
}

#[test]
fn original_map16_f8_grid_shortcut_separates_visibility_and_color_chords() {
    fn observed(modifiers: egui::Modifiers) -> Option<Map16GridShortcut> {
        let context = egui::Context::default();
        let mut shortcut = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F8,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shortcut = take_map16_grid_shortcut(ui);
                });
            },
        );
        shortcut
    }

    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
        egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        egui::Modifiers::ALT | egui::Modifiers::SHIFT,
    ] {
        assert_eq!(observed(modifiers), Some(Map16GridShortcut::Toggle));
    }
    for modifiers in [
        egui::Modifiers::CTRL | egui::Modifiers::ALT,
        egui::Modifiers::CTRL | egui::Modifiers::ALT | egui::Modifiers::SHIFT,
    ] {
        assert_eq!(observed(modifiers), Some(Map16GridShortcut::ToggleColor));
    }

    let (mut visible, mut dark) = (false, false);
    apply_map16_grid_shortcut(&mut visible, &mut dark, Map16GridShortcut::ToggleColor);
    assert_eq!((visible, dark), (false, true));
    apply_map16_grid_shortcut(&mut visible, &mut dark, Map16GridShortcut::Toggle);
    assert_eq!((visible, dark), (true, true));
}

#[test]
fn original_map16_paste_shortcut_requires_ctrl_and_accepts_other_modifiers() {
    fn observed(modifiers: egui::Modifiers) -> bool {
        let context = egui::Context::default();
        let mut taken = false;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::V,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    taken = take_map16_paste_shortcut(ui);
                });
            },
        );
        taken
    }

    for modifiers in [
        egui::Modifiers::CTRL,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL | egui::Modifiers::ALT,
        egui::Modifiers::CTRL | egui::Modifiers::ALT | egui::Modifiers::SHIFT,
    ] {
        assert!(observed(modifiers));
    }
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT,
        egui::Modifiers::COMMAND,
    ] {
        assert!(!observed(modifiers));
    }
}

#[test]
fn ctrl_v_captures_the_current_map16_target_before_requesting_clipboard_data() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    editor.page = 0x23;
    editor.tile = 0x45;
    let expected_revision = editor.workspace.as_ref().unwrap().controller.revision();

    let context = egui::Context::default();
    let _ = context.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::V,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            modifiers: egui::Modifiers::CTRL,
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                editor.selection_and_clipboard(ui, false, lm_app::SMW_COMPLETE_MAP16_PAGES, None);
            });
        },
    );

    assert_eq!(
        editor.clipboard_paste_target,
        Some((
            expected_revision,
            editor.staged_revision,
            Map16Address {
                page: 0x23,
                tile: 0x45
            }
        ))
    );
}

#[test]
fn original_map16_zoom_shortcuts_require_ctrl_and_preserve_bounds() {
    fn observed(key: egui::Key, modifiers: egui::Modifiers) -> Option<Map16ZoomShortcut> {
        let context = egui::Context::default();
        let mut shortcut = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shortcut = take_map16_zoom_shortcut(ui);
                });
            },
        );
        shortcut
    }

    for modifiers in [
        egui::Modifiers::CTRL,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL | egui::Modifiers::ALT,
    ] {
        assert_eq!(
            observed(egui::Key::Num0, modifiers),
            Some(Map16ZoomShortcut::Reset)
        );
        assert_eq!(
            observed(egui::Key::Plus, modifiers),
            Some(Map16ZoomShortcut::Increase)
        );
        assert_eq!(
            observed(egui::Key::Minus, modifiers),
            Some(Map16ZoomShortcut::Decrease)
        );
    }
    for key in [egui::Key::Num0, egui::Key::Plus, egui::Key::Minus] {
        assert_eq!(observed(key, egui::Modifiers::NONE), None);
    }

    assert_eq!(
        map16_zoom_after_shortcut(3200, Map16ZoomShortcut::Reset),
        100
    );
    assert_eq!(
        map16_zoom_after_shortcut(100, Map16ZoomShortcut::Decrease),
        100
    );
    assert_eq!(
        map16_zoom_after_shortcut(4999, Map16ZoomShortcut::Increase),
        5000
    );
    assert_eq!(
        map16_zoom_after_shortcut(5000, Map16ZoomShortcut::Increase),
        5000
    );

    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    assert_eq!(editor.page_zoom_percent, 100);
}

#[test]
fn original_map16_f1_shortcuts_split_page_numbers_and_protected_unlock() {
    fn observed(modifiers: egui::Modifiers) -> Option<Map16F1Shortcut> {
        let context = egui::Context::default();
        let mut shortcut = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F1,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shortcut = take_map16_f1_shortcut(ui);
                });
            },
        );
        shortcut
    }

    assert_eq!(
        observed(egui::Modifiers::NONE),
        Some(Map16F1Shortcut::TogglePageNumbers)
    );
    assert_eq!(
        observed(egui::Modifiers::ALT),
        Some(Map16F1Shortcut::TogglePageNumbers)
    );
    assert_eq!(
        observed(egui::Modifiers::CTRL),
        Some(Map16F1Shortcut::ToggleProtectedPages)
    );
    assert_eq!(
        observed(egui::Modifiers::CTRL | egui::Modifiers::ALT),
        Some(Map16F1Shortcut::ToggleProtectedPages)
    );
    for modifiers in [
        egui::Modifiers::SHIFT,
        egui::Modifiers::SHIFT | egui::Modifiers::ALT,
        egui::Modifiers::SHIFT | egui::Modifiers::CTRL,
    ] {
        assert_eq!(observed(modifiers), None);
    }

    assert!(!map16_page_is_editable(0, false));
    assert!(!map16_page_is_editable(1, false));
    assert!(map16_page_is_editable(2, false));
    assert!(map16_page_is_editable(0, true));
}

#[test]
fn built_in_page_clipboard_paste_requires_explicit_unlock() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    let revision = editor.workspace.as_ref().unwrap().controller.revision();
    let address = Map16Address { page: 0, tile: 3 };
    let replacement = lm_level::Map16Tile {
        top_left: Subtile(0x1234),
        top_right: Subtile(0x2345),
        bottom_left: Subtile(0x3456),
        bottom_right: Subtile(0x4567),
        acts_like: 0x0123,
    };
    let text = native_clipboard::encode_map16_tile(replacement).unwrap();
    let before = editor.workspace.as_ref().unwrap().controller.set().pages[0].tiles[3];

    editor.paste_tile_at(
        &text,
        revision,
        editor.staged_revision,
        address,
        lm_app::SMW_COMPLETE_MAP16_PAGES,
    );
    assert!(editor.error.as_deref().unwrap().contains("protected"));
    assert_eq!(
        editor.workspace.as_ref().unwrap().controller.set().pages[0].tiles[3],
        before
    );

    editor.error = None;
    editor.protected_pages_unlocked = true;
    editor.paste_tile_at(
        &text,
        revision,
        editor.staged_revision,
        address,
        lm_app::SMW_COMPLETE_MAP16_PAGES,
    );
    assert!(editor.error.is_none());
    assert_eq!(
        editor.workspace.as_ref().unwrap().controller.set().pages[0].tiles[3],
        replacement
    );
}

#[test]
fn unmodified_f9_routes_through_the_existing_map16_commit_transaction() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let project_revision = app.project_revision();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    editor.apply(Map16ControllerEdit::SetSubtile {
        address: Map16Address { page: 2, tile: 3 },
        quadrant: Map16Quadrant::TopLeft,
        subtile: Subtile(0x2345),
        resolution_limit: lm_app::SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
    });

    let context = egui::Context::default();
    let mut command = None;
    let _ = context.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::F9,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let shortcut = take_map16_commit_shortcut(ui);
                command = editor.commit_controls(ui, false, project_revision, shortcut);
            });
        },
    );

    app.dispatch(command.expect("F9 should prepare the Map16 commit"))
        .unwrap();
    let reopened = load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap();
    let word = (2 * Map16Page::TILE_COUNT + 3) * 4;
    assert_eq!(reopened.foreground.definitions[word], 0x2345);
}

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

#[test]
fn rom_clipboard_delivery_uses_the_requested_map16_address() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    let revision = editor.workspace.as_ref().unwrap().controller.revision();
    let target = Map16Address { page: 2, tile: 3 };
    let untouched = Map16Address { page: 4, tile: 9 };
    let before = editor.workspace.as_ref().unwrap().controller.set().pages[4].tiles[9];
    let replacement = lm_level::Map16Tile {
        top_left: Subtile(5),
        top_right: Subtile(6),
        bottom_left: Subtile(7),
        bottom_right: Subtile(8),
        acts_like: 0x1234,
    };
    let text = native_clipboard::encode_map16_tile(replacement).unwrap();

    editor.page = untouched.page;
    editor.tile = untouched.tile;
    editor.paste_tile_at(
        &text,
        revision,
        editor.staged_revision,
        target,
        lm_app::SMW_COMPLETE_MAP16_PAGES,
    );

    let set = editor.workspace.as_ref().unwrap().controller.set();
    assert_eq!(set.pages[target.page].tiles[target.tile], replacement);
    assert_eq!(set.pages[untouched.page].tiles[untouched.tile], before);
}

#[test]
fn rom_rectangle_clipboard_targets_captured_origin_and_rejects_stale_delivery() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    let revision = editor.workspace.as_ref().unwrap().controller.revision();
    let staged_revision = editor.staged_revision;
    let destination = 0x020e;
    let tiles: Vec<_> = (0_u16..4)
        .map(|index| lm_level::Map16Tile {
            top_left: Subtile(0x100 + index),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0x0200 + index,
        })
        .collect();
    let rectangle =
        lm_app::NativeMap16Clipboard::from_rectangle(0x0300, 2, 2, tiles.clone()).unwrap();
    let text = native_clipboard::encode_native_map16_rectangle(&rectangle).unwrap();

    editor.page = 9;
    editor.tile = 9;
    editor
        .paste_rectangle_at(&text, revision, staged_revision, destination)
        .unwrap();
    let set = editor.workspace.as_ref().unwrap().controller.set();
    for (global, expected) in [0x020e, 0x020f, 0x021e, 0x021f].into_iter().zip(tiles) {
        assert_eq!(
            set.pages[global / Map16Page::TILE_COUNT].tiles[global % Map16Page::TILE_COUNT],
            expected
        );
    }
    assert!(
        editor
            .paste_rectangle_at(&text, revision, staged_revision, 0x0400)
            .unwrap_err()
            .contains("changed")
    );
}

#[test]
fn staged_map16_history_restores_exact_sets_and_invalidates_divergent_redo() {
    let mut app = AppState::default();
    app.load_rom(pristine_fixture()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut editor = RomMap16Editor::default();
    editor.open(&app);
    let address = Map16Address { page: 2, tile: 3 };
    let original = editor.workspace.as_ref().unwrap().controller.set().pages[2].tiles[3];

    editor.apply(Map16ControllerEdit::SetSubtile {
        address,
        quadrant: Map16Quadrant::TopLeft,
        subtile: Subtile(0x1234),
        resolution_limit: lm_app::SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
    });
    assert_eq!(editor.staged_revision, 1);
    assert_eq!(editor.undo_history.len(), 1);
    assert!(editor.redo_history.is_empty());

    editor.navigate_history(true).unwrap();
    assert_eq!(
        editor.workspace.as_ref().unwrap().controller.set().pages[2].tiles[3],
        original
    );
    assert_eq!(editor.staged_revision, 2);
    assert_eq!(editor.redo_history.len(), 1);

    editor.navigate_history(false).unwrap();
    assert_eq!(
        editor.workspace.as_ref().unwrap().controller.set().pages[2].tiles[3].top_left,
        Subtile(0x1234)
    );
    editor.navigate_history(true).unwrap();
    editor.apply(Map16ControllerEdit::SetSubtile {
        address,
        quadrant: Map16Quadrant::TopRight,
        subtile: Subtile(0x5678),
        resolution_limit: lm_app::SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
    });
    assert!(editor.redo_history.is_empty());
}

#[test]
fn staged_map16_history_is_bounded_to_one_hundred_snapshots() {
    let page = Map16Page::new(vec![lm_level::Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap();
    let mut history = Vec::new();
    for index in 0..=MAP16_HISTORY_LIMIT {
        let mut set = lm_level::Map16Set {
            pages: vec![page.clone()],
        };
        set.pages[0].tiles[0].acts_like = u16::try_from(index).unwrap();
        push_history(&mut history, set);
    }
    assert_eq!(history.len(), MAP16_HISTORY_LIMIT);
    assert_eq!(history[0].pages[0].tiles[0].acts_like, 1);
    assert_eq!(
        history[MAP16_HISTORY_LIMIT - 1].pages[0].tiles[0].acts_like,
        100
    );
}
