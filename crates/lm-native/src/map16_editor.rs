use crate::{
    dialogs,
    document_loader::DocumentLoader,
    map16_editor_render, map16_subtile_form, native_clipboard,
    user_toolbar_images::{MainToolbarImageSet, OriginalToolbarAction, OriginalToolbarImages},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, Map16PageDocumentController};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use map16_subtile_form::SubtileForm;

mod document_io;
mod editing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct Map16Document {
    controller: Map16PageDocumentController,
    graphics: GraphicsInterchangeFile,
    palette: PaletteInterchangeFile,
}

#[derive(Default)]
pub(crate) struct Map16Editor {
    document: Option<Map16Document>,
    selected_tile: usize,
    quadrant: usize,
    subtile: SubtileForm,
    acts_like: String,
    loaded_selection: Option<usize>,
    rendered_revision: Option<u64>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    clipboard_paste_target: Option<(u64, usize)>,
    save_worker: crate::persistence_worker::PersistenceWorker,
    loader: DocumentLoader,
}

impl Map16Editor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(page) = dialogs::choose_map16_page_document() else {
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
            .start(document_io::requests(page, graphics, palette))
        {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for Map16 loading to finish before closing".into());
            return false;
        }
        if self.save_worker.is_running() {
            self.error = Some("wait for Map16 persistence to finish before closing".into());
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
        toolbar_images: &MainToolbarImageSet,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(document_io::decode_document) {
                Ok(document) => {
                    self.document = Some(document);
                    self.selected_tile = 0;
                    self.loaded_selection = None;
                    self.rendered_revision = None;
                    self.texture = None;
                    self.clipboard_paste_target = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.poll_save(context);
        let mut quit_approved = false;
        if self.document.is_some() {
            self.refresh_texture(context);
            self.load_form();
            egui::Window::new(text(catalog, Key::Map16DocumentTitle))
                .default_size([760.0, 620.0])
                .show(context, |ui| self.contents(ui, toolbar_images, catalog));
        }
        if let Some(pending) = self.pending_close {
            egui::Window::new(text(catalog, Key::Map16DocumentDiscardTitle))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.label(text(catalog, Key::Map16DocumentDiscardNotice));
                    ui.horizontal(|ui| {
                        if ui.button(text(catalog, Key::Map16DocumentCancel)).clicked() {
                            self.pending_close = None;
                        }
                        if ui
                            .button(text(catalog, Key::Map16DocumentDiscard))
                            .clicked()
                        {
                            self.clear();
                            quit_approved = pending == PendingClose::Application;
                        }
                    });
                });
        }
        self.show_error(context, catalog);
        quit_approved
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        toolbar_images: &MainToolbarImageSet,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui, toolbar_images, catalog);
        if let Some(text) = pasted
            && let Some((revision, tile)) = self.clipboard_paste_target.take()
        {
            self.paste_tile_at(&text, revision, tile);
        }
        ui.separator();
        ui.columns(2, |columns| {
            self.page_view(&mut columns[0], catalog);
            self.properties(&mut columns[1], catalog);
        });
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        toolbar_images: &MainToolbarImageSet,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let save_available = !self.save_worker.is_running();
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let controller = &mut document.controller;
        let revision = controller.revision();
        let mut native_paste = None;
        ui.horizontal(|ui| {
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Map16,
                    OriginalToolbarAction::Undo,
                    &text(catalog, Key::RomMap16Undo),
                    controller.can_undo(),
                )
                .clicked()
                && controller.undo(revision).is_ok()
            {
                self.loaded_selection = None;
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Map16,
                    OriginalToolbarAction::Redo,
                    &text(catalog, Key::RomMap16Redo),
                    controller.can_redo(),
                )
                .clicked()
                && controller.redo(revision).is_ok()
            {
                self.loaded_selection = None;
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Map16,
                    OriginalToolbarAction::Save,
                    &text(catalog, Key::Map16DocumentSave),
                    save_available,
                )
                .clicked()
            {
                document_io::begin_save(controller, &mut self.save_worker, &mut self.error);
            }
            if ui.button(text(catalog, Key::RomMap16CopyTile)).clicked()
                && let Some(tile) = controller.value().page.tiles.get(self.selected_tile)
            {
                if let Err(error) = native_clipboard::copy_map16_tile_to_system(ui.ctx(), *tile) {
                    self.error = Some(error);
                }
            }
            if ui.button(text(catalog, Key::RomMap16PasteTile)).clicked() {
                self.clipboard_paste_target = Some((controller.revision(), self.selected_tile));
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
            ui.label(text(
                catalog,
                if controller.is_modified() {
                    Key::Map16DocumentModified
                } else {
                    Key::Map16DocumentSaved
                },
            ));
        });
        if let Some(((revision, tile), value)) = native_paste {
            match controller.apply_edits(
                revision,
                &[lm_app::Map16PageDocumentEdit::ReplaceTile { tile, value }],
            ) {
                Ok(()) => {
                    self.loaded_selection = None;
                    self.rendered_revision = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn page_view(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(texture) = &self.texture else {
            ui.label(text(catalog, Key::Map16DocumentPreviewUnavailable));
            return;
        };
        let image = egui::Image::new(texture).sense(egui::Sense::click());
        let response = ui.add(image);
        if response.clicked()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(tile) = map16_editor_render::selected_tile(response.rect, position)
        {
            self.selected_tile = tile;
            self.loaded_selection = None;
        }
        let column = self.selected_tile % 16;
        let row = self.selected_tile / 16;
        let cell = response.rect.width() / 16.0;
        let offsets = [
            0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
        ];
        let minimum = response.rect.min + egui::vec2(offsets[column] * cell, offsets[row] * cell);
        ui.painter().rect_stroke(
            egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell)),
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }

    fn properties(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.heading(
            text(catalog, Key::Map16DocumentTileFormat)
                .replace("{tile}", &format!("{:02X}", self.selected_tile)),
        );
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::RomMap16Quadrant));
            egui::ComboBox::from_id_salt("map16-quadrant")
                .selected_text(quadrant_text(catalog, self.quadrant))
                .show_ui(ui, |ui| {
                    for index in 0..4 {
                        if ui
                            .selectable_value(
                                &mut self.quadrant,
                                index,
                                quadrant_text(catalog, index),
                            )
                            .clicked()
                        {
                            self.loaded_selection = None;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::Map16DocumentSubtileHex));
            ui.text_edit_singleline(&mut self.subtile.tile);
        });
        ui.add(
            egui::Slider::new(&mut self.subtile.palette, 0..=7)
                .text(text(catalog, Key::RomMap16Palette)),
        );
        ui.checkbox(
            &mut self.subtile.priority,
            text(catalog, Key::RomMap16Priority),
        );
        ui.checkbox(
            &mut self.subtile.x_flip,
            text(catalog, Key::Map16DocumentHorizontalFlip),
        );
        ui.checkbox(
            &mut self.subtile.y_flip,
            text(catalog, Key::Map16DocumentVerticalFlip),
        );
        if ui
            .button(text(catalog, Key::RomMap16ApplySubtile))
            .clicked()
        {
            self.apply_subtile();
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::LevelAuxActsLike));
            ui.text_edit_singleline(&mut self.acts_like);
        });
        if ui
            .button(text(catalog, Key::RomMap16ApplyActsLike))
            .clicked()
        {
            self.apply_acts_like();
        }
    }

    fn load_form(&mut self) {
        let key = self
            .selected_tile
            .saturating_mul(4)
            .saturating_add(self.quadrant);
        if self.loaded_selection == Some(key) {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let Some(tile) = document
            .controller
            .value()
            .page
            .tiles
            .get(self.selected_tile)
        else {
            return;
        };
        let subtile = map16_subtile_form::quadrant_value(*tile, self.quadrant);
        self.subtile = SubtileForm::from_subtile(subtile);
        self.acts_like = format!("{:04X}", tile.acts_like);
        self.loaded_selection = Some(key);
    }

    fn refresh_texture(&mut self, context: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let revision = document.controller.revision();
        if self.rendered_revision == Some(revision) {
            return;
        }
        match map16_editor_render::render_texture(
            context,
            document.controller.value(),
            &document.graphics,
            &document.palette,
        ) {
            Ok(texture) => self.texture = Some(texture),
            Err(error) => self.error = Some(error),
        }
        self.rendered_revision = Some(revision);
    }

    fn clear(&mut self) {
        self.document = None;
        self.texture = None;
        self.pending_close = None;
        self.rendered_revision = None;
        self.loaded_selection = None;
        self.clipboard_paste_target = None;
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::Map16DocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::Map16DocumentOk)).clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn poll_save(&mut self, context: &egui::Context) {
        let Some(completion) = self.save_worker.show(context) else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            self.error = Some("Map16 save completed after its document was closed".into());
            return;
        };
        document_io::complete_save(&mut document.controller, completion, &mut self.error);
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

fn quadrant_text(catalog: Option<&LocalizationCatalog>, index: usize) -> String {
    text(
        catalog,
        [
            Key::Map16DocumentTopLeft,
            Key::Map16DocumentTopRight,
            Key::Map16DocumentBottomLeft,
            Key::Map16DocumentBottomRight,
        ][index.min(3)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, Palette};
    use lm_level::{Map16Page, Map16PageFile, Map16Tile, Subtile};

    #[test]
    fn portable_map16_editor_has_no_literal_widget_text() {
        let source = include_str!("map16_editor.rs");
        for literal_widget in [
            "ui.button(\"",
            "ui.label(\"",
            "ui.heading(\"",
            "Button::new(\"",
            "Window::new(\"",
            ".text(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "portable Map16 editor bypasses localization with {literal_widget}"
            );
        }
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("Map16Document"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}")),
                "portable Map16 editor does not consume {key:?}"
            );
        }
    }

    fn editor() -> Map16Editor {
        let file = Map16PageFile {
            source_page: 2,
            page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
        };
        Map16Editor {
            document: Some(Map16Document {
                controller: Map16PageDocumentController::decode(
                    "page.lm16page".into(),
                    &file.encode().unwrap(),
                )
                .unwrap(),
                graphics: GraphicsInterchangeFile {
                    source_slot: 0,
                    graphics: GraphicsFile4bpp { tiles: Vec::new() },
                },
                palette: PaletteInterchangeFile {
                    source_palette: 0,
                    palette: Palette { colors: Vec::new() },
                },
            }),
            ..Map16Editor::default()
        }
    }

    #[test]
    fn typed_map16_paste_replaces_the_complete_tile_in_one_revision() {
        let mut editor = editor();
        let replacement = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0xabcd,
        };
        editor.paste_tile(&native_clipboard::encode_map16_tile(replacement).unwrap());
        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 1);
        assert_eq!(document.controller.value().page.tiles[0], replacement);
        assert!(editor.loaded_selection.is_none());
        assert!(editor.rendered_revision.is_none());
    }

    #[test]
    fn clipboard_delivery_uses_requested_tile_and_rejects_a_stale_revision() {
        let mut editor = editor();
        let replacement = Map16Tile {
            top_left: Subtile(5),
            top_right: Subtile(6),
            bottom_left: Subtile(7),
            bottom_right: Subtile(8),
            acts_like: 0x1234,
        };
        let text = native_clipboard::encode_map16_tile(replacement).unwrap();
        editor.selected_tile = 9;
        editor.paste_tile_at(&text, 0, 3);
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .page
                .tiles[3],
            replacement
        );
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .page
                .tiles[9],
            Map16Tile::default()
        );

        editor.paste_tile_at(&text, 0, 4);
        assert!(editor.error.is_some());
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .page
                .tiles[4],
            Map16Tile::default()
        );
    }
}
