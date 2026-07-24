use crate::args::RgbaMap16ImportCommand;
use crate::atomic_output::write_new_batch;
use crate::indexed_map16_import::build_page;
use crate::oracle_input::{read_bounded, read_exact};
use lm_graphics::{
    GraphicsInterchangeFile, GraphicsOwnership, IndexedBitmapImport, PaletteEntryOwner,
    PaletteInterchangeFile, PaletteOwnership, Rgba8, TransparentPaletteRowImport,
};
use lm_level::Map16PageFile;
#[cfg(test)]
use std::fs;
use std::io;

const WIDTH: usize = 256;
const HEIGHT: usize = 256;
const PIXELS: usize = WIDTH * HEIGHT;
const RGBA_BYTES: usize = PIXELS * 4;

pub fn execute(command: &RgbaMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    let pixels = read_pixels(command)?;
    execute_pixels(command, &pixels)
}

pub(crate) fn execute_pixels(
    command: &RgbaMap16ImportCommand,
    pixels: &[Rgba8],
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(command)?;
    if command.palette_row > 7 {
        return Err("Map16 palette row must be in 0..=7".into());
    }
    if pixels.len() != PIXELS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("RGBA Map16 page must contain exactly {PIXELS} pixels"),
        )
        .into());
    }
    let palette_file = PaletteInterchangeFile::decode(&read_bounded(
        &command.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let ownership = read_palette_ownership(command, palette_file.palette.colors.len())?;
    let palette_import = TransparentPaletteRowImport::quantize(
        pixels,
        usize::from(command.palette_row),
        &palette_file.palette,
        &ownership,
    )?;
    let graphics_file = GraphicsInterchangeFile::decode(&read_bounded(
        &command.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let occupied = read_occupancy(command, graphics_file.graphics.tiles.len())?;
    let imported = IndexedBitmapImport::materialize(
        WIDTH,
        HEIGHT,
        &palette_import.indices,
        &graphics_file.graphics,
        &GraphicsOwnership::editable(graphics_file.graphics.tiles.len()),
        &occupied,
    )?;
    let page = build_page(&imported, command.palette_row, command.acts_like)?;
    publish(
        command,
        palette_file.source_palette,
        graphics_file.source_slot,
        palette_import,
        imported,
        page,
    )
}

fn validate_paths(command: &RgbaMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    let paths = [
        command.rgba.as_path(),
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
        return Err("RGBA Map16 inputs and outputs must all differ".into());
    }
    Ok(())
}

fn read_pixels(command: &RgbaMap16ImportCommand) -> Result<Vec<Rgba8>, io::Error> {
    let rgba = read_exact(&command.rgba, RGBA_BYTES, "RGBA Map16 page")?;
    Ok(rgba
        .chunks_exact(4)
        .map(|pixel| Rgba8 {
            red: pixel[0],
            green: pixel[1],
            blue: pixel[2],
            alpha: pixel[3],
        })
        .collect())
}

fn read_palette_ownership(
    command: &RgbaMap16ImportCommand,
    colors: usize,
) -> Result<PaletteOwnership, io::Error> {
    let access = read_exact(&command.palette_access, colors, "palette access")?;
    if access.iter().any(|value| *value > 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "palette access must contain one canonical 0/1 byte per color",
        ));
    }
    Ok(PaletteOwnership::from_owners(
        access
            .into_iter()
            .map(|editable| {
                if editable == 1 {
                    PaletteEntryOwner::Editable
                } else {
                    PaletteEntryOwner::Fixed
                }
            })
            .collect(),
    ))
}

fn read_occupancy(command: &RgbaMap16ImportCommand, tiles: usize) -> Result<Vec<bool>, io::Error> {
    let occupancy = read_exact(&command.occupancy, tiles, "graphics occupancy")?;
    if occupancy.iter().any(|value| *value > 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "graphics occupancy must contain one canonical 0/1 byte per tile",
        ));
    }
    Ok(occupancy.into_iter().map(|value| value != 0).collect())
}

fn publish(
    command: &RgbaMap16ImportCommand,
    source_palette: u16,
    source_slot: u16,
    palette_import: TransparentPaletteRowImport,
    imported: IndexedBitmapImport,
    page: lm_level::Map16Page,
) -> Result<(), Box<dyn std::error::Error>> {
    let palette = PaletteInterchangeFile {
        source_palette,
        palette: palette_import.palette,
    }
    .encode()?;
    let graphics = GraphicsInterchangeFile {
        source_slot,
        graphics: imported.graphics,
    }
    .encode()?;
    let occupancy = imported
        .occupied
        .into_iter()
        .map(u8::from)
        .collect::<Vec<_>>();
    let page = Map16PageFile {
        source_page: command.source_page,
        page,
    }
    .encode()?;
    write_new_batch(&[
        (command.palette_output.as_path(), palette.as_slice()),
        (command.graphics_output.as_path(), graphics.as_slice()),
        (command.occupancy_output.as_path(), occupancy.as_slice()),
        (command.page_output.as_path(), page.as_slice()),
    ])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-rgba-map16-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn command(directory: &Path) -> RgbaMap16ImportCommand {
        RgbaMap16ImportCommand {
            rgba: directory.join("page.rgba"),
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

    fn write_base(command: &RgbaMap16ImportCommand) {
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
    }

    #[test]
    fn transparent_and_opaque_pixels_publish_linked_artifacts() {
        let directory = directory();
        let command = command(&directory);
        let mut rgba = [240, 16, 8, 255].repeat(PIXELS);
        for pixel in rgba.chunks_exact_mut(4).step_by(2) {
            pixel[3] = 0;
        }
        fs::write(&command.rgba, rgba).unwrap();
        write_base(&command);
        execute(&command).unwrap();

        let palette =
            PaletteInterchangeFile::decode(&fs::read(&command.palette_output).unwrap()).unwrap();
        assert_eq!(palette.palette.colors[32], Bgr555(0));
        assert_ne!(palette.palette.colors[33], Bgr555(0));
        let graphics =
            GraphicsInterchangeFile::decode(&fs::read(&command.graphics_output).unwrap()).unwrap();
        assert_eq!(graphics.graphics.tiles[0].pixels()[0], 0);
        assert_ne!(graphics.graphics.tiles[0].pixels()[1], 0);
        assert!(command.page_output.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fractional_alpha_failure_is_atomic_and_precedes_graphics_reads() {
        let directory = directory();
        let command = command(&directory);
        fs::write(&command.rgba, [1, 2, 3, 128].repeat(PIXELS)).unwrap();
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
        assert!(execute(&command).is_err());
        assert!(!command.palette_output.exists());
        assert!(!command.graphics_output.exists());
        assert!(!command.occupancy_output.exists());
        assert!(!command.page_output.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
