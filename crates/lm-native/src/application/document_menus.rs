use super::NativeApplication;
use eframe::egui;

macro_rules! document_pair {
    ($ui:expr, $editor:expr, $open:literal, $close:literal) => {{
        if $ui
            .add_enabled(!$editor.is_open(), egui::Button::new($open))
            .clicked()
        {
            $ui.close_menu();
            $editor.open();
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
    pub(super) fn documents_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Documents", |ui| {
            self.primary_document_menu_items(ui);
            self.extended_document_menu_items(ui);
        });
    }

    #[rustfmt::skip]
    fn primary_document_menu_items(&mut self, ui: &mut egui::Ui) {
        document_pair!(ui, self.palette_editor, "Open Portable Palette…", "Close Portable Palette");
        ui.separator();
        document_pair!(ui, self.graphics_editor, "Open Portable Graphics…", "Close Portable Graphics");
        ui.separator();
        document_pair!(ui, self.map16_editor, "Open Portable Map16 Page…", "Close Portable Map16 Page");
        ui.separator();
        document_pair!(ui, self.exanimation_editor, "Open Portable ExAnimation…", "Close Portable ExAnimation");
        ui.separator();
        document_pair!(ui, self.level_editor, "Open Portable Complete Level…", "Close Portable Complete Level");
        ui.separator();
        document_pair!(ui, self.overworld_editor, "Open Portable Complete Overworld…", "Close Portable Complete Overworld");
        ui.separator();
        document_pair!(ui, self.path_editor, "Open Portable Overworld Paths…", "Close Portable Overworld Paths");
        ui.separator();
        document_pair!(ui, self.metadata_editor, "Open Portable Overworld Metadata…", "Close Portable Overworld Metadata");
        ui.separator();
        document_pair!(ui, self.appearance_editor, "Open Portable Entity Appearances…", "Close Portable Entity Appearances");
        ui.separator();
        document_pair!(ui, self.overworld_appearance_editor, "Open Portable Overworld Appearances…", "Close Portable Overworld Appearances");
    }

    #[rustfmt::skip]
    fn extended_document_menu_items(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        document_pair!(ui, self.layer3_editor, "Open Portable Layer 3…", "Close Portable Layer 3");
        ui.separator();
        document_pair!(ui, self.mwl_editor, "Open MWL…", "Close MWL");
        ui.separator();
        document_pair!(ui, self.expanded_settings_editor, "Open Expanded Settings…", "Close Expanded Settings");
        ui.separator();
        document_pair!(ui, self.custom_object_editor, "Open Custom Object Library…", "Close Custom Object Library");
        ui.separator();
        document_pair!(ui, self.custom_sprite_editor, "Open Custom Sprite Library…", "Close Custom Sprite Library");
        ui.separator();
        document_pair!(ui, self.native_map16_sidecar_editor, "Open Native Map16 Sidecar…", "Close Native Map16 Sidecar");
        ui.separator();
        document_pair!(ui, self.dsc_sidecar_editor, "Open DSC Sidecar…", "Close DSC Sidecar");
        ui.separator();
        document_pair!(ui, self.map16_set_editor, "Open Complete Map16 Set…", "Close Complete Map16 Set");
        ui.separator();
        document_pair!(ui, self.native_level_document_editor, "Open Native Level Streams…", "Close Native Level Streams");
        ui.separator();
        document_pair!(ui, self.native_level_assets_editor, "Open Native Level Assets…", "Close Native Level Assets");
    }
}
