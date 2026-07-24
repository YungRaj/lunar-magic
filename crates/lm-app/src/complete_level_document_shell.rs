use crate::{
    complete_level_render_spec, editor_shell::read_bounded_utf8, file_persistence,
    read_bounded_bytes, shell_command,
};
use lm_app::{CompleteLevelDocumentController, render_editor_viewport};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{
    CompleteLevelFile, EntityAppearanceFile, MAX_AUXILIARY_EDIT_SCRIPT_BYTES, Map16SetFile,
    parse_auxiliary_edit_script,
};
use lm_render::{
    EditorOverlayFile, MaterializedLayer3Plane, PortableDscLevelRenderRequest,
    draw_editor_overlays, encode_png, render_portable_level, render_portable_level_with_dsc,
};
use std::path::Path;

mod session;

use session::{
    close_complete_level_document, navigate_complete_level_history, save_complete_level_document,
    show_complete_level_document_status,
};

pub(crate) fn execute_complete_level_document_command(
    session: &mut Option<CompleteLevelDocumentController>,
    command: shell_command::CompleteLevelDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::CompleteLevelDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open_complete_level_document(session, &path),
        DocumentCommand::Edit(path) => edit_complete_level_document(session, &path),
        DocumentCommand::Render(path) => render_complete_level_document(session.as_ref(), &path),
        DocumentCommand::Undo => navigate_complete_level_history(session, true),
        DocumentCommand::Redo => navigate_complete_level_history(session, false),
        DocumentCommand::Status => {
            show_complete_level_document_status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save_complete_level_document(session),
        DocumentCommand::Close => close_complete_level_document(session, false),
        DocumentCommand::Discard => close_complete_level_document(session, true),
    }
}

pub(crate) fn render_complete_level_document(
    session: Option<&CompleteLevelDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no complete level document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        complete_level_render_spec::MAX_SPEC_BYTES,
        "complete-level render specification",
    )?;
    let spec = complete_level_render_spec::parse_complete_level_render_spec(&text, spec_path)?;
    let map16 = lm_level::Map16SetFile::decode(&read_bounded_bytes(
        &spec.map16,
        Map16SetFile::MAX_FILE_LEN,
        "Map16 set",
    )?)?;
    let graphics = GraphicsInterchangeFile::decode(&read_bounded_bytes(
        &spec.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
        "graphics",
    )?)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded_bytes(
        &spec.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
        "palette",
    )?)?;
    let appearances = spec
        .appearances
        .as_ref()
        .map(|path| {
            read_bounded_bytes(
                path,
                EntityAppearanceFile::MAX_FILE_LEN,
                "entity appearances",
            )
            .and_then(|bytes| Ok(EntityAppearanceFile::decode(&bytes)?))
        })
        .transpose()?;
    let layer3_plane = spec
        .layer3_plane
        .as_ref()
        .map(|path| {
            read_bounded_bytes(path, MaterializedLayer3Plane::MAX_FILE_LEN, "Layer 3 plane")
                .and_then(|bytes| Ok(MaterializedLayer3Plane::decode(&bytes)?))
        })
        .transpose()?;
    let dsc = spec
        .dsc
        .as_ref()
        .map(crate::complete_level_dsc_render::load)
        .transpose()?;
    let world_canvas = if let (Some(dsc), Some(dsc_spec)) = (dsc.as_ref(), spec.dsc.as_ref()) {
        render_portable_level_with_dsc(
            controller.value(),
            &map16,
            &graphics,
            &palette,
            PortableDscLevelRenderRequest {
                appearances: appearances.as_ref(),
                layer3: layer3_plane.as_ref(),
                dimensions: spec.dimensions,
                dsc,
                context: dsc_spec.context,
            },
        )?
    } else {
        render_portable_level(
            controller.value(),
            &map16,
            &graphics,
            &palette,
            appearances.as_ref(),
            layer3_plane.as_ref(),
            spec.dimensions,
        )?
    };
    let mut canvas = if let Some(viewport) = spec.viewport {
        render_editor_viewport(
            &world_canvas,
            viewport.camera,
            viewport.width,
            viewport.height,
        )?
    } else {
        world_canvas
    };
    if let Some(path) = spec.overlays {
        let overlays = EditorOverlayFile::decode(&read_bounded_bytes(
            &path,
            EditorOverlayFile::MAX_FILE_LEN,
            "editor overlays",
        )?)?;
        draw_editor_overlays(&mut canvas, &overlays.overlays)?;
    }
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "complete level rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

pub(crate) fn open_complete_level_document(
    session: &mut Option<CompleteLevelDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a complete level document is already open".into());
    }
    let controller = CompleteLevelDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, CompleteLevelFile::MAX_FILE_LEN, "complete level")?,
    )?;
    *session = Some(controller);
    show_complete_level_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn edit_complete_level_document(
    session: &mut Option<CompleteLevelDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        MAX_AUXILIARY_EDIT_SCRIPT_BYTES,
        "level auxiliary edit",
    )?;
    let edits = parse_auxiliary_edit_script(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no complete level document is open")?;
    controller.apply_auxiliary_edits(controller.revision(), &edits)?;
    show_complete_level_document_status(session.as_ref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use lm_level::{LayerData, Level, Map16Page, Map16Set, Map16SetFile, Map16Tile};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-app-complete-level-shell-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn open_edit_dirty_close_save_and_reopen_workflow() {
        let directory = directory();
        let document = directory.join("level.lmlevel");
        let script = directory.join("edit.lmedit");
        fs::write(
            &document,
            CompleteLevelFile(Level {
                layer1: LayerData {
                    raw_tilemap: vec![0],
                    ..LayerData::default()
                },
                layer2: LayerData {
                    raw_tilemap: vec![0],
                    ..LayerData::default()
                },
                ..Level::default()
            })
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &script,
            "LMAUXED1\nentrance-insert 0 main 1 2 3 4 5\nmap16-upsert 0x20 1 2 3 4 5\n",
        )
        .unwrap();

        let mut session = None;
        open_complete_level_document(&mut session, &document).unwrap();
        assert!(open_complete_level_document(&mut session, &document).is_err());
        edit_complete_level_document(&mut session, &script).unwrap();
        assert!(close_complete_level_document(&mut session, false).is_err());
        save_complete_level_document(&mut session).unwrap();
        close_complete_level_document(&mut session, false).unwrap();

        let saved = CompleteLevelFile::decode(&fs::read(&document).unwrap()).unwrap();
        assert_eq!(saved.0.entrances.len(), 1);
        assert_eq!(saved.0.map16_overrides.len(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_script_and_discard_preserve_original_file() {
        let directory = directory();
        let document = directory.join("level.lmlevel");
        let valid = directory.join("valid.lmedit");
        let invalid = directory.join("invalid.lmedit");
        let original = CompleteLevelFile(Level::default()).encode().unwrap();
        fs::write(&document, &original).unwrap();
        fs::write(&valid, "LMAUXED1\nentrance-insert 0 main 0 0 0 0 0\n").unwrap();
        fs::write(&invalid, "LMAUXED1\nentrance-remove 9\n").unwrap();

        let mut session = None;
        open_complete_level_document(&mut session, &document).unwrap();
        assert!(edit_complete_level_document(&mut session, &invalid).is_err());
        assert_eq!(session.as_ref().unwrap().revision(), 0);
        edit_complete_level_document(&mut session, &valid).unwrap();
        close_complete_level_document(&mut session, true).unwrap();
        assert_eq!(fs::read(&document).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renders_current_in_memory_revision_to_create_new_png() {
        let directory = directory();
        let document = directory.join("level.lmlevel");
        let map16 = directory.join("My Map16 日本語.lm16set");
        let graphics = directory.join("graphics.lmgfx");
        let palette = directory.join("palette.lmpal");
        let spec = directory.join("render spec.txt");
        let overlays = directory.join("Editor Overlays.lmovly");
        let output = directory.join("preview image.png");
        fs::write(
            &document,
            CompleteLevelFile(Level {
                layer1: LayerData {
                    raw_tilemap: vec![0],
                    ..LayerData::default()
                },
                layer2: LayerData {
                    raw_tilemap: vec![0],
                    ..LayerData::default()
                },
                ..Level::default()
            })
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &map16,
            Map16SetFile {
                set: Map16Set {
                    pages: vec![
                        Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
                    ],
                },
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &graphics,
            GraphicsInterchangeFile {
                source_slot: 0,
                graphics: GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT])],
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
            &overlays,
            EditorOverlayFile {
                overlays: vec![lm_render::EditorOverlay::Grid(lm_render::GridOverlay {
                    origin_x: 0,
                    origin_y: 0,
                    cell_width: 2,
                    cell_height: 2,
                    color: lm_render::Rgba {
                        red: 255,
                        green: 255,
                        blue: 255,
                        alpha: 255,
                    },
                })],
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &spec,
            "LMBNDR1\nmap16 My Map16 日本語.lm16set\ngraphics graphics.lmgfx\npalette palette.lmpal\noutput preview image.png\noverlays Editor Overlays.lmovly\nlayer1-width 1\nlayer1-height 1\nlayer2-width 1\nlayer2-height 1\nviewport-origin-x -1\nviewport-origin-y 0\nviewport-width 4\nviewport-height 2\nzoom-numerator 2\nzoom-denominator 1\n",
        )
        .unwrap();

        let mut session = None;
        open_complete_level_document(&mut session, &document).unwrap();
        render_complete_level_document(session.as_ref(), &spec).unwrap();
        let png = fs::read(&output).unwrap();
        assert_eq!(png.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
        assert_eq!(png.get(16..20), Some(4_u32.to_be_bytes().as_slice()));
        assert_eq!(png.get(20..24), Some(2_u32.to_be_bytes().as_slice()));
        assert!(render_complete_level_document(session.as_ref(), &spec).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
