use crate::atomic_output::write_new;
use crate::command_types::PlanarOperation;
use crate::oracle_input::read_bounded;
use lm_graphics::{IndexedTile, decode_planar_tiles, encode_planar_tiles};
use std::path::Path;

const MAX_GRAPHICS_BYTES: usize = 16 * 1024 * 1024;

pub fn execute(
    operation: PlanarOperation,
    bits_per_pixel: u8,
    input: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("planar graphics output must differ from its input".into());
    }
    if !(1..=8).contains(&bits_per_pixel) {
        return Err(format!("planar bit depth {bits_per_pixel} is outside 1 through 8").into());
    }
    let source = read_bounded(input, MAX_GRAPHICS_BYTES)?;
    let converted = match operation {
        PlanarOperation::Decode => {
            preflight_decoded_len(source.len(), bits_per_pixel)?;
            decode_planar_tiles(&source, bits_per_pixel)?
                .into_iter()
                .flat_map(|tile| tile.pixels().to_vec())
                .collect()
        }
        PlanarOperation::Encode => {
            if source.len() % IndexedTile::PIXEL_COUNT != 0 {
                return Err(format!(
                    "indexed graphics length {} is not a multiple of 64 pixels",
                    source.len()
                )
                .into());
            }
            let tiles = source
                .chunks_exact(IndexedTile::PIXEL_COUNT)
                .map(|pixels| {
                    let mut tile = [0; IndexedTile::PIXEL_COUNT];
                    tile.copy_from_slice(pixels);
                    IndexedTile::new(tile)
                })
                .collect::<Vec<_>>();
            encode_planar_tiles(&tiles, bits_per_pixel)?
        }
    };
    if converted.len() > MAX_GRAPHICS_BYTES {
        return Err("converted planar graphics exceeds the bounded file limit".into());
    }
    write_new(output, converted)?;
    Ok(())
}

fn preflight_decoded_len(
    source_len: usize,
    bits_per_pixel: u8,
) -> Result<usize, Box<dyn std::error::Error>> {
    if !(1..=8).contains(&bits_per_pixel) {
        return Err(format!("planar bit depth {bits_per_pixel} is outside 1 through 8").into());
    }
    let bytes_per_tile = usize::from(bits_per_pixel) * 8;
    if source_len % bytes_per_tile != 0 {
        return Err(format!(
            "planar graphics length {source_len} is not a multiple of {bytes_per_tile} bytes"
        )
        .into());
    }
    let decoded_len = (source_len / bytes_per_tile)
        .checked_mul(IndexedTile::PIXEL_COUNT)
        .ok_or("decoded planar graphics length overflow")?;
    if decoded_len > MAX_GRAPHICS_BYTES {
        return Err("decoded planar graphics exceeds the bounded file limit".into());
    }
    Ok(decoded_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-planar-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn odd_depth_file_workflow_round_trips_and_refuses_partial_or_existing_outputs() {
        let directory = directory();
        let indexed = directory.join("indexed.bin");
        let planar = directory.join("tiles.3bpp");
        let decoded = directory.join("decoded.bin");
        let pixels = (0_u8..64).map(|pixel| pixel & 7).collect::<Vec<_>>();
        fs::write(&indexed, &pixels).unwrap();
        execute(PlanarOperation::Encode, 3, &indexed, &planar).unwrap();
        assert_eq!(fs::metadata(&planar).unwrap().len(), 24);
        execute(PlanarOperation::Decode, 3, &planar, &decoded).unwrap();
        assert_eq!(fs::read(&decoded).unwrap(), pixels);
        assert!(execute(PlanarOperation::Decode, 3, &planar, &decoded).is_err());
        assert!(execute(PlanarOperation::Decode, 3, &planar, &planar).is_err());
        fs::write(directory.join("partial.bin"), [0; 63]).unwrap();
        assert!(
            execute(
                PlanarOperation::Encode,
                3,
                &directory.join("partial.bin"),
                &directory.join("partial.3bpp")
            )
            .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn decoded_size_is_rejected_before_tile_materialization() {
        assert_eq!(preflight_decoded_len(24, 3).unwrap(), 64);
        assert!(preflight_decoded_len(8, 0).is_err());
        assert!(preflight_decoded_len(23, 3).is_err());
        assert!(preflight_decoded_len(MAX_GRAPHICS_BYTES, 1).is_err());
    }
}
