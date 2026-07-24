use super::LevelRenderPaths;
use crate::atomic_output::write_new;
use lm_render::{Canvas, encode_png};
use std::path::Path;

pub(super) fn validate_distinct(
    paths: LevelRenderPaths<'_>,
    extra_input: Option<&Path>,
    output: &Path,
    operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let aliases_required =
        [paths.level, paths.map16, paths.graphics, paths.palette].contains(&output);
    if aliases_required
        || paths.appearances == Some(output)
        || paths.layer3_plane == Some(output)
        || extra_input == Some(output)
    {
        return Err(format!("{operation} output must differ from every input").into());
    }
    Ok(())
}

pub(super) fn write_canvas(
    path: &Path,
    canvas: &Canvas,
    statistic: Option<(&str, usize)>,
) -> Result<(), Box<dyn std::error::Error>> {
    write_new(path, encode_png(canvas)?)?;
    println!("width: {}", canvas.width());
    println!("height: {}", canvas.height());
    if let Some((label, value)) = statistic {
        println!("{label}: {value}");
    }
    println!("output: {}", path.display());
    Ok(())
}
