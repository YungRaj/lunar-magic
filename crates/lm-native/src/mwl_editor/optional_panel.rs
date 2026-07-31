use super::MwlEditor;
use crate::exanimation_form::{self, GlobalForm, RecordForm};
use eframe::egui;
use lm_app::MwlOptionalAssetsEdit;
use lm_graphics::{
    CompactExAnimation, ExAnimationFeature, ExAnimationFeatureOptions, ExAnimationFrame, Rgb8,
    exanimation_frames,
};
use lm_project::MwlOptionalLevelAssets;

#[derive(Default)]
pub(super) struct MwlOptionalAssetsPanel {
    tab: usize,
    loaded_revision: Option<u64>,
    selected_color: usize,
    palette_metadata: [String; 2],
    exanimation_metadata: [String; 2],
    exanimation_features: Option<ExAnimationFeatureOptions>,
    global: GlobalForm,
    trigger_index: usize,
    trigger_enabled: bool,
    trigger_value: String,
    record_index: usize,
    record: RecordForm,
    frame_index: usize,
    frame_words: String,
    frame_move_before: usize,
}

impl MwlOptionalAssetsPanel {
    pub(super) fn invalidate(&mut self) {
        self.loaded_revision = None;
    }

    fn load(&mut self, revision: u64, assets: &MwlOptionalLevelAssets, modes: &[bool; 256]) {
        if self.loaded_revision == Some(revision) {
            return;
        }
        self.palette_metadata = assets.palette_metadata.map(|value| format!("{value:08X}"));
        self.exanimation_metadata = assets
            .exanimation_metadata
            .map(|value| format!("{value:08X}"));
        self.exanimation_features = Some(ExAnimationFeatureOptions::decode(
            assets.exanimation_metadata[0].to_le_bytes()[0],
        ));
        if let Some(animation) = &assets.exanimation {
            self.global = GlobalForm::load(animation.setting, animation.header_value);
            self.load_trigger(animation);
            self.load_record(animation, modes);
        }
        self.loaded_revision = Some(revision);
    }

    fn load_trigger(&mut self, animation: &CompactExAnimation) {
        self.trigger_index = self.trigger_index.min(15);
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger_index) != 0;
        self.trigger_value = format!("{:02X}", animation.trigger_values[self.trigger_index]);
    }

    fn load_record(&mut self, animation: &CompactExAnimation, modes: &[bool; 256]) {
        self.record_index = self
            .record_index
            .min(animation.records.len().saturating_sub(1));
        if let Some(record) = animation.records.get(self.record_index) {
            let frames = exanimation_frames(record, modes[usize::from(record.size_mode())])
                .unwrap_or_default();
            self.record = RecordForm::load(record, &frames);
            self.frame_index = self.frame_index.min(frames.len().saturating_sub(1));
            self.frame_move_before = self.frame_move_before.min(frames.len());
            self.frame_words = frames
                .get(self.frame_index)
                .map(format_frame)
                .unwrap_or_default();
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        assets: &MwlOptionalLevelAssets,
        modes: &[bool; 256],
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        self.load(revision, assets, modes);
        ui.separator();
        ui.heading("Typed MWL optional assets");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, 0, "Palette");
            ui.selectable_value(&mut self.tab, 1, "ExAnimation");
        });
        if self.tab == 0 {
            self.palette(ui, assets)
        } else {
            self.animation(ui, assets, modes)
        }
    }

    fn palette(
        &mut self,
        ui: &mut egui::Ui,
        assets: &MwlOptionalLevelAssets,
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        if let Some(metadata) = metadata_fields(ui, "Palette metadata", &mut self.palette_metadata)
        {
            return Some(metadata.map(MwlOptionalAssetsEdit::SetPaletteMetadata));
        }
        let colors = &assets.palette.colors;
        self.selected_color = self.selected_color.min(colors.len().saturating_sub(1));
        egui::Grid::new("mwl-optional-palette").show(ui, |ui| {
            for (index, color) in colors.iter().copied().enumerate() {
                let rgb = color.to_rgb8();
                if ui
                    .add_sized(
                        [20.0, 20.0],
                        egui::Button::new("")
                            .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)),
                    )
                    .clicked()
                {
                    self.selected_color = index;
                }
                if index % 16 == 15 {
                    ui.end_row();
                }
            }
        });
        let color = colors.get(self.selected_color).copied()?;
        let rgb = color.to_rgb8();
        let mut value = [rgb.red, rgb.green, rgb.blue];
        ui.label(format!(
            "Color {:03X} / BGR555 {:04X}",
            self.selected_color, color.0
        ));
        ui.color_edit_button_srgb(&mut value).changed().then(|| {
            Ok(MwlOptionalAssetsEdit::SetPaletteColor {
                index: self.selected_color,
                color: lm_graphics::Bgr555::from_rgb8(Rgb8 {
                    red: value[0],
                    green: value[1],
                    blue: value[2],
                }),
            })
        })
    }

    fn animation(
        &mut self,
        ui: &mut egui::Ui,
        assets: &MwlOptionalLevelAssets,
        modes: &[bool; 256],
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        if let Some(metadata) =
            metadata_fields(ui, "ExAnimation metadata", &mut self.exanimation_metadata)
        {
            return Some(metadata.map(MwlOptionalAssetsEdit::SetExAnimationMetadata));
        }
        if let Some(features) = &mut self.exanimation_features {
            ui.heading("Super GFX Bypass animation options");
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
            if ui.button("Apply animation options").clicked() {
                return Some(Ok(MwlOptionalAssetsEdit::SetExAnimationFeatures(*features)));
            }
            ui.small(format!(
                "Preserved unrelated low nibble: {:X}",
                features.preserved_low_nibble
            ));
        }
        let Some(animation) = assets.exanimation.as_ref() else {
            return ui
                .button("Create empty ExAnimation section")
                .clicked()
                .then_some(Ok(MwlOptionalAssetsEdit::CreateExAnimation));
        };
        if let Some(edit) = self.animation_header(ui, animation) {
            return Some(edit);
        }
        self.animation_records(ui, animation, modes)
    }

    fn animation_header(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        text_field(ui, "Setting", &mut self.global.setting);
        text_field(ui, "Header", &mut self.global.header);
        if ui.button("Apply ExAnimation globals").clicked() {
            return Some(self.global.parse().map(|(setting, header_value)| {
                MwlOptionalAssetsEdit::SetExAnimationGlobals {
                    setting,
                    header_value,
                }
            }));
        }
        let previous_trigger = self.trigger_index;
        ui.add(egui::Slider::new(&mut self.trigger_index, 0..=15).text("Trigger"));
        if previous_trigger != self.trigger_index {
            self.load_trigger(animation);
        }
        ui.checkbox(&mut self.trigger_enabled, "Trigger enabled");
        ui.add_enabled(
            self.trigger_enabled,
            egui::TextEdit::singleline(&mut self.trigger_value),
        );
        ui.button("Apply trigger").clicked().then(|| {
            let value = if self.trigger_enabled {
                exanimation_form::hex_u8(&self.trigger_value, "trigger value")
            } else {
                Ok(0)
            };
            value.map(|value| MwlOptionalAssetsEdit::SetTrigger {
                index: self.trigger_index,
                value: self.trigger_enabled.then_some(value),
            })
        })
    }

    fn animation_records(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        ui.separator();
        let len = animation.records.len();
        let previous = self.record_index;
        ui.add(egui::DragValue::new(&mut self.record_index).range(0..=len));
        if previous != self.record_index {
            self.load_record(animation, modes);
        }
        for (label, field) in [
            ("Kind", &mut self.record.kind),
            ("Size mode", &mut self.record.size_mode),
            ("Destination", &mut self.record.destination),
        ] {
            text_field(ui, label, field);
        }
        ui.checkbox(&mut self.record.destination_flag, "Destination flag");
        ui.label("Source words, one frame per line");
        ui.add(
            egui::TextEdit::multiline(&mut self.record.frames)
                .desired_rows(5)
                .code_editor(),
        );
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Append record").clicked() {
                action = Some(0);
            }
            if ui
                .add_enabled(self.record_index < len, egui::Button::new("Replace record"))
                .clicked()
            {
                action = Some(1);
            }
            if ui
                .add_enabled(self.record_index < len, egui::Button::new("Remove record"))
                .clicked()
            {
                action = Some(2);
            }
        });
        action
            .map(|action| {
                let record = (action != 2)
                    .then(|| self.record.parse(modes))
                    .transpose()?;
                Ok(match action {
                    0 => MwlOptionalAssetsEdit::InsertRecord {
                        index: len,
                        record: record.expect("append record"),
                    },
                    1 => MwlOptionalAssetsEdit::ReplaceRecord {
                        index: self.record_index,
                        record: record.expect("replace record"),
                    },
                    _ => MwlOptionalAssetsEdit::RemoveRecord {
                        index: self.record_index,
                    },
                })
            })
            .or_else(|| self.animation_frame(ui, animation, modes))
    }

    fn animation_frame(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
    ) -> Option<Result<MwlOptionalAssetsEdit, String>> {
        let record = animation.records.get(self.record_index)?;
        let frames = exanimation_frames(record, modes[usize::from(record.size_mode())]).ok()?;
        let previous = self.frame_index;
        ui.separator();
        ui.label("Semantic frame edit");
        ui.add(egui::DragValue::new(&mut self.frame_index).range(0..=frames.len()));
        if previous != self.frame_index {
            self.frame_words = frames
                .get(self.frame_index)
                .map(format_frame)
                .unwrap_or_default();
        }
        text_field(ui, "Source word(s)", &mut self.frame_words);
        ui.add(
            egui::DragValue::new(&mut self.frame_move_before)
                .range(0..=frames.len())
                .prefix("Move before "),
        );
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Insert frame").clicked() {
                action = Some(0);
            }
            if ui
                .add_enabled(
                    self.frame_index < frames.len(),
                    egui::Button::new("Replace frame"),
                )
                .clicked()
            {
                action = Some(1);
            }
            if ui
                .add_enabled(
                    self.frame_index < frames.len(),
                    egui::Button::new("Remove frame"),
                )
                .clicked()
            {
                action = Some(2);
            }
            if ui
                .add_enabled(
                    self.frame_index < frames.len(),
                    egui::Button::new("Move frame"),
                )
                .clicked()
            {
                action = Some(3);
            }
        });
        action.map(|action| {
            let frame = (action < 2)
                .then(|| parse_frame(&self.frame_words))
                .transpose()?;
            Ok(match action {
                0 => MwlOptionalAssetsEdit::InsertFrame {
                    record: self.record_index,
                    index: self.frame_index,
                    frame: frame.expect("insert frame"),
                },
                1 => MwlOptionalAssetsEdit::ReplaceFrame {
                    record: self.record_index,
                    index: self.frame_index,
                    frame: frame.expect("replace frame"),
                },
                2 => MwlOptionalAssetsEdit::RemoveFrame {
                    record: self.record_index,
                    index: self.frame_index,
                },
                _ => MwlOptionalAssetsEdit::MoveFrameBefore {
                    record: self.record_index,
                    from: self.frame_index,
                    before: self.frame_move_before,
                },
            })
        })
    }
}

impl MwlEditor {
    pub(super) fn show_optional_assets_panel(&mut self, ui: &mut egui::Ui) {
        let Some(interpretation) = self.optional_interpretation.as_ref() else {
            return;
        };
        let result = self.controller.as_ref().map(|controller| {
            let assets = MwlOptionalLevelAssets::decode(
                controller.value(),
                interpretation.maximum_records,
                &interpretation.modes,
            )
            .map_err(|error| error.to_string());
            assets.and_then(|assets| {
                self.optional_panel
                    .show(ui, controller.revision(), &assets, &interpretation.modes)
                    .transpose()
            })
        });
        match result {
            Some(Ok(Some(edit))) => self.apply_optional_assets_edit(&edit),
            Some(Ok(None)) | None => {}
            Some(Err(error)) => self.error = Some(error),
        }
    }

    fn apply_optional_assets_edit(&mut self, edit: &MwlOptionalAssetsEdit) {
        let (Some(controller), Some(interpretation)) = (
            self.controller.as_mut(),
            self.optional_interpretation.as_ref(),
        ) else {
            return;
        };
        match controller.apply_optional_assets_edits(
            controller.revision(),
            interpretation.maximum_records,
            &interpretation.modes,
            std::slice::from_ref(edit),
        ) {
            Ok(()) => {
                self.optional_panel.invalidate();
                self.invalidate();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

fn metadata_fields(
    ui: &mut egui::Ui,
    label: &str,
    fields: &mut [String; 2],
) -> Option<Result<[u32; 2], String>> {
    ui.label(label);
    text_field(ui, "Word 0", &mut fields[0]);
    text_field(ui, "Word 1", &mut fields[1]);
    ui.button("Apply metadata").clicked().then(|| {
        Ok([
            parse_hex_u32(&fields[0], "metadata word 0")?,
            parse_hex_u32(&fields[1], "metadata word 1")?,
        ])
    })
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

fn parse_hex_u32(text: &str, name: &str) -> Result<u32, String> {
    u32::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

fn parse_frame(text: &str) -> Result<ExAnimationFrame, String> {
    let source_words = text
        .split_whitespace()
        .map(|word| exanimation_form::hex_u16(word, "frame source word"))
        .collect::<Result<Vec<_>, _>>()?;
    if !(1..=2).contains(&source_words.len()) {
        return Err(format!(
            "a frame requires one or two source words, found {}",
            source_words.len()
        ));
    }
    Ok(ExAnimationFrame { source_words })
}

fn format_frame(frame: &ExAnimationFrame) -> String {
    frame
        .source_words
        .iter()
        .map(|word| format!("{word:04X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, ExAnimationRecord, Palette};

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [1, 2],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [3, 4],
            exanimation: Some(CompactExAnimation {
                setting: 5,
                header_value: 6,
                trigger_mask: 1,
                trigger_values: [7; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn semantic_panel_uses_shared_toolkit_neutral_edits() {
        let edit = MwlOptionalAssetsEdit::SetExAnimationGlobals {
            setting: 9,
            header_value: 10,
        };
        assert!(matches!(
            edit,
            MwlOptionalAssetsEdit::SetExAnimationGlobals {
                setting: 9,
                header_value: 10
            }
        ));
        assert_eq!(assets().palette.colors.len(), 257);
    }

    #[test]
    fn metadata_parser_rejects_non_hex_without_partial_values() {
        assert_eq!(parse_hex_u32("1234ABCD", "word").unwrap(), 0x1234_abcd);
        assert!(parse_hex_u32("no", "word").is_err());
    }

    #[test]
    fn semantic_frame_form_emits_shared_source_words() {
        assert_eq!(
            parse_frame("1234 ABCD").unwrap().source_words,
            [0x1234, 0xabcd]
        );
        assert!(parse_frame("").is_err());
        assert!(parse_frame("1 2 3").is_err());
    }
}
