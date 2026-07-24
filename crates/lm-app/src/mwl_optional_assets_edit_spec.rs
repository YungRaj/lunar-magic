use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MwlOptionalAssetsEditSpec {
    pub edits: PathBuf,
    pub size_modes: PathBuf,
    pub maximum_records: usize,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<MwlOptionalAssetsEditSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMMWLOES1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = MwlOptionalAssetsEditSpec {
        edits: spec_text::take_path(&mut fields, "edits", base)?,
        size_modes: spec_text::take_path(&mut fields, "size-modes", base)?,
        maximum_records: spec_text::take_usize(&mut fields, "maximum-records")?,
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_all_interpretation_inputs_relative_to_spec() {
        let value = parse(
            "LMMWLOES1\nedits Edit 日本語.txt\nsize-modes modes.bin\nmaximum-records 32\n",
            Path::new("specs/optional.txt"),
        )
        .unwrap();
        assert_eq!(value.edits, Path::new("specs/Edit 日本語.txt"));
        assert_eq!(value.size_modes, Path::new("specs/modes.bin"));
        assert_eq!(value.maximum_records, 32);
    }
}
