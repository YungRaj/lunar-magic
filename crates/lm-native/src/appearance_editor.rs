use crate::{
    appearance_editor_form::AppearanceForm,
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
};
use eframe::egui;
use lm_app::{
    EntityAppearanceDocumentController, EntityAppearanceDocumentEdit, ExtendedUiTextKey,
    LocalizationCatalog,
};
use lm_level::EntityAppearanceFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct AppearanceEditor {
    controller: Option<EntityAppearanceDocumentController>,
    index: usize,
    form: AppearanceForm,
    form_key: Option<(u64, usize)>,
    insert_index: usize,
    move_before: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl AppearanceEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_entity_appearance_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(EntityAppearanceFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "entity appearance document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for appearance loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for appearance persistence to finish before closing".into());
            return false;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "appearance loader returned no file".to_string())?;
                EntityAppearanceDocumentController::decode(path, &bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.invalidate();
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() {
            self.clamp_indices();
            egui::Window::new(text(catalog, ExtendedUiTextKey::AppearanceEditorTitle))
                .default_size([650.0, 500.0])
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let len = controller.value().appearances.len();
        ui.label(
            text(catalog, ExtendedUiTextKey::AppearancePainterRecordsFormat)
                .replace("{count}", &len.to_string()),
        );
        ui.add(
            egui::Slider::new(&mut self.index, 0..=len.saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::AppearanceSelected)),
        );
        if self.form_key != Some((controller.revision(), self.index)) {
            self.form = controller
                .value()
                .appearances
                .get(self.index)
                .copied()
                .map_or_else(AppearanceForm::default, AppearanceForm::load);
            self.form_key = Some((controller.revision(), self.index));
        }
        appearance_fields(ui, &mut self.form, catalog);
        ui.separator();
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    len > 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceReplaceSelected)),
                )
                .clicked()
            {
                edit = Some(
                    self.form
                        .parse()
                        .map(|value| EntityAppearanceDocumentEdit::Replace {
                            index: self.index,
                            value,
                        }),
                );
            }
            if ui
                .add_enabled(
                    len > 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceRemoveSelected)),
                )
                .clicked()
            {
                edit = Some(Ok(EntityAppearanceDocumentEdit::Remove {
                    index: self.index,
                }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.insert_index).range(0..=len));
            if ui
                .button(text(catalog, ExtendedUiTextKey::AppearanceInsertBefore))
                .clicked()
            {
                edit = Some(
                    self.form
                        .parse()
                        .map(|value| EntityAppearanceDocumentEdit::Insert {
                            index: self.insert_index,
                            value,
                        }),
                );
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.move_before).range(0..=len.saturating_sub(1)));
            if ui
                .add_enabled(
                    len > 1,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceMoveBefore)),
                )
                .clicked()
            {
                edit = Some(Ok(EntityAppearanceDocumentEdit::MoveBefore {
                    from: self.index,
                    before: self.move_before,
                }));
            }
        });
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply_edit(&edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let (can_undo, can_redo, modified) = (
            controller.can_undo(),
            controller.can_redo(),
            controller.is_modified(),
        );
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::AppearanceSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::AppearanceModified
                } else {
                    ExtendedUiTextKey::AppearanceSaved
                },
            ));
        });
        let mut changed = false;
        if let Some(controller) = self.controller.as_mut() {
            if let Some(undo) = history {
                let result = if undo {
                    controller.undo(controller.revision())
                } else {
                    controller.redo(controller.revision())
                };
                match result {
                    Ok(value) => changed = value,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn apply_edit(&mut self, edit: &EntityAppearanceDocumentEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn clamp_indices(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let len = controller.value().appearances.len();
        self.index = clamp(self.index, len);
        self.insert_index = self.insert_index.min(len);
        self.move_before = clamp(self.move_before, len);
    }

    fn invalidate(&mut self) {
        self.form_key = None;
    }

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::AppearanceDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::AppearanceUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::AppearanceCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::AppearanceDiscard))
                        .clicked()
                    {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::AppearanceErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::AppearanceOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_close = None;
        self.invalidate();
    }
}

fn appearance_fields(
    ui: &mut egui::Ui,
    form: &mut AppearanceForm,
    catalog: Option<&LocalizationCatalog>,
) {
    let source_names = [
        ExtendedUiTextKey::AppearanceSourceLayer1,
        ExtendedUiTextKey::AppearanceSourceLayer2,
        ExtendedUiTextKey::AppearanceSourceSprite,
    ]
    .map(|key| text(catalog, key));
    egui::ComboBox::from_id_salt("appearance-source-kind")
        .selected_text(&source_names[form.source_kind.min(2)])
        .show_ui(ui, |ui| {
            for (index, name) in source_names.iter().enumerate() {
                ui.selectable_value(&mut form.source_kind, index, name);
            }
        });
    for (key, field) in [
        (
            ExtendedUiTextKey::AppearanceSourceIdHex,
            &mut form.source_id,
        ),
        (
            ExtendedUiTextKey::AppearanceTileIndexHex,
            &mut form.tile_index,
        ),
        (ExtendedUiTextKey::AppearanceXOffsetDecimal, &mut form.x),
        (ExtendedUiTextKey::AppearanceYOffsetDecimal, &mut form.y),
    ] {
        ui.horizontal(|ui| {
            ui.label(text(catalog, key));
            ui.text_edit_singleline(field);
        });
    }
    ui.add(
        egui::Slider::new(&mut form.palette_index, 0..=7)
            .text(text(catalog, ExtendedUiTextKey::AppearancePaletteRow)),
    );
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut form.x_flip,
            text(catalog, ExtendedUiTextKey::AppearanceHorizontalFlip),
        );
        ui.checkbox(
            &mut form.y_flip,
            text(catalog, ExtendedUiTextKey::AppearanceVerticalFlip),
        );
    });
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{AppearanceSource, EntityAppearanceRecord};

    fn record(source: AppearanceSource, tile: u16, x: i32) -> EntityAppearanceRecord {
        EntityAppearanceRecord {
            source,
            tile_index: tile,
            palette_index: 7,
            x,
            y: -24,
            x_flip: true,
            y_flip: false,
        }
    }

    fn controller() -> EntityAppearanceDocumentController {
        EntityAppearanceDocumentController::decode(
            "entities.lmentapp".into(),
            &EntityAppearanceFile {
                appearances: vec![record(AppearanceSource::Sprite(1), 1, 10)],
            }
            .encode()
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn complete_appearance_form_uses_every_typed_key_and_live_catalog() {
        let source = include_str!("appearance_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("Appearance"))
        {
            assert!(
                source.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "missing appearance label {key:?}"
            );
        }
        for literal in [
            "Window::new(\"Portable Entity Appearance Editor\")",
            "Window::new(\"Unsaved entity appearances\")",
            "Window::new(\"Appearance editor error\")",
            "Button::new(\"Replace selected\")",
            "Button::new(\"Undo\")",
            "Button::new(\"Save\")",
        ] {
            assert!(
                !source.contains(literal),
                "fixed-English control: {literal}"
            );
        }
        assert!(
            include_str!("application/windows.rs")
                .contains(".show(context, self.app.localization())")
        );
    }

    #[test]
    fn all_source_kinds_and_painter_order_round_trip_as_one_undoable_revision() {
        let mut controller = controller();
        let layer1 = record(AppearanceSource::Layer1Object(0xfeed_beef), 0x123, -100);
        let layer2 = record(AppearanceSource::Layer2Object(0x1234_5678), 0x456, 200);
        let sprite = record(AppearanceSource::Sprite(0xdead_beef), 0x789, 300);
        let original = controller.value().clone();
        controller
            .apply_edits(
                0,
                &[
                    EntityAppearanceDocumentEdit::Replace {
                        index: 0,
                        value: layer1,
                    },
                    EntityAppearanceDocumentEdit::Insert {
                        index: 1,
                        value: layer2,
                    },
                    EntityAppearanceDocumentEdit::Insert {
                        index: 2,
                        value: sprite,
                    },
                    EntityAppearanceDocumentEdit::MoveBefore { from: 2, before: 0 },
                ],
            )
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().appearances, [sprite, layer1, layer2]);
        let snapshot = controller.begin_save().unwrap();
        let reopened = EntityAppearanceFile::decode(&snapshot.bytes).unwrap();
        assert_eq!(reopened, *controller.value());
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(controller.undo(controller.revision()).unwrap());
        assert_eq!(controller.value(), &original);
        assert!(controller.redo(controller.revision()).unwrap());
        assert_eq!(controller.value(), &reopened);
    }
}
