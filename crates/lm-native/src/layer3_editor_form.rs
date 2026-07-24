use crate::level_editor_forms;
use lm_level::{Layer3Data, Layer3Edit};

#[derive(Clone, Debug, Default)]
pub(crate) struct Layer3Form {
    pub(crate) selectors: [u8; 4],
    pub(crate) graphics: [u16; 4],
    pub(crate) reserved: String,
    pub(crate) tilemap: String,
    pub(crate) remap: String,
}

impl Layer3Form {
    pub(crate) fn load(value: &Layer3Data) -> Self {
        Self {
            selectors: [
                value.settings.start_position,
                value.settings.tilemap_size,
                value.settings.liquid_type,
                value.settings.flags,
            ],
            graphics: value.settings.graphics_files,
            reserved: level_editor_forms::format_bytes(&value.settings.reserved),
            tilemap: level_editor_forms::format_bytes(&value.tilemap),
            remap: level_editor_forms::format_bytes(&value.remap_commands),
        }
    }

    pub(crate) fn edits(&self) -> Result<Vec<Layer3Edit>, String> {
        let reserved = level_editor_forms::parse_bytes(&self.reserved, "Layer 3 reserved byte")?;
        let reserved: [u8; 16] = reserved.try_into().map_err(|value: Vec<u8>| {
            format!(
                "Layer 3 reserved field requires 16 bytes, got {}",
                value.len()
            )
        })?;
        let mut edits = vec![
            Layer3Edit::SetStartPosition(self.selectors[0]),
            Layer3Edit::SetTilemapSize(self.selectors[1]),
            Layer3Edit::SetLiquidType(self.selectors[2]),
            Layer3Edit::SetFlags(self.selectors[3]),
        ];
        edits.extend(
            self.graphics
                .iter()
                .copied()
                .enumerate()
                .map(|(slot, file)| Layer3Edit::SetGraphicsFile { slot, file }),
        );
        edits.extend([
            Layer3Edit::SetReserved(reserved),
            Layer3Edit::ReplaceTilemap(level_editor_forms::parse_bytes(
                &self.tilemap,
                "Layer 3 tilemap byte",
            )?),
            Layer3Edit::ReplaceRemapCommands(level_editor_forms::parse_bytes(
                &self.remap,
                "Layer 3 remap byte",
            )?),
        ]);
        Ok(edits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Layer3Settings, Level};

    #[test]
    fn form_batch_recreates_exact_layer3_value() {
        let expected = Layer3Data {
            settings: Layer3Settings {
                start_position: 0xfe,
                tilemap_size: 3,
                liquid_type: 0x81,
                flags: 0xa5,
                graphics_files: [0, 0x123, 0xabc, 0xfff],
                reserved: [0x5a; 16],
            },
            tilemap: vec![0, 1, 2, 0xff],
            remap_commands: vec![0x80, 3, 4],
        };
        let mut level = Level {
            layer3: Some(Layer3Data::default()),
            ..Level::default()
        };
        level
            .apply_layer3_edits(&Layer3Form::load(&expected).edits().unwrap())
            .unwrap();
        assert_eq!(level.layer3, Some(expected));
    }

    #[test]
    fn form_rejects_reserved_width_and_oversized_graphics_id() {
        let mut form = Layer3Form::load(&Layer3Data::default());
        form.reserved = ["00"; 15].join(" ");
        assert!(form.edits().is_err());
        form = Layer3Form::load(&Layer3Data::default());
        form.graphics[0] = 0x1000;
        let mut level = Level {
            layer3: Some(Layer3Data::default()),
            ..Level::default()
        };
        assert!(level.apply_layer3_edits(&form.edits().unwrap()).is_err());
    }
}
