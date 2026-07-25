use super::NativeApplication;
use eframe::egui;

impl NativeApplication {
    pub(super) fn show_editor_windows(&mut self, context: &egui::Context) {
        if self.palette_editor.show(context) {
            self.request_quit(context);
        }
        if self.graphics_editor.show(context) {
            self.request_quit(context);
        }
        if self.map16_editor.show(context) {
            self.request_quit(context);
        }
        if self.exanimation_editor.show(context) {
            self.request_quit(context);
        }
        if self.level_editor.show(context) {
            self.request_quit(context);
        }
        if self.overworld_editor.show(context) {
            self.request_quit(context);
        }
        if self.path_editor.show(context) {
            self.request_quit(context);
        }
        if self.metadata_editor.show(context) {
            self.request_quit(context);
        }
        if self.appearance_editor.show(context) {
            self.request_quit(context);
        }
        if self.overworld_appearance_editor.show(context) {
            self.request_quit(context);
        }
        if self.layer3_editor.show(context) {
            self.request_quit(context);
        }
        if self.mwl_editor.show(context) {
            self.request_quit(context);
        }
        if self.expanded_settings_editor.show(context) {
            self.request_quit(context);
        }
        if self.custom_object_editor.show(context) {
            self.request_quit(context);
        }
        if self.custom_sprite_editor.show(context) {
            self.request_quit(context);
        }
        let foreground_texture = self.vanilla_level_editor.foreground_texture().cloned();
        if self
            .native_map16_sidecar_editor
            .show(context, foreground_texture.as_ref())
        {
            self.request_quit(context);
        }
        if self.dsc_sidecar_editor.show(context) {
            self.request_quit(context);
        }
        if self.ssc_sidecar_editor.show(context) {
            self.request_quit(context);
        }
        if self.osc_sidecar_editor.show(context) {
            self.request_quit(context);
        }
        if self.map16_set_editor.show(context) {
            self.request_quit(context);
        }
        if self.native_level_document_editor.show(context) {
            self.request_quit(context);
        }
        if self.native_level_assets_editor.show(context) {
            self.request_quit(context);
        }
        self.show_rom_editors(context);
        self.show_project_operations(context);
    }
}
