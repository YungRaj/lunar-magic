use lm_level::{AppearanceSource, EntityAppearanceRecord};

#[derive(Clone, Debug, Default)]
pub(crate) struct AppearanceForm {
    pub(crate) source_kind: usize,
    pub(crate) source_id: String,
    pub(crate) tile_index: String,
    pub(crate) palette_index: u8,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) x_flip: bool,
    pub(crate) y_flip: bool,
}

impl AppearanceForm {
    pub(crate) fn load(value: EntityAppearanceRecord) -> Self {
        let (source_kind, source_id) = match value.source {
            AppearanceSource::Layer1Object(id) => (0, id),
            AppearanceSource::Layer2Object(id) => (1, id),
            AppearanceSource::Sprite(id) => (2, id),
        };
        Self {
            source_kind,
            source_id: format!("{source_id:08X}"),
            tile_index: format!("{:04X}", value.tile_index),
            palette_index: value.palette_index,
            x: value.x.to_string(),
            y: value.y.to_string(),
            x_flip: value.x_flip,
            y_flip: value.y_flip,
        }
    }

    pub(crate) fn parse(&self) -> Result<EntityAppearanceRecord, String> {
        let source_id = parse_hex::<u32>(&self.source_id, "source ID")?;
        let source = match self.source_kind {
            0 => AppearanceSource::Layer1Object(source_id),
            1 => AppearanceSource::Layer2Object(source_id),
            2 => AppearanceSource::Sprite(source_id),
            _ => return Err("invalid appearance source kind".into()),
        };
        Ok(EntityAppearanceRecord {
            source,
            tile_index: parse_hex(&self.tile_index, "tile index")?,
            palette_index: self.palette_index,
            x: parse_decimal(&self.x, "X offset")?,
            y: parse_decimal(&self.y, "Y offset")?,
            x_flip: self.x_flip,
            y_flip: self.y_flip,
        })
    }
}

pub(crate) const SOURCE_NAMES: [&str; 3] = ["Layer 1 object", "Layer 2 object", "Sprite"];

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
    fn form_round_trips_signed_offsets_and_source_width() {
        let value = EntityAppearanceRecord {
            source: AppearanceSource::Sprite(0xfeed_beef),
            tile_index: 0x345,
            palette_index: 7,
            x: -24,
            y: 400,
            x_flip: true,
            y_flip: false,
        };
        assert_eq!(AppearanceForm::load(value).parse().unwrap(), value);
    }
}
