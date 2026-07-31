use eframe::egui;
use lm_app::{
    ShortcutBinding, ShortcutConfig, ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolbarAction,
};
use std::collections::BTreeSet;

const ACTIONS: [(ToolbarAction, &str); 12] = [
    (ToolbarAction::Open, "Open"),
    (ToolbarAction::Save, "Save"),
    (ToolbarAction::SaveAs, "Save As"),
    (ToolbarAction::Undo, "Undo"),
    (ToolbarAction::Redo, "Redo"),
    (ToolbarAction::Copy, "Copy"),
    (ToolbarAction::Cut, "Cut"),
    (ToolbarAction::Paste, "Paste"),
    (ToolbarAction::ShowOverworld, "Show Overworld"),
    (ToolbarAction::ShowMap16, "Show Map16"),
    (ToolbarAction::LevelBack, "Previous Level"),
    (ToolbarAction::LevelForward, "Next Level"),
];

#[derive(Clone)]
struct BindingForm {
    gesture: String,
    action: ToolbarAction,
}

#[derive(Default)]
pub(crate) struct ShortcutEditor {
    open: bool,
    bindings: Vec<BindingForm>,
    error: Option<String>,
}

impl ShortcutEditor {
    pub(crate) fn open(&mut self, active: Option<&ShortcutConfig>) {
        self.bindings = active
            .into_iter()
            .flat_map(|config| &config.bindings)
            .map(|binding| BindingForm {
                gesture: format_gesture(binding.gesture),
                action: binding.action,
            })
            .collect();
        self.error = None;
        self.open = true;
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<ShortcutConfig> {
        if !self.open {
            return None;
        }
        let mut result = None;
        let mut open = self.open;
        egui::Window::new("Keyboard Shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(context, |ui| {
                ui.label(
                    "Use portable gestures such as primary+s, primary+shift+s, alt+f4, or escape.",
                );
                ui.label("Primary means Command on macOS and Ctrl on other platforms.");
                ui.separator();

                let mut remove = None;
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (index, binding) in self.bindings.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [190.0, 22.0],
                                    egui::TextEdit::singleline(&mut binding.gesture),
                                );
                                egui::ComboBox::from_id_salt(("shortcut-action", index))
                                    .selected_text(action_label(binding.action))
                                    .width(165.0)
                                    .show_ui(ui, |ui| {
                                        for (action, label) in ACTIONS {
                                            ui.selectable_value(&mut binding.action, action, label);
                                        }
                                    });
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(index);
                                }
                            });
                        }
                    });
                if let Some(index) = remove {
                    self.bindings.remove(index);
                }
                if ui.button("Add shortcut").clicked() {
                    self.bindings.push(BindingForm {
                        gesture: String::new(),
                        action: ToolbarAction::Open,
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        match build_config(&self.bindings) {
                            Ok(config) => {
                                result = Some(config);
                                self.error = None;
                                self.open = false;
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                    if ui.button("Clear All").clicked() {
                        self.bindings.clear();
                        self.error = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });
        self.open &= open;
        result
    }
}

fn build_config(forms: &[BindingForm]) -> Result<ShortcutConfig, String> {
    let mut gestures = BTreeSet::new();
    let mut bindings = Vec::with_capacity(forms.len());
    for (index, form) in forms.iter().enumerate() {
        let gesture = parse_gesture(form.gesture.trim())
            .map_err(|error| format!("Shortcut {}: {error}", index + 1))?;
        if !gestures.insert(gesture) {
            return Err(format!(
                "Shortcut {} duplicates an earlier gesture.",
                index + 1
            ));
        }
        bindings.push(ShortcutBinding {
            gesture,
            action: form.action,
        });
    }
    let config = ShortcutConfig { bindings };
    config.validate().map_err(|error| error.to_string())?;
    Ok(config)
}

fn parse_gesture(value: &str) -> Result<ShortcutGesture, String> {
    if value.is_empty() {
        return Err("enter a gesture or remove the row".into());
    }
    let mut modifiers = ShortcutModifiers::default();
    let mut seen_modifiers = BTreeSet::new();
    let mut key = None;
    for raw_token in value.split('+') {
        let token = raw_token.trim();
        let normalized = token.to_ascii_lowercase();
        let modifier = match normalized.as_str() {
            "primary" => Some(ShortcutModifiers::PRIMARY),
            "secondary" => Some(ShortcutModifiers::SECONDARY),
            "shift" => Some(ShortcutModifiers::SHIFT),
            "alt" => Some(ShortcutModifiers::ALT),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if !seen_modifiers.insert(normalized.clone()) {
                return Err(format!("modifier {normalized:?} appears more than once"));
            }
            modifiers = modifiers.union(modifier);
        } else {
            let parsed = parse_key(token)?;
            if key.replace(parsed).is_some() {
                return Err("a gesture must contain exactly one key".into());
            }
        }
    }
    Ok(ShortcutGesture {
        modifiers,
        key: key.ok_or_else(|| "a gesture must contain one key".to_owned())?,
    })
}

fn parse_key(value: &str) -> Result<ShortcutKey, String> {
    let normalized = value.to_ascii_lowercase();
    let named = match normalized.as_str() {
        "backspace" => Some(ShortcutKey::Backspace),
        "delete" => Some(ShortcutKey::Delete),
        "enter" => Some(ShortcutKey::Enter),
        "escape" => Some(ShortcutKey::Escape),
        "left" => Some(ShortcutKey::ArrowLeft),
        "right" => Some(ShortcutKey::ArrowRight),
        "up" => Some(ShortcutKey::ArrowUp),
        "down" => Some(ShortcutKey::ArrowDown),
        _ => None,
    };
    if let Some(key) = named {
        return Ok(key);
    }
    if let Some(number) = normalized.strip_prefix('f') {
        return number
            .parse::<u8>()
            .ok()
            .filter(|number| (1..=24).contains(number))
            .map(ShortcutKey::Function)
            .ok_or_else(|| format!("unknown key {value:?}"));
    }
    let mut characters = value.chars();
    let character = characters
        .next()
        .filter(|character| characters.next().is_none() && !character.is_whitespace())
        .ok_or_else(|| format!("unknown key {value:?}"))?;
    Ok(ShortcutKey::Character(character))
}

fn format_gesture(gesture: ShortcutGesture) -> String {
    let mut parts = Vec::new();
    for (modifier, label) in [
        (ShortcutModifiers::PRIMARY, "primary"),
        (ShortcutModifiers::SECONDARY, "secondary"),
        (ShortcutModifiers::SHIFT, "shift"),
        (ShortcutModifiers::ALT, "alt"),
    ] {
        if gesture.modifiers.contains(modifier) {
            parts.push(label.to_owned());
        }
    }
    parts.push(match gesture.key {
        ShortcutKey::Character(character) => character.to_string(),
        ShortcutKey::Function(number) => format!("f{number}"),
        ShortcutKey::Backspace => "backspace".into(),
        ShortcutKey::Delete => "delete".into(),
        ShortcutKey::Enter => "enter".into(),
        ShortcutKey::Escape => "escape".into(),
        ShortcutKey::ArrowLeft => "left".into(),
        ShortcutKey::ArrowRight => "right".into(),
        ShortcutKey::ArrowUp => "up".into(),
        ShortcutKey::ArrowDown => "down".into(),
    });
    parts.join("+")
}

fn action_label(action: ToolbarAction) -> &'static str {
    ACTIONS
        .iter()
        .find_map(|(candidate, label)| (*candidate == action).then_some(*label))
        .expect("every toolbar action has a shortcut-editor label")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_text_round_trips_every_key_family() {
        for value in [
            "primary+shift+s",
            "secondary+alt+f24",
            "backspace",
            "delete",
            "enter",
            "escape",
            "left",
            "right",
            "up",
            "down",
            "primary+É",
        ] {
            let gesture = parse_gesture(value).unwrap();
            assert_eq!(parse_gesture(&format_gesture(gesture)).unwrap(), gesture);
        }
    }

    #[test]
    fn configuration_rejects_empty_invalid_and_duplicate_rows() {
        let form = |gesture: &str| BindingForm {
            gesture: gesture.into(),
            action: ToolbarAction::Save,
        };
        assert!(build_config(&[form("")]).is_err());
        assert!(build_config(&[form("primary+f25")]).is_err());
        assert!(build_config(&[form("primary+s"), form("primary+s")]).is_err());
        assert!(build_config(&[form("primary+primary+s")]).is_err());
    }

    #[test]
    fn configuration_preserves_order_and_allows_multiple_actions() {
        let forms = [
            BindingForm {
                gesture: "primary+s".into(),
                action: ToolbarAction::Save,
            },
            BindingForm {
                gesture: "primary+shift+s".into(),
                action: ToolbarAction::SaveAs,
            },
        ];
        let config = build_config(&forms).unwrap();
        assert_eq!(config.bindings.len(), 2);
        assert_eq!(config.bindings[0].action, ToolbarAction::Save);
        assert_eq!(config.bindings[1].action, ToolbarAction::SaveAs);
    }

    #[test]
    fn every_action_has_a_stable_label() {
        for (action, expected) in ACTIONS {
            assert_eq!(action_label(action), expected);
        }
    }
}
