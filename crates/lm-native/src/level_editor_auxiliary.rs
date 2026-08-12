use crate::level_editor_forms::{Map16OverrideForm, ScreenExitForm, SecondaryExitForm};
use eframe::egui;
use lm_app::{CompleteLevelDocumentEdit, ExtendedUiTextKey as Key, LocalizationCatalog};
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.collection,
                Collection::ScreenExits,
                aux_text(catalog, Key::LevelAuxScreenExits),
            );
            ui.selectable_value(
                &mut self.collection,
                Collection::SecondaryExits,
                aux_text(catalog, Key::LevelAuxSecondaryExits),
            );
            ui.selectable_value(
                &mut self.collection,
                Collection::Map16Overrides,
                aux_text(catalog, Key::LevelAuxMap16Overrides),
            );
        });
        ui.separator();
        match self.collection {
            Collection::ScreenExits => self.show_screen_exits(ui, level, revision, catalog),
            Collection::SecondaryExits => self.show_secondary_exits(ui, level, revision, catalog),
            Collection::Map16Overrides => self.show_map16(ui, level, revision, catalog),
        }
    }

    fn show_screen_exits(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.screen_exits;
        normalize_index(&mut self.screen_index, values.len());
        index_slider(
            ui,
            &mut self.screen_index,
            values.len(),
            aux_text(catalog, Key::LevelAuxScreenExit),
        );
        let key = (revision, self.screen_index);
        if self.screen_key != Some(key) {
            self.screen = values
                .get(self.screen_index)
                .copied()
                .map_or_else(ScreenExitForm::default, ScreenExitForm::load);
            self.screen_key = Some(key);
        }
        ui.horizontal(|ui| {
            ui.label(aux_text(catalog, Key::LevelAuxEncodedValue));
            ui.text_edit_singleline(&mut self.screen.encoded);
        });
        let operation = sequence_buttons(ui, !values.is_empty(), catalog);
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.secondary_exits;
        normalize_index(&mut self.secondary_index, values.len());
        index_slider(
            ui,
            &mut self.secondary_index,
            values.len(),
            aux_text(catalog, Key::LevelAuxSecondaryExit),
        );
        let key = (revision, self.secondary_index);
        if self.secondary_key != Some(key) {
            self.secondary = values
                .get(self.secondary_index)
                .copied()
                .map_or_else(SecondaryExitForm::default, SecondaryExitForm::load);
            self.secondary_key = Some(key);
        }
        secondary_fields(ui, &mut self.secondary, catalog);
        let operation = sequence_buttons(ui, !values.is_empty(), catalog);
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.map16_overrides;
        normalize_index(&mut self.map16_index, values.len());
        index_slider(
            ui,
            &mut self.map16_index,
            values.len(),
            aux_text(catalog, Key::LevelAuxOverride),
        );
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
        map16_fields(ui, &mut self.map16, catalog);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui.button(aux_text(catalog, Key::LevelAuxUpsert)).clicked();
            remove = ui
                .add_enabled(
                    !values.is_empty(),
                    egui::Button::new(aux_text(catalog, Key::LevelAuxRemoveSelected)),
                )
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

fn sequence_buttons(
    ui: &mut egui::Ui,
    populated: bool,
    catalog: Option<&LocalizationCatalog>,
) -> Option<Operation> {
    let mut operation = None;
    ui.horizontal(|ui| {
        if ui.button(aux_text(catalog, Key::LevelAuxAppend)).clicked() {
            operation = Some(Operation::Append);
        }
        if ui
            .add_enabled(
                populated,
                egui::Button::new(aux_text(catalog, Key::LevelAuxReplace)),
            )
            .clicked()
        {
            operation = Some(Operation::Replace);
        }
        if ui
            .add_enabled(
                populated,
                egui::Button::new(aux_text(catalog, Key::LevelAuxRemove)),
            )
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

fn index_slider(ui: &mut egui::Ui, index: &mut usize, len: usize, label: String) {
    ui.add(egui::Slider::new(index, 0..=len.saturating_sub(1)).text(label));
}

fn secondary_fields(
    ui: &mut egui::Ui,
    form: &mut SecondaryExitForm,
    catalog: Option<&LocalizationCatalog>,
) {
    for (label, field) in [
        (
            aux_text(catalog, Key::LevelAuxDestination),
            &mut form.destination,
        ),
        (
            aux_text(catalog, Key::LevelAuxPositionMethod),
            &mut form.position,
        ),
        (aux_text(catalog, Key::LevelAuxScreen), &mut form.screen),
        (aux_text(catalog, Key::LevelAuxX), &mut form.x),
        (aux_text(catalog, Key::LevelAuxY), &mut form.y),
        (
            aux_text(catalog, Key::LevelAuxDestinationFlags),
            &mut form.destination_flags,
        ),
        (
            aux_text(catalog, Key::LevelAuxXOverworldFlags),
            &mut form.x_flags,
        ),
        (
            aux_text(catalog, Key::LevelAuxAdditionalFlags),
            &mut form.additional,
        ),
    ] {
        ui.horizontal(|ui| {
            ui.label(&label);
            ui.text_edit_singleline(field);
        });
    }
}

fn map16_fields(
    ui: &mut egui::Ui,
    form: &mut Map16OverrideForm,
    catalog: Option<&LocalizationCatalog>,
) {
    for (label, field) in [
        (aux_text(catalog, Key::LevelAuxIndex), &mut form.index),
        (aux_text(catalog, Key::LevelAuxTopLeft), &mut form.top_left),
        (
            aux_text(catalog, Key::LevelAuxTopRight),
            &mut form.top_right,
        ),
        (
            aux_text(catalog, Key::LevelAuxBottomLeft),
            &mut form.bottom_left,
        ),
        (
            aux_text(catalog, Key::LevelAuxBottomRight),
            &mut form.bottom_right,
        ),
        (
            aux_text(catalog, Key::LevelAuxActsLike),
            &mut form.acts_like,
        ),
    ] {
        ui.horizontal(|ui| {
            ui.label(&label);
            ui.text_edit_singleline(field);
        });
    }
}

fn aux_text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn complete_level_auxiliary_panel_has_no_literal_widget_text() {
        let source = include_str!("level_editor_auxiliary.rs");
        for literal_widget in ["ui.button(\"", "ui.label(\"", "Button::new(\"", ".text(\""] {
            assert!(
                !source.contains(literal_widget),
                "level auxiliary panel bypasses typed localization with {literal_widget}"
            );
        }
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LevelAux"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}")),
                "level auxiliary panel does not consume {key:?}"
            );
        }
    }
}
