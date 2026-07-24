use crate::level_editor_forms;
use lm_level::{CustomObjectEntry, CustomObjectLibrary, DescriptionFormat, LineEnding};

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomObjectForm {
    pub(crate) object_bytes: String,
    pub(crate) description: String,
    pub(crate) insert_index: usize,
    pub(crate) move_to: usize,
}

impl CustomObjectForm {
    pub(crate) fn load(entry: &CustomObjectEntry, index: usize) -> Self {
        Self {
            object_bytes: level_editor_forms::format_bytes(entry.object.encoded()),
            description: entry.description.clone(),
            insert_index: index,
            move_to: index,
        }
    }

    pub(crate) fn entry(&self) -> Result<CustomObjectEntry, String> {
        CustomObjectEntry::new(
            level_editor_forms::parse_object(&self.object_bytes)?,
            self.description.clone(),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DescriptionFormatForm {
    pub(crate) utf8_bom: bool,
    pub(crate) crlf: bool,
    pub(crate) trailing_line_ending: bool,
}

impl DescriptionFormatForm {
    pub(crate) fn load(library: &CustomObjectLibrary) -> Self {
        Self::load_value(library.description_format())
    }

    pub(crate) fn load_value(value: DescriptionFormat) -> Self {
        Self {
            utf8_bom: value.utf8_bom,
            crlf: value.line_ending == LineEnding::CrLf,
            trailing_line_ending: value.trailing_line_ending,
        }
    }

    pub(crate) const fn value(self) -> DescriptionFormat {
        DescriptionFormat {
            utf8_bom: self.utf8_bom,
            line_ending: if self.crlf {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
            trailing_line_ending: self.trailing_line_ending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::ObjectRecord;

    #[test]
    fn entry_form_round_trips_variable_width_object_and_unicode() {
        let value = CustomObjectEntry::new(
            ObjectRecord::new(vec![1, 2, 3, 4, 5]).unwrap(),
            "Coin ★".into(),
        )
        .unwrap();
        assert_eq!(CustomObjectForm::load(&value, 0).entry().unwrap(), value);
    }

    #[test]
    fn entry_form_rejects_description_line_separators() {
        let form = CustomObjectForm {
            object_bytes: "01 02 03".into(),
            description: "two\nlines".into(),
            ..CustomObjectForm::default()
        };
        assert!(form.entry().is_err());
    }
}
