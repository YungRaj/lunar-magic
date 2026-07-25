use crate::level_editor_forms;
use lm_app::NativeLevelEdit;
use lm_level::{NativeSpriteRecordFields, ObjectEdit, SpriteLengthTable, SpriteToken};

#[derive(Clone, Debug, Default)]
pub(crate) struct NativeLevelRecordForm {
    pub(crate) object: String,
    pub(crate) sprite: String,
    pub(crate) sprite_y_low: u8,
    pub(crate) sprite_extra_bits: u8,
    pub(crate) sprite_screen: u8,
    pub(crate) sprite_x: u8,
    pub(crate) sprite_number: u8,
    pub(crate) sprite_fields_loaded: bool,
}

impl NativeLevelRecordForm {
    pub(crate) fn object_edit(
        &self,
        index: usize,
        insert: bool,
    ) -> Result<NativeLevelEdit, String> {
        let record = level_editor_forms::parse_object(&self.object)?;
        Ok(NativeLevelEdit::Objects(vec![if insert {
            ObjectEdit::Insert { index, record }
        } else {
            ObjectEdit::Replace { index, record }
        }]))
    }

    pub(crate) fn sprite_edit(
        &self,
        index: usize,
        insert: bool,
    ) -> Result<NativeLevelEdit, String> {
        let token = parse_sprite_token(&self.sprite)?;
        Ok(if insert {
            NativeLevelEdit::InsertSprite { index, token }
        } else {
            NativeLevelEdit::ReplaceSprite { index, token }
        })
    }

    pub(crate) fn load_sprite(&mut self, token: Option<&SpriteToken>) {
        self.sprite = match token {
            Some(SpriteToken::Record(record)) => {
                if let Ok(fields) = record.native_fields() {
                    self.sprite_y_low = fields.y_low;
                    self.sprite_extra_bits = fields.extra_bits;
                    self.sprite_screen = fields.screen;
                    self.sprite_x = fields.x;
                    self.sprite_number = fields.sprite_number;
                    self.sprite_fields_loaded = true;
                } else {
                    self.sprite_fields_loaded = false;
                }
                level_editor_forms::format_bytes(&record.encoded)
            }
            Some(SpriteToken::Screen(value)) => {
                self.sprite_fields_loaded = false;
                format!("yhigh {value:02X}")
            }
            Some(SpriteToken::Control(value)) => {
                self.sprite_fields_loaded = false;
                format!("control {value:02X}")
            }
            None => {
                self.sprite_fields_loaded = false;
                String::new()
            }
        };
    }

    pub(crate) fn sprite_field_edit(
        &self,
        index: usize,
        token: Option<&SpriteToken>,
        lengths: &SpriteLengthTable,
    ) -> Result<NativeLevelEdit, String> {
        let Some(SpriteToken::Record(record)) = token else {
            return Err("select a sprite record before applying semantic fields".into());
        };
        let mut record = record.clone();
        record
            .set_native_fields(
                NativeSpriteRecordFields {
                    y_low: self.sprite_y_low,
                    extra_bits: self.sprite_extra_bits,
                    screen: self.sprite_screen,
                    x: self.sprite_x,
                    sprite_number: self.sprite_number,
                },
                lengths,
            )
            .map_err(|error| error.to_string())?;
        Ok(NativeLevelEdit::ReplaceSprite {
            index,
            token: SpriteToken::Record(record),
        })
    }
}

pub(crate) fn parse_sprite_token(text: &str) -> Result<SpriteToken, String> {
    let trimmed = text.trim();
    if let Some(value) = trimmed
        .strip_prefix("yhigh ")
        .or_else(|| trimmed.strip_prefix("screen "))
    {
        return level_editor_forms::parse_hex_u8(value, "sprite upper-Y token")
            .map(SpriteToken::Screen);
    }
    if let Some(value) = trimmed.strip_prefix("control ") {
        return level_editor_forms::parse_hex_u8(value, "sprite control token")
            .map(SpriteToken::Control);
    }
    level_editor_forms::parse_sprite(trimmed).map(SpriteToken::Record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_token_form_supports_all_native_token_kinds() {
        assert_eq!(
            parse_sprite_token("01 02 03").unwrap(),
            SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![1, 2, 3]
            })
        );
        assert_eq!(
            parse_sprite_token("yhigh 7F").unwrap(),
            SpriteToken::Screen(0x7f)
        );
        assert_eq!(
            parse_sprite_token("screen 7F").unwrap(),
            SpriteToken::Screen(0x7f)
        );
        assert_eq!(
            parse_sprite_token("control 80").unwrap(),
            SpriteToken::Control(0x80)
        );
        assert!(parse_sprite_token("yhigh nope").is_err());
    }

    #[test]
    fn semantic_sprite_form_preserves_custom_extensions_and_table_shape() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(2, 0x42, 5).unwrap();
        let token = SpriteToken::Record(lm_level::SpriteRecord {
            encoded: vec![0x9a, 0xc7, 0x42, 0xaa, 0xbb],
        });
        let mut form = NativeLevelRecordForm::default();
        form.load_sprite(Some(&token));
        assert!(form.sprite_fields_loaded);
        assert_eq!(
            (
                form.sprite_y_low,
                form.sprite_extra_bits,
                form.sprite_screen,
                form.sprite_x,
                form.sprite_number,
            ),
            (9, 2, 23, 12, 0x42)
        );
        form.sprite_x = 3;
        form.sprite_y_low = 0x1d;
        let edit = form.sprite_field_edit(4, Some(&token), &lengths).unwrap();
        let NativeLevelEdit::ReplaceSprite {
            token: SpriteToken::Record(record),
            ..
        } = edit
        else {
            panic!("expected sprite replacement");
        };
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap().x, 3);
        assert_eq!(record.native_fields().unwrap().y_low, 0x1d);

        form.sprite_number = 0x43;
        assert!(form.sprite_field_edit(4, Some(&token), &lengths).is_err());
    }
}
