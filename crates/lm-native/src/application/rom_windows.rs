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
        show_rom_editor!(self, context, rom_lunar_magic_metadata_editor);
        show_rom_editor!(self, context, rom_shared_palette_editor);
        show_rom_editor!(self, context, rom_boss_sequence_editor);
        show_rom_editor!(self, context, rom_overworld_message_editor);
        show_rom_editor!(self, context, rom_overworld_path_link_editor);
        show_rom_editor!(self, context, rom_overworld_warp_link_editor);
        show_rom_editor!(self, context, rom_secondary_exit_editor);
        show_rom_editor!(self, context, rom_title_recording_editor);
        show_rom_editor!(self, context, rom_title_tilemap_editor);
        show_rom_editor!(self, context, rom_credits_tilemap_editor);
        show_rom_editor!(self, context, rom_overworld_player_start_editor);
        show_rom_editor!(self, context, rom_overworld_settings_editor);
        show_rom_editor!(self, context, rom_overworld_event_number_editor);
        show_rom_editor!(self, context, rom_overworld_event_reveal_editor);
        show_rom_editor!(self, context, rom_overworld_event_tilemap_editor);
        show_rom_editor!(self, context, rom_overworld_level_name_editor);
        show_rom_editor!(self, context, rom_overworld_special_event_editor);
        let (quit, command) = self.rom_level_assets_editor.show(
            context,
            self.app.project_revision(),
            self.special_world_passed,
            self.level_view_visibility,
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_level_assets_editor.commit_succeeded();
            self.renderer.invalidate();
        }
        if quit {
            self.request_quit(context);
        }
        let active_sidecar = self.native_map16_sidecar_editor.value().cloned();
        let (quit, command) = self.rom_map16_editor.show(
            context,
            self.app.project_revision(),
            active_sidecar.as_ref(),
        );
        if let Some(command) = command
            && self.try_dispatch(context, command)
        {
            self.rom_map16_editor.commit_succeeded();
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
        show_rom_editor!(self, context, rom_overworld_editor);
    }

    pub(super) fn show_project_operations(&mut self, context: &egui::Context) {
        self.toolbar_graphics_transfer.show(context);
        self.level_usage_dialog.show(context);
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
        show_project_operation!(self, context, rom_expansion_dialog);
        let ips_workflow_active = self.ips_create_dialog.has_open_workflow();
        if let Some(action) =
            self.level_access_restriction_dialog
                .show(context, &self.app, ips_workflow_active)
        {
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
        show_project_operation!(self, context, rats_reclamation_dialog);
        show_project_operation!(self, context, ips_patch_dialog);
        show_project_operation!(self, context, copier_header_dialog);
        if let Some(command) = self
            .built_in_runtime_installer
            .show(context, self.app.project_revision())
            && self.try_dispatch(context, command)
        {
            self.built_in_runtime_installer.commit_succeeded();
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
