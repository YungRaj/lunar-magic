use super::NativeApplication;
use crate::rom_overworld_editor::RomOverworldEditor;
use eframe::egui;
use lm_app::EditorMode;

macro_rules! rom_editor_pair {
    ($ui:expr, $enabled:expr, $editor:expr, $app:expr, $open:literal, $close:literal) => {{
        if $ui
            .add_enabled($enabled && !$editor.is_open(), egui::Button::new($open))
            .clicked()
        {
            $ui.close_menu();
            $editor.open($app);
        }
        if $ui
            .add_enabled($editor.is_open(), egui::Button::new($close))
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
            ui,
            settings,
            self.rom_expanded_settings_editor,
            &self.app,
            "Edit Installed Expanded Settings…",
            "Close Installed Expanded Settings"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            level,
            self.rom_level_assets_editor,
            &self.app,
            "Edit Native Level Assets…",
            "Close Native Level Assets"
        );
        if ui
            .add_enabled(
                enabled && profile && !self.rom_mwl_batch_import_dialog.is_open(),
                egui::Button::new("Insert Multiple MWL Levels…"),
            )
            .clicked()
        {
            ui.close_menu();
            self.rom_mwl_batch_import_dialog.open(&self.app);
        }
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled && matches!(self.app.mode, EditorMode::Map16),
            self.rom_map16_editor,
            &self.app,
            "Edit Native Map16 Set…",
            "Close Native Map16 Set"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::Palette(_)),
            self.rom_palette_editor,
            &self.app,
            "Edit Native Palette…",
            "Close Native Palette"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::Graphics(_)),
            self.rom_graphics_editor,
            &self.app,
            "Edit Native Graphics…",
            "Close Native Graphics"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled && profile && matches!(self.app.mode, EditorMode::ExAnimation(_)),
            self.rom_exanimation_editor,
            &self.app,
            "Edit Native ExAnimation…",
            "Close Native ExAnimation"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled
                && matches!(self.app.mode, EditorMode::Overworld)
                && (profile || RomOverworldEditor::handles(&self.app)),
            self.rom_overworld_editor,
            &self.app,
            "Edit Native Overworld…",
            "Close Native Overworld"
        );
    }

    fn global_rom_editor_menu_items(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_lunar_magic_metadata_editor,
            &self.app,
            "Edit Lunar Magic ROM Metadata…",
            "Close Lunar Magic ROM Metadata"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_shared_palette_editor,
            &self.app,
            "Edit Shared/Custom SMW Palettes…",
            "Close Shared/Custom SMW Palettes"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_secondary_exit_editor,
            &self.app,
            "Edit Global Secondary Exits…",
            "Close Global Secondary Exits"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_title_recording_editor,
            &self.app,
            "Edit Title-Screen Recording…",
            "Close Title-Screen Recording"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_title_tilemap_editor,
            &self.app,
            "Edit Title-Screen Tilemap…",
            "Close Title-Screen Tilemap"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_credits_tilemap_editor,
            &self.app,
            "Edit Credits Tilemap…",
            "Close Credits Tilemap"
        );
        self.overworld_support_rom_editor_menu_items(ui, enabled);
    }

    fn overworld_support_rom_editor_menu_items(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_player_start_editor,
            &self.app,
            "Edit Overworld Player Starts…",
            "Close Overworld Player Starts"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_settings_editor,
            &self.app,
            "Edit Overworld Global Settings…",
            "Close Overworld Global Settings"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_event_number_editor,
            &self.app,
            "Edit Overworld Event-Number Map…",
            "Close Overworld Event-Number Map"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_event_reveal_editor,
            &self.app,
            "Edit Overworld Event Reveals…",
            "Close Overworld Event Reveals"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_event_tilemap_editor,
            &self.app,
            "Edit Overworld Event Tilemaps…",
            "Close Overworld Event Tilemaps"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_level_name_editor,
            &self.app,
            "Edit Overworld Level Names…",
            "Close Overworld Level Names"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_boss_sequence_editor,
            &self.app,
            "Edit Boss-Sequence Messages…",
            "Close Boss-Sequence Messages"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_message_editor,
            &self.app,
            "Edit Overworld Messages…",
            "Close Overworld Messages"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_path_link_editor,
            &self.app,
            "Edit Overworld Path Links…",
            "Close Overworld Path Links"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_warp_link_editor,
            &self.app,
            "Edit Overworld Warp Links…",
            "Close Overworld Warp Links"
        );
        ui.separator();
        rom_editor_pair!(
            ui,
            enabled,
            self.rom_overworld_special_event_editor,
            &self.app,
            "Edit Overworld Special Events…",
            "Close Overworld Special Events"
        );
    }
}
