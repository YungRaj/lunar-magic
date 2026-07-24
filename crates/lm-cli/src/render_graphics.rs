use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_render::{encode_png, render_portable_graphics};
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    graphics_path: &Path,
    palette_path: &Path,
    palette_row: usize,
    columns: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if output == graphics_path || output == palette_path {
        return Err("render output must differ from every input".into());
    }
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        graphics_path,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        palette_path,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let canvas = render_portable_graphics(&graphics, &palette, palette_row, columns)?;
    write_new(output, encode_png(&canvas)?)?;
    println!("width: {}", canvas.width());
    println!("height: {}", canvas.height());
    println!("output: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lm-cli-render-graphics-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn output_cannot_alias_inputs() {
        let path = Path::new("same.file");
        assert!(execute(path, path, 0, 16, path).is_err());
    }

    #[test]
    fn oversized_input_cannot_publish_render_output() {
        let graphics_path = path("oversized-graphics.lmgfx");
        let palette_path = path("unused-palette.lmpal");
        let output = path("oversized-sheet.png");
        for file in [&graphics_path, &palette_path, &output] {
            let _ = fs::remove_file(file);
        }
        fs::File::create(&graphics_path)
            .unwrap()
            .set_len(u64::try_from(GraphicsInterchangeFile::MAX_FILE_LEN + 1).unwrap())
            .unwrap();
        assert!(execute(&graphics_path, &palette_path, 0, 1, &output).is_err());
        assert!(!output.exists());
        fs::remove_file(graphics_path).unwrap();
    }

    #[test]
    fn writes_a_png_and_refuses_replacement() {
        let graphics_path = path("graphics.lmgfx");
        let palette_path = path("palette.lmpal");
        let output = path("sheet.png");
        let _ = fs::remove_file(&output);
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; 64])],
            },
        };
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f)]
                    .into_iter()
                    .chain(std::iter::repeat_n(Bgr555(0), 14))
                    .collect(),
            },
        };
        fs::write(&graphics_path, graphics.encode().unwrap()).unwrap();
        fs::write(&palette_path, palette.encode().unwrap()).unwrap();
        execute(&graphics_path, &palette_path, 0, 1, &output).unwrap();
        assert!(fs::read(&output).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(execute(&graphics_path, &palette_path, 0, 1, &output).is_err());
        for file in [&graphics_path, &palette_path, &output] {
            let _ = fs::remove_file(file);
        }
    }
}
