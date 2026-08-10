use super::{LevelScreenOverlay, LevelViewVisibility, NativeApplication};
use crate::frontend_ui;
use crate::rom_expansion_dialog::RomExpansionPreset;
use crate::{
    graphics_insertion_dialog::GraphicsInsertionFamily,
    rom_graphics_editor::QuickGraphicsInsertion,
    toolbar_graphics_transfer::QuickGraphicsExtraction,
};
use eframe::egui;
use lm_app::{
    Command, LevelNavigationDirection, LunarMagicNotification, LunarMagicNotificationKind,
    ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolInvocation, ToolbarActivation,
    UserToolbarButton, UserToolbarTarget, user_toolbar_internal_command,
};

impl NativeApplication {
    pub(super) fn handle_user_toolbar_document_change(&mut self, context: &egui::Context) {
        let current = self.app.document_path.clone();
        if current == self.user_toolbar_observed_document {
            return;
        }
        self.user_toolbar_observed_document = current.clone();
        self.user_toolbar_observed_level = None;
        if current.is_none() {
            return;
        }
        let Some(toolbar) = self.user_toolbar.as_ref() else {
            return;
        };
        let close = toolbar_lifecycle_indexes(
            toolbar,
            "LM_CLOSE_ON_NEW_ROM",
            "LM_CLOSE_ON_NEW_ROM_FORCE_ALL",
        );
        let autorun_disabled = toolbar.global_options.iter().any(|option| {
            matches!(option, lm_app::UserToolbarGlobalOption::Flag(value) if value == "LM_NO_AUTORUN")
        });
        let autorun = if autorun_disabled {
            Vec::new()
        } else {
            toolbar_button_indexes_with_option(toolbar, "LM_AUTORUN_ON_NEW_ROM")
        };
        for index in close {
            self.effects
                .external_tools
                .stop_tool(&format!("usertoolbar-{index}"));
        }
        if let (Some(path), Some(identity)) = (
            current.as_deref(),
            self.app
                .project()
                .and_then(|project| project.identity.as_ref()),
        ) {
            self.notify_user_toolbar_tools(
                LunarMagicNotification::new(
                    LunarMagicNotificationKind::NewRom,
                    lunar_magic_rom_identity_code(identity),
                )
                .expect("supported ROM identity code is bounded"),
                Some(path),
                "LM_NOTIFY_ON_NEW_ROM",
                Some("LM_NOTIFY_ON_NEW_ROM_FORCE_ALL"),
            );
        }
        let buttons = autorun
            .into_iter()
            .filter_map(|index| {
                self.user_toolbar
                    .as_ref()?
                    .buttons
                    .get(index)
                    .cloned()
                    .map(|button| (index, button))
            })
            .collect::<Vec<_>>();
        for (index, button) in buttons {
            self.activate_user_toolbar_button(context, index, &button);
        }
    }

    pub(super) fn handle_user_toolbar_level_change(&mut self) {
        let Some(level) = self.app.current_level() else {
            return;
        };
        if self.user_toolbar_observed_level == Some(level) {
            return;
        }
        self.user_toolbar_observed_level = Some(level);
        self.notify_user_toolbar_tools(
            LunarMagicNotification::new(LunarMagicNotificationKind::NewLevel, level)
                .expect("native level numbers fit the ten-bit notification variable"),
            None,
            "LM_NOTIFY_ON_NEW_LEVEL",
            Some("LM_NOTIFY_ON_NEW_LEVEL_FORCE_ALL"),
        );
    }

    pub(super) fn stop_user_toolbar_tools_on_close(&mut self) {
        self.notify_user_toolbar_tools(
            LunarMagicNotification::new(LunarMagicNotificationKind::Close, 0)
                .expect("zero is a bounded notification variable"),
            None,
            "LM_NOTIFY_ON_CLOSE",
            Some("LM_NOTIFY_ON_CLOSE_FORCE_ALL"),
        );
        let Some(toolbar) = self.user_toolbar.as_ref() else {
            return;
        };
        for index in
            toolbar_lifecycle_indexes(toolbar, "LM_CLOSE_ON_CLOSE", "LM_CLOSE_ON_CLOSE_FORCE_ALL")
        {
            self.effects
                .external_tools
                .stop_tool(&format!("usertoolbar-{index}"));
        }
    }

    fn notify_user_toolbar_tools(
        &mut self,
        notification: LunarMagicNotification,
        rom_path: Option<&std::path::Path>,
        option: &str,
        force_option: Option<&str>,
    ) {
        let tool_ids = self.user_toolbar.as_ref().map_or_else(Vec::new, |toolbar| {
            toolbar_notification_tool_ids(toolbar, option, force_option)
        });
        for tool_id in tool_ids {
            if let Err(error) =
                self.effects
                    .external_tools
                    .notify_tool(&tool_id, notification, rom_path)
            {
                self.effects.error = Some(error);
            }
        }
    }

    pub(super) fn mark_user_toolbar_save_notification(&mut self, kind: LunarMagicNotificationKind) {
        let bit = match kind {
            LunarMagicNotificationKind::SaveLevel => 1,
            LunarMagicNotificationKind::SaveMap16 => 2,
            LunarMagicNotificationKind::SaveOverworld => 4,
            _ => return,
        };
        self.user_toolbar_pending_save_notifications |= bit;
    }

    pub(super) fn publish_user_toolbar_save_notifications(&mut self) {
        let pending = std::mem::take(&mut self.user_toolbar_pending_save_notifications);
        let level = self.user_toolbar_observed_level.unwrap_or(0);
        for (bit, kind, option, variable) in [
            (
                1,
                LunarMagicNotificationKind::SaveLevel,
                "LM_NOTIFY_ON_SAVE_LEVEL",
                level,
            ),
            (
                2,
                LunarMagicNotificationKind::SaveMap16,
                "LM_NOTIFY_ON_SAVE_MAP16",
                level,
            ),
            (
                4,
                LunarMagicNotificationKind::SaveOverworld,
                "LM_NOTIFY_ON_SAVE_OV",
                0,
            ),
        ] {
            if pending & bit != 0 {
                self.notify_user_toolbar_tools(
                    LunarMagicNotification::new(kind, variable)
                        .expect("native save notification variables are bounded"),
                    None,
                    option,
                    None,
                );
            }
        }
    }

    pub(super) fn mark_user_toolbar_level_deleted(&mut self, level: u16) {
        if !self.user_toolbar_pending_deleted_levels.contains(&level) {
            self.user_toolbar_pending_deleted_levels.push(level);
        }
    }

    pub(super) fn publish_user_toolbar_level_deleted_notifications(&mut self) {
        for level in std::mem::take(&mut self.user_toolbar_pending_deleted_levels) {
            self.notify_user_toolbar_tools(
                LunarMagicNotification::new(LunarMagicNotificationKind::DeleteLevel, level)
                    .expect("native level numbers fit the ten-bit notification variable"),
                None,
                "LM_NOTIFY_ON_DELETE_LEVEL",
                None,
            );
        }
    }

    pub(super) fn toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        if self.app.toolbar().is_some() {
            if let Some(activation) = frontend_ui::show_toolbar(ui, &self.app) {
                self.handle_frontend_activation(context, activation);
            }
        } else {
            self.default_toolbar(context, ui);
        }
        if self
            .user_toolbar
            .as_ref()
            .is_some_and(|toolbar| toolbar.toolbar_visible())
        {
            ui.separator();
            self.user_toolbar(context, ui);
        }
    }

    fn user_toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        // Clone the compact descriptors so dispatch can mutably borrow the application.
        let buttons = self
            .user_toolbar
            .as_ref()
            .map(|toolbar| toolbar.buttons.clone())
            .unwrap_or_default();
        self.user_toolbar_images.ensure_textures(context);
        let icon_size = self.user_toolbar_images.icon_size().unwrap_or(16.0);
        let icons = buttons
            .iter()
            .enumerate()
            .map(|(index, _)| {
                self.user_toolbar
                    .as_ref()
                    .and_then(|toolbar| self.user_toolbar_images.texture_for(toolbar, index))
                    .cloned()
            })
            .collect::<Vec<_>>();
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, (button, icon)) in buttons.iter().zip(&icons).enumerate() {
                if button.options.iter().any(|option| option == "LM_NO_BUTTON") {
                    continue;
                }
                match &button.target {
                    UserToolbarTarget::Spacer => {
                        ui.separator();
                    }
                    target => {
                        let label = if button.tooltip.is_empty() {
                            user_toolbar_label(target)
                        } else {
                            button.tooltip.lines().next().unwrap_or("Tool")
                        };
                        let widget = icon.as_ref().map_or_else(
                            || egui::Button::new(label),
                            |texture| {
                                let image = egui::Image::new((
                                    texture.id(),
                                    egui::vec2(icon_size, icon_size),
                                ));
                                if button.tooltip.is_empty() {
                                    egui::Button::image(image)
                                } else {
                                    egui::Button::image_and_text(image, label)
                                }
                            },
                        );
                        if ui.add(widget).on_hover_text(&button.tooltip).clicked() {
                            clicked = Some(index);
                        }
                    }
                }
            }
        });
        if let Some(index) = clicked {
            self.activate_user_toolbar_button(context, index, &buttons[index]);
        }
    }

    fn activate_user_toolbar_button(
        &mut self,
        context: &egui::Context,
        index: usize,
        button: &UserToolbarButton,
    ) {
        match &button.target {
            UserToolbarTarget::Spacer => {}
            UserToolbarTarget::Internal(name) => {
                let Some(original_command) = user_toolbar_internal_command(name) else {
                    self.effects.error = Some(format!(
                        "User toolbar command {name:?} is not part of Lunar Magic 3.63's internal command table"
                    ));
                    return;
                };
                if let Some(action) = user_toolbar_native_action(name) {
                    self.apply_user_toolbar_native_action(context, action);
                    return;
                }
                if let Some(action) = user_toolbar_local_action(name) {
                    self.apply_user_toolbar_local_action(action);
                    return;
                }
                match user_toolbar_command(name, self.app.current_level()) {
                    Some(command) => self.dispatch(context, command),
                    None => {
                        self.effects.error = Some(format!(
                            "Lunar Magic 3.63 user toolbar command {name:?} (ID ${:04X}) is recognized but not supported by this editor yet",
                            original_command.command_id
                        ))
                    }
                }
            }
            UserToolbarTarget::External(command_line) => match split_command_line(command_line) {
                Ok((executable, arguments)) => {
                    let expanded = arguments
                        .iter()
                        .map(|value| expand_lm_placeholders(value, &self.app))
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|arguments| {
                            Ok(ToolInvocation {
                                tool_id: format!("usertoolbar-{index}"),
                                executable: expand_lm_placeholders(&executable, &self.app)?.into(),
                                arguments,
                                working_directory: external_working_directory(
                                    &executable,
                                    button,
                                    &self.app,
                                )?,
                            })
                        });
                    match expanded {
                        Ok(invocation) => {
                            let options =
                                user_toolbar_launch_options(self.user_toolbar.as_ref(), button);
                            if let Err(error) = self
                                .effects
                                .external_tools
                                .enqueue_with_options(invocation, options)
                            {
                                self.effects.error = Some(error);
                            }
                        }
                        Err(error) => self.effects.error = Some(error.to_string()),
                    }
                }
                Err(error) => self.effects.error = Some(error),
            },
        }
    }

    fn apply_user_toolbar_native_action(
        &mut self,
        context: &egui::Context,
        action: UserToolbarNativeAction,
    ) {
        match action {
            UserToolbarNativeAction::HelpContents => self.help_dialog.open(),
            UserToolbarNativeAction::HelpAbout => self.about_dialog.open(),
            UserToolbarNativeAction::Sprite19Fix => {
                self.built_in_runtime_installer.open_sprite19_fix(&self.app);
            }
            UserToolbarNativeAction::AnalyzeLevels => {
                if let Err(error) = self.level_usage_dialog.open(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::RestoreRom => {
                if let Err(error) = self.restore_point_dialog.choose_and_open() {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::CreateRestorePoint => {
                if let Err(error) =
                    crate::restore_point_dialog::create_full_for_open_project(&self.app)
                {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::CreateIps => {
                if let Err(error) = self.ips_create_dialog.choose_and_start() {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::ApplyIps => {
                if let Err(error) = self.ips_patch_dialog.choose_and_start(&self.app) {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::GraphicsCompressionOptions => {
                self.graphics_migration_dialog.open(&self.app);
            }
            UserToolbarNativeAction::GeneralOptions => {
                self.undo_history_settings
                    .open(self.app.undo_snapshot_limit());
            }
            UserToolbarNativeAction::RestoreOptions => {
                self.restore_point_dialog.open_automatic_policy();
            }
            UserToolbarNativeAction::EmulatorSettings => {
                self.external_tool_config_editor
                    .open(self.app.external_tools());
            }
            UserToolbarNativeAction::ExternalEmulatorRun => {
                let configured = configured_snes_emulator_tool_id(self.app.external_tools());
                if let Some(id) = configured {
                    self.dispatch(context, Command::TestRomInEmulator(id));
                } else {
                    self.begin_direct_emulator_test();
                }
            }
            UserToolbarNativeAction::LiveEmulatorRun => self.begin_live_emulator_test(),
            UserToolbarNativeAction::LiveEmulatorStop => self.live_emulator.stop(),
            UserToolbarNativeAction::LiveEmulatorPause => {
                if let Err(error) = self.live_emulator.toggle_manual_pause() {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::LiveEmulatorMute => {
                if let Err(error) = self.live_emulator.toggle_mute() {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::LiveEmulatorFrameAdvance => {
                if let Err(error) = self.live_emulator.step_frame() {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::DeleteLevel => {
                self.level_deletion_dialog.open(&self.app);
            }
            UserToolbarNativeAction::OpenSecondaryEntrances => {
                self.rom_secondary_exit_editor.open(&self.app);
            }
            UserToolbarNativeAction::OpenScreenExitAtPointer => {
                self.vanilla_level_editor
                    .toolbar_open_screen_exit_at_pointer(context);
            }
            UserToolbarNativeAction::FollowScreenExitAtPointer => {
                match self
                    .vanilla_level_editor
                    .toolbar_follow_screen_exit_at_pointer(context)
                {
                    Ok(Some(command)) => self.dispatch(context, command),
                    Ok(None) => {}
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::ScanInvalidExits => match self.app.controller_snapshot() {
                Ok(snapshot) => {
                    if let Err(error) = self
                        .vanilla_level_editor
                        .toolbar_scan_invalid_exits(&snapshot, self.dsc_sidecar_editor.resolved())
                    {
                        self.effects.error = Some(error);
                    }
                }
                Err(error) => self.effects.error = Some(error.to_string()),
            },
            UserToolbarNativeAction::OpenLevelExAnimation => {
                if let Some(level) = self.app.current_level() {
                    self.rom_exanimation_editor.open_level(&self.app, level);
                }
            }
            UserToolbarNativeAction::OpenGlobalExAnimation => {
                if let Some(level) = self.app.current_level() {
                    self.rom_exanimation_editor.open_global(&self.app, level);
                }
            }
            UserToolbarNativeAction::OpenLayer3Bypass => {
                match self.rom_expanded_settings_editor.open_detected(&self.app) {
                    Ok(true) => {}
                    Ok(false) => self
                        .built_in_runtime_installer
                        .open_complete_layer3_for_settings(&self.app),
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::OpenLegacyForegroundBackgroundBypass => {
                let domain = crate::rom_legacy_graphics_bypass_editor::LegacyGraphicsBypassDomain::ForegroundBackground;
                match legacy_graphics_bypass_prerequisite_installed(&self.app) {
                    Ok(true) => {
                        if let Err(error) = self
                            .rom_legacy_fg_bg_bypass_editor
                            .open_domain(&self.app, domain)
                        {
                            self.effects.error = Some(error);
                        }
                    }
                    Ok(false) => self
                        .built_in_runtime_installer
                        .open_expanded_settings_for_legacy_bypass(&self.app, domain),
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::OpenLegacySpriteBypass => {
                let domain =
                    crate::rom_legacy_graphics_bypass_editor::LegacyGraphicsBypassDomain::Sprites;
                match legacy_graphics_bypass_prerequisite_installed(&self.app) {
                    Ok(true) => {
                        if let Err(error) = self
                            .rom_legacy_sprite_bypass_editor
                            .open_domain(&self.app, domain)
                        {
                            self.effects.error = Some(error);
                        }
                    }
                    Ok(false) => self
                        .built_in_runtime_installer
                        .open_expanded_settings_for_legacy_bypass(&self.app, domain),
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::PlaceObject => {
                self.vanilla_level_editor.toolbar_place_object();
            }
            UserToolbarNativeAction::PlaceSprite => {
                self.vanilla_level_editor.toolbar_place_sprite();
            }
            UserToolbarNativeAction::OpenLevelToolPanel(panel) => {
                self.vanilla_level_editor.toolbar_open_tool_panel(panel);
            }
            UserToolbarNativeAction::SelectAll => {
                self.vanilla_level_editor.toolbar_select_all();
            }
            UserToolbarNativeAction::Insert => {
                self.vanilla_level_editor.toolbar_insert_at_pointer(context);
            }
            UserToolbarNativeAction::DeleteSelection => {
                self.vanilla_level_editor.toolbar_delete_selection();
            }
            UserToolbarNativeAction::DeleteAll => {
                self.vanilla_level_editor.toolbar_delete_all();
            }
            UserToolbarNativeAction::Escape => {
                self.vanilla_level_editor.toolbar_escape();
            }
            UserToolbarNativeAction::EditLayer1 => {
                self.vanilla_level_editor.toolbar_edit_layer1();
            }
            UserToolbarNativeAction::EditLayer2 => {
                self.vanilla_level_editor.toolbar_edit_layer2();
            }
            UserToolbarNativeAction::EditSprites => {
                self.vanilla_level_editor.toolbar_edit_sprites();
            }
            UserToolbarNativeAction::Copy => {
                match self.vanilla_level_editor.toolbar_copy_selection() {
                    Ok(text) => context.copy_text(text),
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::Cut => {
                match self.vanilla_level_editor.toolbar_cut_selection() {
                    Ok(text) => context.copy_text(text),
                    Err(error) => self.effects.error = Some(error),
                }
            }
            UserToolbarNativeAction::Paste => {
                self.vanilla_level_editor.toolbar_request_paste(context);
            }
            UserToolbarNativeAction::Nudge { x, y } => {
                self.vanilla_level_editor.toolbar_nudge_selection(x, y);
            }
            UserToolbarNativeAction::ZOrderStep { increase } => {
                self.vanilla_level_editor.toolbar_z_order_step(increase);
            }
            UserToolbarNativeAction::OverlapZOrder(traversal) => {
                self.vanilla_level_editor.toolbar_overlap_z_order(traversal);
            }
            UserToolbarNativeAction::ConditionalDirectMap16 => self
                .vanilla_level_editor
                .toolbar_edit_conditional_direct_map16(),
            UserToolbarNativeAction::RemapDirectMap16 => {
                self.vanilla_level_editor.toolbar_remap_direct_map16();
            }
            UserToolbarNativeAction::TogglePropertiesWindow => {
                self.vanilla_level_editor.toolbar_toggle_properties_window()
            }
            UserToolbarNativeAction::OpenManualEditDialog => {
                self.vanilla_level_editor.toolbar_open_manual_edit_dialog()
            }
            UserToolbarNativeAction::ExpandRom(preset) => {
                self.rom_expansion_dialog.open_preset(&self.app, preset);
            }
            UserToolbarNativeAction::ExportAllLevels => {
                self.rom_mwl_batch_export_dialog
                    .open(&self.app, lm_app::MwlBatchExportMode::All);
            }
            UserToolbarNativeAction::ExportModifiedLevels => {
                self.rom_mwl_batch_export_dialog
                    .open(&self.app, lm_app::MwlBatchExportMode::Modified);
            }
            UserToolbarNativeAction::ExportCurrentLevelBitmap => self
                .rom_level_assets_editor
                .toolbar_export_current_level_bitmap(
                    &self.app,
                    self.special_world_passed,
                    self.level_view_visibility,
                ),
            UserToolbarNativeAction::ExportLevelBitmapDirectory => self
                .rom_level_assets_editor
                .toolbar_export_level_bitmap_directory(
                    &self.app,
                    self.special_world_passed,
                    self.level_view_visibility,
                ),
            UserToolbarNativeAction::SharedPaletteTransfer(action) => self
                .rom_shared_palette_editor
                .open_and_start_transfer(&self.app, action),
            UserToolbarNativeAction::CurrentLevelPaletteTransfer(action) => {
                self.current_level_palette_transfer.start(&self.app, action);
            }
            UserToolbarNativeAction::QuickExtractGraphics(action) => {
                if let Err(error) = self.toolbar_graphics_transfer.start(
                    &self.app,
                    action,
                    self.joined_graphics_files,
                    false,
                ) {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::ExtractGraphics(action) => {
                if let Err(error) = self.toolbar_graphics_transfer.start(
                    &self.app,
                    action,
                    self.joined_graphics_files,
                    true,
                ) {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::QuickInsertGraphics(action) => {
                if let Err(error) = self.rom_graphics_editor.start_quick_import(
                    &self.app,
                    action,
                    self.joined_graphics_files,
                ) {
                    self.effects.error = Some(error);
                }
            }
            UserToolbarNativeAction::OrdinaryInsertGraphics(family) => {
                if let Err(error) = self
                    .rom_graphics_editor
                    .open_ordinary_import(&self.app, family)
                {
                    self.effects.error = Some(error);
                }
            }
        }
    }

    fn apply_user_toolbar_local_action(&mut self, action: UserToolbarLocalAction) {
        if self.app.current_level().is_none() {
            self.effects.error =
                Some("The user-toolbar view command requires an open level".into());
            return;
        }
        match action {
            UserToolbarLocalAction::Zoom => self.vanilla_level_editor.toolbar_zoom_popup(),
            UserToolbarLocalAction::ZoomFilter => {
                self.vanilla_level_editor.toolbar_zoom_filter_toggle();
            }
            UserToolbarLocalAction::Animation => {
                self.vanilla_level_editor.toolbar_animation_toggle();
            }
            UserToolbarLocalAction::IncreaseAnimationFrame => {
                self.vanilla_level_editor.toolbar_animation_step();
            }
            UserToolbarLocalAction::ResetAnimation => {
                self.vanilla_level_editor.toolbar_animation_reset();
            }
            UserToolbarLocalAction::GreenSwitch => {
                self.vanilla_level_editor.toolbar_switch_view_toggle(0);
            }
            UserToolbarLocalAction::YellowSwitch => {
                self.vanilla_level_editor.toolbar_switch_view_toggle(1);
            }
            UserToolbarLocalAction::BlueSwitch => {
                self.vanilla_level_editor.toolbar_switch_view_toggle(2);
            }
            UserToolbarLocalAction::RedSwitch => {
                self.vanilla_level_editor.toolbar_switch_view_toggle(3);
            }
            UserToolbarLocalAction::SilverPow => {
                self.vanilla_level_editor.toolbar_silver_pow_toggle();
            }
            UserToolbarLocalAction::BluePow => {
                self.vanilla_level_editor.toolbar_blue_pow_toggle();
            }
            UserToolbarLocalAction::InvisiblePowObjects => self
                .vanilla_level_editor
                .toolbar_invisible_pow_objects_toggle(),
            UserToolbarLocalAction::OtherInvisibleObjects => self
                .vanilla_level_editor
                .toolbar_other_invisible_objects_toggle(),
            UserToolbarLocalAction::OnOffSwitch => {
                self.vanilla_level_editor.toolbar_on_off_switch_toggle();
            }
            UserToolbarLocalAction::ConditionalDirectMap16 => self
                .vanilla_level_editor
                .toolbar_conditional_direct_map16_toggle(),
            UserToolbarLocalAction::BlockContents => {
                self.vanilla_level_editor.toolbar_block_contents_toggle()
            }
            UserToolbarLocalAction::BlockExits => {
                self.vanilla_level_editor.toolbar_block_exits_toggle()
            }
            UserToolbarLocalAction::HaveStar => {
                self.vanilla_level_editor.toolbar_have_star_toggle()
            }
            UserToolbarLocalAction::Time100 => self.vanilla_level_editor.toolbar_time_100_toggle(),
            UserToolbarLocalAction::FiveYoshiCoins => {
                self.vanilla_level_editor.toolbar_five_yoshi_coins_toggle()
            }
            UserToolbarLocalAction::CustomTrigger(trigger) => self
                .vanilla_level_editor
                .toolbar_custom_trigger_toggle(trigger),
            UserToolbarLocalAction::OneShotTrigger(trigger) => self
                .vanilla_level_editor
                .toolbar_one_shot_trigger_toggle(trigger),
            UserToolbarLocalAction::ManualTrigger { trigger, delta } => self
                .vanilla_level_editor
                .toolbar_manual_trigger_adjust(trigger, delta),
            UserToolbarLocalAction::TriggerSelection { family, delta } => self
                .vanilla_level_editor
                .toolbar_trigger_selection_adjust(family, delta),
            UserToolbarLocalAction::CurrentTrigger { family, delta } => self
                .vanilla_level_editor
                .toolbar_current_trigger_action(family, delta),
            UserToolbarLocalAction::Background512Height => {
                self.vanilla_level_editor
                    .toolbar_background_512_height_toggle();
            }
            UserToolbarLocalAction::Translucent => {
                self.vanilla_level_editor
                    .toolbar_translucent_overlays_toggle();
            }
            UserToolbarLocalAction::EntranceOverlay(toggle) => self
                .vanilla_level_editor
                .toolbar_toggle_entrance_overlay(toggle),
            UserToolbarLocalAction::ZoomToggle => self.vanilla_level_editor.toolbar_zoom_toggle(),
            UserToolbarLocalAction::ZoomDefault => self.vanilla_level_editor.toolbar_zoom_default(),
            UserToolbarLocalAction::ZoomPlus => self
                .vanilla_level_editor
                .toolbar_zoom_adjust(ROM_LEVEL_TOOLBAR_ZOOM_STEP),
            UserToolbarLocalAction::ZoomMinus => self
                .vanilla_level_editor
                .toolbar_zoom_adjust(-ROM_LEVEL_TOOLBAR_ZOOM_STEP),
            _ => toggle_user_toolbar_view_state(
                &mut self.level_view_visibility,
                &mut self.special_world_passed,
                action,
            ),
        }
        self.vanilla_level_editor.invalidate_graphics_preview();
        self.rom_level_assets_editor.invalidate_graphics_preview();
    }

    fn handle_frontend_activation(
        &mut self,
        context: &egui::Context,
        activation: ToolbarActivation,
    ) {
        match activation {
            ToolbarActivation::Command(command) => self.dispatch(context, *command),
            ToolbarActivation::RequestCopyPayload
            | ToolbarActivation::RequestCutPayload
            | ToolbarActivation::RequestClipboardBytes => {
                self.effects.error = Some(
                    "The active native editor has not supplied a typed clipboard payload".into(),
                );
            }
        }
    }

    fn default_toolbar(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        let capabilities = self.app.capabilities();
        self.main_toolbar_images.ensure_textures(context);
        let icon_size = self.main_toolbar_images.icon_size();
        let open_icon = self.main_toolbar_images.texture(1).cloned();
        let save_icon = self.main_toolbar_images.texture(3).cloned();
        let undo_icon = self.main_toolbar_images.texture(5).cloned();
        let redo_icon = self.main_toolbar_images.texture(6).cloned();
        ui.horizontal(|ui| {
            if toolbar_button(ui, "Open", true, open_icon.as_ref(), icon_size).clicked() {
                self.dispatch(context, Command::Open);
            }
            if toolbar_button(
                ui,
                "Save",
                capabilities.can_save(),
                save_icon.as_ref(),
                icon_size,
            )
            .clicked()
            {
                self.dispatch(context, Command::Save);
            }
            ui.separator();
            for (label, enabled, command, icon) in [
                (
                    "Undo",
                    capabilities.history.undo,
                    Command::Undo,
                    undo_icon.as_ref(),
                ),
                (
                    "Redo",
                    capabilities.history.redo,
                    Command::Redo,
                    redo_icon.as_ref(),
                ),
            ] {
                if toolbar_button(ui, label, enabled, icon, icon_size).clicked() {
                    self.dispatch(context, command);
                }
            }
            ui.separator();
            for (label, enabled, direction) in [
                (
                    "Back",
                    capabilities.navigation.level_back,
                    LevelNavigationDirection::Back,
                ),
                (
                    "Forward",
                    capabilities.navigation.level_forward,
                    LevelNavigationDirection::Forward,
                ),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    self.dispatch(context, Command::NavigateLevel(direction));
                }
            }
            ui.label("Level");
            let response = ui.add_sized(
                [55.0, 22.0],
                egui::TextEdit::singleline(&mut self.level_text),
            );
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                match u16::from_str_radix(self.level_text.trim(), 16) {
                    Ok(level) => self.dispatch(context, Command::SelectLevel(level)),
                    Err(error) => {
                        self.effects.error = Some(format!("invalid hexadecimal level: {error}"));
                    }
                }
            }
        });
    }

    pub(super) fn handle_shortcuts(&mut self, context: &egui::Context) {
        let gestures = frontend_ui::shortcut_gestures(context);
        let matching = self.user_toolbar.as_ref().map_or_else(Vec::new, |toolbar| {
            matching_user_toolbar_buttons(toolbar, &gestures)
        });
        if !matching.is_empty() {
            // Lunar Magic lets duplicate user assignments all fire and suppresses its built-in
            // shortcut whenever at least one user-toolbar assignment matches.
            for (index, button) in matching {
                self.activate_user_toolbar_button(context, index, &button);
            }
            return;
        }
        if let Some(activation) = frontend_ui::shortcut_activation(context, &self.app) {
            self.handle_frontend_activation(context, activation);
        }
    }
}

fn toolbar_button_indexes_with_option(toolbar: &lm_app::UserToolbar, option: &str) -> Vec<usize> {
    toolbar
        .buttons
        .iter()
        .enumerate()
        .filter_map(|(index, button)| {
            button
                .options
                .iter()
                .any(|value| value == option)
                .then_some(index)
        })
        .collect()
}

fn toolbar_lifecycle_indexes(
    toolbar: &lm_app::UserToolbar,
    option: &str,
    force_option: &str,
) -> Vec<usize> {
    if toolbar.global_options.iter().any(
        |value| matches!(value, lm_app::UserToolbarGlobalOption::Flag(flag) if flag == force_option),
    ) {
        return (0..toolbar.buttons.len()).collect();
    }
    toolbar_button_indexes_with_option(toolbar, option)
}

fn toolbar_notification_tool_ids(
    toolbar: &lm_app::UserToolbar,
    option: &str,
    force_option: Option<&str>,
) -> Vec<String> {
    let force_all = force_option.is_some_and(|force_option| {
        toolbar.global_options.iter().any(
            |value| matches!(value, lm_app::UserToolbarGlobalOption::Flag(flag) if flag == force_option),
        )
    });
    toolbar
        .buttons
        .iter()
        .enumerate()
        .filter_map(|(index, button)| {
            matches!(button.target, UserToolbarTarget::External(_))
                .then(|| force_all || button.options.iter().any(|candidate| candidate == option))
                .unwrap_or(false)
                .then(|| format!("usertoolbar-{index}"))
        })
        .collect()
}

const fn lunar_magic_rom_identity_code(identity: &lm_rom::RomIdentity) -> u16 {
    match (identity.game, identity.region) {
        (lm_rom::SupportedGame::SuperMarioWorld, lm_rom::Region::NorthAmerica) => 0,
        (lm_rom::SupportedGame::SuperMarioWorld, lm_rom::Region::Japan) => 1,
        (lm_rom::SupportedGame::AllStarsAndWorld, _) => 2,
    }
}

fn user_toolbar_launch_options(
    toolbar: Option<&lm_app::UserToolbar>,
    button: &UserToolbarButton,
) -> crate::external_tool_launcher::LaunchOptions {
    crate::external_tool_launcher::LaunchOptions {
        allow_multiple_instances: button
            .options
            .iter()
            .any(|option| option == "LM_ALLOW_MULT_INSTANCES")
            || toolbar.is_some_and(|toolbar| {
                toolbar.global_options.iter().any(|option| {
                    matches!(
                        option,
                        lm_app::UserToolbarGlobalOption::Flag(value)
                            if value == "LM_ALLOW_MULT_INSTANCES_FORCE_ALL"
                    )
                })
            }),
        hide_console_window: button
            .options
            .iter()
            .any(|option| option == "LM_NO_CONSOLE_WINDOW"),
        open_other: button
            .options
            .iter()
            .any(|option| option == "LM_OPEN_OTHER"),
    }
}

fn external_working_directory(
    executable: &str,
    button: &UserToolbarButton,
    app: &lm_app::AppState,
) -> Result<Option<std::path::PathBuf>, String> {
    if let Some(value) = button.working_directory.as_deref() {
        return expand_lm_placeholders(value, app).map(|value| Some(value.into()));
    }
    if button.options.iter().any(|option| option == "LM_DIR_ROM") {
        return app
            .document_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
            .map(Some)
            .ok_or_else(|| "LM_DIR_ROM requires an open ROM".into());
    }
    if button.options.iter().any(|option| option == "LM_DIR_LM") {
        return std::env::current_exe()
            .map_err(|error| format!("cannot locate application executable: {error}"))
            .and_then(|path| {
                path.parent()
                    .map(std::path::Path::to_path_buf)
                    .map(Some)
                    .ok_or_else(|| "application executable has no parent directory".into())
            });
    }
    Ok(std::path::Path::new(executable)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf))
}

fn toolbar_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    texture: Option<&egui::TextureHandle>,
    size: f32,
) -> egui::Response {
    let button = texture.map_or_else(
        || egui::Button::new(label),
        |texture| egui::Button::image(egui::Image::new((texture.id(), egui::vec2(size, size)))),
    );
    ui.add_enabled(enabled, button).on_hover_text(label)
}

fn toggle_user_toolbar_view_state(
    visibility: &mut LevelViewVisibility,
    special_world_passed: &mut bool,
    action: UserToolbarLocalAction,
) {
    match action {
        UserToolbarLocalAction::Layer1 => visibility.layer1 = !visibility.layer1,
        UserToolbarLocalAction::Layer2 => visibility.layer2 = !visibility.layer2,
        UserToolbarLocalAction::Layer3 => visibility.layer3 = !visibility.layer3,
        UserToolbarLocalAction::Sprites => visibility.sprites = !visibility.sprites,
        UserToolbarLocalAction::SpecialWorld => *special_world_passed = !*special_world_passed,
        UserToolbarLocalAction::TileGrid => visibility.tile_grid = !visibility.tile_grid,
        UserToolbarLocalAction::SurfaceOutline => {
            visibility.surface_outline = !visibility.surface_outline;
        }
        UserToolbarLocalAction::LineGuideOutline => {
            visibility.line_guide_outline = !visibility.line_guide_outline;
        }
        UserToolbarLocalAction::ScreenGrid => {
            visibility.screen_overlay =
                if visibility.screen_overlay == LevelScreenOverlay::ScreenGrid {
                    LevelScreenOverlay::None
                } else {
                    LevelScreenOverlay::ScreenGrid
                };
        }
        UserToolbarLocalAction::ScreenExits => {
            visibility.screen_overlay =
                if visibility.screen_overlay == LevelScreenOverlay::ScreenExits {
                    LevelScreenOverlay::None
                } else {
                    LevelScreenOverlay::ScreenExits
                };
        }
        UserToolbarLocalAction::BoundaryGuide => {
            visibility.screen_overlay =
                if visibility.screen_overlay == LevelScreenOverlay::BoundaryGuide {
                    LevelScreenOverlay::None
                } else {
                    LevelScreenOverlay::BoundaryGuide
                };
        }
        UserToolbarLocalAction::ZoomToggle
        | UserToolbarLocalAction::Zoom
        | UserToolbarLocalAction::ZoomFilter
        | UserToolbarLocalAction::Animation
        | UserToolbarLocalAction::IncreaseAnimationFrame
        | UserToolbarLocalAction::ResetAnimation
        | UserToolbarLocalAction::GreenSwitch
        | UserToolbarLocalAction::YellowSwitch
        | UserToolbarLocalAction::BlueSwitch
        | UserToolbarLocalAction::RedSwitch
        | UserToolbarLocalAction::SilverPow
        | UserToolbarLocalAction::BluePow
        | UserToolbarLocalAction::InvisiblePowObjects
        | UserToolbarLocalAction::OtherInvisibleObjects
        | UserToolbarLocalAction::OnOffSwitch
        | UserToolbarLocalAction::ConditionalDirectMap16
        | UserToolbarLocalAction::BlockContents
        | UserToolbarLocalAction::BlockExits
        | UserToolbarLocalAction::HaveStar
        | UserToolbarLocalAction::Time100
        | UserToolbarLocalAction::FiveYoshiCoins
        | UserToolbarLocalAction::CustomTrigger(_)
        | UserToolbarLocalAction::OneShotTrigger(_)
        | UserToolbarLocalAction::ManualTrigger { .. }
        | UserToolbarLocalAction::TriggerSelection { .. }
        | UserToolbarLocalAction::CurrentTrigger { .. }
        | UserToolbarLocalAction::Background512Height
        | UserToolbarLocalAction::Translucent
        | UserToolbarLocalAction::EntranceOverlay(_)
        | UserToolbarLocalAction::ZoomDefault
        | UserToolbarLocalAction::ZoomPlus
        | UserToolbarLocalAction::ZoomMinus => {
            unreachable!("zoom actions are routed through the level editor")
        }
    }
}

fn matching_user_toolbar_buttons(
    toolbar: &lm_app::UserToolbar,
    gestures: &[ShortcutGesture],
) -> Vec<(usize, UserToolbarButton)> {
    toolbar
        .buttons
        .iter()
        .enumerate()
        .filter(|(_, button)| {
            user_toolbar_shortcut(&button.shortcut)
                .is_some_and(|candidate| gestures.contains(&candidate))
        })
        .map(|(index, button)| (index, button.clone()))
        .collect()
}

fn user_toolbar_shortcut(tokens: &[String]) -> Option<ShortcutGesture> {
    let mut modifiers = ShortcutModifiers::default();
    let mut key = None;
    for token in tokens {
        match token.as_str() {
            "VK_CONTROL" | "VK_LCONTROL" | "VK_RCONTROL" => {
                modifiers = modifiers.union(ShortcutModifiers::SECONDARY);
            }
            "VK_SHIFT" | "VK_LSHIFT" | "VK_RSHIFT" => {
                modifiers = modifiers.union(ShortcutModifiers::SHIFT);
            }
            "VK_ALT" | "VK_LALT" | "VK_RALT" => {
                modifiers = modifiers.union(ShortcutModifiers::ALT);
            }
            value => {
                if key.is_some() {
                    return None;
                }
                key = parse_user_toolbar_key(value);
                key?;
            }
        }
    }
    Some(ShortcutGesture {
        modifiers,
        key: key?,
    })
}

fn parse_user_toolbar_key(value: &str) -> Option<ShortcutKey> {
    if let Some(character) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .and_then(|value| {
            let mut characters = value.chars();
            let character = characters.next()?;
            characters.next().is_none().then_some(character)
        })
    {
        return Some(ShortcutKey::Character(character.to_ascii_lowercase()));
    }
    if let Some(number) = value.strip_prefix("VK_F") {
        let number = number.parse::<u8>().ok()?;
        return (1..=24)
            .contains(&number)
            .then_some(ShortcutKey::Function(number));
    }
    Some(match value {
        "VK_INSERT" => ShortcutKey::Insert,
        "VK_DELETE" => ShortcutKey::Delete,
        "VK_HOME" => ShortcutKey::Home,
        "VK_END" => ShortcutKey::End,
        "VK_PAGEUP" => ShortcutKey::PageUp,
        "VK_PAGEDOWN" => ShortcutKey::PageDown,
        "VK_ESCAPE" => ShortcutKey::Escape,
        "VK_TAB" => ShortcutKey::Tab,
        "VK_BACK" => ShortcutKey::Backspace,
        "VK_RETURN" | "VK_NUMPAD_ENTER" => ShortcutKey::Enter,
        "VK_UP" => ShortcutKey::ArrowUp,
        "VK_DOWN" => ShortcutKey::ArrowDown,
        "VK_LEFT" => ShortcutKey::ArrowLeft,
        "VK_RIGHT" => ShortcutKey::ArrowRight,
        "VK_SPACE" => ShortcutKey::Space,
        "VK_PAUSE" => ShortcutKey::Pause,
        "VK_MULTIPLY" => ShortcutKey::NumpadMultiply,
        "VK_ADD" => ShortcutKey::NumpadAdd,
        "VK_SEPARATOR" => ShortcutKey::NumpadSeparator,
        "VK_SUBTRACT" => ShortcutKey::NumpadSubtract,
        "VK_DECIMAL" => ShortcutKey::NumpadDecimal,
        "VK_DIVIDE" => ShortcutKey::NumpadDivide,
        "VK_LBUTTON" => ShortcutKey::MouseLeft,
        "VK_RBUTTON" => ShortcutKey::MouseRight,
        "VK_MBUTTON" => ShortcutKey::MouseMiddle,
        "VK_XBUTTON1" => ShortcutKey::MouseExtra1,
        "VK_XBUTTON2" => ShortcutKey::MouseExtra2,
        value if value.starts_with("VK_NUMPAD") && value.len() == 10 => {
            ShortcutKey::Character(value.chars().last()?)
        }
        value if value.starts_with("0x") || value.starts_with("0X") => {
            virtual_key(u8::from_str_radix(&value[2..], 16).ok()?)?
        }
        _ => return None,
    })
}

fn virtual_key(value: u8) -> Option<ShortcutKey> {
    Some(match value {
        0x01 => ShortcutKey::MouseLeft,
        0x02 => ShortcutKey::MouseRight,
        0x04 => ShortcutKey::MouseMiddle,
        0x05 => ShortcutKey::MouseExtra1,
        0x06 => ShortcutKey::MouseExtra2,
        0x08 => ShortcutKey::Backspace,
        0x09 => ShortcutKey::Tab,
        0x0d => ShortcutKey::Enter,
        0x13 => ShortcutKey::Pause,
        0x1b => ShortcutKey::Escape,
        0x20 => ShortcutKey::Space,
        0x21 => ShortcutKey::PageUp,
        0x22 => ShortcutKey::PageDown,
        0x23 => ShortcutKey::End,
        0x24 => ShortcutKey::Home,
        0x25 => ShortcutKey::ArrowLeft,
        0x26 => ShortcutKey::ArrowUp,
        0x27 => ShortcutKey::ArrowRight,
        0x28 => ShortcutKey::ArrowDown,
        0x2d => ShortcutKey::Insert,
        0x2e => ShortcutKey::Delete,
        0x30..=0x39 | 0x41..=0x5a => ShortcutKey::Character(char::from(value).to_ascii_lowercase()),
        0x6a => ShortcutKey::NumpadMultiply,
        0x6b => ShortcutKey::NumpadAdd,
        0x6c => ShortcutKey::NumpadSeparator,
        0x6d => ShortcutKey::NumpadSubtract,
        0x6e => ShortcutKey::NumpadDecimal,
        0x6f => ShortcutKey::NumpadDivide,
        0x70..=0x87 => ShortcutKey::Function(value - 0x6f),
        _ => return None,
    })
}

fn expand_lm_placeholders(value: &str, app: &lm_app::AppState) -> Result<String, String> {
    let exe_directory = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
    let rom = app.document_path.as_deref();
    let rom_directory = rom.and_then(std::path::Path::parent);
    let rom_name = rom
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str);
    let rom_stem = rom
        .and_then(std::path::Path::file_stem)
        .and_then(std::ffi::OsStr::to_str);
    let replacements = [
        ("%1", rom.map(|path| path.display().to_string())),
        ("%2", rom_directory.map(directory_with_separator)),
        ("%3", rom_name.map(str::to_owned)),
        ("%4", exe_directory.as_deref().map(directory_with_separator)),
        ("%5", rom_stem.map(str::to_owned)),
        ("%7", app.current_level().map(|level| format!("{level:X}"))),
        ("%8", Some(env!("CARGO_PKG_VERSION").replace('.', ""))),
    ];
    let mut output = value.to_owned();
    for (placeholder, replacement) in replacements {
        if output.contains(placeholder) {
            let replacement = replacement.ok_or_else(|| {
                format!("user toolbar placeholder {placeholder} requires an open ROM")
            })?;
            output = output.replace(placeholder, &replacement);
        }
    }
    if output.contains("%9") {
        return Err(
            "user toolbar LM request-window placeholder %9 has no native equivalent".into(),
        );
    }
    Ok(output)
}

fn directory_with_separator(path: &std::path::Path) -> String {
    let mut value = path.display().to_string();
    if !value.ends_with(std::path::MAIN_SEPARATOR) {
        value.push(std::path::MAIN_SEPARATOR);
    }
    value
}

fn user_toolbar_label(target: &UserToolbarTarget) -> &str {
    match target {
        UserToolbarTarget::Spacer => "",
        UserToolbarTarget::Internal(name) => name.strip_prefix("LM_").unwrap_or(name),
        UserToolbarTarget::External(_) => "External Tool",
    }
}

fn user_toolbar_command(name: &str, current_level: Option<u16>) -> Option<Command> {
    Some(match name {
        "LM_FILE_OPEN_ROM" => Command::Open,
        "LM_FILE_RELOAD_ROM" => Command::Reload,
        "LM_FILE_SAVE_BUTTON" | "LM_FILE_SAVE_FILE" => Command::Save,
        "LM_FILE_SAVE_FILE_AS" | "LM_FILE_SAVE_LEVEL_TO_ROM_AS" => Command::SaveAs,
        "LM_FILE_PREVIOUS_LEVEL" | "LM_MOUSE_LEVEL_BACK" => {
            Command::NavigateLevel(LevelNavigationDirection::Back)
        }
        "LM_FILE_NEXT_LEVEL" | "LM_MOUSE_LEVEL_FORWARD" => {
            Command::NavigateLevel(LevelNavigationDirection::Forward)
        }
        "LM_FILE_EXIT" => Command::Quit,
        "LM_FILE_CLOSE_ROM" => Command::Close,
        "LM_EDIT_UNDO" => Command::Undo,
        "LM_EDIT_REDO" => Command::Redo,
        "LM_VIEW_OVERWORLD" => Command::ShowOverworld,
        "LM_VIEW_16x16" | "LM_VIEW_16x16_OLD" => Command::ShowMap16,
        "LM_VIEW_8x8" => Command::ShowGraphics(0),
        "LM_LEVEL_GRAPHICS" => Command::ShowGraphics(current_level?),
        "LM_VIEW_PALETTES" => Command::ShowPalette(0),
        "LM_VIEW_BACK" | "LM_VIEW_BACK_OLD" => Command::SelectLevel(current_level?),
        "LM_KEY_EXANIM_SLOTS" => Command::ShowExAnimation(current_level.unwrap_or(0)),
        "LM_LEVEL_EXTEND_ANI" => Command::ShowExAnimation(current_level?),
        "LM_VIEW_LAYER_3_EDITOR" | "LM_LEVEL_LAYER3_SETTINGS" => {
            Command::ShowLayer3(current_level?)
        }
        _ => return None,
    })
}

fn legacy_graphics_bypass_prerequisite_installed(app: &lm_app::AppState) -> Result<bool, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let image =
        lm_rom::RomImage::from_bytes(snapshot.rom_bytes).map_err(|error| error.to_string())?;
    let project = lm_project::Project::new(image);
    lm_profile::smw_us_v1_installed_expanded_settings_layout(&project)
        .map(|layout| layout.is_some())
        .map_err(|error| error.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UserToolbarNativeAction {
    HelpContents,
    HelpAbout,
    Sprite19Fix,
    AnalyzeLevels,
    RestoreRom,
    CreateRestorePoint,
    CreateIps,
    ApplyIps,
    GraphicsCompressionOptions,
    GeneralOptions,
    RestoreOptions,
    EmulatorSettings,
    ExternalEmulatorRun,
    LiveEmulatorRun,
    LiveEmulatorStop,
    LiveEmulatorPause,
    LiveEmulatorMute,
    LiveEmulatorFrameAdvance,
    DeleteLevel,
    OpenSecondaryEntrances,
    OpenScreenExitAtPointer,
    FollowScreenExitAtPointer,
    ScanInvalidExits,
    OpenLevelExAnimation,
    OpenGlobalExAnimation,
    OpenLayer3Bypass,
    OpenLegacyForegroundBackgroundBypass,
    OpenLegacySpriteBypass,
    PlaceObject,
    PlaceSprite,
    OpenLevelToolPanel(crate::vanilla_level_editor::LevelToolPanel),
    SelectAll,
    Insert,
    DeleteSelection,
    DeleteAll,
    Escape,
    EditLayer1,
    EditLayer2,
    EditSprites,
    Copy,
    Cut,
    Paste,
    Nudge { x: i32, y: i32 },
    ZOrderStep { increase: bool },
    OverlapZOrder(crate::vanilla_level_editor::ZOrderTraversal),
    ConditionalDirectMap16,
    RemapDirectMap16,
    TogglePropertiesWindow,
    OpenManualEditDialog,
    ExpandRom(RomExpansionPreset),
    ExportAllLevels,
    ExportModifiedLevels,
    ExportCurrentLevelBitmap,
    ExportLevelBitmapDirectory,
    SharedPaletteTransfer(crate::rom_shared_palette_editor::SharedPaletteTransferAction),
    CurrentLevelPaletteTransfer(crate::current_level_palette_transfer::CurrentLevelPaletteAction),
    ExtractGraphics(QuickGraphicsExtraction),
    QuickExtractGraphics(QuickGraphicsExtraction),
    QuickInsertGraphics(QuickGraphicsInsertion),
    OrdinaryInsertGraphics(GraphicsInsertionFamily),
}

fn user_toolbar_native_action(name: &str) -> Option<UserToolbarNativeAction> {
    Some(match name {
        "LM_HELP_CONTENTS" => UserToolbarNativeAction::HelpContents,
        "LM_HELP_ABOUT" => UserToolbarNativeAction::HelpAbout,
        "LM_KEY_SPRITE19_FIX" => UserToolbarNativeAction::Sprite19Fix,
        "LM_FILE_ANALYZE_LEVELS" => UserToolbarNativeAction::AnalyzeLevels,
        "LM_FILE_RESTORE" => UserToolbarNativeAction::RestoreRom,
        "LM_FILE_CREATE_RESTORE" => UserToolbarNativeAction::CreateRestorePoint,
        "LM_FILE_CREATE_IPS" => UserToolbarNativeAction::CreateIps,
        "LM_FILE_APPLY_IPS" => UserToolbarNativeAction::ApplyIps,
        "LM_OPTIONS_COMPRESSION" => UserToolbarNativeAction::GraphicsCompressionOptions,
        "LM_OPTIONS_GENERAL" => UserToolbarNativeAction::GeneralOptions,
        "LM_OPTIONS_RESTORE" => UserToolbarNativeAction::RestoreOptions,
        "LM_FILE_EMULATOR_SETTINGS" | "LM_FILE_TILE_EDITOR_SETTINGS" => {
            UserToolbarNativeAction::EmulatorSettings
        }
        "LM_FILE_EMULATOR_RUN" => UserToolbarNativeAction::ExternalEmulatorRun,
        "LM_FILE_INT_EMULATOR_RUN" => UserToolbarNativeAction::LiveEmulatorRun,
        "LM_FILE_INT_EMULATOR_UNLOAD" => UserToolbarNativeAction::LiveEmulatorStop,
        "LM_FILE_INT_EMULATOR_PAUSE" => UserToolbarNativeAction::LiveEmulatorPause,
        "LM_FILE_INT_EMULATOR_MUTE" => UserToolbarNativeAction::LiveEmulatorMute,
        "LM_FILE_INT_EMULATOR_FRAME_ADVANCE" => UserToolbarNativeAction::LiveEmulatorFrameAdvance,
        "LM_FILE_DELETE_LEVEL" => UserToolbarNativeAction::DeleteLevel,
        "LM_LEVEL_ENTRANCE2" => UserToolbarNativeAction::OpenSecondaryEntrances,
        "LM_MOUSE_EDIT_SCREEN_EXIT" => UserToolbarNativeAction::OpenScreenExitAtPointer,
        "LM_MOUSE_SCREEN_EXIT" => UserToolbarNativeAction::FollowScreenExitAtPointer,
        "LM_LEVEL_SCAN_EXITS" => UserToolbarNativeAction::ScanInvalidExits,
        "LM_LEVEL_EX20_LEVEL" | "LM_LEVEL_EX20_SETTINGS" => {
            UserToolbarNativeAction::OpenLevelExAnimation
        }
        "LM_LEVEL_EX20_GLOBAL" => UserToolbarNativeAction::OpenGlobalExAnimation,
        "LM_LEVEL_SUPER_BYPASS"
        | "LM_LEVEL_SUPER_BYPASS2"
        | "LM_LEVEL_LAYER3_BYPASS"
        | "LM_LEVEL_LAYER3_BYPASS2" => UserToolbarNativeAction::OpenLayer3Bypass,
        "LM_LEVEL_BYPASS_FG" => UserToolbarNativeAction::OpenLegacyForegroundBackgroundBypass,
        "LM_LEVEL_BYPASS_SP" => UserToolbarNativeAction::OpenLegacySpriteBypass,
        "LM_VIEW_ADD_OBJECT" | "LM_VIEW_OBJECT" | "LM_VIEW_ADD_OBJECT_OLD" => {
            UserToolbarNativeAction::PlaceObject
        }
        "LM_VIEW_ADD_SPRITE" | "LM_VIEW_SPRITE" | "LM_VIEW_ADD_SPRITE_OLD" => {
            UserToolbarNativeAction::PlaceSprite
        }
        "LM_VIEW_BACKGROUND" | "LM_LEVEL_BG" => UserToolbarNativeAction::OpenLevelToolPanel(
            crate::vanilla_level_editor::LevelToolPanel::Layer2,
        ),
        "LM_VIEW_SPRITE_DATA" | "LM_LEVEL_SPRITES" => UserToolbarNativeAction::OpenLevelToolPanel(
            crate::vanilla_level_editor::LevelToolPanel::Sprites,
        ),
        "LM_LEVEL_ENTRANCE"
        | "LM_LEVEL_PROPERTIES"
        | "LM_LEVEL_OTHER"
        | "LM_LEVEL_BYPASS_MUSIC"
        | "LM_LEVEL_LAYER12_SETTINGS" => UserToolbarNativeAction::OpenLevelToolPanel(
            crate::vanilla_level_editor::LevelToolPanel::Settings,
        ),
        "LM_LEVEL_EXITS" => UserToolbarNativeAction::OpenLevelToolPanel(
            crate::vanilla_level_editor::LevelToolPanel::ScreenExits,
        ),
        "LM_EDIT_SELECT_ALL" => UserToolbarNativeAction::SelectAll,
        "LM_EDIT_INSERT" => UserToolbarNativeAction::Insert,
        "LM_EDIT_DELETE" => UserToolbarNativeAction::DeleteSelection,
        "LM_EDIT_DELETE_ALL" => UserToolbarNativeAction::DeleteAll,
        "LM_EDIT_ESCAPE" => UserToolbarNativeAction::Escape,
        "LM_EDIT_EDIT_LAYER_1" => UserToolbarNativeAction::EditLayer1,
        "LM_EDIT_EDIT_LAYER_2" => UserToolbarNativeAction::EditLayer2,
        "LM_EDIT_SPRITES" => UserToolbarNativeAction::EditSprites,
        "LM_EDIT_COPY" => UserToolbarNativeAction::Copy,
        "LM_EDIT_CUT" => UserToolbarNativeAction::Cut,
        "LM_EDIT_PASTE" => UserToolbarNativeAction::Paste,
        "LM_EDIT_INCREASE_X" => UserToolbarNativeAction::Nudge { x: 1, y: 0 },
        "LM_EDIT_DECREASE_X" => UserToolbarNativeAction::Nudge { x: -1, y: 0 },
        "LM_EDIT_INCREASE_Y" => UserToolbarNativeAction::Nudge { x: 0, y: 1 },
        "LM_EDIT_DECREASE_Y" => UserToolbarNativeAction::Nudge { x: 0, y: -1 },
        "LM_EDIT_ZORDER_UP" => UserToolbarNativeAction::ZOrderStep { increase: true },
        "LM_EDIT_ZORDER_DOWN" => UserToolbarNativeAction::ZOrderStep { increase: false },
        "LM_EDIT_BRING_FORWARD" => UserToolbarNativeAction::OverlapZOrder(
            crate::vanilla_level_editor::ZOrderTraversal::Forward,
        ),
        "LM_EDIT_SEND_BACKWARD" => UserToolbarNativeAction::OverlapZOrder(
            crate::vanilla_level_editor::ZOrderTraversal::Backward,
        ),
        "LM_EDIT_BRING_TO_FRONT" => UserToolbarNativeAction::OverlapZOrder(
            crate::vanilla_level_editor::ZOrderTraversal::Front,
        ),
        "LM_EDIT_SEND_TO_BACK" => UserToolbarNativeAction::OverlapZOrder(
            crate::vanilla_level_editor::ZOrderTraversal::Back,
        ),
        "LM_EDIT_CDM16" => UserToolbarNativeAction::ConditionalDirectMap16,
        "LM_EDIT_REMAP_DM16" => UserToolbarNativeAction::RemapDirectMap16,
        "LM_EDIT_PROPERTIES" => UserToolbarNativeAction::TogglePropertiesWindow,
        "LM_EDIT_EDIT_MANUAL" => UserToolbarNativeAction::OpenManualEditDialog,
        "LM_FILE_EXPAND_ROM2" => UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom2MiB),
        "LM_FILE_EXPAND_ROM3" => UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom3MiB),
        "LM_FILE_EXPAND_ROM4" => UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom4MiB),
        "LM_FILE_EXPAND_ROM8" => {
            UserToolbarNativeAction::ExpandRom(RomExpansionPreset::ExLoRom8MiB)
        }
        "LM_FILE_EXPAND_ROM6_SA1" => {
            UserToolbarNativeAction::ExpandRom(RomExpansionPreset::Sa1_6MiB)
        }
        "LM_FILE_EXPAND_ROM8_SA1" => {
            UserToolbarNativeAction::ExpandRom(RomExpansionPreset::Sa1_8MiB)
        }
        "LM_FILE_EXPORT_DIRECTORY" => UserToolbarNativeAction::ExportAllLevels,
        "LM_FILE_SAVE_DIRECTORY" => UserToolbarNativeAction::ExportModifiedLevels,
        "LM_FILE_EXPORT_DIRECTORY_BITMAP" => UserToolbarNativeAction::ExportLevelBitmapDirectory,
        "LM_FILE_EXPORT_BITMAP" => UserToolbarNativeAction::ExportCurrentLevelBitmap,
        "LM_FILE_EXTRACT_PALETTE" => UserToolbarNativeAction::SharedPaletteTransfer(
            crate::rom_shared_palette_editor::SharedPaletteTransferAction::Export,
        ),
        "LM_FILE_INSERT_PALETTE" => UserToolbarNativeAction::SharedPaletteTransfer(
            crate::rom_shared_palette_editor::SharedPaletteTransferAction::Import,
        ),
        "LM_FILE_EXPORT_PALETTE" => UserToolbarNativeAction::CurrentLevelPaletteTransfer(
            crate::current_level_palette_transfer::CurrentLevelPaletteAction::Export,
        ),
        "LM_FILE_IMPORT_PALETTE" => UserToolbarNativeAction::CurrentLevelPaletteTransfer(
            crate::current_level_palette_transfer::CurrentLevelPaletteAction::Import,
        ),
        "LM_FILE_EXTRACT_GFX_BUTTON" => {
            UserToolbarNativeAction::QuickExtractGraphics(QuickGraphicsExtraction::Standard)
        }
        "LM_FILE_EXTRACT_EXGFX_BUTTON" => {
            UserToolbarNativeAction::QuickExtractGraphics(QuickGraphicsExtraction::ExGraphics)
        }
        "LM_FILE_EXTRACT_GFX" => {
            UserToolbarNativeAction::ExtractGraphics(QuickGraphicsExtraction::Standard)
        }
        "LM_FILE_EXTRACT_EXGFX" => {
            UserToolbarNativeAction::ExtractGraphics(QuickGraphicsExtraction::ExGraphics)
        }
        "LM_FILE_INSERT_GFX_BUTTON" => {
            UserToolbarNativeAction::QuickInsertGraphics(QuickGraphicsInsertion::Standard)
        }
        "LM_FILE_INSERT_EXGFX_BUTTON" => {
            UserToolbarNativeAction::QuickInsertGraphics(QuickGraphicsInsertion::ExGraphics)
        }
        "LM_FILE_INSERT_GFX" => {
            UserToolbarNativeAction::OrdinaryInsertGraphics(GraphicsInsertionFamily::Standard)
        }
        "LM_FILE_INSERT_EXGFX" => {
            UserToolbarNativeAction::OrdinaryInsertGraphics(GraphicsInsertionFamily::ExGraphics)
        }
        _ => return None,
    })
}

fn configured_snes_emulator_tool_id(tools: &[lm_app::ExternalTool]) -> Option<String> {
    tools
        .iter()
        .find(|tool| {
            !tool.id.to_ascii_lowercase().contains("gba")
                && (tool.uses_argument_placeholder("rom")
                    || tool.uses_argument_placeholder("rom_8dot3"))
        })
        .map(|tool| tool.id.clone())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UserToolbarLocalAction {
    Layer1,
    Layer2,
    Layer3,
    Sprites,
    SpecialWorld,
    TileGrid,
    SurfaceOutline,
    LineGuideOutline,
    ScreenGrid,
    ScreenExits,
    BoundaryGuide,
    Zoom,
    ZoomFilter,
    Animation,
    IncreaseAnimationFrame,
    ResetAnimation,
    GreenSwitch,
    YellowSwitch,
    BlueSwitch,
    RedSwitch,
    SilverPow,
    BluePow,
    InvisiblePowObjects,
    OtherInvisibleObjects,
    OnOffSwitch,
    ConditionalDirectMap16,
    BlockContents,
    BlockExits,
    HaveStar,
    Time100,
    FiveYoshiCoins,
    CustomTrigger(u8),
    OneShotTrigger(u8),
    ManualTrigger { trigger: u8, delta: i8 },
    TriggerSelection { family: u8, delta: i8 },
    CurrentTrigger { family: u8, delta: i8 },
    Background512Height,
    Translucent,
    EntranceOverlay(crate::vanilla_level_editor::EntranceOverlayToggle),
    ZoomToggle,
    ZoomDefault,
    ZoomPlus,
    ZoomMinus,
}

const ROM_LEVEL_TOOLBAR_ZOOM_STEP: i16 = 100;

fn user_toolbar_local_action(name: &str) -> Option<UserToolbarLocalAction> {
    if let Some(trigger) = hexadecimal_suffix(name, "LM_VIEW_CUSTOM_TRIGGER_", 0x0f) {
        return Some(UserToolbarLocalAction::CustomTrigger(trigger));
    }
    if let Some(trigger) = hexadecimal_suffix(name, "LM_VIEW_ONESHOT_TRIGGER_", 0x1f) {
        return Some(UserToolbarLocalAction::OneShotTrigger(trigger));
    }
    if let Some(trigger) = hexadecimal_suffix(name, "LM_VIEW_MANUAL_TRIGGER_INC_", 0x0f) {
        return Some(UserToolbarLocalAction::ManualTrigger { trigger, delta: 1 });
    }
    if let Some(trigger) = hexadecimal_suffix(name, "LM_VIEW_MANUAL_TRIGGER_DEC_", 0x0f) {
        return Some(UserToolbarLocalAction::ManualTrigger { trigger, delta: -1 });
    }
    Some(match name {
        "LM_VIEW_LAYER_1" => UserToolbarLocalAction::Layer1,
        "LM_VIEW_LAYER_2" => UserToolbarLocalAction::Layer2,
        "LM_VIEW_LAYER_3" => UserToolbarLocalAction::Layer3,
        "LM_VIEW_SPRITES" => UserToolbarLocalAction::Sprites,
        "LM_VIEW_SPECIAL_WORLD" => UserToolbarLocalAction::SpecialWorld,
        "LM_VIEW_TILE_GRID" => UserToolbarLocalAction::TileGrid,
        "LM_VIEW_SURFACE_OUTLINE" => UserToolbarLocalAction::SurfaceOutline,
        "LM_VIEW_LINE_GUIDE_OUTLINE" => UserToolbarLocalAction::LineGuideOutline,
        "LM_VIEW_SCREEN_GRID" => UserToolbarLocalAction::ScreenGrid,
        "LM_VIEW_SCREEN_EXITS" => UserToolbarLocalAction::ScreenExits,
        "LM_VIEW_SCREEN_GRID_2" => UserToolbarLocalAction::BoundaryGuide,
        "LM_VIEW_ALL_ENTRANCES" => UserToolbarLocalAction::EntranceOverlay(
            crate::vanilla_level_editor::EntranceOverlayToggle::All,
        ),
        "LM_VIEW_LEVEL_ENTRANCE" => UserToolbarLocalAction::EntranceOverlay(
            crate::vanilla_level_editor::EntranceOverlayToggle::Primary,
        ),
        "LM_VIEW_LEVEL_ENTRANCE_2" => UserToolbarLocalAction::EntranceOverlay(
            crate::vanilla_level_editor::EntranceOverlayToggle::Secondary,
        ),
        "LM_VIEW_MIDWAY_POINT" => UserToolbarLocalAction::EntranceOverlay(
            crate::vanilla_level_editor::EntranceOverlayToggle::Midway,
        ),
        "LM_VIEW_ZOOM" => UserToolbarLocalAction::Zoom,
        "LM_VIEW_ZOOM_FILTER" => UserToolbarLocalAction::ZoomFilter,
        "LM_VIEW_ANIMATION" => UserToolbarLocalAction::Animation,
        "LM_VIEW_INCREASE_FRAME" => UserToolbarLocalAction::IncreaseAnimationFrame,
        "LM_VIEW_RESET_ANIMATION" => UserToolbarLocalAction::ResetAnimation,
        "LM_VIEW_GREEN_SWITCH" => UserToolbarLocalAction::GreenSwitch,
        "LM_VIEW_YELLOW_SWITCH" => UserToolbarLocalAction::YellowSwitch,
        "LM_VIEW_BLUE_SWITCH" => UserToolbarLocalAction::BlueSwitch,
        "LM_VIEW_RED_SWITCH" => UserToolbarLocalAction::RedSwitch,
        "LM_VIEW_SILVER_POW" => UserToolbarLocalAction::SilverPow,
        "LM_VIEW_POW" => UserToolbarLocalAction::BluePow,
        "LM_VIEW_INVISIBLE" => UserToolbarLocalAction::InvisiblePowObjects,
        "LM_VIEW_INVISIBLE_2" => UserToolbarLocalAction::OtherInvisibleObjects,
        "LM_VIEW_LINE_ON" => UserToolbarLocalAction::OnOffSwitch,
        "LM_VIEW_CDM16" => UserToolbarLocalAction::ConditionalDirectMap16,
        "LM_VIEW_BLOCK_CONTENTS" => UserToolbarLocalAction::BlockContents,
        "LM_VIEW_BLOCK_EXITS" => UserToolbarLocalAction::BlockExits,
        "LM_VIEW_HAVE_STAR" => UserToolbarLocalAction::HaveStar,
        "LM_VIEW_TIME_100" => UserToolbarLocalAction::Time100,
        "LM_VIEW_5YOSHI_COINS" => UserToolbarLocalAction::FiveYoshiCoins,
        "LM_VIEW_CUSTOM_TRIGGER_INC" => UserToolbarLocalAction::TriggerSelection {
            family: 0,
            delta: 1,
        },
        "LM_VIEW_CUSTOM_TRIGGER_DEC" => UserToolbarLocalAction::TriggerSelection {
            family: 0,
            delta: -1,
        },
        "LM_VIEW_ONESHOT_TRIGGER_INC" => UserToolbarLocalAction::TriggerSelection {
            family: 1,
            delta: 1,
        },
        "LM_VIEW_ONESHOT_TRIGGER_DEC" => UserToolbarLocalAction::TriggerSelection {
            family: 1,
            delta: -1,
        },
        "LM_VIEW_MANUAL_TRIGGER_INC" => UserToolbarLocalAction::TriggerSelection {
            family: 2,
            delta: 1,
        },
        "LM_VIEW_MANUAL_TRIGGER_DEC" => UserToolbarLocalAction::TriggerSelection {
            family: 2,
            delta: -1,
        },
        "LM_VIEW_CUSTOM_TRIGGER_CURRENT" => UserToolbarLocalAction::CurrentTrigger {
            family: 0,
            delta: 0,
        },
        "LM_VIEW_ONESHOT_TRIGGER_CURRENT" => UserToolbarLocalAction::CurrentTrigger {
            family: 1,
            delta: 0,
        },
        "LM_VIEW_MANUAL_TRIGGER_CURRENT_INC" => UserToolbarLocalAction::CurrentTrigger {
            family: 2,
            delta: 1,
        },
        "LM_VIEW_MANUAL_TRIGGER_CURRENT_DEC" => UserToolbarLocalAction::CurrentTrigger {
            family: 2,
            delta: -1,
        },
        "LM_VIEW_512HEIGHT_BG" => UserToolbarLocalAction::Background512Height,
        "LM_VIEW_TRANSLUCENT" => UserToolbarLocalAction::Translucent,
        "LM_VIEW_ZOOM_TOGGLE" => UserToolbarLocalAction::ZoomToggle,
        "LM_VIEW_ZOOM_DEFAULT" => UserToolbarLocalAction::ZoomDefault,
        "LM_VIEW_ZOOM_PLUS" => UserToolbarLocalAction::ZoomPlus,
        "LM_VIEW_ZOOM_MINUS" => UserToolbarLocalAction::ZoomMinus,
        _ => return None,
    })
}

fn hexadecimal_suffix(name: &str, prefix: &str, maximum: u8) -> Option<u8> {
    let suffix = name.strip_prefix(prefix)?;
    if suffix.is_empty() || suffix.len() > 2 {
        return None;
    }
    let value = u8::from_str_radix(suffix, 16).ok()?;
    (value <= maximum).then_some(value)
}

fn split_command_line(value: &str) -> Result<(String, Vec<String>), String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for character in value.chars() {
        match character {
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            value => word.push(value),
        }
    }
    if quoted {
        return Err("user toolbar external command has an unterminated quote".into());
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut words = words.into_iter();
    let executable = words
        .next()
        .ok_or_else(|| "user toolbar external command is empty".to_owned())?;
    Ok((executable, words.collect()))
}

#[cfg(test)]
mod user_toolbar_tests {
    use super::*;

    #[test]
    fn external_command_line_preserves_quoted_arguments() {
        assert_eq!(
            split_command_line(r#""tool path.exe" "a b" plain"#).unwrap(),
            ("tool path.exe".into(), vec!["a b".into(), "plain".into()])
        );
        assert!(split_command_line("\"unfinished").is_err());
    }

    #[test]
    fn original_internal_names_map_to_native_commands() {
        assert_eq!(
            user_toolbar_command("LM_FILE_OPEN_ROM", None),
            Some(Command::Open)
        );
        assert_eq!(
            user_toolbar_command("LM_FILE_RELOAD_ROM", None),
            Some(Command::Reload)
        );
        assert_eq!(
            user_toolbar_native_action("LM_EDIT_INSERT"),
            Some(UserToolbarNativeAction::Insert)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_OVERWORLD", None),
            Some(Command::ShowOverworld)
        );
        assert_eq!(
            user_toolbar_command("LM_MOUSE_LEVEL_BACK", None),
            Some(Command::NavigateLevel(LevelNavigationDirection::Back))
        );
        assert_eq!(
            user_toolbar_command("LM_MOUSE_LEVEL_FORWARD", None),
            Some(Command::NavigateLevel(LevelNavigationDirection::Forward))
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_8x8", Some(0x105)),
            Some(Command::ShowGraphics(0))
        );
        assert_eq!(
            user_toolbar_command("LM_KEY_EXANIM_SLOTS", Some(0x105)),
            Some(Command::ShowExAnimation(0x105))
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_LAYER_3_EDITOR", Some(0x106)),
            Some(Command::ShowLayer3(0x106))
        );
        assert_eq!(
            user_toolbar_command("LM_LEVEL_GRAPHICS", Some(0x106)),
            Some(Command::ShowGraphics(0x106))
        );
        assert_eq!(
            user_toolbar_command("LM_LEVEL_EXTEND_ANI", Some(0x106)),
            Some(Command::ShowExAnimation(0x106))
        );
        assert_eq!(
            user_toolbar_command("LM_LEVEL_LAYER3_SETTINGS", Some(0x106)),
            Some(Command::ShowLayer3(0x106))
        );
        assert_eq!(user_toolbar_command("LM_VIEW_LAYER_3_EDITOR", None), None);
        assert_eq!(user_toolbar_command("LM_LEVEL_GRAPHICS", None), None);
        assert_eq!(user_toolbar_command("LM_UNKNOWN", None), None);
        assert_eq!(
            user_toolbar_native_action("LM_HELP_CONTENTS"),
            Some(UserToolbarNativeAction::HelpContents)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_BG"),
            Some(UserToolbarNativeAction::OpenLevelToolPanel(
                crate::vanilla_level_editor::LevelToolPanel::Layer2
            ))
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_SPRITES"),
            Some(UserToolbarNativeAction::OpenLevelToolPanel(
                crate::vanilla_level_editor::LevelToolPanel::Sprites
            ))
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_PROPERTIES"),
            Some(UserToolbarNativeAction::OpenLevelToolPanel(
                crate::vanilla_level_editor::LevelToolPanel::Settings
            ))
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_BYPASS_MUSIC"),
            Some(UserToolbarNativeAction::OpenLevelToolPanel(
                crate::vanilla_level_editor::LevelToolPanel::Settings
            ))
        );
        assert_eq!(
            user_toolbar_native_action("LM_OPTIONS_GENERAL"),
            Some(UserToolbarNativeAction::GeneralOptions)
        );
        assert_eq!(
            user_toolbar_native_action("LM_OPTIONS_RESTORE"),
            Some(UserToolbarNativeAction::RestoreOptions)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_ENTRANCE2"),
            Some(UserToolbarNativeAction::OpenSecondaryEntrances)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_EXITS"),
            Some(UserToolbarNativeAction::OpenLevelToolPanel(
                crate::vanilla_level_editor::LevelToolPanel::ScreenExits
            ))
        );
        assert_eq!(
            user_toolbar_native_action("LM_MOUSE_EDIT_SCREEN_EXIT"),
            Some(UserToolbarNativeAction::OpenScreenExitAtPointer)
        );
        assert_eq!(
            user_toolbar_native_action("LM_MOUSE_SCREEN_EXIT"),
            Some(UserToolbarNativeAction::FollowScreenExitAtPointer)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_SCAN_EXITS"),
            Some(UserToolbarNativeAction::ScanInvalidExits)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_EX20_LEVEL"),
            Some(UserToolbarNativeAction::OpenLevelExAnimation)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_EX20_GLOBAL"),
            Some(UserToolbarNativeAction::OpenGlobalExAnimation)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_EX20_SETTINGS"),
            Some(UserToolbarNativeAction::OpenLevelExAnimation)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_LAYER3_BYPASS"),
            Some(UserToolbarNativeAction::OpenLayer3Bypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_LAYER3_BYPASS2"),
            Some(UserToolbarNativeAction::OpenLayer3Bypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_SUPER_BYPASS"),
            Some(UserToolbarNativeAction::OpenLayer3Bypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_SUPER_BYPASS2"),
            Some(UserToolbarNativeAction::OpenLayer3Bypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_BYPASS_FG"),
            Some(UserToolbarNativeAction::OpenLegacyForegroundBackgroundBypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_LEVEL_BYPASS_SP"),
            Some(UserToolbarNativeAction::OpenLegacySpriteBypass)
        );
        assert_eq!(
            user_toolbar_native_action("LM_HELP_ABOUT"),
            Some(UserToolbarNativeAction::HelpAbout)
        );
        assert_eq!(user_toolbar_native_action("LM_HELP_UNKNOWN"), None);
        assert_eq!(
            user_toolbar_native_action("LM_KEY_SPRITE19_FIX"),
            Some(UserToolbarNativeAction::Sprite19Fix)
        );
        for (name, action) in [
            (
                "LM_FILE_ANALYZE_LEVELS",
                UserToolbarNativeAction::AnalyzeLevels,
            ),
            ("LM_FILE_RESTORE", UserToolbarNativeAction::RestoreRom),
            (
                "LM_FILE_CREATE_RESTORE",
                UserToolbarNativeAction::CreateRestorePoint,
            ),
            ("LM_FILE_CREATE_IPS", UserToolbarNativeAction::CreateIps),
            ("LM_FILE_APPLY_IPS", UserToolbarNativeAction::ApplyIps),
            (
                "LM_OPTIONS_COMPRESSION",
                UserToolbarNativeAction::GraphicsCompressionOptions,
            ),
            (
                "LM_FILE_EMULATOR_SETTINGS",
                UserToolbarNativeAction::EmulatorSettings,
            ),
            (
                "LM_FILE_TILE_EDITOR_SETTINGS",
                UserToolbarNativeAction::EmulatorSettings,
            ),
            (
                "LM_FILE_EMULATOR_RUN",
                UserToolbarNativeAction::ExternalEmulatorRun,
            ),
            (
                "LM_FILE_INT_EMULATOR_RUN",
                UserToolbarNativeAction::LiveEmulatorRun,
            ),
            (
                "LM_FILE_INT_EMULATOR_UNLOAD",
                UserToolbarNativeAction::LiveEmulatorStop,
            ),
            (
                "LM_FILE_INT_EMULATOR_PAUSE",
                UserToolbarNativeAction::LiveEmulatorPause,
            ),
            (
                "LM_FILE_INT_EMULATOR_MUTE",
                UserToolbarNativeAction::LiveEmulatorMute,
            ),
            (
                "LM_FILE_INT_EMULATOR_FRAME_ADVANCE",
                UserToolbarNativeAction::LiveEmulatorFrameAdvance,
            ),
            ("LM_VIEW_ADD_OBJECT", UserToolbarNativeAction::PlaceObject),
            ("LM_VIEW_OBJECT", UserToolbarNativeAction::PlaceObject),
            (
                "LM_VIEW_ADD_OBJECT_OLD",
                UserToolbarNativeAction::PlaceObject,
            ),
            ("LM_VIEW_ADD_SPRITE", UserToolbarNativeAction::PlaceSprite),
            ("LM_VIEW_SPRITE", UserToolbarNativeAction::PlaceSprite),
            (
                "LM_VIEW_ADD_SPRITE_OLD",
                UserToolbarNativeAction::PlaceSprite,
            ),
            ("LM_EDIT_SELECT_ALL", UserToolbarNativeAction::SelectAll),
            ("LM_EDIT_DELETE", UserToolbarNativeAction::DeleteSelection),
            ("LM_EDIT_DELETE_ALL", UserToolbarNativeAction::DeleteAll),
            ("LM_EDIT_ESCAPE", UserToolbarNativeAction::Escape),
            ("LM_EDIT_EDIT_LAYER_1", UserToolbarNativeAction::EditLayer1),
            ("LM_EDIT_EDIT_LAYER_2", UserToolbarNativeAction::EditLayer2),
            ("LM_EDIT_SPRITES", UserToolbarNativeAction::EditSprites),
            ("LM_EDIT_COPY", UserToolbarNativeAction::Copy),
            ("LM_EDIT_CUT", UserToolbarNativeAction::Cut),
            ("LM_EDIT_PASTE", UserToolbarNativeAction::Paste),
            (
                "LM_EDIT_CDM16",
                UserToolbarNativeAction::ConditionalDirectMap16,
            ),
            (
                "LM_EDIT_REMAP_DM16",
                UserToolbarNativeAction::RemapDirectMap16,
            ),
            (
                "LM_EDIT_PROPERTIES",
                UserToolbarNativeAction::TogglePropertiesWindow,
            ),
            (
                "LM_EDIT_EDIT_MANUAL",
                UserToolbarNativeAction::OpenManualEditDialog,
            ),
            (
                "LM_FILE_EXPAND_ROM2",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom2MiB),
            ),
            (
                "LM_FILE_EXPAND_ROM3",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom3MiB),
            ),
            (
                "LM_FILE_EXPAND_ROM4",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::LoRom4MiB),
            ),
            (
                "LM_FILE_EXPAND_ROM8",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::ExLoRom8MiB),
            ),
            (
                "LM_FILE_EXPAND_ROM6_SA1",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::Sa1_6MiB),
            ),
            (
                "LM_FILE_EXPAND_ROM8_SA1",
                UserToolbarNativeAction::ExpandRom(RomExpansionPreset::Sa1_8MiB),
            ),
            (
                "LM_FILE_EXPORT_DIRECTORY",
                UserToolbarNativeAction::ExportAllLevels,
            ),
            (
                "LM_FILE_SAVE_DIRECTORY",
                UserToolbarNativeAction::ExportModifiedLevels,
            ),
            (
                "LM_FILE_EXPORT_DIRECTORY_BITMAP",
                UserToolbarNativeAction::ExportLevelBitmapDirectory,
            ),
            (
                "LM_FILE_EXPORT_BITMAP",
                UserToolbarNativeAction::ExportCurrentLevelBitmap,
            ),
            (
                "LM_FILE_EXTRACT_PALETTE",
                UserToolbarNativeAction::SharedPaletteTransfer(
                    crate::rom_shared_palette_editor::SharedPaletteTransferAction::Export,
                ),
            ),
            (
                "LM_FILE_INSERT_PALETTE",
                UserToolbarNativeAction::SharedPaletteTransfer(
                    crate::rom_shared_palette_editor::SharedPaletteTransferAction::Import,
                ),
            ),
            (
                "LM_FILE_EXPORT_PALETTE",
                UserToolbarNativeAction::CurrentLevelPaletteTransfer(
                    crate::current_level_palette_transfer::CurrentLevelPaletteAction::Export,
                ),
            ),
            (
                "LM_FILE_IMPORT_PALETTE",
                UserToolbarNativeAction::CurrentLevelPaletteTransfer(
                    crate::current_level_palette_transfer::CurrentLevelPaletteAction::Import,
                ),
            ),
            (
                "LM_FILE_EXTRACT_GFX_BUTTON",
                UserToolbarNativeAction::QuickExtractGraphics(QuickGraphicsExtraction::Standard),
            ),
            (
                "LM_FILE_EXTRACT_EXGFX_BUTTON",
                UserToolbarNativeAction::QuickExtractGraphics(QuickGraphicsExtraction::ExGraphics),
            ),
            (
                "LM_FILE_EXTRACT_GFX",
                UserToolbarNativeAction::ExtractGraphics(QuickGraphicsExtraction::Standard),
            ),
            (
                "LM_FILE_EXTRACT_EXGFX",
                UserToolbarNativeAction::ExtractGraphics(QuickGraphicsExtraction::ExGraphics),
            ),
            (
                "LM_FILE_INSERT_GFX_BUTTON",
                UserToolbarNativeAction::QuickInsertGraphics(QuickGraphicsInsertion::Standard),
            ),
            (
                "LM_FILE_INSERT_EXGFX_BUTTON",
                UserToolbarNativeAction::QuickInsertGraphics(QuickGraphicsInsertion::ExGraphics),
            ),
            (
                "LM_FILE_INSERT_GFX",
                UserToolbarNativeAction::OrdinaryInsertGraphics(GraphicsInsertionFamily::Standard),
            ),
            (
                "LM_FILE_INSERT_EXGFX",
                UserToolbarNativeAction::OrdinaryInsertGraphics(
                    GraphicsInsertionFamily::ExGraphics,
                ),
            ),
        ] {
            assert_eq!(user_toolbar_native_action(name), Some(action));
        }
        assert_eq!(
            user_toolbar_command("LM_FILE_CLOSE_ROM", None),
            Some(Command::Close)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_16x16_OLD", None),
            Some(Command::ShowMap16)
        );
        assert_eq!(
            user_toolbar_command("LM_VIEW_BACK", Some(0x106)),
            Some(Command::SelectLevel(0x106))
        );
        assert_eq!(user_toolbar_command("LM_VIEW_BACK_OLD", None), None);
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_LAYER_1"),
            Some(UserToolbarLocalAction::Layer1)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SPECIAL_WORLD"),
            Some(UserToolbarLocalAction::SpecialWorld)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM"),
            Some(UserToolbarLocalAction::Zoom)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_FILTER"),
            Some(UserToolbarLocalAction::ZoomFilter)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ANIMATION"),
            Some(UserToolbarLocalAction::Animation)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_INCREASE_FRAME"),
            Some(UserToolbarLocalAction::IncreaseAnimationFrame)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_RESET_ANIMATION"),
            Some(UserToolbarLocalAction::ResetAnimation)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_GREEN_SWITCH"),
            Some(UserToolbarLocalAction::GreenSwitch)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_YELLOW_SWITCH"),
            Some(UserToolbarLocalAction::YellowSwitch)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_BLUE_SWITCH"),
            Some(UserToolbarLocalAction::BlueSwitch)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_RED_SWITCH"),
            Some(UserToolbarLocalAction::RedSwitch)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SILVER_POW"),
            Some(UserToolbarLocalAction::SilverPow)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_POW"),
            Some(UserToolbarLocalAction::BluePow)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_INVISIBLE"),
            Some(UserToolbarLocalAction::InvisiblePowObjects)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_INVISIBLE_2"),
            Some(UserToolbarLocalAction::OtherInvisibleObjects)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_LINE_ON"),
            Some(UserToolbarLocalAction::OnOffSwitch)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_CDM16"),
            Some(UserToolbarLocalAction::ConditionalDirectMap16)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_BLOCK_CONTENTS"),
            Some(UserToolbarLocalAction::BlockContents)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_BLOCK_EXITS"),
            Some(UserToolbarLocalAction::BlockExits)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_HAVE_STAR"),
            Some(UserToolbarLocalAction::HaveStar)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_TIME_100"),
            Some(UserToolbarLocalAction::Time100)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_5YOSHI_COINS"),
            Some(UserToolbarLocalAction::FiveYoshiCoins)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_CUSTOM_TRIGGER_A"),
            Some(UserToolbarLocalAction::CustomTrigger(0x0a))
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ONESHOT_TRIGGER_1F"),
            Some(UserToolbarLocalAction::OneShotTrigger(0x1f))
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_MANUAL_TRIGGER_INC_F"),
            Some(UserToolbarLocalAction::ManualTrigger {
                trigger: 0x0f,
                delta: 1,
            })
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_MANUAL_TRIGGER_DEC_0"),
            Some(UserToolbarLocalAction::ManualTrigger {
                trigger: 0,
                delta: -1,
            })
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ONESHOT_TRIGGER_INC"),
            Some(UserToolbarLocalAction::TriggerSelection {
                family: 1,
                delta: 1,
            })
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_MANUAL_TRIGGER_CURRENT_DEC"),
            Some(UserToolbarLocalAction::CurrentTrigger {
                family: 2,
                delta: -1,
            })
        );
        assert_eq!(user_toolbar_local_action("LM_VIEW_CUSTOM_TRIGGER_10"), None);
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ONESHOT_TRIGGER_20"),
            None
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_512HEIGHT_BG"),
            Some(UserToolbarLocalAction::Background512Height)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_TRANSLUCENT"),
            Some(UserToolbarLocalAction::Translucent)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_TOGGLE"),
            Some(UserToolbarLocalAction::ZoomToggle)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_DEFAULT"),
            Some(UserToolbarLocalAction::ZoomDefault)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_PLUS"),
            Some(UserToolbarLocalAction::ZoomPlus)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_ZOOM_MINUS"),
            Some(UserToolbarLocalAction::ZoomMinus)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_TILE_GRID"),
            Some(UserToolbarLocalAction::TileGrid)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SURFACE_OUTLINE"),
            Some(UserToolbarLocalAction::SurfaceOutline)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_LINE_GUIDE_OUTLINE"),
            Some(UserToolbarLocalAction::LineGuideOutline)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SCREEN_GRID"),
            Some(UserToolbarLocalAction::ScreenGrid)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SCREEN_EXITS"),
            Some(UserToolbarLocalAction::ScreenExits)
        );
        assert_eq!(
            user_toolbar_local_action("LM_VIEW_SCREEN_GRID_2"),
            Some(UserToolbarLocalAction::BoundaryGuide)
        );
        for (name, toggle) in [
            (
                "LM_VIEW_ALL_ENTRANCES",
                crate::vanilla_level_editor::EntranceOverlayToggle::All,
            ),
            (
                "LM_VIEW_LEVEL_ENTRANCE",
                crate::vanilla_level_editor::EntranceOverlayToggle::Primary,
            ),
            (
                "LM_VIEW_LEVEL_ENTRANCE_2",
                crate::vanilla_level_editor::EntranceOverlayToggle::Secondary,
            ),
            (
                "LM_VIEW_MIDWAY_POINT",
                crate::vanilla_level_editor::EntranceOverlayToggle::Midway,
            ),
        ] {
            assert_eq!(
                user_toolbar_local_action(name),
                Some(UserToolbarLocalAction::EntranceOverlay(toggle))
            );
        }
    }

    #[test]
    fn original_run_emulator_prefers_configured_snes_profile_over_gba_and_unrelated_tools() {
        let tool = |id: &str, argument: &str| lm_app::ExternalTool {
            id: id.into(),
            name: id.into(),
            executable: "emulator".into(),
            arguments: vec![argument.into()],
            working_directory: None,
            subscriptions: Vec::new(),
        };
        let tools = [
            tool("lunar-magic-gba-emulator", "{rom}"),
            tool("graphics", "{graphics}"),
            tool("snes", "{rom_8dot3}"),
            tool("later", "{rom}"),
        ];
        assert_eq!(
            configured_snes_emulator_tool_id(&tools),
            Some("snes".into())
        );
        assert_eq!(
            configured_snes_emulator_tool_id(&tools[..2]),
            None,
            "GBA and graphics tools cannot receive an SMW test ROM"
        );
    }

    #[test]
    fn every_native_internal_route_belongs_to_the_authenticated_original_table() {
        let supported = lm_app::lunar_magic_363_user_toolbar_commands()
            .filter(|entry| {
                user_toolbar_command(entry.name, Some(0x105)).is_some()
                    || user_toolbar_local_action(entry.name).is_some()
                    || user_toolbar_native_action(entry.name).is_some()
            })
            .collect::<Vec<_>>();
        assert_eq!(supported.len(), 254);
        assert!(
            supported
                .iter()
                .all(|entry| { lm_app::user_toolbar_internal_command(entry.name).is_some() })
        );
    }

    #[test]
    fn general_options_toolbar_route_opens_the_native_resource_041f_dialog() {
        let mut native = NativeApplication::default();
        assert!(!native.undo_history_settings.is_open());
        native.apply_user_toolbar_native_action(
            &egui::Context::default(),
            UserToolbarNativeAction::GeneralOptions,
        );
        assert!(native.undo_history_settings.is_open());
    }

    #[test]
    fn restore_options_toolbar_route_opens_the_native_automatic_policy_workspace() {
        let mut native = NativeApplication::default();
        assert!(!native.restore_point_dialog.automatic_policy_is_open());
        native.apply_user_toolbar_native_action(
            &egui::Context::default(),
            UserToolbarNativeAction::RestoreOptions,
        );
        assert!(native.restore_point_dialog.automatic_policy_is_open());
    }

    #[test]
    fn diagnostic_authenticated_internal_routes_partition_the_complete_original_table() {
        let unsupported = lm_app::lunar_magic_363_user_toolbar_commands()
            .filter(|entry| {
                user_toolbar_command(entry.name, Some(0x105)).is_none()
                    && user_toolbar_local_action(entry.name).is_none()
                    && user_toolbar_native_action(entry.name).is_none()
            })
            .collect::<Vec<_>>();
        assert_eq!(unsupported.len(), 63);
        if std::env::var_os("LM_DIAGNOSTIC_UNSUPPORTED_TOOLBAR_COMMANDS").is_some() {
            for entry in unsupported {
                eprintln!(
                    "{:03}\t{:04X}\t{}",
                    entry.slot, entry.command_id, entry.name
                );
            }
        }
    }

    #[test]
    fn original_path_placeholders_expand_without_a_shell() {
        let mut app = lm_app::AppState::default();
        app.document_path = Some(std::path::PathBuf::from("/tmp/rom dir/game.smc"));
        assert_eq!(
            expand_lm_placeholders("%1|%2|%3|%5", &app).unwrap(),
            "/tmp/rom dir/game.smc|/tmp/rom dir/|game.smc|game"
        );
        assert!(expand_lm_placeholders("%9", &app).is_err());
        app.document_path = None;
        assert!(expand_lm_placeholders("%1", &app).is_err());
    }

    #[test]
    fn original_user_shortcut_tokens_cover_modifiers_named_and_numeric_keys() {
        assert_eq!(
            user_toolbar_shortcut(&["'o'".into(), "VK_CONTROL".into(), "VK_SHIFT".into()]),
            Some(ShortcutGesture {
                modifiers: ShortcutModifiers::SECONDARY.union(ShortcutModifiers::SHIFT),
                key: ShortcutKey::Character('o'),
            })
        );
        assert_eq!(
            parse_user_toolbar_key("VK_F24"),
            Some(ShortcutKey::Function(24))
        );
        assert_eq!(
            parse_user_toolbar_key("VK_PAGEUP"),
            Some(ShortcutKey::PageUp)
        );
        assert_eq!(parse_user_toolbar_key("0x2E"), Some(ShortcutKey::Delete));
        assert_eq!(
            parse_user_toolbar_key("0x41"),
            Some(ShortcutKey::Character('a'))
        );
        for (token, numeric, key) in [
            ("VK_LBUTTON", "0x01", ShortcutKey::MouseLeft),
            ("VK_RBUTTON", "0x02", ShortcutKey::MouseRight),
            ("VK_MBUTTON", "0x04", ShortcutKey::MouseMiddle),
            ("VK_XBUTTON1", "0x05", ShortcutKey::MouseExtra1),
            ("VK_XBUTTON2", "0x06", ShortcutKey::MouseExtra2),
        ] {
            assert_eq!(parse_user_toolbar_key(token), Some(key));
            assert_eq!(parse_user_toolbar_key(numeric), Some(key));
        }
        for (token, numeric, key) in [
            ("VK_PAUSE", "0x13", ShortcutKey::Pause),
            ("VK_MULTIPLY", "0x6A", ShortcutKey::NumpadMultiply),
            ("VK_ADD", "0x6B", ShortcutKey::NumpadAdd),
            ("VK_SEPARATOR", "0x6C", ShortcutKey::NumpadSeparator),
            ("VK_SUBTRACT", "0x6D", ShortcutKey::NumpadSubtract),
            ("VK_DECIMAL", "0x6E", ShortcutKey::NumpadDecimal),
            ("VK_DIVIDE", "0x6F", ShortcutKey::NumpadDivide),
        ] {
            assert_eq!(parse_user_toolbar_key(token), Some(key));
            assert_eq!(parse_user_toolbar_key(numeric), Some(key));
        }
        assert!(user_toolbar_shortcut(&["'a'".into(), "'b'".into()]).is_none());
    }

    #[test]
    fn hidden_toolbar_shortcuts_match_and_duplicate_assignments_all_survive() {
        let mut toolbar = lm_app::UserToolbar::parse(include_str!(
            "../../../../docs/oracle-work/lm363/user-toolbar/usertoolbar.txt"
        ))
        .unwrap();
        assert!(!toolbar.toolbar_visible());
        toolbar.buttons.push(toolbar.buttons[1].clone());
        let gesture = user_toolbar_shortcut(&toolbar.buttons[1].shortcut).unwrap();
        let matches = matching_user_toolbar_buttons(&toolbar, &[gesture]);
        assert_eq!(
            matches.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [1, 3]
        );
    }

    #[test]
    fn external_working_directory_matches_original_defaults_and_rom_override() {
        let app = lm_app::AppState::default();
        let program =
            lm_app::UserToolbar::parse("***START***\n\"/opt/tools/editor\" --flag\n***END***")
                .unwrap();
        assert_eq!(
            external_working_directory("/opt/tools/editor", &program.buttons[0], &app).unwrap(),
            Some(std::path::PathBuf::from("/opt/tools"))
        );
        let mut app = lm_app::AppState::default();
        app.document_path = Some(std::path::PathBuf::from("/tmp/roms/game.smc"));
        let rom = lm_app::UserToolbar::parse(
            "***START***\n\"editor\"\nLM_DEFAULT\nLM_DIR_ROM\n***END***",
        )
        .unwrap();
        assert_eq!(
            external_working_directory("editor", &rom.buttons[0], &app).unwrap(),
            Some(std::path::PathBuf::from("/tmp/roms"))
        );
    }

    #[test]
    fn lifecycle_option_selection_is_exact_and_global_no_autorun_is_retained() {
        let toolbar = lm_app::UserToolbar::parse(
            "LM_NO_AUTORUN\n***START***\n\"one\"\nLM_DEFAULT\nLM_AUTORUN_ON_NEW_ROM,LM_CLOSE_ON_CLOSE\n***START***\n\"two\"\nLM_DEFAULT\nLM_CLOSE_ON_NEW_ROM\n***END***",
        )
        .unwrap();
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_AUTORUN_ON_NEW_ROM"),
            [0]
        );
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_CLOSE_ON_CLOSE"),
            [0]
        );
        assert_eq!(
            toolbar_button_indexes_with_option(&toolbar, "LM_CLOSE_ON_NEW_ROM"),
            [1]
        );
        assert!(toolbar.global_options.iter().any(|option| {
            matches!(option, lm_app::UserToolbarGlobalOption::Flag(value) if value == "LM_NO_AUTORUN")
        }));
        let forced = lm_app::UserToolbar::parse(
            "LM_CLOSE_ON_CLOSE_FORCE_ALL\n***START***\n\"one\"\n***START***\n\"two\"\n***END***",
        )
        .unwrap();
        assert_eq!(
            toolbar_lifecycle_indexes(&forced, "LM_CLOSE_ON_CLOSE", "LM_CLOSE_ON_CLOSE_FORCE_ALL"),
            [0, 1]
        );
    }

    #[test]
    fn process_launch_options_honor_per_button_and_force_all_contracts() {
        let toolbar = lm_app::UserToolbar::parse(
            "LM_ALLOW_MULT_INSTANCES_FORCE_ALL\n***START***\n\"one\"\nLM_DEFAULT\nLM_NO_CONSOLE_WINDOW\n***START***\n\"two\"\n***END***",
        )
        .unwrap();
        assert_eq!(
            user_toolbar_launch_options(Some(&toolbar), &toolbar.buttons[0]),
            crate::external_tool_launcher::LaunchOptions {
                allow_multiple_instances: true,
                hide_console_window: true,
                open_other: false,
            }
        );
        assert_eq!(
            user_toolbar_launch_options(Some(&toolbar), &toolbar.buttons[1]),
            crate::external_tool_launcher::LaunchOptions {
                allow_multiple_instances: true,
                hide_console_window: false,
                open_other: false,
            }
        );
        let individual = lm_app::UserToolbar::parse(
            "***START***\n\"one\"\nLM_DEFAULT\nLM_ALLOW_MULT_INSTANCES,LM_OPEN_OTHER\n***END***",
        )
        .unwrap();
        assert_eq!(
            user_toolbar_launch_options(Some(&individual), &individual.buttons[0]),
            crate::external_tool_launcher::LaunchOptions {
                allow_multiple_instances: true,
                hide_console_window: false,
                open_other: true,
            }
        );
    }

    #[test]
    fn notification_selection_is_external_only_exact_and_honors_documented_force_all() {
        let toolbar = lm_app::UserToolbar::parse(
            "LM_NOTIFY_ON_NEW_LEVEL_FORCE_ALL\n***START***\n\"one\"\nLM_DEFAULT\nLM_NOTIFY_ON_NEW_ROM,LM_NOTIFY_ON_DELETE_LEVEL\n***START***\nLM_FILE_SAVE\nLM_DEFAULT\nLM_NOTIFY_ON_NEW_ROM,LM_NOTIFY_ON_DELETE_LEVEL\n***START***\n\"two\"\n***END***",
        )
        .unwrap();
        assert_eq!(
            toolbar_notification_tool_ids(
                &toolbar,
                "LM_NOTIFY_ON_NEW_ROM",
                Some("LM_NOTIFY_ON_NEW_ROM_FORCE_ALL")
            ),
            ["usertoolbar-0"]
        );
        assert_eq!(
            toolbar_notification_tool_ids(
                &toolbar,
                "LM_NOTIFY_ON_NEW_LEVEL",
                Some("LM_NOTIFY_ON_NEW_LEVEL_FORCE_ALL")
            ),
            ["usertoolbar-0", "usertoolbar-2"]
        );
        assert!(
            toolbar_notification_tool_ids(&toolbar, "LM_NOTIFY_ON_SAVE_LEVEL", None).is_empty()
        );
        assert_eq!(
            toolbar_notification_tool_ids(&toolbar, "LM_NOTIFY_ON_DELETE_LEVEL", None),
            ["usertoolbar-0"]
        );
    }

    #[test]
    fn successful_save_publication_coalesces_each_dirty_domain_once() {
        let mut native = NativeApplication::default();
        native.user_toolbar_observed_level = Some(0x105);
        native.mark_user_toolbar_save_notification(LunarMagicNotificationKind::SaveLevel);
        native.mark_user_toolbar_save_notification(LunarMagicNotificationKind::SaveMap16);
        native.mark_user_toolbar_save_notification(LunarMagicNotificationKind::SaveOverworld);
        native.mark_user_toolbar_save_notification(LunarMagicNotificationKind::SaveLevel);
        assert_eq!(native.user_toolbar_pending_save_notifications, 0b111);
        native.publish_user_toolbar_save_notifications();
        assert_eq!(native.user_toolbar_pending_save_notifications, 0);
    }

    #[test]
    fn successful_save_publication_coalesces_each_deleted_level_once() {
        let mut native = NativeApplication::default();
        native.mark_user_toolbar_level_deleted(0x105);
        native.mark_user_toolbar_level_deleted(0x106);
        native.mark_user_toolbar_level_deleted(0x105);
        assert_eq!(native.user_toolbar_pending_deleted_levels, [0x105, 0x106]);
        native.publish_user_toolbar_level_deleted_notifications();
        assert!(native.user_toolbar_pending_deleted_levels.is_empty());
    }

    #[test]
    fn new_document_transition_enqueues_autorun_once_through_permission_gate() {
        let mut native = NativeApplication::default();
        native.user_toolbar = Some(
            lm_app::UserToolbar::parse(
                "***START***\n\"/usr/bin/true\"\nLM_DEFAULT\nLM_AUTORUN_ON_NEW_ROM\n***END***",
            )
            .unwrap(),
        );
        native.app.document_path = Some(std::path::PathBuf::from("/tmp/game.smc"));
        let context = egui::Context::default();
        native.handle_user_toolbar_document_change(&context);
        assert_eq!(
            native.effects.external_tools.pending_tool_ids(),
            ["usertoolbar-0"]
        );
        native.handle_user_toolbar_document_change(&context);
        assert_eq!(
            native.effects.external_tools.pending_tool_ids(),
            ["usertoolbar-0"]
        );
    }

    #[test]
    fn local_view_actions_toggle_the_same_state_consumed_by_level_rendering() {
        let mut visibility = LevelViewVisibility::default();
        let mut special_world = false;
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::Layer1,
        );
        assert!(!visibility.layer1);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::SpecialWorld,
        );
        assert!(special_world);
        assert!(!visibility.tile_grid);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::TileGrid,
        );
        assert!(visibility.tile_grid);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::SurfaceOutline,
        );
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::LineGuideOutline,
        );
        assert!(visibility.surface_outline);
        assert!(visibility.line_guide_outline);
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::None);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::ScreenGrid,
        );
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::ScreenGrid);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::ScreenExits,
        );
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::ScreenExits);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::BoundaryGuide,
        );
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::BoundaryGuide);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::ScreenGrid,
        );
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::ScreenGrid);
        toggle_user_toolbar_view_state(
            &mut visibility,
            &mut special_world,
            UserToolbarLocalAction::ScreenGrid,
        );
        assert_eq!(visibility.screen_overlay, LevelScreenOverlay::None);
        let mut native = NativeApplication::default();
        native.apply_user_toolbar_local_action(UserToolbarLocalAction::Sprites);
        assert!(native.level_view_visibility.sprites);
        assert!(native.effects.error.is_some());
    }
}
