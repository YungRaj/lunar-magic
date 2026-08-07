use super::NativeApplication;
use crate::frontend_ui;
use eframe::egui;
use lm_app::{
    Command, LevelNavigationDirection, ToolInvocation, ToolbarActivation, UserToolbarButton,
    UserToolbarTarget,
};

impl NativeApplication {
    pub(super) fn toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        if self.app.toolbar().is_some() {
            if let Some(activation) = frontend_ui::show_toolbar(ui, &self.app) {
                self.handle_frontend_activation(context, activation);
            }
        } else {
            self.default_toolbar(context, ui);
        }
        if self
            .user_toolbar
            .as_ref()
            .is_some_and(|toolbar| toolbar.toolbar_visible())
        {
            ui.separator();
            self.user_toolbar(context, ui);
        }
    }

    fn user_toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        // Clone the compact descriptors so dispatch can mutably borrow the application.
        let buttons = self
            .user_toolbar
            .as_ref()
            .map(|toolbar| toolbar.buttons.clone())
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (index, button) in buttons.iter().enumerate() {
                match &button.target {
                    UserToolbarTarget::Spacer => {
                        ui.separator();
                    }
                    target => {
                        let label = if button.tooltip.is_empty() {
                            user_toolbar_label(target)
                        } else {
                            button.tooltip.lines().next().unwrap_or("Tool")
                        };
                        if ui.button(label).on_hover_text(&button.tooltip).clicked() {
                            self.activate_user_toolbar_button(context, index, button);
                        }
                    }
                }
            }
        });
    }

    fn activate_user_toolbar_button(
        &mut self,
        context: &egui::Context,
        index: usize,
        button: &UserToolbarButton,
    ) {
        match &button.target {
            UserToolbarTarget::Spacer => {}
            UserToolbarTarget::Internal(name) => match user_toolbar_command(name) {
                Some(command) => self.dispatch(context, command),
                None => {
                    self.effects.error = Some(format!(
                        "User toolbar command {name:?} is not supported by this editor yet"
                    ))
                }
            },
            UserToolbarTarget::External(command_line) => match split_command_line(command_line) {
                Ok((executable, arguments)) => {
                    let expanded = arguments
                        .iter()
                        .map(|value| expand_lm_placeholders(value, &self.app))
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|arguments| {
                            Ok(ToolInvocation {
                                tool_id: format!("usertoolbar-{index}"),
                                executable: expand_lm_placeholders(&executable, &self.app)?.into(),
                                arguments,
                                working_directory: button
                                    .working_directory
                                    .as_deref()
                                    .map(|value| {
                                        expand_lm_placeholders(value, &self.app).map(Into::into)
                                    })
                                    .transpose()?,
                            })
                        });
                    match expanded {
                        Ok(invocation) => {
                            if let Err(error) = self.effects.external_tools.enqueue(invocation) {
                                self.effects.error = Some(error);
                            }
                        }
                        Err(error) => self.effects.error = Some(error.to_string()),
                    }
                }
                Err(error) => self.effects.error = Some(error),
            },
        }
    }

    fn handle_frontend_activation(
        &mut self,
        context: &egui::Context,
        activation: ToolbarActivation,
    ) {
        match activation {
            ToolbarActivation::Command(command) => self.dispatch(context, *command),
            ToolbarActivation::RequestCopyPayload
            | ToolbarActivation::RequestCutPayload
            | ToolbarActivation::RequestClipboardBytes => {
                self.effects.error = Some(
                    "The active native editor has not supplied a typed clipboard payload".into(),
                );
            }
        }
    }

    fn default_toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        let capabilities = self.app.capabilities();
        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                self.dispatch(context, Command::Open);
            }
            if ui
                .add_enabled(capabilities.can_save(), egui::Button::new("Save"))
                .clicked()
            {
                self.dispatch(context, Command::Save);
            }
            ui.separator();
            for (label, enabled, command) in [
                ("Undo", capabilities.history.undo, Command::Undo),
                ("Redo", capabilities.history.redo, Command::Redo),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    self.dispatch(context, command);
                }
            }
            ui.separator();
            for (label, enabled, direction) in [
                (
                    "Back",
                    capabilities.navigation.level_back,
                    LevelNavigationDirection::Back,
                ),
                (
                    "Forward",
                    capabilities.navigation.level_forward,
                    LevelNavigationDirection::Forward,
                ),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    self.dispatch(context, Command::NavigateLevel(direction));
                }
            }
            ui.label("Level");
            let response = ui.add_sized(
                [55.0, 22.0],
                egui::TextEdit::singleline(&mut self.level_text),
            );
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                match u16::from_str_radix(self.level_text.trim(), 16) {
                    Ok(level) => self.dispatch(context, Command::SelectLevel(level)),
                    Err(error) => {
                        self.effects.error = Some(format!("invalid hexadecimal level: {error}"));
                    }
                }
            }
        });
    }

    pub(super) fn handle_shortcuts(&mut self, context: &egui::Context) {
        if let Some(activation) = frontend_ui::shortcut_activation(context, &self.app) {
            self.handle_frontend_activation(context, activation);
        }
    }
}

fn expand_lm_placeholders(value: &str, app: &lm_app::AppState) -> Result<String, String> {
    let exe_directory = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
    let rom = app.document_path.as_deref();
    let rom_directory = rom.and_then(std::path::Path::parent);
    let rom_name = rom
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str);
    let rom_stem = rom
        .and_then(std::path::Path::file_stem)
        .and_then(std::ffi::OsStr::to_str);
    let replacements = [
        ("%1", rom.map(|path| path.display().to_string())),
        ("%2", rom_directory.map(directory_with_separator)),
        ("%3", rom_name.map(str::to_owned)),
        ("%4", exe_directory.as_deref().map(directory_with_separator)),
        ("%5", rom_stem.map(str::to_owned)),
        ("%7", app.current_level().map(|level| format!("{level:X}"))),
        ("%8", Some(env!("CARGO_PKG_VERSION").replace('.', ""))),
    ];
    let mut output = value.to_owned();
    for (placeholder, replacement) in replacements {
        if output.contains(placeholder) {
            let replacement = replacement.ok_or_else(|| {
                format!("user toolbar placeholder {placeholder} requires an open ROM")
            })?;
            output = output.replace(placeholder, &replacement);
        }
    }
    if output.contains("%9") {
        return Err(
            "user toolbar LM request-window placeholder %9 has no native equivalent".into(),
        );
    }
    Ok(output)
}

fn directory_with_separator(path: &std::path::Path) -> String {
    let mut value = path.display().to_string();
    if !value.ends_with(std::path::MAIN_SEPARATOR) {
        value.push(std::path::MAIN_SEPARATOR);
    }
    value
}

fn user_toolbar_label(target: &UserToolbarTarget) -> &str {
    match target {
        UserToolbarTarget::Spacer => "",
        UserToolbarTarget::Internal(name) => name.strip_prefix("LM_").unwrap_or(name),
        UserToolbarTarget::External(_) => "External Tool",
    }
}

fn user_toolbar_command(name: &str) -> Option<Command> {
    Some(match name {
        "LM_FILE_OPEN_ROM" => Command::Open,
        "LM_FILE_SAVE_BUTTON" | "LM_FILE_SAVE_FILE" => Command::Save,
        "LM_FILE_SAVE_FILE_AS" | "LM_FILE_SAVE_LEVEL_TO_ROM_AS" => Command::SaveAs,
        "LM_FILE_PREVIOUS_LEVEL" => Command::NavigateLevel(LevelNavigationDirection::Back),
        "LM_FILE_NEXT_LEVEL" => Command::NavigateLevel(LevelNavigationDirection::Forward),
        "LM_EDIT_UNDO" => Command::Undo,
        "LM_EDIT_REDO" => Command::Redo,
        "LM_VIEW_OVERWORLD" => Command::ShowOverworld,
        "LM_VIEW_16x16" => Command::ShowMap16,
        _ => return None,
    })
}

fn split_command_line(value: &str) -> Result<(String, Vec<String>), String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for character in value.chars() {
        match character {
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            value => word.push(value),
        }
    }
    if quoted {
        return Err("user toolbar external command has an unterminated quote".into());
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut words = words.into_iter();
    let executable = words
        .next()
        .ok_or_else(|| "user toolbar external command is empty".to_owned())?;
    Ok((executable, words.collect()))
}

#[cfg(test)]
mod user_toolbar_tests {
    use super::*;

    #[test]
    fn external_command_line_preserves_quoted_arguments() {
        assert_eq!(
            split_command_line(r#""tool path.exe" "a b" plain"#).unwrap(),
            ("tool path.exe".into(), vec!["a b".into(), "plain".into()])
        );
        assert!(split_command_line("\"unfinished").is_err());
    }

    #[test]
    fn original_internal_names_map_to_native_commands() {
        assert_eq!(
            user_toolbar_command("LM_FILE_OPEN_ROM"),
            Some(Command::Open)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_OVERWORLD"),
            Some(Command::ShowOverworld)
        );
        assert_eq!(user_toolbar_command("LM_UNKNOWN"), None);
    }

    #[test]
    fn original_path_placeholders_expand_without_a_shell() {
        let mut app = lm_app::AppState::default();
        app.document_path = Some(std::path::PathBuf::from("/tmp/rom dir/game.smc"));
        assert_eq!(
            expand_lm_placeholders("%1|%2|%3|%5", &app).unwrap(),
            "/tmp/rom dir/game.smc|/tmp/rom dir/|game.smc|game"
        );
        assert!(expand_lm_placeholders("%9", &app).is_err());
        app.document_path = None;
        assert!(expand_lm_placeholders("%1", &app).is_err());
    }
}
