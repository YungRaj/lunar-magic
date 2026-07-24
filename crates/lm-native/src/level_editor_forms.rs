use lm_level::{
    Entrance, EntranceKind, Map16Tile, ObjectRecord, ScreenExit, SecondaryExit, SpriteRecord,
    Subtile,
};

pub(crate) fn parse_hex_u16(text: &str, name: &str) -> Result<u16, String> {
    u16::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

pub(crate) fn parse_hex_u32(text: &str, name: &str) -> Result<u32, String> {
    u32::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

pub(crate) fn parse_hex_u8(text: &str, name: &str) -> Result<u8, String> {
    u8::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

pub(crate) fn format_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn parse_bytes(text: &str, name: &str) -> Result<Vec<u8>, String> {
    text.split_whitespace()
        .map(|word| parse_hex_u8(word, name))
        .collect()
}

pub(crate) fn parse_object(text: &str) -> Result<ObjectRecord, String> {
    ObjectRecord::new(parse_bytes(text, "object byte")?).map_err(|error| error.to_string())
}

pub(crate) fn parse_sprite(text: &str) -> Result<SpriteRecord, String> {
    let encoded = parse_bytes(text, "sprite byte")?;
    if encoded.len() < 3 {
        return Err("sprite records require at least three bytes".into());
    }
    Ok(SpriteRecord { encoded })
}

#[derive(Clone, Debug)]
pub(crate) struct EntranceForm {
    pub(crate) kind: usize,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) screen: String,
    pub(crate) action: String,
    pub(crate) flags: String,
}

impl Default for EntranceForm {
    fn default() -> Self {
        Self {
            kind: 0,
            x: "0000".into(),
            y: "0000".into(),
            screen: "00".into(),
            action: "00".into(),
            flags: "0000".into(),
        }
    }
}

impl EntranceForm {
    pub(crate) fn load(value: Entrance) -> Self {
        Self {
            kind: match value.kind {
                EntranceKind::Main => 0,
                EntranceKind::Midway => 1,
                EntranceKind::Secondary => 2,
            },
            x: format!("{:04X}", value.x),
            y: format!("{:04X}", value.y),
            screen: format!("{:02X}", value.screen),
            action: format!("{:02X}", value.action),
            flags: format!("{:04X}", value.raw_flags),
        }
    }

    pub(crate) fn parse(&self) -> Result<Entrance, String> {
        Ok(Entrance {
            kind: [
                EntranceKind::Main,
                EntranceKind::Midway,
                EntranceKind::Secondary,
            ][self.kind.min(2)],
            x: parse_hex_u16(&self.x, "entrance X")?,
            y: parse_hex_u16(&self.y, "entrance Y")?,
            screen: parse_hex_u8(&self.screen, "entrance screen")?,
            action: parse_hex_u8(&self.action, "entrance action")?,
            raw_flags: parse_hex_u16(&self.flags, "entrance flags")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ScreenExitForm {
    pub(crate) encoded: String,
}

impl ScreenExitForm {
    pub(crate) fn load(value: ScreenExit) -> Self {
        Self {
            encoded: format!("{:08X}", value.encoded),
        }
    }

    pub(crate) fn parse(&self) -> Result<ScreenExit, String> {
        Ok(ScreenExit {
            encoded: parse_hex_u32(&self.encoded, "screen exit")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SecondaryExitForm {
    pub(crate) destination: String,
    pub(crate) position: String,
    pub(crate) screen: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) destination_flags: String,
    pub(crate) x_flags: String,
    pub(crate) additional: String,
}

impl SecondaryExitForm {
    pub(crate) fn load(value: SecondaryExit) -> Self {
        Self {
            destination: format!("{:04X}", value.destination_level),
            position: format!("{:02X}", value.position_and_method),
            screen: format!("{:02X}", value.screen),
            x: format!("{:02X}", value.x),
            y: format!("{:02X}", value.y),
            destination_flags: format!("{:02X}", value.destination_flags),
            x_flags: format!("{:02X}", value.x_and_overworld_flags),
            additional: format!("{:02X}", value.additional_flags),
        }
    }

    pub(crate) fn parse(&self) -> Result<SecondaryExit, String> {
        Ok(SecondaryExit {
            destination_level: parse_hex_u16(&self.destination, "secondary destination")?,
            position_and_method: parse_hex_u8(&self.position, "secondary position/method")?,
            screen: parse_hex_u8(&self.screen, "secondary screen")?,
            x: parse_hex_u8(&self.x, "secondary X")?,
            y: parse_hex_u8(&self.y, "secondary Y")?,
            destination_flags: parse_hex_u8(
                &self.destination_flags,
                "secondary destination flags",
            )?,
            x_and_overworld_flags: parse_hex_u8(&self.x_flags, "secondary X/overworld flags")?,
            additional_flags: parse_hex_u8(&self.additional, "secondary additional flags")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Map16OverrideForm {
    pub(crate) index: String,
    pub(crate) top_left: String,
    pub(crate) top_right: String,
    pub(crate) bottom_left: String,
    pub(crate) bottom_right: String,
    pub(crate) acts_like: String,
}

impl Map16OverrideForm {
    pub(crate) fn load(index: u32, tile: Map16Tile) -> Self {
        Self {
            index: format!("{index:08X}"),
            top_left: format!("{:04X}", tile.top_left.0),
            top_right: format!("{:04X}", tile.top_right.0),
            bottom_left: format!("{:04X}", tile.bottom_left.0),
            bottom_right: format!("{:04X}", tile.bottom_right.0),
            acts_like: format!("{:04X}", tile.acts_like),
        }
    }

    pub(crate) fn parse(&self) -> Result<(u32, Map16Tile), String> {
        Ok((
            parse_hex_u32(&self.index, "Map16 override index")?,
            Map16Tile {
                top_left: Subtile(parse_hex_u16(&self.top_left, "top-left subtile")?),
                top_right: Subtile(parse_hex_u16(&self.top_right, "top-right subtile")?),
                bottom_left: Subtile(parse_hex_u16(&self.bottom_left, "bottom-left subtile")?),
                bottom_right: Subtile(parse_hex_u16(&self.bottom_right, "bottom-right subtile")?),
                acts_like: parse_hex_u16(&self.acts_like, "Acts Like")?,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_record_forms_preserve_bytes_and_enforce_domain_bounds() {
        assert_eq!(parse_object("01 02 03").unwrap().encoded(), &[1, 2, 3]);
        assert_eq!(
            parse_sprite("AA BB CC DD").unwrap().encoded,
            [0xaa, 0xbb, 0xcc, 0xdd]
        );
        assert!(parse_object("01 02").is_err());
        assert!(parse_sprite("01 02").is_err());
    }

    #[test]
    fn auxiliary_forms_preserve_lossless_values() {
        let exit = SecondaryExit {
            destination_level: 0x1234,
            position_and_method: 1,
            screen: 2,
            x: 3,
            y: 4,
            destination_flags: 5,
            x_and_overworld_flags: 6,
            additional_flags: 7,
        };
        assert_eq!(SecondaryExitForm::load(exit).parse().unwrap(), exit);
        let tile = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 5,
        };
        assert_eq!(
            Map16OverrideForm::load(0x12345, tile).parse().unwrap(),
            (0x12345, tile)
        );
    }
}
