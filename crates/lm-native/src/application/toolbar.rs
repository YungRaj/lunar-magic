use super::{LevelViewVisibility, NativeApplication};
use crate::frontend_ui;
use eframe::egui;
use lm_app::{
    Command, LevelNavigationDirection, ShortcutGesture, ShortcutKey, ShortcutModifiers,
    ToolInvocation, ToolbarActivation, UserToolbarButton, UserToolbarTarget,
};

impl NativeApplication {
    pub(super) fn handle_user_toolbar_document_change(&mut self, context: &egui::Context) {
        let current = self.app.document_path.clone();
        if current == self.user_toolbar_observed_document {
            return;
        }
        self.user_toolbar_observed_document = current.clone();
        if current.is_none() {
            return;
        }
        let Some(toolbar) = self.user_toolbar.as_ref() else {
            return;
        };
        let close = toolbar_lifecycle_indexes(
            toolbar,
            "LM_CLOSE_ON_NEW_ROM",
            "LM_CLOSE_ON_NEW_ROM_FORCE_ALL",
        );
        let autorun_disabled = toolbar.global_options.iter().any(|option| {
            matches!(option, lm_app::UserToolbarGlobalOption::Flag(value) if value == "LM_NO_AUTORUN")
        });
        let autorun = if autorun_disabled {
            Vec::new()
        } else {
            toolbar_button_indexes_with_option(toolbar, "LM_AUTORUN_ON_NEW_ROM")
        };
        for index in close {
            self.effects
                .external_tools
                .stop_tool(&format!("usertoolbar-{index}"));
        }
        let buttons = autorun
            .into_iter()
            .filter_map(|index| {
                self.user_toolbar
                    .as_ref()?
                    .buttons
                    .get(index)
                    .cloned()
                    .map(|button| (index, button))
            })
            .collect::<Vec<_>>();
        for (index, button) in buttons {
            self.activate_user_toolbar_button(context, index, &button);
        }
    }

    pub(super) fn stop_user_toolbar_tools_on_close(&mut self) {
        let Some(toolbar) = self.user_toolbar.as_ref() else {
            return;
        };
        for index in
            toolbar_lifecycle_indexes(toolbar, "LM_CLOSE_ON_CLOSE", "LM_CLOSE_ON_CLOSE_FORCE_ALL")
        {
            self.effects
                .external_tools
                .stop_tool(&format!("usertoolbar-{index}"));
        }
    }

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
        self.user_toolbar_images.ensure_textures(context);
        let icon_size = self.user_toolbar_images.icon_size().unwrap_or(16.0);
        let icons = buttons
            .iter()
            .enumerate()
            .map(|(index, _)| {
                self.user_toolbar
                    .as_ref()
                    .and_then(|toolbar| self.user_toolbar_images.texture_for(toolbar, index))
                    .cloned()
            })
            .collect::<Vec<_>>();
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, (button, icon)) in buttons.iter().zip(&icons).enumerate() {
                if button.options.iter().any(|option| option == "LM_NO_BUTTON") {
                    continue;
                }
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
                        let widget = icon.as_ref().map_or_else(
                            || egui::Button::new(label),
                            |texture| {
                                let image = egui::Image::new((
                                    texture.id(),
                                    egui::vec2(icon_size, icon_size),
                                ));
                                if button.tooltip.is_empty() {
                                    egui::Button::image(image)
                                } else {
                                    egui::Button::image_and_text(image, label)
                                }
                            },
                        );
                        if ui.add(widget).on_hover_text(&button.tooltip).clicked() {
                            clicked = Some(index);
                        }
                    }
                }
            }
        });
        if let Some(index) = clicked {
            self.activate_user_toolbar_button(context, index, &buttons[index]);
        }
    }

    fn activate_user_toolbar_button(
        &mut self,
        context: &egui::Context,
        index: usize,
        button: &UserToolbarButton,
    ) {
        match &button.target {
            UserToolbarTarget::Spacer => {}
            UserToolbarTarget::Internal(name) => {
                if let Some(action) = user_toolbar_local_action(name) {
                    self.apply_user_toolbar_local_action(action);
                    return;
                }
                match user_toolbar_command(name, self.app.current_level()) {
                    Some(command) => self.dispatch(context, command),
                    None => {
                        self.effects.error = Some(format!(
                            "User toolbar command {name:?} is not supported by this editor yet"
                        ))
                    }
                }
            }
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
                                working_directory: external_working_directory(
                                    &executable,
                                    button,
                                    &self.app,
                                )?,
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

    fn apply_user_toolbar_local_action(&mut self, action: UserToolbarLocalAction) {
        if self.app.current_level().is_none() {
            self.effects.error =
                Some("The user-toolbar view command requires an open level".into());
            return;
        }
        match action {
            UserToolbarLocalAction::ZoomToggle => self.vanilla_level_editor.toolbar_zoom_toggle(),
            UserToolbarLocalAction::ZoomDefault => self.vanilla_level_editor.toolbar_zoom_default(),
            UserToolbarLocalAction::ZoomPlus => self
                .vanilla_level_editor
                .toolbar_zoom_adjust(ROM_LEVEL_TOOLBAR_ZOOM_STEP),
            UserToolbarLocalAction::ZoomMinus => self
                .vanilla_level_editor
                .toolbar_zoom_adjust(-ROM_LEVEL_TOOLBAR_ZOOM_STEP),
            _ => toggle_user_toolbar_view_state(
                &mut self.level_view_visibility,
                &mut self.special_world_passed,
                action,
            ),
        }
        self.vanilla_level_editor.invalidate_graphics_preview();
        self.rom_level_assets_editor.invalidate_graphics_preview();
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
        self.main_toolbar_images.ensure_textures(context);
        let icon_size = self.main_toolbar_images.icon_size();
        let open_icon = self.main_toolbar_images.texture(1).cloned();
        let save_icon = self.main_toolbar_images.texture(3).cloned();
        let undo_icon = self.main_toolbar_images.texture(5).cloned();
        let redo_icon = self.main_toolbar_images.texture(6).cloned();
        ui.horizontal(|ui| {
            if toolbar_button(ui, "Open", true, open_icon.as_ref(), icon_size).clicked() {
                self.dispatch(context, Command::Open);
            }
            if toolbar_button(
                ui,
                "Save",
                capabilities.can_save(),
                save_icon.as_ref(),
                icon_size,
            )
            .clicked()
            {
                self.dispatch(context, Command::Save);
            }
            ui.separator();
            for (label, enabled, command, icon) in [
                (
                    "Undo",
                    capabilities.history.undo,
                    Command::Undo,
                    undo_icon.as_ref(),
                ),
                (
                    "Redo",
                    capabilities.history.redo,
                    Command::Redo,
                    redo_icon.as_ref(),
                ),
            ] {
                if toolbar_button(ui, label, enabled, icon, icon_size).clicked() {
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

fn toolbar_button_indexes_with_option(toolbar: &lm_app::UserToolbar, option: &str) -> Vec<usize> {
    toolbar
        .buttons
        .iter()
        .enumerate()
        .filter_map(|(index, button)| {
            button
                .options
                .iter()
                .any(|value| value == option)
                .then_some(index)
        })
        .collect()
}

fn toolbar_lifecycle_indexes(
    toolbar: &lm_app::UserToolbar,
    option: &str,
    force_option: &str,
) -> Vec<usize> {
    if toolbar.global_options.iter().any(
        |value| matches!(value, lm_app::UserToolbarGlobalOption::Flag(flag) if flag == force_option),
    ) {
        return (0..toolbar.buttons.len()).collect();
    }
    toolbar_button_indexes_with_option(toolbar, option)
}

fn external_working_directory(
    executable: &str,
    button: &UserToolbarButton,
    app: &lm_app::AppState,
) -> Result<Option<std::path::PathBuf>, String> {
    if let Some(value) = button.working_directory.as_deref() {
        return expand_lm_placeholders(value, app).map(|value| Some(value.into()));
    }
    if button.options.iter().any(|option| option == "LM_DIR_ROM") {
        return app
            .document_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
            .map(Some)
            .ok_or_else(|| "LM_DIR_ROM requires an open ROM".into());
    }
    if button.options.iter().any(|option| option == "LM_DIR_LM") {
        return std::env::current_exe()
            .map_err(|error| format!("cannot locate application executable: {error}"))
            .and_then(|path| {
                path.parent()
                    .map(std::path::Path::to_path_buf)
                    .map(Some)
                    .ok_or_else(|| "application executable has no parent directory".into())
            });
    }
    Ok(std::path::Path::new(executable)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf))
}

fn toolbar_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    texture: Option<&egui::TextureHandle>,
    size: f32,
) -> egui::Response {
    let button = texture.map_or_else(
        || egui::Button::new(label),
        |texture| egui::Button::image(egui::Image::new((texture.id(), egui::vec2(size, size)))),
    );
    ui.add_enabled(enabled, button).on_hover_text(label)
}

fn toggle_user_toolbar_view_state(
    visibility: &mut LevelViewVisibility,
    special_world_passed: &mut bool,
    action: UserToolbarLocalAction,
) {
    match action {
        UserToolbarLocalAction::Layer1 => visibility.layer1 = !visibility.layer1,
        UserToolbarLocalAction::Layer2 => visibility.layer2 = !visibility.layer2,
        UserToolbarLocalAction::Layer3 => visibility.layer3 = !visibility.layer3,
        UserToolbarLocalAction::Sprites => visibility.sprites = !visibility.sprites,
        UserToolbarLocalAction::SpecialWorld => *special_world_passed = !*special_world_passed,
        UserToolbarLocalAction::ZoomToggle
        | UserToolbarLocalAction::ZoomDefault
        | UserToolbarLocalAction::ZoomPlus
        | UserToolbarLocalAction::ZoomMinus => {
            unreachable!("zoom actions are routed through the level editor")
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

fn user_toolbar_command(name: &str, current_level: Option<u16>) -> Option<Command> {
    Some(match name {
        "LM_FILE_OPEN_ROM" => Command::Open,
        "LM_FILE_SAVE_BUTTON" | "LM_FILE_SAVE_FILE" => Command::Save,
        "LM_FILE_SAVE_FILE_AS" | "LM_FILE_SAVE_LEVEL_TO_ROM_AS" => Command::SaveAs,
        "LM_FILE_PREVIOUS_LEVEL" => Command::NavigateLevel(LevelNavigationDirection::Back),
        "LM_FILE_NEXT_LEVEL" => Command::NavigateLevel(LevelNavigationDirection::Forward),
        "LM_FILE_EXIT" => Command::Quit,
        "LM_EDIT_UNDO" => Command::Undo,
        "LM_EDIT_REDO" => Command::Redo,
        "LM_VIEW_OVERWORLD" => Command::ShowOverworld,
        "LM_VIEW_16x16" => Command::ShowMap16,
        "LM_VIEW_8x8" => Command::ShowGraphics(0),
        "LM_VIEW_PALETTES" => Command::ShowPalette(0),
        "LM_KEY_EXANIM_SLOTS" => Command::ShowExAnimation(current_level.unwrap_or(0)),
        "LM_VIEW_LAYER_3_EDITOR" => Command::ShowLayer3(current_level?),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UserToolbarLocalAction {
    Layer1,
    Layer2,
    Layer3,
    Sprites,
    SpecialWorld,
    ZoomToggle,
    ZoomDefault,
    ZoomPlus,
    ZoomMinus,
}

const ROM_LEVEL_TOOLBAR_ZOOM_STEP: i16 = 100;

fn user_toolbar_local_action(name: &str) -> Option<UserToolbarLocalAction> {
    Some(match name {
        "LM_VIEW_LAYER_1" => UserToolbarLocalAction::Layer1,
        "LM_VIEW_LAYER_2" => UserToolbarLocalAction::Layer2,
        "LM_VIEW_LAYER_3" => UserToolbarLocalAction::Layer3,
        "LM_VIEW_SPRITES" => UserToolbarLocalAction::Sprites,
        "LM_VIEW_SPECIAL_WORLD" => UserToolbarLocalAction::SpecialWorld,
        "LM_VIEW_ZOOM_TOGGLE" => UserToolbarLocalAction::ZoomToggle,
        "LM_VIEW_ZOOM_DEFAULT" => UserToolbarLocalAction::ZoomDefault,
        "LM_VIEW_ZOOM_PLUS" => UserToolbarLocalAction::ZoomPlus,
        "LM_VIEW_ZOOM_MINUS" => UserToolbarLocalAction::ZoomMinus,
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
            user_toolbar_command("LM_FILE_OPEN_ROM", None),
            Some(Command::Open)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_OVERWORLD", None),
            Some(Command::ShowOverworld)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_8x8", Some(0x105)),
            Some(Command::ShowGraphics(0))
        );
        assert_eq!(
            user_toolbar_command("LM_KEY_EXANIM_SLOTS", Some(0x105)),
            Some(Command::ShowExAnimation(0x105))
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_LAYER_3_EDITOR", Some(0x106)),
            Some(Command::ShowLayer3(0x106))
        );
        assert_eq!(user_toolbar_command("LM_VIEW_LAYER_3_EDITOR", None), None);
        assert_eq!(user_toolbar_command("LM_UNKNOWN", None), None);
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_LAYER_1"),
            Some(UserToolbarLocalAction::Layer1)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SPECIAL_WORLD"),
            Some(UserToolbarLocalAction::SpecialWorld)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_TOGGLE"),
            Some(UserToolbarLocalAction::ZoomToggle)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_DEFAULT"),
            Some(UserToolbarLocalAction::ZoomDefault)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_PLUS"),
            Some(UserToolbarLocalAction::ZoomPlus)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_MINUS"),
            Some(UserToolbarLocalAction::ZoomMinus)
        );
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

    #[test]
    fn external_working_directory_matches_original_defaults_and_rom_override() {
        let app = lm_app::AppState::default();
        let program =
            lm_app::UserToolbar::parse("***START***\n\"/opt/tools/editor\" --flag\n***END***")
                .unwrap();
        assert_eq!(
            external_working_directory("/opt/tools/editor", &program.buttons[0], &app).unwrap(),
            Some(std::path::PathBuf::from("/opt/tools"))
        );
        let mut app = lm_app::AppState::default();
        app.document_path = Some(std::path::PathBuf::from("/tmp/roms/game.smc"));
        let rom = lm_app::UserToolbar::parse(
            "***START***\n\"editor\"\nLM_DEFAULT\nLM_DIR_ROM\n***END***",
        )
        .unwrap();
        assert_eq!(
            external_working_directory("editor", &rom.buttons[0], &app).unwrap(),
            Some(std::path::PathBuf::from("/tmp/roms"))
        );
    }

    #[test]
    fn lifecycle_option_selection_is_exact_and_global_no_autorun_is_retained() {
        let toolbar = lm_app::UserToolbar::parse(
            "LM_NO_AUTORUN\n***START***\n\"one\"\nLM_DEFAULT\nLM_AUTORUN_ON_NEW_ROM,LM_CLOSE_ON_CLOSE\n***START***\n\"two\"\nLM_DEFAULT\nLM_CLOSE_ON_NEW_ROM\n***END***",
        )
        .unwrap();
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_AUTORUN_ON_NEW_ROM"),
            [0]
        );
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_CLOSE_ON_CLOSE"),
            [0]
        );
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_CLOSE_ON_NEW_ROM"),
            [1]
        );
        assert!(toolbar.global_options.iter().any(|option| {
            matches!(option, lm_app::UserToolbarGlobalOption::Flag(value) if value == "LM_NO_AUTORUN")
        }));
        let forced = lm_app::UserToolbar::parse(
            "LM_CLOSE_ON_CLOSE_FORCE_ALL\n***START***\n\"one\"\n***START***\n\"two\"\n***END***",
        )
        .unwrap();
        assert_eq!(
            toolbar_lifecycle_indexes(&forced, "LM_CLOSE_ON_CLOSE", "LM_CLOSE_ON_CLOSE_FORCE_ALL"),
            [0, 1]
        );
    }

    #[test]
    fn new_document_transition_enqueues_autorun_once_through_permission_gate() {
        let mut native = NativeApplication::default();
        native.user_toolbar = Some(
            lm_app::UserToolbar::parse(
                "***START***\n\"/usr/bin/true\"\nLM_DEFAULT\nLM_AUTORUN_ON_NEW_ROM\n***END***",
            )
            .unwrap(),
        );
        native.app.document_path = Some(std::path::PathBuf::from("/tmp/game.smc"));
        let context = egui::Context::default();
        native.handle_user_toolbar_document_change(&context);
        assert_eq!(
            native.effects.external_tools.pending_tool_ids(),
            ["usertoolbar-0"]
        );
        native.handle_user_toolbar_document_change(&context);
        assert_eq!(
            native.effects.external_tools.pending_tool_ids(),
            ["usertoolbar-0"]
        );
    }

    #[test]
    fn local_view_actions_toggle_the_same_state_consumed_by_level_rendering() {
        let mut visibility = LevelViewVisibility::default();
        let mut special_world = false;
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::Layer1,
        );
        assert!(!visibility.layer1);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::SpecialWorld,
        );
        assert!(special_world);
        let mut native = NativeApplication::default();
        native.apply_user_toolbar_local_action(UserToolbarLocalAction::Sprites);
        assert!(native.level_view_visibility.sprites);
        assert!(native.effects.error.is_some());
    }
}
