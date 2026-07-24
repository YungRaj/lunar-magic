use super::*;
use lm_graphics::{
    Bgr555, GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile, Palette, PaletteInterchangeFile,
};
use lm_level::{Map16Page, Map16Tile};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn path(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lm-map16-page-shell-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn file() -> Map16PageFile {
    Map16PageFile {
        source_page: 0x12,
        page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
    }
}

#[test]
fn open_edit_save_close_round_trips_a_real_page() {
    let document = path("Page 日本語.map16");
    let edits = path("Page edits.txt");
    let render_spec = path("Page render.txt");
    let graphics = path("Page graphics.lmgfx");
    let palette = path("Page palette.lmpal");
    let output = path("Page preview.png");
    fs::write(&document, file().encode().unwrap()).unwrap();
    fs::write(
        &edits,
        "LMPGEDT1\ntile 01 1 2 3 4 abcd\nsubtile 01 br 8004\nacts-like 02 ffff\n",
    )
    .unwrap();
    fs::write(
        &graphics,
        GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT]); 5],
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
    fs::write(
        &render_spec,
        format!(
            "LMPGDR1\ngraphics {}\npalette {}\noutput {}\n",
            graphics.display(),
            palette.display(),
            output.display()
        ),
    )
    .unwrap();
    let mut session = None;
    open(&mut session, &document).unwrap();
    edit(&mut session, &edits).unwrap();
    render(session.as_ref(), &render_spec).unwrap();
    assert!(output.is_file());
    let expected = encode_png(
        &render_portable_map16_page(
            &GraphicsInterchangeFile::decode(&fs::read(&graphics).unwrap()).unwrap(),
            &PaletteInterchangeFile::decode(&fs::read(&palette).unwrap()).unwrap(),
            session.as_ref().unwrap().value(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert_eq!(
        Map16PageFile::decode(&fs::read(&document).unwrap()).unwrap(),
        file()
    );
    assert!(close(&mut session, false).is_err());
    save(&mut session).unwrap();
    close(&mut session, false).unwrap();
    let saved = Map16PageFile::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved.source_page, 0x12);
    assert_eq!(saved.page.tiles[1].bottom_right.0, 0x8004);
    assert_eq!(saved.page.tiles[2].acts_like, 0xffff);
    fs::remove_file(document).unwrap();
    fs::remove_file(edits).unwrap();
    fs::remove_file(render_spec).unwrap();
    fs::remove_file(graphics).unwrap();
    fs::remove_file(palette).unwrap();
    fs::remove_file(output).unwrap();
}

#[test]
fn failed_open_and_dirty_discard_preserve_the_file() {
    let document = path("Page.map16");
    fs::write(&document, b"bad").unwrap();
    let mut session = None;
    assert!(open(&mut session, &document).is_err());
    assert!(session.is_none());
    let original = file().encode().unwrap();
    fs::write(&document, &original).unwrap();
    open(&mut session, &document).unwrap();
    session
        .as_mut()
        .unwrap()
        .apply_edits(
            0,
            &[lm_app::Map16PageDocumentEdit::SetActsLike { tile: 0, value: 1 }],
        )
        .unwrap();
    close(&mut session, true).unwrap();
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_file(document).unwrap();
}
