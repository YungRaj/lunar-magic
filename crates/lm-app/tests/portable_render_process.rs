use lm_graphics::{
    Bgr555, CompactExAnimation, CompactExAnimationFile, GraphicsFile4bpp, GraphicsInterchangeFile,
    IndexedTile, Palette, PaletteInterchangeFile,
};
use lm_level::{Map16Page, Map16PageFile, Map16Set, Map16SetFile, Map16Tile};
use lm_overworld::{EventRevealTable, OverworldLayer};
use lm_project::{
    CompleteOverworldData, CompleteOverworldFile, CompleteOverworldShape, OverworldLayers,
};
use lm_render::{EditorOverlay, EditorOverlayFile, GridOverlay, Rgba};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lm-app-portable-render-process-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

fn assert_png_dimensions(bytes: &[u8], width: u32, height: u32) {
    assert_eq!(bytes.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_eq!(bytes.get(16..20), Some(width.to_be_bytes().as_slice()));
    assert_eq!(bytes.get(20..24), Some(height.to_be_bytes().as_slice()));
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end fixture proves both commands share one process.
fn scripted_binary_renders_portable_views_and_refuses_existing_outputs() {
    let directory = directory();
    let graphics = directory.join("Graphics 日本語.lmgfx");
    let palette = directory.join("Palette.lmpal");
    let page = directory.join("Page 00.map16");
    let map16_set = directory.join("All Map16.lm16set");
    let overworld = directory.join("World 日本語.lmow");
    let size_modes = directory.join("Size modes.bin");
    let overlays = directory.join("Editor Overlays.lmovly");
    let map16_output = directory.join("Rendered Page.png");
    let overworld_output = directory.join("Rendered World.png");
    let map16_spec = directory.join("Map16 render spec.txt");
    let document_render_spec = directory.join("Open Map16 render spec.txt");
    let overworld_spec = directory.join("Overworld render spec.txt");
    let world_open_spec = directory.join("World open spec.txt");
    let world_document_render_spec = directory.join("Open World render spec.txt");
    let world_edit = directory.join("World edits.txt");
    let graphics_edit = directory.join("Graphics edits.txt");
    let graphics_document_spec = directory.join("Graphics render spec.txt");
    let palette_edit = directory.join("Palette edits.txt");
    let palette_document_spec = directory.join("Palette render spec.txt");
    let exanimation = directory.join("Animation 日本語.lmexan");
    let exanimation_open_spec = directory.join("Animation open spec.txt");
    let exanimation_edit = directory.join("Animation edits.txt");
    let map16_edit = directory.join("Map16 edits.txt");
    let document_output = directory.join("Open Map16 Page.png");
    let world_document_output = directory.join("Open World.png");
    let graphics_document_output = directory.join("Open Graphics.png");
    let palette_document_output = directory.join("Open Palette.png");
    let script = directory.join("commands.txt");
    let dirty_script = directory.join("dirty commands.txt");
    let dirty_world_edit = directory.join("Unsaved World edits.txt");

    fs::write(
        &graphics,
        GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
                    IndexedTile::new([1; IndexedTile::PIXEL_COUNT]),
                ],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &palette,
        PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0); 128],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let map16_page = Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap();
    fs::write(
        &page,
        Map16PageFile {
            source_page: 0,
            page: map16_page.clone(),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &map16_set,
        Map16SetFile {
            set: Map16Set {
                pages: vec![map16_page],
            },
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let modes = vec![false; 256];
    fs::write(&size_modes, vec![0; 256]).unwrap();
    fs::write(
        &overlays,
        EditorOverlayFile {
            overlays: vec![EditorOverlay::Grid(GridOverlay {
                origin_x: 0,
                origin_y: 0,
                cell_width: 8,
                cell_height: 8,
                color: Rgba {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 128,
                },
            })],
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &exanimation,
        CompactExAnimationFile {
            source_slot: 0,
            animation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![],
            },
        }
        .encode(&modes)
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &overworld,
        CompleteOverworldFile {
            source_slot: 0,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 0,
                endpoints: 0,
                messages: 0,
                sprites: 0,
                sprite_record_len: 7,
                palette_colors: 16,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                },
                event_reveals: EventRevealTable { entries: vec![] },
                endpoints: vec![],
                messages: vec![],
                sprites: vec![],
                palette: Palette {
                    colors: vec![Bgr555(0); 16],
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: vec![],
                },
            },
        }
        .encode(&modes)
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &map16_spec,
        "LMM16R1\ngraphics Graphics 日本語.lmgfx\npalette Palette.lmpal\npage Page 00.map16\noutput Rendered Page.png\noverlays Editor Overlays.lmovly\nviewport-origin-x -2\nviewport-origin-y 1\nviewport-width 10\nviewport-height 11\nzoom-numerator 2\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &document_render_spec,
        "LMM16DR1\ngraphics Graphics 日本語.lmgfx\npalette Palette.lmpal\npage 0\noutput Open Map16 Page.png\noverlays Editor Overlays.lmovly\nviewport-origin-x 1\nviewport-origin-y -2\nviewport-width 12\nviewport-height 13\nzoom-numerator 1\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &map16_edit,
        "LMM16ED1\nsubtile 0 0 tl 0001 100\nappend-blank-page 200\n",
    )
    .unwrap();
    fs::write(
        &overworld_spec,
        "LMOWRND1\noverworld World 日本語.lmow\nsize-modes Size modes.bin\nmaximum-animation-records 32\nmap16 All Map16.lm16set\ngraphics Graphics 日本語.lmgfx\ncompleted-reveals 0\noutput Rendered World.png\noverlays Editor Overlays.lmovly\nviewport-origin-x -1\nviewport-origin-y 0\nviewport-width 6\nviewport-height 4\nzoom-numerator 2\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &world_open_spec,
        "LMOWDOC1\noverworld World 日本語.lmow\nsize-modes Size modes.bin\nmaximum-animation-records 32\n",
    )
    .unwrap();
    fs::write(
        &world_document_render_spec,
        "LMOWDRN1\nmap16 All Map16.lm16set\ngraphics Graphics 日本語.lmgfx\ncompleted-reveals 0\noutput Open World.png\noverlays Editor Overlays.lmovly\nviewport-origin-x 0\nviewport-origin-y -1\nviewport-width 8\nviewport-height 6\nzoom-numerator 1\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &world_edit,
        "LMOWEDT1\nslot 0\npalette-owners 10 editable\nlayer 1 0 0 0001\n",
    )
    .unwrap();
    fs::write(
        &graphics_edit,
        format!("LMGFXED1\nowners 2 editable\nset 0 {}\n", "1".repeat(64)),
    )
    .unwrap();
    fs::write(
        &graphics_document_spec,
        "LMGFXDR1\npalette Palette.lmpal\npalette-row 0\ncolumns 2\noutput Open Graphics.png\noverlays Editor Overlays.lmovly\nviewport-origin-x -1\nviewport-origin-y 0\nviewport-width 14\nviewport-height 15\nzoom-numerator 2\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(&palette_edit, "LMPALED1\nowners 80 editable\nset 1 001f\n").unwrap();
    fs::write(
        &palette_document_spec,
        "LMPALDR1\ncolumns 16\ncell-size 4\noutput Open Palette.png\noverlays Editor Overlays.lmovly\nviewport-origin-x 0\nviewport-origin-y -1\nviewport-width 16\nviewport-height 17\nzoom-numerator 1\nzoom-denominator 1\n",
    )
    .unwrap();
    fs::write(
        &exanimation_open_spec,
        "LMEXDOC1\nanimation Animation 日本語.lmexan\nsize-modes Size modes.bin\nmaximum-records 32\n",
    )
    .unwrap();
    fs::write(&exanimation_edit, "LMEXAED1\nsetting 05\n").unwrap();
    fs::write(
        &script,
        format!(
            "ex-open-file {}\nex-edit-file {}\nex-undo\nex-redo\nex-save\nex-close\npal-open {}\npal-edit-file {}\npal-undo\npal-redo\npal-render-file {}\npal-save\npal-close\ngfx-open {}\ngfx-edit-file {}\ngfx-undo\ngfx-redo\ngfx-render-file {}\ngfx-save\ngfx-close\nmap16-set-open {}\nmap16-set-edit-file {}\nmap16-set-undo\nmap16-set-redo\nmap16-set-render-file {}\nmap16-set-save\nmap16-set-close\nworld-open-file {}\nworld-edit-file {}\nworld-undo\nworld-redo\nworld-render-file {}\nworld-save\nworld-close\nmap16-render-file {}\noverworld-render-file {}\nquit\n",
            exanimation_open_spec.display(),
            exanimation_edit.display(),
            palette.display(),
            palette_edit.display(),
            palette_document_spec.display(),
            graphics.display(),
            graphics_edit.display(),
            graphics_document_spec.display(),
            map16_set.display(),
            map16_edit.display(),
            document_render_spec.display(),
            world_open_spec.display(),
            world_edit.display(),
            world_document_render_spec.display(),
            map16_spec.display(),
            overworld_spec.display()
        ),
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains("complete overworld undo: applied"));
    assert!(stdout.contains("complete overworld redo: applied"));
    assert!(stdout.contains("complete Map16 undo: applied"));
    assert!(stdout.contains("complete Map16 redo: applied"));
    assert!(stdout.contains("graphics undo: applied"));
    assert!(stdout.contains("graphics redo: applied"));
    assert!(stdout.contains("palette undo: applied"));
    assert!(stdout.contains("palette redo: applied"));
    assert!(stdout.contains("ExAnimation undo: applied"));
    assert!(stdout.contains("ExAnimation redo: applied"));
    let map16_png = fs::read(&map16_output).unwrap();
    let document_png = fs::read(&document_output).unwrap();
    let overworld_png = fs::read(&overworld_output).unwrap();
    let world_document_png = fs::read(&world_document_output).unwrap();
    let graphics_document_png = fs::read(&graphics_document_output).unwrap();
    let palette_document_png = fs::read(&palette_document_output).unwrap();
    assert_eq!(map16_png.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_eq!(document_png.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_png_dimensions(&map16_png, 10, 11);
    assert_png_dimensions(&document_png, 12, 13);
    assert_eq!(
        palette_document_png.get(..8),
        Some(b"\x89PNG\r\n\x1a\n".as_slice())
    );
    assert_png_dimensions(&palette_document_png, 16, 17);
    assert_eq!(
        graphics_document_png.get(..8),
        Some(b"\x89PNG\r\n\x1a\n".as_slice())
    );
    assert_png_dimensions(&graphics_document_png, 14, 15);
    assert_eq!(
        world_document_png.get(..8),
        Some(b"\x89PNG\r\n\x1a\n".as_slice())
    );
    assert_png_dimensions(&world_document_png, 8, 6);
    assert_eq!(
        overworld_png.get(..8),
        Some(b"\x89PNG\r\n\x1a\n".as_slice())
    );
    assert_png_dimensions(&overworld_png, 6, 4);

    fs::write(
        &dirty_world_edit,
        "LMOWEDT1\nslot 0\npalette-owners 10 editable\nlayer 1 0 0 0002\n",
    )
    .unwrap();
    fs::write(
        &dirty_script,
        format!(
            "world-open-file {}\nworld-edit-file {}\nquit\n",
            world_open_spec.display(),
            dirty_world_edit.display()
        ),
    )
    .unwrap();
    let dirty_exit = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", dirty_script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!dirty_exit.status.success());
    assert_eq!(
        CompleteOverworldFile::decode(&fs::read(&overworld).unwrap(), 32, &modes)
            .unwrap()
            .data
            .layers
            .layer1
            .tiles,
        [1]
    );

    let second = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(fs::read(&map16_output).unwrap(), map16_png);
    assert_eq!(fs::read(&document_output).unwrap(), document_png);
    assert_eq!(fs::read(&overworld_output).unwrap(), overworld_png);
    assert_eq!(
        fs::read(&graphics_document_output).unwrap(),
        graphics_document_png
    );
    assert_eq!(
        fs::read(&palette_document_output).unwrap(),
        palette_document_png
    );
    assert_eq!(
        fs::read(&world_document_output).unwrap(),
        world_document_png
    );
    assert_eq!(
        Map16SetFile::decode(&fs::read(&map16_set).unwrap())
            .unwrap()
            .set
            .pages[0]
            .tiles[0]
            .top_left
            .0,
        1
    );
    assert_eq!(
        Map16SetFile::decode(&fs::read(&map16_set).unwrap())
            .unwrap()
            .set
            .pages
            .len(),
        2
    );
    assert_eq!(
        CompleteOverworldFile::decode(&fs::read(&overworld).unwrap(), 32, &modes)
            .unwrap()
            .data
            .layers
            .layer1
            .tiles,
        [1]
    );
    assert_eq!(
        GraphicsInterchangeFile::decode(&fs::read(&graphics).unwrap())
            .unwrap()
            .graphics
            .tiles[0],
        IndexedTile::new([1; 64])
    );
    assert_eq!(
        PaletteInterchangeFile::decode(&fs::read(&palette).unwrap())
            .unwrap()
            .palette
            .colors[1],
        Bgr555(0x001f)
    );
    assert_eq!(
        CompactExAnimationFile::decode(&fs::read(&exanimation).unwrap(), 32, &modes)
            .unwrap()
            .animation
            .setting,
        5
    );
    fs::remove_dir_all(directory).unwrap();
}
