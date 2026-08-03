use super::AggregatePanels;
use crate::level_editor_forms;
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_graphics::ExAnimationFeature;
use lm_level::{ExpandedLevelHeader, ExpandedLevelSettingsRecord, SuperGraphicsBypass};
use lm_project::NativeLevelAssetsFile;

impl AggregatePanels {
    pub(super) fn settings_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        if file.assets.expanded_settings.is_none() {
            ui.label("This aggregate has no expanded-settings record.");
            return self.exanimation_feature_panel(ui);
        }
        ui.heading("Super GFX Bypass");
        ui.checkbox(&mut self.bypass_enabled, "Use per-level GFX/ExGFX files");
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
        if ui.button("Apply Super GFX bypass").clicked() {
            let settings = file
                .assets
                .expanded_settings
                .as_ref()
                .expect("presence checked above");
            let result = super_graphics_edits(
                settings,
                SuperGraphicsBypass {
                    enabled: self.bypass_enabled,
                    foreground_background: self.bypass_foreground_background,
                    sprites: self.bypass_sprites,
                },
            )
            .map(NativeLevelAssetsControllerEdit::ExpandedSettingsWords);
            return Some(result);
        }
        ui.separator();
        ui.heading("Sprite boundary interaction");
        ui.checkbox(
            &mut self.sprites_beyond_boundaries_use_air,
            "Sprites beyond level boundaries interact with air instead of water",
        );
        ui.small("Lunar Magic recommends enabling this for tide levels.");
        if ui.button("Apply sprite boundary interaction").clicked() {
            let settings = file
                .assets
                .expanded_settings
                .as_ref()
                .expect("presence checked above");
            let value = sprite_boundary_air_edit(settings, self.sprites_beyond_boundaries_use_air);
            return Some(Ok(NativeLevelAssetsControllerEdit::ExpandedSettingsWords(
                vec![value],
            )));
        }
        if let Some(edit) = self.exanimation_feature_panel(ui) {
            return Some(edit);
        }
        ui.separator();
        ui.label("Raw expanded words (unproven fields remain editable and lossless):");
        egui::Grid::new("aggregate-settings").show(ui, |ui| {
            for (index, value) in self.settings.iter_mut().enumerate() {
                ui.label(format!("Word {index:X}"));
                ui.text_edit_singleline(value);
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.button("Apply all words").clicked().then(|| {
            self.settings
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    level_editor_forms::parse_hex_u16(value, &format!("settings word {index:X}"))
                        .map(|word| (index, word))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NativeLevelAssetsControllerEdit::ExpandedSettingsWords)
        })
    }

    fn exanimation_feature_panel(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.separator();
        ui.heading("Animation options");
        let Some(features) = &mut self.exanimation_features else {
            ui.label("This profile does not declare installed animation-feature storage.");
            return None;
        };
        for (feature, label) in [
            (ExAnimationFeature::PaletteAnimation, "Palette animation"),
            (
                ExAnimationFeature::VanillaAnimation,
                "Vanilla animated tiles",
            ),
            (ExAnimationFeature::GlobalExAnimation, "Global ExAnimation"),
            (ExAnimationFeature::LevelExAnimation, "Level ExAnimation"),
        ] {
            let mut enabled = features.enabled(feature);
            if ui.checkbox(&mut enabled, label).changed() {
                features.set_enabled(feature, enabled);
            }
        }
        ui.small(format!(
            "Preserved unrelated low nibble: {:X}",
            features.preserved_low_nibble
        ));
        ui.button("Apply animation options").clicked().then(|| {
            Ok(NativeLevelAssetsControllerEdit::ExAnimationFeatures(
                *features,
            ))
        })
    }
}

fn super_graphics_edits(
    settings: &ExpandedLevelSettingsRecord,
    bypass: SuperGraphicsBypass,
) -> Result<Vec<(usize, u16)>, String> {
    let mut header = ExpandedLevelHeader::from(settings);
    header
        .set_super_graphics_bypass(bypass)
        .map_err(|error| error.to_string())?;
    Ok([0]
        .into_iter()
        .chain(2..=11)
        .map(|index| (index, header.fields[index]))
        .collect())
}

fn sprite_boundary_air_edit(settings: &ExpandedLevelSettingsRecord, enabled: bool) -> (usize, u16) {
    let mut header = ExpandedLevelHeader::from(settings);
    header.set_sprites_beyond_boundaries_use_air(enabled);
    (8, header.fields[8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_bypass_edits_touch_only_recovered_words() {
        let source =
            std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(7));
        let settings = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        let bypass = SuperGraphicsBypass {
            enabled: true,
            foreground_background: [1, 2, 3, 4, 5, 6],
            sprites: [0x101, 0x202, 0x303, 0x404],
        };
        let edits = super_graphics_edits(&settings, bypass).unwrap();
        assert_eq!(
            edits.iter().map(|(word, _)| *word).collect::<Vec<_>>(),
            [0].into_iter().chain(2..=11).collect::<Vec<_>>()
        );
        let mut rebuilt = settings.clone();
        for (word, value) in edits {
            rebuilt.set_word(word, value).unwrap();
        }
        let header = ExpandedLevelHeader::from(&rebuilt);
        assert_eq!(header.super_graphics_bypass(), bypass);
        assert_eq!(rebuilt.word(1).unwrap(), settings.word(1).unwrap());
        for word in 12..16 {
            assert_eq!(rebuilt.word(word).unwrap(), settings.word(word).unwrap());
        }
    }

    #[test]
    fn installed_sprite_boundary_air_edit_preserves_word_eight_unowned_bits() {
        let mut source = [0; 32];
        source[16..18].copy_from_slice(&0xb123_u16.to_le_bytes());
        let settings = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        assert_eq!(sprite_boundary_air_edit(&settings, true), (8, 0xf123));
        assert_eq!(sprite_boundary_air_edit(&settings, false), (8, 0xb123));
    }
}
