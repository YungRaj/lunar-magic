use super::{MetadataEditor, edit_buttons, selector, submap_combo, text, text_field};
use crate::metadata_editor_forms::{LevelNameForm, PlayerStartForm, SettingsForm};
use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};
use lm_overworld::MetadataEdit;

impl MetadataEditor {
    pub(super) fn show_level_names(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<MetadataEdit, String>> {
        let controller = self.controller.as_ref()?;
        let values = &controller.metadata().level_names;
        selector(
            ui,
            &mut self.name_index,
            values.len(),
            &text(catalog, ExtendedUiTextKey::MetadataLevelNameRecord),
        );
        if self.name_key != Some((controller.revision(), self.name_index)) {
            self.name = values
                .get(self.name_index)
                .map_or_else(LevelNameForm::default, LevelNameForm::load);
            self.name_key = Some((controller.revision(), self.name_index));
        }
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataLevelKeyHex),
            &mut self.name.level,
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataTileBytesHex),
            &mut self.name.tiles,
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataRawFlagsHex),
            &mut self.name.raw_flags,
        );
        let (upsert, remove) = edit_buttons(
            ui,
            !values.is_empty(),
            ExtendedUiTextKey::MetadataUpsertName,
            catalog,
        );
        if upsert {
            Some(self.name.parse().map(MetadataEdit::UpsertLevelName))
        } else if remove {
            Some(Ok(MetadataEdit::RemoveLevelName(
                values[self.name_index].level,
            )))
        } else {
            None
        }
    }

    pub(super) fn show_player_starts(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<MetadataEdit, String>> {
        let controller = self.controller.as_ref()?;
        let values = &controller.metadata().player_starts;
        selector(
            ui,
            &mut self.start_index,
            values.len(),
            &text(catalog, ExtendedUiTextKey::MetadataPlayerStartRecord),
        );
        if self.start_key != Some((controller.revision(), self.start_index)) {
            self.start = values
                .get(self.start_index)
                .copied()
                .map_or_else(PlayerStartForm::default, PlayerStartForm::load);
            self.start_key = Some((controller.revision(), self.start_index));
        }
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataPlayerKeyHex),
            &mut self.start.player,
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataXHex),
            &mut self.start.x,
        );
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataYHex),
            &mut self.start.y,
        );
        submap_combo(ui, &mut self.start.submap, "metadata-start-submap", catalog);
        text_field(
            ui,
            &text(catalog, ExtendedUiTextKey::MetadataRawFlagsHex),
            &mut self.start.raw_flags,
        );
        let (upsert, remove) = edit_buttons(
            ui,
            !values.is_empty(),
            ExtendedUiTextKey::MetadataUpsertStart,
            catalog,
        );
        if upsert {
            Some(self.start.parse().map(MetadataEdit::UpsertPlayerStart))
        } else if remove {
            Some(Ok(MetadataEdit::RemovePlayerStart(
                values[self.start_index].player,
            )))
        } else {
            None
        }
    }

    pub(super) fn show_submap_settings(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<MetadataEdit, String>> {
        let controller = self.controller.as_ref()?;
        let values = &controller.metadata().submap_settings;
        selector(
            ui,
            &mut self.settings_index,
            values.len(),
            &text(catalog, ExtendedUiTextKey::MetadataSettingsRecord),
        );
        if self.settings_key != Some((controller.revision(), self.settings_index)) {
            self.settings = values
                .get(self.settings_index)
                .copied()
                .map_or_else(SettingsForm::default, SettingsForm::load);
            self.settings_key = Some((controller.revision(), self.settings_index));
        }
        submap_combo(
            ui,
            &mut self.settings.submap,
            "metadata-settings-submap",
            catalog,
        );
        for (label, field) in [
            (
                ExtendedUiTextKey::MetadataMusicHex,
                &mut self.settings.music,
            ),
            (
                ExtendedUiTextKey::MetadataPaletteHex,
                &mut self.settings.palette,
            ),
            (
                ExtendedUiTextKey::MetadataLayer1ScrollHex,
                &mut self.settings.layer1_scroll,
            ),
            (
                ExtendedUiTextKey::MetadataLayer2ScrollHex,
                &mut self.settings.layer2_scroll,
            ),
            (
                ExtendedUiTextKey::MetadataRawFlagsHex,
                &mut self.settings.raw_flags,
            ),
            (
                ExtendedUiTextKey::MetadataUnknownBytesHex,
                &mut self.settings.unknown,
            ),
        ] {
            text_field(ui, &text(catalog, label), field);
        }
        let (upsert, remove) = edit_buttons(
            ui,
            !values.is_empty(),
            ExtendedUiTextKey::MetadataUpsertSettings,
            catalog,
        );
        if upsert {
            Some(
                self.settings
                    .parse()
                    .map(MetadataEdit::UpsertSubmapSettings),
            )
        } else if remove {
            Some(Ok(MetadataEdit::RemoveSubmapSettings(
                values[self.settings_index].submap,
            )))
        } else {
            None
        }
    }
}
