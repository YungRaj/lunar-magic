use crate::expanded_settings_editor_form::ExpandedSettingsForm;
use eframe::egui;
use lm_app::{AppState, Command, ExpandedSettingsController, RevisionProfileControllers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomExpandedSettingsEditor {
    controller: Option<ExpandedSettingsController>,
    form: ExpandedSettingsForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomExpandedSettingsEditor {
    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let controller = self.controller.as_ref()?;
        controller.is_modified().then(|| {
            let content_revision = controller
                .record()
                .encoded()
                .iter()
                .fold(0x4558_5041_4e44_4544_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(*byte)
                });
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ controller.revision().rotate_left(29)
                ^ content_revision
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("expanded-settings workspace is closed")?;
        if !controller.is_modified() {
            return Ok(app.recovery_snapshot());
        }
        let prepared = controller
            .prepare_commit("Recover staged installed expanded settings")
            .map_err(|error| error.to_string())?;
        if prepared.expected_revision != app.project_revision() {
            return Err(
                "expanded-settings recovery mutation was prepared from a stale revision".into(),
            );
        }
        app.recovery_snapshot_with_mutation(&prepared.mutation, app.current_level())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .profiled_controller_snapshot()
            .map_err(|e| e.to_string())
            .and_then(|profiled| {
                profiled
                    .profile
                    .decode_expanded_settings(&profiled.snapshot)
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(controller) => {
                self.form = ExpandedSettingsForm::load(controller.record());
                self.controller = Some(controller);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn open_detected(&mut self, app: &AppState) -> Result<bool, String> {
        if self.is_open() {
            return Ok(true);
        }
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        if !matches!(snapshot.mode, lm_app::EditorMode::Level(_)) {
            return Err("select a level before opening expanded settings".into());
        }
        let image = lm_rom::RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(|error| error.to_string())?;
        let project = lm_project::Project::new(image);
        let Some(layout) = lm_profile::smw_us_v1_installed_expanded_settings_layout(&project)
            .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        let controller = ExpandedSettingsController::decode(&snapshot, layout)
            .map_err(|error| error.to_string())?;
        self.form = ExpandedSettingsForm::load(controller.record());
        self.controller = Some(controller);
        self.error = None;
        Ok(true)
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
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
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.controller.is_some() {
            egui::Window::new("ROM Expanded Settings")
                .default_size([470.0, 580.0])
                .show(context, |ui| {
                    command = self.contents(ui, project_revision);
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let controller = self.controller.as_ref()?;
        let stale = controller.revision() != project_revision;
        ui.label("Exact installed 32-byte record; unknown words remain lossless.");
        if stale {
            ui.colored_label(egui::Color32::YELLOW, "The ROM changed after this editor was opened. Close and reopen it before committing.");
        }
        ui.heading("Custom Layer 3 tilemap graphics");
        ui.checkbox(
            &mut self.form.layer3_enabled,
            "Enable custom Layer 3 tilemap",
        );
        ui.horizontal(|ui| {
            ui.label("GFX/ExGFX file");
            ui.text_edit_singleline(&mut self.form.layer3_file);
        });
        ui.add(
            egui::Slider::new(&mut self.form.layer3_length_selector, 0..=3).text("Length selector"),
        );
        ui.add(
            egui::Slider::new(&mut self.form.layer3_offset_selector, 0..=3)
                .text("Destination selector"),
        );
        if ui
            .add_enabled(!stale, egui::Button::new("Stage Layer 3 settings"))
            .clicked()
        {
            self.stage_edits(project_revision, self.form.layer3_edits());
        }
        ui.horizontal(|ui| {
            ui.label("Expanded mode");
            ui.text_edit_singleline(&mut self.form.layer3_expanded_mode);
        });
        ui.small("Exact 32-bit mode packed from the high nibbles of words 8–F.");
        if ui
            .add_enabled(!stale, egui::Button::new("Stage Layer 3 expanded mode"))
            .clicked()
        {
            self.stage_edits(project_revision, self.form.layer3_expanded_mode_edits());
        }
        ui.separator();
        ui.heading("Super GFX Bypass");
        ui.checkbox(
            &mut self.form.bypass_enabled,
            "Use per-level GFX/ExGFX files",
        );
        egui::Grid::new("rom-expanded-settings-super-gfx")
            .num_columns(4)
            .show(ui, |ui| {
                for (slot, label) in ["FG1", "FG2", "FG3", "BG1", "BG2", "BG3"]
                    .into_iter()
                    .enumerate()
                {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.form.bypass_foreground_background[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
                for (slot, label) in ["SP1", "SP2", "SP3", "SP4"].into_iter().enumerate() {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.form.bypass_sprites[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if ui
            .add_enabled(!stale, egui::Button::new("Stage Super GFX bypass"))
            .clicked()
        {
            self.stage_edits(project_revision, self.form.super_graphics_bypass_edits());
        }
        ui.separator();
        ui.heading("Sprite boundary interaction");
        ui.checkbox(
            &mut self.form.sprites_beyond_boundaries_use_air,
            "Sprites beyond level boundaries interact with air instead of water",
        );
        if ui
            .add_enabled(
                !stale,
                egui::Button::new("Stage sprite boundary interaction"),
            )
            .clicked()
        {
            self.stage_edits(project_revision, self.form.sprite_boundary_edits());
        }
        ui.separator();
        ui.label("All sixteen exact native words");
        egui::Grid::new("rom-expanded-settings-words")
            .striped(true)
            .show(ui, |ui| {
                for (index, word) in self.form.words.iter_mut().enumerate() {
                    ui.label(format!("Word {index:X}"));
                    ui.text_edit_singleline(word);
                    ui.end_row();
                }
            });
        let mut result = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Stage all words"))
                .clicked()
            {
                self.stage_edits(project_revision, self.form.edits());
            }
            let modified = self
                .controller
                .as_ref()
                .is_some_and(ExpandedSettingsController::is_modified);
            if ui
                .add_enabled(modified && !stale, egui::Button::new("Commit to ROM"))
                .clicked()
            {
                if let Some(controller) = self.controller.as_ref() {
                    match controller.prepare_commit("Edit installed expanded settings") {
                        Ok(prepared) => result = Some(prepared.into_command()),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                } else {
                    self.error = Some("expanded-settings workspace is closed".into());
                }
            }
            ui.label(if modified { "Staged" } else { "Unchanged" });
        });
        result
    }

    fn stage_edits(&mut self, project_revision: u64, edits: Result<Vec<(usize, u16)>, String>) {
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("expanded-settings workspace is closed".into());
            return;
        };
        if controller.revision() != project_revision {
            self.error = Some("the ROM changed after this editor was opened".into());
            return;
        }
        let edits = match edits {
            Ok(edits) => edits,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if let Err(error) = controller.apply_word_edits(&edits) {
            self.error = Some(error.to_string());
        } else {
            self.form = ExpandedSettingsForm::load(controller.record());
        }
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged ROM settings?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("These staged settings have not been committed to the ROM.");
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
            egui::Window::new("ROM expanded-settings error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{ExpandedLevelHeader, SuperGraphicsBypass};
    use lm_project::ExpandedLevelSettingsLayout;
    use lm_rom::{Mapper, SnesChecksum, compute_snes_checksum};

    #[test]
    fn staged_expanded_settings_are_recovered_without_committing_live_project() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x2000,
            entries: 0x200,
            stride: 0x20,
        };
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let mut controller =
            ExpandedSettingsController::decode(&app.controller_snapshot().unwrap(), layout)
                .unwrap();
        controller.apply_word_edits(&[(7, 0x3456)]).unwrap();
        let editor = RomExpandedSettingsEditor {
            form: ExpandedSettingsForm::load(controller.record()),
            controller: Some(controller),
            ..Default::default()
        };

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let record = reopened
            .project()
            .unwrap()
            .load_expanded_level_settings(0x105, layout)
            .unwrap();
        assert_eq!(record.word(7).unwrap(), 0x3456);
    }

    #[test]
    fn focused_semantic_controls_commit_reopen_checksum_undo_and_reject_stale_stage() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let original = bytes.clone();
        let layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x2000,
            entries: 0x200,
            stride: 0x20,
        };
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let controller =
            ExpandedSettingsController::decode(&app.controller_snapshot().unwrap(), layout)
                .unwrap();
        let revision = controller.revision();
        let mut editor = RomExpandedSettingsEditor {
            form: ExpandedSettingsForm::load(controller.record()),
            controller: Some(controller),
            ..Default::default()
        };

        editor.form.bypass_enabled = true;
        editor.form.bypass_foreground_background = [1, 2, 3, 4, 5, 6];
        editor.form.bypass_sprites = [0x101, 0x202, 0x303, 0x404];
        editor.stage_edits(revision, editor.form.super_graphics_bypass_edits());
        editor.form.layer3_expanded_mode = "89AFCDEF".into();
        editor.stage_edits(revision, editor.form.layer3_expanded_mode_edits());
        editor.form.sprites_beyond_boundaries_use_air = false;
        editor.stage_edits(revision, editor.form.sprite_boundary_edits());

        let staged = editor.controller.as_ref().unwrap().record();
        assert_eq!(
            ExpandedLevelHeader::from(staged).super_graphics_bypass(),
            SuperGraphicsBypass {
                enabled: true,
                foreground_background: [1, 2, 3, 4, 5, 6],
                sprites: [0x101, 0x202, 0x303, 0x404],
            }
        );
        assert_eq!(staged.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
        assert!(!ExpandedLevelHeader::from(staged).sprites_beyond_boundaries_use_air());

        let before_stale = staged.clone();
        editor.form.layer3_expanded_mode = "01234567".into();
        editor.stage_edits(revision + 1, editor.form.layer3_expanded_mode_edits());
        assert_eq!(editor.controller.as_ref().unwrap().record(), &before_stale);
        assert!(editor.error.is_some());

        let command = editor
            .controller
            .as_ref()
            .unwrap()
            .prepare_commit("Focused semantic settings")
            .unwrap()
            .into_command();
        app.dispatch(command).unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_expanded_level_settings(0x105, layout)
            .unwrap();
        assert_eq!(reopened, before_stale);
        assert_eq!(
            SnesChecksum::decode(app.project().unwrap().rom.logical_bytes(), 0x7fdc).unwrap(),
            compute_snes_checksum(app.project().unwrap().rom.logical_bytes(), 0x7fdc).unwrap()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.as_file_bytes().as_ref(),
            original
        );
    }
}
