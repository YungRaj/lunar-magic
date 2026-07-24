use lm_overworld::SpriteAppearancePart;

#[derive(Clone, Debug, Default)]
pub(crate) struct DefinitionForm {
    pub(crate) sprite_id: String,
    pub(crate) insert_index: usize,
    pub(crate) move_before: usize,
    pub(crate) move_to_end: bool,
}

impl DefinitionForm {
    pub(crate) fn load(sprite_id: u16, index: usize) -> Self {
        Self {
            sprite_id: format!("{sprite_id:04X}"),
            insert_index: index,
            move_before: index,
            move_to_end: false,
        }
    }

    pub(crate) fn sprite_id(&self) -> Result<u16, String> {
        parse_hex(&self.sprite_id, "sprite ID")
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PartForm {
    pub(crate) tile_index: String,
    pub(crate) palette_index: u8,
    pub(crate) x_offset: String,
    pub(crate) y_offset: String,
    pub(crate) x_flip: bool,
    pub(crate) y_flip: bool,
    pub(crate) insert_index: usize,
}

impl PartForm {
    pub(crate) fn load(value: SpriteAppearancePart, index: usize) -> Self {
        Self {
            tile_index: format!("{:04X}", value.tile_index),
            palette_index: value.palette_index,
            x_offset: value.x_offset.to_string(),
            y_offset: value.y_offset.to_string(),
            x_flip: value.x_flip,
            y_flip: value.y_flip,
            insert_index: index,
        }
    }

    pub(crate) fn parse(&self) -> Result<SpriteAppearancePart, String> {
        Ok(SpriteAppearancePart {
            tile_index: parse_hex(&self.tile_index, "tile index")?,
            palette_index: self.palette_index,
            x_offset: parse_decimal(&self.x_offset, "X offset")?,
            y_offset: parse_decimal(&self.y_offset, "Y offset")?,
            x_flip: self.x_flip,
            y_flip: self.y_flip,
        })
    }
}

fn parse_hex<T>(text: &str, name: &str) -> Result<T, String>
where
    T: TryFrom<u64>,
{
    let text = text
        .trim()
        .strip_prefix("0x")
        .or_else(|| text.trim().strip_prefix("0X"))
        .unwrap_or(text.trim());
    u64::from_str_radix(text, 16)
        .ok()
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| format!("invalid hexadecimal {name}"))
}

fn parse_decimal<T>(text: &str, name: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    text.trim()
        .parse()
        .map_err(|_| format!("invalid decimal {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_form_round_trips_signed_offsets_and_flips() {
        let value = SpriteAppearancePart {
            tile_index: 0x345,
            palette_index: 7,
            x_offset: -123,
            y_offset: 456,
            x_flip: true,
            y_flip: true,
        };
        assert_eq!(PartForm::load(value, 3).parse().unwrap(), value);
    }

    #[test]
    fn definition_form_retains_full_sprite_id() {
        assert_eq!(DefinitionForm::load(0xfedc, 0).sprite_id().unwrap(), 0xfedc);
    }
}
