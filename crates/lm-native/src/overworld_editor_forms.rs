use crate::level_editor_forms;
use lm_overworld::{EventReveal, OverworldEndpoint, OverworldSprite, Submap};

#[derive(Clone, Debug, Default)]
pub(crate) struct RevealForm {
    pub(crate) source: String,
    pub(crate) destination: String,
}

impl RevealForm {
    pub(crate) fn load(value: EventReveal) -> Self {
        Self {
            source: format!("{:04X}", value.source_tile),
            destination: format!("{:04X}", value.destination_tile),
        }
    }

    pub(crate) fn parse(&self) -> Result<EventReveal, String> {
        Ok(EventReveal {
            source_tile: level_editor_forms::parse_hex_u16(&self.source, "reveal source")?,
            destination_tile: level_editor_forms::parse_hex_u16(
                &self.destination,
                "reveal destination",
            )?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EndpointForm {
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) submap: String,
}

impl EndpointForm {
    pub(crate) fn load(value: OverworldEndpoint) -> Self {
        Self {
            x: format!("{:04X}", value.x),
            y: format!("{:04X}", value.y),
            submap: format!("{:02X}", value.submap),
        }
    }

    pub(crate) fn parse(&self) -> Result<OverworldEndpoint, String> {
        Ok(OverworldEndpoint {
            x: level_editor_forms::parse_hex_u16(&self.x, "endpoint X")?,
            y: level_editor_forms::parse_hex_u16(&self.y, "endpoint Y")?,
            submap: level_editor_forms::parse_hex_u8(&self.submap, "endpoint submap")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SpriteForm {
    pub(crate) id: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) submap: usize,
    pub(crate) extra: String,
}

impl SpriteForm {
    pub(crate) fn load(value: &OverworldSprite) -> Self {
        Self {
            id: format!("{:04X}", value.id),
            x: format!("{:04X}", value.x),
            y: format!("{:04X}", value.y),
            submap: usize::from(value.submap.encoded()),
            extra: level_editor_forms::format_bytes(&value.extra),
        }
    }

    pub(crate) fn parse(&self, extra_len: usize) -> Result<OverworldSprite, String> {
        let extra = level_editor_forms::parse_bytes(&self.extra, "sprite extension byte")?;
        if extra.len() != extra_len {
            return Err(format!(
                "overworld sprite requires {extra_len} extension bytes, got {}",
                extra.len()
            ));
        }
        let submap = Submap::decode(u8::try_from(self.submap).unwrap_or(u8::MAX))
            .ok_or("invalid overworld submap")?;
        Ok(OverworldSprite {
            id: level_editor_forms::parse_hex_u16(&self.id, "sprite ID")?,
            x: level_editor_forms::parse_hex_u16(&self.x, "sprite X")?,
            y: level_editor_forms::parse_hex_u16(&self.y, "sprite Y")?,
            submap,
            extra,
        })
    }
}

pub(crate) const SUBMAP_NAMES: [&str; 7] = [
    "Main",
    "Yoshi's Island",
    "Vanilla Dome",
    "Forest of Illusion",
    "Valley of Bowser",
    "Special World",
    "Star World",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_form_preserves_extension_and_submap() {
        let sprite = OverworldSprite {
            id: 0x123,
            x: 4,
            y: 5,
            submap: Submap::StarWorld,
            extra: vec![0xaa, 0xbb],
        };
        assert_eq!(SpriteForm::load(&sprite).parse(2).unwrap(), sprite);
        assert!(SpriteForm::load(&sprite).parse(1).is_err());
    }
}
