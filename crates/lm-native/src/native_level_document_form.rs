use crate::level_editor_forms;
use lm_app::NativeLevelEdit;
use lm_level::{
    CustomTimeError, CustomTimeSettings, Layer1VerticalScrollMode, LegacyHeaderEdit,
    NativeSpriteRecordFields, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord, SpriteLengthTable,
    SpriteToken,
};
use lm_project::LoadedLevelSlot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NativeLevelHeaderForm {
    pub(crate) background_palette: u8,
    pub(crate) last_screen: u8,
    pub(crate) level_mode: u8,
    pub(crate) background_color: u8,
    pub(crate) sprite_tileset: u8,
    pub(crate) default_music_selector: u8,
    pub(crate) time_limit_selector: u8,
    pub(crate) custom_time_enabled: bool,
    pub(crate) custom_time_value: u16,
    pub(crate) force_time_reset: bool,
    pub(crate) sprite_palette: u8,
    pub(crate) foreground_palette: u8,
    pub(crate) object_tileset: u8,
    pub(crate) layer1_vertical_scroll: u8,
}

impl NativeLevelHeaderForm {
    pub(crate) fn load(level: &LoadedLevelSlot) -> Self {
        let header = level.layer1.header;
        let vertical = lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical;
        let custom_time = level.layer1.objects.custom_time(vertical);
        Self {
            background_palette: header.background_palette(),
            last_screen: header.last_screen(),
            level_mode: header.level_mode(),
            background_color: header.background_color(),
            sprite_tileset: header.sprite_tileset(),
            default_music_selector: header.default_music_selector(),
            time_limit_selector: header.time_limit_selector(),
            custom_time_enabled: custom_time.is_some(),
            custom_time_value: custom_time.map_or(300, CustomTimeSettings::value),
            force_time_reset: custom_time.is_some_and(CustomTimeSettings::force_reset),
            sprite_palette: header.sprite_palette(),
            foreground_palette: header.foreground_palette(),
            object_tileset: header.object_tileset(),
            layer1_vertical_scroll: header.layer1_vertical_scroll().raw(),
        }
    }

    pub(crate) fn edits(self) -> Result<Vec<NativeLevelEdit>, CustomTimeError> {
        let custom_time = self
            .custom_time_enabled
            .then(|| CustomTimeSettings::new(self.custom_time_value, self.force_time_reset))
            .transpose()?;
        Ok(vec![
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(
                self.background_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(self.last_screen)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(self.level_mode)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(self.background_color)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(self.sprite_tileset)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(
                self.default_music_selector,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::TimeLimitSelector(
                self.time_limit_selector,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(self.sprite_palette)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(
                self.foreground_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(self.object_tileset)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::Layer1VerticalScroll(
                Layer1VerticalScrollMode::from_raw(self.layer1_vertical_scroll),
            )),
            NativeLevelEdit::SetCustomTime(custom_time),
        ])
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NativeLevelRecordForm {
    pub(crate) object: String,
    pub(crate) object_command: u8,
    pub(crate) object_parameter: u8,
    pub(crate) object_first: u8,
    pub(crate) object_second: u8,
    pub(crate) object_screen: u16,
    pub(crate) object_fields_loaded: bool,
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

    pub(crate) fn load_object(&mut self, record: Option<&ObjectRecord>, screen: Option<u16>) {
        let Some(record) = record else {
            self.object.clear();
            self.object_fields_loaded = false;
            return;
        };
        self.object = level_editor_forms::format_bytes(record.encoded());
        let coordinates = record.coordinate_nibbles();
        self.object_command = record.command_id();
        self.object_parameter = record.parameter();
        self.object_first = coordinates.first;
        self.object_second = coordinates.second;
        self.object_screen = screen.unwrap_or_default();
        self.object_fields_loaded = screen.is_some();
    }

    pub(crate) fn object_field_edit(&self, index: usize) -> Result<NativeLevelEdit, String> {
        if !self.object_fields_loaded {
            return Err("select an ordinary object before applying semantic fields".into());
        }
        Ok(NativeLevelEdit::Objects(vec![
            ObjectEdit::SetCommandId {
                index,
                command_id: self.object_command,
            },
            ObjectEdit::SetParameter {
                index,
                parameter: self.object_parameter,
            },
            ObjectEdit::RelocateOrdinary {
                index,
                screen: self.object_screen,
                coordinates: ObjectCoordinateNibbles {
                    first: self.object_first,
                    second: self.object_second,
                },
            },
        ]))
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
    fn native_header_form_emits_every_legacy_field_and_custom_timer_atomically() {
        let form = NativeLevelHeaderForm {
            background_palette: 1,
            last_screen: 2,
            level_mode: 3,
            background_color: 4,
            sprite_tileset: 5,
            default_music_selector: 6,
            time_limit_selector: 2,
            custom_time_enabled: true,
            custom_time_value: 0xabc,
            force_time_reset: true,
            sprite_palette: 7,
            foreground_palette: 6,
            object_tileset: 15,
            layer1_vertical_scroll: 2,
        };
        assert_eq!(
            form.edits().unwrap(),
            vec![
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(1)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(2)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(4)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(5)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(6)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::TimeLimitSelector(2)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(7)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(6)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(15)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::Layer1VerticalScroll(
                    Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying,
                )),
                NativeLevelEdit::SetCustomTime(
                    Some(CustomTimeSettings::new(0xabc, true).unwrap(),)
                ),
            ]
        );
    }

    #[test]
    fn native_header_form_rejects_non_persistable_enabled_zero_timer() {
        let form = NativeLevelHeaderForm {
            custom_time_enabled: true,
            custom_time_value: 0,
            force_time_reset: false,
            ..NativeLevelHeaderForm::default()
        };
        assert_eq!(form.edits(), Err(CustomTimeError::DisabledEncoding));
    }

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

    #[test]
    fn semantic_object_form_builds_one_atomic_field_and_relocation_batch() {
        let record = ObjectRecord::new(vec![0x45, 0x26, 0x42, 0xaa]).unwrap();
        let mut form = NativeLevelRecordForm::default();
        form.load_object(Some(&record), Some(7));
        assert!(form.object_fields_loaded);
        assert_eq!(form.object_command, 0x22);
        assert_eq!(form.object_parameter, 0x42);
        assert_eq!((form.object_first, form.object_second), (5, 6));
        form.object_screen = 9;
        form.object_first = 3;
        let NativeLevelEdit::Objects(edits) = form.object_field_edit(4).unwrap() else {
            panic!("expected object batch");
        };
        assert_eq!(
            edits.last(),
            Some(&ObjectEdit::RelocateOrdinary {
                index: 4,
                screen: 9,
                coordinates: ObjectCoordinateNibbles {
                    first: 3,
                    second: 6,
                },
            })
        );
        form.load_object(Some(&record), None);
        assert!(form.object_field_edit(4).is_err());
    }
}
