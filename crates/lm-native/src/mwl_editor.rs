use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    mwl_editor_form::MwlForm,
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog, MwlDocumentController, MwlDocumentEdit};
use lm_level::MwlFile;

mod object_panel;
mod optional_import;
mod optional_panel;
mod sprite_panel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingLoad {
    Open,
    OptionalInterpretation { maximum_records: usize },
    OptionalAssets { maximum_records: usize },
}

#[derive(Clone, Debug)]
struct OptionalAssetsInterpretation {
    maximum_records: usize,
    modes: [bool; 256],
}

pub(crate) struct MwlEditor {
    controller: Option<MwlDocumentController>,
    form: MwlForm,
    layer3_settings: crate::expanded_settings_editor_form::ExpandedSettingsForm,
    loaded_header_revision: Option<u64>,
    loaded_section_key: Option<(u64, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    pending_load: Option<PendingLoad>,
    optional_interpretation: Option<OptionalAssetsInterpretation>,
    optional_maximum_records: String,
    optional_panel: optional_panel::MwlOptionalAssetsPanel,
    object_panel: object_panel::MwlObjectPanel,
    sprite_panel: sprite_panel::MwlSpritePanel,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl Default for MwlEditor {
    fn default() -> Self {
        Self {
            controller: None,
            form: MwlForm::default(),
            layer3_settings: Default::default(),
            loaded_header_revision: None,
            loaded_section_key: None,
            error: None,
            pending_close: None,
            pending_load: None,
            optional_interpretation: None,
            optional_maximum_records: "32".into(),
            optional_panel: optional_panel::MwlOptionalAssetsPanel::default(),
            object_panel: object_panel::MwlObjectPanel::default(),
            sprite_panel: sprite_panel::MwlSpritePanel::default(),
            persistence: DocumentPersistence::default(),
            loader: DocumentLoader::default(),
        }
    }
}

impl MwlEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_mwl_document() else {
            return;
        };
        match self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "MWL document",
        )]) {
            Ok(()) => self.pending_load = Some(PendingLoad::Open),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for MWL loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for MWL persistence to finish before closing".into());
            return false;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        self.poll_load(context);
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new(text(catalog, ExtendedUiTextKey::MwlDocumentEditorTitle))
                .default_size([800.0, 650.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let revision = controller.revision();
        if self.loaded_header_revision != Some(revision) {
            let section_index = self.form.section_index;
            self.form = MwlForm::load_header(controller.value());
            self.form.section_index = section_index.min(MwlFile::SECTION_COUNT - 1);
            self.loaded_header_revision = Some(revision);
            self.loaded_section_key = None;
            self.layer3_settings = controller
                .value()
                .expanded_settings_section()
                .map(|settings| {
                    crate::expanded_settings_editor_form::ExpandedSettingsForm::load(&settings)
                })
                .unwrap_or_default();
        }
        if self.loaded_section_key != Some((revision, self.form.section_index)) {
            self.form
                .load_section(controller.value(), self.form.section_index);
            self.loaded_section_key = Some((revision, self.form.section_index));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
        ui.separator();
        let version = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.value().version);
        ui.label(
            text(catalog, ExtendedUiTextKey::MwlDocumentVersionFormat)
                .replace("{version}", &format!("{version:04X}")),
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MwlDocumentFlagsHex),
            &mut self.form.flags,
        );
        ui.label(text(
            catalog,
            ExtendedUiTextKey::MwlDocumentAttributionNotice,
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.attribution)
                .desired_rows(3)
                .code_editor(),
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MwlDocumentLevelNumberNotice),
            &mut self.form.level_number,
        );
        self.entrance_settings(ui, catalog);
        if ui
            .button(text(catalog, ExtendedUiTextKey::MwlDocumentApplyHeader))
            .clicked()
        {
            match self.form.header_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        self.layer3_settings_panel(ui, catalog);
        ui.separator();
        self.optional_assets_import_controls(ui, catalog);
        self.show_optional_assets_panel(ui, catalog);
        ui.separator();
        let object_result = self
            .controller
            .as_mut()
            .map(|controller| self.object_panel.show(ui, controller, catalog));
        match object_result {
            Some(Ok(true)) => self.invalidate(),
            Some(Err(error)) => self.error = Some(error),
            Some(Ok(false)) | None => {}
        }
        ui.separator();
        let sprite_result = self
            .controller
            .as_mut()
            .map(|controller| self.sprite_panel.show(ui, controller, catalog));
        match sprite_result {
            Some(Ok(true)) => self.invalidate(),
            Some(Err(error)) => self.error = Some(error),
            Some(Ok(false)) | None => {}
        }
        ui.separator();
        let previous_section = self.form.section_index;
        let section_names = mwl_section_names(catalog);
        egui::ComboBox::from_id_salt("mwl-section")
            .selected_text(&section_names[self.form.section_index])
            .show_ui(ui, |ui| {
                for (index, name) in section_names.iter().enumerate() {
                    ui.selectable_value(&mut self.form.section_index, index, name);
                }
            });
        if previous_section != self.form.section_index {
            self.loaded_section_key = None;
            self.load_form();
        }
        let section_len = self.controller.as_ref().map_or(0, |controller| {
            controller.value().sections[self.form.section_index]
                .bytes
                .len()
        });
        ui.label(
            text(catalog, ExtendedUiTextKey::MwlDocumentSectionLengthFormat)
                .replace("{length}", &section_len.to_string()),
        );
        ui.label(text(catalog, ExtendedUiTextKey::MwlDocumentSectionBytes));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.section_bytes)
                .desired_rows(14)
                .code_editor(),
        );
        if ui
            .button(text(catalog, ExtendedUiTextKey::MwlDocumentReplaceSection))
            .clicked()
        {
            match self.form.section_edit() {
                Ok(edit) => self.apply_edits(std::slice::from_ref(&edit)),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn layer3_settings_panel(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let available = self
            .controller
            .as_ref()
            .is_some_and(|controller| controller.value().expanded_settings_section().is_ok());
        ui.collapsing(
            text(catalog, ExtendedUiTextKey::MwlDocumentLayer3Heading),
            |ui| {
                if !available {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::MwlDocumentLayer3Unavailable,
                    ));
                    return;
                }
                ui.checkbox(
                    &mut self.layer3_settings.layer3_enabled,
                    text(catalog, ExtendedUiTextKey::MwlDocumentLayer3Enable),
                );
                text_field(
                    ui,
                    &text(catalog, ExtendedUiTextKey::MwlDocumentLayer3File),
                    &mut self.layer3_settings.layer3_file,
                );
                ui.add(
                    egui::Slider::new(&mut self.layer3_settings.layer3_length_selector, 0..=3)
                        .text(text(catalog, ExtendedUiTextKey::MwlDocumentLengthSelector)),
                );
                ui.add(
                    egui::Slider::new(&mut self.layer3_settings.layer3_offset_selector, 0..=3)
                        .text(text(
                            catalog,
                            ExtendedUiTextKey::MwlDocumentDestinationSelector,
                        )),
                );
                text_field(
                    ui,
                    &text(catalog, ExtendedUiTextKey::MwlDocumentExpandedMode),
                    &mut self.layer3_settings.layer3_expanded_mode,
                );
                if ui
                    .button(text(catalog, ExtendedUiTextKey::MwlDocumentApplyLayer3))
                    .clicked()
                {
                    match apply_layer3_settings_form(
                        self.controller.as_mut().expect("availability checked"),
                        &self.layer3_settings,
                    ) {
                        Ok(()) => self.invalidate(),
                        Err(error) => self.error = Some(error),
                    }
                }
            },
        );
    }

    fn entrance_settings(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.collapsing(
            text(catalog, ExtendedUiTextKey::MwlDocumentEntranceHeading),
            |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::MwlDocumentEntranceNotice));
                egui::Grid::new("mwl-main-entrance").show(ui, |ui| {
                    for (key, value) in [
                        ExtendedUiTextKey::MwlDocumentMainPosition,
                        ExtendedUiTextKey::MwlDocumentMainVertical,
                        ExtendedUiTextKey::MwlDocumentMainScreenMethod,
                        ExtendedUiTextKey::MwlDocumentMainModeScreen,
                        ExtendedUiTextKey::MwlDocumentMainFlags,
                        ExtendedUiTextKey::MwlDocumentMainHighPosition,
                        ExtendedUiTextKey::MwlDocumentMainAdditionalFlags,
                    ]
                    .into_iter()
                    .zip(&mut self.form.main_entrance)
                    {
                        ui.label(text(catalog, key));
                        ui.text_edit_singleline(value);
                        ui.end_row();
                    }
                    for (key, value) in [
                        ExtendedUiTextKey::MwlDocumentMidwayPosition,
                        ExtendedUiTextKey::MwlDocumentMidwayFlags,
                        ExtendedUiTextKey::MwlDocumentMidwayHighPosition,
                        ExtendedUiTextKey::MwlDocumentMidwayAdditionalFlags,
                    ]
                    .into_iter()
                    .zip(&mut self.form.midway_entrance)
                    {
                        ui.label(text(catalog, key));
                        ui.text_edit_singleline(value);
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.checkbox(
                    &mut self.form.separate_layer2_scroll,
                    text(catalog, ExtendedUiTextKey::MwlDocumentSeparateLayer2Scroll),
                );
                egui::Grid::new("mwl-layer2-scroll").show(ui, |ui| {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::MwlDocumentOriginalScrollPreset,
                    ));
                    ui.add_enabled(
                        !self.form.separate_layer2_scroll,
                        egui::DragValue::new(&mut self.form.layer2_original_scroll).range(0..=15),
                    );
                    ui.end_row();
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::MwlDocumentHorizontalSelector,
                    ));
                    ui.add_enabled(
                        self.form.separate_layer2_scroll,
                        egui::DragValue::new(&mut self.form.layer2_horizontal_scroll).range(0..=31),
                    );
                    ui.end_row();
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::MwlDocumentVerticalSelector,
                    ));
                    ui.add_enabled(
                        self.form.separate_layer2_scroll,
                        egui::DragValue::new(&mut self.form.layer2_vertical_scroll).range(0..=31),
                    );
                    ui.end_row();
                });
                ui.separator();
                ui.heading(text(catalog, ExtendedUiTextKey::MwlDocumentSpriteSpawning));
                ui.add(
                    egui::Slider::new(&mut self.form.sprite_vertical_spawn_range, 0..=3).text(
                        text(catalog, ExtendedUiTextKey::MwlDocumentVerticalSpawnRange),
                    ),
                );
                ui.checkbox(
                    &mut self.form.sprite_smart_spawn,
                    text(catalog, ExtendedUiTextKey::MwlDocumentSmartSpawn),
                );
            },
        );
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let (can_undo, can_redo, modified) = (
            controller.can_undo(),
            controller.can_redo(),
            controller.is_modified(),
        );
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::MwlDocumentUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::MwlDocumentRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::MwlDocumentSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::MwlDocumentModified
                } else {
                    ExtendedUiTextKey::MwlDocumentSaved
                },
            ));
        });
        let mut changed = false;
        if let Some(controller) = self.controller.as_mut() {
            if let Some(undo) = history {
                let result = if undo {
                    controller.undo(controller.revision())
                } else {
                    controller.redo(controller.revision())
                };
                match result {
                    Ok(value) => changed = value,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn apply_edits(&mut self, edits: &[MwlDocumentEdit]) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) = controller.apply_edits(controller.revision(), edits) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn invalidate(&mut self) {
        self.loaded_header_revision = None;
        self.loaded_section_key = None;
        self.object_panel.invalidate();
        self.sprite_panel.invalidate();
    }

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::MwlDocumentDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::MwlDocumentUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::MwlDocumentCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::MwlDocumentDiscard))
                        .clicked()
                    {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::MwlDocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::MwlDocumentOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_close = None;
        self.pending_load = None;
        self.optional_interpretation = None;
        self.optional_panel.invalidate();
        self.object_panel.invalidate();
        self.sprite_panel.invalidate();
        self.invalidate();
    }
}

pub(super) fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

trait OptionalCatalogText {
    fn extended_text(&self, key: ExtendedUiTextKey) -> &str;
}

impl OptionalCatalogText for Option<&LocalizationCatalog> {
    fn extended_text(&self, key: ExtendedUiTextKey) -> &str {
        self.map_or_else(|| key.english(), |catalog| catalog.extended_text(key))
    }
}

fn mwl_section_names(catalog: Option<&LocalizationCatalog>) -> [String; 8] {
    [
        ExtendedUiTextKey::MwlDocumentSectionLevelHeader,
        ExtendedUiTextKey::MwlDocumentSectionLayer1,
        ExtendedUiTextKey::MwlDocumentSectionLayer2,
        ExtendedUiTextKey::MwlDocumentSectionSprites,
        ExtendedUiTextKey::MwlDocumentSectionPalette,
        ExtendedUiTextKey::MwlDocumentSectionSecondaryExits,
        ExtendedUiTextKey::MwlDocumentSectionExAnimation,
        ExtendedUiTextKey::MwlDocumentSectionExpandedHeader,
    ]
    .map(|key| text(catalog, key))
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

fn apply_layer3_settings_form(
    controller: &mut MwlDocumentController,
    form: &crate::expanded_settings_editor_form::ExpandedSettingsForm,
) -> Result<(), String> {
    let (enabled, descriptor, mode) = form.layer3_settings()?;
    controller
        .apply_layer3_settings(controller.revision(), enabled, descriptor, Some(mode))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::ExpandedLevelSettingsRecord;

    #[test]
    fn complete_mwl_editor_surface_has_no_literal_widget_text() {
        let sources = [
            include_str!("mwl_editor.rs"),
            include_str!("mwl_editor/object_panel.rs"),
            include_str!("mwl_editor/sprite_panel.rs"),
            include_str!("mwl_editor/optional_import.rs"),
            include_str!("mwl_editor/optional_panel.rs"),
        ]
        .join("\n");
        for literal_widget in [
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            ".text(\"",
            ".prefix(\"",
            "ui.selectable_value(&mut self.tab, 0, \"",
        ] {
            assert!(
                !sources.contains(literal_widget),
                "MWL editor regressed to literal widget text: {literal_widget}"
            );
        }
    }

    #[test]
    fn layer3_native_form_edits_complete_mwl_settings_and_reloads_losslessly() {
        let mut file = MwlFile::default();
        let mut bytes = [0x5a; ExpandedLevelSettingsRecord::ENCODED_LEN];
        bytes[2..4].copy_from_slice(&0x007f_u16.to_le_bytes());
        let baseline = ExpandedLevelSettingsRecord::decode(&bytes).unwrap();
        file.set_expanded_settings_section(&baseline);
        let mut controller =
            MwlDocumentController::decode("portable.mwl".into(), &file.encode().unwrap()).unwrap();
        let mut form = crate::expanded_settings_editor_form::ExpandedSettingsForm::load(&baseline);
        form.layer3_enabled = true;
        form.layer3_file = "ABC".into();
        form.layer3_length_selector = 2;
        form.layer3_offset_selector = 3;
        form.layer3_expanded_mode = "89ABCDEF".into();

        apply_layer3_settings_form(&mut controller, &form).unwrap();

        let edited = controller.value().expanded_settings_section().unwrap();
        assert!(edited.layer3_tilemap_enabled());
        assert_eq!(
            edited
                .layer3_tilemap_graphics_descriptor()
                .unwrap()
                .packed(),
            0xeabc
        );
        assert_eq!(edited.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
        for word in 8..16 {
            assert_eq!(
                edited.word(word).unwrap() & 0x0fff,
                baseline.word(word).unwrap() & 0x0fff
            );
        }
        assert!(controller.undo(1).unwrap());
        assert_eq!(
            controller.value().expanded_settings_section().unwrap(),
            baseline
        );
        let revision = controller.revision();
        form.layer3_expanded_mode = "100000000".into();
        assert!(apply_layer3_settings_form(&mut controller, &form).is_err());
        assert_eq!(controller.revision(), revision);
        assert_eq!(
            controller.value().expanded_settings_section().unwrap(),
            baseline
        );
    }
}
