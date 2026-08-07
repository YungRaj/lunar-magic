use super::NativeApplication;
use eframe::egui;
use lm_app::UiTextKey;

macro_rules! document_pair {
    ($application:expr, $ui:expr, $editor:expr, $name:expr) => {{
        let document = $application.menu_text($name);
        let open = $application
            .menu_text(UiTextKey::DocumentsOpenFormat)
            .replace("{document}", &document);
        let close = $application
            .menu_text(UiTextKey::DocumentsCloseFormat)
            .replace("{document}", &document);
        if $ui
            .add_enabled(!$editor.is_open(), egui::Button::new(open))
            .clicked()
        {
            $ui.close_menu();
            $editor.open();
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
    pub(super) fn documents_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(self.menu_text(UiTextKey::MenuDocuments), |ui| {
            self.primary_document_menu_items(ui);
            self.extended_document_menu_items(ui);
        });
    }

    #[rustfmt::skip]
    fn primary_document_menu_items(&mut self, ui: &mut egui::Ui) {
        document_pair!(self, ui, self.palette_editor, UiTextKey::DocumentPortablePalette);
        ui.separator();
        document_pair!(self, ui, self.graphics_editor, UiTextKey::DocumentPortableGraphics);
        ui.separator();
        document_pair!(self, ui, self.map16_editor, UiTextKey::DocumentPortableMap16Page);
        ui.separator();
        document_pair!(self, ui, self.exanimation_editor, UiTextKey::DocumentPortableExAnimation);
        ui.separator();
        document_pair!(self, ui, self.level_editor, UiTextKey::DocumentPortableCompleteLevel);
        ui.separator();
        document_pair!(self, ui, self.overworld_editor, UiTextKey::DocumentPortableCompleteOverworld);
        ui.separator();
        document_pair!(self, ui, self.path_editor, UiTextKey::DocumentPortableOverworldPaths);
        ui.separator();
        document_pair!(self, ui, self.metadata_editor, UiTextKey::DocumentPortableOverworldMetadata);
        ui.separator();
        document_pair!(self, ui, self.appearance_editor, UiTextKey::DocumentPortableEntityAppearances);
        ui.separator();
        document_pair!(self, ui, self.overworld_appearance_editor, UiTextKey::DocumentPortableOverworldAppearances);
    }

    #[rustfmt::skip]
    fn extended_document_menu_items(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        document_pair!(self, ui, self.layer3_editor, UiTextKey::DocumentPortableLayer3);
        ui.separator();
        document_pair!(self, ui, self.mwl_editor, UiTextKey::DocumentMwl);
        ui.separator();
        document_pair!(self, ui, self.expanded_settings_editor, UiTextKey::DocumentExpandedSettings);
        ui.separator();
        document_pair!(self, ui, self.custom_object_editor, UiTextKey::DocumentCustomObjectLibrary);
        ui.separator();
        document_pair!(self, ui, self.custom_sprite_editor, UiTextKey::DocumentCustomSpriteLibrary);
        ui.separator();
        document_pair!(self, ui, self.native_map16_sidecar_editor, UiTextKey::DocumentNativeMap16Sidecar);
        ui.separator();
        document_pair!(self, ui, self.dsc_sidecar_editor, UiTextKey::DocumentDscSidecar);
        ui.separator();
        document_pair!(self, ui, self.ssc_sidecar_editor, UiTextKey::DocumentSscCustomSpriteMetadata);
        ui.separator();
        document_pair!(self, ui, self.osc_sidecar_editor, UiTextKey::DocumentOscCustomObjectMetadata);
        ui.separator();
        document_pair!(self, ui, self.map16_set_editor, UiTextKey::DocumentCompleteMap16Set);
        ui.separator();
        document_pair!(self, ui, self.native_level_document_editor, UiTextKey::DocumentNativeLevelStreams);
        ui.separator();
        document_pair!(self, ui, self.native_level_assets_editor, UiTextKey::DocumentNativeLevelAssets);
    }
}
