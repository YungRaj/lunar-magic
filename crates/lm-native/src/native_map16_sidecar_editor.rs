use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    native_map16_sidecar_form::NativeMap16SidecarForm,
};
use eframe::egui;
use lm_app::{
    NativeMap16SidecarController, NativeMap16SidecarDocumentKind, NativeMap16SidecarEdit,
};
use lm_level::S16Sidecar;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    path: PathBuf,
    kind: NativeMap16SidecarDocumentKind,
}

#[derive(Default)]
pub(crate) struct NativeMap16SidecarEditor {
    controller: Option<NativeMap16SidecarController>,
    pending_open: Option<PendingOpen>,
    form: NativeMap16SidecarForm,
    loaded_key: Option<(u64, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
    loading_kind: Option<NativeMap16SidecarDocumentKind>,
}

impl NativeMap16SidecarEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_native_map16_sidecar() else {
            return;
        };
        self.pending_open = Some(PendingOpen {
            path,
            kind: NativeMap16SidecarDocumentKind::M16,
        });
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for Map16 sidecar loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for Map16 sidecar persistence to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
        if let Some(result) = self.loader.show(context) {
            let kind = self.loading_kind.take();
            match result {
                Err(error) => self.error = Some(error),
                Ok(mut loaded) => match (kind, loaded.files.pop()) {
                    (Some(kind), Some((path, bytes))) => {
                        match NativeMap16SidecarController::decode(path.clone(), kind, &bytes) {
                            Ok(controller) => {
                                self.controller = Some(controller);
                                self.loaded_key = None;
                            }
                            Err(error) => {
                                self.error = Some(error.to_string());
                                self.pending_open = Some(PendingOpen { path, kind });
                            }
                        }
                    }
                    (None, _) => {
                        self.error = Some("Map16 sidecar load lost its kind".into());
                    }
                    (_, None) => {
                        self.error = Some("Map16 sidecar loader returned no file".into());
                    }
                },
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        self.show_open_configuration(context);
        if self.controller.is_some() {
            self.clamp_and_load();
            egui::Window::new("Native Map16 Sidecar Editor")
                .default_size([540.0, 360.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn show_open_configuration(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Map16 sidecar interpretation")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.radio_value(
                        &mut pending.kind,
                        NativeMap16SidecarDocumentKind::M16,
                        ".m16 — exact 0x2000-byte custom-object table",
                    );
                    ui.radio_value(
                        &mut pending.kind,
                        NativeMap16SidecarDocumentKind::S16,
                        ".s16 — sparse sprite Map16 workspace",
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let request = BoundedRead::new(
            pending.path.clone(),
            u64::try_from(S16Sidecar::CAPACITY).unwrap_or(u64::MAX),
            "native Map16 sidecar",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => self.loading_kind = Some(pending.kind),
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let kind = match controller.value().kind() {
            NativeMap16SidecarDocumentKind::M16 => ".m16 exact",
            NativeMap16SidecarDocumentKind::S16 => ".s16 sparse canonical",
        };
        let count = controller.value().entry_count();
        let encoded_len = controller.value().encode().len();
        ui.label(format!(
            "Kind: {kind}; entries: {count}; save bytes: {encoded_len}"
        ));
        let previous = self.form.entry;
        ui.add(
            egui::Slider::new(&mut self.form.entry, 0..=count.saturating_sub(1)).text("Raw entry"),
        );
        if previous != self.form.entry {
            self.loaded_key = None;
            self.clamp_and_load();
        }
        ui.horizontal(|ui| {
            ui.label("Raw little-endian dword (hex)");
            ui.text_edit_singleline(&mut self.form.value);
        });
        if ui.button("Apply raw entry").clicked() {
            match self.form.edit() {
                Ok(edit) => self.apply_edit(edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
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
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked();
            ui.label(if modified { "Modified" } else { "Saved" });
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
            self.loaded_key = None;
        }
    }

    fn apply_edit(&mut self, edit: NativeMap16SidecarEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(&edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.loaded_key = None;
        }
    }

    fn clamp_and_load(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let count = controller.value().entry_count();
        self.form.entry = self.form.entry.min(count.saturating_sub(1));
        let key = (controller.revision(), self.form.entry);
        if self.loaded_key != Some(key) {
            let value = controller.value().entry(self.form.entry).unwrap_or(0);
            self.form = NativeMap16SidecarForm::load(self.form.entry, value);
            self.loaded_key = Some(key);
        }
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved native Map16 sidecar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved raw-entry changes?");
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
            egui::Window::new("Native Map16 sidecar error")
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
        self.controller = None;
        self.pending_open = None;
        self.pending_close = None;
        self.loaded_key = None;
    }
}
