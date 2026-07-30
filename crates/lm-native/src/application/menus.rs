use super::NativeApplication;
use eframe::egui;
use lm_app::{Command, EditorMode, ProjectStatus};

impl NativeApplication {
    pub(super) fn menu_bar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        let capabilities = self.app.capabilities();
        egui::menu::bar(ui, |ui| {
            self.file_menu(context, ui, capabilities.project);
            self.edit_menu(context, ui, capabilities.history);
            self.editors_menu(context, ui, capabilities.project);
            self.profile_menu(context, ui, capabilities.project);
            self.tools_menu(context, ui);
            self.documents_menu(ui);
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
}
