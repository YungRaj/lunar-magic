use eframe::egui;
use lm_app::{ToolbarAction, ToolbarConfig, ToolbarItem, UiTextKey};

const ACTIONS: [(ToolbarAction, &str, UiTextKey, &str); 12] = [
    (ToolbarAction::Open, "Open", UiTextKey::FileOpen, "open"),
    (ToolbarAction::Save, "Save", UiTextKey::FileSave, "save"),
    (
        ToolbarAction::SaveAs,
        "Save As",
        UiTextKey::FileSaveAs,
        "save-as",
    ),
    (ToolbarAction::Undo, "Undo", UiTextKey::EditUndo, "undo"),
    (ToolbarAction::Redo, "Redo", UiTextKey::EditRedo, "redo"),
    (ToolbarAction::Copy, "Copy", UiTextKey::EditCopy, "copy"),
    (ToolbarAction::Cut, "Cut", UiTextKey::EditCut, "cut"),
    (ToolbarAction::Paste, "Paste", UiTextKey::EditPaste, "paste"),
    (
        ToolbarAction::ShowOverworld,
        "Show Overworld",
        UiTextKey::ViewOverworld,
        "overworld",
    ),
    (
        ToolbarAction::ShowMap16,
        "Show Map16",
        UiTextKey::ViewMap16,
        "map16",
    ),
    (
        ToolbarAction::LevelBack,
        "Previous Level",
        UiTextKey::ViewLevel,
        "level-back",
    ),
    (
        ToolbarAction::LevelForward,
        "Next Level",
        UiTextKey::ViewLevel,
        "level-forward",
    ),
];

#[derive(Clone)]
enum ToolbarForm {
    Action {
        id: String,
        action: ToolbarAction,
        label: UiTextKey,
    },
    Separator,
}

pub(crate) enum ToolbarEditorResult {
    Apply(ToolbarConfig),
    UseDefault,
}

#[derive(Default)]
pub(crate) struct ToolbarEditor {
    open: bool,
    items: Vec<ToolbarForm>,
    error: Option<String>,
}

impl ToolbarEditor {
    pub(crate) fn open(&mut self, active: Option<&ToolbarConfig>) {
        self.items = active
            .into_iter()
            .flat_map(|config| &config.items)
            .map(|item| match item {
                ToolbarItem::Action { id, action, label } => ToolbarForm::Action {
                    id: id.clone(),
                    action: *action,
                    label: *label,
                },
                ToolbarItem::Separator => ToolbarForm::Separator,
            })
            .collect();
        self.error = None;
        self.open = true;
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<ToolbarEditorResult> {
        if !self.open {
            return None;
        }
        let mut result = None;
        let mut open = self.open;
        egui::Window::new("Customize Toolbar")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(650.0)
            .show(context, |ui| {
                ui.label("Buttons are shown from top to bottom. Text keys follow the active language catalog.");
                if self.items.is_empty() {
                    ui.label("The built-in toolbar is active. Add a button to create a custom layout.");
                }
                ui.separator();
                self.show_items(ui);
                self.show_add_controls(ui);
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                result = self.show_footer(ui);
            });
        self.open &= open;
        result
    }

    fn show_items(&mut self, ui: &mut egui::Ui) {
        let mut operation = None;
        let item_count = self.items.len();
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                for (index, item) in self.items.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        show_item_selectors(ui, index, item);
                        if ui.add_enabled(index > 0, egui::Button::new("↑")).clicked() {
                            operation = Some(Operation::MoveUp(index));
                        }
                        if ui
                            .add_enabled(index + 1 < item_count, egui::Button::new("↓"))
                            .clicked()
                        {
                            operation = Some(Operation::MoveDown(index));
                        }
                        if ui.small_button("Remove").clicked() {
                            operation = Some(Operation::Remove(index));
                        }
                    });
                }
            });
        if let Some(operation) = operation {
            apply_operation(&mut self.items, operation);
        }
    }

    fn show_add_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Add Button").clicked() {
                let action = ToolbarAction::Open;
                self.items.push(ToolbarForm::Action {
                    id: next_id(&self.items, action),
                    action,
                    label: default_label(action),
                });
            }
            if ui.button("Add Separator").clicked() {
                self.items.push(ToolbarForm::Separator);
            }
        });
    }

    fn show_footer(&mut self, ui: &mut egui::Ui) -> Option<ToolbarEditorResult> {
        let mut result = None;
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                match build_config(&self.items) {
                    Ok(config) => {
                        result = Some(ToolbarEditorResult::Apply(config));
                        self.error = None;
                        self.open = false;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui.button("Use Built-in Toolbar").clicked() {
                result = Some(ToolbarEditorResult::UseDefault);
                self.error = None;
                self.open = false;
            }
            if ui.button("Cancel").clicked() {
                self.open = false;
            }
        });
        result
    }
}

fn show_item_selectors(ui: &mut egui::Ui, index: usize, item: &mut ToolbarForm) {
    match item {
        ToolbarForm::Separator => {
            ui.label("Separator");
            ui.add_space(414.0);
        }
        ToolbarForm::Action { action, label, .. } => {
            egui::ComboBox::from_id_salt(("toolbar-action", index))
                .selected_text(action_name(*action))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for (candidate, name, _, _) in ACTIONS {
                        if ui.selectable_value(action, candidate, name).changed() {
                            *label = default_label(candidate);
                        }
                    }
                });
            egui::ComboBox::from_id_salt(("toolbar-label", index))
                .selected_text(crate::frontend_ui::default_text(*label))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for key in UiTextKey::ALL {
                        ui.selectable_value(label, key, crate::frontend_ui::default_text(key));
                    }
                });
        }
    }
}

#[derive(Clone, Copy)]
enum Operation {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

fn apply_operation(items: &mut Vec<ToolbarForm>, operation: Operation) {
    match operation {
        Operation::MoveUp(index) if index > 0 && index < items.len() => {
            items.swap(index - 1, index);
        }
        Operation::MoveDown(index) if index + 1 < items.len() => {
            items.swap(index, index + 1);
        }
        Operation::Remove(index) if index < items.len() => {
            items.remove(index);
        }
        Operation::MoveUp(_) | Operation::MoveDown(_) | Operation::Remove(_) => {}
    }
}

fn build_config(forms: &[ToolbarForm]) -> Result<ToolbarConfig, String> {
    let config = ToolbarConfig {
        items: forms
            .iter()
            .map(|item| match item {
                ToolbarForm::Action { id, action, label } => ToolbarItem::Action {
                    id: id.clone(),
                    action: *action,
                    label: *label,
                },
                ToolbarForm::Separator => ToolbarItem::Separator,
            })
            .collect(),
    };
    config.validate().map_err(|error| error.to_string())?;
    Ok(config)
}

fn next_id(forms: &[ToolbarForm], action: ToolbarAction) -> String {
    let slug = action_slug(action);
    for suffix in 1..=forms.len() + 1 {
        let candidate = format!("native-{slug}-{suffix}");
        if !forms
            .iter()
            .any(|item| matches!(item, ToolbarForm::Action { id, .. } if id == &candidate))
        {
            return candidate;
        }
    }
    unreachable!("one more candidate than existing forms guarantees a free ID")
}

fn action_name(action: ToolbarAction) -> &'static str {
    action_metadata(action).1
}

fn default_label(action: ToolbarAction) -> UiTextKey {
    action_metadata(action).2
}

fn action_slug(action: ToolbarAction) -> &'static str {
    action_metadata(action).3
}

fn action_metadata(
    action: ToolbarAction,
) -> (ToolbarAction, &'static str, UiTextKey, &'static str) {
    ACTIONS
        .iter()
        .copied()
        .find(|(candidate, _, _, _)| *candidate == action)
        .expect("every toolbar action has editor metadata")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: &str, action: ToolbarAction) -> ToolbarForm {
        ToolbarForm::Action {
            id: id.into(),
            action,
            label: default_label(action),
        }
    }

    #[test]
    fn every_action_has_stable_complete_metadata() {
        for (action, name, label, slug) in ACTIONS {
            assert_eq!(action_name(action), name);
            assert_eq!(default_label(action), label);
            assert_eq!(action_slug(action), slug);
        }
    }

    #[test]
    fn staged_forms_preserve_ids_labels_order_and_separators() {
        let forms = [
            action("open-one", ToolbarAction::Open),
            ToolbarForm::Separator,
            ToolbarForm::Action {
                id: "odd-label".into(),
                action: ToolbarAction::Save,
                label: UiTextKey::StatusReady,
            },
        ];
        let config = build_config(&forms).unwrap();
        assert_eq!(
            config.items,
            vec![
                ToolbarItem::Action {
                    id: "open-one".into(),
                    action: ToolbarAction::Open,
                    label: UiTextKey::FileOpen,
                },
                ToolbarItem::Separator,
                ToolbarItem::Action {
                    id: "odd-label".into(),
                    action: ToolbarAction::Save,
                    label: UiTextKey::StatusReady,
                },
            ]
        );
    }

    #[test]
    fn validation_rejects_empty_edge_and_consecutive_separators() {
        assert!(build_config(&[]).is_err());
        assert!(build_config(&[ToolbarForm::Separator]).is_err());
        assert!(
            build_config(&[
                action("open", ToolbarAction::Open),
                ToolbarForm::Separator,
                ToolbarForm::Separator,
                action("save", ToolbarAction::Save),
            ])
            .is_err()
        );
    }

    #[test]
    fn generated_ids_do_not_collide_and_operations_are_bounded() {
        let mut forms = vec![action("native-open-1", ToolbarAction::Open)];
        assert_eq!(next_id(&forms, ToolbarAction::Open), "native-open-2");
        apply_operation(&mut forms, Operation::MoveUp(0));
        apply_operation(&mut forms, Operation::MoveDown(1));
        apply_operation(&mut forms, Operation::Remove(1));
        assert_eq!(forms.len(), 1);
    }
}
