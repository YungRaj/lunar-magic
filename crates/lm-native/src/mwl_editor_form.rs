use crate::level_editor_forms;
use lm_app::MwlDocumentEdit;
use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind};

#[derive(Clone, Debug, Default)]
pub(crate) struct MwlForm {
    pub(crate) flags: String,
    pub(crate) attribution: String,
    pub(crate) level_number: String,
    pub(crate) section_index: usize,
    pub(crate) section_bytes: String,
}

impl MwlForm {
    pub(crate) fn load_header(file: &MwlFile) -> Self {
        let level_number = MwlLevelHeaderSection::decode(
            &file.sections[MwlSectionKind::LevelHeader as usize].bytes,
        )
        .map_or_else(
            |_| String::new(),
            |header| format!("{:04X}", header.level_number()),
        );
        Self {
            flags: format!("{:08X}", file.flags),
            attribution: level_editor_forms::format_bytes(&file.attribution),
            level_number,
            ..Self::default()
        }
    }

    pub(crate) fn load_section(&mut self, file: &MwlFile, index: usize) {
        self.section_index = index.min(MwlFile::SECTION_COUNT - 1);
        self.section_bytes =
            level_editor_forms::format_bytes(&file.sections[self.section_index].bytes);
    }

    pub(crate) fn header_edits(&self) -> Result<Vec<MwlDocumentEdit>, String> {
        let attribution =
            level_editor_forms::parse_bytes(&self.attribution, "MWL attribution byte")?;
        let attribution: [u8; MwlFile::ATTRIBUTION_LEN] =
            attribution.try_into().map_err(|value: Vec<u8>| {
                format!(
                    "MWL attribution requires {} bytes, got {}",
                    MwlFile::ATTRIBUTION_LEN,
                    value.len()
                )
            })?;
        let mut edits = vec![
            MwlDocumentEdit::SetFlags(level_editor_forms::parse_hex_u32(&self.flags, "MWL flags")?),
            MwlDocumentEdit::SetAttribution(attribution),
        ];
        if !self.level_number.trim().is_empty() {
            edits.push(MwlDocumentEdit::SetLevelNumber(
                level_editor_forms::parse_hex_u16(&self.level_number, "MWL level number")?,
            ));
        }
        Ok(edits)
    }

    pub(crate) fn section_edit(&self) -> Result<MwlDocumentEdit, String> {
        Ok(MwlDocumentEdit::ReplaceSection {
            section: section_kind(self.section_index),
            bytes: level_editor_forms::parse_bytes(&self.section_bytes, "MWL section byte")?,
        })
    }
}

pub(crate) const SECTION_NAMES: [&str; MwlFile::SECTION_COUNT] = [
    "Level header",
    "Layer 1",
    "Layer 2",
    "Sprites",
    "Palette",
    "Secondary exits",
    "ExAnimation",
    "Expanded header",
];

const fn section_kind(index: usize) -> MwlSectionKind {
    match index {
        0 => MwlSectionKind::LevelHeader,
        1 => MwlSectionKind::Layer1,
        2 => MwlSectionKind::Layer2,
        3 => MwlSectionKind::Sprites,
        4 => MwlSectionKind::Palette,
        5 => MwlSectionKind::SecondaryExits,
        6 => MwlSectionKind::ExAnimation,
        _ => MwlSectionKind::ExpandedHeader,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MwlSection;

    fn file() -> MwlFile {
        let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
            std::array::from_fn(|_| MwlSection::default());
        let mut header = MwlLevelHeaderSection([0x5a; MwlLevelHeaderSection::ENCODED_LEN]);
        header.set_level_number(0x105);
        sections[0].bytes = header.0.to_vec();
        MwlFile {
            version: MwlFile::CURRENT_VERSION,
            flags: 0x9234_5678,
            attribution: [0xa5; MwlFile::ATTRIBUTION_LEN],
            sections,
        }
    }

    #[test]
    fn header_form_preserves_all_attribution_bytes() {
        let form = MwlForm::load_header(&file());
        let edits = form.header_edits().unwrap();
        assert_eq!(edits.len(), 3);
        assert_eq!(
            edits[1],
            MwlDocumentEdit::SetAttribution([0xa5; MwlFile::ATTRIBUTION_LEN])
        );
    }

    #[test]
    fn section_form_keeps_empty_and_opaque_sections_distinct() {
        let mut file = file();
        file.sections[3].bytes = vec![0, 0x81, 0xff];
        let mut form = MwlForm::load_header(&file);
        form.load_section(&file, 3);
        assert!(matches!(
            form.section_edit().unwrap(),
            MwlDocumentEdit::ReplaceSection {
                section: MwlSectionKind::Sprites,
                bytes
            } if bytes == [0, 0x81, 0xff]
        ));
    }
}
