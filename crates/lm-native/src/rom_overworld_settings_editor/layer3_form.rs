use lm_overworld::{
    OVERWORLD_LAYER3_GFX_SLOTS, OVERWORLD_LAYER3_LAYOUT_WORDS, OverworldLayer3SettingsRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Layer3Form {
    pub uses_custom_tilemap: bool,
    pub uses_custom_graphics: bool,
    pub tilemap_file: u16,
    pub tilemap_size: u8,
    pub tilemap_position: u8,
    pub layout_words: [u16; OVERWORLD_LAYER3_LAYOUT_WORDS],
    pub graphics_files: [u16; OVERWORLD_LAYER3_GFX_SLOTS],
}

impl Default for Layer3Form {
    fn default() -> Self {
        Self {
            uses_custom_tilemap: false,
            uses_custom_graphics: false,
            tilemap_file: 0,
            tilemap_size: 0,
            tilemap_position: 0,
            layout_words: [0; OVERWORLD_LAYER3_LAYOUT_WORDS],
            graphics_files: [0; OVERWORLD_LAYER3_GFX_SLOTS],
        }
    }
}

impl Layer3Form {
    pub(super) fn load(record: &OverworldLayer3SettingsRecord) -> Self {
        Self {
            uses_custom_tilemap: record.uses_custom_tilemap(),
            uses_custom_graphics: record.uses_custom_graphics(),
            tilemap_file: record.tilemap_file(),
            tilemap_size: record.tilemap_size(),
            tilemap_position: record.tilemap_position(),
            layout_words: std::array::from_fn(|index| {
                record
                    .address_layout_word(index)
                    .expect("bounded address-layout index")
            }),
            graphics_files: std::array::from_fn(|index| {
                record
                    .graphics_file(index)
                    .expect("bounded graphics-file index")
            }),
        }
    }

    pub(super) fn apply(
        &self,
        source: &OverworldLayer3SettingsRecord,
    ) -> Result<OverworldLayer3SettingsRecord, String> {
        let mut edited = source.clone();
        edited.set_uses_custom_tilemap(self.uses_custom_tilemap);
        edited.set_uses_custom_graphics(self.uses_custom_graphics);
        edited
            .set_tilemap_file(self.tilemap_file)
            .map_err(|error| error.to_string())?;
        edited
            .set_tilemap_size(self.tilemap_size)
            .map_err(|error| error.to_string())?;
        edited
            .set_tilemap_position(self.tilemap_position)
            .map_err(|error| error.to_string())?;
        for (index, value) in self.layout_words.into_iter().enumerate() {
            edited
                .set_address_layout_word(index, value)
                .map_err(|error| error.to_string())?;
        }
        for (index, value) in self.graphics_files.into_iter().enumerate() {
            edited
                .set_graphics_file(index, value)
                .map_err(|error| error.to_string())?;
        }
        Ok(edited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_preserves_opaque_flags_bytes_and_graphics_high_nibbles() {
        let bytes = std::array::from_fn(|index| u8::try_from(index).unwrap() ^ 0xa5);
        let source = OverworldLayer3SettingsRecord::from_bytes(bytes);
        let mut form = Layer3Form::load(&source);
        form.uses_custom_tilemap = true;
        form.uses_custom_graphics = false;
        form.tilemap_file = 0x456;
        form.graphics_files[3] = 0xabc;
        let edited = form.apply(&source).unwrap();
        assert_eq!(edited.preserved_bytes(), source.preserved_bytes());
        assert_eq!(
            edited.feature_flags() & !0x6000,
            source.feature_flags() & !0x6000
        );
        assert_eq!(edited.encoded()[0x1f] & 0xf0, source.encoded()[0x1f] & 0xf0);
        assert_eq!(edited.tilemap_file(), 0x456);
        assert_eq!(edited.graphics_file(3), Some(0xabc));
    }
}
