use crate::{
    custom_object_editor_form::DescriptionFormatForm, custom_sprite_editor_form::CustomSpriteForm,
    dialogs, document_loader::DocumentLoader, document_persistence::DocumentPersistence,
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    CustomSpriteLibraryController, CustomSpriteLibraryEdit, ExtendedUiTextKey, LocalizationCatalog,
};

mod document_io;
mod editing;
mod lifecycle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct CustomSpriteEditor {
    controller: Option<CustomSpriteLibraryController>,
    index: usize,
    form: CustomSpriteForm,
    form_key: Option<(u64, usize)>,
    format: Option<DescriptionFormatForm>,
    header: String,
    search: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl CustomSpriteEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(data_path) = dialogs::choose_custom_sprite_data() else {
            return;
        };
        let Some(descriptions_path) = dialogs::choose_custom_sprite_descriptions() else {
            return;
        };
        let Some(lengths_path) = dialogs::choose_sprite_length_table() else {
            return;
        };
        if let Err(error) = self.loader.start(document_io::requests(
            data_path,
            descriptions_path,
            lengths_path,
        )) {
            self.error = Some(error);
        }
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let key = (controller.revision(), self.index);
        if self.form_key != Some(key) {
            self.form = controller
                .library()
                .entries()
                .get(self.index)
                .map_or_else(CustomSpriteForm::default, |entry| {
                    CustomSpriteForm::load(entry, self.index)
                });
            self.format = Some(DescriptionFormatForm::load_value(
                controller.library().description_format(),
            ));
            self.header = format!("{:02X}", controller.library().header());
            self.form_key = Some(key);
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui, catalog);
        if let Some(text) = pasted {
            self.paste_placement(&text);
        }
        ui.separator();
        let entries = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.library().entries().len());
        ui.label(
            text(catalog, ExtendedUiTextKey::CustomSpritePlacementsFormat)
                .replace("{count}", &entries.to_string()),
        );
        self.header_and_search(ui, catalog);
        ui.add(
            egui::Slider::new(&mut self.index, 0..=entries.saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::CustomSpritePlacement)),
        );
        ui.label(text(catalog, ExtendedUiTextKey::CustomSpriteRecordsNotice));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.sprite_records)
                .desired_rows(8)
                .code_editor(),
        );
        ui.label(text(
            catalog,
            ExtendedUiTextKey::CustomSpriteDescriptionNotice,
        ));
        ui.text_edit_singleline(&mut self.form.description);
        self.clipboard_actions(ui, entries, catalog);
        if let Some(edit) = self.entry_actions(ui, entries, catalog) {
            match edit {
                Ok(edit) => self.apply_edit(&edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn clipboard_actions(
        &mut self,
        ui: &mut egui::Ui,
        entries: usize,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    entries > 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteCopyPlacement)),
                )
                .clicked()
                && let Some(sprites) = self.current_sprites()
            {
                match native_clipboard::encode_level_sprites(sprites) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    entries > 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpritePastePlacement)),
                )
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
    }

    fn header_and_search(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::CustomSpriteHeaderHex));
            ui.text_edit_singleline(&mut self.header);
            if ui
                .button(text(catalog, ExtendedUiTextKey::CustomSpriteApplyHeader))
                .clicked()
            {
                match u8::from_str_radix(self.header.trim(), 16) {
                    Ok(header) => self.apply_edit(&CustomSpriteLibraryEdit::SetHeader(header)),
                    Err(error) => {
                        self.error = Some(format!("invalid sprite-library header: {error}"));
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::CustomSpriteSearch));
            if ui.text_edit_singleline(&mut self.search).changed() && !self.search.is_empty() {
                if let Some(found) = self.controller.as_ref().and_then(|controller| {
                    controller.library().search(&self.search).first().copied()
                }) {
                    self.index = found;
                    self.form_key = None;
                }
            }
        });
    }

    fn entry_actions(
        &mut self,
        ui: &mut egui::Ui,
        entries: usize,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<CustomSpriteLibraryEdit, String>> {
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    entries > 0,
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::CustomSpriteReplaceSelected,
                    )),
                )
                .clicked()
            {
                edit = Some(
                    self.form
                        .entry()
                        .map(|entry| CustomSpriteLibraryEdit::Replace {
                            index: self.index,
                            entry,
                        }),
                );
            }
            if ui
                .add_enabled(
                    entries > 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteRemoveSelected)),
                )
                .clicked()
            {
                edit = Some(Ok(CustomSpriteLibraryEdit::Remove { index: self.index }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.form.insert_index).range(0..=entries));
            if ui
                .button(text(catalog, ExtendedUiTextKey::CustomSpriteInsertAt))
                .clicked()
            {
                edit = Some(
                    self.form
                        .entry()
                        .map(|entry| CustomSpriteLibraryEdit::Insert {
                            index: self.form.insert_index,
                            entry,
                        }),
                );
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.form.move_to).range(0..=entries.saturating_sub(1)),
            );
            if ui
                .add_enabled(
                    entries > 1,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteMoveTo)),
                )
                .clicked()
            {
                edit = Some(Ok(CustomSpriteLibraryEdit::Move {
                    from: self.index,
                    to: self.form.move_to,
                }));
            }
        });
        ui.separator();
        if let Some(format) = self.format.as_mut() {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut format.utf8_bom,
                    text(catalog, ExtendedUiTextKey::CustomSpriteUtf8Bom),
                );
                ui.checkbox(
                    &mut format.crlf,
                    text(catalog, ExtendedUiTextKey::CustomSpriteCrlf),
                );
                ui.checkbox(
                    &mut format.trailing_line_ending,
                    text(catalog, ExtendedUiTextKey::CustomSpriteTrailingLineEnding),
                );
            });
            if ui
                .button(text(catalog, ExtendedUiTextKey::CustomSpriteApplyFraming))
                .clicked()
            {
                edit = Some(Ok(CustomSpriteLibraryEdit::SetDescriptionFormat(
                    format.value(),
                )));
            }
        }
        edit
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
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::CustomSpriteSavePair)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::CustomSpriteModified
                } else {
                    ExtendedUiTextKey::CustomSpriteSaved
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
                if let Err(error) = self.persistence.begin_pair(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn clamp_index(&mut self) {
        let len = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.library().entries().len());
        self.index = if len == 0 { 0 } else { self.index.min(len - 1) };
        self.form.insert_index = self.form.insert_index.min(len);
        self.form.move_to = if len == 0 {
            0
        } else {
            self.form.move_to.min(len - 1)
        };
    }

    fn invalidate(&mut self) {
        self.form_key = None;
        self.format = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{
        CustomSpriteEntry, DescriptionFormat, LineEnding, SpriteLengthTable, SpriteRecord,
    };

    fn editor() -> CustomSpriteEditor {
        let controller = CustomSpriteLibraryController::decode(
            "sprites.mw2".into(),
            "sprites.mwt".into(),
            &[0x5a, 1, 2, 3, 0, 4, 5, 0xff],
            b"Original placement\n",
            SpriteLengthTable::standard(),
        )
        .unwrap();
        let mut editor = CustomSpriteEditor {
            controller: Some(controller),
            ..CustomSpriteEditor::default()
        };
        editor.load_form();
        editor
    }

    #[test]
    fn typed_placement_paste_replaces_all_records_and_preserves_description() {
        let mut editor = editor();
        let replacement = vec![
            SpriteRecord {
                encoded: vec![1, 7, 8],
            },
            SpriteRecord {
                encoded: vec![0, 9, 10],
            },
        ];
        let text = native_clipboard::encode_level_sprites(&replacement).unwrap();

        editor.paste_placement(&text);

        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.library().entries()[0].sprites, replacement);
        assert_eq!(
            controller.library().entries()[0].description,
            "Original placement"
        );
        assert!(editor.form_key.is_none());
    }

    #[test]
    fn wrong_clipboard_domain_preserves_sprite_library_and_revision() {
        let mut editor = editor();
        let before = editor.controller.as_ref().unwrap().library().clone();
        let text = native_clipboard::encode_palette_color(lm_graphics::Bgr555(0x1234)).unwrap();

        editor.paste_placement(&text);

        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 0);
        assert_eq!(controller.library(), &before);
        assert!(editor.error.is_some());
    }

    #[test]
    fn complete_custom_sprite_form_uses_every_typed_key_and_live_catalog() {
        let form = include_str!("custom_sprite_editor.rs");
        let lifecycle = include_str!("custom_sprite_editor/lifecycle.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("CustomSprite"))
        {
            assert!(
                form.contains(&format!("ExtendedUiTextKey::{key:?}"))
                    || lifecycle.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "missing custom-sprite label {key:?}"
            );
        }
        for literal in [
            "Window::new(\"Custom Sprite Placement Editor\")",
            "Window::new(\"Unsaved custom-sprite library\")",
            "Window::new(\"Custom-sprite editor error\")",
            "Button::new(\"Copy placement\")",
            "Button::new(\"Save paired files\")",
        ] {
            assert!(
                !form.contains(literal) && !lifecycle.contains(literal),
                "fixed-English control: {literal}"
            );
        }
        assert!(
            include_str!("application/windows.rs")
                .contains(".show(context, self.app.localization())")
        );
    }

    #[test]
    fn variable_width_placements_header_unicode_and_framing_round_trip() {
        let mut editor = editor();
        let controller = editor.controller.as_mut().unwrap();
        let original = controller.library().clone();
        let pair = CustomSpriteEntry::new(
            vec![
                SpriteRecord {
                    encoded: vec![1, 8, 9],
                },
                SpriteRecord {
                    encoded: vec![0, 10, 11],
                },
            ],
            "Enemy pair ★".into(),
        )
        .unwrap();
        let single = CustomSpriteEntry::new(
            vec![SpriteRecord {
                encoded: vec![5, 4, 5],
            }],
            "Boss ✓".into(),
        )
        .unwrap();
        let format = DescriptionFormat {
            utf8_bom: true,
            line_ending: LineEnding::CrLf,
            trailing_line_ending: true,
        };
        controller
            .apply_edits(
                controller.revision(),
                &[
                    CustomSpriteLibraryEdit::Replace {
                        index: 0,
                        entry: pair.clone(),
                    },
                    CustomSpriteLibraryEdit::Insert {
                        index: 1,
                        entry: single.clone(),
                    },
                    CustomSpriteLibraryEdit::Move { from: 1, to: 0 },
                    CustomSpriteLibraryEdit::SetHeader(0x44),
                    CustomSpriteLibraryEdit::SetDescriptionFormat(format),
                ],
            )
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.library().entries(), [single, pair]);
        assert_eq!(controller.library().header(), 0x44);
        assert_eq!(controller.library().description_format(), format);
        let lengths = controller.sprite_lengths().clone();
        let snapshot = controller.begin_save().unwrap();
        let reopened =
            lm_level::CustomSpriteLibrary::decode(&snapshot.data, &snapshot.descriptions, &lengths)
                .unwrap();
        assert_eq!(reopened, *controller.library());
        assert!(snapshot.descriptions.starts_with(&[0xef, 0xbb, 0xbf]));
        assert!(
            snapshot
                .descriptions
                .windows(2)
                .any(|bytes| bytes == b"\r\n")
        );
        assert!(snapshot.descriptions.ends_with(b"\r\n"));
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(controller.undo(controller.revision()).unwrap());
        assert_eq!(controller.library(), &original);
        assert_eq!(controller.sprite_lengths(), &lengths);
        assert!(controller.redo(controller.revision()).unwrap());
        assert_eq!(controller.library(), &reopened);
    }
}
