use crate::{level_editor_forms, overworld_editor_forms::SUBMAP_NAMES};
use lm_overworld::{OverworldLevelName, PlayerStart, Submap, SubmapSettings};
use std::fmt::Write;

#[derive(Clone, Debug, Default)]
pub(crate) struct LevelNameForm {
    pub(crate) level: String,
    pub(crate) tiles: String,
    pub(crate) raw_flags: String,
}

impl LevelNameForm {
    pub(crate) fn load(value: &OverworldLevelName) -> Self {
        Self {
            level: format!("{:04X}", value.level),
            tiles: encode_hex(&value.tiles),
            raw_flags: format!("{:02X}", value.raw_flags),
        }
    }

    pub(crate) fn parse(&self) -> Result<OverworldLevelName, String> {
        Ok(OverworldLevelName {
            level: level_editor_forms::parse_hex_u16(&self.level, "level-name key")?,
            tiles: parse_fixed_hex(&self.tiles, "level-name tiles")?,
            raw_flags: level_editor_forms::parse_hex_u8(&self.raw_flags, "level-name flags")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PlayerStartForm {
    pub(crate) player: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) submap: usize,
    pub(crate) raw_flags: String,
}

impl PlayerStartForm {
    pub(crate) fn load(value: PlayerStart) -> Self {
        Self {
            player: format!("{:02X}", value.player),
            x: format!("{:04X}", value.x),
            y: format!("{:04X}", value.y),
            submap: usize::from(value.submap.encoded()),
            raw_flags: format!("{:02X}", value.raw_flags),
        }
    }

    pub(crate) fn parse(&self) -> Result<PlayerStart, String> {
        Ok(PlayerStart {
            player: level_editor_forms::parse_hex_u8(&self.player, "player key")?,
            x: level_editor_forms::parse_hex_u16(&self.x, "player X")?,
            y: level_editor_forms::parse_hex_u16(&self.y, "player Y")?,
            submap: parse_submap(self.submap)?,
            raw_flags: level_editor_forms::parse_hex_u8(&self.raw_flags, "player flags")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SettingsForm {
    pub(crate) submap: usize,
    pub(crate) music: String,
    pub(crate) palette: String,
    pub(crate) layer1_scroll: String,
    pub(crate) layer2_scroll: String,
    pub(crate) raw_flags: String,
    pub(crate) unknown: String,
}

impl SettingsForm {
    pub(crate) fn load(value: SubmapSettings) -> Self {
        Self {
            submap: usize::from(value.submap.encoded()),
            music: format!("{:02X}", value.music),
            palette: format!("{:02X}", value.palette),
            layer1_scroll: format!("{:02X}", value.layer1_scroll),
            layer2_scroll: format!("{:02X}", value.layer2_scroll),
            raw_flags: format!("{:04X}", value.raw_flags),
            unknown: encode_hex(&value.unknown),
        }
    }

    pub(crate) fn parse(&self) -> Result<SubmapSettings, String> {
        Ok(SubmapSettings {
            submap: parse_submap(self.submap)?,
            music: level_editor_forms::parse_hex_u8(&self.music, "submap music")?,
            palette: level_editor_forms::parse_hex_u8(&self.palette, "submap palette")?,
            layer1_scroll: level_editor_forms::parse_hex_u8(&self.layer1_scroll, "Layer 1 scroll")?,
            layer2_scroll: level_editor_forms::parse_hex_u8(&self.layer2_scroll, "Layer 2 scroll")?,
            raw_flags: level_editor_forms::parse_hex_u16(&self.raw_flags, "submap flags")?,
            unknown: parse_fixed_hex(&self.unknown, "unknown submap bytes")?,
        })
    }
}

pub(crate) const METADATA_SUBMAP_NAMES: [&str; 7] = SUBMAP_NAMES;

fn parse_submap(value: usize) -> Result<Submap, String> {
    Submap::decode(u8::try_from(value).unwrap_or(u8::MAX)).ok_or("invalid metadata submap".into())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02X}").expect("writing to a string cannot fail");
            output
        },
    )
}

fn parse_fixed_hex<const N: usize>(text: &str, name: &str) -> Result<[u8; N], String> {
    let text = text.trim();
    if text.len() != N * 2 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{name} must contain exactly {N} hexadecimal bytes"));
    }
    let mut result = [0; N];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|error| format!("invalid {name}: {error}"))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_form_preserves_unknown_bytes_and_flags() {
        let value = SubmapSettings {
            submap: Submap::StarWorld,
            music: 1,
            palette: 2,
            layer1_scroll: 3,
            layer2_scroll: 4,
            raw_flags: 0x9234,
            unknown: [5, 6, 7, 8, 9],
        };
        assert_eq!(SettingsForm::load(value).parse().unwrap(), value);
    }

    #[test]
    fn name_form_requires_all_nineteen_tiles() {
        let form = LevelNameForm {
            level: "105".into(),
            tiles: "12".repeat(OverworldLevelName::TILE_COUNT),
            raw_flags: "81".into(),
        };
        assert_eq!(form.parse().unwrap().tiles, [0x12; 19]);
        let mut malformed = form;
        malformed.tiles.pop();
        assert!(malformed.parse().is_err());
    }
}
