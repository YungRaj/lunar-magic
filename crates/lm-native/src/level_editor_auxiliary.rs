use crate::level_editor_forms::{Map16OverrideForm, ScreenExitForm, SecondaryExitForm};
use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
use lm_level::{CompleteLevelFile, LevelAuxiliaryEdit, Map16OverrideEdit, SequenceEdit};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Collection {
    #[default]
    ScreenExits,
    SecondaryExits,
    Map16Overrides,
}

#[derive(Default)]
pub(crate) struct LevelAuxiliaryPanelState {
    collection: Collection,
    screen_index: usize,
    screen: ScreenExitForm,
    screen_key: Option<(u64, usize)>,
    secondary_index: usize,
    secondary: SecondaryExitForm,
    secondary_key: Option<(u64, usize)>,
    map16_index: usize,
    map16: Map16OverrideForm,
    map16_key: Option<(u64, usize)>,
}

impl LevelAuxiliaryPanelState {
    pub(crate) fn invalidate(&mut self) {
        self.screen_key = None;
        self.secondary_key = None;
        self.map16_key = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.collection,
                Collection::ScreenExits,
                "Screen exits",
            );
            ui.selectable_value(
                &mut self.collection,
                Collection::SecondaryExits,
                "Secondary exits",
            );
            ui.selectable_value(
                &mut self.collection,
                Collection::Map16Overrides,
                "Map16 overrides",
            );
        });
        ui.separator();
        match self.collection {
            Collection::ScreenExits => self.show_screen_exits(ui, level, revision),
            Collection::SecondaryExits => self.show_secondary_exits(ui, level, revision),
            Collection::Map16Overrides => self.show_map16(ui, level, revision),
        }
    }

    fn show_screen_exits(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.screen_exits;
        normalize_index(&mut self.screen_index, values.len());
        index_slider(ui, &mut self.screen_index, values.len(), "Screen exit");
        let key = (revision, self.screen_index);
        if self.screen_key != Some(key) {
            self.screen = values
                .get(self.screen_index)
                .copied()
                .map_or_else(ScreenExitForm::default, ScreenExitForm::load);
            self.screen_key = Some(key);
        }
        ui.horizontal(|ui| {
            ui.label("Encoded value (hex)");
            ui.text_edit_singleline(&mut self.screen.encoded);
        });
        let operation = sequence_buttons(ui, !values.is_empty());
        operation.map(|operation| {
            let sequence = match operation {
                Operation::Append => SequenceEdit::Insert {
                    index: values.len(),
                    value: self.screen.parse()?,
                },
                Operation::Replace => SequenceEdit::Replace {
                    index: self.screen_index,
                    value: self.screen.parse()?,
                },
                Operation::Remove => SequenceEdit::Remove {
                    index: self.screen_index,
                },
            };
            Ok(vec![CompleteLevelDocumentEdit::Auxiliary(
                LevelAuxiliaryEdit::ScreenExit(sequence),
            )])
        })
    }

    fn show_secondary_exits(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.secondary_exits;
        normalize_index(&mut self.secondary_index, values.len());
        index_slider(
            ui,
            &mut self.secondary_index,
            values.len(),
            "Secondary exit",
        );
        let key = (revision, self.secondary_index);
        if self.secondary_key != Some(key) {
            self.secondary = values
                .get(self.secondary_index)
                .copied()
                .map_or_else(SecondaryExitForm::default, SecondaryExitForm::load);
            self.secondary_key = Some(key);
        }
        secondary_fields(ui, &mut self.secondary);
        let operation = sequence_buttons(ui, !values.is_empty());
        operation.map(|operation| {
            let sequence = match operation {
                Operation::Append => SequenceEdit::Insert {
                    index: values.len(),
                    value: self.secondary.parse()?,
                },
                Operation::Replace => SequenceEdit::Replace {
                    index: self.secondary_index,
                    value: self.secondary.parse()?,
                },
                Operation::Remove => SequenceEdit::Remove {
                    index: self.secondary_index,
                },
            };
            Ok(vec![CompleteLevelDocumentEdit::Auxiliary(
                LevelAuxiliaryEdit::SecondaryExit(sequence),
            )])
        })
    }

    fn show_map16(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.map16_overrides;
        normalize_index(&mut self.map16_index, values.len());
        index_slider(ui, &mut self.map16_index, values.len(), "Override");
        let key = (revision, self.map16_index);
        if self.map16_key != Some(key) {
            self.map16 = values
                .get(self.map16_index)
                .copied()
                .map_or_else(Map16OverrideForm::default, |(index, tile)| {
                    Map16OverrideForm::load(index, tile)
                });
            self.map16_key = Some(key);
        }
        map16_fields(ui, &mut self.map16);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui.button("Upsert").clicked();
            remove = ui
                .add_enabled(!values.is_empty(), egui::Button::new("Remove selected"))
                .clicked();
        });
        if upsert {
            Some(self.map16.parse().map(|(index, tile)| {
                vec![CompleteLevelDocumentEdit::Auxiliary(
                    LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Upsert { index, tile }),
                )]
            }))
        } else if remove {
            let index = values[self.map16_index].0;
            Some(Ok(vec![CompleteLevelDocumentEdit::Auxiliary(
                LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Remove { index }),
            )]))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Append,
    Replace,
    Remove,
}

fn sequence_buttons(ui: &mut egui::Ui, populated: bool) -> Option<Operation> {
    let mut operation = None;
    ui.horizontal(|ui| {
        if ui.button("Append").clicked() {
            operation = Some(Operation::Append);
        }
        if ui
            .add_enabled(populated, egui::Button::new("Replace"))
            .clicked()
        {
            operation = Some(Operation::Replace);
        }
        if ui
            .add_enabled(populated, egui::Button::new("Remove"))
            .clicked()
        {
            operation = Some(Operation::Remove);
        }
    });
    operation
}

fn normalize_index(index: &mut usize, len: usize) {
    if len > 0 {
        *index = (*index).min(len - 1);
    } else {
        *index = 0;
    }
}

fn index_slider(ui: &mut egui::Ui, index: &mut usize, len: usize, label: &str) {
    ui.add(egui::Slider::new(index, 0..=len.saturating_sub(1)).text(label));
}

fn secondary_fields(ui: &mut egui::Ui, form: &mut SecondaryExitForm) {
    for (label, field) in [
        ("Destination (hex)", &mut form.destination),
        ("Position/method (hex)", &mut form.position),
        ("Screen (hex)", &mut form.screen),
        ("X (hex)", &mut form.x),
        ("Y (hex)", &mut form.y),
        ("Destination flags (hex)", &mut form.destination_flags),
        ("X/overworld flags (hex)", &mut form.x_flags),
        ("Additional flags (hex)", &mut form.additional),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
}

fn map16_fields(ui: &mut egui::Ui, form: &mut Map16OverrideForm) {
    for (label, field) in [
        ("Index (hex)", &mut form.index),
        ("Top left (hex)", &mut form.top_left),
        ("Top right (hex)", &mut form.top_right),
        ("Bottom left (hex)", &mut form.bottom_left),
        ("Bottom right (hex)", &mut form.bottom_right),
        ("Acts Like (hex)", &mut form.acts_like),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
}
