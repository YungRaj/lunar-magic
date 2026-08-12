use super::{AggregatePanels, text};
use crate::level_editor_forms;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, NativeLevelAssetsControllerEdit};
use lm_graphics::ExAnimationFeature;
use lm_level::{ExpandedLevelHeader, SuperGraphicsBypass};
use lm_project::NativeLevelAssetsFile;

impl AggregatePanels {
    pub(super) fn settings_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        if file.assets.expanded_settings.is_none() {
            ui.label(text(catalog, Key::NativeAssetsSettingsUnavailable));
            return self.exanimation_feature_panel(ui, catalog);
        }
        ui.heading(text(catalog, Key::NativeAssetsSettingsLayer3Title));
        ui.checkbox(
            &mut self.layer3_settings.layer3_enabled,
            text(catalog, Key::NativeAssetsSettingsLayer3Enable),
        );
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::NativeAssetsSettingsGfxFile));
            ui.text_edit_singleline(&mut self.layer3_settings.layer3_file);
        });
        ui.add(
            egui::Slider::new(&mut self.layer3_settings.layer3_length_selector, 0..=3)
                .text(text(catalog, Key::NativeAssetsSettingsLengthSelector)),
        );
        ui.add(
            egui::Slider::new(&mut self.layer3_settings.layer3_offset_selector, 0..=3)
                .text(text(catalog, Key::NativeAssetsSettingsDestinationSelector)),
        );
        if ui
            .button(text(catalog, Key::NativeAssetsSettingsApplyLayer3))
            .clicked()
        {
            return Some(layer3_tilemap_edit(&self.layer3_settings));
        }
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::NativeAssetsSettingsExpandedMode));
            ui.text_edit_singleline(&mut self.layer3_settings.layer3_expanded_mode);
        });
        ui.small(text(catalog, Key::NativeAssetsSettingsExpandedModeNotice));
        if ui
            .button(text(catalog, Key::NativeAssetsSettingsApplyExpandedMode))
            .clicked()
        {
            return Some(
                self.layer3_settings
                    .layer3_expanded_mode()
                    .map(NativeLevelAssetsControllerEdit::Layer3ExpandedMode),
            );
        }
        ui.separator();
        ui.heading(text(catalog, Key::NativeAssetsSettingsBypassTitle));
        ui.checkbox(
            &mut self.bypass_enabled,
            text(catalog, Key::NativeAssetsSettingsBypassEnable),
        );
        egui::Grid::new("aggregate-super-gfx")
            .num_columns(4)
            .show(ui, |ui| {
                for (slot, label) in ["FG1", "FG2", "FG3", "BG1", "BG2", "BG3"]
                    .into_iter()
                    .enumerate()
                {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.bypass_foreground_background[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
                for (slot, label) in ["SP1", "SP2", "SP3", "SP4"].into_iter().enumerate() {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.bypass_sprites[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if ui
            .button(text(catalog, Key::NativeAssetsSettingsApplyBypass))
            .clicked()
        {
            let result = super_graphics_edit(SuperGraphicsBypass {
                enabled: self.bypass_enabled,
                foreground_background: self.bypass_foreground_background,
                sprites: self.bypass_sprites,
            });
            return Some(result);
        }
        ui.separator();
        ui.heading(text(catalog, Key::NativeAssetsSettingsBoundaryTitle));
        ui.checkbox(
            &mut self.sprites_beyond_boundaries_use_air,
            text(catalog, Key::NativeAssetsSettingsBoundaryAir),
        );
        ui.small(text(catalog, Key::NativeAssetsSettingsBoundaryNotice));
        if ui
            .button(text(catalog, Key::NativeAssetsSettingsApplyBoundary))
            .clicked()
        {
            return Some(Ok(sprite_boundary_air_edit(
                self.sprites_beyond_boundaries_use_air,
            )));
        }
        if let Some(edit) = self.exanimation_feature_panel(ui, catalog) {
            return Some(edit);
        }
        ui.separator();
        ui.label(text(catalog, Key::NativeAssetsSettingsRawWordsNotice));
        egui::Grid::new("aggregate-settings").show(ui, |ui| {
            for (index, value) in self.settings.iter_mut().enumerate() {
                ui.label(
                    text(catalog, Key::NativeAssetsSettingsWordFormat)
                        .replace("{index}", &format!("{index:X}")),
                );
                ui.text_edit_singleline(value);
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.button(text(catalog, Key::NativeAssetsSettingsApplyWords))
            .clicked()
            .then(|| {
                self.settings
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        level_editor_forms::parse_hex_u16(
                            value,
                            &format!("settings word {index:X}"),
                        )
                        .map(|word| (index, word))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(NativeLevelAssetsControllerEdit::ExpandedSettingsWords)
            })
    }

    fn exanimation_feature_panel(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.separator();
        ui.heading(text(catalog, Key::NativeAssetsSettingsAnimationOptions));
        let Some(features) = &mut self.exanimation_features else {
            ui.label(text(catalog, Key::NativeAssetsSettingsAnimationUnavailable));
            return None;
        };
        for (feature, key) in [
            (
                ExAnimationFeature::PaletteAnimation,
                Key::NativeAssetsSettingsPaletteAnimation,
            ),
            (
                ExAnimationFeature::VanillaAnimation,
                Key::NativeAssetsSettingsVanillaAnimation,
            ),
            (
                ExAnimationFeature::GlobalExAnimation,
                Key::NativeAssetsSettingsGlobalAnimation,
            ),
            (
                ExAnimationFeature::LevelExAnimation,
                Key::NativeAssetsSettingsLevelAnimation,
            ),
        ] {
            let mut enabled = features.enabled(feature);
            if ui.checkbox(&mut enabled, text(catalog, key)).changed() {
                features.set_enabled(feature, enabled);
            }
        }
        ui.small(
            text(catalog, Key::NativeAssetsSettingsPreservedNibbleFormat)
                .replace("{value}", &format!("{:X}", features.preserved_low_nibble)),
        );
        ui.button(text(catalog, Key::NativeAssetsSettingsApplyAnimation))
            .clicked()
            .then(|| {
                Ok(NativeLevelAssetsControllerEdit::ExAnimationFeatureStates {
                    palette: features.enabled(ExAnimationFeature::PaletteAnimation),
                    vanilla: features.enabled(ExAnimationFeature::VanillaAnimation),
                    global: features.enabled(ExAnimationFeature::GlobalExAnimation),
                    level: features.enabled(ExAnimationFeature::LevelExAnimation),
                })
            })
    }
}

fn super_graphics_edit(
    bypass: SuperGraphicsBypass,
) -> Result<NativeLevelAssetsControllerEdit, String> {
    let mut header = ExpandedLevelHeader::default();
    header
        .set_super_graphics_bypass(bypass)
        .map_err(|error| error.to_string())?;
    Ok(NativeLevelAssetsControllerEdit::SuperGraphicsBypass(bypass))
}

fn sprite_boundary_air_edit(enabled: bool) -> NativeLevelAssetsControllerEdit {
    NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(enabled)
}

fn layer3_tilemap_edit(
    form: &crate::expanded_settings_editor_form::ExpandedSettingsForm,
) -> Result<NativeLevelAssetsControllerEdit, String> {
    let file = level_editor_forms::parse_hex_u16(&form.layer3_file, "Layer 3 graphics file")?;
    let descriptor = lm_level::Layer3TilemapGraphicsDescriptor::new(
        file,
        form.layer3_length_selector,
        form.layer3_offset_selector,
    )
    .map_err(|error| error.to_string())?;
    Ok(NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
        enabled: form.layer3_enabled,
        descriptor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_aggregate_settings_panel_has_no_literal_widget_text() {
        let source = include_str!("settings.rs");
        for literal_widget in [
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "Button::new(\"",
            "ui.small(\"",
            ".text(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "aggregate Settings panel regressed to fixed widget text: {literal_widget}"
            );
        }
    }

    #[test]
    fn installed_bypass_form_emits_validated_semantic_intent() {
        let bypass = SuperGraphicsBypass {
            enabled: true,
            foreground_background: [1, 2, 3, 4, 5, 6],
            sprites: [0x101, 0x202, 0x303, 0x404],
        };
        assert!(matches!(
            super_graphics_edit(bypass).unwrap(),
            NativeLevelAssetsControllerEdit::SuperGraphicsBypass(value) if value == bypass
        ));
        let mut invalid = bypass;
        invalid.sprites[3] = 0x1000;
        assert!(super_graphics_edit(invalid).is_err());
    }

    #[test]
    fn installed_sprite_boundary_air_form_emits_semantic_intent() {
        assert!(matches!(
            sprite_boundary_air_edit(true),
            NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(true)
        ));
    }

    #[test]
    fn aggregate_layer3_form_emits_validated_semantic_intent() {
        let mut form = crate::expanded_settings_editor_form::ExpandedSettingsForm::default();
        form.layer3_enabled = true;
        form.layer3_file = "ABC".into();
        form.layer3_length_selector = 2;
        form.layer3_offset_selector = 3;
        assert!(matches!(
            layer3_tilemap_edit(&form).unwrap(),
            NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
                enabled: true,
                descriptor,
            } if descriptor.packed() == 0xEABC
        ));
        form.layer3_file = "1000".into();
        assert!(layer3_tilemap_edit(&form).is_err());
    }

    #[test]
    fn aggregate_layer3_mode_form_emits_all_packed_bits() {
        let mut form = crate::expanded_settings_editor_form::ExpandedSettingsForm::default();
        form.layer3_expanded_mode = "89ABCDEF".into();
        assert_eq!(form.layer3_expanded_mode().unwrap().packed(), 0x89ab_cdef);
    }
}
