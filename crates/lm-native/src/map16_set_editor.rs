mod clipboard;
mod document_io;
mod editing;
mod preview;
mod toolbar;

use document_io::decode_document;

use crate::{
    dialogs, document_loader::DocumentLoader, document_persistence::DocumentPersistence,
    map16_subtile_form,
};
use eframe::egui;
use lm_app::{LocalizationCatalog, Map16DocumentController, UiTextKey};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::Map16Page;
use map16_subtile_form::SubtileForm;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct Document {
    controller: Map16DocumentController,
    graphics: GraphicsInterchangeFile,
    palette: PaletteInterchangeFile,
}

#[derive(Default)]
pub(crate) struct Map16SetEditor {
    document: Option<Document>,
    page: usize,
    tile: usize,
    quadrant: usize,
    subtile: SubtileForm,
    acts_like: String,
    loaded_key: Option<(u64, usize, usize, usize)>,
    rendered_key: Option<(u64, usize)>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    clipboard_paste_target: Option<(u64, lm_level::Map16Address)>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl Map16SetEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(set) = dialogs::choose_map16_set_document() else {
            return;
        };
        let Some(graphics) = dialogs::choose_graphics_document() else {
            return;
        };
        let Some(palette) = dialogs::choose_palette_document() else {
            return;
        };
        if let Err(error) = self
            .loader
            .start(document_io::requests(set, graphics, palette))
        {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for Map16-set loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for Map16-set persistence to finish before closing".into());
            return false;
        }
        let Some(document) = &self.document else {
            return true;
        };
        if !document.controller.is_modified() {
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
            match result.and_then(decode_document) {
                Ok(document) => {
                    self.document = Some(document);
                    self.page = 0;
                    self.tile = 0;
                    self.clipboard_paste_target = None;
                    self.invalidate();
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(document) = self.document.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, &mut document.controller)
        {
            self.error = Some(error);
        }
        if self.document.is_some() {
            self.clamp_selection();
            self.refresh_texture(context);
            self.load_form();
            egui::Window::new(crate::frontend_ui::localized_text(
                catalog,
                UiTextKey::Map16SetEditorTitle,
            ))
            .default_size([820.0, 680.0])
            .show(context, |ui| self.contents(ui, catalog));
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
            && let Some((revision, address)) = self.clipboard_paste_target.take()
        {
            self.paste_tile_at(&text, revision, address);
        }
        ui.separator();
        let page_count = self
            .document
            .as_ref()
            .map_or(0, |document| document.controller.value().set.pages.len());
        let previous_page = self.page;
        ui.add(
            egui::Slider::new(&mut self.page, 0..=page_count.saturating_sub(1)).text(
                crate::frontend_ui::localized_text(catalog, UiTextKey::Map16SetPage),
            ),
        );
        if previous_page != self.page {
            self.tile = 0;
            self.invalidate();
        }
        ui.columns(2, |columns| {
            self.page_view(&mut columns[0], catalog);
            self.properties(&mut columns[1], catalog);
        });
    }

    fn clamp_selection(&mut self) {
        let pages = self
            .document
            .as_ref()
            .map_or(0, |document| document.controller.value().set.pages.len());
        self.page = self.page.min(pages.saturating_sub(1));
        self.tile = self.tile.min(Map16Page::TILE_COUNT - 1);
    }

    fn invalidate(&mut self) {
        self.loaded_key = None;
        self.rendered_key = None;
        self.texture = None;
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
        egui::Window::new(crate::frontend_ui::localized_text(
            catalog,
            UiTextKey::Map16SetUnsavedTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.label(crate::frontend_ui::localized_text(
                catalog,
                UiTextKey::Map16SetDiscardQuestion,
            ));
            ui.horizontal(|ui| {
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonCancel,
                    ))
                    .clicked()
                {
                    self.pending_close = None;
                }
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::UnsavedDiscard,
                    ))
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
            egui::Window::new(crate::frontend_ui::localized_text(
                catalog,
                UiTextKey::Map16SetErrorTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(error);
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonOk,
                    ))
                    .clicked()
                {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.document = None;
        self.pending_close = None;
        self.clipboard_paste_target = None;
        self.invalidate();
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn complete_map16_editor_surface_has_no_literal_widget_text() {
        let sources = [
            include_str!("map16_set_editor.rs"),
            include_str!("map16_set_editor/toolbar.rs"),
            include_str!("map16_set_editor/editing.rs"),
            include_str!("map16_set_editor/preview.rs"),
        ]
        .join("\n");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
            "ui.heading(\"",
            ".text(\"",
        ] {
            assert!(
                !sources.contains(literal_widget),
                "complete Map16 editor bypasses typed localization with {literal_widget}"
            );
        }
        for key in [
            UiTextKey::Map16SetEditorTitle,
            UiTextKey::EditCopy,
            UiTextKey::Map16SetApplySubtile,
            UiTextKey::Map16SetApplyActsLike,
            UiTextKey::Map16SetUnsavedTitle,
            UiTextKey::Map16SetErrorTitle,
        ] {
            assert!(sources.contains(&format!("UiTextKey::{key:?}")));
        }
    }
}
