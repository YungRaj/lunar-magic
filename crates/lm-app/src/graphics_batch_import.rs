use crate::PreparedRomCommit;
use lm_graphics::JoinedGraphics;
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions, PayloadPointer, Project, RomMutation};
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

/// Prepares an atomic SMW-US GFX33/GFX32 replacement through the live startup operands.
///
/// Both streams must reside in one LoROM bank because the original startup decoder has one shared
/// bank operand. Candidate banks are tried within the caller's allocation range; a candidate is
/// published only after both payloads decode byte-exactly through the rewritten live operands.
///
/// # Errors
///
/// Rejects unauthenticated startup code, wrong file counts or sizes, incompatible allocation
/// policy, insufficient same-bank space, pointer/checksum failures, or semantic reopen mismatch.
pub fn prepare_smw_us_v1_special_graphics_import(
    expected_revision: u64,
    image: RomImage,
    checksum_field: usize,
    raw_files: &[Vec<u8>],
    options: &GraphicsSaveOptions,
) -> Result<PreparedRomCommit, String> {
    prepare_smw_us_v1_special_graphics_import_inner(
        expected_revision,
        image,
        checksum_field,
        raw_files,
        options,
        true,
    )
}

pub(crate) fn prepare_smw_us_v1_special_graphics_import_resized(
    expected_revision: u64,
    image: RomImage,
    checksum_field: usize,
    raw_files: &[Vec<u8>],
    options: &GraphicsSaveOptions,
) -> Result<PreparedRomCommit, String> {
    prepare_smw_us_v1_special_graphics_import_inner(
        expected_revision,
        image,
        checksum_field,
        raw_files,
        options,
        false,
    )
}

fn prepare_smw_us_v1_special_graphics_import_inner(
    expected_revision: u64,
    image: RomImage,
    checksum_field: usize,
    raw_files: &[Vec<u8>],
    options: &GraphicsSaveOptions,
    require_current_sizes: bool,
) -> Result<PreparedRomCommit, String> {
    const FILE_NUMBERS: [usize; 2] = [0x33, 0x32];
    if raw_files.len() != FILE_NUMBERS.len() {
        return Err(format!(
            "special graphics import requires GFX33 and GFX32; got {} files",
            raw_files.len()
        ));
    }
    let live = lm_profile::smw_us_v1_special_graphics_layouts(&image)
        .map_err(|error| format!("special graphics startup layout: {error}"))?;
    if require_current_sizes {
        let original = Project::new(image.clone());
        for ((file_number, bytes), layout) in FILE_NUMBERS
            .into_iter()
            .zip(raw_files)
            .zip([live.gfx33, live.gfx32])
        {
            let current = original
                .load_decompressed_graphics_file(0, layout)
                .map_err(|error| format!("GFX{file_number:02X}: {error}"))?;
            if bytes.len() != current.len() {
                return Err(format!(
                    "GFX{file_number:02X}: expected {} raw bytes, got {}",
                    current.len(),
                    bytes.len()
                ));
            }
        }
    }
    let gfx33_planes = live
        .gfx33
        .split_pointer_planes
        .ok_or("GFX33 startup layout lost its split operands")?;
    let gfx32_planes = live
        .gfx32
        .split_pointer_planes
        .ok_or("GFX32 startup layout lost its split operands")?;
    if gfx33_planes.bank_offset != gfx32_planes.bank_offset {
        return Err("special graphics startup layouts do not share one bank operand".into());
    }
    let pointers = [
        PayloadPointer::SplitLowBank {
            low_word_offset: gfx33_planes.low_offset,
            bank_offset: gfx33_planes.bank_offset,
            shared_bank: false,
        },
        PayloadPointer::SplitLowBank {
            low_word_offset: gfx32_planes.low_offset,
            bank_offset: gfx32_planes.bank_offset,
            shared_bank: true,
        },
    ];
    let bank_size = options
        .allocation
        .bank_size
        .filter(|size| *size == 0x8000)
        .ok_or("special graphics insertion requires 32 KiB LoROM bank allocation")?;
    let search = options.allocation.search.clone();
    if search.start >= search.end || search.end > image.logical_len() {
        return Err("special graphics allocation search is outside the ROM".into());
    }
    let first_bank = search.start / bank_size;
    let last_bank = (search.end - 1) / bank_size;
    let checksum_end = checksum_field
        .checked_add(4)
        .filter(|end| *end <= image.logical_len())
        .ok_or("special graphics checksum field is outside the ROM")?;
    let mut last_error = None;
    for bank in first_bank..=last_bank {
        let bank_start = bank * bank_size;
        let bank_end = bank_start + bank_size;
        let candidate_start = search.start.max(bank_start);
        let candidate_end = search.end.min(bank_end);
        if candidate_start >= candidate_end {
            continue;
        }
        let mut candidate_options = options.clone();
        candidate_options.allocation.search = candidate_start..candidate_end;
        for range in [
            gfx33_planes.low_offset..gfx33_planes.low_offset + 2,
            gfx32_planes.low_offset..gfx32_planes.low_offset + 2,
            gfx33_planes.bank_offset..gfx33_planes.bank_offset + 1,
            checksum_field..checksum_end,
        ] {
            let protected = ProtectedRange(range);
            if !candidate_options.allocation.protected.contains(&protected) {
                candidate_options.allocation.protected.push(protected);
            }
        }
        let mut project = Project::new(image.clone());
        match project.save_decompressed_graphics_pointers_with_checksum(
            &FILE_NUMBERS,
            &pointers,
            raw_files,
            live.gfx33,
            checksum_field,
            &candidate_options,
        ) {
            Ok(_) => {
                let reopened = lm_profile::smw_us_v1_special_graphics_layouts(&project.rom)
                    .map_err(|error| format!("reopen special graphics startup layout: {error}"))?;
                for ((file_number, expected), layout) in FILE_NUMBERS
                    .into_iter()
                    .zip(raw_files)
                    .zip([reopened.gfx33, reopened.gfx32])
                {
                    let actual = project
                        .load_decompressed_graphics_file(0, layout)
                        .map_err(|error| format!("reopen GFX{file_number:02X}: {error}"))?;
                    if actual != *expected {
                        return Err(format!(
                            "reopen GFX{file_number:02X} differs after insertion"
                        ));
                    }
                }
                let mutation = RomMutation::between(
                    live.gfx33.mapper,
                    image.logical_bytes(),
                    project.rom.logical_bytes(),
                )
                .map_err(|error| error.to_string())?;
                return Ok(PreparedRomCommit {
                    expected_revision,
                    description: "Insert GFX32/GFX33 files".into(),
                    mutation,
                });
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(format!(
        "no single LoROM bank in {:#x}..{:#x} can hold GFX33 and GFX32: {}",
        search.start,
        search.end,
        last_error.unwrap_or_else(|| "no candidate bank".into())
    ))
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
    use lm_codec::encode_lz2;
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

    fn special_source_image() -> (RomImage, [Vec<u8>; 2]) {
        let current = [vec![0x11; 0x1800], vec![0x22; 0x4000]];
        let mut bytes = vec![0xff; 0x1_0000];
        for (offset, value) in [
            (0x3889, &[0x10, 0xa0][..]),
            (0x388d, &[0x84, 0x8a, 0xa9][..]),
            (0x3891, &[0x85, 0x8c][..]),
            (0x38d5, &[0x80, 0xd6, 0xa9][..]),
            (0x38da, &[0x85, 0x8a, 0xe2, 0x20, 0xc2, 0x10][..]),
        ] {
            bytes[offset..offset + value.len()].copy_from_slice(value);
        }
        let sources = [(0x4000, 0x00c000_u32), (0x5000, 0x00d000_u32)];
        for ((pc, pointer), decoded) in sources.into_iter().zip(&current) {
            let compressed = encode_lz2(decoded);
            bytes[pc..pc + compressed.len()].copy_from_slice(&compressed);
            let low = (pointer as u16).to_le_bytes();
            let offset = if pc == 0x4000 {
                lm_profile::SMW_US_V1_GFX33_STARTUP_POINTER_LOW_OFFSET
            } else {
                lm_profile::SMW_US_V1_GFX32_STARTUP_POINTER_LOW_OFFSET
            };
            bytes[offset..offset + 2].copy_from_slice(&low);
        }
        bytes[lm_profile::SMW_US_V1_SPECIAL_GRAPHICS_STARTUP_POINTER_BANK_OFFSET] = 0;
        (RomImage::from_bytes(bytes).unwrap(), current)
    }

    fn special_options(search: std::ops::Range<usize>) -> GraphicsSaveOptions {
        GraphicsSaveOptions {
            allocation: AllocationPolicy {
                search,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
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
    fn special_import_repoints_both_live_operands_into_one_bank_and_reopens() {
        let (source, _) = special_source_image();
        let before = source.logical_bytes().to_vec();
        let expected = [vec![0x5a; 0x1800], vec![0xa5; 0x4000]];
        let prepared = prepare_smw_us_v1_special_graphics_import(
            41,
            source,
            0x7fdc,
            &expected,
            &special_options(0x8000..0x1_0000),
        )
        .unwrap();
        assert_eq!(prepared.expected_revision, 41);
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        let layouts = lm_profile::smw_us_v1_special_graphics_layouts(&reopened.rom).unwrap();
        assert_eq!(
            reopened
                .load_decompressed_graphics_file(0, layouts.gfx33)
                .unwrap(),
            expected[0]
        );
        assert_eq!(
            reopened
                .load_decompressed_graphics_file(0, layouts.gfx32)
                .unwrap(),
            expected[1]
        );
        let gfx33 = layouts.gfx33.read_pointer(&reopened, 0).unwrap().get();
        let gfx32 = layouts.gfx32.read_pointer(&reopened, 0).unwrap().get();
        assert_eq!(gfx33 >> 16, gfx32 >> 16);
        assert_ne!(gfx33 >> 16, 0);
    }

    #[test]
    fn special_import_rejects_corrupt_code_and_missing_same_bank_space() {
        let (source, current) = special_source_image();
        let mut corrupt = source.clone();
        corrupt.write(0x38da, &[0xea]).unwrap();
        assert!(
            prepare_smw_us_v1_special_graphics_import(
                0,
                corrupt,
                0x7fdc,
                &current,
                &special_options(0x8000..0x1_0000),
            )
            .unwrap_err()
            .contains("not authenticated")
        );
        assert!(
            prepare_smw_us_v1_special_graphics_import(
                0,
                source,
                0x7fdc,
                &current,
                &special_options(0x8000..0x8010),
            )
            .unwrap_err()
            .contains("no single LoROM bank")
        );
    }

    #[test]
    #[ignore = "requires a locally supplied Lunar Magic-modified SMW-US ROM"]
    fn external_lunar_magic_rom_special_graphics_repoint_and_reopen() {
        let path = std::env::var_os("LM_SPECIAL_GRAPHICS_ROM")
            .expect("LM_SPECIAL_GRAPHICS_ROM must name the modified ROM");
        let image = RomImage::from_bytes(std::fs::read(path).unwrap()).unwrap();
        let layouts = lm_profile::smw_us_v1_special_graphics_layouts(&image).unwrap();
        let project = Project::new(image.clone());
        let files = [
            project
                .load_decompressed_graphics_file(0, layouts.gfx33)
                .unwrap(),
            project
                .load_decompressed_graphics_file(0, layouts.gfx32)
                .unwrap(),
        ];
        let before = image.logical_bytes().to_vec();
        let mut options = special_options(0x80_000..image.logical_len());
        options.allocation.fill_bytes = vec![0x00, 0xff];
        let prepared =
            prepare_smw_us_v1_special_graphics_import(0, image, 0x7fdc, &files, &options).unwrap();
        let reopened =
            Project::new(RomImage::from_bytes(apply(before, &prepared.mutation)).unwrap());
        let relocated = lm_profile::smw_us_v1_special_graphics_layouts(&reopened.rom).unwrap();
        assert_eq!(
            reopened
                .load_decompressed_graphics_file(0, relocated.gfx33)
                .unwrap(),
            files[0]
        );
        assert_eq!(
            reopened
                .load_decompressed_graphics_file(0, relocated.gfx32)
                .unwrap(),
            files[1]
        );
        assert_eq!(
            relocated.gfx33.read_pointer(&reopened, 0).unwrap().get() >> 16,
            relocated.gfx32.read_pointer(&reopened, 0).unwrap().get() >> 16
        );
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
