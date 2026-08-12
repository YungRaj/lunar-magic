use crate::overworld_editor_render::{OverworldAnimationDomain, OverworldAnimationOwner};
use crate::{
    exanimation_form::{GlobalForm, RecordForm},
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    ExAnimationControllerEdit, ExtendedUiTextKey, LocalizationCatalog, OverworldControllerEdit,
};
use lm_graphics::{CompactExAnimation, ExAnimationFrameEdit, exanimation_frames};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Record,
    Frame,
}

#[derive(Default)]
pub(crate) struct OverworldAnimationPanel {
    loaded_revision: Option<u64>,
    loaded_record: Option<usize>,
    selected: usize,
    selected_frame: usize,
    global: GlobalForm,
    record: RecordForm,
    record_editable: bool,
    trigger: usize,
    trigger_enabled: bool,
    trigger_value: u8,
    paste_target: Option<PasteTarget>,
    domain: OverworldAnimationDomain,
}

impl OverworldAnimationPanel {
    pub(crate) fn invalidate(&mut self) {
        self.loaded_revision = None;
        self.loaded_record = None;
        self.paste_target = None;
    }

    pub(crate) fn navigate(&mut self, owner: OverworldAnimationOwner) {
        self.domain = owner.domain;
        self.selected = owner.record;
        self.loaded_record = None;
        self.loaded_revision = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        global_animation: Option<&CompactExAnimation>,
        modes: &[bool; 256],
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        if self.domain == OverworldAnimationDomain::Global && global_animation.is_none() {
            self.domain = OverworldAnimationDomain::Local;
        }
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.domain,
                OverworldAnimationDomain::Local,
                text(catalog, ExtendedUiTextKey::OverworldAnimationThisMap),
            );
            ui.add_enabled_ui(global_animation.is_some(), |ui| {
                ui.selectable_value(
                    &mut self.domain,
                    OverworldAnimationDomain::Global,
                    text(catalog, ExtendedUiTextKey::OverworldAnimationGlobal),
                );
            });
        });
        let displayed = match self.domain {
            OverworldAnimationDomain::Local => animation,
            OverworldAnimationDomain::Global => global_animation.unwrap_or(animation),
        };
        let editable = self.domain == OverworldAnimationDomain::Local;
        self.load(displayed, modes, revision);
        if !editable {
            ui.small(text(
                catalog,
                ExtendedUiTextKey::OverworldAnimationGlobalReadOnly,
            ));
        }
        ui.add_enabled_ui(editable, |ui| self.domain_ui(ui, displayed, modes, catalog))
            .inner
    }

    fn domain_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::OverworldAnimationSetting));
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::OverworldAnimationHeader));
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::OverworldAnimationApplyGlobals,
            ))
            .clicked()
        {
            return Some(self.global.parse().map(|(setting, header)| {
                OverworldControllerEdit::Animation(vec![
                    ExAnimationControllerEdit::SetSetting(setting),
                    ExAnimationControllerEdit::SetHeaderValue(header),
                ])
            }));
        }
        ui.separator();
        self.trigger_ui(ui, animation, catalog);
        ui.separator();
        self.record_ui(ui, animation, modes, catalog)
    }

    fn trigger_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if ui
            .add(
                egui::Slider::new(&mut self.trigger, 0..=15)
                    .text(text(catalog, ExtendedUiTextKey::OverworldAnimationTrigger)),
            )
            .changed()
        {
            self.load_trigger(animation);
        }
        ui.checkbox(
            &mut self.trigger_enabled,
            text(catalog, ExtendedUiTextKey::OverworldAnimationEnabled),
        );
        ui.add_enabled(
            self.trigger_enabled,
            egui::Slider::new(&mut self.trigger_value, 0..=u8::MAX)
                .text(text(catalog, ExtendedUiTextKey::OverworldAnimationValue)),
        );
    }

    fn record_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let records = &animation.records;
        if !records.is_empty() {
            self.selected = self.selected.min(records.len() - 1);
        }
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::OverworldAnimationApplyTrigger,
            ))
            .clicked()
        {
            return Some(Ok(OverworldControllerEdit::Animation(vec![
                ExAnimationControllerEdit::SetTrigger {
                    trigger: self.trigger,
                    value: self.trigger_enabled.then_some(self.trigger_value),
                },
            ])));
        }
        ui.add(
            egui::Slider::new(&mut self.selected, 0..=records.len().saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::OverworldAnimationRecord)),
        );
        if self.loaded_record != Some(self.selected) {
            self.load_record(animation, modes);
        }
        for (label, field) in [
            (
                ExtendedUiTextKey::OverworldAnimationKind,
                &mut self.record.kind,
            ),
            (
                ExtendedUiTextKey::OverworldAnimationRecordTrigger,
                &mut self.record.size_mode,
            ),
            (
                ExtendedUiTextKey::OverworldAnimationDestination,
                &mut self.record.destination,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(text(catalog, label));
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(
            &mut self.record.destination_flag,
            text(
                catalog,
                ExtendedUiTextKey::OverworldAnimationDestinationFlag,
            ),
        );
        ui.label(text(
            catalog,
            ExtendedUiTextKey::OverworldAnimationSourceWords,
        ));
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(6));
        if !self.record_editable && !records.is_empty() {
            ui.label(text(
                catalog,
                ExtendedUiTextKey::OverworldAnimationSpecialNotice,
            ));
        }
        if let Some(edit) = self.clipboard_ui(ui, animation, modes, catalog) {
            return Some(edit);
        }
        let mut operation = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::OverworldAnimationAppend))
                .clicked()
            {
                operation = Some(RecordOperation::Append);
            }
            if ui
                .add_enabled(
                    !records.is_empty() && self.record_editable,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::OverworldAnimationReplace)),
                )
                .clicked()
            {
                operation = Some(RecordOperation::Replace);
            }
            if ui
                .add_enabled(
                    !records.is_empty(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::OverworldAnimationRemove)),
                )
                .clicked()
            {
                operation = Some(RecordOperation::Remove);
            }
        });
        operation.map(|operation| self.record_edit(operation, records.len(), modes))
    }

    fn clipboard_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let record = animation.records.get(self.selected);
        let frames = record
            .and_then(|record| {
                exanimation_frames(record, modes[usize::from(record.size_mode())]).ok()
            })
            .unwrap_or_default();
        self.selected_frame = self.selected_frame.min(frames.len().saturating_sub(1));
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    record.is_some(),
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::OverworldAnimationCopyRecord,
                    )),
                )
                .clicked()
                && let Some(record) = record
            {
                copy_result = Some(native_clipboard::encode_exanimation_record(record));
            }
            if ui
                .add_enabled(
                    record.is_some(),
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::OverworldAnimationPasteRecord,
                    )),
                )
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Record);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            ui.add(
                egui::DragValue::new(&mut self.selected_frame)
                    .range(0..=frames.len().saturating_sub(1))
                    .prefix(text(
                        catalog,
                        ExtendedUiTextKey::OverworldAnimationFramePrefix,
                    )),
            );
            if ui
                .add_enabled(
                    !frames.is_empty(),
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::OverworldAnimationCopyFrame,
                    )),
                )
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_exanimation_frame(
                    &frames[self.selected_frame],
                ));
            }
            if ui
                .add_enabled(
                    !frames.is_empty(),
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::OverworldAnimationPasteFrame,
                    )),
                )
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Frame);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
        }
        let text = pasted_text(ui)?;
        let edit = match self.paste_target.take()? {
            PasteTarget::Record => {
                native_clipboard::decode_exanimation_record(&text).map(|record| {
                    ExAnimationControllerEdit::ReplaceRecord {
                        index: self.selected,
                        record,
                    }
                })
            }
            PasteTarget::Frame => native_clipboard::decode_exanimation_frame(&text).map(|frame| {
                ExAnimationControllerEdit::EditRecordFrames {
                    record: self.selected,
                    edits: vec![ExAnimationFrameEdit::Replace {
                        index: self.selected_frame,
                        frame,
                    }],
                }
            }),
        };
        Some(edit.map(|edit| OverworldControllerEdit::Animation(vec![edit])))
    }

    fn record_edit(
        &self,
        operation: RecordOperation,
        len: usize,
        modes: &[bool; 256],
    ) -> Result<OverworldControllerEdit, String> {
        let edit = match operation {
            RecordOperation::Append => ExAnimationControllerEdit::InsertRecord {
                index: len,
                record: self.record.parse(modes)?,
            },
            RecordOperation::Replace => ExAnimationControllerEdit::ReplaceRecord {
                index: self.selected,
                record: self.record.parse(modes)?,
            },
            RecordOperation::Remove => ExAnimationControllerEdit::RemoveRecord {
                index: self.selected,
            },
        };
        Ok(OverworldControllerEdit::Animation(vec![edit]))
    }

    fn load(&mut self, animation: &CompactExAnimation, modes: &[bool; 256], revision: u64) {
        let domain_revision = revision
            .wrapping_mul(2)
            .wrapping_add(u64::from(self.domain == OverworldAnimationDomain::Global));
        if self.loaded_revision == Some(domain_revision) {
            return;
        }
        self.global = GlobalForm::load(animation.setting, animation.header_value);
        self.loaded_revision = Some(domain_revision);
        self.loaded_record = None;
        self.load_trigger(animation);
        self.load_record(animation, modes);
    }

    fn load_trigger(&mut self, animation: &CompactExAnimation) {
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger) != 0;
        self.trigger_value = animation.trigger_values[self.trigger];
    }

    fn load_record(&mut self, animation: &CompactExAnimation, modes: &[bool; 256]) {
        if animation.records.is_empty() {
            self.record = RecordForm::default();
            self.record_editable = true;
            self.loaded_record = Some(0);
            return;
        }
        self.selected = self.selected.min(animation.records.len() - 1);
        let record = &animation.records[self.selected];
        if let Ok(frames) = exanimation_frames(record, modes[usize::from(record.size_mode())]) {
            self.record = RecordForm::load(record, &frames);
            self.record_editable = true;
        } else {
            self.record = RecordForm::load(record, &[]);
            self.record_editable = false;
        }
        self.loaded_record = Some(self.selected);
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn complete_overworld_animation_panel_uses_every_typed_key() {
        let source = include_str!("overworld_editor_animation.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .take_while(|key| *key != ExtendedUiTextKey::OverworldAnimationOptionsHeading)
            .filter(|key| format!("{key:?}").starts_with("OverworldAnimation"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for bypass in [
            "ui.button(\"Apply animation globals\")",
            "ui.button(\"Apply trigger\")",
            "egui::Button::new(\"Copy record\")",
            "egui::Button::new(\"Paste frame\")",
            "egui::Button::new(\"Remove\")",
        ] {
            assert!(!source.contains(bypass));
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

#[derive(Clone, Copy)]
enum RecordOperation {
    Append,
    Replace,
    Remove,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_navigation_selects_the_exact_local_or_global_record_and_invalidates_forms() {
        let mut panel = OverworldAnimationPanel {
            loaded_revision: Some(9),
            loaded_record: Some(1),
            ..Default::default()
        };
        panel.navigate(OverworldAnimationOwner {
            domain: OverworldAnimationDomain::Global,
            record: 0x1f,
        });
        assert_eq!(panel.domain, OverworldAnimationDomain::Global);
        assert_eq!(panel.selected, 0x1f);
        assert_eq!(panel.loaded_revision, None);
        assert_eq!(panel.loaded_record, None);

        panel.navigate(OverworldAnimationOwner {
            domain: OverworldAnimationDomain::Local,
            record: 3,
        });
        assert_eq!(panel.domain, OverworldAnimationDomain::Local);
        assert_eq!(panel.selected, 3);
    }
}
