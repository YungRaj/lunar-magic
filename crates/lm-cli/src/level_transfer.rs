use crate::args::LevelTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_level::{NativeLevelFile, SpriteLengthTable};
use lm_project::{
    LevelPointerTable, LevelRomLayout, LevelSaveOptions, LoadedLevelSlot, PayloadReadPolicy,
    PayloadReclamation, Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const LEVEL_SLOTS: usize = 0x200;

#[derive(Clone, Copy)]
struct LevelTarget {
    level: usize,
    layout: LevelRomLayout,
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: LevelTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        LevelTransferCommand::Export {
            rom,
            mapper,
            level,
            layer1_table,
            sprite_table,
            expanded_sprites,
            sprite_lengths,
            output,
        } => export(
            &rom,
            target(mapper, level, layer1_table, sprite_table, expanded_sprites),
            sprite_lengths.as_deref(),
            &output,
        ),
        LevelTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            level,
            layer1_table,
            sprite_table,
            expanded_sprites,
            sprite_lengths,
            level_file,
            checksum_field,
            search_start,
            search_end,
            ownership_manifest,
        } => import(
            &input_rom,
            &output_rom,
            target(mapper, level, layer1_table, sprite_table, expanded_sprites),
            sprite_lengths.as_deref(),
            &level_file,
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
    target: LevelTarget,
    sprite_lengths: Option<&Path>,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let lengths = crate::sprite_length_file::read(sprite_lengths)?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let loaded = project.load_level_slot(target.level, target.layout, &lengths)?;
    let source_level =
        u16::try_from(target.level).map_err(|_| "level number exceeds file format")?;
    write_new(
        output,
        NativeLevelFile {
            source_level,
            layer1: loaded.layer1,
            sprites: loaded.sprites,
        }
        .encode()?,
    )?;
    println!("exported-level: {:#05x}", target.level);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    target: LevelTarget,
    sprite_lengths: Option<&Path>,
    level_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let lengths = crate::sprite_length_file::read(sprite_lengths)?;
    let file = NativeLevelFile::decode(
        &read_bounded(level_file, NativeLevelFile::MAX_FILE_LEN)?,
        &lengths,
    )?;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let snapshot = import_image(
        read_rom(input_rom)?,
        target,
        &lengths,
        &file,
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, snapshot)?;
    println!("imported-level: {:#05x}", target.level);
    println!("source-level: {:#05x}", file.source_level);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: LevelTarget,
    sprite_lengths: &SpriteLengthTable,
    file: &NativeLevelFile,
    policy: ImportPolicy,
    ownership: Option<&RatsOwnershipManifest>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut project = Project::new(RomImage::from_bytes(input)?);
    if policy.search.start >= policy.search.end || policy.search.end > project.rom.logical_len() {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    let layer1_pointer = target.layout.layer1.pointer_offset(target.level)?;
    let sprite_pointer = target
        .layout
        .sprites
        .low_or_contiguous_table()
        .pointer_offset(target.level)?;
    let previous_layer1 = project
        .load_payload(
            layer1_pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrTerminated {
                terminator: vec![0xff],
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let previous_sprites = project
        .load_payload(
            sprite_pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let table_len = LEVEL_SLOTS.checked_mul(3).ok_or("pointer table overflow")?;
    let protected = vec![
        protected(target.layout.layer1.offset, table_len)?,
        protected(
            target.layout.sprites.low_or_contiguous_table().offset,
            table_len,
        )?,
        protected(policy.checksum_field, 4)?,
    ];
    let allocation_policy = AllocationPolicy {
        search: policy.search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected,
    };
    let expected = LoadedLevelSlot {
        number: target.level,
        layer1: file.layer1.clone(),
        sprites: file.sprites.clone(),
    };
    let options = LevelSaveOptions {
        layer1_allocation: allocation_policy.clone(),
        sprite_allocation: allocation_policy,
        previous_layer1,
        previous_sprites,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if let Some(manifest) = ownership {
        project.save_level_slot_with_checksum_and_reclamation(
            target.layout,
            &expected,
            sprite_lengths,
            &options,
            PayloadReclamation {
                checksum_field: policy.checksum_field,
                manifest,
            },
        )?;
    } else {
        project.save_level_slot_with_checksum(
            target.layout,
            &expected,
            sprite_lengths,
            policy.checksum_field,
            &options,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if reopened.load_level_slot(target.level, target.layout, sprite_lengths)? != expected {
        return Err("saved level failed semantic reopen verification".into());
    }
    Ok(snapshot)
}

fn layout(
    mapper: Mapper,
    layer1_table: usize,
    sprite_table: usize,
    expanded_sprites: bool,
) -> LevelRomLayout {
    let pointers = |offset| LevelPointerTable {
        offset,
        entries: LEVEL_SLOTS,
        stride: 3,
    };
    LevelRomLayout {
        mapper,
        layer1: pointers(layer1_table),
        sprites: pointers(sprite_table).into(),
        expanded_sprites,
    }
}

fn target(
    mapper: Mapper,
    level: usize,
    layer1_table: usize,
    sprite_table: usize,
    expanded_sprites: bool,
) -> LevelTarget {
    LevelTarget {
        level,
        layout: layout(mapper, layer1_table, sprite_table, expanded_sprites),
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
    use lm_level::{LevelObjectData, NativeSpriteStream};
    use lm_project::RatsOwnershipManifest;
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{compute_snes_checksum, pc_to_snes};

    fn file() -> NativeLevelFile {
        NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0x00, 0x20, 0x01, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn import_allocates_both_streams_repointers_checksums_and_reopens() {
        let mut bytes = vec![0xff; 0x10000];
        bytes[0x23..0x26].copy_from_slice(&[0x00, 0xa0, 0x80]);
        bytes[0x623..0x626].copy_from_slice(&[0x00, 0xb0, 0x80]);
        bytes[0x2000..0x2009].copy_from_slice(&[0, 0, 0, 0, 0, 1, 2, 3, 0xff]);
        bytes[0x3000..0x3002].copy_from_slice(&[0, 0xff]);
        bytes[0x7fdc..0x7fe0].fill(0);
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 0x620, false),
            &SpriteLengthTable::standard(),
            &file(),
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x4000..0x7800,
            },
            None,
        )
        .unwrap();
        let project = Project::new(RomImage::from_bytes(output.clone()).unwrap());
        let loaded = project
            .load_level_slot(
                1,
                layout(Mapper::LoRom, 0x20, 0x620, false),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert_eq!(loaded.layer1, file().layer1);
        assert_eq!(loaded.sprites, file().sprites);
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            &[
                checksum.complement.to_le_bytes(),
                checksum.checksum.to_le_bytes()
            ]
            .concat()
        );
    }

    #[test]
    fn format_mismatch_fails_before_mutation() {
        let bytes = vec![0xff; 0x10000];
        let mut incompatible = file();
        incompatible.sprites.expanded = true;
        assert!(
            import_image(
                bytes,
                target(Mapper::LoRom, 1, 0x20, 0x620, false),
                &SpriteLengthTable::standard(),
                &incompatible,
                ImportPolicy {
                    checksum_field: 0x7fdc,
                    search: 0x4000..0x7800,
                },
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn ownership_backed_import_reclaims_both_level_streams() {
        let mut bytes = vec![0xff; 0x10000];
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x2000..0x4000));
        let old_layer = allocator.allocate(&[0, 0, 0, 0, 0, 1, 2, 3, 0xff]).unwrap();
        let old_sprites = allocator.allocate(&[0, 0xff]).unwrap();
        for (pointer_offset, block) in [(0x23, &old_layer), (0x623, &old_sprites)] {
            let pointer = pc_to_snes(Mapper::LoRom, block.payload.start)
                .unwrap()
                .to_le_bytes();
            bytes[pointer_offset..pointer_offset + 3].copy_from_slice(&pointer[..3]);
        }
        bytes[0x7fdc..0x7fe0].fill(0);
        let manifest = RatsOwnershipManifest {
            owned: vec![old_layer.clone(), old_sprites.clone()],
            retained: Vec::new(),
        };
        let lengths = SpriteLengthTable::standard();
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 0x620, false),
            &lengths,
            &file(),
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x4000..0x7800,
            },
            Some(&manifest),
        )
        .unwrap();
        for block in &manifest.owned {
            assert!(output[block.full_range()].iter().all(|byte| *byte == 0xff));
        }
        let reopened = Project::new(RomImage::from_bytes(output.clone()).unwrap());
        let loaded = reopened
            .load_level_slot(1, layout(Mapper::LoRom, 0x20, 0x620, false), &lengths)
            .unwrap();
        assert_eq!(loaded.layer1, file().layer1);
        assert_eq!(loaded.sprites, file().sprites);
        assert_eq!(
            &output[0x7fdc..0x7fe0],
            &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
        );
    }
}
