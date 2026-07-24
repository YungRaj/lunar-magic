use crate::args::OverworldTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use crate::overworld_layout::OverworldLayoutDescriptor;
use lm_project::{
    CompleteOverworldData, CompleteOverworldFile, CompleteOverworldRomLayout,
    CompleteOverworldSaveOptions, EndpointSaveOptions, EventRevealSaveOptions,
    ExAnimationSaveOptions, MessageSaveOptions, OverworldSaveOptions, PaletteSaveOptions,
    PayloadReadPolicy, PayloadReclamation, Project, RatsOwnershipManifest, SpriteSaveOptions,
};
use lm_rats::{AllocationPolicy, ProtectedRange, RatsBlock};
use lm_rom::{Mapper, RomImage};
use std::ops::Range;
use std::path::Path;

const OVERWORLD_SLOTS: usize = 0x200;

#[derive(Clone, Copy)]
struct TargetFiles<'a> {
    mapper: Mapper,
    slot: usize,
    layout: &'a Path,
    size_modes: &'a Path,
}

#[derive(Clone, Copy)]
struct OverworldTargetSpec {
    slot: usize,
    descriptor: OverworldLayoutDescriptor,
    mapper: Mapper,
}

#[derive(Clone, Copy)]
struct OverworldTarget<'a> {
    spec: OverworldTargetSpec,
    modes: &'a [bool],
}

impl OverworldTargetSpec {
    fn interpreted(self, modes: &[bool]) -> OverworldTarget<'_> {
        OverworldTarget { spec: self, modes }
    }

    fn layout(self) -> CompleteOverworldRomLayout {
        self.descriptor.rom_layout(self.mapper)
    }
}

#[derive(Clone, Copy)]
struct ImportDocument<'a> {
    data: &'a CompleteOverworldData,
    shape: lm_project::CompleteOverworldShape,
}

struct ImportPolicy {
    checksum_field: usize,
    search: Range<usize>,
}

pub fn execute(command: OverworldTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        OverworldTransferCommand::Export {
            rom,
            mapper,
            slot,
            layout,
            size_modes,
            output,
        } => export(
            &rom,
            TargetFiles {
                mapper,
                slot,
                layout: &layout,
                size_modes: &size_modes,
            },
            &output,
        ),
        OverworldTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            slot,
            layout,
            size_modes,
            overworld_file,
            checksum_field,
            search_start,
            search_end,
            ownership_manifest,
        } => import(
            &input_rom,
            &output_rom,
            TargetFiles {
                mapper,
                slot,
                layout: &layout,
                size_modes: &size_modes,
            },
            &overworld_file,
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
    files: TargetFiles<'_>,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let descriptor = load_layout(files.layout)?;
    let modes = crate::size_mode_file::read(files.size_modes)?;
    let target = OverworldTargetSpec {
        slot: files.slot,
        descriptor,
        mapper: files.mapper,
    }
    .interpreted(&modes);
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let data =
        project.load_complete_overworld(target.spec.slot, target.spec.layout(), target.modes)?;
    let source_slot =
        u16::try_from(target.spec.slot).map_err(|_| "overworld slot exceeds file format")?;
    write_new(
        output,
        CompleteOverworldFile {
            source_slot,
            shape: target.spec.descriptor.shape(),
            data,
        }
        .encode(target.modes)?,
    )?;
    println!("exported-overworld: {:#05x}", target.spec.slot);
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    files: TargetFiles<'_>,
    overworld_file: &Path,
    policy: ImportPolicy,
    ownership_manifest: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    let descriptor = load_layout(files.layout)?;
    let modes = crate::size_mode_file::read(files.size_modes)?;
    let target = OverworldTargetSpec {
        slot: files.slot,
        descriptor,
        mapper: files.mapper,
    }
    .interpreted(&modes);
    let file = CompleteOverworldFile::decode(
        &read_bounded(overworld_file, CompleteOverworldFile::MAX_FILE_LEN)?,
        target.spec.descriptor.animation_max_records,
        target.modes,
    )?;
    let ownership = crate::owned_relocation::read_optional(ownership_manifest)?;
    let snapshot = import_image(
        read_rom(input_rom)?,
        target,
        ImportDocument {
            data: &file.data,
            shape: file.shape,
        },
        policy,
        ownership.as_ref(),
    )?;
    write_new(output_rom, snapshot)?;
    println!("imported-overworld: {:#05x}", target.spec.slot);
    println!("source-slot: {:#05x}", file.source_slot);
    println!("output: {}", output_rom.display());
    Ok(())
}

fn import_image(
    input: Vec<u8>,
    target: OverworldTarget<'_>,
    document: ImportDocument<'_>,
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
    if document.shape != target.spec.descriptor.shape() {
        return Err(format!(
            "overworld file shape {:?} does not match target layout {:?}",
            document.shape,
            target.spec.descriptor.shape()
        )
        .into());
    }
    let layout = target.spec.layout();
    let previous = load_previous_blocks(&project, target.spec.slot, &layout)?;
    let table_len = OVERWORLD_SLOTS
        .checked_mul(3)
        .ok_or("pointer table overflow")?;
    let mut protected = target
        .spec
        .descriptor
        .pointer_tables()
        .into_iter()
        .map(|start| protected_range(start, table_len))
        .collect::<Result<Vec<_>, _>>()?;
    protected.push(protected_range(policy.checksum_field, 4)?);
    let allocation = AllocationPolicy {
        search: policy.search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected,
    };
    let options = save_options(allocation, previous);
    if let Some(manifest) = ownership {
        project.save_complete_overworld_with_checksum_and_reclamation(
            target.spec.slot,
            document.data,
            layout,
            &options,
            target.modes,
            PayloadReclamation {
                checksum_field: policy.checksum_field,
                manifest,
            },
        )?;
    } else {
        project.save_complete_overworld_with_checksum(
            target.spec.slot,
            document.data,
            layout,
            &options,
            target.modes,
            policy.checksum_field,
        )?;
    }
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if &reopened.load_complete_overworld(target.spec.slot, layout, target.modes)? != document.data {
        return Err("saved complete overworld failed semantic reopen verification".into());
    }
    Ok(snapshot)
}

#[derive(Clone, Debug, Default)]
struct PreviousBlocks {
    layer1: Option<RatsBlock>,
    layer2: Option<RatsBlock>,
    event_sources: Option<RatsBlock>,
    event_destinations: Option<RatsBlock>,
    endpoints: Option<RatsBlock>,
    messages: Option<RatsBlock>,
    sprites: Option<RatsBlock>,
    palette: Option<RatsBlock>,
    animation: Option<RatsBlock>,
}

fn load_previous_blocks(
    project: &Project,
    slot: usize,
    layout: &CompleteOverworldRomLayout,
) -> Result<PreviousBlocks, Box<dyn std::error::Error>> {
    let layer_len = checked_product(&[layout.layers.width, layout.layers.height, 2])?;
    let event_len = checked_product(&[layout.event_reveals.entries_per_slot, 2])?;
    let endpoint_len = checked_product(&[layout.endpoints.endpoints_per_slot, 5])?;
    let message_len = checked_product(&[layout.messages.messages_per_slot, 144])?;
    let sprite_len =
        checked_product(&[layout.sprites.sprites_per_slot, layout.sprites.record_len])?;
    let palette_len = checked_product(&[layout.palette.colors_per_palette, 2])?;
    let fixed = |pointer: usize, len: usize| {
        Ok::<_, Box<dyn std::error::Error>>(
            project
                .load_payload(
                    pointer,
                    layout.layers.mapper,
                    &PayloadReadPolicy::TaggedOrFixed { len },
                )?
                .block,
        )
    };
    Ok(PreviousBlocks {
        layer1: fixed(layout.layers.layer1.pointer_offset(slot)?, layer_len)?,
        layer2: fixed(layout.layers.layer2.pointer_offset(slot)?, layer_len)?,
        event_sources: fixed(
            layout.event_reveals.sources.pointer_offset(slot)?,
            event_len,
        )?,
        event_destinations: fixed(
            layout.event_reveals.destinations.pointer_offset(slot)?,
            event_len,
        )?,
        endpoints: fixed(
            layout.endpoints.pointers.pointer_offset(slot)?,
            endpoint_len,
        )?,
        messages: fixed(layout.messages.pointers.pointer_offset(slot)?, message_len)?,
        sprites: fixed(layout.sprites.pointers.pointer_offset(slot)?, sprite_len)?,
        palette: fixed(layout.palette.pointers.pointer_offset(slot)?, palette_len)?,
        animation: project
            .load_payload(
                layout.animation.pointers.pointer_offset(slot)?,
                layout.animation.mapper,
                &PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: layout.animation.maximum_encoded_len,
                    bank_size: Some(0x8000),
                },
            )?
            .block,
    })
}

fn save_options(
    allocation: AllocationPolicy,
    previous: PreviousBlocks,
) -> CompleteOverworldSaveOptions {
    CompleteOverworldSaveOptions {
        layers: OverworldSaveOptions {
            layer1_allocation: allocation.clone(),
            layer2_allocation: allocation.clone(),
            previous_layer1: previous.layer1,
            previous_layer2: previous.layer2,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        event_reveals: EventRevealSaveOptions {
            source_allocation: allocation.clone(),
            destination_allocation: allocation.clone(),
            previous_sources: previous.event_sources,
            previous_destinations: previous.event_destinations,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        endpoints: EndpointSaveOptions {
            allocation: allocation.clone(),
            previous_block: previous.endpoints,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        messages: MessageSaveOptions {
            allocation: allocation.clone(),
            previous_block: previous.messages,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        sprites: SpriteSaveOptions {
            allocation: allocation.clone(),
            previous_block: previous.sprites,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: previous.palette,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        animation: ExAnimationSaveOptions {
            allocation,
            previous_block: previous.animation,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

fn checked_product(values: &[usize]) -> Result<usize, Box<dyn std::error::Error>> {
    values
        .iter()
        .try_fold(1_usize, |product, value| product.checked_mul(*value))
        .ok_or_else(|| "overworld payload size overflow".into())
}

fn load_layout(path: &Path) -> Result<OverworldLayoutDescriptor, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path, OverworldLayoutDescriptor::MAX_FILE_LEN)?;
    Ok(OverworldLayoutDescriptor::parse(std::str::from_utf8(
        &bytes,
    )?)?)
}

fn protected_range(start: usize, len: usize) -> Result<ProtectedRange, Box<dyn std::error::Error>> {
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
#[path = "overworld_transfer_tests.rs"]
mod tests;
