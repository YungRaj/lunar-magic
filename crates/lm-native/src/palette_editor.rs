use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    native_clipboard,
};
use eframe::egui;
use lm_app::{PaletteControllerEdit, PaletteDocumentController};
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
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
            egui::Window::new("Portable Palette Editor")
                .default_size([520.0, 420.0])
                .show(context, |ui| self.contents(ui));
        }
        if let Some(pending) = self.pending_close {
            egui::Window::new("Unsaved palette")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.label("Discard unsaved palette changes?");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.pending_close = None;
                        }
                        if ui.button("Discard").clicked() {
                            self.controller = None;
                            self.pending_close = None;
                            quit_approved = pending == PendingClose::Application;
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Palette error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
        quit_approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
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
            if ui
                .add_enabled(controller.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                if let Err(error) = controller.undo(revision) {
                    self.error = Some(error.to_string());
                }
            }
            if ui
                .add_enabled(controller.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                if let Err(error) = controller.redo(revision) {
                    self.error = Some(error.to_string());
                }
            }
            if ui
                .add_enabled(save_available, egui::Button::new("Save"))
                .clicked()
            {
                Self::begin_save(controller, &mut self.save_worker, &mut self.error);
            }
            clipboard_controls(ui, controller, self.selected, &mut self.error);
            ui.label(if controller.is_modified() {
                "Modified"
            } else {
                "Saved"
            });
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
            ui.label(format!(
                "Color {:03X} — BGR555 {:04X}",
                self.selected, color.0
            ));
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

fn clipboard_controls(
    ui: &mut egui::Ui,
    controller: &mut PaletteDocumentController,
    selected: usize,
    error_slot: &mut Option<String>,
) {
    if ui.button("Copy color").clicked()
        && let Some(color) = controller.value().palette.colors.get(selected)
    {
        if let Err(error) = native_clipboard::copy_palette_color_to_system(ui.ctx(), *color) {
            *error_slot = Some(error);
        }
    }
    if ui.button("Paste color").clicked() {
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
