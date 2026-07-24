use lm_level::{Map16Quadrant, Map16Tile, Subtile};

#[derive(Clone, Debug, Default)]
pub(crate) struct SubtileForm {
    pub(crate) tile: String,
    pub(crate) palette: u8,
    pub(crate) priority: bool,
    pub(crate) x_flip: bool,
    pub(crate) y_flip: bool,
}

impl SubtileForm {
    pub(crate) fn from_subtile(value: Subtile) -> Self {
        Self {
            tile: format!("{:03X}", value.tile_number()),
            palette: value.palette(),
            priority: value.priority(),
            x_flip: value.x_flip(),
            y_flip: value.y_flip(),
        }
    }

    pub(crate) fn parse(&self) -> Result<Subtile, String> {
        let tile = u16::from_str_radix(self.tile.trim(), 16)
            .map_err(|error| format!("invalid 8×8 tile: {error}"))?;
        if tile > 0x03ff {
            return Err("8×8 tile exceeds 10-bit range".into());
        }
        let mut word = tile | (u16::from(self.palette) << 10);
        word |= u16::from(self.priority) << 13;
        word |= u16::from(self.x_flip) << 14;
        word |= u16::from(self.y_flip) << 15;
        Ok(Subtile(word))
    }
}

pub(crate) fn quadrant(index: usize) -> Map16Quadrant {
    [
        Map16Quadrant::TopLeft,
        Map16Quadrant::TopRight,
        Map16Quadrant::BottomLeft,
        Map16Quadrant::BottomRight,
    ][index.min(3)]
}

pub(crate) fn quadrant_name(index: usize) -> &'static str {
    ["Top left", "Top right", "Bottom left", "Bottom right"][index.min(3)]
}

pub(crate) fn quadrant_value(tile: Map16Tile, index: usize) -> Subtile {
    [
        tile.top_left,
        tile.top_right,
        tile.bottom_left,
        tile.bottom_right,
    ][index.min(3)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtile_form_preserves_all_packed_fields() {
        let value = SubtileForm {
            tile: "3AB".into(),
            palette: 6,
            priority: true,
            x_flip: true,
            y_flip: false,
        }
        .parse()
        .unwrap();
        assert_eq!(value.tile_number(), 0x3ab);
        assert_eq!(value.palette(), 6);
        assert!(value.priority());
        assert!(value.x_flip());
        assert!(!value.y_flip());
    }

    #[test]
    fn subtile_form_rejects_values_outside_ten_bits() {
        assert!(
            SubtileForm {
                tile: "400".into(),
                ..SubtileForm::default()
            }
            .parse()
            .is_err()
        );
    }
}
