use crate::PreparedRomCommit;
use lm_graphics::GraphicsFile4bpp;
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions, Project, RomMutation};
use lm_rom::RomImage;

/// Validates and prepares one atomic replacement of every profile-declared standard GFX file.
///
/// Each raw input must be valid 4bpp and retain the exact decoded tile count of its corresponding
/// ROM slot. All compressed payloads are then allocated, repointed, semantically reopened, and
/// checksum-repaired by one private project transaction before a revision-bound commit is returned.
///
/// # Errors
///
/// Returns a slot-addressed diagnostic for count, decoding, shape, native I/O, or mutation errors.
pub fn prepare_standard_graphics_import(
    expected_revision: u64,
    image: RomImage,
    layout: GraphicsRomLayout,
    checksum_field: usize,
    raw_files: &[Vec<u8>],
    options: &GraphicsSaveOptions,
) -> Result<PreparedRomCommit, String> {
    let total = layout.pointers.entries;
    if raw_files.len() != total || total == 0 {
        return Err(format!(
            "standard GFX import requires exactly {total} files, got {}",
            raw_files.len()
        ));
    }
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    let mut decoded = Vec::with_capacity(total);
    for (slot, bytes) in raw_files.iter().enumerate() {
        let imported =
            GraphicsFile4bpp::decode(bytes).map_err(|error| format!("GFX{slot:02X}: {error}"))?;
        let current = project
            .load_graphics_file(slot, layout)
            .map_err(|error| format!("GFX{slot:02X}: {error}"))?;
        if imported.tiles.len() != current.tiles.len() {
            return Err(format!(
                "GFX{slot:02X}: expected {} tiles, got {}",
                current.tiles.len(),
                imported.tiles.len()
            ));
        }
        decoded.push(imported);
    }
    project
        .save_graphics_files_with_checksum(&decoded, layout, checksum_field, options)
        .map_err(|error| error.to_string())?;
    let mutation = RomMutation::between(layout.mapper, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision,
        description: "Insert all standard GFX files".into(),
        mutation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::IndexedTile;
    use lm_project::{GraphicsCompression, LevelPointerTable};
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::Mapper;

    fn layout() -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 2,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        }
    }

    fn options(search: std::ops::Range<usize>) -> GraphicsSaveOptions {
        GraphicsSaveOptions {
            allocation: AllocationPolicy {
                search,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x200..0x206), ProtectedRange(0x7fdc..0x7fe0)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn source_image() -> RomImage {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let files = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([2; 64]), IndexedTile::new([3; 64])],
            },
        ];
        project
            .save_graphics_files_with_checksum(&files, layout(), 0x7fdc, &options(0x1000..0x4000))
            .unwrap();
        project.rom
    }

    fn apply(mut bytes: Vec<u8>, mutation: &RomMutation) -> Vec<u8> {
        bytes.extend_from_slice(&mutation.appended);
        for write in &mutation.writes {
            let end = write.offset + write.bytes.len();
            bytes[write.offset..end].copy_from_slice(&write.bytes);
        }
        bytes
    }

    #[test]
    fn prepared_batch_changes_every_slot_and_reopens_exactly() {
        let source = source_image();
        let before = source.logical_bytes().to_vec();
        let expected = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([9; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([10; 64]), IndexedTile::new([11; 64])],
            },
        ];
        let raw = expected
            .iter()
            .map(|file| file.encode().unwrap())
            .collect::<Vec<_>>();
        let prepared = prepare_standard_graphics_import(
            17,
            source,
            layout(),
            0x7fdc,
            &raw,
            &options(0x4000..0x7000),
        )
        .unwrap();
        assert_eq!(prepared.expected_revision, 17);
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        for (slot, expected) in expected.iter().enumerate() {
            assert_eq!(
                reopened.load_graphics_file(slot, layout()).unwrap(),
                *expected
            );
        }
    }

    #[test]
    fn missing_and_wrong_shape_batches_are_rejected_before_a_commit_exists() {
        let source = source_image();
        assert!(
            prepare_standard_graphics_import(
                0,
                source.clone(),
                layout(),
                0x7fdc,
                &[],
                &options(0x4000..0x7000),
            )
            .is_err()
        );
        let wrong = vec![vec![0; 64], vec![0; 64]];
        assert!(
            prepare_standard_graphics_import(
                0,
                source,
                layout(),
                0x7fdc,
                &wrong,
                &options(0x4000..0x7000),
            )
            .is_err()
        );
    }
}
