use crate::args::ExAnimationTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_graphics::{CompactExAnimation, CompactExAnimationFile};
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, LevelPointerTable, PayloadReadPolicy,
    PayloadReclamation, Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const ANIMATION_SLOTS: usize = 0x200;

#[derive(Clone, Copy)]
struct ExAnimationTargetSpec {
    slot: usize,
    layout: ExAnimationRomLayout,
}

#[derive(Clone, Copy)]
struct ExAnimationTarget<'a> {
    spec: ExAnimationTargetSpec,
    modes: &'a [bool],
}

impl ExAnimationTargetSpec {
    fn interpreted(self, modes: &[bool]) -> ExAnimationTarget<'_> {
        ExAnimationTarget { spec: self, modes }
    }
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: ExAnimationTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ExAnimationTransferCommand::Export {
            rom,
            mapper,
            slot,
            pointer_table,
            maximum_records,
            maximum_encoded_len,
            size_modes,
            output,
        } => export(
            &rom,
            target(
                mapper,
                slot,
                pointer_table,
                maximum_records,
                maximum_encoded_len,
            ),
            &size_modes,
            &output,
        ),
        ExAnimationTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            slot,
            pointer_table,
            maximum_records,
            maximum_encoded_len,
            size_modes,
            animation_file,
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
                maximum_records,
                maximum_encoded_len,
            ),
            &size_modes,
            &animation_file,
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
    target: ExAnimationTargetSpec,
    size_modes: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let modes = crate::size_mode_file::read(size_modes)?;
    let target = target.interpreted(&modes);
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let animation = project.load_exanimation(target.spec.slot, target.spec.layout, target.modes)?;
    let source_slot =
        u16::try_from(target.spec.slot).map_err(|_| "ExAnimation slot exceeds file format")?;
    write_new(
        output,
        CompactExAnimationFile {
            source_slot,
            animation,
        }
        .encode(target.modes)?,
    )?;
    println!("exported-exanimation: {:#05x}", target.spec.slot);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    target: ExAnimationTargetSpec,
    size_modes: &Path,
    animation_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let modes = crate::size_mode_file::read(size_modes)?;
    let target = target.interpreted(&modes);
    let animation = CompactExAnimationFile::decode(
        &read_bounded(animation_file, CompactExAnimationFile::MAX_FILE_LEN)?,
        target.spec.layout.maximum_records,
        target.modes,
    )?
    .animation;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let snapshot = import_image(
        read_rom(input_rom)?,
        target,
        &animation,
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, snapshot)?;
    println!("imported-exanimation: {:#05x}", target.spec.slot);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: ExAnimationTarget<'_>,
    animation: &CompactExAnimation,
    policy: ImportPolicy,
    ownership: Option<&RatsOwnershipManifest>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut project = Project::new(RomImage::from_bytes(input)?);
    if policy.search.start >= policy.search.end || policy.search.end > project.rom.logical_len() {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    if target.modes.len() != 256 {
        return Err("ExAnimation size-mode table must contain exactly 256 entries".into());
    }
    let pointer = target
        .spec
        .layout
        .pointers
        .pointer_offset(target.spec.slot)?;
    let previous_block = project
        .load_payload(
            pointer,
            target.spec.layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: target.spec.layout.maximum_encoded_len,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let table_len = ANIMATION_SLOTS
        .checked_mul(3)
        .ok_or("pointer table overflow")?;
    let allocation_policy = AllocationPolicy {
        search: policy.search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected: vec![
            protected(target.spec.layout.pointers.offset, table_len)?,
            protected(policy.checksum_field, 4)?,
        ],
    };
    let options = ExAnimationSaveOptions {
        allocation: allocation_policy,
        previous_block,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    if let Some(manifest) = ownership {
        project.save_exanimation_with_checksum_and_reclamation(
            target.spec.slot,
            animation,
            target.spec.layout,
            target.modes,
            &options,
            PayloadReclamation {
                checksum_field: policy.checksum_field,
                manifest,
            },
        )?;
    } else {
        project.save_exanimation_with_checksum(
            target.spec.slot,
            animation,
            target.spec.layout,
            target.modes,
            policy.checksum_field,
            &options,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if &reopened.load_exanimation(target.spec.slot, target.spec.layout, target.modes)? != animation
    {
        return Err("saved ExAnimation failed semantic reopen verification".into());
    }
    Ok(snapshot)
}

fn layout(
    mapper: Mapper,
    pointer_table: usize,
    maximum_records: usize,
    maximum_encoded_len: usize,
) -> ExAnimationRomLayout {
    ExAnimationRomLayout {
        mapper,
        pointers: LevelPointerTable {
            offset: pointer_table,
            entries: ANIMATION_SLOTS,
            stride: 3,
        },
        maximum_records,
        maximum_encoded_len,
    }
}

fn target(
    mapper: Mapper,
    slot: usize,
    pointer_table: usize,
    maximum_records: usize,
    maximum_encoded_len: usize,
) -> ExAnimationTargetSpec {
    ExAnimationTargetSpec {
        slot,
        layout: layout(mapper, pointer_table, maximum_records, maximum_encoded_len),
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
    use lm_graphics::ExAnimationRecord;
    use lm_project::RatsOwnershipManifest;
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{compute_snes_checksum, pc_to_snes};

    fn animation() -> CompactExAnimation {
        CompactExAnimation {
            setting: 3,
            header_value: 0x1234_5678,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 2, 0x123, false, &[7, 8], false).unwrap()],
        }
    }

    #[test]
    fn import_allocates_repointers_checksums_and_reopens() {
        let modes = [false; 256];
        let old = CompactExAnimation {
            records: Vec::new(),
            ..animation()
        }
        .encode(&modes)
        .unwrap();
        let mut bytes = vec![0xff; 0x10000];
        bytes[0x23..0x26].copy_from_slice(&[0x00, 0xa0, 0x80]);
        bytes[0x2000..0x2000 + old.len()].copy_from_slice(&old);
        bytes[0x7fdc..0x7fe0].fill(0);
        let output = import_image(
            bytes,
            target(Mapper::LoRom, 1, 0x20, 32, 0x4000).interpreted(&modes),
            &animation(),
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
                .load_exanimation(1, layout(Mapper::LoRom, 0x20, 32, 0x4000), &modes)
                .unwrap(),
            animation()
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
    fn invalid_mode_count_fails_before_loading_payloads() {
        assert!(
            import_image(
                vec![0xff; 0x10000],
                target(Mapper::LoRom, 0, 0x20, 32, 0x4000).interpreted(&[false; 255]),
                &animation(),
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
    fn ownership_backed_import_reclaims_exact_exanimation_block() {
        let modes = [false; 256];
        let payload = CompactExAnimation {
            records: Vec::new(),
            ..animation()
        }
        .encode(&modes)
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
            target(Mapper::LoRom, 1, 0x20, 32, 0x4000).interpreted(&modes),
            &animation(),
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
