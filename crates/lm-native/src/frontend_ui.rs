use eframe::egui;
use lm_app::{
    AppState, ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolbarActivation, ToolbarItem,
    UiTextKey,
};

pub(crate) fn show_toolbar(ui: &mut egui::Ui, app: &AppState) -> Option<ToolbarActivation> {
    let toolbar = app.toolbar()?;
    let mut activation = None;
    ui.horizontal_wrapped(|ui| {
        for item in &toolbar.items {
            match item {
                ToolbarItem::Separator => {
                    ui.separator();
                }
                ToolbarItem::Action { action, label, .. } => {
                    let text = app
                        .localization()
                        .map_or_else(|| default_text(*label), |catalog| catalog.text(*label));
                    let response = ui
                        .add_enabled(app.toolbar_action_enabled(*action), egui::Button::new(text));
                    if response.clicked() {
                        activation = app.activate_toolbar_action(*action);
                    }
                }
            }
        }
    });
    activation
}

pub(crate) const fn default_text(key: UiTextKey) -> &'static str {
    match key {
        UiTextKey::AppTitle => "Lunar Magic Rust",
        UiTextKey::FileOpen => "Open",
        UiTextKey::FileSave => "Save",
        UiTextKey::FileSaveAs => "Save As",
        UiTextKey::FileClose => "Close",
        UiTextKey::FileQuit => "Quit",
        UiTextKey::EditUndo => "Undo",
        UiTextKey::EditRedo => "Redo",
        UiTextKey::EditCopy => "Copy",
        UiTextKey::EditCut => "Cut",
        UiTextKey::EditPaste => "Paste",
        UiTextKey::ViewLevel => "Level",
        UiTextKey::ViewOverworld => "Overworld",
        UiTextKey::ViewMap16 => "Map16",
        UiTextKey::ViewGraphics => "Graphics",
        UiTextKey::ViewPalette => "Palette",
        UiTextKey::ViewExAnimation => "ExAnimation",
        UiTextKey::StatusReady => "Ready",
        UiTextKey::ViewLayer3 => "Layer 3",
    }
}

pub(crate) fn shortcut_activation(
    context: &egui::Context,
    app: &AppState,
) -> Option<ToolbarActivation> {
    context.input(|input| {
        input.events.iter().find_map(|event| {
            let gesture = match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => ShortcutGesture {
                    modifiers: translate_modifiers(*modifiers),
                    key: translate_key(*key)?,
                },
                egui::Event::Text(text) => ShortcutGesture {
                    modifiers: translate_modifiers(input.modifiers),
                    key: translate_text_key(text)?,
                },
                _ => return None,
            };
            app.shortcut_action(gesture)
                .and_then(|action| app.activate_toolbar_action(action))
        })
    })
}

fn translate_text_key(text: &str) -> Option<ShortcutKey> {
    let mut characters = text.chars();
    let character = characters.next()?;
    (characters.next().is_none() && !character.is_control())
        .then_some(ShortcutKey::Character(character))
}

fn translate_modifiers(modifiers: egui::Modifiers) -> ShortcutModifiers {
    let mut result = ShortcutModifiers::default();
    if modifiers.command {
        result = result.union(ShortcutModifiers::PRIMARY);
    }
    if modifiers.shift {
        result = result.union(ShortcutModifiers::SHIFT);
    }
    if modifiers.alt {
        result = result.union(ShortcutModifiers::ALT);
    }
    if modifiers.ctrl && !modifiers.command {
        result = result.union(ShortcutModifiers::SECONDARY);
    }
    result
}

fn translate_key(key: egui::Key) -> Option<ShortcutKey> {
    use egui::Key;
    match key {
        Key::Backspace => Some(ShortcutKey::Backspace),
        Key::Delete => Some(ShortcutKey::Delete),
        Key::Enter => Some(ShortcutKey::Enter),
        Key::Escape => Some(ShortcutKey::Escape),
        Key::ArrowLeft => Some(ShortcutKey::ArrowLeft),
        Key::ArrowRight => Some(ShortcutKey::ArrowRight),
        Key::ArrowUp => Some(ShortcutKey::ArrowUp),
        Key::ArrowDown => Some(ShortcutKey::ArrowDown),
        _ => translate_alphanumeric_key(key).or_else(|| translate_function_key(key)),
    }
}

fn translate_alphanumeric_key(key: egui::Key) -> Option<ShortcutKey> {
    use egui::Key;
    let character = match key {
        Key::A => 'a',
        Key::B => 'b',
        Key::C => 'c',
        Key::D => 'd',
        Key::E => 'e',
        Key::F => 'f',
        Key::G => 'g',
        Key::H => 'h',
        Key::I => 'i',
        Key::J => 'j',
        Key::K => 'k',
        Key::L => 'l',
        Key::M => 'm',
        Key::N => 'n',
        Key::O => 'o',
        Key::P => 'p',
        Key::Q => 'q',
        Key::R => 'r',
        Key::S => 's',
        Key::T => 't',
        Key::U => 'u',
        Key::V => 'v',
        Key::W => 'w',
        Key::X => 'x',
        Key::Y => 'y',
        Key::Z => 'z',
        Key::Num0 => '0',
        Key::Num1 => '1',
        Key::Num2 => '2',
        Key::Num3 => '3',
        Key::Num4 => '4',
        Key::Num5 => '5',
        Key::Num6 => '6',
        Key::Num7 => '7',
        Key::Num8 => '8',
        Key::Num9 => '9',
        _ => return None,
    };
    Some(ShortcutKey::Character(character))
}

fn translate_function_key(key: egui::Key) -> Option<ShortcutKey> {
    use egui::Key;
    Some(match key {
        Key::F1 => ShortcutKey::Function(1),
        Key::F2 => ShortcutKey::Function(2),
        Key::F3 => ShortcutKey::Function(3),
        Key::F4 => ShortcutKey::Function(4),
        Key::F5 => ShortcutKey::Function(5),
        Key::F6 => ShortcutKey::Function(6),
        Key::F7 => ShortcutKey::Function(7),
        Key::F8 => ShortcutKey::Function(8),
        Key::F9 => ShortcutKey::Function(9),
        Key::F10 => ShortcutKey::Function(10),
        Key::F11 => ShortcutKey::Function(11),
        Key::F12 => ShortcutKey::Function(12),
        Key::F13 => ShortcutKey::Function(13),
        Key::F14 => ShortcutKey::Function(14),
        Key::F15 => ShortcutKey::Function(15),
        Key::F16 => ShortcutKey::Function(16),
        Key::F17 => ShortcutKey::Function(17),
        Key::F18 => ShortcutKey::Function(18),
        Key::F19 => ShortcutKey::Function(19),
        Key::F20 => ShortcutKey::Function(20),
        Key::F21 => ShortcutKey::Function(21),
        Key::F22 => ShortcutKey::Function(22),
        Key::F23 => ShortcutKey::Function(23),
        Key::F24 => ShortcutKey::Function(24),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_translation_covers_portable_named_boundaries() {
        assert_eq!(
            translate_key(egui::Key::F24),
            Some(ShortcutKey::Function(24))
        );
        assert_eq!(
            translate_key(egui::Key::ArrowLeft),
            Some(ShortcutKey::ArrowLeft)
        );
        assert_eq!(
            translate_key(egui::Key::A),
            Some(ShortcutKey::Character('a'))
        );
        assert_eq!(translate_key(egui::Key::F25), None);
    }

    #[test]
    fn modifier_translation_keeps_primary_and_secondary_distinct() {
        let primary = translate_modifiers(egui::Modifiers {
            command: true,
            ctrl: true,
            ..Default::default()
        });
        assert!(primary.contains(ShortcutModifiers::PRIMARY));
        assert!(!primary.contains(ShortcutModifiers::SECONDARY));
        let secondary = translate_modifiers(egui::Modifiers {
            ctrl: true,
            ..Default::default()
        });
        assert!(secondary.contains(ShortcutModifiers::SECONDARY));
    }

    #[test]
    fn text_translation_accepts_one_unicode_scalar_only() {
        assert_eq!(translate_text_key("界"), Some(ShortcutKey::Character('界')));
        assert_eq!(translate_text_key("ab"), None);
        assert_eq!(translate_text_key("\n"), None);
    }

    #[test]
    fn english_fallback_covers_every_typed_localization_key() {
        for key in UiTextKey::ALL {
            assert!(
                !default_text(key).is_empty(),
                "missing fallback for {key:?}"
            );
        }
        assert_eq!(default_text(UiTextKey::FileSaveAs), "Save As");
        assert_eq!(default_text(UiTextKey::ViewLayer3), "Layer 3");
    }
}
