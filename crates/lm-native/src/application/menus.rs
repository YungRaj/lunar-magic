use super::NativeApplication;
use eframe::egui;
use lm_app::{Command, EditorMode, EmulatorTestRequest, ExternalTool, ProjectStatus, UiTextKey};

impl NativeApplication {
    const ORIGINAL_GENERAL_OPTIONS_DIALOG_ID: u16 = 0x041f;

    pub(super) fn menu_text(&self, key: UiTextKey) -> String {
        self.localized(key, key.english())
    }

    fn original_general_option_text(&self, control_id: u32, fallback: &str) -> String {
        self.app
            .localization()
            .and_then(|catalog| {
                catalog.original_dialog_control_text(
                    Self::ORIGINAL_GENERAL_OPTIONS_DIALOG_ID,
                    control_id,
                )
            })
            .unwrap_or(fallback)
            .to_owned()
    }

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
        ui.menu_button(self.menu_text(UiTextKey::MenuHelp), |ui| {
            if ui.button(self.menu_text(UiTextKey::HelpTopics)).clicked() {
                ui.close_menu();
                self.help_dialog.open();
            }
            if ui
                .button(self.menu_text(UiTextKey::HelpCompatibilityDiagnostics))
                .clicked()
            {
                ui.close_menu();
                self.diagnostics_dialog.open(&self.app);
            }
            if ui.button(self.menu_text(UiTextKey::HelpAbout)).clicked() {
                ui.close_menu();
                self.about_dialog.open();
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, status: ProjectStatus) {
        let layer1 = self.menu_text(UiTextKey::ViewLayer1);
        let layer2 = self.menu_text(UiTextKey::ViewLayer2);
        let layer3 = self.menu_text(UiTextKey::ViewLayer3);
        let sprites = self.menu_text(UiTextKey::ViewLayerSprites);
        let special_world = self.menu_text(UiTextKey::ViewSpecialWorldPassed);
        ui.menu_button(self.menu_text(UiTextKey::MenuView), |ui| {
            let enabled =
                !matches!(status, ProjectStatus::Closed) && self.app.current_level().is_some();
            let mut visibility_changed = false;
            ui.add_enabled_ui(enabled, |ui| {
                visibility_changed |= ui
                    .checkbox(&mut self.level_view_visibility.layer1, layer1.as_str())
                    .changed();
                visibility_changed |= ui
                    .checkbox(&mut self.level_view_visibility.layer2, layer2.as_str())
                    .changed();
                visibility_changed |= ui
                    .checkbox(&mut self.level_view_visibility.layer3, layer3.as_str())
                    .changed();
                visibility_changed |= ui
                    .checkbox(&mut self.level_view_visibility.sprites, sprites.as_str())
                    .changed();
            });
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(special_world.as_str()).selected(self.special_world_passed),
                )
                .clicked()
            {
                self.special_world_passed = !self.special_world_passed;
                self.vanilla_level_editor.invalidate_graphics_preview();
                self.rom_level_assets_editor.invalidate_graphics_preview();
            }
            if visibility_changed {
                self.vanilla_level_editor.invalidate_graphics_preview();
                self.rom_level_assets_editor.invalidate_graphics_preview();
            }
        });
    }

    fn file_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button(self.menu_text(UiTextKey::MenuFile), |ui| {
            if ui.button(self.menu_text(UiTextKey::FileOpen)).clicked() {
                ui.close_menu();
                self.dispatch(context, Command::Open);
            }
            let recent = self.app.recent_documents().paths().to_vec();
            ui.add_enabled_ui(!recent.is_empty(), |ui| {
                ui.menu_button(self.menu_text(UiTextKey::FileOpenRecent), |ui| {
                    for path in recent {
                        if ui.button(path.display().to_string()).clicked() {
                            ui.close_menu();
                            self.open_recent(context, path);
                        }
                    }
                });
            });
            let enabled = !matches!(status, ProjectStatus::Closed);
            if ui
                .add_enabled(
                    enabled && !self.rom_mwl_import_dialog.is_open(),
                    egui::Button::new(self.menu_text(UiTextKey::FileOpenLevelFile)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.rom_mwl_import_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(self.menu_text(UiTextKey::FileOpenLevelNumber)),
                )
                .clicked()
            {
                ui.close_menu();
                self.open_level_number_dialog.open(self.app.current_level());
            }
            if ui
                .add_enabled(
                    enabled && crate::vanilla_level_editor::VanillaLevelEditor::handles(&self.app),
                    egui::Button::new(self.menu_text(UiTextKey::FileOpenLevelAddress)),
                )
                .clicked()
            {
                ui.close_menu();
                self.open_level_address_dialog.open();
            }
            if ui
                .add_enabled(
                    self.app.current_level_deletion_available()
                        && !self.level_deletion_dialog.is_open(),
                    egui::Button::new(self.menu_text(UiTextKey::FileDeleteLevel)),
                )
                .clicked()
            {
                ui.close_menu();
                self.level_deletion_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.multiple_level_deletion_dialog.is_open(),
                    egui::Button::new(self.menu_text(UiTextKey::FileDeleteMultipleLevels)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.multiple_level_deletion_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    enabled && !self.multiple_level_deletion_dialog.is_open(),
                    egui::Button::new(self.menu_text(UiTextKey::FileClearOriginalLevelArea)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self
                    .multiple_level_deletion_dialog
                    .open_clear_original_level_area(&self.app)
                {
                    self.effects.error = Some(error);
                }
            }
            for (label, command) in [
                (self.menu_text(UiTextKey::FileSave), Command::Save),
                (self.menu_text(UiTextKey::FileSaveAs), Command::SaveAs),
                (self.menu_text(UiTextKey::FileClose), Command::Close),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    ui.close_menu();
                    self.dispatch(context, command);
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(self.menu_text(UiTextKey::FileExpandRom)),
                )
                .clicked()
            {
                ui.close_menu();
                self.rom_expansion_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(self.menu_text(UiTextKey::FileConvertCopierHeader)),
                )
                .clicked()
            {
                ui.close_menu();
                self.copier_header_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.level_usage_dialog.is_busy(),
                    egui::Button::new(self.menu_text(UiTextKey::FileAnalyzeLevelUsage)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.level_usage_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(self.menu_text(UiTextKey::FileScanRom)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.rom_user_area_scan_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            for (key, action) in [
                (
                    UiTextKey::FileExtractOldBypassList,
                    crate::legacy_graphics_bypass_transfer::LegacyGraphicsBypassTransferAction::Extract,
                ),
                (
                    UiTextKey::FileInsertOldBypassList,
                    crate::legacy_graphics_bypass_transfer::LegacyGraphicsBypassTransferAction::Insert,
                ),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(self.menu_text(key)))
                    .clicked()
                {
                    ui.close_menu();
                    self.legacy_graphics_bypass_transfer
                        .start(&self.app, action);
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(self.menu_text(UiTextKey::FileRestrictLevelAccess)),
                )
                .clicked()
            {
                ui.close_menu();
                self.level_access_restriction_dialog.open();
            }
            self.restore_point_menu_items(ui, status);
            if ui
                .add_enabled(
                    enabled && self.app.revision_profile().is_some(),
                    egui::Button::new(self.menu_text(UiTextKey::FileMigrateGraphicsCompression)),
                )
                .clicked()
            {
                ui.close_menu();
                self.graphics_migration_dialog.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.built_in_runtime_installer.is_open(),
                    egui::Button::new(self.menu_text(UiTextKey::FileInstallBuiltInRuntime)),
                )
                .clicked()
            {
                ui.close_menu();
                self.built_in_runtime_installer.open(&self.app);
            }
            if ui
                .add_enabled(
                    enabled && !self.rats_reclamation_dialog.is_busy(),
                    egui::Button::new(self.menu_text(UiTextKey::FileReclaimOwnedRatsBlocks)),
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
            if ui.button(self.menu_text(UiTextKey::FileQuit)).clicked() {
                ui.close_menu();
                self.request_quit(context);
            }
        });
    }

    fn restore_point_menu_item(&mut self, ui: &mut egui::Ui, status: ProjectStatus) {
        if ui
            .add_enabled(
                matches!(status, ProjectStatus::Closed) && !self.restore_point_dialog.is_busy(),
                egui::Button::new(self.menu_text(UiTextKey::FileRestoreRom)),
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
            (self.menu_text(UiTextKey::FileCreateFullRestore), None),
            (
                self.menu_text(UiTextKey::FileAppendDeltaRestore),
                Some(RestoreAppendMode::Delta),
            ),
            (
                self.menu_text(UiTextKey::FileAppendFullRestore),
                Some(RestoreAppendMode::Full),
            ),
            (
                self.menu_text(UiTextKey::FileAppendAutomaticRestore),
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
                egui::Button::new(self.menu_text(UiTextKey::FileApplyIpsPatch)),
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
                egui::Button::new(self.menu_text(UiTextKey::FileCreateIpsPatch)),
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
        ui.menu_button(self.menu_text(UiTextKey::MenuEdit), |ui| {
            for (label, enabled, command) in [
                (
                    self.menu_text(UiTextKey::EditUndo),
                    history.undo,
                    Command::Undo,
                ),
                (
                    self.menu_text(UiTextKey::EditRedo),
                    history.redo,
                    Command::Redo,
                ),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    ui.close_menu();
                    self.dispatch(context, command);
                }
            }
        });
    }

    fn editors_menu(&mut self, context: &egui::Context, ui: &mut egui::Ui, status: ProjectStatus) {
        ui.menu_button(self.menu_text(UiTextKey::MenuEditors), |ui| {
            let enabled = !matches!(status, ProjectStatus::Closed);
            let level = match self.app.mode {
                EditorMode::Level(level) | EditorMode::Layer3(level) => level,
                _ => u16::from_str_radix(self.level_text.trim(), 16).unwrap_or(0),
            };
            for (label, command) in [
                (
                    self.menu_text(UiTextKey::ViewLevel),
                    Command::SelectLevel(level),
                ),
                (
                    self.menu_text(UiTextKey::ViewOverworld),
                    Command::ShowOverworld,
                ),
                (self.menu_text(UiTextKey::ViewMap16), Command::ShowMap16),
                (
                    self.menu_text(UiTextKey::ViewGraphics),
                    Command::ShowGraphics(0),
                ),
                (
                    self.menu_text(UiTextKey::ViewPalette),
                    Command::ShowPalette(0),
                ),
                (
                    self.menu_text(UiTextKey::ViewExAnimation),
                    Command::ShowExAnimation(0),
                ),
                (
                    self.menu_text(UiTextKey::ViewLayer3),
                    Command::ShowLayer3(level),
                ),
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
        ui.menu_button(self.menu_text(UiTextKey::MenuProfile), |ui| {
            let enabled = !matches!(status, ProjectStatus::Closed);
            if ui
                .add_enabled(
                    enabled && !self.profile_loader.is_running(),
                    egui::Button::new(self.menu_text(UiTextKey::ProfileInstallRevision)),
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
                    egui::Button::new(self.menu_text(UiTextKey::ProfileClear)),
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
                    egui::Button::new(self.menu_text(UiTextKey::ProfileInstallPatch)),
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

    pub(super) fn open_recent(&mut self, context: &egui::Context, path: std::path::PathBuf) {
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
        ui.menu_button(self.menu_text(UiTextKey::MenuTools), |ui| {
            if ui
                .button(self.menu_text(UiTextKey::ToolsKeyboardShortcuts))
                .clicked()
            {
                ui.close_menu();
                self.shortcut_editor.open(self.app.shortcuts());
            }
            if ui
                .button(self.menu_text(UiTextKey::ToolsCustomizeToolbar))
                .clicked()
            {
                ui.close_menu();
                self.toolbar_editor.open(self.app.toolbar());
            }
            if ui
                .button(self.menu_text(UiTextKey::ToolsUndoHistory))
                .clicked()
            {
                ui.close_menu();
                self.undo_history_settings
                    .open(self.app.undo_snapshot_limit());
            }
            ui.menu_button(self.menu_text(UiTextKey::ToolsAnimationRate), |ui| {
                for rate in crate::animation_rate::AnimationRate::ALL {
                    if ui
                        .selectable_label(self.animation_rate == rate, rate.label())
                        .clicked()
                    {
                        self.animation_rate = rate;
                        self.renderer.invalidate();
                        self.vanilla_level_editor.invalidate_graphics_preview();
                        ui.close_menu();
                    }
                }
            });
            let mut auto_deselect = self.auto_deselect_on_editor_select;
            if ui
                .checkbox(&mut auto_deselect, "Auto-Deselect on Editor Select")
                .changed()
            {
                self.set_auto_deselect_on_editor_select(auto_deselect);
            }
            let mut show_ids = self.show_add_editor_ids.unwrap_or(true);
            if ui
                .checkbox(&mut show_ids, "Show ID in Add Object/Sprite Editors")
                .changed()
            {
                self.set_show_add_editor_ids(show_ids);
            }
            let mut remember_window_size = self.remember_window_size.unwrap_or(true);
            if ui
                .checkbox(&mut remember_window_size, "Remember Window Size")
                .changed()
            {
                self.set_remember_window_size(remember_window_size);
            }
            let mut scan_exits = self.scan_exits_on_save.unwrap_or(true);
            let scan_exits_label =
                self.original_general_option_text(0x22a9, "Scan Exits on Save to ROM");
            if ui.checkbox(&mut scan_exits, scan_exits_label).changed() {
                self.set_scan_exits_on_save(scan_exits);
            }
            let mut count_sprites = self.count_sprites_on_save.unwrap_or(true);
            let count_sprites_label =
                self.original_general_option_text(0x22aa, "Count Sprites on Save to ROM");
            if ui
                .checkbox(&mut count_sprites, count_sprites_label)
                .changed()
            {
                self.set_count_sprites_on_save(count_sprites);
            }
            let mut check_object_placement = self.check_object_placement_on_save.unwrap_or(true);
            let check_object_placement_label =
                self.original_general_option_text(0x22ab, "Check Object Placement on Save to ROM");
            if ui
                .checkbox(&mut check_object_placement, check_object_placement_label)
                .changed()
            {
                self.set_check_object_placement_on_save(check_object_placement);
            }
            let mut correct_fatal_errors = self.correct_fatal_errors.unwrap_or(true);
            if ui
                .checkbox(
                    &mut correct_fatal_errors,
                    "Correct Fatal Errors in Level Data",
                )
                .changed()
            {
                self.set_correct_fatal_errors(correct_fatal_errors);
            }
            let mut warn_vertical_fireball = self.warn_vertical_fireball_buoyancy.unwrap_or(true);
            let warn_vertical_fireball_label = self
                .original_general_option_text(0x22ad, "Check if Vertical Fireball has Buoyancy");
            if ui
                .checkbox(&mut warn_vertical_fireball, warn_vertical_fireball_label)
                .changed()
            {
                self.set_warn_vertical_fireball_buoyancy(warn_vertical_fireball);
            }
            let mut warn_ips_sibling = self.warn_ips_sibling_on_save.unwrap_or(true);
            if ui
                .checkbox(&mut warn_ips_sibling, "Check if ROMFileName.ips Exists")
                .changed()
            {
                self.set_warn_ips_sibling_on_save(warn_ips_sibling);
            }
            let mut convert_berry = self.convert_berry_gfx_tile.unwrap_or(true);
            if ui
                .checkbox(&mut convert_berry, "Convert Berry GFX Tile")
                .changed()
            {
                self.set_convert_berry_gfx_tile(convert_berry);
            }
            let locale = self.app.localization().map_or_else(
                || self.menu_text(UiTextKey::ToolsBuiltInEnglish),
                |catalog| catalog.locale().into(),
            );
            let language = self
                .menu_text(UiTextKey::ToolsLanguageFormat)
                .replace("{locale}", &locale);
            let installed = self.installed_localizations.clone();
            let installed_original = self.installed_original_localizations.clone();
            let active_locale = self
                .app
                .localization()
                .map(|catalog| catalog.locale().to_owned());
            ui.menu_button(language, |ui| {
                for catalog in installed {
                    if ui
                        .add_enabled(
                            !self.configuration_loader.is_running(),
                            egui::Button::new(&catalog.locale).selected(
                                active_locale.as_deref() == Some(catalog.locale.as_str()),
                            ),
                        )
                        .clicked()
                    {
                        ui.close_menu();
                        if let Err(error) = self
                            .configuration_loader
                            .start_localization_path(catalog.path)
                        {
                            self.effects.error = Some(error);
                        } else {
                            self.auto_detect_localization = false;
                        }
                    }
                }
                for module in installed_original {
                    if ui
                        .add_enabled(
                            !self.configuration_loader.is_running(),
                            egui::Button::new(&module.metadata.display_name).selected(
                                active_locale.as_deref() == Some(module.metadata.locale.as_str()),
                            ),
                        )
                        .clicked()
                    {
                        ui.close_menu();
                        let locale = module.metadata.locale.clone();
                        match self.app.set_localization(module.catalog) {
                            Ok(()) => {
                                self.auto_detect_localization = false;
                                self.app.status =
                                    format!("Installed {locale} original language module");
                            }
                            Err(error) => self.effects.error = Some(error.to_string()),
                        }
                    }
                }
                if !self.installed_localizations.is_empty()
                    || !self.installed_original_localizations.is_empty()
                {
                    ui.separator();
                }
                if ui
                    .add_enabled(
                        !self.configuration_loader.is_running(),
                        egui::Button::new(self.menu_text(UiTextKey::ToolsInstallLanguage)),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    match self.configuration_loader.choose_localization_and_start() {
                        Ok(true) => self.auto_detect_localization = false,
                        Ok(false) => {}
                        Err(error) => self.effects.error = Some(error),
                    }
                }
                if ui
                    .add_enabled(
                        !self.configuration_loader.is_running(),
                        egui::Button::new(self.menu_text(UiTextKey::ToolsAutoDetectLanguage))
                            .selected(self.auto_detect_localization),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.auto_detect_localization = true;
                    if let Err(error) = self.start_auto_detected_localization() {
                        self.effects.error = Some(error);
                    } else {
                        self.app.status = "Enabled automatic system-language selection".into();
                    }
                }
                if ui
                    .add_enabled(
                        self.app.localization().is_some(),
                        egui::Button::new(self.menu_text(UiTextKey::ToolsUseBuiltInEnglish)),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.auto_detect_localization = false;
                    self.app.clear_localization();
                    self.app.status = "Restored built-in English".into();
                }
            });
            if ui
                .add_enabled(
                    !self.configuration_loader.is_running(),
                    egui::Button::new(self.menu_text(UiTextKey::ToolsInstallFrontendConfiguration)),
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
                    egui::Button::new(self.menu_text(UiTextKey::ToolsInstallToolConfiguration)),
                )
                .clicked()
            {
                ui.close_menu();
                if let Err(error) = self.configuration_loader.choose_external_tools_and_start() {
                    self.effects.error = Some(error);
                }
            }
            if ui
                .add_enabled(
                    !self.external_tool_config_editor.is_open(),
                    egui::Button::new(crate::external_tool_config_editor::menu_text(
                        self.app.localization(),
                    )),
                )
                .clicked()
            {
                ui.close_menu();
                self.external_tool_config_editor
                    .open(self.app.external_tools());
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
                ui.menu_button(self.menu_text(UiTextKey::ToolsTestRomInEmulator), |ui| {
                    let enabled = matches!(self.app.mode, lm_app::EditorMode::Level(_))
                        && self.app.project().is_some();
                    for (id, name) in &emulator_tools {
                        if ui.add_enabled(enabled, egui::Button::new(name)).clicked() {
                            ui.close_menu();
                            self.dispatch(context, Command::TestRomInEmulator(id.clone()));
                        }
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(self.menu_text(UiTextKey::ToolsChooseEmulator)),
                        )
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
                    .add_enabled(
                        enabled,
                        egui::Button::new(self.menu_text(UiTextKey::ToolsTestRomInEmulatorAction)),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.begin_direct_emulator_test();
                }
            }
            if !tools.is_empty() {
                ui.separator();
            }
            let live_enabled =
                matches!(self.app.mode, EditorMode::Level(_)) && self.app.project().is_some();
            if ui
                .add_enabled(
                    live_enabled,
                    egui::Button::new(self.menu_text(UiTextKey::ToolsLiveEmulator)),
                )
                .clicked()
            {
                ui.close_menu();
                self.begin_live_emulator_test();
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

    pub(super) fn begin_direct_emulator_test(&mut self) {
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
                replace_tile_editor_palette: false,
            },
            revision: snapshot.revision,
            level,
            rom_bytes: snapshot.rom_bytes,
        };
        if let Err(error) = self.effects.external_tools.enqueue_emulator_test(request) {
            self.effects.error = Some(error);
        }
    }

    pub(super) fn begin_live_emulator_test(&mut self) {
        let Some(core) = crate::live_emulator::choose_core() else {
            return;
        };
        let EditorMode::Level(level) = self.app.mode else {
            return;
        };
        let snapshot = match self.app.controller_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.effects.error = Some(error.to_string());
                return;
            }
        };
        if let Err(error) =
            self.live_emulator
                .start(core, snapshot.revision, level, snapshot.rom_bytes)
        {
            self.effects.error = Some(error);
        }
    }
}
