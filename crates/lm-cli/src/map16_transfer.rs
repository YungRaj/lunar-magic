use crate::args::Map16TransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_exact, read_rom};
use lm_level::{Map16Page, Map16PageFile};
use lm_project::{
    LevelPointerTable, Map16RomLayout, Map16SaveOptions, PayloadReadPolicy, PayloadReclamation,
    Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const PAGE_SLOTS: usize = 0x100;

#[derive(Clone, Copy)]
struct Map16Target {
    page: usize,
    layout: Map16RomLayout,
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: Map16TransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Map16TransferCommand::Export {
            rom,
            mapper,
            page,
            graphics_table,
            acts_like_table,
            output,
        } => export(
            &rom,
            target(mapper, page, graphics_table, acts_like_table),
            &output,
        ),
        Map16TransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            page,
            graphics_table,
            acts_like_table,
            page_file,
            checksum_field,
            search_start,
            search_end,
            ownership_manifest,
        } => import(
            &input_rom,
            &output_rom,
            target(mapper, page, graphics_table, acts_like_table),
            &page_file,
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
    target: Map16Target,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let page_data = project.load_map16_page(target.page, target.layout)?;
    let source_page =
        u16::try_from(target.page).map_err(|_| "Map16 page number exceeds file format")?;
    write_new(
        output,
        Map16PageFile {
            source_page,
            page: page_data,
        }
        .encode()?,
    )?;
    println!("exported-page: {:#04x}", target.page);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    target: Map16Target,
    page_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let page_data = Map16PageFile::decode(&read_exact(
        page_file,
        Map16PageFile::ENCODED_LEN,
        "Map16 page",
    )?)?
    .page;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let snapshot = import_image(
        read_rom(input_rom)?,
        target,
        &page_data,
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, snapshot)?;
    println!("imported-page: {:#04x}", target.page);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: Map16Target,
    page_data: &Map16Page,
    policy: ImportPolicy,
    ownership: Option<&RatsOwnershipManifest>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut project = Project::new(RomImage::from_bytes(input)?);
    if policy.search.start >= policy.search.end || policy.search.end > project.rom.logical_len() {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    let graphics_pointer = target.layout.graphics.pointer_offset(target.page)?;
    let acts_like_pointer = target.layout.acts_like.pointer_offset(target.page)?;
    let previous_graphics = project
        .load_payload(
            graphics_pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x800 },
        )?
        .block;
    let previous_acts_like = project
        .load_payload(
            acts_like_pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x200 },
        )?
        .block;
    let protected = protected_ranges(
        target.layout.graphics.offset,
        target.layout.acts_like.offset,
        policy.checksum_field,
    )?;
    let allocation_policy = AllocationPolicy {
        search: policy.search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected,
    };
    let options = Map16SaveOptions {
        graphics_allocation: allocation_policy.clone(),
        acts_like_allocation: allocation_policy,
        previous_graphics,
        previous_acts_like,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if let Some(manifest) = ownership {
        project.save_map16_page_with_checksum_and_reclamation(
            target.page,
            page_data,
            target.layout,
            &options,
            PayloadReclamation {
                checksum_field: policy.checksum_field,
                manifest,
            },
        )?;
    } else {
        project.save_map16_page_with_checksum(
            target.page,
            page_data,
            target.layout,
            policy.checksum_field,
            &options,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if &reopened.load_map16_page(target.page, target.layout)? != page_data {
        return Err("saved Map16 page failed reopen verification".into());
    }
    Ok(snapshot)
}

fn layout(mapper: Mapper, graphics: usize, acts_like: usize) -> Map16RomLayout {
    let table = |offset| LevelPointerTable {
        offset,
        entries: PAGE_SLOTS,
        stride: 3,
    };
    Map16RomLayout {
        mapper,
        graphics: table(graphics),
        acts_like: table(acts_like),
    }
}

fn target(mapper: Mapper, page: usize, graphics: usize, acts_like: usize) -> Map16Target {
    Map16Target {
        page,
        layout: layout(mapper, graphics, acts_like),
    }
}

fn protected_ranges(
    graphics: usize,
    acts_like: usize,
    checksum: usize,
) -> Result<Vec<ProtectedRange>, Box<dyn std::error::Error>> {
    let table_len = PAGE_SLOTS.checked_mul(3).ok_or("pointer table overflow")?;
    let range = |start: usize, len: usize| {
        start
            .checked_add(len)
            .map(|end| ProtectedRange(start..end))
            .ok_or("protected range overflow")
    };
    Ok(vec![
        range(graphics, table_len)?,
        range(acts_like, table_len)?,
        range(checksum, 4)?,
    ])
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
    use lm_level::{Map16Tile, Subtile};
    use lm_project::RatsOwnershipManifest;
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{compute_snes_checksum, pc_to_snes};

    #[test]
    fn protected_ranges_cover_both_complete_tables_and_checksum() {
        let ranges = protected_ranges(0x100, 0x500, 0x7fdc).unwrap();
        assert_eq!(ranges[0], ProtectedRange(0x100..0x400));
        assert_eq!(ranges[1], ProtectedRange(0x500..0x800));
        assert_eq!(ranges[2], ProtectedRange(0x7fdc..0x7fe0));
    }

    #[test]
    fn layout_declares_every_page_slot() {
        let layout = layout(Mapper::LoRom, 0x20, 0x400);
        assert!(layout.graphics.pointer_offset(0xff).is_ok());
        assert!(layout.graphics.pointer_offset(0x100).is_err());
    }

    #[test]
    fn standalone_file_requires_exact_page_shape() {
        assert!(Map16PageFile::decode(&vec![0; Map16PageFile::ENCODED_LEN - 1]).is_err());
        assert_eq!(Map16Page::TILE_COUNT, 256);
    }

    #[test]
    fn import_updates_both_planes_checksum_and_reopens() {
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x23..0x26].copy_from_slice(&[0x00, 0x90, 0x80]);
        bytes[0x403..0x406].copy_from_slice(&[0x00, 0xa0, 0x80]);
        bytes[0x1000..0x1800].fill(0);
        bytes[0x2000..0x2200].fill(0);
        bytes[0x7fdc..0x7fe0].fill(0);
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[3] = Map16Tile {
            top_left: Subtile(0x4321),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0x130,
        };
        let page = Map16Page::new(tiles).unwrap();
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 0x400),
            &page,
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x3000..0x7000,
            },
            None,
        )
        .unwrap();
        let reopened = Project::new(RomImage::from_bytes(output.clone()).unwrap());
        assert_eq!(
            reopened
                .load_map16_page(1, layout(Mapper::LoRom, 0x20, 0x400))
                .unwrap(),
            page
        );
        assert_eq!(
            &output[0x7fdc..0x7fe0],
            &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
        );
    }

    #[test]
    fn ownership_backed_import_reclaims_both_map16_blocks() {
        let original = Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap();
        let (graphics, acts_like) = original.encode().unwrap();
        let mut bytes = vec![0xff; 0x10000];
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x2000..0x4000));
        let old_graphics = allocator.allocate(&graphics).unwrap();
        let old_acts_like = allocator.allocate(&acts_like).unwrap();
        for (pointer_offset, block) in [(0x23, &old_graphics), (0x403, &old_acts_like)] {
            let pointer = pc_to_snes(Mapper::LoRom, block.payload.start)
                .unwrap()
                .to_le_bytes();
            bytes[pointer_offset..pointer_offset + 3].copy_from_slice(&pointer[..3]);
        }
        bytes[0x7fdc..0x7fe0].fill(0);
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[0].top_left = Subtile(0x1234);
        tiles[0].acts_like = 0x130;
        let replacement = Map16Page::new(tiles).unwrap();
        let manifest = RatsOwnershipManifest {
            owned: vec![old_graphics.clone(), old_acts_like.clone()],
            retained: Vec::new(),
        };
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 0x400),
            &replacement,
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x5000..0x7800,
            },
            Some(&manifest),
        )
        .unwrap();
        for block in &manifest.owned {
            assert!(output[block.full_range()].iter().all(|byte| *byte == 0xff));
        }
        assert_eq!(
            Project::new(RomImage::from_bytes(output.clone()).unwrap())
                .load_map16_page(1, layout(Mapper::LoRom, 0x20, 0x400))
                .unwrap(),
            replacement
        );
        assert_eq!(
            &output[0x7fdc..0x7fe0],
            &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
        );
    }
}
