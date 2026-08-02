use crate::args::Command;
use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_level::{MwlFile, MwlSectionKind};
use lm_rats::scan;
use lm_rom::{RomImage, additive_checksum, detect_identity};

mod portable;
mod utility;

pub fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    if execute_delegated(&command)? {
        return Ok(());
    }
    match command {
        Command::Inspect(path) => inspect(&read_bounded(path, MAX_ROM_BYTES)?)?,
        Command::Rats(path) => list_rats(&read_bounded(path, MAX_ROM_BYTES)?)?,
        Command::RatsObserve { rom, output } => crate::rats_observation::execute(&rom, &output)?,
        Command::Mwl(path) => inspect_mwl(&read_bounded(path, MwlFile::MAX_FILE_BYTES)?)?,
        Command::MwlCorpus { root } => crate::mwl_corpus::audit(&root)?,
        Command::MwlNormalize { input, output } => crate::mwl_file::normalize(&input, &output)?,
        Command::MwlObserve { input, output } => crate::mwl_file::observe(&input, &output)?,
        Command::MwlPaletteTpl { input, output } => {
            crate::mwl_palette::export_tpl(&input, &output)?;
        }
        Command::Level {
            rom,
            mapper,
            number,
            layer1_table,
            sprite_table,
            expanded_sprites,
        } => crate::level::inspect(
            read_bounded(rom, MAX_ROM_BYTES)?,
            mapper,
            number,
            layer1_table,
            sprite_table,
            expanded_sprites,
        )?,
        Command::LevelSplitBank {
            rom,
            mapper,
            number,
            layer1_table,
            sprite_low_table,
            sprite_bank_table,
            expanded_sprites,
        } => crate::level::inspect_split_bank(
            read_bounded(rom, MAX_ROM_BYTES)?,
            mapper,
            number,
            layer1_table,
            sprite_low_table,
            sprite_bank_table,
            expanded_sprites,
        )?,
        Command::LevelLayer2 {
            rom,
            mapper,
            number,
            layer1_table,
            layer2_table,
            output,
        } => crate::level::export_layer2(
            read_bounded(rom, MAX_ROM_BYTES)?,
            mapper,
            number,
            layer1_table,
            layer2_table,
            &output,
        )?,
        Command::Map16 {
            rom,
            mapper,
            page,
            graphics_table,
            acts_like_table,
            observation,
        } => crate::map16::inspect(
            read_bounded(rom, MAX_ROM_BYTES)?,
            mapper,
            page,
            graphics_table,
            acts_like_table,
            observation.as_deref(),
        )?,
        Command::Map16Transfer(command) => crate::map16_transfer::execute(command)?,
        Command::GraphicsTransfer(command) => crate::graphics_transfer::execute(command)?,
        Command::GraphicsMigration(command) => crate::graphics_migration::execute(&command)?,
        Command::LevelTransfer(command) => crate::level_transfer::execute(command)?,
        Command::PaletteTransfer(command) => crate::palette_transfer::execute(command)?,
        Command::ExAnimationTransfer(command) => crate::exanimation_transfer::execute(command)?,
        Command::OverworldTransfer(command) => crate::overworld_transfer::execute(command)?,
        Command::ExpandedSettingsTransfer(command) => {
            crate::expanded_settings_transfer::execute(command)?;
        }
        Command::Asset(asset) => crate::assets::execute(asset)?,
        _ => unreachable!("command should have been handled by a delegated dispatcher"),
    }
    Ok(())
}

fn execute_delegated(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    if execute_native_command(command)? {
        return Ok(true);
    }
    if crate::expanded_settings_file::execute_command(command)? {
        return Ok(true);
    }
    if crate::native_assets_file::execute_command(command)? {
        return Ok(true);
    }
    if let Command::MwlTransferOptionalAssets {
        source,
        target,
        size_modes,
        maximum_records,
        output,
    } = command
    {
        crate::mwl_optional_transfer::execute(
            source,
            target,
            size_modes,
            *maximum_records,
            output,
        )?;
        return Ok(true);
    }
    if let Command::MwlEditOptionalAssets {
        input,
        size_modes,
        maximum_records,
        edits,
        output,
    } = command
    {
        crate::mwl_optional_edit::execute(input, size_modes, *maximum_records, edits, output)?;
        return Ok(true);
    }
    if let Command::MwlObserveOptionalAssets {
        input,
        size_modes,
        maximum_records,
        output,
    } = command
    {
        crate::mwl_optional_observation::execute(input, size_modes, *maximum_records, output)?;
        return Ok(true);
    }
    if let Command::MwlEditLayer3Settings {
        input,
        enabled,
        file,
        length_selector,
        offset_selector,
        output,
    } = command
    {
        crate::mwl_layer3_settings::execute(
            input,
            *enabled,
            *file,
            *length_selector,
            *offset_selector,
            output,
        )?;
        return Ok(true);
    }
    if let Command::MwlObserveLayer3Settings { input, output } = command {
        crate::mwl_layer3_settings::observe(input, output)?;
        return Ok(true);
    }
    if let Command::EditCompleteLevel {
        input,
        script,
        output,
    } = command
    {
        crate::level_bundle_edit::execute(input, script, output)?;
        return Ok(true);
    }
    Ok(portable::execute(command)?
        || execute_rats(command)?
        || execute_codec(command)?
        || execute_profile(command)?
        || execute_graphics_utility(command)?
        || utility::execute(command)?)
}

fn execute_native_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(
        crate::lunar_magic_metadata_native::execute_command(command)?
            || crate::exanimation_slot_options::execute_command(command)?
            || crate::secondary_exit_native::execute_command(command)?
            || crate::title_recording_native::execute_command(command)?
            || crate::title_tilemap_native::execute_command(command)?
            || crate::credits_tilemap_native::execute_command(command)?
            || crate::overworld_path_native::execute_command(command)?
            || crate::overworld_boss_sequence_native::execute_command(command)?
            || crate::overworld_event_native::execute_command(command)?
            || crate::overworld_event_number_native::execute_command(command)?
            || crate::overworld_transfer_observation::execute_command(command)?
            || crate::overworld_event_tilemap_native::execute_command(command)?
            || crate::overworld_special_event_native::execute_command(command)?
            || crate::overworld_message_native::execute_command(command)?
            || crate::overworld_warp_native::execute_command(command)?
            || crate::overworld_level_name_native::execute_command(command)?
            || crate::overworld_settings_native::execute_command(command)?
            || crate::shared_palette_native::execute_command(command)?
            || crate::overworld_start_native::execute_command(command)?,
    )
}

fn execute_rats(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::RatsManifest {
            input,
            normalized_output,
            observation,
        } => crate::rats_reclaim::inspect_manifest(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::RatsPlan {
            rom,
            manifest,
            fill,
        } => crate::rats_reclaim::plan(rom, manifest, *fill)?,
        Command::RatsReclaim {
            input,
            output,
            manifest,
            fill,
        } => crate::rats_reclaim::execute(input, output, manifest, *fill)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn execute_profile(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Profile { profile, rom } => crate::profile::inspect(profile, rom.as_deref())?,
        Command::ProfileExport {
            kind,
            rom,
            profile,
            slot,
            output,
        } => crate::profile_export::execute(*kind, rom, profile, *slot, output)?,
        Command::ProfileImport {
            kind,
            input_rom,
            output_rom,
            profile,
            slot,
            asset,
            search_start,
            search_end,
        } => crate::profile_import::execute(
            *kind,
            input_rom,
            output_rom,
            profile,
            *slot,
            asset,
            *search_start..*search_end,
        )?,
        Command::RevisionPatchInstall {
            input_rom,
            output_rom,
            profile,
            template,
            search_start,
            search_end,
            fill,
        } => crate::revision_patch_install::execute(
            input_rom,
            output_rom,
            profile,
            template,
            *search_start..*search_end,
            *fill,
        )?,
        Command::ExpandedSettingsInstall {
            input_rom,
            output_rom,
        } => crate::expanded_settings_install::execute(input_rom, output_rom)?,
        Command::Map16RuntimeInstall {
            input_rom,
            output_rom,
        } => crate::map16_runtime_install::execute(input_rom, output_rom)?,
        Command::Sprite19FixInstall {
            input_rom,
            output_rom,
        } => crate::sprite19_fix_install::execute(input_rom, output_rom)?,
        Command::SupportPatchBInstall {
            input_rom,
            output_rom,
        } => crate::support_patch_b_install::execute(input_rom, output_rom)?,
        Command::SmwMap16CompleteExport {
            rom,
            template,
            output,
        } => crate::native_map16_complete::export(rom, template.as_deref(), output)?,
        Command::SmwMap16CompleteImport {
            input_rom,
            map16,
            output_rom,
        } => crate::native_map16_complete::import(input_rom, map16, output_rom)?,
        Command::Layer3Install {
            input_rom,
            output_rom,
        } => crate::layer3_install::execute(input_rom, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn execute_codec(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Codec {
            operation,
            input,
            output,
        } => crate::codec::transform(*operation, input, output)?,
        Command::CodecSizedRleDecode {
            input,
            output,
            expected_len,
        } => crate::codec::decode_sized(input, output, *expected_len)?,
        Command::CodecObserve {
            kind,
            input,
            output_bound,
            observation,
        } => crate::codec_observation::execute(*kind, input, *output_bound, observation)?,
        Command::Planar {
            operation,
            bits_per_pixel,
            input,
            output,
        } => crate::planar::execute(*operation, *bits_per_pixel, input, output)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn execute_graphics_utility(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::QuantizeRgb24 {
            input,
            maximum_colors,
            palette_output,
            indices_output,
        } => {
            crate::quantize_rgb24::execute(input, *maximum_colors, palette_output, indices_output)?;
        }
        Command::ImportIndexedMap16 {
            indices,
            graphics,
            occupancy,
            palette_row,
            acts_like,
            source_page,
            graphics_output,
            occupancy_output,
            page_output,
        } => {
            crate::indexed_map16_import::execute(
                crate::indexed_map16_import::IndexedMap16Import {
                    indices,
                    graphics,
                    occupancy,
                    palette_row: *palette_row,
                    acts_like: *acts_like,
                    source_page: *source_page,
                    graphics_output,
                    occupancy_output,
                    page_output,
                },
            )?;
        }
        Command::ImportRgbMap16(command) => crate::rgb_map16_import::execute(command)?,
        Command::ImportRgbaMap16(command) => crate::rgba_map16_import::execute(command)?,
        Command::ImportPngMap16(command) => crate::png_map16_import::execute(command)?,
        Command::EditExAnimationFrames {
            input,
            size_modes,
            maximum_records,
            record,
            edits,
            output,
        } => crate::exanimation_frames::execute(
            input,
            size_modes,
            *maximum_records,
            *record,
            edits,
            output,
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn inspect_mwl(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let file = MwlFile::decode(bytes)?;
    println!("version: {:#06x}", file.version);
    println!("flags: {:#010x}", file.flags);
    for kind in [
        MwlSectionKind::LevelHeader,
        MwlSectionKind::Layer1,
        MwlSectionKind::Layer2,
        MwlSectionKind::Sprites,
        MwlSectionKind::Palette,
        MwlSectionKind::SecondaryExits,
        MwlSectionKind::ExAnimation,
        MwlSectionKind::ExpandedHeader,
    ] {
        println!("section-{kind:?}: {:#x}", file.section(kind).len());
    }
    if !file.section(MwlSectionKind::Layer2).is_empty() {
        let layer2 = file.layer2_section()?;
        println!(
            "layer2-descriptor: {:#010x} (active-bank: {}, compressed: {}, split-planes: {})",
            layer2.descriptor.raw(),
            layer2.descriptor.active_bank(),
            layer2.descriptor.uses_compressed_tilemap(),
            layer2.descriptor.uses_split_planes()
        );
        println!("layer2-source-address: {:#010x}", layer2.source_address);
    }
    Ok(())
}

fn inspect(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let rom = RomImage::from_bytes(bytes.to_vec())?;
    println!("file-size: {:#x}", rom.as_file_bytes().len());
    println!("logical-size: {:#x}", rom.logical_len());
    println!("copier-header: {:?}", rom.copier_header());
    println!(
        "additive-checksum: {:#06x}",
        additive_checksum(rom.logical_bytes())
    );
    println!("rats-blocks: {}", scan(rom.logical_bytes()).len());
    match detect_identity(&rom) {
        Ok(identity) => {
            println!("game: {:?}", identity.game);
            println!("mapper: {:?}", identity.mapper);
            println!("region: {:?}", identity.region);
            println!("revision: {}", identity.revision);
            println!("checksum-matches: {}", identity.checksum_matches());
        }
        Err(error) => println!("identity: {error}"),
    }
    Ok(())
}

fn list_rats(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let rom = RomImage::from_bytes(bytes.to_vec())?;
    for block in scan(rom.logical_bytes()) {
        println!(
            "header={:#x} payload={:#x}..{:#x} len={:#x}",
            block.header_offset,
            block.payload.start,
            block.payload.end,
            block.payload.len()
        );
    }
    Ok(())
}
