use crate::args::RgbMap16ImportCommand;
use crate::atomic_output::write_new_batch;
use crate::indexed_map16_import::build_page;
use crate::oracle_input::{read_bounded, read_exact};
use lm_graphics::{
    GraphicsInterchangeFile, GraphicsOwnership, IndexedBitmapImport, OpaquePaletteRowImport,
    PaletteEntryOwner, PaletteInterchangeFile, PaletteOwnership, Rgb8,
};
use lm_level::Map16PageFile;
#[cfg(test)]
use std::fs;
use std::io;

const WIDTH: usize = 256;
const HEIGHT: usize = 256;
const PIXELS: usize = WIDTH * HEIGHT;
const RGB_BYTES: usize = PIXELS * 3;

pub fn execute(command: &RgbMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(command)?;
    if command.palette_row > 7 {
        return Err("Map16 palette row must be in 0..=7".into());
    }
    let pixels = read_pixels(command)?;
    let (source_palette, palette_import) = prepare_palette(command, &pixels)?;
    let (source_slot, imported) = prepare_graphics(command, &palette_import.indices)?;
    let page = build_page(&imported, command.palette_row, command.acts_like)?;

    let palette_output = PaletteInterchangeFile {
        source_palette,
        palette: palette_import.palette,
    }
    .encode()?;
    let graphics_output = GraphicsInterchangeFile {
        source_slot,
        graphics: imported.graphics,
    }
    .encode()?;
    let occupancy_output = imported
        .occupied
        .iter()
        .map(|occupied| u8::from(*occupied))
        .collect::<Vec<_>>();
    let page_output = Map16PageFile {
        source_page: command.source_page,
        page,
    }
    .encode()?;
    write_new_batch(&[
        (command.palette_output.as_path(), palette_output.as_slice()),
        (
            command.graphics_output.as_path(),
            graphics_output.as_slice(),
        ),
        (
            command.occupancy_output.as_path(),
            occupancy_output.as_slice(),
        ),
        (command.page_output.as_path(), page_output.as_slice()),
    ])?;
    Ok(())
}

fn validate_paths(command: &RgbMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    let paths = [
        command.rgb.as_path(),
        command.palette.as_path(),
        command.palette_access.as_path(),
        command.graphics.as_path(),
        command.occupancy.as_path(),
        command.palette_output.as_path(),
        command.graphics_output.as_path(),
        command.occupancy_output.as_path(),
        command.page_output.as_path(),
    ];
    if paths
        .iter()
        .enumerate()
        .any(|(index, path)| paths[..index].contains(path))
    {
        return Err("RGB Map16 inputs and outputs must all differ".into());
    }
    Ok(())
}

fn read_pixels(command: &RgbMap16ImportCommand) -> Result<Vec<Rgb8>, Box<dyn std::error::Error>> {
    let rgb = read_exact(&command.rgb, RGB_BYTES, "RGB Map16 page")?;
    Ok(rgb
        .chunks_exact(3)
        .map(|pixel| Rgb8 {
            red: pixel[0],
            green: pixel[1],
            blue: pixel[2],
        })
        .collect())
}

fn prepare_palette(
    command: &RgbMap16ImportCommand,
    pixels: &[Rgb8],
) -> Result<(u16, OpaquePaletteRowImport), Box<dyn std::error::Error>> {
    let palette_file = PaletteInterchangeFile::decode(&read_bounded(
        &command.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let palette_access = read_exact(
        &command.palette_access,
        palette_file.palette.colors.len(),
        "palette access",
    )?;
    if palette_access.iter().any(|value| *value > 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "palette access must contain one canonical 0/1 byte per color",
        )
        .into());
    }
    let palette_ownership = PaletteOwnership::from_owners(
        palette_access
            .iter()
            .map(|editable| {
                if *editable == 1 {
                    PaletteEntryOwner::Editable
                } else {
                    PaletteEntryOwner::Fixed
                }
            })
            .collect(),
    );
    let imported = OpaquePaletteRowImport::quantize(
        pixels,
        usize::from(command.palette_row),
        &palette_file.palette,
        &palette_ownership,
    )?;
    Ok((palette_file.source_palette, imported))
}

fn prepare_graphics(
    command: &RgbMap16ImportCommand,
    indices: &[u8],
) -> Result<(u16, IndexedBitmapImport), Box<dyn std::error::Error>> {
    let graphics_file = GraphicsInterchangeFile::decode(&read_bounded(
        &command.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let occupancy = read_exact(
        &command.occupancy,
        graphics_file.graphics.tiles.len(),
        "graphics occupancy",
    )?;
    if occupancy.iter().any(|value| *value > 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "graphics occupancy must contain one canonical 0/1 byte per tile",
        )
        .into());
    }
    let imported = IndexedBitmapImport::materialize(
        WIDTH,
        HEIGHT,
        indices,
        &graphics_file.graphics,
        &GraphicsOwnership::editable(graphics_file.graphics.tiles.len()),
        &occupancy
            .iter()
            .map(|value| *value != 0)
            .collect::<Vec<_>>(),
    )?;
    Ok((graphics_file.source_slot, imported))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-rgb-map16-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn command(directory: &std::path::Path) -> RgbMap16ImportCommand {
        RgbMap16ImportCommand {
            rgb: directory.join("page.rgb"),
            palette: directory.join("base.lmpal"),
            palette_access: directory.join("palette.access"),
            graphics: directory.join("base.lmgfx"),
            occupancy: directory.join("base.occ"),
            palette_row: 2,
            acts_like: 0x130,
            source_page: 0x20,
            palette_output: directory.join("result.lmpal"),
            graphics_output: directory.join("result.lmgfx"),
            occupancy_output: directory.join("result.occ"),
            page_output: directory.join("result.map16"),
        }
    }

    #[test]
    fn rgb_page_publishes_all_four_linked_artifacts() {
        let directory = directory();
        let command = command(&directory);
        let rgb = [240, 16, 8].repeat(PIXELS);
        fs::write(&command.rgb, rgb).unwrap();
        fs::write(
            &command.palette,
            PaletteInterchangeFile {
                source_palette: 4,
                palette: Palette {
                    colors: vec![Bgr555(0); 128],
                },
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(&command.palette_access, vec![1; 128]).unwrap();
        fs::write(
            &command.graphics,
            GraphicsInterchangeFile {
                source_slot: 0x32,
                graphics: GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new([0; 64]); 16],
                },
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(&command.occupancy, [0; 16]).unwrap();
        execute(&command).unwrap();
        let palette =
            PaletteInterchangeFile::decode(&fs::read(&command.palette_output).unwrap()).unwrap();
        assert_eq!(palette.palette.colors[32], Bgr555(0));
        assert_ne!(palette.palette.colors[33], Bgr555(0));
        let page = Map16PageFile::decode(&fs::read(&command.page_output).unwrap()).unwrap();
        assert_eq!(page.page.tiles[0].top_left.palette(), 2);
        assert_eq!(page.page.tiles[0].top_left.tile_number(), 0);
        assert_eq!(fs::read(&command.occupancy_output).unwrap()[0], 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn protected_palette_failure_publishes_nothing() {
        let directory = directory();
        let command = command(&directory);
        fs::write(&command.rgb, [240, 16, 8].repeat(PIXELS)).unwrap();
        fs::write(
            &command.palette,
            PaletteInterchangeFile {
                source_palette: 4,
                palette: Palette {
                    colors: vec![Bgr555(0); 128],
                },
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(&command.palette_access, vec![0; 128]).unwrap();
        // Later inputs deliberately do not exist: palette protection must fail before reading them.
        assert!(execute(&command).is_err());
        assert!(!command.palette_output.exists());
        assert!(!command.graphics_output.exists());
        assert!(!command.occupancy_output.exists());
        assert!(!command.page_output.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
