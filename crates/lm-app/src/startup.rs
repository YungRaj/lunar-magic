use crate::application_frontend_commands::install_ui_config;
use crate::application_rom_commands::{MAX_ROM_BYTES, install_profile, read_bounded_bytes};
use crate::application_tool_commands::show_tool_status;
use lm_app::{AppState, ToolConfig, recent_state_file, startup_args};

pub(super) struct InitializedShell {
    pub(super) recent_state: Option<recent_state_file::RecentStateFile>,
    pub(super) command_lines: Option<Vec<String>>,
    pub(super) allow_in_place_rom_write: bool,
}

pub(super) fn initialize(
    app: &mut AppState,
) -> Result<Option<InitializedShell>, Box<dyn std::error::Error>> {
    let options = startup_args::StartupOptions::parse(std::env::args_os().skip(1))?;
    if options.help {
        print_startup_help();
        return Ok(None);
    }
    let command_lines = options
        .command_script
        .as_deref()
        .map(crate::command_script::load)
        .transpose()?;
    let mut recent_state = options
        .recent_state
        .map(|path| recent_state_file::RecentStateFile::load(path, app))
        .transpose()?;
    if let Some(path) = options.ui_config {
        install_ui_config(app, &path)?;
    }
    if let Some(path) = options.tools_config {
        let config = ToolConfig::decode(&read_bounded_bytes(
            &path,
            ToolConfig::MAX_ENCODED_LEN,
            "external-tool configuration",
        )?)?;
        app.set_external_tools(config.tools)?;
        show_tool_status(app);
    }
    if let Some(path) = options.rom {
        app.load_rom_at(
            read_bounded_bytes(&path, MAX_ROM_BYTES, "ROM")?,
            Some(path.clone()),
        )?;
        let mut recent = app.recent_documents().clone();
        recent.note(path);
        app.set_recent_documents(recent);
        println!("{}", app.status);
    } else {
        println!("No ROM open. Use: open PATH");
    }
    if let Some(path) = options.revision_profile {
        install_profile(app, &path)?;
    }
    if let Some(state) = recent_state.as_mut() {
        state.persist_if_changed(app)?;
    }
    if options.allow_in_place_rom_write {
        println!("warning: in-place ROM replacement is explicitly enabled");
    }
    Ok(Some(InitializedShell {
        recent_state,
        command_lines,
        allow_in_place_rom_write: options.allow_in_place_rom_write,
    }))
}

fn print_startup_help() {
    println!(
        "usage: lm-app [ROM] [--rom ROM] [--profile FILE] [--ui-config FILE] \
         [--tools-config FILE] [--recent-state FILE] [--script FILE] \
         [--allow-in-place-rom-write]\n\
         ROM may be supplied either positionally or with --rom, but not both. Use -- before a \
         positional ROM whose name begins with '-'. In-place save is disabled by default; use \
         save-as PATH for create-new publication."
    );
}
