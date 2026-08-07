use super::NativeApplication;
use crate::frontend_ui;
use eframe::egui;
use lm_app::{
    Command, LevelNavigationDirection, ShortcutGesture, ShortcutKey, ShortcutModifiers,
    ToolInvocation, ToolbarActivation, UserToolbarButton, UserToolbarTarget,
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
        let gestures = frontend_ui::shortcut_gestures(context);
        let matching = self.user_toolbar.as_ref().map_or_else(Vec::new, |toolbar| {
            matching_user_toolbar_buttons(toolbar, &gestures)
        });
        if !matching.is_empty() {
            // Lunar Magic lets duplicate user assignments all fire and suppresses its built-in
            // shortcut whenever at least one user-toolbar assignment matches.
            for (index, button) in matching {
                self.activate_user_toolbar_button(context, index, &button);
            }
            return;
        }
        if let Some(activation) = frontend_ui::shortcut_activation(context, &self.app) {
            self.handle_frontend_activation(context, activation);
        }
    }
}

fn matching_user_toolbar_buttons(
    toolbar: &lm_app::UserToolbar,
    gestures: &[ShortcutGesture],
) -> Vec<(usize, UserToolbarButton)> {
    toolbar
        .buttons
        .iter()
        .enumerate()
        .filter(|(_, button)| {
            user_toolbar_shortcut(&button.shortcut)
                .is_some_and(|candidate| gestures.contains(&candidate))
        })
        .map(|(index, button)| (index, button.clone()))
        .collect()
}

fn user_toolbar_shortcut(tokens: &[String]) -> Option<ShortcutGesture> {
    let mut modifiers = ShortcutModifiers::default();
    let mut key = None;
    for token in tokens {
        match token.as_str() {
            "VK_CONTROL" | "VK_LCONTROL" | "VK_RCONTROL" => {
                modifiers = modifiers.union(ShortcutModifiers::SECONDARY);
            }
            "VK_SHIFT" | "VK_LSHIFT" | "VK_RSHIFT" => {
                modifiers = modifiers.union(ShortcutModifiers::SHIFT);
            }
            "VK_ALT" | "VK_LALT" | "VK_RALT" => {
                modifiers = modifiers.union(ShortcutModifiers::ALT);
            }
            value => {
                if key.is_some() {
                    return None;
                }
                key = parse_user_toolbar_key(value);
                key?;
            }
        }
    }
    Some(ShortcutGesture {
        modifiers,
        key: key?,
    })
}

fn parse_user_toolbar_key(value: &str) -> Option<ShortcutKey> {
    if let Some(character) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .and_then(|value| {
            let mut characters = value.chars();
            let character = characters.next()?;
            characters.next().is_none().then_some(character)
        })
    {
        return Some(ShortcutKey::Character(character.to_ascii_lowercase()));
    }
    if let Some(number) = value.strip_prefix("VK_F") {
        let number = number.parse::<u8>().ok()?;
        return (1..=24)
            .contains(&number)
            .then_some(ShortcutKey::Function(number));
    }
    Some(match value {
        "VK_INSERT" => ShortcutKey::Insert,
        "VK_DELETE" => ShortcutKey::Delete,
        "VK_HOME" => ShortcutKey::Home,
        "VK_END" => ShortcutKey::End,
        "VK_PAGEUP" => ShortcutKey::PageUp,
        "VK_PAGEDOWN" => ShortcutKey::PageDown,
        "VK_ESCAPE" => ShortcutKey::Escape,
        "VK_TAB" => ShortcutKey::Tab,
        "VK_BACK" => ShortcutKey::Backspace,
        "VK_RETURN" | "VK_NUMPAD_ENTER" => ShortcutKey::Enter,
        "VK_UP" => ShortcutKey::ArrowUp,
        "VK_DOWN" => ShortcutKey::ArrowDown,
        "VK_LEFT" => ShortcutKey::ArrowLeft,
        "VK_RIGHT" => ShortcutKey::ArrowRight,
        "VK_SPACE" => ShortcutKey::Space,
        value if value.starts_with("VK_NUMPAD") && value.len() == 10 => {
            ShortcutKey::Character(value.chars().last()?)
        }
        value if value.starts_with("0x") || value.starts_with("0X") => {
            virtual_key(u8::from_str_radix(&value[2..], 16).ok()?)?
        }
        _ => return None,
    })
}

fn virtual_key(value: u8) -> Option<ShortcutKey> {
    Some(match value {
        0x08 => ShortcutKey::Backspace,
        0x09 => ShortcutKey::Tab,
        0x0d => ShortcutKey::Enter,
        0x1b => ShortcutKey::Escape,
        0x20 => ShortcutKey::Space,
        0x21 => ShortcutKey::PageUp,
        0x22 => ShortcutKey::PageDown,
        0x23 => ShortcutKey::End,
        0x24 => ShortcutKey::Home,
        0x25 => ShortcutKey::ArrowLeft,
        0x26 => ShortcutKey::ArrowUp,
        0x27 => ShortcutKey::ArrowRight,
        0x28 => ShortcutKey::ArrowDown,
        0x2d => ShortcutKey::Insert,
        0x2e => ShortcutKey::Delete,
        0x30..=0x39 | 0x41..=0x5a => ShortcutKey::Character(char::from(value).to_ascii_lowercase()),
        0x70..=0x87 => ShortcutKey::Function(value - 0x6f),
        _ => return None,
    })
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

    #[test]
    fn original_user_shortcut_tokens_cover_modifiers_named_and_numeric_keys() {
        assert_eq!(
            user_toolbar_shortcut(&["'o'".into(), "VK_CONTROL".into(), "VK_SHIFT".into()]),
            Some(ShortcutGesture {
                modifiers: ShortcutModifiers::SECONDARY.union(ShortcutModifiers::SHIFT),
                key: ShortcutKey::Character('o'),
            })
        );
        assert_eq!(
            parse_user_toolbar_key("VK_F24"),
            Some(ShortcutKey::Function(24))
        );
        assert_eq!(
            parse_user_toolbar_key("VK_PAGEUP"),
            Some(ShortcutKey::PageUp)
        );
        assert_eq!(parse_user_toolbar_key("0x2E"), Some(ShortcutKey::Delete));
        assert_eq!(
            parse_user_toolbar_key("0x41"),
            Some(ShortcutKey::Character('a'))
        );
        assert_eq!(parse_user_toolbar_key("VK_PAUSE"), None);
        assert!(user_toolbar_shortcut(&["'a'".into(), "'b'".into()]).is_none());
    }

    #[test]
    fn hidden_toolbar_shortcuts_match_and_duplicate_assignments_all_survive() {
        let mut toolbar = lm_app::UserToolbar::parse(include_str!(
            "../../../../docs/oracle-work/lm363/user-toolbar/usertoolbar.txt"
        ))
        .unwrap();
        assert!(!toolbar.toolbar_visible());
        toolbar.buttons.push(toolbar.buttons[1].clone());
        let gesture = user_toolbar_shortcut(&toolbar.buttons[1].shortcut).unwrap();
        let matches = matching_user_toolbar_buttons(&toolbar, &[gesture]);
        assert_eq!(
            matches.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [1, 3]
        );
    }
}
