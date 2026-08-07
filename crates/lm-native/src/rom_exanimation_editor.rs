mod clipboard;
mod lifecycle;
mod workspace;

use workspace::Workspace;

use crate::{
    exanimation_form::{self, GlobalForm, RecordForm},
    native_clipboard,
};
use eframe::egui;
use lm_app::{AppState, Command, ExAnimationControllerEdit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Record,
    Frame,
}
#[derive(Default)]
pub(crate) struct RomExAnimationEditor {
    workspace: Option<Workspace>,
    global: GlobalForm,
    record: RecordForm,
    selected_record: usize,
    selected_frame: usize,
    trigger_index: usize,
    trigger_enabled: bool,
    trigger_value: String,
    loaded: Option<(u64, usize)>,
    record_editable: bool,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    paste_target: Option<PasteTarget>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
}

impl RomExAnimationEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = match self.manifest_loader.show(context, revision) {
            Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if self.workspace.is_some() {
            self.load();
            egui::Window::new("ROM ExAnimation Editor")
                .default_size([760.0, 680.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let (target_label, can_switch, target_modified, global_unavailable) =
            self.workspace.as_ref().map(|workspace| {
                (
                    workspace.target_label(),
                    workspace.alternate_controller.is_some() && !workspace.controller.is_modified(),
                    workspace.controller.is_modified(),
                    workspace.global_unavailable.clone(),
                )
            })?;
        let mut switch_target = false;
        ui.horizontal(|ui| {
            ui.heading(target_label);
            if ui
                .add_enabled(can_switch, egui::Button::new("Switch level/global domain"))
                .clicked()
            {
                switch_target = true;
            }
        });
        if let Some(error) = global_unavailable {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Global ExAnimation is unavailable: {error}"),
            );
        }
        if target_modified {
            ui.label("Commit or revert this domain before switching level/global targets.");
        }
        if switch_target
            && self
                .workspace
                .as_mut()
                .is_some_and(Workspace::switch_target)
        {
            self.selected_record = 0;
            self.selected_frame = 0;
            self.invalidate();
            self.load();
        }
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        ui.columns(2, |columns| {
            self.record_list(&mut columns[0], stale);
            self.properties(&mut columns[1], stale);
        });
        if !stale && let Some(text) = pasted {
            match self.paste_target.take() {
                Some(PasteTarget::Record) => self.paste_record(&text),
                Some(PasteTarget::Frame) => self.paste_frame(&text),
                None => {}
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit ExAnimation to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged animation changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn record_list(&mut self, ui: &mut egui::Ui, stale: bool) {
        ui.heading("Records");
        let labels = self
            .workspace
            .as_ref()
            .map(|w| {
                w.controller
                    .animation()
                    .records
                    .iter()
                    .enumerate()
                    .map(|(i, r)| format!("{i:02X}: kind {:02X}", r.kind()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (index, label) in labels.iter().enumerate() {
            if ui
                .selectable_value(&mut self.selected_record, index, label)
                .clicked()
            {
                self.loaded = None;
                self.load();
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Copy record").clicked()
                && let Some(record) = self.current_record()
            {
                match native_clipboard::encode_exanimation_record(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    !stale && !labels.is_empty(),
                    egui::Button::new("Paste record"),
                )
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Record);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        let len = labels.len();
        if ui
            .add_enabled(!stale, egui::Button::new("Append form as record"))
            .clicked()
        {
            self.apply_record(true);
        }
        if ui
            .add_enabled(
                !stale && self.selected_record < len,
                egui::Button::new("Remove selected"),
            )
            .clicked()
        {
            self.apply(&[ExAnimationControllerEdit::RemoveRecord {
                index: self.selected_record,
            }]);
            self.selected_record = self.selected_record.saturating_sub(1);
        }
    }
    fn properties(&mut self, ui: &mut egui::Ui, stale: bool) {
        ui.heading("Slot settings");
        ui.horizontal(|ui| {
            ui.label("Setting");
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label("Header");
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui
            .add_enabled(!stale, egui::Button::new("Apply slot settings"))
            .clicked()
        {
            match self.global.parse() {
                Ok((setting, header)) => self.apply(&[
                    ExAnimationControllerEdit::SetSetting(setting),
                    ExAnimationControllerEdit::SetHeaderValue(header),
                ]),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.heading("Trigger");
        if ui
            .add(egui::Slider::new(&mut self.trigger_index, 0..=15))
            .changed()
        {
            self.load_trigger();
        }
        ui.checkbox(&mut self.trigger_enabled, "Enabled");
        ui.add_enabled(
            self.trigger_enabled,
            egui::TextEdit::singleline(&mut self.trigger_value),
        );
        if ui
            .add_enabled(!stale, egui::Button::new("Apply trigger"))
            .clicked()
        {
            let value = if self.trigger_enabled {
                exanimation_form::hex_u8(&self.trigger_value, "trigger value").map(Some)
            } else {
                Ok(None)
            };
            match value {
                Ok(value) => self.apply(&[ExAnimationControllerEdit::SetTrigger {
                    trigger: self.trigger_index,
                    value,
                }]),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.heading(format!("Record {:02X}", self.selected_record));
        for (label, field) in [
            ("Kind", &mut self.record.kind),
            ("Trigger", &mut self.record.size_mode),
            ("Destination", &mut self.record.destination),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(&mut self.record.destination_flag, "Destination flag");
        ui.label("Source words, one frame per line");
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(7));
        if !self.record_editable {
            ui.label("This transfer kind has no ordinary source-word payload.");
        }
        let exists = self
            .workspace
            .as_ref()
            .is_some_and(|w| self.selected_record < w.controller.animation().records.len());
        self.frame_clipboard(ui, stale, exists);
        if ui
            .add_enabled(
                !stale && exists && self.record_editable,
                egui::Button::new("Replace record"),
            )
            .clicked()
        {
            self.apply_record(false);
        }
    }
    fn apply_record(&mut self, append: bool) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        match self.record.parse(&workspace.modes) {
            Ok(record) => {
                let edit = if append {
                    ExAnimationControllerEdit::InsertRecord {
                        index: workspace.controller.animation().records.len(),
                        record,
                    }
                } else {
                    ExAnimationControllerEdit::ReplaceRecord {
                        index: self.selected_record,
                        record,
                    }
                };
                self.apply(&[edit]);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn apply(&mut self, edits: &[ExAnimationControllerEdit]) {
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("ExAnimation workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(edits) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
            self.load();
        }
    }
    fn load(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (workspace.controller.revision(), self.selected_record);
        if self.loaded == Some(key) {
            return;
        }
        let animation = workspace.controller.animation();
        self.global = GlobalForm::load(animation.setting, animation.header_value);
        self.selected_record = self
            .selected_record
            .min(animation.records.len().saturating_sub(1));
        if let Some(record) = animation.records.get(self.selected_record) {
            if let Ok(frames) = workspace.controller.record_frames(self.selected_record) {
                self.record = RecordForm::load(record, &frames);
                self.record_editable = true;
            } else {
                self.record = RecordForm::load(record, &[]);
                self.record_editable = false;
            }
        } else {
            self.record = RecordForm::default();
            self.record_editable = true;
        }
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger_index) != 0;
        self.trigger_value = format!("{:02X}", animation.trigger_values[self.trigger_index]);
        self.loaded = Some((workspace.controller.revision(), self.selected_record));
    }
    fn load_trigger(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let animation = workspace.controller.animation();
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger_index) != 0;
        self.trigger_value = format!("{:02X}", animation.trigger_values[self.trigger_index]);
    }
    fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        workspace.prepare_commit(&self.search_start, &self.search_end)
    }
    fn prepare_commit_owned(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        workspace.prepare_commit_with_reclamation(&self.search_start, &self.search_end, manifest)
    }
    fn invalidate(&mut self) {
        self.loaded = None;
    }
}
