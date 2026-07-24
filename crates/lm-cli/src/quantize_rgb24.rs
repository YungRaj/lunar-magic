use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_graphics::{PaletteInterchangeFile, Rgb8, WuQuantizer};
#[cfg(test)]
use std::fs;
use std::io;
use std::path::Path;

const MAX_RGB_BYTES: usize = 16 * 1024 * 1024 * 3;

pub fn execute(
    input: &Path,
    maximum_colors: usize,
    palette_output: &Path,
    indices_output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == palette_output || input == indices_output || palette_output == indices_output {
        return Err("RGB input, palette output, and index output paths must all differ".into());
    }
    let bytes = read_bounded(input, MAX_RGB_BYTES)?;
    if bytes.len() % 3 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RGB24 input contains a partial pixel",
        )
        .into());
    }
    let pixels = bytes
        .chunks_exact(3)
        .map(|pixel| Rgb8 {
            red: pixel[0],
            green: pixel[1],
            blue: pixel[2],
        })
        .collect::<Vec<_>>();
    let quantized = WuQuantizer::quantize(&pixels, maximum_colors)?;
    let palette = PaletteInterchangeFile {
        source_palette: 0,
        palette: quantized.palette,
    }
    .encode()?;
    write_new_batch(&[
        (palette_output, palette.as_slice()),
        (indices_output, quantized.indices.as_slice()),
    ])?;
    println!("pixels: {}", pixels.len());
    println!("colors: {}", palette.len().saturating_sub(16) / 2);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-quantize-rgb24-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn rgb_pixels_publish_palette_and_indexes_together() {
        let directory = directory();
        let input = directory.join("pixels.rgb");
        let palette = directory.join("palette.lmpal");
        let indices = directory.join("pixels.idx");
        fs::write(&input, [255, 0, 0, 250, 4, 0, 0, 0, 255]).unwrap();
        execute(&input, 2, &palette, &indices).unwrap();
        let decoded = PaletteInterchangeFile::decode(&fs::read(&palette).unwrap()).unwrap();
        assert_eq!(decoded.palette.colors.len(), 2);
        assert_eq!(fs::read(&indices).unwrap().len(), 3);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aliases_and_partial_pixels_fail_before_publication() {
        let same = Path::new("same");
        assert!(execute(same, 2, same, Path::new("other")).is_err());
        let directory = directory();
        let input = directory.join("pixels.rgb");
        let palette = directory.join("palette.lmpal");
        let indices = directory.join("pixels.idx");
        fs::write(&input, [1, 2]).unwrap();
        assert!(execute(&input, 2, &palette, &indices).is_err());
        assert!(!palette.exists());
        assert!(!indices.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
