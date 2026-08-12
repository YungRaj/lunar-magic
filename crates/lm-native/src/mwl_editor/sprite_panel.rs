use super::OptionalCatalogText;
use crate::{
    level_editor_forms,
    native_level_document_form::{NativeSpriteHeaderForm, show_sprite_header_form},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, MwlDocumentController};
use lm_level::{
    NativeSpriteRecordFields, NativeSpriteStream, SpriteLengthTable, SpriteRecord, SpriteToken,
};

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
    header: NativeSpriteHeaderForm,
    kind: TokenKind,
    value: String,
    lengths: SpriteLengthTable,
    length_table: String,
    length_id: String,
    length_value: String,
    y_low: String,
    extra_bits: String,
    screen: String,
    x: String,
    sprite_number: String,
}

impl Default for MwlSpritePanel {
    fn default() -> Self {
        Self {
            expanded: false,
            loaded: None,
            stream: None,
            selected: 0,
            header: NativeSpriteHeaderForm::default(),
            kind: TokenKind::Record,
            value: String::new(),
            lengths: SpriteLengthTable::standard(),
            length_table: "0".into(),
            length_id: "00".into(),
            length_value: "03".into(),
            y_low: String::new(),
            extra_bits: String::new(),
            screen: String::new(),
            x: String::new(),
            sprite_number: String::new(),
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<bool, String> {
        ui.collapsing(catalog.extended_text(Key::MwlSpriteHeading), |ui| {
            let changed = ui
                .checkbox(
                    &mut self.expanded,
                    catalog.extended_text(Key::MwlSpriteExpanded),
                )
                .changed();
            if changed {
                self.invalidate();
            }
            self.length_controls(ui, catalog)?;
            self.ensure_loaded(controller)?;
            self.contents(ui, controller, catalog)
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<bool, String> {
        let stream = self.stream.as_ref().expect("loaded sprite stream");
        ui.label(
            catalog
                .extended_text(Key::MwlSpriteTokenCountFormat)
                .replace("{count}", &stream.tokens.len().to_string()),
        );
        show_sprite_header_form(ui, "mwl-sprite-header", &mut self.header);
        if ui
            .button(catalog.extended_text(Key::MwlSpriteStageHeader))
            .clicked()
        {
            self.stream.as_mut().expect("loaded sprite stream").header =
                self.header.header().map_err(|error| error.to_string())?;
        }

        let mut new_selection = None;
        egui::ScrollArea::vertical()
            .id_salt("mwl-sprite-tokens")
            .max_height(180.0)
            .show(ui, |ui| {
                let tokens = &self.stream.as_ref().expect("loaded sprite stream").tokens;
                let placements = self
                    .stream
                    .as_ref()
                    .expect("loaded sprite stream")
                    .native_placements();
                for (index, token) in tokens.iter().enumerate() {
                    let placement = placements
                        .iter()
                        .find(|placement| placement.token_index == index);
                    if ui
                        .selectable_label(
                            self.selected == index,
                            token_label(index, token, placement),
                        )
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
                TokenKind::Record => catalog.extended_text(Key::MwlSpriteRecordBytes),
                TokenKind::Screen => catalog.extended_text(Key::MwlSpriteUpperYToken),
                TokenKind::Control => catalog.extended_text(Key::MwlSpriteControlToken),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.kind,
                    TokenKind::Record,
                    catalog.extended_text(Key::MwlSpriteRecordBytes),
                );
                ui.selectable_value(
                    &mut self.kind,
                    TokenKind::Screen,
                    catalog.extended_text(Key::MwlSpriteUpperYToken),
                );
                ui.selectable_value(
                    &mut self.kind,
                    TokenKind::Control,
                    catalog.extended_text(Key::MwlSpriteControlToken),
                );
            });
        ui.text_edit_singleline(&mut self.value);
        self.token_controls(ui, catalog)?;
        self.reorder_controls(ui, catalog)?;
        self.semantic_controls(ui, catalog)?;

        let mut committed = false;
        if ui
            .button(catalog.extended_text(Key::MwlSpriteCommit))
            .clicked()
        {
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

    fn length_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.label(catalog.extended_text(Key::MwlSpriteLengthNotice));
        let mut apply = false;
        let mut reset = false;
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.length_table);
            ui.text_edit_singleline(&mut self.length_id);
            ui.text_edit_singleline(&mut self.length_value);
            apply = ui
                .button(catalog.extended_text(Key::MwlSpriteSetLength))
                .clicked();
            reset = ui
                .button(catalog.extended_text(Key::MwlSpriteResetLengths))
                .clicked();
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

    fn semantic_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        if !matches!(
            self.stream
                .as_ref()
                .and_then(|stream| stream.tokens.get(self.selected)),
            Some(SpriteToken::Record(_))
        ) {
            return Ok(());
        }
        ui.label(catalog.extended_text(Key::MwlSpriteRecoveredFields));
        egui::Grid::new("mwl-sprite-semantic-fields")
            .num_columns(2)
            .show(ui, |ui| {
                field(
                    ui,
                    catalog.extended_text(Key::MwlSpriteYLow),
                    &mut self.y_low,
                );
                field(
                    ui,
                    catalog.extended_text(Key::MwlSpriteExtraBits),
                    &mut self.extra_bits,
                );
                field(
                    ui,
                    catalog.extended_text(Key::MwlSpriteScreen),
                    &mut self.screen,
                );
                field(ui, catalog.extended_text(Key::MwlSpriteX), &mut self.x);
                field(
                    ui,
                    catalog.extended_text(Key::MwlSpriteNumber),
                    &mut self.sprite_number,
                );
            });
        if ui
            .button(catalog.extended_text(Key::MwlSpriteStageFields))
            .clicked()
        {
            self.stage_semantic_fields()?;
            self.load_form();
        }
        Ok(())
    }

    fn stage_semantic_fields(&mut self) -> Result<(), String> {
        let fields = NativeSpriteRecordFields {
            y_low: level_editor_forms::parse_hex_u8(&self.y_low, "sprite Y low bits")?,
            extra_bits: level_editor_forms::parse_hex_u8(&self.extra_bits, "sprite extra bits")?,
            screen: level_editor_forms::parse_hex_u8(&self.screen, "sprite screen")?,
            x: level_editor_forms::parse_hex_u8(&self.x, "sprite X")?,
            sprite_number: level_editor_forms::parse_hex_u8(&self.sprite_number, "sprite number")?,
        };
        let Some(SpriteToken::Record(record)) = self
            .stream
            .as_mut()
            .expect("loaded sprite stream")
            .tokens
            .get_mut(self.selected)
        else {
            return Err("select an ordinary sprite record to edit fields".into());
        };
        record
            .set_native_fields(fields, &self.lengths)
            .map_err(|error| error.to_string())
    }

    fn token_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui
                .button(catalog.extended_text(Key::MwlInsertBefore))
                .clicked()
            {
                let token = self.form_token()?;
                let stream = self.stream.as_mut().expect("loaded sprite stream");
                let index = self.selected.min(stream.tokens.len());
                stream
                    .insert(index, token)
                    .map_err(|error| error.to_string())?;
                self.selected = index;
                self.load_form();
            }
            if ui.button(catalog.extended_text(Key::MwlReplace)).clicked() {
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
            if ui.button(catalog.extended_text(Key::MwlDelete)).clicked() {
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

    fn reorder_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button(catalog.extended_text(Key::MwlMoveUp)).clicked() && self.selected > 0 {
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
            if ui.button(catalog.extended_text(Key::MwlMoveDown)).clicked()
                && self.selected + 1 < len
            {
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
        self.header = NativeSpriteHeaderForm::load(stream.header);
        let Some(token) = stream.tokens.get(self.selected) else {
            self.kind = TokenKind::Record;
            self.value.clear();
            self.clear_semantic_form();
            return;
        };
        match token {
            SpriteToken::Record(record) => {
                self.kind = TokenKind::Record;
                self.value = level_editor_forms::format_bytes(&record.encoded);
                if let Ok(fields) = record.native_fields() {
                    self.y_low = format!("{:02X}", fields.y_low);
                    self.extra_bits = format!("{:X}", fields.extra_bits);
                    self.screen = format!("{:02X}", fields.screen);
                    self.x = format!("{:X}", fields.x);
                    self.sprite_number = format!("{:02X}", fields.sprite_number);
                } else {
                    self.clear_semantic_form();
                }
            }
            SpriteToken::Screen(value) => {
                self.kind = TokenKind::Screen;
                self.value = format!("{value:02X}");
                self.clear_semantic_form();
            }
            SpriteToken::Control(value) => {
                self.kind = TokenKind::Control;
                self.value = format!("{value:02X}");
                self.clear_semantic_form();
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
                "sprite upper-Y token",
            )?)),
            TokenKind::Control => Ok(SpriteToken::Control(level_editor_forms::parse_hex_u8(
                &self.value,
                "sprite control token",
            )?)),
        }
    }

    fn clear_semantic_form(&mut self) {
        self.y_low.clear();
        self.extra_bits.clear();
        self.screen.clear();
        self.x.clear();
        self.sprite_number.clear();
    }
}

fn token_label(
    index: usize,
    token: &SpriteToken,
    placement: Option<&lm_level::NativeSpritePlacement>,
) -> String {
    match token {
        SpriteToken::Record(record) => placement.map_or_else(
            || {
                format!(
                    "{index:03}: record {}",
                    level_editor_forms::format_bytes(&record.encoded)
                )
            },
            |placement| {
                format!(
                    "{index:03}: sprite {:02X} screen {:02X} X {:X} Y {:03X} extra {} · {}",
                    placement.sprite_number,
                    placement.screen,
                    placement.major & 0x0f,
                    placement.minor,
                    placement.extra_bits,
                    level_editor_forms::format_bytes(&record.encoded)
                )
            },
        ),
        SpriteToken::Screen(value) => format!("{index:03}: upper Y {value:02X}"),
        SpriteToken::Control(value) => format!("{index:03}: control {value:02X}"),
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
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

    #[test]
    fn mwl_header_form_preserves_the_stream_framing_discriminator() {
        let mut panel = MwlSpritePanel {
            stream: Some(NativeSpriteStream {
                header: 0x20,
                expanded: true,
                tokens: Vec::new(),
            }),
            ..MwlSpritePanel::default()
        };
        panel.load_form();
        panel.header.memory = 0x12;
        panel.header.buoyancy_2 = true;
        panel.stream.as_mut().unwrap().header = panel.header.header().unwrap();
        assert_eq!(panel.stream.unwrap().header, 0x72);
    }

    #[test]
    fn recovered_sprite_fields_preserve_extensions_and_validate_custom_shape() {
        let mut panel = MwlSpritePanel {
            stream: Some(NativeSpriteStream {
                header: 0,
                expanded: false,
                tokens: vec![SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x9a, 0xc7, 0x42, 0xaa, 0xbb],
                })],
            }),
            ..MwlSpritePanel::default()
        };
        panel.lengths.set(2, 0x42, 5).unwrap();
        panel.load_form();
        assert_eq!(panel.y_low, "09");
        assert_eq!(panel.screen, "17");
        panel.y_low = "1D".into();
        panel.screen = "1E".into();
        panel.x = "3".into();
        panel.stage_semantic_fields().unwrap();
        let SpriteToken::Record(record) = &panel.stream.as_ref().unwrap().tokens[0] else {
            panic!("record expected");
        };
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap().y_low, 0x1d);

        let before = record.clone();
        panel.extra_bits = "1".into();
        assert!(panel.stage_semantic_fields().is_err());
        assert_eq!(
            panel.stream.as_ref().unwrap().tokens[0],
            SpriteToken::Record(before)
        );
    }
}
