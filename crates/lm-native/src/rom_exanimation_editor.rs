mod clipboard;
mod lifecycle;
mod workspace;

use workspace::Workspace;

use crate::{
    exanimation_form::{self, GlobalForm, RecordForm},
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, ExAnimationControllerEdit, ExtendedUiTextKey as Key, LocalizationCatalog,
};

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
    pub(crate) fn stage_recovery_on_project(
        &self,
        app: &AppState,
        staged: &mut lm_project::Project,
    ) -> Result<u16, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("ExAnimation workspace is closed")?;
        if !workspace.any_modified() {
            return Err("ExAnimation workspace has no active staged recovery edit".into());
        }
        if !workspace.controller.is_modified() {
            return Err("inactive ExAnimation recovery domain is unexpectedly modified".into());
        }
        if workspace.controller.revision() != app.project_revision() {
            return Err(
                "ExAnimation recovery controller was prepared from a stale revision".into(),
            );
        }
        let options =
            workspace.save_options_for_image(&self.search_start, &self.search_end, &staged.rom)?;
        workspace
            .controller
            .save_to_project(staged, &options)
            .map_err(|error| error.to_string())?;
        Ok(workspace.slot)
    }

    pub(crate) fn staged_recovery_mutation(
        &self,
        app: &AppState,
    ) -> Result<Option<(lm_project::RomMutation, u16)>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("ExAnimation workspace is closed")?;
        if !workspace.any_modified() {
            return Ok(None);
        }
        if !workspace.controller.is_modified() {
            return Err("inactive ExAnimation recovery domain is unexpectedly modified".into());
        }
        let command = self.prepare_commit()?;
        let Command::CommitRomMutation {
            expected_revision,
            mutation,
            ..
        } = command
        else {
            return Err("ExAnimation recovery expected one prepared ROM mutation".into());
        };
        if expected_revision != app.project_revision() {
            return Err("ExAnimation recovery mutation was prepared from a stale revision".into());
        }
        Ok(Some((mutation, workspace.slot)))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        workspace.any_modified().then(|| {
            let alternate_revision = workspace
                .alternate_controller
                .as_ref()
                .filter(|controller| controller.is_modified())
                .map_or(0, |controller| controller.revision().rotate_left(7));
            app.project_revision().wrapping_mul(0x8ebc_6af0_9c88_c6e3)
                ^ workspace.controller.revision().rotate_left(31)
                ^ alternate_revision
                ^ u64::from(workspace.editing_global)
                ^ 0x4558_414e_494d_0000
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("ExAnimation workspace is closed")?;
        if !workspace.any_modified() {
            return Ok(app.recovery_snapshot());
        }
        let (mutation, slot) = self
            .staged_recovery_mutation(app)?
            .ok_or("staged ExAnimation mutation disappeared")?;
        app.recovery_snapshot_with_mutation(&mutation, Some(slot))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
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
            egui::Window::new(text(catalog, Key::RomExAnimationTitle))
                .default_size([760.0, 680.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision, catalog) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }
    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let (target_label, can_switch, target_modified, global_unavailable) =
            self.workspace.as_ref().map(|workspace| {
                (
                    workspace.target_label(catalog),
                    workspace.alternate_controller.is_some() && !workspace.controller.is_modified(),
                    workspace.controller.is_modified(),
                    workspace.global_unavailable.clone(),
                )
            })?;
        let mut switch_target = false;
        ui.horizontal(|ui| {
            ui.heading(target_label);
            if ui
                .add_enabled(
                    can_switch,
                    egui::Button::new(text(catalog, Key::RomExAnimationSwitchDomain)),
                )
                .clicked()
            {
                switch_target = true;
            }
        });
        if let Some(error) = global_unavailable {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, Key::RomExAnimationGlobalUnavailableFormat)
                    .replace("{error}", &error),
            );
        }
        if target_modified {
            ui.label(text(catalog, Key::RomExAnimationSwitchBlocked));
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
                text(catalog, Key::RomPaletteStaleNotice),
            );
        }
        ui.columns(2, |columns| {
            self.record_list(&mut columns[0], stale, catalog);
            self.properties(&mut columns[1], stale, catalog);
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
            ui.label(text(catalog, Key::RomPaletteAllocation));
            ui.text_edit_singleline(&mut self.search_start);
            ui.label(text(catalog, Key::RomPaletteRangeSeparator));
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new(text(catalog, Key::RomExAnimationCommit)),
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
                egui::Button::new(text(catalog, Key::RomPaletteCommitReclaim)),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(text(
            catalog,
            if modified {
                Key::RomExAnimationStaged
            } else {
                Key::RomExAnimationUnmodified
            },
        ));
        None
    }
    fn record_list(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.heading(text(catalog, Key::ExAnimationDocumentRecords));
        let labels = self
            .workspace
            .as_ref()
            .map(|w| {
                w.controller
                    .animation()
                    .records
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        text(catalog, Key::ExAnimationDocumentRecordListFormat)
                            .replace("{index}", &format!("{i:02X}"))
                            .replace("{kind}", &format!("{:02X}", r.kind()))
                    })
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
            if ui
                .button(text(catalog, Key::NativeAssetsAnimationCopyRecord))
                .clicked()
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
                    egui::Button::new(text(catalog, Key::NativeAssetsAnimationPasteRecord)),
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
            .add_enabled(
                !stale,
                egui::Button::new(text(catalog, Key::RomExAnimationAppendRecord)),
            )
            .clicked()
        {
            self.apply_record(true);
        }
        if ui
            .add_enabled(
                !stale && self.selected_record < len,
                egui::Button::new(text(catalog, Key::ExAnimationDocumentRemoveSelected)),
            )
            .clicked()
        {
            self.apply(&[ExAnimationControllerEdit::RemoveRecord {
                index: self.selected_record,
            }]);
            self.selected_record = self.selected_record.saturating_sub(1);
        }
    }
    fn properties(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.heading(text(catalog, Key::ExAnimationDocumentSlotSettings));
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::NativeAssetsAnimationSetting));
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::NativeAssetsAnimationHeader));
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui
            .add_enabled(
                !stale,
                egui::Button::new(text(catalog, Key::NativeAssetsAnimationApplySlots)),
            )
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
        ui.heading(text(catalog, Key::NativeAssetsAnimationTrigger));
        if ui
            .add(egui::Slider::new(&mut self.trigger_index, 0..=15))
            .changed()
        {
            self.load_trigger();
        }
        ui.checkbox(
            &mut self.trigger_enabled,
            text(catalog, Key::NativeAssetsAnimationEnabled),
        );
        ui.add_enabled(
            self.trigger_enabled,
            egui::TextEdit::singleline(&mut self.trigger_value),
        );
        if ui
            .add_enabled(
                !stale,
                egui::Button::new(text(catalog, Key::NativeAssetsAnimationApplyTrigger)),
            )
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
        ui.heading(
            text(catalog, Key::ExAnimationDocumentRecordFormat)
                .replace("{index}", &format!("{:02X}", self.selected_record)),
        );
        for (key, field) in [
            (Key::NativeAssetsAnimationKind, &mut self.record.kind),
            (
                Key::NativeAssetsAnimationTrigger,
                &mut self.record.size_mode,
            ),
            (
                Key::NativeAssetsAnimationDestination,
                &mut self.record.destination,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(text(catalog, key));
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(
            &mut self.record.destination_flag,
            text(catalog, Key::NativeAssetsAnimationDestinationFlag),
        );
        ui.label(text(catalog, Key::NativeAssetsAnimationSourceWords));
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(7));
        if !self.record_editable {
            ui.label(text(catalog, Key::RomExAnimationSpecialTransferNotice));
        }
        let exists = self
            .workspace
            .as_ref()
            .is_some_and(|w| self.selected_record < w.controller.animation().records.len());
        self.frame_clipboard(ui, stale, exists, catalog);
        if ui
            .add_enabled(
                !stale && exists && self.record_editable,
                egui::Button::new(text(catalog, Key::RomExAnimationReplaceRecord)),
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

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::{RomExAnimationEditor, Workspace};
    use lm_app::{Command, ExAnimationController, ExAnimationControllerEdit};
    use lm_graphics::CompactExAnimation;
    use lm_project::{ExAnimationSaveOptions, Project};
    use lm_rats::AllocationPolicy;
    use lm_rom::RomImage;

    #[test]
    fn complete_rom_exanimation_surface_uses_every_typed_key() {
        let sources = [
            include_str!("rom_exanimation_editor.rs"),
            include_str!("rom_exanimation_editor/clipboard.rs"),
            include_str!("rom_exanimation_editor/lifecycle.rs"),
            include_str!("rom_exanimation_editor/workspace.rs"),
        ]
        .join("\n");
        for key in lm_app::ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("RomExAnimation"))
        {
            assert!(
                sources.contains(&format!("{key:?}")),
                "missing ROM ExAnimation label {key:?}"
            );
        }
    }

    #[test]
    fn complete_rom_exanimation_surface_has_no_literal_widget_text() {
        let sources = [
            include_str!("rom_exanimation_editor.rs"),
            include_str!("rom_exanimation_editor/clipboard.rs"),
            include_str!("rom_exanimation_editor/lifecycle.rs"),
        ]
        .join("\n");
        for literal_widget in [
            "Window::new(\"",
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "Button::new(\"",
            ".prefix(\"",
        ] {
            assert!(
                !sources.contains(literal_widget),
                "ROM ExAnimation editor regressed to fixed widget text: {literal_widget}"
            );
        }
    }

    #[test]
    fn staged_rom_exanimation_edit_is_recovered_without_committing_live_project() {
        let mut source = lm_app::AppState::default();
        source
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        source
            .dispatch(Command::ConvertRomTo64MbitExLoRom {
                expected_revision: source.project_revision(),
            })
            .unwrap();
        let profile = lm_profile::test_support::profile();
        let initial = CompactExAnimation {
            setting: 1,
            header_value: 0x1234,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        };
        let mut project =
            Project::new(RomImage::from_bytes(source.project().unwrap().save_snapshot()).unwrap());
        project
            .save_exanimation_with_checksum(
                0,
                &initial,
                profile.exanimation,
                &profile.exanimation_double_size_modes,
                0x7fdc,
                &ExAnimationSaveOptions {
                    allocation: AllocationPolicy {
                        search: 0x600000..0x680000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0, 0xff],
                        protected: Vec::new(),
                    },
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
            )
            .unwrap();

        let mut app = lm_app::AppState::default();
        app.load_rom(project.save_snapshot()).unwrap();
        app.dispatch(Command::ShowExAnimation(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = ExAnimationController::decode(
            &snapshot,
            profile.exanimation,
            &profile.exanimation_double_size_modes,
        )
        .unwrap();
        let replacement = 7;
        controller
            .apply_edits(&[ExAnimationControllerEdit::SetSetting(replacement)])
            .unwrap();
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
        let editor = RomExAnimationEditor {
            workspace: Some(Workspace {
                controller,
                alternate_controller: None,
                global_unavailable: None,
                editing_global: false,
                profile: profile.clone(),
                modes: profile.exanimation_double_size_modes,
                slot: 0,
                image,
                internal_header: snapshot.identity.internal_header_offset,
            }),
            search_start: "600000".into(),
            search_end: "680000".into(),
            ..RomExAnimationEditor::default()
        };

        assert!(editor.staged_recovery_generation(&app).is_some());
        let mut staged = app.project().unwrap().clone();
        assert_eq!(
            editor.stage_recovery_on_project(&app, &mut staged).unwrap(),
            0
        );
        assert_eq!(
            staged
                .load_exanimation(
                    0,
                    profile.exanimation,
                    &profile.exanimation_double_size_modes,
                )
                .unwrap()
                .setting,
            replacement
        );
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = lm_app::AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let animation = reopened
            .project()
            .unwrap()
            .load_exanimation(
                0,
                profile.exanimation,
                &profile.exanimation_double_size_modes,
            )
            .unwrap();
        assert_eq!(animation.setting, replacement);
    }
}
