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
use lm_app::Map16DocumentController;
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(decode_document) {
                Ok(document) => {
                    self.document = Some(document);
                    self.page = 0;
                    self.tile = 0;
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
            egui::Window::new("Complete Map16 Set Editor")
                .default_size([820.0, 680.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui);
        if let Some(text) = pasted {
            self.paste_tile(&text);
        }
        ui.separator();
        let page_count = self
            .document
            .as_ref()
            .map_or(0, |document| document.controller.value().set.pages.len());
        let previous_page = self.page;
        ui.add(egui::Slider::new(&mut self.page, 0..=page_count.saturating_sub(1)).text("Page"));
        if previous_page != self.page {
            self.tile = 0;
            self.invalidate();
        }
        ui.columns(2, |columns| {
            self.page_view(&mut columns[0]);
            self.properties(&mut columns[1]);
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

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved complete Map16 set")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved complete Map16 changes?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Complete Map16 editor error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.document = None;
        self.pending_close = None;
        self.invalidate();
    }
}
