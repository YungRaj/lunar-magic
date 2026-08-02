use super::NativeApplication;
use eframe::egui;
use lm_app::{Command, EditorMode, EmulatorTestRequest, ExternalTool, ProjectStatus};

impl NativeApplication {
    pub(super) fn menu_bar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        let capabilities = self.app.capabilities();
        egui::menu::bar(ui, |ui| {
            self.file_menu(context, ui, capabilities.project);
            self.edit_menu(context, ui, capabilities.history);
            self.view_menu(ui, capabilities.project);
            self.editors_menu(context, ui, capabilities.project);
            self.profile_menu(context, ui, capabilities.project);
            self.tools_menu(context, ui);
            self.documents_menu(ui);
            self.help_menu(ui);
        });
    }

    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Help", |ui| {
            if ui.button("Compatibility diagnostics…").clicked() {
                ui.close_menu();
                self.diagnostics_dialog.open(&self.app);
            }
            if ui.button("About Lunar Magic Rust…").clicked() {
                ui.close_menu();
                self.about_dialog.open();
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button("View", |ui| {
            let enabled =
                !matches!(status, ProjectStatus::Closed) && self.app.current_level().is_some();
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new("Special World Passed Graphics")
                        .selected(self.special_world_passed),
                )
                .clicked()
            {
                self.special_world_passed = !self.special_world_passed;
                self.vanilla_level_editor.invalidate_graphics_preview();
                self.rom_level_assets_editor.invalidate_graphics_preview();
            }
        });
    }

    fn file_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button("File", |ui| {
            if ui.button("Open…").clicked() {
                ui.close_menu();
                self.dispatch(context, Command::Open);
            }
            let recent = self.app.recent_documents().paths().to_vec();
            ui.add_enabled_ui(!recent.is_empty(), |ui| {
                ui.menu_button("Open Recent", |ui| {
                    for path in recent {
                        if ui.button(path.display().to_string()).clicked() {
                            ui.close_menu();
                            self.open_recent(context, path);
                        }
                    }
                });
            });
            let enabled = !matches!(status, ProjectStatus::Closed);
            for (label, command) in [
                ("Save", Command::Save),
                ("Save As…", Command::SaveAs),
                ("Close", Command::Close),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    ui.close_menu();
                    self.dispatch(context, command);
                }
            }
            if ui
                .add_enabled(enabled, egui::Button::new("Expand ROM…"))
                .clicked()
            {
                ui.close_menu();
                self.rom_expansion_dialog.open(&self.app);
            }
            if ui
                .add_enabled(enabled, egui::Button::new("Convert Copier Header…"))
                .clicked()
            {
                ui.close_menu();
                self.copier_header_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.level_usage_dialog.is_busy(),
                    egui::Button::new("Analyze Level Usage…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.level_usage_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(enabled, egui::Button::new("Restrict Level Access…"))
                .clicked()
            {
                ui.close_menu();
                self.level_access_restriction_dialog.open();
            }
            self.restore_point_menu_items(ui, status);
            if ui
                .add_enabled(
                    enabled && self.app.revision_profile().is_some(),
                    egui::Button::new("Migrate Graphics Compression…"),
                )
                .clicked()
            {
                ui.close_menu();
                self.graphics_migration_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.built_in_runtime_installer.is_open(),
                    egui::Button::new("Install Built-in Runtime…"),
                )
                .clicked()
            {
                ui.close_menu();
                self.built_in_runtime_installer.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.rats_reclamation_dialog.is_busy(),
                    egui::Button::new("Reclaim Owned RATS Blocks…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.rats_reclamation_dialog.choose_and_start(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            self.ips_menu_items(ui, enabled);
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.close_menu();
                self.request_quit(context);
            }
        });
    }

    fn restore_point_menu_item(&mut self, ui: &mut egui::Ui, status: ProjectStatus) {
        if ui
            .add_enabled(
                matches!(status, ProjectStatus::Closed) && !self.restore_point_dialog.is_busy(),
                egui::Button::new("Restore ROM from Restore Point…"),
            )
            .clicked()
        {
            ui.close_menu();
            if let Err(error) = self.restore_point_dialog.choose_and_open() {
                self.effects.error = Some(error);
            }
        }
    }

    fn restore_point_menu_items(&mut self, ui: &mut egui::Ui, status: ProjectStatus) {
        self.restore_point_menu_item(ui, status);
        self.create_restore_point_menu_item(ui, !matches!(status, ProjectStatus::Closed));
    }

    fn create_restore_point_menu_item(&mut self, ui: &mut egui::Ui, enabled: bool) {
        use crate::restore_point_dialog::RestoreAppendMode;

        let actions = [
            ("Create Full Restore Point…", None),
            (
                "Append Delta Restore Point…",
                Some(RestoreAppendMode::Delta),
            ),
            ("Append Full Restore Point…", Some(RestoreAppendMode::Full)),
            (
                "Append Automatic Restore Point…",
                Some(RestoreAppendMode::Automatic),
            ),
        ];
        for (label, mode) in actions {
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                ui.close_menu();
                if matches!(mode, Some(RestoreAppendMode::Automatic)) {
                    self.restore_point_dialog.open_automatic_policy();
                    continue;
                }
                let result = mode.map_or_else(
                    || crate::restore_point_dialog::create_full_for_open_project(&self.app),
                    |mode| crate::restore_point_dialog::append_for_open_project(&self.app, mode),
                );
                if let Err(error) = result {
                    self.effects.error = Some(error);
                }
            }
        }
    }

    fn ips_menu_items(&mut self, ui: &mut egui::Ui, project_open: bool) {
        if ui
            .add_enabled(
                project_open && !self.ips_patch_dialog.is_busy(),
                egui::Button::new("Apply IPS Patch…"),
            )
            .clicked()
        {
            ui.close_menu();
            if let Err(error) = self.ips_patch_dialog.choose_and_start(&self.app) {
                self.effects.error = Some(error);
            }
        }
        if ui
            .add_enabled(
                !self.ips_create_dialog.is_busy(),
                egui::Button::new("Create IPS Patch…"),
            )
            .clicked()
        {
            ui.close_menu();
            if let Err(error) = self.ips_create_dialog.choose_and_start() {
                self.effects.error = Some(error);
            }
        }
    }

    fn edit_menu(
        &mut self,
        context: &egui::Context,
        ui: &mut egui::Ui,
        history: lm_app::HistoryCapabilities,
    ) {
        ui.menu_button("Edit", |ui| {
            for (label, enabled, command) in [
                ("Undo", history.undo, Command::Undo),
                ("Redo", history.redo, Command::Redo),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    ui.close_menu();
                    self.dispatch(context, command);
                }
            }
        });
    }

    fn editors_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button("Editors", |ui| {
            let enabled = !matches!(status, ProjectStatus::Closed);
            let level = match self.app.mode {
                EditorMode::Level(level) | EditorMode::Layer3(level) => level,
                _ => u16::from_str_radix(self.level_text.trim(), 16).unwrap_or(0),
            };
            for (label, command) in [
                ("Level", Command::SelectLevel(level)),
                ("Overworld", Command::ShowOverworld),
                ("Map16", Command::ShowMap16),
                ("Graphics", Command::ShowGraphics(0)),
                ("Palette", Command::ShowPalette(0)),
                ("ExAnimation", Command::ShowExAnimation(0)),
                ("Layer 3", Command::ShowLayer3(level)),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    ui.close_menu();
                    self.dispatch(context, command);
                }
            }
            self.rom_editor_menu_items(ui, enabled);
        });
    }

    fn profile_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button("Profile", |ui| {
            let enabled = !matches!(status, ProjectStatus::Closed);
            if ui
                .add_enabled(
                    enabled && !self.profile_loader.is_running(),
                    egui::Button::new("Install Revision Profile…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.profile_loader.choose_and_start() {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    self.app.revision_profile().is_some() && !self.profile_loader.is_running(),
                    egui::Button::new("Clear Profile"),
                )
                .clicked()
            {
                ui.close_menu();
                if self.try_dispatch(context, Command::ClearRevisionProfile) {
                    self.renderer.invalidate();
                }
            }
            if ui
                .add_enabled(
                    self.app.revision_profile().is_some()
                        && !self.revision_patch_installer.is_busy(),
                    egui::Button::new("Install Revision Patch…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.revision_patch_installer.choose_and_start(&self.app) {
                    self.effects.error = Some(error);
                }
            }
        });
    }

    fn open_recent(&mut self, context: &egui::Context, path: std::path::PathBuf) {
        self.effects.request_rom_path(path);
        match self.app.dispatch(Command::Open) {
            Ok(effects) => self.effects.handle(&mut self.app, context, effects),
            Err(error) => {
                self.effects.cancel_requested_rom_path();
                self.effects.error = Some(error.to_string());
            }
        }
    }

    fn tools_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        ui.menu_button("Tools", |ui| {
            if ui.button("Keyboard Shortcuts…").clicked() {
                ui.close_menu();
                self.shortcut_editor.open(self.app.shortcuts());
            }
            if ui.button("Customize Toolbar…").clicked() {
                ui.close_menu();
                self.toolbar_editor.open(self.app.toolbar());
            }
            let locale = self.app.localization().map_or_else(
                || "Built-in English".to_owned(),
                |catalog| catalog.locale().into(),
            );
            ui.menu_button(format!("Language ({locale})"), |ui| {
                if ui
                    .add_enabled(
                        !self.configuration_loader.is_running(),
                        egui::Button::new("Install Language Catalog…"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    if let Err(error) = self.configuration_loader.choose_localization_and_start() {
                        self.effects.error = Some(error);
                    }
                }
                if ui
                    .add_enabled(
                        self.app.localization().is_some(),
                        egui::Button::new("Use Built-in English"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.app.clear_localization();
                    self.app.status = "Restored built-in English".into();
                }
            });
            if ui
                .add_enabled(
                    !self.configuration_loader.is_running(),
                    egui::Button::new("Install Frontend Configuration…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.configuration_loader.choose_frontend_and_start() {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    !self.configuration_loader.is_running(),
                    egui::Button::new("Install Tool Configuration…"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.configuration_loader.choose_external_tools_and_start() {
                    self.effects.error = Some(error);
                }
            }
            let tools = self
                .app
                .external_tools()
                .iter()
                .map(|tool| (tool.id.clone(), tool.name.clone()))
                .collect::<Vec<_>>();
            let emulator_tools = self
                .app
                .external_tools()
                .iter()
                .filter(|tool| tool.uses_argument_placeholder("rom"))
                .map(|tool| (tool.id.clone(), tool.name.clone()))
                .collect::<Vec<_>>();
            if !emulator_tools.is_empty() {
                ui.separator();
                ui.menu_button("Test ROM in Emulator", |ui| {
                    let enabled = matches!(self.app.mode, lm_app::EditorMode::Level(_))
                        && self.app.project().is_some();
                    for (id, name) in &emulator_tools {
                        if ui.add_enabled(enabled, egui::Button::new(name)).clicked() {
                            ui.close_menu();
                            self.dispatch(context, Command::TestRomInEmulator(id.clone()));
                        }
                    }
                    if ui
                        .add_enabled(enabled, egui::Button::new("Choose Emulator…"))
                        .clicked()
                    {
                        ui.close_menu();
                        self.begin_direct_emulator_test();
                    }
                });
            } else {
                ui.separator();
                let enabled =
                    matches!(self.app.mode, EditorMode::Level(_)) && self.app.project().is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new("Test ROM in Emulator…"))
                    .clicked()
                {
                    ui.close_menu();
                    self.begin_direct_emulator_test();
                }
            }
            if !tools.is_empty() {
                ui.separator();
            }
            for (id, name) in tools {
                if ui.button(name).clicked() {
                    ui.close_menu();
                    self.dispatch(context, Command::RunExternalTool(id));
                }
            }
        });
    }

    fn begin_direct_emulator_test(&mut self) {
        let Some(executable) = crate::dialogs::choose_emulator() else {
            return;
        };
        let level = match self.app.mode {
            EditorMode::Level(level) => level,
            _ => return,
        };
        let snapshot = match self.app.controller_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.effects.error = Some(error.to_string());
                return;
            }
        };
        let name = executable.file_name().map_or_else(
            || "Emulator".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        let request = EmulatorTestRequest {
            tool: ExternalTool {
                id: "chosen-emulator".into(),
                name,
                executable,
                arguments: vec!["{rom}".into()],
                working_directory: None,
                subscriptions: Vec::new(),
            },
            revision: snapshot.revision,
            level,
            rom_bytes: snapshot.rom_bytes,
        };
        if let Err(error) = self.effects.external_tools.enqueue_emulator_test(request) {
            self.effects.error = Some(error);
        }
    }
}
