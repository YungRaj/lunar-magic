mod application_frontend_commands;
mod application_rom_commands;
mod application_tool_commands;
mod command_script;
mod complete_level_document_shell;
mod complete_level_dsc_render;
mod complete_level_render_spec;
mod copier_header_shell;
mod custom_object_edit_script;
mod custom_object_shell;
mod custom_sprite_document_spec;
mod custom_sprite_edit_script;
mod custom_sprite_shell;
mod dsc_sidecar_shell;
mod editor_shell;
mod entity_appearance_document_shell;
mod entity_appearance_edit_script;
mod exanimation_document_shell;
mod exanimation_document_spec;
mod exanimation_feature_edit_script;
mod exanimation_edit_script;
mod expanded_settings_document_shell;
mod expanded_settings_edit_script;
mod graphics_document_shell;
mod graphics_edit_script;
mod graphics_render_spec;
mod ips_shell;
mod layer3_document_shell;
mod layer3_edit_script;
mod level_edit_script;
mod map16_document_shell;
mod map16_edit_script;
mod map16_page_document_shell;
mod map16_page_edit_script;
mod map16_render_spec;
mod mwl_document_shell;
mod mwl_edit_script;
mod mwl_layer3_settings_spec;
mod mwl_optional_assets_edit_spec;
mod mwl_optional_assets_spec;
mod native_assets_document_shell;
mod native_assets_document_spec;
mod native_assets_edit_loader;
mod native_assets_edit_spec;
mod native_level_document_shell;
mod native_level_document_spec;
mod native_map16_sidecar_edit_script;
mod native_map16_sidecar_shell;
mod native_map16_sidecar_spec;
mod overworld_appearance_document_shell;
mod overworld_appearance_edit_script;
mod overworld_document_shell;
mod overworld_edit_script;
mod overworld_metadata_edit_script;
mod overworld_metadata_shell;
mod overworld_path_edit_script;
mod overworld_path_shell;
mod overworld_render_spec;
mod palette_document_shell;
mod palette_edit_script;
mod palette_render_spec;
mod portable_document_sessions;
mod portable_render_shell;
mod revision_patch_install_spec;
mod revision_patch_shell;
mod shell_command;
mod shell_document_command;
mod spec_text;
mod sprite_spawn_edit_script;
mod startup;
mod tool_shell;
mod ui_shell;
mod viewport_spec;

#[cfg(test)]
use application_frontend_commands::install_ui_config;
use application_frontend_commands::{dispatch_and_print, execute_ui_command, save, save_as};
use application_rom_commands::{
    close_and_print, expand_rom, install_profile, open_and_print, open_recent, read_bounded_bytes,
    request_quit, select_asset, show_recent, show_status,
};
use application_tool_commands::execute_tool_command;
use complete_level_document_shell::execute_complete_level_document_command;
use custom_object_shell::execute_custom_object_command;
#[cfg(test)]
use custom_object_shell::{
    close_custom_objects, edit_custom_objects, navigate_custom_object_history, open_custom_objects,
    save_custom_objects,
};
use custom_sprite_shell::execute_custom_sprite_command;
use dsc_sidecar_shell::execute as execute_dsc_sidecar_command;
#[cfg(test)]
use editor_shell::{
    commit_exanimation_edits, commit_graphics_edits, commit_level_edits, commit_map16_edits,
    commit_overworld_edits, commit_palette_edits, execute_exanimation_script,
    execute_graphics_script, execute_level_script, execute_map16_script, execute_overworld_script,
    execute_palette_script,
};
use editor_shell::{
    edit_expanded_settings, edit_expanded_settings_word, edit_level_header, execute_editor_script,
    export_all_mwl_levels, export_modified_mwl_levels, export_mwl_level, import_mwl_level,
    import_mwl_level_directory, migrate_graphics_compression,
};
use entity_appearance_document_shell::execute_entity_appearance_document_command;
use exanimation_document_shell::execute_exanimation_document_command;
use expanded_settings_document_shell::execute as execute_expanded_settings_document_command;
use graphics_document_shell::execute_graphics_document_command;
use layer3_document_shell::execute_layer3_document_command;
#[cfg(test)]
use layer3_document_shell::{
    close_layer3_document, edit_layer3_document, navigate_layer3_document_history,
    open_layer3_document, save_layer3_document,
};
use lm_app::{AppState, Command, FrontendEffect, file_persistence};
#[cfg(test)]
use lm_app::{FrontendConfig, ToolConfig};
use lm_level::{ExpandedOverworldSettings, SecondaryExitTable};
use lm_overworld::{
    BossSequenceMessageTable, CreditsTilemap, EventNumberMap, EventRevealTable,
    EventTilemapBuffers, ExpandedLayerTilemap, NativeOverworldLevelNameTable,
    NativeOverworldPlayerStarts, OverworldMessage, OverworldMetadata, OverworldPathLinkTable,
    OverworldWarpLinkTable, SpecialEventRevealTable, decode_native_overworld_message_file,
    encode_native_overworld_message_file,
};
use lm_profile::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
    load_smw_us_v1_event_tilemaps, load_smw_us_v1_overworld_messages,
    smw_us_v1_boss_sequence_locator, smw_us_v1_credits_tilemap_locator,
    smw_us_v1_default_special_expanded_settings_record, smw_us_v1_expanded_settings_layout,
    smw_us_v1_lunar_magic_metadata_layout, smw_us_v1_overworld_event_number_map_locator,
    smw_us_v1_overworld_event_reveal_locator, smw_us_v1_overworld_level_name_locator,
    smw_us_v1_overworld_level_name_runtime, smw_us_v1_overworld_path_patch_locator,
    smw_us_v1_overworld_player_start_layout, smw_us_v1_overworld_warp_patch_locator,
    smw_us_v1_secondary_exit_locator, smw_us_v1_special_event_reveal_locator,
    smw_us_v1_title_recording_locator, smw_us_v1_title_tilemap_locator,
};
use lm_rom::LunarMagicRomMetadata;
use lm_title::{
    TitleScreenRecording, decode_snes9x_title_recording, decode_zsnes_title_recording,
    encode_zsnes_title_recording,
};
use map16_document_shell::execute_map16_document_command;
use map16_page_document_shell::execute_map16_page_document_command;
use mwl_document_shell::execute_mwl_document_command;
use native_assets_document_shell::execute as execute_native_assets_document_command;
use native_level_document_shell::execute_native_level_document_command;
use native_map16_sidecar_shell::execute as execute_native_map16_sidecar_command;
use overworld_appearance_document_shell::execute_overworld_appearance_document_command;
use overworld_document_shell::execute_overworld_document_command;
use overworld_metadata_shell::execute_metadata_document_command;
#[cfg(test)]
use overworld_metadata_shell::{
    close_metadata_document, edit_metadata_document, navigate_metadata_history,
    open_metadata_document, save_metadata_document,
};
use overworld_path_shell::execute_path_document_command;
#[cfg(test)]
use overworld_path_shell::{
    close_path_document, edit_path_document, navigate_path_history, open_path_document,
    save_path_document,
};
use palette_document_shell::execute_palette_document_command;
use portable_document_sessions::PortableDocumentSessions;
use revision_patch_shell::install_revision_patch;
use shell_command::ShellCommand;
#[cfg(test)]
use std::fs;
use std::io::{self, BufRead, Write};

fn main() {
    if let Err(error) = run() {
        eprintln!("lm-app: {error}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines)] // Exhaustive top-level routing; domain logic lives in focused modules.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = AppState::default();
    let mut documents = PortableDocumentSessions::default();
    let Some(mut startup) = startup::initialize(&mut app)? else {
        return Ok(());
    };
    print_help();
    let allow_in_place_rom_write = startup.allow_in_place_rom_write;

    let stdin = io::stdin();
    let interactive = startup.command_lines.is_none();
    let mut lines: Box<dyn Iterator<Item = io::Result<String>> + '_> = match startup.command_lines {
        Some(lines) => Box::new(lines.into_iter().map(Ok)),
        None => Box::new(stdin.lock().lines()),
    };
    loop {
        if interactive {
            print!("lm> ");
            io::stdout().flush()?;
        }
        let Some(line) = lines.next() else {
            if documents.has_dirty_documents() {
                return Err(format!(
                    "end of input with unsaved portable documents ({}); save or explicitly discard before closing",
                    documents.dirty_documents().join(", ")
                )
                .into());
            }
            if app
                .dispatch(Command::Quit)?
                .contains(&FrontendEffect::QuitApplication)
            {
                break;
            }
            return Err(
                "end of input with unsaved changes; save or explicitly discard before closing"
                    .into(),
            );
        };
        let line = line?;
        match shell_command::parse(&line)? {
            ShellCommand::Empty => {}
            ShellCommand::Help => print_help(),
            ShellCommand::Status => show_status(&app),
            ShellCommand::Recent => show_recent(&app),
            ShellCommand::OpenRecent(index) => open_recent(&mut app, &mut lines, index)?,
            ShellCommand::Open(path) => open_and_print(&mut app, &mut lines, path)?,
            ShellCommand::Close => close_and_print(&mut app, &mut lines)?,
            ShellCommand::InstallRevisionProfile(path) => install_profile(&mut app, &path)?,
            ShellCommand::InstallRevisionPatch(path) => {
                install_revision_patch(&mut app, &path)?;
            }
            ShellCommand::InstallSettings => {
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::InstallSettings { rev: revision })?;
                println!("{}", app.status);
            }
            ShellCommand::InstallLayer3 => {
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::InstallLayer3 { rev: revision })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldPathExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_file()?)?;
                println!(
                    "exported-native-overworld-path-links: {}",
                    table.links.len()
                );
            }
            ShellCommand::NativeOverworldPathImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    12 + OverworldPathLinkTable::MAX_LINKS * 12,
                    "native overworld path-link file",
                )?;
                let table = OverworldPathLinkTable::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldPathLinks {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldMessageExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let messages = load_smw_us_v1_overworld_messages(project)?.messages;
                file_persistence::write_new(
                    &path,
                    &encode_native_overworld_message_file(&messages)?,
                )?;
                println!("exported-native-overworld-messages: {}", messages.len());
            }
            ShellCommand::NativeOverworldMessageImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    10 + 512 * OverworldMessage::ENCODED_LEN,
                    "native overworld-message file",
                )?;
                let messages = decode_native_overworld_message_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldMessages {
                    rev: revision,
                    messages,
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldEventExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_overworld_event_reveals_detected(
                        smw_us_v1_overworld_event_reveal_locator(),
                    )?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_event_file()?)?;
                println!(
                    "exported-native-overworld-event-reveals: {}",
                    table.entries.len()
                );
            }
            ShellCommand::NativeOverworldEventImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    10 + EventRevealTable::MAX_ENTRIES * 4,
                    "native overworld event-reveal file",
                )?;
                let table = EventRevealTable::decode_native_event_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldEventReveals {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldEventMapExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let map = project
                    .load_overworld_event_number_map_detected(
                        smw_us_v1_overworld_event_number_map_locator(),
                    )?
                    .map;
                file_persistence::write_new(&path, &map.encode_native_file()?)?;
                println!("exported-native-overworld-event-map: {}", map.stored_len());
            }
            ShellCommand::NativeOverworldEventMapImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    10 + EventNumberMap::ENTRY_COUNT,
                    "native overworld event-number map",
                )?;
                let map = EventNumberMap::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldEventNumberMap {
                    rev: revision,
                    map: Box::new(map),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldSpecialEventExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_file()?)?;
                println!("exported-native-special-event-reveals: 24");
            }
            ShellCommand::NativeOverworldSpecialEventImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    SpecialEventRevealTable::FILE_LEN,
                    "native special-event reveal file",
                )?;
                let table = SpecialEventRevealTable::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeSpecialEventReveals {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldEventTilemapExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let loaded = load_smw_us_v1_event_tilemaps(project)?;
                file_persistence::write_new(&path, &loaded.buffers.encode_native_file())?;
                println!("exported-native-event-tilemap-bytes: 6144");
            }
            ShellCommand::NativeOverworldEventTilemapImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    EventTilemapBuffers::FILE_LEN,
                    "native overworld event-tilemap file",
                )?;
                let buffers = EventTilemapBuffers::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldEventTilemaps {
                    rev: revision,
                    buffers: Box::new(buffers),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldBossSequenceExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_file())?;
                println!("exported-native-boss-sequence-glyphs: 1344");
            }
            ShellCommand::NativeOverworldBossSequenceImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    BossSequenceMessageTable::FILE_LEN,
                    "native overworld boss-sequence file",
                )?;
                let table = BossSequenceMessageTable::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldBossSequence {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeCreditsTilemapExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let tilemap = project
                    .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())?
                    .tilemap;
                file_persistence::write_new(&path, &tilemap.encode_native_file())?;
                println!("exported-credits-tilemap-words: 8192");
            }
            ShellCommand::NativeCreditsTilemapImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    CreditsTilemap::FILE_LEN,
                    "native credits tilemap file",
                )?;
                let tilemap = CreditsTilemap::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeCreditsTilemap {
                    rev: revision,
                    tilemap: Box::new(tilemap),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeTitleTilemapExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let tilemap = project
                    .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())?
                    .tilemap;
                file_persistence::write_new(&path, &tilemap.encode_native_file())?;
                println!("exported-title-tilemap-words: 1856");
            }
            ShellCommand::NativeTitleTilemapImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    ExpandedLayerTilemap::FILE_LEN,
                    "native title tilemap file",
                )?;
                let tilemap = ExpandedLayerTilemap::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeTitleTilemap {
                    rev: revision,
                    tilemap: Box::new(tilemap),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeTitleRecordingExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let recording = project
                    .load_title_recording_detected(&smw_us_v1_title_recording_locator())?
                    .recording
                    .ok_or("ROM has no installed title-screen recording")?;
                file_persistence::write_new(&path, &recording.encode_native_file())?;
                println!(
                    "exported-title-recording-bytes: {}",
                    recording.bytes().len()
                );
            }
            ShellCommand::NativeTitleRecordingImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    TitleScreenRecording::MAX_FILE_LEN,
                    "native title recording file",
                )?;
                let recording = TitleScreenRecording::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeTitleRecording {
                    rev: revision,
                    recording,
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeTitleRecordingZsnesExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let recording = project
                    .load_title_recording_detected(&smw_us_v1_title_recording_locator())?
                    .recording
                    .ok_or("ROM has no installed title-screen recording")?;
                file_persistence::write_new(&path, &encode_zsnes_title_recording(&recording))?;
                println!(
                    "exported-title-recording-zst-bytes: {}",
                    recording.bytes().len()
                );
            }
            ShellCommand::NativeTitleRecordingZsnesImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    0x20c13,
                    "ZSNES title recording state",
                )?;
                let recording = decode_zsnes_title_recording(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeTitleRecording {
                    rev: revision,
                    recording,
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeTitleRecordingSnes9xImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    64 * 1024 * 1024,
                    "Snes9x title recording state",
                )?;
                let recording = decode_snes9x_title_recording(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeTitleRecording {
                    rev: revision,
                    recording,
                })?;
                println!("{}", app.status);
            }
            ShellCommand::LunarMagicMetadataExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let metadata = project
                    .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())?
                    .ok_or("ROM has no installed Lunar Magic metadata")?;
                file_persistence::write_new(&path, &metadata.encode_file())?;
                println!(
                    "exported-lunar-magic-metadata-bytes: {}",
                    LunarMagicRomMetadata::FILE_LEN
                );
            }
            ShellCommand::LunarMagicMetadataImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    LunarMagicRomMetadata::FILE_LEN,
                    "Lunar Magic metadata file",
                )?;
                let metadata = LunarMagicRomMetadata::decode_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceLunarMagicRomMetadata {
                    rev: revision,
                    metadata: Box::new(metadata),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeSecondaryExitExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_file()?)?;
                println!("exported-native-secondary-exits: 8192");
            }
            ShellCommand::NativeSecondaryExitImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    SecondaryExitTable::FILE_LEN,
                    "native secondary-exit table",
                )?;
                let table = SecondaryExitTable::decode_native_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeSecondaryExits {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldWarpExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let table = project
                    .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?
                    .table;
                file_persistence::write_new(&path, &table.encode_native_warp_file()?)?;
                println!(
                    "exported-native-overworld-warp-links: {}",
                    table.links.len()
                );
            }
            ShellCommand::NativeOverworldWarpImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    12 + OverworldWarpLinkTable::MAX_LINKS * 8,
                    "native overworld warp-link file",
                )?;
                let table = OverworldWarpLinkTable::decode_native_warp_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldWarpLinks {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldLevelNameExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let names = project
                    .load_overworld_level_names_detected(
                        smw_us_v1_overworld_level_name_locator(),
                        smw_us_v1_overworld_level_name_runtime(),
                    )?
                    .table
                    .names;
                let count = names.len();
                let encoded = OverworldMetadata {
                    level_names: names,
                    ..OverworldMetadata::default()
                }
                .encode_file()?;
                file_persistence::write_new(&path, &encoded)?;
                println!("exported-native-overworld-level-names: {count}");
            }
            ShellCommand::NativeOverworldLevelNameImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    OverworldMetadata::MAX_FILE_LEN,
                    "native overworld level-name metadata file",
                )?;
                let metadata = OverworldMetadata::decode_file(&bytes)?;
                if !metadata.player_starts.is_empty() || !metadata.submap_settings.is_empty() {
                    return Err(
                        "native level-name import requires LMOWMETA containing names only".into(),
                    );
                }
                let table = NativeOverworldLevelNameTable {
                    names: metadata.level_names,
                };
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldLevelNames {
                    rev: revision,
                    table: Box::new(table),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldSettingsExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let settings = if project.rom.logical_bytes().get(
                    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START
                        ..SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START + 4,
                ) == Some(b"STAR")
                {
                    project.load_expanded_overworld_settings(
                        SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                        smw_us_v1_expanded_settings_layout(),
                    )?
                } else {
                    ExpandedOverworldSettings {
                        records: std::array::from_fn(|_| {
                            smw_us_v1_default_special_expanded_settings_record()
                        }),
                    }
                };
                file_persistence::write_new(&path, &settings.encode_file())?;
                println!("exported-native-overworld-settings: 7");
            }
            ShellCommand::NativeOverworldSettingsImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    ExpandedOverworldSettings::ENCODED_LEN,
                    "native overworld settings file",
                )?;
                let settings = ExpandedOverworldSettings::decode_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldSettings {
                    rev: revision,
                    settings: Box::new(settings),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::NativeOverworldPlayerStartExport(path) => {
                let project = app.project().ok_or("no project is open")?;
                let starts = project
                    .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())?;
                file_persistence::write_new(&path, &starts.encode_file()?)?;
                println!("exported-native-overworld-player-starts: 2");
            }
            ShellCommand::NativeOverworldPlayerStartImport(path) => {
                let bytes = application_rom_commands::read_bounded_bytes(
                    &path,
                    NativeOverworldPlayerStarts::FILE_LEN,
                    "native overworld player-start file",
                )?;
                let starts = NativeOverworldPlayerStarts::decode_file(&bytes)?;
                let revision = app.controller_snapshot()?.revision;
                app.dispatch(Command::ReplaceNativeOverworldPlayerStarts {
                    rev: revision,
                    starts: Box::new(starts),
                })?;
                println!("{}", app.status);
            }
            ShellCommand::ClearRevisionProfile => {
                dispatch_and_print(&mut app, Command::ClearRevisionProfile)?;
            }
            ShellCommand::Ui(command) => {
                execute_ui_command(&mut app, command, allow_in_place_rom_write)?;
            }
            ShellCommand::Tool(command) => execute_tool_command(&mut app, command)?,
            custom @ (ShellCommand::CustomObjectOpen(_)
            | ShellCommand::CustomObjectEdit(_)
            | ShellCommand::CustomObjectUndo
            | ShellCommand::CustomObjectRedo
            | ShellCommand::CustomObjectStatus
            | ShellCommand::CustomObjectSave
            | ShellCommand::CustomObjectClose
            | ShellCommand::CustomObjectDiscard) => {
                execute_custom_object_command(&mut documents.custom_objects, custom)?;
            }
            custom_sprite @ (ShellCommand::CustomSpriteOpen(_)
            | ShellCommand::CustomSpriteEdit(_)
            | ShellCommand::CustomSpriteUndo
            | ShellCommand::CustomSpriteRedo
            | ShellCommand::CustomSpriteStatus
            | ShellCommand::CustomSpriteSave
            | ShellCommand::CustomSpriteClose
            | ShellCommand::CustomSpriteDiscard) => {
                execute_custom_sprite_command(&mut documents.custom_sprites, custom_sprite)?;
            }
            dsc @ (ShellCommand::DscSidecarOpen(_)
            | ShellCommand::DscSidecarReplace(_)
            | ShellCommand::DscSidecarUndo
            | ShellCommand::DscSidecarRedo
            | ShellCommand::DscSidecarStatus
            | ShellCommand::DscSidecarSave
            | ShellCommand::DscSidecarClose
            | ShellCommand::DscSidecarDiscard) => {
                execute_dsc_sidecar_command(&mut documents.dsc_sidecar, dsc)?;
            }
            native_sidecar @ (ShellCommand::NativeMap16SidecarOpen(_)
            | ShellCommand::NativeMap16SidecarEdit(_)
            | ShellCommand::NativeMap16SidecarUndo
            | ShellCommand::NativeMap16SidecarRedo
            | ShellCommand::NativeMap16SidecarStatus
            | ShellCommand::NativeMap16SidecarSave
            | ShellCommand::NativeMap16SidecarClose
            | ShellCommand::NativeMap16SidecarDiscard) => {
                execute_native_map16_sidecar_command(
                    &mut documents.native_map16_sidecar,
                    native_sidecar,
                )?;
            }
            ShellCommand::MetadataDocument(command) => {
                execute_metadata_document_command(&mut documents.metadata, command)?;
            }
            ShellCommand::PathDocument(command) => {
                execute_path_document_command(&mut documents.paths, command)?;
            }
            ShellCommand::Layer3Document(command) => {
                execute_layer3_document_command(&mut documents.layer3, command)?;
            }
            ShellCommand::ExpandedSettingsDocument(command) => {
                execute_expanded_settings_document_command(
                    &mut documents.expanded_settings,
                    command,
                )?;
            }
            ShellCommand::CompleteLevelDocument(command) => {
                execute_complete_level_document_command(&mut documents.complete_level, command)?;
            }
            ShellCommand::Map16Document(command) => {
                execute_map16_document_command(&mut documents.map16, command)?;
            }
            ShellCommand::Map16PageDocument(command) => {
                execute_map16_page_document_command(&mut documents.map16_page, command)?;
            }
            ShellCommand::OverworldDocument(command) => {
                execute_overworld_document_command(&mut documents.overworld, command)?;
            }
            ShellCommand::OverworldAppearanceDocument(command) => {
                execute_overworld_appearance_document_command(
                    &mut documents.overworld_appearances,
                    command,
                )?;
            }
            ShellCommand::GraphicsDocument(command) => {
                execute_graphics_document_command(&mut documents.graphics, command)?;
            }
            ShellCommand::PaletteDocument(command) => {
                execute_palette_document_command(&mut documents.palette, command)?;
            }
            ShellCommand::ExAnimationDocument(command) => {
                execute_exanimation_document_command(&mut documents.exanimation, command)?;
            }
            ShellCommand::EntityAppearanceDocument(command) => {
                execute_entity_appearance_document_command(
                    &mut documents.entity_appearances,
                    command,
                )?;
            }
            ShellCommand::MwlDocument(command) => {
                execute_mwl_document_command(&mut documents.mwl, command)?;
            }
            ShellCommand::NativeLevelDocument(command) => {
                execute_native_level_document_command(&mut documents.native_level, command)?;
            }
            ShellCommand::NativeAssetsDocument(command) => {
                execute_native_assets_document_command(&mut documents.native_assets, command)?;
            }
            ShellCommand::RenderMap16(path) => portable_render_shell::render_map16_spec(&path)?,
            ShellCommand::RenderOverworld(path) => {
                portable_render_shell::render_overworld_spec(&path)?;
            }
            ShellCommand::IpsApply(path) => ips_shell::apply(&path)?,
            ShellCommand::IpsCreate(path) => ips_shell::create(&path)?,
            ShellCommand::CopierHeaderAdd(path) => copier_header_shell::add(&path)?,
            ShellCommand::CopierHeaderRemove(path) => copier_header_shell::remove(&path)?,
            ShellCommand::SelectLevel(level) => {
                dispatch_and_print(&mut app, Command::SelectLevel(level))?;
            }
            ShellCommand::LevelBack => dispatch_and_print(
                &mut app,
                Command::NavigateLevel(lm_app::LevelNavigationDirection::Back),
            )?,
            ShellCommand::LevelForward => dispatch_and_print(
                &mut app,
                Command::NavigateLevel(lm_app::LevelNavigationDirection::Forward),
            )?,
            ShellCommand::SetLevelViewport {
                x,
                y,
                zoom_numerator,
                zoom_denominator,
            } => dispatch_and_print(
                &mut app,
                Command::SetLevelViewport(lm_app::LevelViewport::new(
                    lm_render::Point { x, y },
                    zoom_numerator,
                    zoom_denominator,
                )?),
            )?,
            ShellCommand::EditLevelHeader {
                field,
                value,
                search_start,
                search_end,
            } => {
                edit_level_header(&mut app, field, value, search_start..search_end)?;
                println!("{}", app.status);
            }
            ShellCommand::EditExpandedSettingsWord { index, value } => {
                edit_expanded_settings_word(&mut app, index, value)?;
                println!("{}", app.status);
            }
            ShellCommand::EditExpandedSettings(path) => {
                edit_expanded_settings(&mut app, &path)?;
                println!("{}", app.status);
            }
            ShellCommand::ApplyEditorScript {
                editor,
                script,
                search_start,
                search_end,
            } => execute_editor_script(&mut app, editor, &script, search_start..search_end)?,
            ShellCommand::ApplyOwnedEditorScript {
                editor,
                script,
                ownership_manifest,
                search_start,
                search_end,
            } => editor_shell::execute_owned_editor_script(
                &mut app,
                editor,
                &script,
                &ownership_manifest,
                search_start..search_end,
            )?,
            ShellCommand::ImportMwlLevel {
                path,
                search_start,
                search_end,
            } => import_mwl_level(&mut app, &path, search_start..search_end)?,
            ShellCommand::ImportMwlLevelDirectory {
                path,
                search_start,
                search_end,
            } => import_mwl_level_directory(&mut app, &path, search_start..search_end)?,
            ShellCommand::ExportMwlLevel(path) => export_mwl_level(&app, &path)?,
            ShellCommand::ExportAllMwlLevels(path) => export_all_mwl_levels(&app, &path)?,
            ShellCommand::ExportModifiedMwlLevels(path) => {
                export_modified_mwl_levels(&app, &path)?;
            }
            ShellCommand::MigrateGraphicsCompression {
                target,
                search_start,
                search_end,
            } => {
                migrate_graphics_compression(&mut app, target, search_start..search_end)?;
                println!("{}", app.status);
            }
            ShellCommand::ExpandRom {
                target_logical_len,
                fill,
            } => expand_rom(&mut app, target_logical_len, fill)?,
            ShellCommand::ShowOverworld => {
                app.dispatch(Command::ShowOverworld)?;
                println!("{}", app.status);
            }
            ShellCommand::ShowMap16 => {
                app.dispatch(Command::ShowMap16)?;
                println!("{}", app.status);
            }
            ShellCommand::ShowGraphics(slot) => select_asset(&mut app, "graphics", slot)?,
            ShellCommand::ShowPalette(slot) => select_asset(&mut app, "palette", slot)?,
            ShellCommand::ShowExAnimation(slot) => select_asset(&mut app, "exanimation", slot)?,
            ShellCommand::ShowLayer3(level) => select_asset(&mut app, "layer3", level)?,
            ShellCommand::Undo => {
                app.dispatch(Command::Undo)?;
                println!("{}", app.status);
            }
            ShellCommand::Redo => {
                app.dispatch(Command::Redo)?;
                println!("{}", app.status);
            }
            ShellCommand::Save => {
                save(&mut app, allow_in_place_rom_write)?;
                println!("{}", app.status);
            }
            ShellCommand::SaveAs(destination) => {
                save_as(&mut app, &destination)?;
                println!("{}", app.status);
            }
            ShellCommand::Quit => {
                if request_quit(&mut app, &mut documents, &mut lines)? {
                    break;
                }
            }
            ShellCommand::Unknown(command) => {
                println!("unknown command {command:?}; type help");
            }
        }
        if let Some(state) = startup.recent_state.as_mut() {
            state.persist_if_changed(&app)?;
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "commands: open PATH, recent, open-recent INDEX, close, status, profile PATH, profile-clear, revision-patch-install-file SPEC, expanded-settings-install, layer3-install, overworld-native-path-export PATH, overworld-native-path-import PATH, overworld-native-warp-export PATH, overworld-native-warp-import PATH, overworld-native-name-export PATH, overworld-native-name-import PATH, overworld-native-settings-export PATH, overworld-native-settings-import PATH, overworld-native-start-export PATH, overworld-native-start-import PATH, level HEX, level-back, level-forward, level-view X Y ZOOM_NUM ZOOM_DEN, \
         level-header FIELD VALUE SEARCH_START SEARCH_END, level-edit SCRIPT SEARCH_START SEARCH_END, level-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, \
         native-assets-edit SPEC SEARCH_START SEARCH_END, native-assets-edit-owned SPEC SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, level-mwl-import FILE SEARCH_START SEARCH_END, level-mwl-import-dir DIRECTORY SEARCH_START SEARCH_END, level-mwl-export FILE, level-mwl-export-all TEMPLATE, level-mwl-export-modified TEMPLATE, \
         expanded-settings-word INDEX VALUE, expanded-settings-edit SCRIPT, \
         map16-edit SCRIPT SEARCH_START SEARCH_END, map16-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, palette-edit SCRIPT SEARCH_START SEARCH_END, palette-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, \
         graphics-edit SCRIPT SEARCH_START SEARCH_END, graphics-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, \
         graphics-recompress lz2|lz3 SEARCH_START SEARCH_END, \
         rom-expand TARGET_LOGICAL_SIZE FILL, \
         ips-create SPEC, ips-apply SPEC, \
         copier-header-add SPEC, copier-header-remove SPEC, \
         exanimation-edit SCRIPT SEARCH_START SEARCH_END, exanimation-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, overworld, \
         overworld-edit SCRIPT SEARCH_START SEARCH_END, overworld-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST, \
         ui-config FILE, ui-status, ui-action ACTION, ui-shortcut GESTURE, \
         tools-config FILE, tools-status, tool-run ID, tool-event opened|saved|level, \
         tool-exec ID, tool-event-exec opened|saved|level, \
         custom-open DATA.mw0, custom-edit SCRIPT, custom-undo, custom-redo, custom-status, custom-save, custom-close, \
         custom-discard, custom-sprite-open SPEC, custom-sprite-edit SCRIPT, custom-sprite-undo, custom-sprite-redo, custom-sprite-status, \
         custom-sprite-save, custom-sprite-close, custom-sprite-discard, \
         dsc-open FILE, dsc-replace FILE, dsc-undo, dsc-redo, dsc-status, dsc-save, dsc-close, dsc-discard, \
         native-sidecar-open SPEC, native-sidecar-edit SCRIPT, native-sidecar-undo, native-sidecar-redo, native-sidecar-status, \
         native-sidecar-save, native-sidecar-close, native-sidecar-discard, \
         metadata-open FILE, metadata-edit SCRIPT, metadata-undo, metadata-redo, metadata-status, metadata-save, \
         metadata-close, metadata-discard, path-open FILE, path-edit SCRIPT, path-undo, path-redo, path-status, path-save, \
         path-close, path-discard, layer3-open FILE, layer3-edit-file SCRIPT, layer3-undo, layer3-redo, layer3-status, \
         layer3-save, layer3-close, layer3-discard, mwl-open FILE, mwl-edit-file SCRIPT, \
         mwl-import-optional-assets-file SPEC, mwl-edit-optional-assets-file SPEC, \
         mwl-edit-layer3-settings-file SPEC, mwl-undo, mwl-redo, \
         expanded-settings-open FILE, expanded-settings-edit-file SCRIPT, expanded-settings-undo, expanded-settings-redo, expanded-settings-status, \
         expanded-settings-save, expanded-settings-close, expanded-settings-discard, \
         mwl-status, mwl-save, mwl-close, mwl-discard, bundle-open FILE, bundle-edit-file SCRIPT, bundle-render-file SPEC, \
         native-level-open SPEC, native-level-edit-file SCRIPT, native-level-status, \
         native-level-undo, native-level-redo, native-level-save, native-level-close, native-level-discard, \
         native-assets-open-file SPEC, native-assets-edit-file SPEC, native-assets-render-file SPEC, native-assets-status, \
         native-assets-undo, native-assets-redo, native-assets-save, native-assets-close, native-assets-discard, \
         map16-page-open FILE, map16-page-edit-file SCRIPT, map16-page-render-file SPEC, map16-page-status, \
         map16-page-undo, map16-page-redo, map16-page-save, map16-page-close, map16-page-discard, \
         entity-app-open FILE, entity-app-edit-file SCRIPT, entity-app-undo, entity-app-redo, entity-app-status, \
         entity-app-save, entity-app-close, entity-app-discard, \
         world-app-open FILE, world-app-edit-file SCRIPT, world-app-undo, world-app-redo, world-app-status, \
         world-app-save, world-app-close, world-app-discard, \
         bundle-status, bundle-undo, bundle-redo, bundle-save, bundle-close, bundle-discard, map16-set-open FILE, map16-set-edit-file SCRIPT, map16-set-render-file SPEC, map16-set-status, map16-set-undo, map16-set-redo, map16-set-save, map16-set-close, map16-set-discard, world-open-file SPEC, world-edit-file SCRIPT, world-render-file SPEC, world-status, world-undo, world-redo, world-save, world-close, world-discard, gfx-open FILE, gfx-edit-file SCRIPT, gfx-render-file SPEC, gfx-status, gfx-undo, gfx-redo, gfx-save, gfx-close, gfx-discard, pal-open FILE, pal-edit-file SCRIPT, pal-render-file SPEC, pal-status, pal-undo, pal-redo, pal-save, pal-close, pal-discard, ex-open-file SPEC, ex-edit-file SCRIPT, ex-status, ex-undo, ex-redo, ex-save, ex-close, ex-discard, map16-render-file SPEC, overworld-render-file SPEC, map16, graphics HEX, palette HEX, exanimation HEX, layer3 HEX, undo, redo, save, save-as \
         PATH, quit"
    );
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
