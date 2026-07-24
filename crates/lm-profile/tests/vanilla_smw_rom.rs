use lm_profile::{
    SMW_US_V1_VANILLA_GRAPHICS_FILES, smw_us_v1_vanilla_graphics_layout,
};
use lm_project::Project;
use lm_rom::RomImage;
use std::{fs, path::PathBuf};

#[test]
fn every_ordinary_graphics_file_in_the_local_reference_rom_decodes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Super Mario World (USA).sfc");
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    for file_number in 0..SMW_US_V1_VANILLA_GRAPHICS_FILES {
        let graphics = project
            .load_graphics_file(file_number, smw_us_v1_vanilla_graphics_layout())
            .unwrap_or_else(|error| panic!("failed to decode GFX{file_number:02X}: {error}"));
        assert!(
            !graphics.tiles.is_empty(),
            "GFX{file_number:02X} decoded no tiles"
        );
    }
}
