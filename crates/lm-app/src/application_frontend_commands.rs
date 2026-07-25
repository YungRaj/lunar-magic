use crate::{read_bounded_bytes, shell_command, tool_shell, ui_shell};
use lm_app::{AppState, Command, FrontendConfig, ToolbarActivation, file_persistence};
use std::path::Path;

pub(crate) fn execute_ui_command(
    app: &mut AppState,
    command: shell_command::UiCommand,
    allow_in_place_rom_write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        shell_command::UiCommand::Install(path) => install_ui_config(app, &path),
        shell_command::UiCommand::Status => {
            show_ui_status(app);
            Ok(())
        }
        shell_command::UiCommand::Action(action) => activate_ui_action(
            app,
            ui_shell::parse_action(&action)?,
            allow_in_place_rom_write,
        ),
        shell_command::UiCommand::Shortcut(gesture) => {
            let gesture = ui_shell::parse_gesture(&gesture)?;
            let action = app
                .shortcut_action(gesture)
                .ok_or("shortcut is not configured")?;
            activate_ui_action(app, action, allow_in_place_rom_write)
        }
    }
}

pub(crate) fn install_ui_config(
    app: &mut AppState,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = FrontendConfig::decode(&read_bounded_bytes(
        path,
        FrontendConfig::MAX_ENCODED_LEN,
        "frontend configuration",
    )?)?;
    app.set_frontend_config(config)?;
    show_ui_status(app);
    Ok(())
}

fn show_ui_status(app: &AppState) {
    match (app.localization(), app.toolbar(), app.shortcuts()) {
        (Some(localization), Some(toolbar), Some(shortcuts)) => println!(
            "frontend: locale {} — {} toolbar items — {} shortcuts",
            localization.locale(),
            toolbar.items.len(),
            shortcuts.bindings.len()
        ),
        _ => println!("no complete frontend configuration installed"),
    }
}

fn activate_ui_action(
    app: &mut AppState,
    action: lm_app::ToolbarAction,
    allow_in_place_rom_write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let activation = app
        .activate_toolbar_action(action)
        .ok_or("frontend action is currently disabled")?;
    match activation {
        ToolbarActivation::Command(command) => {
            execute_command_activation(app, *command, allow_in_place_rom_write)
        }
        ToolbarActivation::RequestCopyPayload => {
            println!("frontend request: copy selection");
            Ok(())
        }
        ToolbarActivation::RequestCutPayload => {
            println!("frontend request: cut selection");
            Ok(())
        }
        ToolbarActivation::RequestClipboardBytes => {
            println!("frontend request: read clipboard bytes");
            Ok(())
        }
    }
}

fn execute_command_activation(
    app: &mut AppState,
    command: Command,
    allow_in_place_rom_write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Save => save(app, allow_in_place_rom_write),
        command @ (Command::Undo
        | Command::Redo
        | Command::ShowOverworld
        | Command::ShowMap16
        | Command::NavigateLevel(_)) => dispatch_and_print(app, command),
        Command::Open | Command::SaveAs => {
            Err("this frontend action requires a path; use open PATH or save-as PATH".into())
        }
        _ => Err("unsupported parameterized frontend action".into()),
    }
}

pub(crate) fn dispatch_and_print(
    app: &mut AppState,
    command: Command,
) -> Result<(), Box<dyn std::error::Error>> {
    let effects = app.dispatch(command)?;
    println!("{}", app.status);
    tool_shell::print_event_invocations(&effects);
    Ok(())
}

pub(crate) fn save(
    app: &mut AppState,
    allow_in_place_rom_write: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if app.document_path.is_some() && !allow_in_place_rom_write {
        return Err(
            "in-place ROM replacement is disabled; use save-as PATH or restart with --allow-in-place-rom-write"
                .into(),
        );
    }
    let effects = app.dispatch(Command::Save)?;
    let (request_id, path, bytes) = match effects.into_iter().next() {
        Some(lm_app::FrontendEffect::PersistRomAt {
            request_id,
            path,
            bytes,
        }) => (request_id, path, bytes),
        Some(lm_app::FrontendEffect::ChooseSaveDestination { request_id, .. }) => {
            app.cancel_save(request_id)?;
            return Err("document has no path; use save-as PATH".into());
        }
        _ => return Err("application did not provide a save request".into()),
    };
    if let Err(error) = file_persistence::replace_existing(&path, &bytes) {
        app.save_failed(request_id, error.to_string())?;
        return Err(error.into());
    }
    let effects = app.confirm_saved(request_id)?;
    tool_shell::print_event_invocations(&effects);
    Ok(())
}

pub(crate) fn save_as(
    app: &mut AppState,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let effects = app.dispatch(Command::SaveAs)?;
    let save = effects.into_iter().find_map(|effect| match effect {
        lm_app::FrontendEffect::ChooseSaveDestination { request_id, bytes } => {
            Some((request_id, bytes))
        }
        _ => None,
    });
    let (request_id, bytes) = save.ok_or("application did not provide a save snapshot")?;
    if let Err(error) = file_persistence::write_new(destination, &bytes) {
        app.save_failed(request_id, error.to_string())?;
        return Err(error.into());
    }
    let effects = app.confirm_saved_at(request_id, destination)?;
    tool_shell::print_event_invocations(&effects);
    Ok(())
}
