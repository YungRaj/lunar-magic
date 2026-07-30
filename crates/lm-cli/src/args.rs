use crate::arg_assets::parse_asset_command;
use crate::arg_codec_commands::{parse_codec_observation, parse_planar};
use crate::arg_image_imports::{
    parse_indexed_map16_import, parse_png_map16_import, parse_rgb_map16_import,
    parse_rgba_map16_import,
};
use crate::arg_mwl_commands::parse_mwl_command;
use crate::arg_oracle_commands::{parse_oracle_capture, parse_oracle_verification};
use crate::arg_rom_commands::{parse_rats_command, parse_rom_command};
use crate::arg_transfers::{
    parse_exanimation_transfer, parse_expanded_settings_transfer, parse_graphics_transfer,
    parse_level_transfer, parse_map16_transfer, parse_overworld_transfer, parse_palette_transfer,
};
use crate::arg_values::{
    ArgsError, parse_codec_operation, parse_direction, parse_hex_bytes, parse_mapper, parse_number,
    parse_profile_export_kind, parse_profile_import_kind, parse_sprite_format,
};
pub use crate::command_types::*;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn parse() -> Result<Command, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    parse_from(&args)
}

fn parse_from(args: &[OsString]) -> Result<Command, Box<dyn std::error::Error>> {
    let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
    if let Some(command) = parse_special_command(args, &text)? {
        return Ok(command);
    }
    if let Some(command) = parse_mwl_command(args, &text)? {
        return Ok(command);
    }
    if let Some(command) = parse_level_inspection_command(args, &text)? {
        return Ok(command);
    }
    match text.as_slice() {
        [command, path] if command == "inspect" => Ok(Command::Inspect(PathBuf::from(&args[1]))),
        [command, _, mapper, page, graphics_table, acts_like_table] if command == "map16" => {
            Ok(Command::Map16 {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                page: usize::try_from(parse_number(page)?)?,
                graphics_table: usize::try_from(parse_number(graphics_table)?)?,
                acts_like_table: usize::try_from(parse_number(acts_like_table)?)?,
                observation: None,
            })
        }
        [command, _, mapper, page, graphics_table, acts_like_table, _]
            if command == "map16" =>
        {
            Ok(Command::Map16 {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                page: usize::try_from(parse_number(page)?)?,
                graphics_table: usize::try_from(parse_number(graphics_table)?)?,
                acts_like_table: usize::try_from(parse_number(acts_like_table)?)?,
                observation: Some(PathBuf::from(&args[6])),
            })
        }
        [command, mapper, direction, value] if command == "address" => Ok(Command::Address {
            mapper: parse_mapper(mapper)?,
            direction: parse_direction(direction)?,
            value: parse_number(value)?,
        }),
        [command, operation, _, _] if command == "codec" => Ok(Command::Codec {
            operation: parse_codec_operation(operation)?,
            input: PathBuf::from(&args[2]),
            output: PathBuf::from(&args[3]),
        }),
        [command, operation, _, _, expected] if command == "codec" && operation == "rle-sized-decode" => {
            Ok(Command::CodecSizedRleDecode {
                input: PathBuf::from(&args[2]),
                output: PathBuf::from(&args[3]),
                expected_len: usize::try_from(parse_number(expected)?)?,
            })
        }
        [command, kind, _, output_bound, _] if command == "codec-observe" => {
            parse_codec_observation(args, kind, output_bound)
        }
        [command, operation, bits_per_pixel, _, _] if command == "planar" =>
            parse_planar(args, operation, bits_per_pixel),
        [command, _, colors, _, _] if command == "quantize-rgb24" => Ok(Command::QuantizeRgb24 {
            input: PathBuf::from(&args[1]),
            maximum_colors: usize::try_from(parse_number(colors)?)?,
            palette_output: PathBuf::from(&args[3]),
            indices_output: PathBuf::from(&args[4]),
        }),
        [command, _, _] if command == "diff" => Ok(Command::Diff { left: PathBuf::from(&args[1]), right: PathBuf::from(&args[2]) }),
        [command, _, _, field_offset] if command == "checksum" => Ok(Command::Checksum {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
            field_offset: usize::try_from(parse_number(field_offset)?)?,
        }),
        [command, _, _] if command == "checksum-auto" => Ok(Command::ChecksumAuto {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _, _, offset, bytes] if command == "patch" => Ok(Command::Patch {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
            offset: usize::try_from(parse_number(offset)?)?,
            bytes: parse_hex_bytes(bytes)?,
        }),
        [command] if command == "graphics-recompress" => Err(ArgsError(
            "usage: graphics-recompress INPUT_ROM OUTPUT_ROM MAPPER POINTER_TABLE ENTRIES MAX_COMPRESSED MAX_DECOMPRESSED lz2|lz3 lz2|lz3 CHECKSUM_FIELD SEARCH_START SEARCH_END"
                .into(),
        )
        .into()),
        _ => Err(ArgsError("usage: lm-cli <inspect ROM|rats ROM|rats-manifest INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|rats-plan ROM MANIFEST FILL|rats-reclaim INPUT_ROM OUTPUT_ROM MANIFEST FILL|mwl FILE|mwl-corpus DIRECTORY|mwl-normalize INPUT OUTPUT|mwl-observe INPUT OUTPUT|mwl-palette-tpl INPUT OUTPUT|mwl-transfer-optional-assets SOURCE TARGET SIZE_MODES MAX_RECORDS OUTPUT|mwl-edit-optional-assets INPUT SIZE_MODES MAX_RECORDS EDITS OUTPUT|lm16-map16-file INPUT [NORMALIZED_OUTPUT]|level-bundle INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|level-bundle-edit INPUT SCRIPT OUTPUT|native-level-file INPUT SPRITE_LENGTHS|standard [NORMALIZED_OUTPUT [OBSERVATION]]|appearance-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|overworld-appearance-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|animation-frame-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|layer3-plane-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|layer-tilemap-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|credits-tilemap-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|overworld-event-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|layer3-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|custom-object-library DATA.mw0 DESCRIPTIONS.mw0t [NORMALIZED_DATA NORMALIZED_DESCRIPTIONS [OBSERVATION]]|map16-page-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|map16-set-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|exanimation-file INPUT SIZE_MODES MAX_RECORDS [NORMALIZED_OUTPUT [OBSERVATION]]|graphics-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|palette-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|overworld-file INPUT SIZE_MODES MAX_RECORDS [NORMALIZED_OUTPUT [OBSERVATION]]|render-map16-page GRAPHICS PALETTE PAGE OUTPUT_PNG|render-graphics GRAPHICS PALETTE PALETTE_ROW COLUMNS OUTPUT_PNG|render-palette PALETTE COLUMNS CELL_SIZE OUTPUT_PNG|render-level LEVEL MAP16_SET GRAPHICS PALETTE L1_WIDTH L1_HEIGHT L2_WIDTH L2_HEIGHT OUTPUT_PNG|overworld-path INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|overworld-metadata INPUT [NORMALIZED_OUTPUT [OBSERVATION]]|level ROM MAPPER NUMBER LAYER1_TABLE SPRITE_TABLE legacy|expanded|level-split-bank ROM MAPPER NUMBER LAYER1_TABLE SPRITE_LOW_TABLE SPRITE_BANK_TABLE legacy|expanded|level-layer2 ROM MAPPER NUMBER LAYER1_TABLE LAYER2_TABLE OUTPUT|level-export ROM MAPPER NUMBER LAYER1_TABLE SPRITE_TABLE legacy|expanded SPRITE_LENGTHS|standard OUTPUT|level-import INPUT_ROM OUTPUT_ROM MAPPER NUMBER LAYER1_TABLE SPRITE_TABLE legacy|expanded SPRITE_LENGTHS|standard LEVEL_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|expanded-settings-export ROM MAPPER SLOT TABLE_OFFSET ENTRIES STRIDE OUTPUT|expanded-settings-import INPUT_ROM OUTPUT_ROM MAPPER SLOT TABLE_OFFSET ENTRIES STRIDE RECORD CHECKSUM_FIELD|map16 ROM MAPPER PAGE GRAPHICS_TABLE ACTS_TABLE [OBSERVATION]|map16-export ROM MAPPER PAGE GRAPHICS_TABLE ACTS_TABLE OUTPUT|map16-import INPUT_ROM OUTPUT_ROM MAPPER PAGE GRAPHICS_TABLE ACTS_TABLE PAGE_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|graphics ROM MAPPER FILE POINTER_TABLE MAX_COMPRESSED MAX_DECOMPRESSED [OBSERVATION]|graphics-export ROM MAPPER SLOT POINTER_TABLE MAX_COMPRESSED MAX_DECOMPRESSED OUTPUT|graphics-import INPUT_ROM OUTPUT_ROM MAPPER SLOT POINTER_TABLE MAX_COMPRESSED MAX_DECOMPRESSED GRAPHICS_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|palette ROM MAPPER INDEX POINTER_TABLE COLORS [OBSERVATION]|palette-export ROM MAPPER INDEX POINTER_TABLE COLORS OUTPUT|palette-import INPUT_ROM OUTPUT_ROM MAPPER INDEX POINTER_TABLE COLORS PALETTE_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|exanimation ROM MAPPER SLOT POINTER_TABLE MAX_RECORDS MAX_ENCODED SIZE_MODES [OBSERVATION]|exanimation-export ROM MAPPER SLOT POINTER_TABLE MAX_RECORDS MAX_DECOMPRESSED SIZE_MODES OUTPUT|exanimation-import INPUT_ROM OUTPUT_ROM MAPPER SLOT POINTER_TABLE MAX_RECORDS MAX_ENCODED SIZE_MODES ANIMATION_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|overworld-export ROM MAPPER SLOT LAYOUT SIZE_MODES OUTPUT|overworld-import INPUT_ROM OUTPUT_ROM MAPPER SLOT LAYOUT SIZE_MODES OVERWORLD_FILE CHECKSUM_FIELD SEARCH_START SEARCH_END|overworld-messages ROM MAPPER SLOT POINTER_TABLE COUNT [OBSERVATION]|overworld-sprites ROM MAPPER SLOT POINTER_TABLE COUNT RECORD_LEN [OBSERVATION]|address MAPPER DIRECTION VALUE|codec OP INPUT OUTPUT|codec-observe lz2|lz3|rle-terminated|rle-sized INPUT OUTPUT_BOUND OBSERVATION|diff LEFT RIGHT|oracle-capture CASE VERSION OPERATION BEFORE AFTER DECODED_BEFORE DECODED_AFTER none|changed-rats OUTPUT [KEY=VALUE ...]|oracle-verify MANIFEST BEFORE AFTER [DECODED_BEFORE DECODED_AFTER]|checksum INPUT OUTPUT FIELD_OFFSET|checksum-auto INPUT OUTPUT|rom-expand INPUT OUTPUT MAPPER TARGET_LOGICAL_SIZE FILL|copier-header-add INPUT OUTPUT FILL|copier-header-remove INPUT OUTPUT|patch INPUT OUTPUT OFFSET HEX_BYTES|ips-apply SOURCE PATCH OUTPUT|ips-create BEFORE AFTER OUTPUT|expanded-settings-install INPUT_ROM OUTPUT_ROM|smw-map16-runtime-install INPUT_ROM OUTPUT_ROM|layer3-install INPUT_ROM OUTPUT_ROM|smw-overworld-transfer-observe ROM OUTPUT|smw-overworld-transfer-full-observe ROM OUTPUT|smw-transferred-map16-observe ROM OUTPUT|smw-overworld-path-export ROM OUTPUT|smw-overworld-path-import INPUT_ROM LINKS OUTPUT_ROM|smw-overworld-warp-export ROM OUTPUT|smw-overworld-warp-import INPUT_ROM LINKS OUTPUT_ROM>".into()).into()),
    }
}

fn parse_level_inspection_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    Ok(match text {
        [
            command,
            _,
            mapper,
            number,
            layer1_table,
            sprite_table,
            sprite_format,
        ] if command == "level" => Some(Command::Level {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            number: usize::try_from(parse_number(number)?)?,
            layer1_table: usize::try_from(parse_number(layer1_table)?)?,
            sprite_table: usize::try_from(parse_number(sprite_table)?)?,
            expanded_sprites: parse_sprite_format(sprite_format)?,
        }),
        [
            command,
            _,
            mapper,
            number,
            layer1_table,
            sprite_low_table,
            sprite_bank_table,
            sprite_format,
        ] if command == "level-split-bank" => Some(Command::LevelSplitBank {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            number: usize::try_from(parse_number(number)?)?,
            layer1_table: usize::try_from(parse_number(layer1_table)?)?,
            sprite_low_table: usize::try_from(parse_number(sprite_low_table)?)?,
            sprite_bank_table: usize::try_from(parse_number(sprite_bank_table)?)?,
            expanded_sprites: parse_sprite_format(sprite_format)?,
        }),
        [command, _, mapper, number, layer1_table, layer2_table, _]
            if command == "level-layer2" =>
        {
            Some(Command::LevelLayer2 {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                number: usize::try_from(parse_number(number)?)?,
                layer1_table: usize::try_from(parse_number(layer1_table)?)?,
                layer2_table: usize::try_from(parse_number(layer2_table)?)?,
                output: PathBuf::from(&args[6]),
            })
        }
        _ => None,
    })
}

fn parse_special_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    if let Some(command) = parse_rats_command(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_rom_command(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_indexed_map16_import(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_rgb_map16_import(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_rgba_map16_import(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_png_map16_import(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_exanimation_frame_edit(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_profile_command(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_dsc_render::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_dsc_sidecar::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_native_sidecars::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_custom_libraries::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_portable_files::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_oracle_verification(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = parse_oracle_capture(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = parse_asset_command(args, text)? {
        return Ok(Some(command));
    }
    for parsed in [
        parse_map16_transfer(args, text)?,
        parse_graphics_transfer(args, text)?,
        parse_level_transfer(args, text)?,
        parse_palette_transfer(args, text)?,
        parse_exanimation_transfer(args, text)?,
        parse_overworld_transfer(args, text)?,
        parse_expanded_settings_transfer(args, text)?,
    ] {
        if parsed.is_some() {
            return Ok(parsed);
        }
    }
    Ok(None)
}

fn parse_exanimation_frame_edit(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, maximum_records, record, _, _] if command == "exanimation-frames" => {
            Some(Command::EditExAnimationFrames {
                input: PathBuf::from(&args[1]),
                size_modes: PathBuf::from(&args[2]),
                maximum_records: usize::try_from(parse_number(maximum_records)?)
                    .map_err(|_| ArgsError("maximum record count exceeds usize".into()))?,
                record: usize::try_from(parse_number(record)?)
                    .map_err(|_| ArgsError("record index exceeds usize".into()))?,
                edits: PathBuf::from(&args[5]),
                output: PathBuf::from(&args[6]),
            })
        }
        _ => None,
    })
}

fn parse_profile_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    if let Some(command) = parse_title_command(args, text)
        .or_else(|| parse_special_event_command(args, text))
        .or_else(|| parse_transfer_observation_command(args, text))
    {
        return Ok(Some(command));
    }
    if let Some(command) = parse_native_overworld_command(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_shared_palette_native::parse(args, text) {
        return Ok(Some(command));
    }
    Ok(match text {
        [command, _] if command == "profile" => Some(Command::Profile {
            profile: PathBuf::from(&args[1]),
            rom: None,
        }),
        [command, _, _] if command == "profile" => Some(Command::Profile {
            profile: PathBuf::from(&args[1]),
            rom: Some(PathBuf::from(&args[2])),
        }),
        [command, kind, _, _, slot, _] if command == "profile-export" => {
            Some(Command::ProfileExport {
                kind: parse_profile_export_kind(kind)?,
                rom: PathBuf::from(&args[2]),
                profile: PathBuf::from(&args[3]),
                slot: usize::try_from(parse_number(slot)?)?,
                output: PathBuf::from(&args[5]),
            })
        }
        [command, kind, _, _, _, slot, _, start, end] if command == "profile-import" => {
            Some(Command::ProfileImport {
                kind: parse_profile_import_kind(kind)?,
                input_rom: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
                profile: PathBuf::from(&args[4]),
                slot: usize::try_from(parse_number(slot)?)?,
                asset: PathBuf::from(&args[6]),
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
            })
        }
        [command, _, _, _, _, start, end, fill] if command == "revision-patch-install" => {
            Some(Command::RevisionPatchInstall {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                profile: PathBuf::from(&args[3]),
                template: PathBuf::from(&args[4]),
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                fill: u8::try_from(parse_number(fill)?)?,
            })
        }
        [command, _, _] if command == "expanded-settings-install" => {
            Some(Command::ExpandedSettingsInstall {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
            })
        }
        [command, _, _] if command == "smw-map16-runtime-install" => {
            Some(Command::Map16RuntimeInstall {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
            })
        }
        [command, _, _] if command == "layer3-install" => Some(Command::Layer3Install {
            input_rom: PathBuf::from(&args[1]),
            output_rom: PathBuf::from(&args[2]),
        }),
        _ => None,
    })
}

fn parse_native_overworld_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Option<Command> {
    let export = |constructor: fn(PathBuf, PathBuf) -> Command| {
        Some(constructor(
            PathBuf::from(&args[1]),
            PathBuf::from(&args[2]),
        ))
    };
    let import = |constructor: fn(PathBuf, PathBuf, PathBuf) -> Command| {
        Some(constructor(
            PathBuf::from(&args[1]),
            PathBuf::from(&args[2]),
            PathBuf::from(&args[3]),
        ))
    };
    match text {
        [command, _, _] if command == "smw-overworld-path-export" => {
            export(|rom, output| Command::SmwOverworldPathExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-path-import" => import(
            |input_rom, links, output_rom| Command::SmwOverworldPathImport {
                input_rom,
                links,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-message-export" => {
            export(|rom, output| Command::SmwOverworldMessageExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-message-install" => import(
            |input_rom, messages, output_rom| Command::SmwOverworldMessageInstall {
                input_rom,
                messages,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-event-export" => {
            export(|rom, output| Command::SmwOverworldEventExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-event-import" => import(
            |input_rom, events, output_rom| Command::SmwOverworldEventImport {
                input_rom,
                events,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-event-map-export" => {
            export(|rom, output| Command::SmwOverworldEventMapExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-event-map-import" => import(
            |input_rom, event_map, output_rom| Command::SmwOverworldEventMapImport {
                input_rom,
                event_map,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-warp-export" => {
            export(|rom, output| Command::SmwOverworldWarpExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-warp-import" => import(
            |input_rom, links, output_rom| Command::SmwOverworldWarpImport {
                input_rom,
                links,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-name-export" => {
            export(|rom, output| Command::SmwOverworldNameExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-name-import" => import(
            |input_rom, names, output_rom| Command::SmwOverworldNameImport {
                input_rom,
                names,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-settings-export" => {
            export(|rom, output| Command::SmwOverworldSettingsExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-settings-import" => import(
            |input_rom, settings, output_rom| Command::SmwOverworldSettingsImport {
                input_rom,
                settings,
                output_rom,
            },
        ),
        [command, _, _] if command == "smw-overworld-layer3-settings-observe" => {
            export(|rom, output| Command::SmwOverworldLayer3SettingsObserve { rom, output })
        }
        [command, _, _] if command == "smw-overworld-start-export" => {
            export(|rom, output| Command::SmwOverworldStartExport { rom, output })
        }
        [command, _, _, _] if command == "smw-overworld-start-import" => import(
            |input_rom, starts, output_rom| Command::SmwOverworldStartImport {
                input_rom,
                starts,
                output_rom,
            },
        ),
        _ => None,
    }
}

fn parse_transfer_observation_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Option<Command> {
    let paths = || (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    match text {
        [command, _, _] if command == "smw-overworld-transfer-observe" => {
            let (rom, output) = paths();
            Some(Command::SmwOverworldTransferObserve { rom, output })
        }
        [command, _, _] if command == "smw-overworld-transfer-full-observe" => {
            let (rom, output) = paths();
            Some(Command::SmwOverworldTransferFullObserve { rom, output })
        }
        [command, _, _] if command == "smw-transferred-map16-observe" => {
            let (rom, output) = paths();
            Some(Command::SmwTransferredMap16Observe { rom, output })
        }
        [command, _, _] if command == "smw-installed-map16-remaps-observe" => {
            let (rom, output) = paths();
            Some(Command::SmwInstalledMap16RemapsObserve { rom, output })
        }
        _ => None,
    }
}

fn parse_special_event_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Option<Command> {
    match text {
        [command, _, _] if command == "smw-overworld-special-event-export" => {
            Some(Command::SmwOverworldSpecialEventExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-overworld-special-event-import" => {
            Some(Command::SmwOverworldSpecialEventImport {
                input_rom: PathBuf::from(&args[1]),
                events: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-overworld-boss-sequence-export" => {
            Some(Command::SmwOverworldBossSequenceExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-overworld-boss-sequence-import" => {
            Some(Command::SmwOverworldBossSequenceImport {
                input_rom: PathBuf::from(&args[1]),
                messages: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-credits-tilemap-export" => {
            Some(Command::SmwCreditsTilemapExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-credits-tilemap-import" => {
            Some(Command::SmwCreditsTilemapImport {
                input_rom: PathBuf::from(&args[1]),
                tilemap: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-overworld-event-tilemap-export" => {
            Some(Command::SmwOverworldEventTilemapExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-overworld-event-tilemap-import" => {
            Some(Command::SmwOverworldEventTilemapImport {
                input_rom: PathBuf::from(&args[1]),
                tilemaps: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        _ => None,
    }
}

fn parse_title_command(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _, _] if command == "smw-title-tilemap-export" => {
            Some(Command::SmwTitleTilemapExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-title-tilemap-import" => {
            Some(Command::SmwTitleTilemapImport {
                input_rom: PathBuf::from(&args[1]),
                tilemap: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-title-recording-export" => {
            Some(Command::SmwTitleRecordingExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-title-recording-import" => {
            Some(Command::SmwTitleRecordingImport {
                input_rom: PathBuf::from(&args[1]),
                recording: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-title-recording-zst-export" => {
            Some(Command::SmwTitleRecordingZsnesExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-title-recording-zst-import" => {
            Some(Command::SmwTitleRecordingZsnesImport {
                input_rom: PathBuf::from(&args[1]),
                state: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _, _] if command == "smw-title-recording-s9x-import" => {
            Some(Command::SmwTitleRecordingSnes9xImport {
                input_rom: PathBuf::from(&args[1]),
                state: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-lm-metadata-export" => {
            Some(Command::SmwLunarMagicMetadataExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-lm-metadata-import" => {
            Some(Command::SmwLunarMagicMetadataImport {
                input_rom: PathBuf::from(&args[1]),
                metadata: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        [command, _, _] if command == "smw-secondary-exit-export" => {
            Some(Command::SmwSecondaryExitExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-secondary-exit-import" => {
            Some(Command::SmwSecondaryExitImport {
                input_rom: PathBuf::from(&args[1]),
                table: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
