use crate::PreparedRomCommit;
use lm_graphics::JoinedGraphics;
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions, Project, RomMutation};
use lm_rats::ProtectedRange;
use lm_rom::RomImage;

/// Public file identities for one complete graphics pointer table, in pointer-table order.
#[derive(Clone, Copy, Debug)]
pub struct NamedGraphicsImport<'a> {
    /// Pointer-table slots to replace, in the same order as `file_numbers` and the raw inputs.
    pub slots: &'a [usize],
    pub file_numbers: &'a [usize],
    pub description: &'a str,
}

/// Validates and prepares one atomic replacement of the profile's standard GFX range.
///
/// Each raw input must retain the exact decompressed size of its corresponding ROM slot. All
/// compressed payloads are then allocated, repointed, semantically reopened, and
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
    let file_numbers = (0..layout.pointers.entries.min(0x34)).collect::<Vec<_>>();
    prepare_named_graphics_import(
        expected_revision,
        image,
        layout,
        checksum_field,
        raw_files,
        NamedGraphicsImport {
            slots: &file_numbers,
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
    let total = raw_files.len();
    if total == 0 || named.slots.len() != total || named.file_numbers.len() != total {
        return Err(format!(
            "graphics import requires equal nonzero file, slot, and file-number counts; got {} files, {} slots, and {} numbers",
            raw_files.len(),
            named.slots.len(),
            named.file_numbers.len()
        ));
    }
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    let mut decoded = Vec::with_capacity(total);
    for ((bytes, slot), file_number) in raw_files.iter().zip(named.slots).zip(named.file_numbers) {
        let pointer = layout
            .read_pointer(&project, *slot)
            .map_err(|error| format!("{}: {error}", graphics_file_label(*file_number)))?;
        if *slot >= 0x80 && pointer.get() == 0 {
            if !matches!(bytes.len(), 0x800 | 0x0c00 | 0x1000) {
                return Err(format!(
                    "{}: new ExGFX must contain 2048, 3072, or 4096 raw bytes, got {}",
                    graphics_file_label(*file_number),
                    bytes.len()
                ));
            }
        } else {
            let current = project
                .load_decompressed_graphics_file(*slot, layout)
                .map_err(|error| format!("{}: {error}", graphics_file_label(*file_number)))?;
            if bytes.len() != current.len() {
                return Err(format!(
                    "{}: expected {} raw bytes, got {}",
                    graphics_file_label(*file_number),
                    current.len(),
                    bytes.len()
                ));
            }
        }
        decoded.push(bytes.clone());
    }
    let mut options = options.clone();
    protect_pointer_storage(&mut options, layout, project.rom.logical_len())?;
    project
        .save_decompressed_graphics_slots_with_checksum(
            named.slots,
            &decoded,
            layout,
            checksum_field,
            &options,
        )
        .map_err(|error| error.to_string())?;
    let mutation = RomMutation::between(layout.mapper, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision,
        description: named.description.into(),
        mutation,
    })
}

fn graphics_file_label(file_number: usize) -> String {
    let prefix = if file_number < 0x80 { "GFX" } else { "ExGFX" };
    format!("{prefix}{file_number:02X}")
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
    let slots = (0..layout.pointers.entries.min(0x34)).collect::<Vec<_>>();
    let sizes = slots
        .iter()
        .copied()
        .map(|slot| {
            project
                .load_decompressed_graphics_file(slot, layout)
                .map(|bytes| bytes.len())
                .map_err(|error| format!("GFX{slot:02X}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let files = JoinedGraphics::split(joined, &sizes)
        .map_err(|error| format!("AllGFX.bin: {error}"))?
        .files;
    prepare_named_graphics_import(
        expected_revision,
        image,
        layout,
        checksum_field,
        &files,
        NamedGraphicsImport {
            slots: &slots,
            file_numbers: &slots,
            description: "Insert AllGFX.bin",
        },
        options,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
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
                slots: &[0, 1],
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
                slots: &[0, 1],
                file_numbers: &[0x33, 0x32],
                description: "unused",
            },
            &options(0x4000..0x7000),
        )
        .unwrap_err();
        assert!(error.starts_with("GFX33:"), "{error}");
    }

    #[test]
    fn named_import_replaces_only_the_selected_sparse_slots() {
        let sparse_layout = GraphicsRomLayout {
            pointers: LevelPointerTable {
                entries: 4,
                ..layout().pointers
            },
            ..layout()
        };
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let originals = (0_u8..4)
            .map(|color| GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([color; 64])],
            })
            .collect::<Vec<_>>();
        project
            .save_graphics_files_with_checksum(
                &originals,
                sparse_layout,
                0x7fdc,
                &options(0x1000..0x4000),
            )
            .unwrap();
        let source = project.rom;
        let before = source.logical_bytes().to_vec();
        let replacements = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([8; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([9; 64])],
            },
        ];
        let raw = replacements
            .iter()
            .map(|file| file.encode().unwrap())
            .collect::<Vec<_>>();
        let prepared = prepare_named_graphics_import(
            7,
            source,
            sparse_layout,
            0x7fdc,
            &raw,
            NamedGraphicsImport {
                slots: &[1, 3],
                file_numbers: &[0x81, 0x123],
                description: "Insert ExGFX files",
            },
            &options(0x4000..0x7000),
        )
        .unwrap();
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        assert_eq!(
            reopened.load_graphics_file(0, sparse_layout).unwrap(),
            originals[0]
        );
        assert_eq!(
            reopened.load_graphics_file(1, sparse_layout).unwrap(),
            replacements[0]
        );
        assert_eq!(
            reopened.load_graphics_file(2, sparse_layout).unwrap(),
            originals[2]
        );
        assert_eq!(
            reopened.load_graphics_file(3, sparse_layout).unwrap(),
            replacements[1]
        );
    }

    #[test]
    fn named_import_can_install_a_new_native_depth_exgraphics_slot() {
        let expanded_layout = GraphicsRomLayout {
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 0x81,
                stride: 3,
            },
            maximum_decompressed_len: 0x1000,
            ..layout()
        };
        let mut source_bytes = vec![0xff; 0x8000];
        let pointer = expanded_layout.pointers.offset + 0x80 * 3;
        source_bytes[pointer..pointer + 3].fill(0);
        let source = RomImage::from_bytes(source_bytes).unwrap();
        let raw = vec![vec![0x5a; 0x800]];
        let prepared = prepare_named_graphics_import(
            9,
            source.clone(),
            expanded_layout,
            0x7fdc,
            &raw,
            NamedGraphicsImport {
                slots: &[0x80],
                file_numbers: &[0x80],
                description: "Insert ExGFX files",
            },
            &options(0x4000..0x7000),
        )
        .unwrap();
        let reopened = Project::new(
            RomImage::from_bytes(apply(source.logical_bytes().to_vec(), &prepared.mutation))
                .unwrap(),
        );
        assert_eq!(
            reopened
                .load_decompressed_graphics_file(0x80, expanded_layout)
                .unwrap(),
            raw[0]
        );

        let error = prepare_named_graphics_import(
            0,
            source,
            expanded_layout,
            0x7fdc,
            &[vec![0; 0x801]],
            NamedGraphicsImport {
                slots: &[0x80],
                file_numbers: &[0x80],
                description: "unused",
            },
            &options(0x4000..0x7000),
        )
        .unwrap_err();
        assert!(error.starts_with("ExGFX80:"), "{error}");
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
