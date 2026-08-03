use crate::level_editor_forms;
use lm_level::{ExpandedLevelSettingsRecord, Layer3TilemapGraphicsDescriptor};

#[derive(Clone, Debug, Default)]
pub(crate) struct ExpandedSettingsForm {
    pub(crate) words: [String; ExpandedLevelSettingsRecord::WORD_COUNT],
    pub(crate) layer3_enabled: bool,
    pub(crate) layer3_file: String,
    pub(crate) layer3_length_selector: u8,
    pub(crate) layer3_offset_selector: u8,
    pub(crate) layer3_expanded_mode: String,
}

impl ExpandedSettingsForm {
    pub(crate) fn load(value: &ExpandedLevelSettingsRecord) -> Self {
        let descriptor = value
            .layer3_tilemap_graphics_descriptor()
            .expect("fixed descriptor word is in range");
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
        let mut bytes = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
        for (index, value) in self.edits()? {
            bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut record = ExpandedLevelSettingsRecord::decode(&bytes)
            .expect("form always constructs an exact record");
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
}
