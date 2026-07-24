use crate::{
    file_persistence, map16_render_spec, overworld_render_spec, read_bounded_bytes, spec_text,
};
use lm_graphics::{GraphicsInterchangeFile, MaterializedAnimationFrame, PaletteInterchangeFile};
use lm_level::{Map16PageFile, Map16SetFile};
use lm_overworld::SpriteAppearanceFile;
use lm_project::CompleteOverworldFile;
use lm_render::{encode_png, render_portable_map16_page, render_portable_overworld};
use std::path::Path;

pub(crate) fn render_map16_spec(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = crate::editor_shell::read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "Map16 render specification",
    )?;
    let spec = map16_render_spec::parse_map16_render_spec(&text, spec_path)?;
    let graphics = GraphicsInterchangeFile::decode(&asset(
        &spec.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
        "graphics",
    )?)?;
    let palette = PaletteInterchangeFile::decode(&asset(
        &spec.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
        "palette",
    )?)?;
    let page = Map16PageFile::decode(&asset(
        &spec.page,
        Map16PageFile::ENCODED_LEN,
        "Map16 page",
    )?)?;
    let canvas = crate::viewport_spec::render(
        render_portable_map16_page(&graphics, &palette, &page)?,
        spec.viewport,
        spec.overlays.as_deref(),
    )?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "Map16 page rendered: {}x{} — {}",
        canvas.width(),
        canvas.height(),
        spec.output.display()
    );
    Ok(())
}

pub(crate) fn render_overworld_spec(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = crate::editor_shell::read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "overworld render specification",
    )?;
    let spec = overworld_render_spec::parse_overworld_render_spec(&text, spec_path)?;
    let size_modes = asset(&spec.size_modes, 256, "ExAnimation size-mode table")?;
    if size_modes.len() != 256 {
        return Err(format!(
            "ExAnimation size-mode table must contain exactly 256 bytes, got {}",
            size_modes.len()
        )
        .into());
    }
    let modes = size_modes
        .into_iter()
        .map(|value| value != 0)
        .collect::<Vec<_>>();
    let overworld = CompleteOverworldFile::decode(
        &asset(
            &spec.overworld,
            CompleteOverworldFile::MAX_FILE_LEN,
            "complete overworld",
        )?,
        spec.maximum_animation_records,
        &modes,
    )?;
    let map16 = Map16SetFile::decode(&asset(
        &spec.map16,
        Map16SetFile::MAX_FILE_LEN,
        "Map16 set",
    )?)?;
    let graphics = GraphicsInterchangeFile::decode(&asset(
        &spec.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
        "graphics",
    )?)?;
    let appearances = spec
        .appearances
        .as_ref()
        .map(|path| {
            asset(
                path,
                SpriteAppearanceFile::MAX_FILE_LEN,
                "sprite appearances",
            )
            .and_then(|bytes| Ok(SpriteAppearanceFile::decode(&bytes)?))
        })
        .transpose()?;
    let animation_frame = spec
        .animation_frame
        .as_ref()
        .map(|path| {
            asset(
                path,
                MaterializedAnimationFrame::MAX_FILE_LEN,
                "materialized animation frame",
            )
            .and_then(|bytes| Ok(MaterializedAnimationFrame::decode(&bytes)?))
        })
        .transpose()?;
    let world_canvas = render_portable_overworld(
        &overworld,
        &map16,
        &graphics,
        appearances.as_ref(),
        animation_frame.as_ref(),
        spec.completed_reveals,
    )?;
    let canvas =
        crate::viewport_spec::render(world_canvas, spec.viewport, spec.overlays.as_deref())?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "overworld rendered: {}x{} — {} completed reveals — {}",
        canvas.width(),
        canvas.height(),
        spec.completed_reveals,
        spec.output.display()
    );
    Ok(())
}

fn asset(
    path: &Path,
    maximum: usize,
    kind: &'static str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    read_bounded_bytes(path, maximum, kind)
}
