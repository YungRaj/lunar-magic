use super::Map16SetEditor;
use crate::native_clipboard;
use eframe::egui;
use lm_app::{LocalizationCatalog, Map16DocumentEdit, UiTextKey};
use lm_level::Map16Page;

impl Map16SetEditor {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let controller = &document.controller;
        let (can_undo, can_redo, modified) = (
            controller.can_undo(),
            controller.can_redo(),
            controller.is_modified(),
        );
        let mut history = None;
        let mut save_requested = false;
        let mut copy_requested = false;
        let mut native_paste = None;
        let mut append_requested = false;
        let mut remove_requested = false;
        let page_count = controller.value().set.pages.len();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::EditUndo,
                    )),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::EditRedo,
                    )),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::FileSave,
                    )),
                )
                .clicked();
            copy_requested = ui
                .button(crate::frontend_ui::localized_text(
                    catalog,
                    UiTextKey::EditCopy,
                ))
                .clicked();
            if ui
                .button(crate::frontend_ui::localized_text(
                    catalog,
                    UiTextKey::EditPaste,
                ))
                .clicked()
            {
                self.clipboard_paste_target = Some((
                    controller.revision(),
                    lm_level::Map16Address {
                        page: self.page,
                        tile: self.tile,
                    },
                ));
                match native_clipboard::request_map16_tile_paste(ui.ctx()) {
                    Ok(Some(tile)) => {
                        let target = self
                            .clipboard_paste_target
                            .take()
                            .expect("native paste target was just recorded");
                        native_paste = Some((target, tile));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.clipboard_paste_target = None;
                        self.error = Some(error);
                    }
                }
            }
            append_requested = ui
                .add_enabled(
                    page_count < lm_level::Map16Set::MAX_PAGES,
                    egui::Button::new(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::Map16SetAddBlankPage,
                    )),
                )
                .clicked();
            remove_requested = ui
                .add_enabled(
                    page_count > 1,
                    egui::Button::new(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::Map16SetRemoveLastPage,
                    )),
                )
                .clicked();
            ui.label(crate::frontend_ui::localized_text(
                catalog,
                if modified {
                    UiTextKey::Map16SetModified
                } else {
                    UiTextKey::Map16SetSaved
                },
            ));
        });
        if let Some(((revision, address), tile)) = native_paste {
            match native_clipboard::encode_map16_tile(tile) {
                Ok(text) => self.paste_tile_at(&text, revision, address),
                Err(error) => self.error = Some(error),
            }
        }
        if copy_requested && let Some(tile) = self.current_tile() {
            if let Err(error) = native_clipboard::copy_map16_tile_to_system(ui.ctx(), tile) {
                self.error = Some(error);
            }
        }
        let mut changed = false;
        if let Some(document) = self.document.as_mut() {
            let controller = &mut document.controller;
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
        if append_requested {
            self.append_blank_page(page_count);
        }
        if remove_requested {
            self.apply_edit(&Map16DocumentEdit::RemoveLastPage {
                resolution_limit: page_count
                    .saturating_sub(1)
                    .saturating_mul(Map16Page::TILE_COUNT),
            });
            self.clamp_selection();
        }
    }

    fn append_blank_page(&mut self, page_count: usize) {
        let Ok(page) = Map16Page::new(vec![lm_level::Map16Tile::default(); Map16Page::TILE_COUNT])
        else {
            self.error = Some("could not construct a blank Map16 page".into());
            return;
        };
        self.apply_edit(&Map16DocumentEdit::AppendPage {
            page,
            resolution_limit: page_count
                .saturating_add(1)
                .saturating_mul(Map16Page::TILE_COUNT),
        });
        if self.error.is_none() {
            self.page = page_count;
            self.tile = 0;
        }
    }
}
