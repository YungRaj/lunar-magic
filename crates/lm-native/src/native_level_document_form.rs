use crate::level_editor_forms;
use lm_app::NativeLevelEdit;
use lm_level::{ObjectEdit, SpriteToken};

#[derive(Clone, Debug, Default)]
pub(crate) struct NativeLevelRecordForm {
    pub(crate) object: String,
    pub(crate) sprite: String,
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
}
