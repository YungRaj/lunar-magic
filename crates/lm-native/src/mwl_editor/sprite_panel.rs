use crate::level_editor_forms;
use eframe::egui;
use lm_app::MwlDocumentController;
use lm_level::{NativeSpriteStream, SpriteLengthTable, SpriteRecord, SpriteToken};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenKind {
    Record,
    Screen,
    Control,
}

pub(super) struct MwlSpritePanel {
    expanded: bool,
    loaded: Option<(u64, bool)>,
    stream: Option<NativeSpriteStream>,
    selected: usize,
    header: String,
    kind: TokenKind,
    value: String,
    lengths: SpriteLengthTable,
    length_table: String,
    length_id: String,
    length_value: String,
}

impl Default for MwlSpritePanel {
    fn default() -> Self {
        Self {
            expanded: false,
            loaded: None,
            stream: None,
            selected: 0,
            header: String::new(),
            kind: TokenKind::Record,
            value: String::new(),
            lengths: SpriteLengthTable::standard(),
            length_table: "0".into(),
            length_id: "00".into(),
            length_value: "03".into(),
        }
    }
}

impl MwlSpritePanel {
    pub(super) fn invalidate(&mut self) {
        self.loaded = None;
        self.stream = None;
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        controller: &mut MwlDocumentController,
    ) -> Result<bool, String> {
        ui.collapsing("Typed sprite stream", |ui| {
            let changed = ui
                .checkbox(&mut self.expanded, "Expanded sprite framing")
                .changed();
            if changed {
                self.invalidate();
            }
            self.length_controls(ui)?;
            self.ensure_loaded(controller)?;
            self.contents(ui, controller)
        })
        .body_returned
        .transpose()
        .map(Option::unwrap_or_default)
    }

    fn ensure_loaded(&mut self, controller: &MwlDocumentController) -> Result<(), String> {
        let key = (controller.revision(), self.expanded);
        if self.loaded == Some(key) {
            return Ok(());
        }
        let stream = controller
            .sprites(self.expanded, &self.lengths)
            .map_err(|error| error.to_string())?;
        self.stream = Some(stream);
        self.loaded = Some(key);
        self.selected = 0;
        self.load_form();
        Ok(())
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        controller: &mut MwlDocumentController,
    ) -> Result<bool, String> {
        let stream = self.stream.as_ref().expect("loaded sprite stream");
        ui.label(format!(
            "{} tokens; revision length-table interpretation",
            stream.tokens.len()
        ));
        ui.horizontal(|ui| {
            ui.label("Header (hex)");
            ui.text_edit_singleline(&mut self.header);
        });
        if ui.button("Stage header").clicked() {
            self.stream.as_mut().expect("loaded sprite stream").header =
                level_editor_forms::parse_hex_u8(&self.header, "sprite header")?;
        }

        let mut new_selection = None;
        egui::ScrollArea::vertical()
            .id_salt("mwl-sprite-tokens")
            .max_height(180.0)
            .show(ui, |ui| {
                let tokens = &self.stream.as_ref().expect("loaded sprite stream").tokens;
                for (index, token) in tokens.iter().enumerate() {
                    if ui
                        .selectable_label(self.selected == index, token_label(index, token))
                        .clicked()
                    {
                        new_selection = Some(index);
                    }
                }
            });
        if let Some(index) = new_selection {
            self.selected = index;
            self.load_form();
        }

        egui::ComboBox::from_id_salt("mwl-sprite-token-kind")
            .selected_text(match self.kind {
                TokenKind::Record => "Record bytes",
                TokenKind::Screen => "Screen token",
                TokenKind::Control => "Control token",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.kind, TokenKind::Record, "Record bytes");
                ui.selectable_value(&mut self.kind, TokenKind::Screen, "Screen token");
                ui.selectable_value(&mut self.kind, TokenKind::Control, "Control token");
            });
        ui.text_edit_singleline(&mut self.value);
        self.token_controls(ui)?;
        self.reorder_controls(ui)?;

        let mut committed = false;
        if ui.button("Commit typed sprite stream").clicked() {
            controller
                .replace_sprites(
                    controller.revision(),
                    self.stream.as_ref().expect("loaded sprite stream"),
                    &self.lengths,
                )
                .map_err(|error| error.to_string())?;
            self.invalidate();
            committed = true;
        }
        Ok(committed)
    }

    fn length_controls(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        ui.label("Sprite record-length interpretation (table, sprite ID, bytes; hex):");
        let mut apply = false;
        let mut reset = false;
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.length_table);
            ui.text_edit_singleline(&mut self.length_id);
            ui.text_edit_singleline(&mut self.length_value);
            apply = ui.button("Set length").clicked();
            reset = ui.button("Reset standard lengths").clicked();
        });
        if apply {
            self.apply_length_form()?;
            self.invalidate();
        }
        if reset {
            self.lengths = SpriteLengthTable::standard();
            self.invalidate();
        }
        Ok(())
    }

    fn apply_length_form(&mut self) -> Result<(), String> {
        let table =
            level_editor_forms::parse_hex_u8(&self.length_table, "sprite length-table selector")?;
        let id = level_editor_forms::parse_hex_u8(&self.length_id, "sprite ID")?;
        let length = level_editor_forms::parse_hex_u8(&self.length_value, "sprite record length")?;
        self.lengths
            .set(table, id, length)
            .map_err(|error| error.to_string())
    }

    fn token_controls(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button("Insert before").clicked() {
                let token = self.form_token()?;
                let stream = self.stream.as_mut().expect("loaded sprite stream");
                let index = self.selected.min(stream.tokens.len());
                stream
                    .insert(index, token)
                    .map_err(|error| error.to_string())?;
                self.selected = index;
                self.load_form();
            }
            if ui.button("Replace").clicked() {
                let token = self.form_token()?;
                let stream = self.stream.as_mut().expect("loaded sprite stream");
                if self.selected >= stream.tokens.len() {
                    return Err("select an existing sprite token to replace".into());
                }
                stream
                    .remove(self.selected)
                    .map_err(|error| error.to_string())?;
                stream
                    .insert(self.selected, token)
                    .map_err(|error| error.to_string())?;
                self.load_form();
            }
            if ui.button("Delete").clicked() {
                let stream = self.stream.as_mut().expect("loaded sprite stream");
                stream
                    .remove(self.selected)
                    .map_err(|error| error.to_string())?;
                self.selected = self.selected.min(stream.tokens.len().saturating_sub(1));
                self.load_form();
            }
            Ok(())
        })
        .inner?;
        Ok(())
    }

    fn reorder_controls(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button("Move up").clicked() && self.selected > 0 {
                let before = self.selected - 1;
                self.stream
                    .as_mut()
                    .expect("loaded sprite stream")
                    .move_before(self.selected, before)
                    .map_err(|error| error.to_string())?;
                self.selected = before;
            }
            let len = self
                .stream
                .as_ref()
                .expect("loaded sprite stream")
                .tokens
                .len();
            if ui.button("Move down").clicked() && self.selected + 1 < len {
                let destination = self.selected + 2;
                self.stream
                    .as_mut()
                    .expect("loaded sprite stream")
                    .move_before(self.selected, destination)
                    .map_err(|error| error.to_string())?;
                self.selected += 1;
            }
            Ok(())
        })
        .inner?;
        Ok(())
    }

    fn load_form(&mut self) {
        let Some(stream) = self.stream.as_ref() else {
            return;
        };
        self.header = format!("{:02X}", stream.header);
        let Some(token) = stream.tokens.get(self.selected) else {
            self.kind = TokenKind::Record;
            self.value.clear();
            return;
        };
        match token {
            SpriteToken::Record(record) => {
                self.kind = TokenKind::Record;
                self.value = level_editor_forms::format_bytes(&record.encoded);
            }
            SpriteToken::Screen(value) => {
                self.kind = TokenKind::Screen;
                self.value = format!("{value:02X}");
            }
            SpriteToken::Control(value) => {
                self.kind = TokenKind::Control;
                self.value = format!("{value:02X}");
            }
        }
    }

    fn form_token(&self) -> Result<SpriteToken, String> {
        match self.kind {
            TokenKind::Record => Ok(SpriteToken::Record(SpriteRecord {
                encoded: level_editor_forms::parse_bytes(&self.value, "sprite record byte")?,
            })),
            TokenKind::Screen => Ok(SpriteToken::Screen(level_editor_forms::parse_hex_u8(
                &self.value,
                "sprite screen token",
            )?)),
            TokenKind::Control => Ok(SpriteToken::Control(level_editor_forms::parse_hex_u8(
                &self.value,
                "sprite control token",
            )?)),
        }
    }
}

fn token_label(index: usize, token: &SpriteToken) -> String {
    match token {
        SpriteToken::Record(record) => format!(
            "{index:03}: record {}",
            level_editor_forms::format_bytes(&record.encoded)
        ),
        SpriteToken::Screen(value) => format!("{index:03}: screen {value:02X}"),
        SpriteToken::Control(value) => format!("{index:03}: control {value:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_forms_preserve_record_and_control_kinds() {
        let mut panel = MwlSpritePanel {
            kind: TokenKind::Record,
            value: "11 D0 BD".into(),
            ..MwlSpritePanel::default()
        };
        assert!(matches!(
            panel.form_token().unwrap(),
            SpriteToken::Record(SpriteRecord { encoded }) if encoded == [0x11, 0xd0, 0xbd]
        ));
        panel.kind = TokenKind::Screen;
        panel.value = "7F".into();
        assert_eq!(panel.form_token().unwrap(), SpriteToken::Screen(0x7f));
        panel.kind = TokenKind::Control;
        panel.value = "90".into();
        assert_eq!(panel.form_token().unwrap(), SpriteToken::Control(0x90));
    }

    #[test]
    fn invalid_token_forms_are_rejected_before_staging() {
        let mut panel = MwlSpritePanel {
            kind: TokenKind::Record,
            value: "GG".into(),
            ..MwlSpritePanel::default()
        };
        assert!(panel.form_token().is_err());
        panel.kind = TokenKind::Screen;
        panel.value = "100".into();
        assert!(panel.form_token().is_err());
    }

    #[test]
    fn custom_length_form_updates_only_the_addressed_revision_entry() {
        let mut panel = MwlSpritePanel {
            length_table: "2".into(),
            length_id: "42".into(),
            length_value: "05".into(),
            ..MwlSpritePanel::default()
        };
        panel.apply_length_form().unwrap();
        assert_eq!(panel.lengths.record_len(&[0x08, 0x20, 0x42]), Some(5));
        assert_eq!(panel.lengths.record_len(&[0x00, 0x20, 0x42]), Some(3));

        panel.length_table = "4".into();
        assert!(panel.apply_length_form().is_err());
        assert_eq!(panel.lengths.record_len(&[0x08, 0x20, 0x42]), Some(5));
    }
}
