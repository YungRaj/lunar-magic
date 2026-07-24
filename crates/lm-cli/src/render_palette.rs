use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::PaletteInterchangeFile;
use lm_render::{encode_png, render_portable_palette};
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    palette_path: &Path,
    columns: usize,
    cell_size: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if output == palette_path {
        return Err("render output must differ from its input".into());
    }
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        palette_path,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let canvas = render_portable_palette(&palette, columns, cell_size)?;
    write_new(output, encode_png(&canvas)?)?;
    println!("width: {}", canvas.width());
    println!("height: {}", canvas.height());
    println!("output: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lm-cli-render-palette-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn output_cannot_alias_input() {
        let path = Path::new("same.file");
        assert!(execute(path, 16, 8, path).is_err());
    }

    #[test]
    fn writes_a_png_and_refuses_replacement() {
        let palette_path = path("palette.lmpal");
        let output = path("swatches.png");
        let _ = fs::remove_file(&output);
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f), Bgr555(0x03e0)],
            },
        };
        fs::write(&palette_path, palette.encode().unwrap()).unwrap();
        execute(&palette_path, 2, 3, &output).unwrap();
        assert!(fs::read(&output).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(execute(&palette_path, 2, 3, &output).is_err());
        for file in [&palette_path, &output] {
            let _ = fs::remove_file(file);
        }
    }
}
