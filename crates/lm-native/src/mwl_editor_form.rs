use crate::level_editor_forms;
use lm_app::MwlDocumentEdit;
use lm_level::{
    Layer2ScrollSettings, MwlFile, MwlLevelHeaderSection, MwlMainEntranceSettings,
    MwlMidwayEntranceSettings, MwlSectionKind, SpriteSpawnSettings,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct MwlForm {
    pub(crate) flags: String,
    pub(crate) attribution: String,
    pub(crate) level_number: String,
    pub(crate) main_entrance: [String; 7],
    pub(crate) midway_entrance: [String; 4],
    pub(crate) separate_layer2_scroll: bool,
    pub(crate) layer2_original_scroll: u8,
    pub(crate) layer2_horizontal_scroll: u8,
    pub(crate) layer2_vertical_scroll: u8,
    loaded_layer2_scroll: Option<Layer2ScrollSettings>,
    pub(crate) sprite_vertical_spawn_range: u8,
    pub(crate) sprite_smart_spawn: bool,
    loaded_sprite_spawn: Option<SpriteSpawnSettings>,
    pub(crate) section_index: usize,
    pub(crate) section_bytes: String,
}

impl MwlForm {
    pub(crate) fn load_header(file: &MwlFile) -> Self {
        let header = MwlLevelHeaderSection::decode(
            &file.sections[MwlSectionKind::LevelHeader as usize].bytes,
        );
        let level_number = header.as_ref().map_or_else(
            |_| String::new(),
            |value| format!("{:04X}", value.level_number()),
        );
        let main_entrance = header.as_ref().map_or_else(
            |_| std::array::from_fn(|_| String::new()),
            |header| {
                let value = header.main_entrance();
                [
                    value.position,
                    value.vertical_settings,
                    value.screen_and_method,
                    value.level_mode_and_screen,
                    value.flags,
                    value.high_position,
                    value.additional_flags,
                ]
                .map(|value| format!("{value:02X}"))
            },
        );
        let midway_entrance = header.as_ref().map_or_else(
            |_| std::array::from_fn(|_| String::new()),
            |header| {
                let value = header.midway_entrance();
                [
                    value.position,
                    value.flags,
                    value.high_position,
                    value.additional_flags,
                ]
                .map(|value| format!("{value:02X}"))
            },
        );
        let layer2_scroll = header
            .as_ref()
            .ok()
            .map(MwlLevelHeaderSection::layer2_scroll_settings);
        let (
            separate_layer2_scroll,
            layer2_original_scroll,
            layer2_horizontal_scroll,
            layer2_vertical_scroll,
        ) = match layer2_scroll {
            Some(Layer2ScrollSettings::Original { table_index }) => (false, table_index, 0, 0),
            Some(Layer2ScrollSettings::Separate {
                horizontal,
                vertical,
            }) => (true, 0, horizontal, vertical),
            None => (false, 0, 0, 0),
        };
        let sprite_spawn = header
            .as_ref()
            .ok()
            .map(MwlLevelHeaderSection::sprite_spawn_settings);
        Self {
            flags: format!("{:08X}", file.flags),
            attribution: level_editor_forms::format_bytes(&file.attribution),
            level_number,
            main_entrance,
            midway_entrance,
            separate_layer2_scroll,
            layer2_original_scroll,
            layer2_horizontal_scroll,
            layer2_vertical_scroll,
            loaded_layer2_scroll: layer2_scroll,
            sprite_vertical_spawn_range: sprite_spawn
                .map_or(0, SpriteSpawnSettings::vertical_range),
            sprite_smart_spawn: sprite_spawn.is_some_and(SpriteSpawnSettings::smart_spawn),
            loaded_sprite_spawn: sprite_spawn,
            ..Self::default()
        }
    }

    pub(crate) fn load_section(&mut self, file: &MwlFile, index: usize) {
        self.section_index = index.min(MwlFile::SECTION_COUNT - 1);
        self.section_bytes =
            level_editor_forms::format_bytes(&file.sections[self.section_index].bytes);
    }

    pub(crate) fn header_edits(&self) -> Result<Vec<MwlDocumentEdit>, String> {
        let attribution =
            level_editor_forms::parse_bytes(&self.attribution, "MWL attribution byte")?;
        let attribution: [u8; MwlFile::ATTRIBUTION_LEN] =
            attribution.try_into().map_err(|value: Vec<u8>| {
                format!(
                    "MWL attribution requires {} bytes, got {}",
                    MwlFile::ATTRIBUTION_LEN,
                    value.len()
                )
            })?;
        let mut edits = vec![
            MwlDocumentEdit::SetFlags(level_editor_forms::parse_hex_u32(&self.flags, "MWL flags")?),
            MwlDocumentEdit::SetAttribution(attribution),
        ];
        if !self.level_number.trim().is_empty() {
            edits.push(MwlDocumentEdit::SetLevelNumber(
                level_editor_forms::parse_hex_u16(&self.level_number, "MWL level number")?,
            ));
        }
        if self
            .main_entrance
            .iter()
            .all(|value| !value.trim().is_empty())
        {
            let values = parse_hex_bytes(&self.main_entrance, "main entrance")?;
            edits.push(MwlDocumentEdit::SetMainEntrance(MwlMainEntranceSettings {
                position: values[0],
                vertical_settings: values[1],
                screen_and_method: values[2],
                level_mode_and_screen: values[3],
                flags: values[4],
                high_position: values[5],
                additional_flags: values[6],
            }));
        }
        if self
            .midway_entrance
            .iter()
            .all(|value| !value.trim().is_empty())
        {
            let values = parse_hex_bytes(&self.midway_entrance, "midway entrance")?;
            edits.push(MwlDocumentEdit::SetMidwayEntrance(
                MwlMidwayEntranceSettings {
                    position: values[0],
                    flags: values[1],
                    high_position: values[2],
                    additional_flags: values[3],
                },
            ));
        }
        let layer2_scroll = if self.separate_layer2_scroll {
            Layer2ScrollSettings::Separate {
                horizontal: self.layer2_horizontal_scroll,
                vertical: self.layer2_vertical_scroll,
            }
        } else {
            Layer2ScrollSettings::Original {
                table_index: self.layer2_original_scroll,
            }
        };
        if self
            .loaded_layer2_scroll
            .is_some_and(|loaded| loaded != layer2_scroll)
        {
            edits.push(MwlDocumentEdit::SetLayer2Scroll(layer2_scroll));
        }
        if let Some(loaded) = self.loaded_sprite_spawn {
            let sprite_spawn = loaded
                .with_properties(self.sprite_vertical_spawn_range, self.sprite_smart_spawn)
                .map_err(|error| error.to_string())?;
            if sprite_spawn != loaded {
                edits.push(MwlDocumentEdit::SetSpriteSpawnSettings(sprite_spawn));
            }
        }
        Ok(edits)
    }

    pub(crate) fn section_edit(&self) -> Result<MwlDocumentEdit, String> {
        Ok(MwlDocumentEdit::ReplaceSection {
            section: section_kind(self.section_index),
            bytes: level_editor_forms::parse_bytes(&self.section_bytes, "MWL section byte")?,
        })
    }
}

fn parse_hex_bytes<const N: usize>(values: &[String; N], field: &str) -> Result<[u8; N], String> {
    let mut parsed = [0; N];
    for (index, value) in values.iter().enumerate() {
        parsed[index] = level_editor_forms::parse_hex_u8(value, &format!("{field} byte {index}"))?;
    }
    Ok(parsed)
}

pub(crate) const SECTION_NAMES: [&str; MwlFile::SECTION_COUNT] = [
    "Level header",
    "Layer 1",
    "Layer 2",
    "Sprites",
    "Palette",
    "Secondary exits",
    "ExAnimation",
    "Expanded header",
];

const fn section_kind(index: usize) -> MwlSectionKind {
    match index {
        0 => MwlSectionKind::LevelHeader,
        1 => MwlSectionKind::Layer1,
        2 => MwlSectionKind::Layer2,
        3 => MwlSectionKind::Sprites,
        4 => MwlSectionKind::Palette,
        5 => MwlSectionKind::SecondaryExits,
        6 => MwlSectionKind::ExAnimation,
        _ => MwlSectionKind::ExpandedHeader,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MwlSection;

    fn file() -> MwlFile {
        let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
            std::array::from_fn(|_| MwlSection::default());
        let mut header = MwlLevelHeaderSection([0x5a; MwlLevelHeaderSection::ENCODED_LEN]);
        header.set_level_number(0x105);
        sections[0].bytes = header.0.to_vec();
        MwlFile {
            version: MwlFile::CURRENT_VERSION,
            flags: 0x9234_5678,
            attribution: [0xa5; MwlFile::ATTRIBUTION_LEN],
            sections,
        }
    }

    #[test]
    fn header_form_preserves_all_attribution_bytes() {
        let form = MwlForm::load_header(&file());
        let edits = form.header_edits().unwrap();
        assert_eq!(edits.len(), 5);
        assert_eq!(
            edits[1],
            MwlDocumentEdit::SetAttribution([0xa5; MwlFile::ATTRIBUTION_LEN])
        );
    }

    #[test]
    fn scroll_form_emits_an_edit_only_after_semantic_change() {
        let mut form = MwlForm::load_header(&file());
        assert!(
            form.header_edits()
                .unwrap()
                .iter()
                .all(|edit| !matches!(edit, MwlDocumentEdit::SetLayer2Scroll(_)))
        );
        form.separate_layer2_scroll = true;
        form.layer2_horizontal_scroll = 0x1b;
        form.layer2_vertical_scroll = 0x12;
        assert!(form.header_edits().unwrap().iter().any(|edit| {
            matches!(
                edit,
                MwlDocumentEdit::SetLayer2Scroll(Layer2ScrollSettings::Separate {
                    horizontal: 0x1b,
                    vertical: 0x12
                })
            )
        }));
    }

    #[test]
    fn spawn_form_preserves_shared_header_flags_and_emits_one_semantic_edit() {
        let mut file = file();
        file.sections[0].bytes[6] = 0xe1;
        let mut form = MwlForm::load_header(&file);
        form.sprite_vertical_spawn_range = 3;
        form.sprite_smart_spawn = true;
        let edits = form.header_edits().unwrap();
        assert!(edits.iter().any(|edit| {
            matches!(edit, MwlDocumentEdit::SetSpriteSpawnSettings(value) if value.raw() == 0xe7)
        }));
    }

    #[test]
    fn section_form_keeps_empty_and_opaque_sections_distinct() {
        let mut file = file();
        file.sections[3].bytes = vec![0, 0x81, 0xff];
        let mut form = MwlForm::load_header(&file);
        form.load_section(&file, 3);
        assert!(matches!(
            form.section_edit().unwrap(),
            MwlDocumentEdit::ReplaceSection {
                section: MwlSectionKind::Sprites,
                bytes
            } if bytes == [0, 0x81, 0xff]
        ));
    }
}
