use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    native_clipboard,
    user_toolbar_images::{MainToolbarImageSet, OriginalToolbarAction, OriginalToolbarImages},
};
use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, PaletteControllerEdit, PaletteDocumentController,
};
use lm_graphics::{Bgr555, PaletteChange, PaletteInterchangeFile, PaletteOwnership, Rgb8};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct PaletteEditor {
    controller: Option<PaletteDocumentController>,
    selected: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    save_worker: crate::persistence_worker::PersistenceWorker,
    loader: DocumentLoader,
}

impl PaletteEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.controller.is_some() {
            return;
        }
        let Some(path) = dialogs::choose_palette_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(PaletteInterchangeFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "palette document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for palette loading to finish before closing".into());
            return false;
        }
        if self.save_worker.is_running() {
            self.error = Some("wait for palette persistence to finish before closing".into());
            return false;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.controller = None;
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
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "palette loader returned no file".to_string())?;
                PaletteDocumentController::decode(path, &bytes).map_err(|error| error.to_string())
            }) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.selected = 0;
                }
                Err(error) => self.error = Some(error),
            }
        }
        self.poll_save(context);
        let mut quit_approved = false;
        if self.controller.is_some() {
            egui::Window::new(text(catalog, Key::PaletteDocumentTitle))
                .default_size([520.0, 420.0])
                .show(context, |ui| self.contents(ui, toolbar_images, catalog));
        }
        if let Some(pending) = self.pending_close {
            egui::Window::new(text(catalog, Key::PaletteDocumentDiscardTitle))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.label(text(catalog, Key::PaletteDocumentDiscardNotice));
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(catalog, Key::PaletteDocumentCancel))
                            .clicked()
                        {
                            self.pending_close = None;
                        }
                        if ui
                            .button(text(catalog, Key::PaletteDocumentDiscard))
                            .clicked()
                        {
                            self.controller = None;
                            self.pending_close = None;
                            quit_approved = pending == PendingClose::Application;
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::PaletteDocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::PaletteDocumentOk)).clicked() {
                        self.error = None;
                    }
                });
        }
        quit_approved
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        toolbar_images: &MainToolbarImageSet,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let save_available = !self.save_worker.is_running();
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let revision = controller.revision();
        ui.horizontal(|ui| {
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::LevelPalette,
                    OriginalToolbarAction::Undo,
                    &text(catalog, Key::PaletteDocumentUndo),
                    controller.can_undo(),
                )
                .clicked()
            {
                if let Err(error) = controller.undo(revision) {
                    self.error = Some(error.to_string());
                }
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::LevelPalette,
                    OriginalToolbarAction::Redo,
                    &text(catalog, Key::PaletteDocumentRedo),
                    controller.can_redo(),
                )
                .clicked()
            {
                if let Err(error) = controller.redo(revision) {
                    self.error = Some(error.to_string());
                }
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::LevelPalette,
                    OriginalToolbarAction::Save,
                    &text(catalog, Key::PaletteDocumentSave),
                    save_available,
                )
                .clicked()
            {
                Self::begin_save(controller, &mut self.save_worker, &mut self.error);
            }
            clipboard_controls(ui, controller, self.selected, &mut self.error, catalog);
            ui.label(text(
                catalog,
                if controller.is_modified() {
                    Key::PaletteDocumentModified
                } else {
                    Key::PaletteDocumentSaved
                },
            ));
        });
        ui.separator();
        let color_count = controller.value().palette.colors.len();
        self.selected = self.selected.min(color_count.saturating_sub(1));
        if let Some(text) = pasted {
            apply_pasted_color(
                controller,
                self.selected,
                color_count,
                &text,
                &mut self.error,
            );
        }
        let revision = controller.revision();
        let colors = &controller.value().palette.colors;
        egui::Grid::new("portable-palette-grid")
            .spacing([3.0, 3.0])
            .show(ui, |ui| {
                for (index, color) in colors.iter().copied().enumerate() {
                    let rgb = color.to_rgb8();
                    let button = egui::Button::new("  ")
                        .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue));
                    if ui.add_sized([24.0, 24.0], button).clicked() {
                        self.selected = index;
                    }
                    if index % 16 == 15 {
                        ui.end_row();
                    }
                }
            });
        if let Some(color) = controller
            .value()
            .palette
            .colors
            .get(self.selected)
            .copied()
        {
            ui.separator();
            ui.label(
                text(catalog, Key::PaletteDocumentColorFormat)
                    .replace("{index}", &format!("{:03X}", self.selected))
                    .replace("{value}", &format!("{:04X}", color.0)),
            );
            let rgb = color.to_rgb8();
            let mut value = [rgb.red, rgb.green, rgb.blue];
            if ui.color_edit_button_srgb(&mut value).changed() {
                let replacement = Bgr555::from_rgb8(Rgb8 {
                    red: value[0],
                    green: value[1],
                    blue: value[2],
                });
                let ownership = PaletteOwnership::editable(colors.len());
                let edit = PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                    index: self.selected,
                    color: replacement,
                }]);
                if let Err(error) = controller.apply_edits(revision, &ownership, &[edit]) {
                    self.error = Some(error.to_string());
                }
            }
        }
    }

    fn begin_save(
        controller: &mut PaletteDocumentController,
        worker: &mut crate::persistence_worker::PersistenceWorker,
        error_slot: &mut Option<String>,
    ) {
        match controller.begin_save() {
            Ok(snapshot) => {
                if let Err(error) = worker.start(
                    snapshot.request_id,
                    crate::persistence_worker::PersistenceTarget::Replace(snapshot.path),
                    snapshot.bytes,
                ) {
                    let _cancel_result = controller.cancel_save(snapshot.request_id);
                    *error_slot = Some(error);
                }
            }
            Err(error) => *error_slot = Some(error.to_string()),
        }
    }

    fn poll_save(&mut self, context: &egui::Context) {
        let Some(completion) = self.save_worker.show(context) else {
            return;
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("palette save completed after its document was closed".into());
            return;
        };
        let result = match completion.result {
            Ok(()) => controller.acknowledge_save(completion.request_id),
            Err(error) => {
                let cancellation = controller.cancel_save(completion.request_id);
                self.error = Some(error);
                cancellation
            }
        };
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn clipboard_controls(
    ui: &mut egui::Ui,
    controller: &mut PaletteDocumentController,
    selected: usize,
    error_slot: &mut Option<String>,
    catalog: Option<&LocalizationCatalog>,
) {
    if ui
        .button(text(catalog, Key::NativeAssetsPaletteCopyColor))
        .clicked()
        && let Some(color) = controller.value().palette.colors.get(selected)
    {
        if let Err(error) = native_clipboard::copy_palette_color_to_system(ui.ctx(), *color) {
            *error_slot = Some(error);
        }
    }
    if ui
        .button(text(catalog, Key::NativeAssetsPalettePasteColor))
        .clicked()
    {
        match native_clipboard::request_palette_color_paste(ui.ctx()) {
            Ok(Some(color)) => {
                let color_count = controller.value().palette.colors.len();
                apply_color(controller, selected, color_count, color, error_slot);
            }
            Ok(None) => {}
            Err(error) => *error_slot = Some(error),
        }
    }
}

fn apply_pasted_color(
    controller: &mut PaletteDocumentController,
    selected: usize,
    color_count: usize,
    text: &str,
    error_slot: &mut Option<String>,
) {
    match native_clipboard::decode_palette_color(text) {
        Ok(color) => apply_color(controller, selected, color_count, color, error_slot),
        Err(error) => *error_slot = Some(error),
    }
}

fn apply_color(
    controller: &mut PaletteDocumentController,
    selected: usize,
    color_count: usize,
    color: Bgr555,
    error_slot: &mut Option<String>,
) {
    let ownership = PaletteOwnership::editable(color_count);
    let edit = PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
        index: selected,
        color,
    }]);
    if let Err(error) = controller.apply_edits(controller.revision(), &ownership, &[edit]) {
        *error_slot = Some(error.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_portable_palette_surface_uses_every_document_key() {
        let source = include_str!("palette_editor.rs");
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("PaletteDocument"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}")),
                "missing portable palette label {key:?}"
            );
        }
    }

    #[test]
    fn complete_portable_palette_surface_has_no_literal_widget_text() {
        let source = include_str!("palette_editor.rs");
        for literal_widget in [
            "Window::new(\"",
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "portable palette editor regressed to fixed widget text: {literal_widget}"
            );
        }
        assert_eq!(source.matches("Button::new(\"  \")").count(), 1);
    }
    use lm_graphics::{Palette, PaletteInterchangeFile};

    fn controller() -> PaletteDocumentController {
        let file = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0); 16],
            },
        };
        PaletteDocumentController::decode("palette.lmpal".into(), &file.encode().unwrap()).unwrap()
    }

    #[test]
    fn clean_close_is_immediate_but_dirty_close_requires_confirmation() {
        let mut clean = PaletteEditor {
            controller: Some(controller()),
            ..PaletteEditor::default()
        };
        assert!(clean.request_close(false));
        assert!(!clean.is_open());

        let mut dirty_controller = controller();
        dirty_controller
            .apply_edits(
                0,
                &PaletteOwnership::editable(16),
                &[PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                    index: 1,
                    color: Bgr555(1),
                }])],
            )
            .unwrap();
        let mut dirty = PaletteEditor {
            controller: Some(dirty_controller),
            ..PaletteEditor::default()
        };
        assert!(!dirty.request_close(true));
        assert!(dirty.is_open());
        assert_eq!(dirty.pending_close, Some(PendingClose::Application));
    }
}
