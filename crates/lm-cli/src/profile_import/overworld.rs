use super::common::ImportContext;
use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_project::{
    CompleteOverworldFile, CompleteOverworldRomLayout, CompleteOverworldSaveOptions,
    EndpointSaveOptions, EventRevealSaveOptions, ExAnimationSaveOptions, MessageSaveOptions,
    OverworldSaveOptions, PaletteSaveOptions, PayloadReadPolicy, Project, SpriteSaveOptions,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::RomImage;
use std::path::Path;

pub(super) fn import(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context.profile.overworld;
    let modes = context.profile.exanimation_double_size_modes;
    let file = CompleteOverworldFile::decode(
        &read_bounded(asset, CompleteOverworldFile::MAX_FILE_LEN)?,
        layout.animation.maximum_records,
        &modes,
    )?;
    if file.shape != context.profile.overworld_shape {
        return Err("overworld file shape does not match the profile".into());
    }
    let previous = load_previous(&context.project, slot, &layout)?;
    let allocation = context.allocation()?;
    context.project.save_complete_overworld_with_checksum(
        slot,
        &file.data,
        layout,
        &save_options(allocation, previous),
        &modes,
        context.checksum_field,
    )?;
    let snapshot = context.project.save_snapshot();
    if Project::new(RomImage::from_bytes(snapshot.clone())?)
        .load_complete_overworld(slot, layout, &modes)?
        != file.data
    {
        return Err("profile-imported overworld failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-overworld: {slot:#05x}");
    Ok(())
}

#[derive(Default)]
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

fn load_previous(
    project: &Project,
    slot: usize,
    layout: &CompleteOverworldRomLayout,
) -> Result<PreviousBlocks, Box<dyn std::error::Error>> {
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
        layer1: fixed(
            layout.layers.layer1.pointer_offset(slot)?,
            product(&[layout.layers.width, layout.layers.height, 2])?,
        )?,
        layer2: fixed(
            layout.layers.layer2.pointer_offset(slot)?,
            product(&[layout.layers.width, layout.layers.height, 2])?,
        )?,
        event_sources: fixed(
            layout.event_reveals.sources.pointer_offset(slot)?,
            product(&[layout.event_reveals.entries_per_slot, 2])?,
        )?,
        event_destinations: fixed(
            layout.event_reveals.destinations.pointer_offset(slot)?,
            product(&[layout.event_reveals.entries_per_slot, 2])?,
        )?,
        endpoints: fixed(
            layout.endpoints.pointers.pointer_offset(slot)?,
            product(&[layout.endpoints.endpoints_per_slot, 5])?,
        )?,
        messages: fixed(
            layout.messages.pointers.pointer_offset(slot)?,
            product(&[layout.messages.messages_per_slot, 144])?,
        )?,
        sprites: fixed(
            layout.sprites.pointers.pointer_offset(slot)?,
            product(&[layout.sprites.sprites_per_slot, layout.sprites.record_len])?,
        )?,
        palette: fixed(
            layout.palette.pointers.pointer_offset(slot)?,
            product(&[layout.palette.colors_per_palette, 2])?,
        )?,
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
        endpoints: single_endpoint(allocation.clone(), previous.endpoints),
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

fn single_endpoint(
    allocation: AllocationPolicy,
    previous_block: Option<RatsBlock>,
) -> EndpointSaveOptions {
    EndpointSaveOptions {
        allocation,
        previous_block,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn product(values: &[usize]) -> Result<usize, Box<dyn std::error::Error>> {
    values
        .iter()
        .try_fold(1usize, |a, b| a.checked_mul(*b))
        .ok_or_else(|| "overworld payload size overflow".into())
}
