use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MwlOptionalAssetsImportSpec {
    pub source: PathBuf,
    pub size_modes: PathBuf,
    pub maximum_records: usize,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<MwlOptionalAssetsImportSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMMWLOPT1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = MwlOptionalAssetsImportSpec {
        source: spec_text::take_path(&mut fields, "source", base)?,
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
    fn binds_relative_inputs_and_interpretation_limits() {
        let spec = parse(
            "LMMWLOPT1\nsource Source 日本語.mwl\nsize-modes modes.bin\nmaximum-records 32\n",
            Path::new("specs/import.txt"),
        )
        .unwrap();
        assert_eq!(spec.source, Path::new("specs/Source 日本語.mwl"));
        assert_eq!(spec.size_modes, Path::new("specs/modes.bin"));
        assert_eq!(spec.maximum_records, 32);
    }

    #[test]
    fn rejects_missing_duplicate_and_unknown_fields() {
        assert!(parse("LMMWLOPT1\n", Path::new("import.txt")).is_err());
        assert!(
            parse(
                "LMMWLOPT1\nsource a\nsource b\nsize-modes m\nmaximum-records 1\n",
                Path::new("import.txt"),
            )
            .is_err()
        );
        assert!(
            parse(
                "LMMWLOPT1\nsource a\nsize-modes m\nmaximum-records 1\nextra x\n",
                Path::new("import.txt"),
            )
            .is_err()
        );
    }
}
