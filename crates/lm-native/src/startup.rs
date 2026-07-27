use crate::{dialogs, profile_loader};
use lm_app::{
    AppState, Command, FrontendConfig, ToolConfig, recent_state_file::RecentStateFile,
    startup_args::StartupOptions,
};

pub(crate) struct InitializedNative {
    pub app: AppState,
    pub recent_state: Option<RecentStateFile>,
}

pub(crate) fn initialize(options: StartupOptions) -> Result<InitializedNative, String> {
    if options.command_script.is_some() {
        return Err(
            "--script is supported by lm-app; the graphical frontend is interactive".into(),
        );
    }
    if options.allow_in_place_rom_write {
        return Err(
            "--allow-in-place-rom-write is unnecessary in the GUI; clicking Save is explicit"
                .into(),
        );
    }
    let mut app = AppState::default();
    let recent_state = options
        .recent_state
        .map(|path| RecentStateFile::load(path, &mut app).map_err(|error| error.to_string()))
        .transpose()?;
    if let Some(path) = options.ui_config {
        let bytes = dialogs::read_regular_bounded(
            &path,
            FrontendConfig::MAX_ENCODED_LEN as u64,
            "frontend configuration",
        )
        .map_err(|error| error.to_string())?;
        app.set_frontend_config(FrontendConfig::decode(&bytes).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.tools_config {
        let bytes = dialogs::read_regular_bounded(
            &path,
            ToolConfig::MAX_ENCODED_LEN as u64,
            "external-tool configuration",
        )
        .map_err(|error| error.to_string())?;
        let config = ToolConfig::decode(&bytes).map_err(|error| error.to_string())?;
        app.set_external_tools(config.tools)
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.rom {
        app.load_rom_at(
            dialogs::read_rom(&path).map_err(|error| error.to_string())?,
            Some(path),
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(level) = options.level {
        app.dispatch(Command::SelectLevel(level))
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.revision_profile {
        let profile = profile_loader::read(&path).map_err(|error| error.to_string())?;
        app.dispatch(Command::InstallRevisionProfile(Box::new(profile)))
            .map_err(|error| error.to_string())?;
    }
    Ok(InitializedNative { app, recent_state })
}

pub(crate) const HELP: &str = "usage: lm-native [ROM] [--rom ROM] [--level HEX] [--profile FILE] \
    [--ui-config FILE] [--tools-config FILE] [--recent-state FILE]";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_startup_produces_a_closed_application() {
        let initialized = initialize(StartupOptions::default()).unwrap();
        assert!(initialized.app.controller_snapshot().is_err());
        assert!(initialized.recent_state.is_none());
    }

    #[test]
    fn terminal_only_startup_modes_are_rejected_explicitly() {
        let scripted = StartupOptions {
            command_script: Some("commands.txt".into()),
            ..StartupOptions::default()
        };
        assert!(initialize(scripted).err().unwrap().contains("--script"));
        let in_place = StartupOptions {
            allow_in_place_rom_write: true,
            ..StartupOptions::default()
        };
        assert!(
            initialize(in_place)
                .err()
                .unwrap()
                .contains("--allow-in-place-rom-write")
        );
    }
}
