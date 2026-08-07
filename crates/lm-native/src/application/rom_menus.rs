use super::NativeApplication;
use crate::rom_overworld_editor::RomOverworldEditor;
use eframe::egui;
use lm_app::{EditorMode, UiTextKey};

macro_rules! rom_editor_pair {
    ($application:expr, $ui:expr, $enabled:expr, $editor:expr, $app:expr, $name:expr) => {{
        let editor = $application.menu_text($name);
        let open = $application
            .menu_text(UiTextKey::EditorEditFormat)
            .replace("{editor}", &editor);
        let close = $application
            .menu_text(UiTextKey::EditorCloseFormat)
            .replace("{editor}", &editor);
        if $ui
            .add_enabled($enabled && !$editor.is_open(), egui::Button::new(open))
            .clicked()
        {
            $ui.close_menu();
            $editor.open($app);
        }
        if $ui
            .add_enabled($editor.is_open(), egui::Button::new(close))
            .clicked()
        {
            $ui.close_menu();
            $editor.request_close(false);
        }
    }};
}

impl NativeApplication {
    pub(super) fn rom_editor_menu_items(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let profile = self.app.revision_profile().is_some();
        let level = enabled && profile && matches!(self.app.mode, EditorMode::Level(_));
        let settings = level
            && self
                .app
                .revision_profile()
                .is_some_and(|profile| profile.expanded_settings.is_some());

        self.global_rom_editor_menu_items(ui, enabled);
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            settings,
            self.rom_expanded_settings_editor,
            &self.app,
            UiTextKey::EditorInstalledExpandedSettings
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            level,
            self.rom_level_assets_editor,
            &self.app,
            UiTextKey::EditorNativeLevelAssets
        );
        let batch_export = enabled
            && (profile || crate::vanilla_level_editor::VanillaLevelEditor::handles(&self.app))
            && !self.rom_mwl_batch_export_dialog.is_open();
        if ui
            .add_enabled(
                batch_export,
                egui::Button::new(self.menu_text(UiTextKey::EditorExportAllMwl)),
            )
            .clicked()
        {
            ui.close_menu();
            self.rom_mwl_batch_export_dialog
                .open(&self.app, lm_app::MwlBatchExportMode::All);
        }
        if ui
            .add_enabled(
                batch_export,
                egui::Button::new(self.menu_text(UiTextKey::EditorExportModifiedMwl)),
            )
            .clicked()
        {
            ui.close_menu();
            self.rom_mwl_batch_export_dialog
                .open(&self.app, lm_app::MwlBatchExportMode::Modified);
        }
        if ui
            .add_enabled(
                enabled && profile && !self.rom_mwl_batch_import_dialog.is_open(),
                egui::Button::new(self.menu_text(UiTextKey::EditorInsertMultipleMwl)),
            )
            .clicked()
        {
            ui.close_menu();
            self.rom_mwl_batch_import_dialog.open(&self.app);
        }
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled && matches!(self.app.mode, EditorMode::Map16),
            self.rom_map16_editor,
            &self.app,
            UiTextKey::EditorNativeMap16Set
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::Palette(_)),
            self.rom_palette_editor,
            &self.app,
            UiTextKey::EditorNativePalette
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::Graphics(_)),
            self.rom_graphics_editor,
            &self.app,
            UiTextKey::EditorNativeGraphics
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::ExAnimation(_)),
            self.rom_exanimation_editor,
            &self.app,
            UiTextKey::EditorNativeExAnimation
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled
                && matches!(self.app.mode, EditorMode::Overworld)
                && (profile || RomOverworldEditor::handles(&self.app)),
            self.rom_overworld_editor,
            &self.app,
            UiTextKey::EditorNativeOverworld
        );
    }

    fn global_rom_editor_menu_items(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_lunar_magic_metadata_editor,
            &self.app,
            UiTextKey::EditorLunarMagicRomMetadata
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_shared_palette_editor,
            &self.app,
            UiTextKey::EditorSharedCustomSmwPalettes
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_secondary_exit_editor,
            &self.app,
            UiTextKey::EditorGlobalSecondaryExits
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_title_recording_editor,
            &self.app,
            UiTextKey::EditorTitleScreenRecording
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_title_tilemap_editor,
            &self.app,
            UiTextKey::EditorTitleScreenTilemap
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_credits_tilemap_editor,
            &self.app,
            UiTextKey::EditorCreditsTilemap
        );
        self.overworld_support_rom_editor_menu_items(ui, enabled);
    }

    fn overworld_support_rom_editor_menu_items(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_player_start_editor,
            &self.app,
            UiTextKey::EditorOverworldPlayerStarts
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_settings_editor,
            &self.app,
            UiTextKey::EditorOverworldGlobalSettings
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_event_number_editor,
            &self.app,
            UiTextKey::EditorOverworldEventNumberMap
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_event_reveal_editor,
            &self.app,
            UiTextKey::EditorOverworldEventReveals
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_event_tilemap_editor,
            &self.app,
            UiTextKey::EditorOverworldEventTilemaps
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_level_name_editor,
            &self.app,
            UiTextKey::EditorOverworldLevelNames
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_boss_sequence_editor,
            &self.app,
            UiTextKey::EditorBossSequenceMessages
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_message_editor,
            &self.app,
            UiTextKey::EditorOverworldMessages
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_path_link_editor,
            &self.app,
            UiTextKey::EditorOverworldPathLinks
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_warp_link_editor,
            &self.app,
            UiTextKey::EditorOverworldWarpLinks
        );
        ui.separator();
        rom_editor_pair!(
            self,
            ui,
            enabled,
            self.rom_overworld_special_event_editor,
            &self.app,
            UiTextKey::EditorOverworldSpecialEvents
        );
    }
}
