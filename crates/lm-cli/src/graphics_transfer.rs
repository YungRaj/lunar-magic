use crate::args::GraphicsTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_graphics::{GraphicsFile4bpp, GraphicsInterchangeFile};
use lm_project::{
    GraphicsCompression, GraphicsRomLayout, GraphicsSaveOptions, LevelPointerTable,
    PayloadReadPolicy, Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const GRAPHICS_SLOTS: usize = 0x100;

#[derive(Clone, Copy)]
struct GraphicsTarget {
    slot: usize,
    layout: GraphicsRomLayout,
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: GraphicsTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        GraphicsTransferCommand::Export {
            rom,
            mapper,
            slot,
            pointer_table,
            maximum_compressed_len,
            maximum_decompressed_len,
            compression,
            output,
        } => export(
            &rom,
            target(
                mapper,
                slot,
                pointer_table,
                maximum_compressed_len,
                maximum_decompressed_len,
                compression,
            ),
            &output,
        ),
        GraphicsTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            slot,
            pointer_table,
            maximum_compressed_len,
            maximum_decompressed_len,
            compression,
            graphics_file,
            checksum_field,
            search_start,
            search_end,
            ownership_manifest,
        } => import(
            &input_rom,
            &output_rom,
            target(
                mapper,
                slot,
                pointer_table,
                maximum_compressed_len,
                maximum_decompressed_len,
                compression,
            ),
            &graphics_file,
            ImportPolicy {
                checksum_field,
                search: search_start..search_end,
            },
            ownership_manifest.as_deref(),
        ),
    }
}

fn export(
    rom: &Path,
    target: GraphicsTarget,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let graphics = project.load_graphics_file(target.slot, target.layout)?;
    let source_slot =
        u16::try_from(target.slot).map_err(|_| "graphics slot exceeds file format")?;
    let file = GraphicsInterchangeFile {
        source_slot,
        graphics,
    };
    write_new(output, file.encode()?)?;
    println!("exported-graphics: {:#04x}", target.slot);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    target: GraphicsTarget,
    graphics_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        graphics_file,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?
    .graphics;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let output = import_image(
        read_rom(input_rom)?,
        target,
        &graphics,
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, output)?;
    println!("imported-graphics: {:#04x}", target.slot);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: GraphicsTarget,
    graphics: &GraphicsFile4bpp,
    policy: ImportPolicy,
    ownership: Option<&RatsOwnershipManifest>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut project = Project::new(RomImage::from_bytes(input)?);
    if policy.search.start >= policy.search.end || policy.search.end > project.rom.logical_len() {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    let pointer = target.layout.pointers.pointer_offset(target.slot)?;
    let previous_block = project
        .load_payload(
            pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: target.layout.maximum_compressed_len,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let table_len = GRAPHICS_SLOTS
        .checked_mul(3)
        .ok_or("pointer table overflow")?;
    let allocation_policy = AllocationPolicy {
        search: policy.search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected: vec![
            protected(target.layout.pointers.offset, table_len)?,
            protected(policy.checksum_field, 4)?,
        ],
    };
    let options = GraphicsSaveOptions {
        allocation: allocation_policy,
        previous_block,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if let Some(manifest) = ownership {
        project.save_graphics_file_with_checksum_and_reclamation(
            target.slot,
            graphics,
            target.layout,
            policy.checksum_field,
            &options,
            manifest,
        )?;
    } else {
        project.save_graphics_file_with_checksum(
            target.slot,
            graphics,
            target.layout,
            policy.checksum_field,
            &options,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if &reopened.load_graphics_file(target.slot, target.layout)? != graphics {
        return Err("saved graphics failed reopen verification".into());
    }
    Ok(snapshot)
}

fn layout(
    mapper: Mapper,
    pointer_table: usize,
    maximum_compressed_len: usize,
    maximum_decompressed_len: usize,
    compression: GraphicsCompression,
) -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper,
        pointers: LevelPointerTable {
            offset: pointer_table,
            entries: GRAPHICS_SLOTS,
            stride: 3,
        },
        split_pointer_planes: None,
        compression,
        maximum_compressed_len,
        maximum_decompressed_len,
    }
}

fn target(
    mapper: Mapper,
    slot: usize,
    pointer_table: usize,
    maximum_compressed_len: usize,
    maximum_decompressed_len: usize,
    compression: GraphicsCompression,
) -> GraphicsTarget {
    GraphicsTarget {
        slot,
        layout: layout(
            mapper,
            pointer_table,
            maximum_compressed_len,
            maximum_decompressed_len,
            compression,
        ),
    }
}

fn protected(start: usize, len: usize) -> Result<ProtectedRange, Box<dyn std::error::Error>> {
    Ok(ProtectedRange(
        start..start.checked_add(len).ok_or("protected range overflow")?,
    ))
}

fn require_distinct(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        Err("refusing to overwrite the input ROM; choose a different output path".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_codec::{encode_lz2, encode_lz3};
    use lm_graphics::IndexedTile;
    use lm_project::RatsOwnershipManifest;
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator};
    use lm_rom::{compute_snes_checksum, pc_to_snes};

    fn graphics() -> GraphicsFile4bpp {
        GraphicsFile4bpp {
            tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                index.to_le_bytes()[0] & 0x0f
            }))],
        }
    }

    #[test]
    fn imports_both_compression_modes_then_repointers_checksums_and_reopens() {
        for compression in [GraphicsCompression::Lz2, GraphicsCompression::Lz3] {
            let original = graphics();
            let raw = original.encode().unwrap();
            let compressed = match compression {
                GraphicsCompression::Lz2 => encode_lz2(&raw),
                GraphicsCompression::Lz3 => encode_lz3(&raw),
            };
            let mut bytes = vec![0xff; 0x8000];
            bytes[0x23..0x26].copy_from_slice(&[0x00, 0x90, 0x80]);
            bytes[0x1000..0x1000 + compressed.len()].copy_from_slice(&compressed);
            bytes[0x7fdc..0x7fe0].fill(0);
            let mut replacement = graphics();
            replacement.tiles[0].set_pixel(0, 0, 9).unwrap();
            let output = import_image(
                bytes,
                target(Mapper::LoRom, 1, 0x20, 0x8000, 0x10000, compression),
                &replacement,
                ImportPolicy {
                    checksum_field: 0x7fdc,
                    search: 0x2000..0x7000,
                },
                None,
            )
            .unwrap();
            let reopened = Project::new(RomImage::from_bytes(output.clone()).unwrap());
            assert_eq!(
                reopened
                    .load_graphics_file(
                        1,
                        layout(Mapper::LoRom, 0x20, 0x8000, 0x10000, compression),
                    )
                    .unwrap(),
                replacement
            );
            assert_eq!(
                &output[0x7fdc..0x7fe0],
                &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
            );
        }
    }

    #[test]
    fn ownership_backed_import_reclaims_exact_replaced_block() {
        let original = graphics();
        let compressed = encode_lz2(&original.encode().unwrap());
        let mut bytes = vec![0xff; 0x8000];
        let old = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x1000..0x2000))
            .allocate(&compressed)
            .unwrap();
        let pointer = pc_to_snes(Mapper::LoRom, old.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x23..0x26].copy_from_slice(&pointer[..3]);
        bytes[0x7fdc..0x7fe0].fill(0);
        let mut replacement = original;
        replacement.tiles[0].set_pixel(0, 0, 9).unwrap();
        let output = import_image(
            bytes,
            target(
                Mapper::LoRom,
                1,
                0x20,
                0x8000,
                0x10000,
                GraphicsCompression::Lz2,
            ),
            &replacement,
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x2000..0x7000,
            },
            Some(&RatsOwnershipManifest {
                owned: vec![old.clone()],
                retained: Vec::new(),
            }),
        )
        .unwrap();
        assert!(output[old.full_range()].iter().all(|byte| *byte == 0xff));
        assert_eq!(
            &output[0x7fdc..0x7fe0],
            &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
        );
    }

    #[test]
    fn ownership_backed_import_rejects_a_manifest_for_another_block() {
        let compressed = encode_lz2(&graphics().encode().unwrap());
        let mut bytes = vec![0xff; 0x8000];
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x1000..0x3000));
        let current = allocator.allocate(&compressed).unwrap();
        let foreign = allocator.allocate(&[1, 2, 3]).unwrap();
        let pointer = pc_to_snes(Mapper::LoRom, current.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x23..0x26].copy_from_slice(&pointer[..3]);
        let error = import_image(
            bytes,
            target(
                Mapper::LoRom,
                1,
                0x20,
                0x8000,
                0x10000,
                GraphicsCompression::Lz2,
            ),
            &graphics(),
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x3000..0x7000,
            },
            Some(&RatsOwnershipManifest {
                owned: vec![foreign],
                retained: Vec::new(),
            }),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ReclamationPreviousBlocksMismatch")
        );
    }
}
