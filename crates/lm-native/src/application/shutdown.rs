use super::NativeApplication;
use eframe::egui;
use lm_app::Command;

macro_rules! close_or_pause {
    ($self:expr, $editor:ident) => {
        if !$self.$editor.request_close(true) {
            return;
        }
    };
}

impl NativeApplication {
    pub(super) fn request_quit(&mut self, context: &egui::Context) {
        close_or_pause!(self, palette_editor);
        close_or_pause!(self, graphics_editor);
        close_or_pause!(self, map16_editor);
        close_or_pause!(self, exanimation_editor);
        close_or_pause!(self, level_editor);
        close_or_pause!(self, overworld_editor);
        close_or_pause!(self, path_editor);
        close_or_pause!(self, metadata_editor);
        close_or_pause!(self, appearance_editor);
        close_or_pause!(self, overworld_appearance_editor);
        close_or_pause!(self, layer3_editor);
        close_or_pause!(self, mwl_editor);
        close_or_pause!(self, expanded_settings_editor);
        close_or_pause!(self, custom_object_editor);
        close_or_pause!(self, custom_sprite_editor);
        close_or_pause!(self, native_map16_sidecar_editor);
        close_or_pause!(self, dsc_sidecar_editor);
        close_or_pause!(self, map16_set_editor);
        close_or_pause!(self, native_level_document_editor);
        close_or_pause!(self, native_level_assets_editor);
        close_or_pause!(self, rom_expanded_settings_editor);
        close_or_pause!(self, rom_lunar_magic_metadata_editor);
        close_or_pause!(self, rom_boss_sequence_editor);
        close_or_pause!(self, rom_overworld_message_editor);
        close_or_pause!(self, rom_overworld_path_link_editor);
        close_or_pause!(self, rom_overworld_warp_link_editor);
        close_or_pause!(self, rom_secondary_exit_editor);
        close_or_pause!(self, rom_title_recording_editor);
        close_or_pause!(self, rom_title_tilemap_editor);
        close_or_pause!(self, rom_credits_tilemap_editor);
        close_or_pause!(self, rom_overworld_player_start_editor);
        close_or_pause!(self, rom_overworld_settings_editor);
        close_or_pause!(self, rom_overworld_event_number_editor);
        close_or_pause!(self, rom_overworld_event_reveal_editor);
        close_or_pause!(self, rom_overworld_event_tilemap_editor);
        close_or_pause!(self, rom_overworld_level_name_editor);
        close_or_pause!(self, rom_overworld_special_event_editor);
        close_or_pause!(self, rom_level_assets_editor);
        close_or_pause!(self, rom_map16_editor);
        close_or_pause!(self, rom_palette_editor);
        close_or_pause!(self, rom_graphics_editor);
        close_or_pause!(self, rom_exanimation_editor);
        close_or_pause!(self, rom_overworld_editor);
        self.dispatch(context, Command::Quit);
    }
}
