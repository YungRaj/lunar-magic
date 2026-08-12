use super::NativeApplication;
use crate::effects::Confirmation;
use crate::level_access_restriction_dialog::LevelAccessRestrictionAction;
use eframe::egui;

macro_rules! show_rom_editor {
    ($self:expr, $context:expr, $editor:ident) => {{
        let (quit, command) = $self.$editor.show($context, $self.app.project_revision());
        if let Some(command) = command {
            if $self.try_dispatch($context, command) {
                $self.$editor.commit_succeeded();
                $self.renderer.invalidate();
            }
        }
        if quit {
            $self.request_quit($context);
        }
    }};
    ($self:expr, $context:expr, $editor:ident, $notification:expr) => {{
        let (quit, command) = $self.$editor.show($context, $self.app.project_revision());
        if let Some(command) = command {
            if $self.try_dispatch($context, command) {
                $self.$editor.commit_succeeded();
                $self.mark_user_toolbar_save_notification($notification);
                $self.renderer.invalidate();
            }
        }
        if quit {
            $self.request_quit($context);
        }
    }};
}

macro_rules! show_project_operation {
    ($self:expr, $context:expr, $dialog:ident) => {{
        if let Some(command) = $self.$dialog.show($context, &$self.app) {
            if $self.try_dispatch($context, command) {
                $self.$dialog.commit_succeeded();
                $self.renderer.invalidate();
            }
        }
    }};
}

impl NativeApplication {
    pub(super) fn show_rom_editors(&mut self, context: &egui::Context) {
        show_rom_editor!(self, context, rom_expanded_settings_editor);
        show_rom_editor!(self, context, rom_legacy_fg_bg_bypass_editor);
        show_rom_editor!(self, context, rom_legacy_sprite_bypass_editor);
        show_rom_editor!(self, context, rom_lunar_magic_metadata_editor);
        show_rom_editor!(self, context, rom_shared_palette_editor);
        show_rom_editor!(self, context, rom_boss_sequence_editor);
        show_rom_editor!(
            self,
            context,
            rom_overworld_message_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        show_rom_editor!(
            self,
            context,
            rom_overworld_path_link_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        show_rom_editor!(
            self,
            context,
            rom_overworld_warp_link_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        let (quit, command) = self.rom_secondary_exit_editor.show(
            context,
            self.app.project_revision(),
            self.app.localization(),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_secondary_exit_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(lm_app::LunarMagicNotificationKind::SaveLevel);
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(self, context, rom_title_recording_editor);
        for (quit, command, title) in [
            {
                let (quit, command) = self.rom_title_tilemap_editor.show(
                    context,
                    self.app.project_revision(),
                    self.app.localization(),
                );
                (quit, command, true)
            },
            {
                let (quit, command) = self.rom_credits_tilemap_editor.show(
                    context,
                    self.app.project_revision(),
                    self.app.localization(),
                );
                (quit, command, false)
            },
        ] {
            if let Some(command) = command
                && self.try_dispatch(context, command)
            {
                if title {
                    self.rom_title_tilemap_editor.commit_succeeded();
                } else {
                    self.rom_credits_tilemap_editor.commit_succeeded();
                }
                self.renderer.invalidate();
            }
            if quit {
                self.request_quit(context);
            }
        }
        let (quit, command) = self.rom_overworld_player_start_editor.show(
            context,
            self.app.project_revision(),
            self.app.localization(),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_overworld_player_start_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(
                lm_app::LunarMagicNotificationKind::SaveOverworld,
            );
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(
            self,
            context,
            rom_overworld_settings_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        let (quit, command) = self.rom_overworld_event_number_editor.show(
            context,
            self.app.project_revision(),
            self.app.localization(),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_overworld_event_number_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(
                lm_app::LunarMagicNotificationKind::SaveOverworld,
            );
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(
            self,
            context,
            rom_overworld_event_reveal_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        show_rom_editor!(
            self,
            context,
            rom_overworld_event_tilemap_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        let (quit, command) = self.rom_overworld_level_name_editor.show(
            context,
            self.app.project_revision(),
            self.app.localization(),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_overworld_level_name_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(
                lm_app::LunarMagicNotificationKind::SaveOverworld,
            );
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(
            self,
            context,
            rom_overworld_special_event_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
        let (quit, command) = self.rom_level_assets_editor.show(
            context,
            self.app.project_revision(),
            self.special_world_passed,
            self.level_view_visibility,
            self.animation_rate,
            self.allow_control_wheel_zoom.unwrap_or(true),
        );
        let command = command.and_then(|command| {
            let result = self
                .app
                .controller_snapshot()
                .map_err(|error| error.to_string())
                .and_then(|snapshot| {
                    crate::menu_color_fix::prepare_level_save_command(
                        &snapshot,
                        self.menu_color_fix.unwrap_or(true),
                        command,
                    )
                });
            match result {
                Ok(command) => Some(command),
                Err(error) => {
                    self.effects.error = Some(error);
                    None
                }
            }
        });
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_level_assets_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(lm_app::LunarMagicNotificationKind::SaveLevel);
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        let active_sidecar = self.native_map16_sidecar_editor.value().cloned();
        let map16_selection_generation = self.rom_map16_editor.selection_generation();
        let (quit, command) = self.rom_map16_editor.show(
            context,
            self.app.project_revision(),
            active_sidecar.as_ref(),
        );
        if self.auto_deselect_on_editor_select
            && self.rom_map16_editor.selection_generation() != map16_selection_generation
        {
            self.vanilla_level_editor.editor_selector_selected();
        }
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_map16_editor.commit_succeeded();
            self.mark_user_toolbar_save_notification(lm_app::LunarMagicNotificationKind::SaveMap16);
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(self, context, rom_palette_editor);
        let (quit, command) = self.rom_graphics_editor.show(
            context,
            &self.app,
            self.special_world_passed,
            &mut self.joined_graphics_files,
            self.convert_berry_gfx_tile.unwrap_or(true),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_graphics_editor.commit_succeeded();
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        show_rom_editor!(self, context, rom_exanimation_editor);
        show_rom_editor!(
            self,
            context,
            rom_overworld_editor,
            lm_app::LunarMagicNotificationKind::SaveOverworld
        );
    }

    pub(super) fn show_project_operations(&mut self, context: &egui::Context) {
        self.toolbar_graphics_transfer.show(context);
        if let Some(command) = self
            .legacy_graphics_bypass_transfer
            .show(context, &self.app)
            && self.try_dispatch(context, command)
        {
            self.renderer.invalidate();
        }
        if let Some(command) = self.current_level_palette_transfer.show(context, &self.app)
            && self.try_dispatch(context, command)
        {
            self.mark_user_toolbar_save_notification(lm_app::LunarMagicNotificationKind::SaveLevel);
            self.renderer.invalidate();
        }
        self.level_usage_dialog
            .show(context, self.app.localization());
        self.rom_user_area_scan_dialog
            .show(context, self.app.localization());
        self.ips_create_dialog.show(context);
        self.restore_point_dialog.show(context, &self.app);
        self.rom_mwl_batch_export_dialog.show(context);
        if let Some(command) = self.rom_mwl_batch_import_dialog.show(context, &self.app) {
            if self.try_dispatch(context, command) {
                self.rom_mwl_batch_import_dialog.commit_succeeded();
                self.renderer.invalidate();
            } else {
                self.rom_mwl_batch_import_dialog.commit_failed();
            }
        }
        if let Some(command) = self.rom_mwl_import_dialog.show(context, &self.app) {
            if self.try_dispatch(context, command) {
                if let Some(level) = self.rom_mwl_import_dialog.commit_succeeded() {
                    let _ = self.try_dispatch(context, lm_app::Command::SelectLevel(level));
                }
                self.renderer.invalidate();
            } else {
                self.rom_mwl_import_dialog.commit_failed();
            }
        }
        show_project_operation!(self, context, rom_expansion_dialog);
        let ips_workflow_active = self.ips_create_dialog.has_open_workflow();
        if let Some(action) = self.level_access_restriction_dialog.show(
            context,
            &self.app,
            ips_workflow_active,
            self.app.localization(),
        ) {
            match action {
                LevelAccessRestrictionAction::Restrict(command) => {
                    if self.try_dispatch(context, command) {
                        let create_restore_point =
                            self.restore_point_dialog.destructive_full_enabled();
                        self.level_access_restriction_dialog
                            .commit_succeeded(create_restore_point);
                        self.renderer.invalidate();
                        let _accepted = self.try_dispatch(context, lm_app::Command::Save);
                    }
                }
                LevelAccessRestrictionAction::CreateRestorePoint => {
                    self.create_restriction_restore_point();
                }
                LevelAccessRestrictionAction::PersistRestrictedRom => {
                    let _accepted = self.try_dispatch(context, lm_app::Command::Save);
                }
                LevelAccessRestrictionAction::CreateIps => {
                    match self.ips_create_dialog.choose_and_start() {
                        Ok(started) => self
                            .level_access_restriction_dialog
                            .ips_choice_completed(started),
                        Err(error) => self.level_access_restriction_dialog.workflow_failed(error),
                    }
                }
                LevelAccessRestrictionAction::SaveAndClose => {
                    self.effects.save_before_confirmation_action(
                        &mut self.app,
                        context,
                        Confirmation::DiscardAndClose { quit_after: false },
                    );
                }
            }
        }
        show_project_operation!(self, context, graphics_migration_dialog);
        self.vram_patch_options_dialog.show(context);
        if let Some(selection) = self.vram_patch_options_dialog.take_pending() {
            self.pending_vram_patch_selection = Some(selection);
            self.vram_patch_selection_initialized = true;
            self.app.status = match selection {
                crate::vram_patch_options_dialog::VramPatchSelection::None => {
                    "VRAM patch will remain uninstalled on the next level save."
                }
                crate::vram_patch_options_dialog::VramPatchSelection::Normal => {
                    "Normal VRAM patch will be applied on the next level save."
                }
                crate::vram_patch_options_dialog::VramPatchSelection::Hd16x9
                | crate::vram_patch_options_dialog::VramPatchSelection::Hd21x9 => {
                    "The installed HD VRAM patch selection is retained."
                }
            }
            .into();
        }
        show_project_operation!(self, context, rats_reclamation_dialog);
        show_project_operation!(self, context, ips_patch_dialog);
        show_project_operation!(self, context, copier_header_dialog);
        if let Some(command) = self
            .built_in_runtime_installer
            .show(context, self.app.project_revision())
            && self.try_dispatch(context, command)
        {
            let open_expanded_settings = self.built_in_runtime_installer.commit_succeeded();
            if open_expanded_settings
                && let Err(error) = self.rom_expanded_settings_editor.open_detected(&self.app)
            {
                self.effects.error = Some(error);
            }
            if let Some(domain) = self
                .built_in_runtime_installer
                .take_legacy_bypass_continuation()
            {
                let editor = match domain {
                    crate::rom_legacy_graphics_bypass_editor::LegacyGraphicsBypassDomain::ForegroundBackground => {
                        &mut self.rom_legacy_fg_bg_bypass_editor
                    }
                    crate::rom_legacy_graphics_bypass_editor::LegacyGraphicsBypassDomain::Sprites => {
                        &mut self.rom_legacy_sprite_bypass_editor
                    }
                };
                if let Err(error) = editor.open_domain(&self.app, domain) {
                    self.effects.error = Some(error);
                }
            }
            self.renderer.invalidate();
        }
        if let Some(command) = self
            .revision_patch_installer
            .show(context, self.app.project_revision())
            && self.try_dispatch(context, command)
        {
            self.revision_patch_installer.commit_succeeded();
            self.renderer.invalidate();
        }
        self.persist_recent_state();
    }

    fn create_restriction_restore_point(&mut self) {
        match crate::restore_point_dialog::create_or_append_associated_full_for_open_project(
            &self.app,
        ) {
            Ok(true) => {
                self.level_access_restriction_dialog
                    .restore_point_completed();
            }
            Ok(false) => {}
            Err(error) => self.level_access_restriction_dialog.workflow_failed(error),
        }
    }
}
