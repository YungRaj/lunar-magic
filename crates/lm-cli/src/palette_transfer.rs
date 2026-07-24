use crate::args::PaletteTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_graphics::{Palette, PaletteInterchangeFile};
use lm_project::{
    LevelPointerTable, PaletteRomLayout, PaletteSaveOptions, PayloadReadPolicy, PayloadReclamation,
    Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const PALETTE_SLOTS: usize = 0x200;

#[derive(Clone, Copy)]
struct PaletteTarget {
    number: usize,
    layout: PaletteRomLayout,
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: PaletteTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        PaletteTransferCommand::Export {
            rom,
            mapper,
            palette,
            pointer_table,
            colors,
            output,
        } => export(
            &rom,
            target(mapper, palette, pointer_table, colors),
            &output,
        ),
        PaletteTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            palette,
            pointer_table,
            colors,
            palette_file,
            checksum_field,
            search_start,
            search_end,
            ownership_manifest,
        } => import(
            &input_rom,
            &output_rom,
            target(mapper, palette, pointer_table, colors),
            &palette_file,
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
    target: PaletteTarget,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let palette = project.load_palette(target.number, target.layout)?;
    let source_palette =
        u16::try_from(target.number).map_err(|_| "palette number exceeds file format")?;
    write_new(
        output,
        PaletteInterchangeFile {
            source_palette,
            palette,
        }
        .encode()?,
    )?;
    println!("exported-palette: {:#05x}", target.number);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    target: PaletteTarget,
    palette_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        palette_file,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?
    .palette;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let snapshot = import_image(
        read_rom(input_rom)?,
        target,
        &palette,
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, snapshot)?;
    println!("imported-palette: {:#05x}", target.number);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: PaletteTarget,
    palette: &Palette,
    policy: ImportPolicy,
    ownership: Option<&RatsOwnershipManifest>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut project = Project::new(RomImage::from_bytes(input)?);
    if policy.search.start >= policy.search.end || policy.search.end > project.rom.logical_len() {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    if palette.colors.len() != target.layout.colors_per_palette {
        return Err(format!(
            "palette file contains {} colors but target layout requires {}",
            palette.colors.len(),
            target.layout.colors_per_palette,
        )
        .into());
    }
    let payload_len = target
        .layout
        .colors_per_palette
        .checked_mul(2)
        .ok_or("palette size overflow")?;
    let pointer = target.layout.pointers.pointer_offset(target.number)?;
    let previous_block = project
        .load_payload(
            pointer,
            target.layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: payload_len },
        )?
        .block;
    let table_len = PALETTE_SLOTS
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
    let options = PaletteSaveOptions {
        allocation: allocation_policy,
        previous_block,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if let Some(manifest) = ownership {
        project.save_palette_with_checksum_and_reclamation(
            target.number,
            palette,
            target.layout,
            &options,
            PayloadReclamation {
                checksum_field: policy.checksum_field,
                manifest,
            },
        )?;
    } else {
        project.save_palette_with_checksum(
            target.number,
            palette,
            target.layout,
            policy.checksum_field,
            &options,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if &reopened.load_palette(target.number, target.layout)? != palette {
        return Err("saved palette failed semantic reopen verification".into());
    }
    Ok(snapshot)
}

fn layout(mapper: Mapper, pointer_table: usize, colors: usize) -> PaletteRomLayout {
    PaletteRomLayout {
        mapper,
        pointers: LevelPointerTable {
            offset: pointer_table,
            entries: PALETTE_SLOTS,
            stride: 3,
        },
        colors_per_palette: colors,
    }
}

fn target(mapper: Mapper, number: usize, pointer_table: usize, colors: usize) -> PaletteTarget {
    PaletteTarget {
        number,
        layout: layout(mapper, pointer_table, colors),
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
    use lm_graphics::Bgr555;
    use lm_project::RatsOwnershipManifest;
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{compute_snes_checksum, pc_to_snes};

    fn palette() -> Palette {
        Palette {
            colors: (0_u16..32).map(|value| Bgr555(value * 17)).collect(),
        }
    }

    #[test]
    fn import_allocates_repointers_checksums_and_reopens() {
        let mut bytes = vec![0xff; 0x10000];
        bytes[0x23..0x26].copy_from_slice(&[0x00, 0xa0, 0x80]);
        bytes[0x2000..0x2040].fill(0);
        bytes[0x7fdc..0x7fe0].fill(0);
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 32),
            &palette(),
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x3000..0x7800,
            },
            None,
        )
        .unwrap();
        let project = Project::new(RomImage::from_bytes(output).unwrap());
        assert_eq!(
            project
                .load_palette(1, layout(Mapper::LoRom, 0x20, 32))
                .unwrap(),
            palette()
        );
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
    fn target_shape_mismatch_is_rejected() {
        assert!(
            import_image(
                vec![0xff; 0x10000],
                target(Mapper::LoRom, 0, 0x20, 16),
                &palette(),
                ImportPolicy {
                    checksum_field: 0x7fdc,
                    search: 0x3000..0x7800,
                },
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn ownership_backed_import_reclaims_exact_palette_block() {
        let payload = Palette {
            colors: vec![Bgr555(0); 32],
        }
        .encode_snes()
        .unwrap();
        let mut bytes = vec![0xff; 0x10000];
        let old = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x1000..0x2000))
            .allocate(&payload)
            .unwrap();
        let pointer = pc_to_snes(Mapper::LoRom, old.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x23..0x26].copy_from_slice(&pointer[..3]);
        bytes[0x7fdc..0x7fe0].fill(0);
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 32),
            &palette(),
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x3000..0x7800,
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
}
