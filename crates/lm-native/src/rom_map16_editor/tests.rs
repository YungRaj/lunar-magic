use super::*;
use lm_app::Command;
use lm_level::{Map16Address, Map16Quadrant, Subtile};
use lm_profile::load_smw_us_v1_transferred_map16;
use lm_project::Project;
use lm_rom::{Mapper, RomImage};
use std::{fs, path::Path};

fn expanded_fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/before.smc");
    let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
    let mut project = Project::new(image);
    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .unwrap();
    project.rom.as_file_bytes().to_vec()
}

#[test]
fn ordinary_rom_gui_route_edits_and_commits_native_map16() {
    let mut app = AppState::default();
    app.load_rom(expanded_fixture()).unwrap();
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
    let image = crate::vanilla_map16_preview::render_rom_map16_page(
        workspace.image.as_file_bytes().to_vec(),
        0x105,
        lm_level::LegacyLevelHeader::default(),
        &workspace.controller.set().pages[0],
    )
    .unwrap();
    assert_eq!(image.size, [256, 256]);
    let original_pixels = image.pixels;
    editor.search_start = "80000".into();
    editor.search_end = "100000".into();
    editor.apply(Map16ControllerEdit::SetSubtile {
        address: Map16Address { page: 0, tile: 0 },
        quadrant: Map16Quadrant::BottomRight,
        subtile: Subtile(0x2345),
        resolution_limit: 2048,
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
    let reopened = load_smw_us_v1_transferred_map16(app.project().unwrap()).unwrap();
    assert_eq!(reopened.definitions[3], 0x2345);
    assert_eq!(reopened.acts_like.len(), 2884);
}
