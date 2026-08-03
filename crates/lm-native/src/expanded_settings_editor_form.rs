use crate::level_editor_forms;
use lm_level::{
    ExpandedLevelHeader, ExpandedLevelSettingsRecord, Layer3TilemapGraphicsDescriptor,
    SuperGraphicsBypass,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct ExpandedSettingsForm {
    pub(crate) words: [String; ExpandedLevelSettingsRecord::WORD_COUNT],
    pub(crate) layer3_enabled: bool,
    pub(crate) layer3_file: String,
    pub(crate) layer3_length_selector: u8,
    pub(crate) layer3_offset_selector: u8,
    pub(crate) layer3_expanded_mode: String,
    pub(crate) bypass_enabled: bool,
    pub(crate) bypass_foreground_background: [u16; 6],
    pub(crate) bypass_sprites: [u16; 4],
    pub(crate) sprites_beyond_boundaries_use_air: bool,
}

impl ExpandedSettingsForm {
    pub(crate) fn load(value: &ExpandedLevelSettingsRecord) -> Self {
        let descriptor = value
            .layer3_tilemap_graphics_descriptor()
            .expect("fixed descriptor word is in range");
        let header = ExpandedLevelHeader::from(value);
        let bypass = header.super_graphics_bypass();
        Self {
            words: std::array::from_fn(|index| {
                format!(
                    "{:04X}",
                    value.word(index).expect("fixed word index is in range")
                )
            }),
            layer3_enabled: value.layer3_tilemap_enabled(),
            layer3_file: format!("{:03X}", descriptor.file()),
            layer3_length_selector: descriptor.length_selector(),
            layer3_offset_selector: descriptor.offset_selector(),
            layer3_expanded_mode: format!("{:08X}", value.layer3_expanded_mode_flags().packed()),
            bypass_enabled: bypass.enabled,
            bypass_foreground_background: bypass.foreground_background,
            bypass_sprites: bypass.sprites,
            sprites_beyond_boundaries_use_air: header.sprites_beyond_boundaries_use_air(),
        }
    }

    pub(crate) fn layer3_expanded_mode(&self) -> Result<lm_level::Layer3ExpandedModeFlags, String> {
        let packed = u32::from_str_radix(
            self.layer3_expanded_mode
                .strip_prefix("0x")
                .unwrap_or(&self.layer3_expanded_mode),
            16,
        )
        .map_err(|_| "Layer 3 expanded mode must be an eight-digit hexadecimal value".to_owned())?;
        Ok(lm_level::Layer3ExpandedModeFlags::from_packed(packed))
    }

    pub(crate) fn layer3_settings(
        &self,
    ) -> Result<
        (
            bool,
            Layer3TilemapGraphicsDescriptor,
            lm_level::Layer3ExpandedModeFlags,
        ),
        String,
    > {
        let file = level_editor_forms::parse_hex_u16(&self.layer3_file, "Layer 3 graphics file")?;
        let descriptor = Layer3TilemapGraphicsDescriptor::new(
            file,
            self.layer3_length_selector,
            self.layer3_offset_selector,
        )
        .map_err(|error| error.to_string())?;
        Ok((
            self.layer3_enabled,
            descriptor,
            self.layer3_expanded_mode()?,
        ))
    }

    pub(crate) fn layer3_edits(&self) -> Result<Vec<(usize, u16)>, String> {
        let mut record = self.record()?;
        record
            .set_layer3_tilemap_enabled(self.layer3_enabled)
            .map_err(|error| error.to_string())?;
        let (_, descriptor, _) = self.layer3_settings()?;
        record
            .set_layer3_tilemap_graphics_descriptor(descriptor)
            .map_err(|error| error.to_string())?;
        Ok(vec![
            (0, record.word(0).expect("fixed word")),
            (1, record.word(1).expect("fixed word")),
        ])
    }

    pub(crate) fn layer3_expanded_mode_edits(&self) -> Result<Vec<(usize, u16)>, String> {
        let mut record = self.record()?;
        record
            .set_layer3_expanded_mode_flags(self.layer3_expanded_mode()?)
            .map_err(|error| error.to_string())?;
        Ok((8..16)
            .map(|word| (word, record.word(word).expect("fixed word")))
            .collect())
    }

    pub(crate) fn super_graphics_bypass_edits(&self) -> Result<Vec<(usize, u16)>, String> {
        let mut header = ExpandedLevelHeader::from(self.record()?);
        header
            .set_super_graphics_bypass(SuperGraphicsBypass {
                enabled: self.bypass_enabled,
                foreground_background: self.bypass_foreground_background,
                sprites: self.bypass_sprites,
            })
            .map_err(|error| error.to_string())?;
        let record = ExpandedLevelSettingsRecord::from(header);
        Ok([0]
            .into_iter()
            .chain(2..=11)
            .map(|word| (word, record.word(word).expect("fixed word")))
            .collect())
    }

    pub(crate) fn sprite_boundary_edits(&self) -> Result<Vec<(usize, u16)>, String> {
        let mut header = ExpandedLevelHeader::from(self.record()?);
        header.set_sprites_beyond_boundaries_use_air(self.sprites_beyond_boundaries_use_air);
        let record = ExpandedLevelSettingsRecord::from(header);
        Ok(vec![(8, record.word(8).expect("fixed word"))])
    }

    fn record(&self) -> Result<ExpandedLevelSettingsRecord, String> {
        let mut bytes = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
        for (index, value) in self.edits()? {
            bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        Ok(ExpandedLevelSettingsRecord::decode(&bytes)
            .expect("form always constructs an exact record"))
    }

    pub(crate) fn edits(&self) -> Result<Vec<(usize, u16)>, String> {
        self.words
            .iter()
            .enumerate()
            .map(|(index, value)| {
                level_editor_forms::parse_hex_u16(value, &format!("expanded word {index:X}"))
                    .map(|value| (index, value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_recreates_all_sixteen_words_exactly() {
        let bytes =
            std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(17));
        let source = ExpandedLevelSettingsRecord::decode(&bytes).unwrap();
        let form = ExpandedSettingsForm::load(&source);
        let mut rebuilt = ExpandedLevelSettingsRecord::decode(&[0; 32]).unwrap();
        for (index, value) in form.edits().unwrap() {
            rebuilt.set_word(index, value).unwrap();
        }
        assert_eq!(rebuilt, source);
    }

    #[test]
    fn malformed_word_rejects_complete_batch() {
        let source = ExpandedLevelSettingsRecord::decode(&[0; 32]).unwrap();
        let mut form = ExpandedSettingsForm::load(&source);
        form.words[15] = "10000".into();
        assert!(form.edits().is_err());
    }

    #[test]
    fn semantic_layer3_form_preserves_unknown_word_zero_bits() {
        let mut bytes = [0; 32];
        bytes[..2].copy_from_slice(&0x8123_u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&0x007f_u16.to_le_bytes());
        let source = ExpandedLevelSettingsRecord::decode(&bytes).unwrap();
        let mut form = ExpandedSettingsForm::load(&source);
        form.layer3_enabled = true;
        form.layer3_file = "ABC".into();
        form.layer3_length_selector = 2;
        form.layer3_offset_selector = 3;
        let edits = form.layer3_edits().unwrap();
        assert_eq!(edits[0], (0, 0xa123));
        assert_eq!(edits[1], (1, 0xeabc));
    }

    #[test]
    fn expanded_mode_form_round_trips_all_thirty_two_bits() {
        let mut source = ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        source
            .set_layer3_expanded_mode_flags(lm_level::Layer3ExpandedModeFlags::from_packed(
                0x89ab_cdef,
            ))
            .unwrap();
        let mut form = ExpandedSettingsForm::load(&source);
        assert_eq!(form.layer3_expanded_mode().unwrap().packed(), 0x89ab_cdef);
        form.layer3_expanded_mode = "100000000".into();
        assert!(form.layer3_expanded_mode().is_err());
    }

    #[test]
    fn semantic_forms_preserve_every_unowned_shared_field() {
        let bytes =
            std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(11));
        let source = ExpandedLevelSettingsRecord::decode(&bytes).unwrap();

        let mut mode_form = ExpandedSettingsForm::load(&source);
        mode_form.layer3_expanded_mode = "89ABCDEF".into();
        let mode = apply(&source, &mode_form.layer3_expanded_mode_edits().unwrap());
        assert_eq!(mode.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
        for word in 8..16 {
            assert_eq!(
                mode.word(word).unwrap() & 0x0fff,
                source.word(word).unwrap() & 0x0fff
            );
        }

        let mut bypass_form = ExpandedSettingsForm::load(&source);
        bypass_form.bypass_enabled = true;
        bypass_form.bypass_foreground_background = [1, 2, 3, 4, 5, 6];
        bypass_form.bypass_sprites = [0x101, 0x202, 0x303, 0x404];
        let bypass = apply(&source, &bypass_form.super_graphics_bypass_edits().unwrap());
        assert_eq!(
            ExpandedLevelHeader::from(&bypass).super_graphics_bypass(),
            SuperGraphicsBypass {
                enabled: true,
                foreground_background: [1, 2, 3, 4, 5, 6],
                sprites: [0x101, 0x202, 0x303, 0x404],
            }
        );
        assert_eq!(bypass.word(1).unwrap(), source.word(1).unwrap());
        for word in 12..16 {
            assert_eq!(bypass.word(word).unwrap(), source.word(word).unwrap());
        }

        let mut boundary_form = ExpandedSettingsForm::load(&source);
        boundary_form.sprites_beyond_boundaries_use_air = true;
        let boundary = apply(&source, &boundary_form.sprite_boundary_edits().unwrap());
        assert!(ExpandedLevelHeader::from(&boundary).sprites_beyond_boundaries_use_air());
        assert_eq!(
            boundary.word(8).unwrap() & !0x4000,
            source.word(8).unwrap() & !0x4000
        );
        for word in 0..16 {
            if word != 8 {
                assert_eq!(boundary.word(word).unwrap(), source.word(word).unwrap());
            }
        }
    }

    fn apply(
        source: &ExpandedLevelSettingsRecord,
        edits: &[(usize, u16)],
    ) -> ExpandedLevelSettingsRecord {
        let mut staged = source.clone();
        for &(word, value) in edits {
            staged.set_word(word, value).unwrap();
        }
        staged
    }
}
