use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    native_clipboard,
    overworld_appearance_editor_forms::{DefinitionForm, PartForm},
    persistence_worker::PersistenceWorker,
};
use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, OverworldAppearanceDocumentController,
    OverworldAppearanceDocumentEdit,
};

mod document_io;
mod form_fields;
mod native_mode;
mod panels;
mod preview;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingAppearanceLoad {
    PortableOpen,
    NativeImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppearancePasteMode {
    ReplacePart { index: usize },
    InsertPartAfter { index: usize },
    ReplaceComposition,
    AppendComposition,
    InsertDefinition { index: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AppearancePasteTarget {
    revision: u64,
    sprite_id: u16,
    mode: AppearancePasteMode,
}

#[derive(Default)]
pub(crate) struct OverworldAppearanceEditor {
    controller: Option<OverworldAppearanceDocumentController>,
    native_controller: Option<lm_app::NativeOverworldAppearanceController>,
    native_form: native_mode::NativeAppearanceForm,
    definition_index: usize,
    definition: DefinitionForm,
    definition_key: Option<(u64, usize)>,
    part_index: usize,
    part: PartForm,
    part_key: Option<(u64, u16, usize)>,
    preview_drag: Option<preview::PreviewDrag>,
    clipboard_paste_target: Option<AppearancePasteTarget>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    native_persistence: PersistenceWorker,
    loader: DocumentLoader,
    pending_load: Option<PendingAppearanceLoad>,
}

impl OverworldAppearanceEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.native_controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() || self.native_persistence.is_running() {
            self.error = Some("wait for appearance loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for appearance persistence to finish before closing".into());
            return false;
        }
        let modified = self
            .controller
            .as_ref()
            .is_some_and(OverworldAppearanceDocumentController::is_modified)
            || self
                .native_controller
                .as_ref()
                .is_some_and(lm_app::NativeOverworldAppearanceController::is_modified);
        if !modified {
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
            let pending = self.pending_load.take();
            match pending {
                Some(PendingAppearanceLoad::PortableOpen) => {
                    match result.and_then(document_io::decode) {
                        Ok(controller) => {
                            self.controller = Some(controller);
                            self.clipboard_paste_target = None;
                            self.invalidate();
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Some(PendingAppearanceLoad::NativeImport) => {
                    match result.and_then(document_io::decode_native_pair) {
                        Ok(controller) => {
                            self.controller = None;
                            self.native_controller = Some(controller);
                            self.clipboard_paste_target = None;
                            self.native_form.invalidate();
                            self.invalidate();
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                None => {
                    self.error = Some("appearance loader completed without an operation".into())
                }
            }
        }
        if let Some(completion) = self.native_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if let Some(controller) = self.native_controller.as_mut()
            && let Some(Err(error)) = self.persistence.show_pair(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() || self.native_controller.is_some() {
            self.clamp_indices();
            let title = if self.native_controller.is_some() {
                text(catalog, Key::OverworldAppearanceNativeTitle)
            } else {
                text(catalog, Key::OverworldAppearancePortableTitle)
            };
            egui::Window::new(title)
                .default_size([720.0, 600.0])
                .vscroll(true)
                .show(context, |ui| {
                    ui.add_enabled_ui(
                        !self.loader.is_running() && !self.native_persistence.is_running(),
                        |ui| {
                            if self.native_controller.is_some() {
                                self.native_contents(ui, catalog);
                            } else {
                                self.contents(ui, catalog);
                            }
                        },
                    );
                });
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui, catalog);
        if let Some(text) = pasted
            && let Some(target) = self.clipboard_paste_target.take()
        {
            self.paste_appearance_at(&text, target);
        }
        ui.separator();
        let (revision, definitions) = {
            let Some(controller) = self.controller.as_ref() else {
                return;
            };
            (
                controller.revision(),
                controller.value().definitions.clone(),
            )
        };
        ui.label(
            text(catalog, Key::OverworldAppearanceDefinitionsFormat)
                .replace("{count}", &definitions.len().to_string()),
        );
        ui.add(
            egui::Slider::new(
                &mut self.definition_index,
                0..=definitions.len().saturating_sub(1),
            )
            .text(text(catalog, Key::OverworldAppearanceDefinition)),
        );
        let selected = definitions.get(self.definition_index);
        if self.definition_key != Some((revision, self.definition_index)) {
            self.definition = selected.map_or_else(DefinitionForm::default, |definition| {
                DefinitionForm::load(definition.sprite_id, self.definition_index)
            });
            self.definition_key = Some((revision, self.definition_index));
        }
        let mut edit = self.definition_fields(ui, &definitions, catalog);
        ui.separator();
        if let Some(definition) = selected {
            let preview_edit = self.appearance_preview(ui, revision, definition, catalog);
            let part_edit = self.part_fields(ui, revision, definition, catalog);
            edit = edit.or(preview_edit.map(Ok)).or(part_edit);
        } else {
            ui.label(text(catalog, Key::OverworldAppearanceEmptyNotice));
        }
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
        let mut native_import_requested = false;
        let mut native_export_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, Key::AppearanceUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, Key::AppearanceRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, Key::AppearanceSave)),
                )
                .clicked();
            native_import_requested = ui
                .button(text(catalog, Key::OverworldAppearanceImportNative))
                .clicked();
            native_export_requested = ui
                .button(text(catalog, Key::OverworldAppearanceExportNative))
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    Key::AppearanceModified
                } else {
                    Key::AppearanceSaved
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
        if native_import_requested {
            self.import_native_pair();
        }
        if native_export_requested {
            self.export_native_pair();
        }
    }

    fn apply_edit(&mut self, edit: &OverworldAppearanceDocumentEdit) {
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

    fn paste_appearance_at(&mut self, text: &str, target: AppearancePasteTarget) {
        let parts = match target.mode {
            AppearancePasteMode::ReplacePart { .. }
            | AppearancePasteMode::InsertPartAfter { .. } => {
                native_clipboard::decode_overworld_appearance_part(text).map(|part| vec![part])
            }
            AppearancePasteMode::ReplaceComposition
            | AppearancePasteMode::AppendComposition
            | AppearancePasteMode::InsertDefinition { .. } => {
                native_clipboard::decode_overworld_appearance_parts(text)
            }
        };
        let parts = match parts {
            Ok(parts) => parts,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let (edits, definition_index, part_index) = match target.mode {
            AppearancePasteMode::ReplacePart { index } => (
                vec![OverworldAppearanceDocumentEdit::ReplacePart {
                    sprite_id: target.sprite_id,
                    index,
                    value: parts[0],
                }],
                None,
                index,
            ),
            AppearancePasteMode::InsertPartAfter { index } => {
                let Some(index) = index.checked_add(1) else {
                    self.error = Some("overworld appearance paste index overflow".into());
                    return;
                };
                (
                    vec![OverworldAppearanceDocumentEdit::InsertPart {
                        sprite_id: target.sprite_id,
                        index,
                        value: parts[0],
                    }],
                    None,
                    index,
                )
            }
            AppearancePasteMode::ReplaceComposition => (
                vec![OverworldAppearanceDocumentEdit::ReplaceParts {
                    sprite_id: target.sprite_id,
                    values: parts,
                }],
                None,
                0,
            ),
            AppearancePasteMode::AppendComposition => {
                let Some(controller) = self.controller.as_ref() else {
                    self.error =
                        Some("overworld appearance document closed before paste delivery".into());
                    return;
                };
                let Some(definition) = controller.value().definition(target.sprite_id) else {
                    self.error = Some(format!(
                        "overworld appearance sprite {:04X} no longer exists",
                        target.sprite_id
                    ));
                    return;
                };
                let selected = definition.parts.len();
                let mut values = definition.parts.clone();
                values.extend(parts);
                (
                    vec![OverworldAppearanceDocumentEdit::ReplaceParts {
                        sprite_id: target.sprite_id,
                        values,
                    }],
                    None,
                    selected,
                )
            }
            AppearancePasteMode::InsertDefinition { index } => (
                vec![
                    OverworldAppearanceDocumentEdit::InsertDefinition {
                        index,
                        sprite_id: target.sprite_id,
                    },
                    OverworldAppearanceDocumentEdit::ReplaceParts {
                        sprite_id: target.sprite_id,
                        values: parts,
                    },
                ],
                Some(index),
                0,
            ),
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("overworld appearance document closed before paste delivery".into());
            return;
        };
        match controller.apply_edits(target.revision, &edits) {
            Ok(()) => {
                if let Some(index) = definition_index {
                    self.definition_index = index;
                }
                self.part_index = part_index;
                self.invalidate();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn clamp_indices(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let definitions = &controller.value().definitions;
        self.definition_index = clamp(self.definition_index, definitions.len());
        let part_len = definitions
            .get(self.definition_index)
            .map_or(0, |definition| definition.parts.len());
        self.part_index = clamp(self.part_index, part_len);
        self.definition.insert_index = self.definition.insert_index.min(definitions.len());
        self.definition.move_before = clamp(self.definition.move_before, definitions.len());
        self.part.insert_index = self.part.insert_index.min(part_len);
        self.part.move_before = clamp(self.part.move_before, part_len);
    }

    fn invalidate(&mut self) {
        self.definition_key = None;
        self.part_key = None;
        self.preview_drag = None;
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
        egui::Window::new(text(catalog, Key::AppearanceDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, Key::AppearanceUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::AppearanceCancel)).clicked() {
                        self.pending_close = None;
                    }
                    if ui.button(text(catalog, Key::AppearanceDiscard)).clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::AppearanceErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::AppearanceOk)).clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.native_controller = None;
        self.clipboard_paste_target = None;
        self.pending_close = None;
        self.pending_load = None;
        self.native_form.invalidate();
        self.invalidate();
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearanceFile, SpriteAppearancePart};

    fn part(tile_index: u16) -> SpriteAppearancePart {
        SpriteAppearancePart {
            tile_index,
            palette_index: 3,
            x_offset: -4,
            y_offset: 5,
            x_flip: true,
            y_flip: false,
        }
    }

    fn editor() -> OverworldAppearanceEditor {
        let file = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x1234,
                parts: vec![part(1), part(2)],
            }],
        };
        OverworldAppearanceEditor {
            controller: Some(
                OverworldAppearanceDocumentController::decode(
                    "appearance.lmowapp".into(),
                    &file.encode().unwrap(),
                )
                .unwrap(),
            ),
            ..OverworldAppearanceEditor::default()
        }
    }

    #[test]
    fn typed_part_paste_replaces_or_inserts_as_one_revision() {
        let replacement = part(0xabcd);
        let text = native_clipboard::encode_overworld_appearance_part(replacement).unwrap();
        let mut editor = editor();
        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 0,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::ReplacePart { index: 0 },
            },
        );
        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().definitions[0].parts[0], replacement);
        assert_eq!(editor.part_index, 0);

        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 1,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::InsertPartAfter { index: 0 },
            },
        );
        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 2);
        assert_eq!(controller.value().definitions[0].parts.len(), 3);
        assert_eq!(controller.value().definitions[0].parts[1], replacement);
        assert_eq!(editor.part_index, 1);
    }

    #[test]
    fn part_paste_rejects_stale_targets_and_other_domains_without_mutation() {
        let mut editor = editor();
        let target = AppearancePasteTarget {
            revision: 0,
            sprite_id: 0x1234,
            mode: AppearancePasteMode::ReplacePart { index: 0 },
        };
        let text = native_clipboard::encode_overworld_appearance_part(part(3)).unwrap();
        editor.paste_appearance_at(&text, target);
        editor.error = None;
        editor.paste_appearance_at(&text, target);
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 1);

        editor.error = None;
        let wrong_domain = native_clipboard::encode_palette_color(lm_graphics::Bgr555(1)).unwrap();
        editor.paste_appearance_at(
            &wrong_domain,
            AppearancePasteTarget {
                revision: 1,
                ..target
            },
        );
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 1);
    }

    #[test]
    fn composition_paste_replaces_appends_and_inserts_a_definition_atomically() {
        let composition = vec![part(7), part(8), part(9)];
        let text = native_clipboard::encode_overworld_appearance_parts(&composition).unwrap();
        let mut editor = editor();
        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 0,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::ReplaceComposition,
            },
        );
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 1);
        assert_eq!(
            editor.controller.as_ref().unwrap().value().definitions[0].parts,
            composition
        );

        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 1,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::AppendComposition,
            },
        );
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 2);
        assert_eq!(
            editor.controller.as_ref().unwrap().value().definitions[0].parts,
            [composition.clone(), composition.clone()].concat()
        );
        assert_eq!(editor.part_index, 3);

        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 2,
                sprite_id: 0x5678,
                mode: AppearancePasteMode::InsertDefinition { index: 1 },
            },
        );
        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 3);
        assert_eq!(controller.value().definitions[1].sprite_id, 0x5678);
        assert_eq!(controller.value().definitions[1].parts, composition);
        assert_eq!(editor.definition_index, 1);
        assert_eq!(editor.part_index, 0);

        let before = controller.value().clone();
        editor.error = None;
        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 2,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::AppendComposition,
            },
        );
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 3);
        assert_eq!(editor.controller.as_ref().unwrap().value(), &before);

        editor.error = None;
        editor.paste_appearance_at(
            &text,
            AppearancePasteTarget {
                revision: 3,
                sprite_id: 0x1234,
                mode: AppearancePasteMode::InsertDefinition { index: 0 },
            },
        );
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 3);
        assert_eq!(editor.controller.as_ref().unwrap().value(), &before);
    }
}
