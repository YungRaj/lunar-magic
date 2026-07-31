use crate::PreparedRomCommit;
use lm_graphics::{GraphicsFile4bpp, JoinedGraphics};
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions, Project, RomMutation};
use lm_rats::ProtectedRange;
use lm_rom::RomImage;

/// Public file identities for one complete graphics pointer table, in pointer-table order.
#[derive(Clone, Copy, Debug)]
pub struct NamedGraphicsImport<'a> {
    pub file_numbers: &'a [usize],
    pub description: &'a str,
}

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
    let file_numbers = (0..layout.pointers.entries).collect::<Vec<_>>();
    prepare_named_graphics_import(
        expected_revision,
        image,
        layout,
        checksum_field,
        raw_files,
        NamedGraphicsImport {
            file_numbers: &file_numbers,
            description: "Insert all standard GFX files",
        },
        options,
    )
}

/// Validates and prepares one atomic replacement of a complete graphics pointer table while
/// retaining the external GFX file numbers used for diagnostics and the commit description.
///
/// `file_numbers` is parallel to the pointer-table order. This matters for recovered tables such
/// as pristine SMW's special pair, whose entries are ordered GFX33 then GFX32.
///
/// # Errors
///
/// Rejects empty or mismatched file mappings, malformed or wrong-shape graphics, unsafe pointer
/// storage, allocation failure, or a batch that does not semantically reopen.
pub fn prepare_named_graphics_import(
    expected_revision: u64,
    image: RomImage,
    layout: GraphicsRomLayout,
    checksum_field: usize,
    raw_files: &[Vec<u8>],
    named: NamedGraphicsImport<'_>,
    options: &GraphicsSaveOptions,
) -> Result<PreparedRomCommit, String> {
    let total = layout.pointers.entries;
    if raw_files.len() != total || named.file_numbers.len() != total || total == 0 {
        return Err(format!(
            "graphics import requires exactly {total} files and file numbers, got {} files and {} numbers",
            raw_files.len(),
            named.file_numbers.len()
        ));
    }
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    let mut decoded = Vec::with_capacity(total);
    for (slot, (bytes, file_number)) in raw_files.iter().zip(named.file_numbers).enumerate() {
        let imported = GraphicsFile4bpp::decode(bytes)
            .map_err(|error| format!("GFX{file_number:02X}: {error}"))?;
        let current = project
            .load_graphics_file(slot, layout)
            .map_err(|error| format!("GFX{file_number:02X}: {error}"))?;
        if imported.tiles.len() != current.tiles.len() {
            return Err(format!(
                "GFX{file_number:02X}: expected {} tiles, got {}",
                current.tiles.len(),
                imported.tiles.len()
            ));
        }
        decoded.push(imported);
    }
    let mut options = options.clone();
    protect_pointer_storage(&mut options, layout, project.rom.logical_len())?;
    project
        .save_graphics_files_with_checksum(&decoded, layout, checksum_field, &options)
        .map_err(|error| error.to_string())?;
    let mutation = RomMutation::between(layout.mapper, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision,
        description: named.description.into(),
        mutation,
    })
}

fn protect_pointer_storage(
    options: &mut GraphicsSaveOptions,
    layout: GraphicsRomLayout,
    image_len: usize,
) -> Result<(), String> {
    let component = |offset: usize,
                     entries: usize,
                     stride: usize,
                     width: usize|
     -> Result<ProtectedRange, String> {
        let len = entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(stride))
            .and_then(|last| last.checked_add(width))
            .ok_or("graphics pointer storage range overflow")?;
        let end = offset
            .checked_add(len)
            .ok_or("graphics pointer storage range overflow")?;
        if end > image_len {
            return Err("graphics pointer storage lies outside the ROM".into());
        }
        Ok(ProtectedRange(offset..end))
    };
    let ranges = if let Some(planes) = layout.split_pointer_planes {
        vec![
            component(planes.low_offset, planes.entries, planes.stride, 1)?,
            component(planes.high_offset, planes.entries, planes.stride, 1)?,
            component(planes.bank_offset, planes.entries, planes.stride, 1)?,
        ]
    } else {
        vec![component(
            layout.pointers.offset,
            layout.pointers.entries,
            layout.pointers.stride,
            3,
        )?]
    };
    for range in ranges {
        if !options.allocation.protected.contains(&range) {
            options.allocation.protected.push(range);
        }
    }
    Ok(())
}

/// Splits one `AllGFX.bin` image at the exact current-ROM file boundaries and prepares the same
/// atomic complete standard-GFX replacement as [`prepare_standard_graphics_import`].
///
/// # Errors
///
/// Returns a diagnostic when current slots cannot be decoded, the joined size is inexact, or the
/// resulting complete batch cannot be prepared.
pub fn prepare_joined_standard_graphics_import(
    expected_revision: u64,
    image: RomImage,
    layout: GraphicsRomLayout,
    checksum_field: usize,
    joined: &[u8],
    options: &GraphicsSaveOptions,
) -> Result<PreparedRomCommit, String> {
    let project = Project::new(image.clone());
    let sizes = (0..layout.pointers.entries)
        .map(|slot| {
            project
                .load_graphics_file(slot, layout)
                .and_then(|graphics| graphics.encode().map_err(Into::into))
                .map(|bytes| bytes.len())
                .map_err(|error| format!("GFX{slot:02X}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let files = JoinedGraphics::split(joined, &sizes)
        .map_err(|error| format!("AllGFX.bin: {error}"))?
        .files;
    prepare_standard_graphics_import(
        expected_revision,
        image,
        layout,
        checksum_field,
        &files,
        options,
    )
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

    #[test]
    fn joined_import_uses_current_rom_boundaries_and_reopens_every_slot() {
        let source = source_image();
        let before = source.logical_bytes().to_vec();
        let expected = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([12; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([13; 64]), IndexedTile::new([14; 64])],
            },
        ];
        let joined = JoinedGraphics {
            files: expected.iter().map(|file| file.encode().unwrap()).collect(),
        }
        .join()
        .unwrap();
        let prepared = prepare_joined_standard_graphics_import(
            23,
            source,
            layout(),
            0x7fdc,
            &joined,
            &options(0x4000..0x7000),
        )
        .unwrap();
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        for (slot, expected) in expected.iter().enumerate() {
            assert_eq!(
                reopened.load_graphics_file(slot, layout()).unwrap(),
                *expected
            );
        }

        assert!(
            prepare_joined_standard_graphics_import(
                0,
                source_image(),
                layout(),
                0x7fdc,
                &joined[..joined.len() - 1],
                &options(0x4000..0x7000),
            )
            .is_err()
        );
    }

    #[test]
    fn named_import_uses_external_file_numbers_and_one_atomic_commit() {
        let source = source_image();
        let before = source.logical_bytes().to_vec();
        let expected = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0x0d; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0x0e; 64]), IndexedTile::new([0x0f; 64])],
            },
        ];
        let raw = expected
            .iter()
            .map(|file| file.encode().unwrap())
            .collect::<Vec<_>>();
        let prepared = prepare_named_graphics_import(
            31,
            source,
            layout(),
            0x7fdc,
            &raw,
            NamedGraphicsImport {
                file_numbers: &[0x33, 0x32],
                description: "Insert GFX32/GFX33 files",
            },
            &options(0x4000..0x7000),
        )
        .unwrap();
        assert_eq!(prepared.expected_revision, 31);
        assert_eq!(prepared.description, "Insert GFX32/GFX33 files");
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        for (slot, expected) in expected.iter().enumerate() {
            assert_eq!(
                reopened.load_graphics_file(slot, layout()).unwrap(),
                *expected
            );
        }

        let error = prepare_named_graphics_import(
            0,
            source_image(),
            layout(),
            0x7fdc,
            &[vec![0; 31], raw[1].clone()],
            NamedGraphicsImport {
                file_numbers: &[0x33, 0x32],
                description: "unused",
            },
            &options(0x4000..0x7000),
        )
        .unwrap_err();
        assert!(error.starts_with("GFX33:"), "{error}");
    }

    #[test]
    fn named_import_protects_every_split_pointer_plane() {
        let mut options = options(0x4000..0x7000);
        let layout = GraphicsRomLayout {
            pointers: LevelPointerTable {
                offset: 0x4100,
                entries: 2,
                stride: 1,
            },
            split_pointer_planes: Some(lm_project::GraphicsPointerPlanes {
                low_offset: 0x4100,
                high_offset: 0x4200,
                bank_offset: 0x4300,
                entries: 2,
                stride: 1,
            }),
            ..layout()
        };
        protect_pointer_storage(&mut options, layout, 0x8000).unwrap();
        for start in [0x4100, 0x4200, 0x4300] {
            assert!(
                options
                    .allocation
                    .protected
                    .contains(&ProtectedRange(start..start + 2))
            );
        }
    }
}
