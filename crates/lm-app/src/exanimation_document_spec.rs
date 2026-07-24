use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExAnimationDocumentOpenSpec {
    pub animation: PathBuf,
    pub size_modes: PathBuf,
    pub maximum_records: usize,
}

pub(crate) fn parse_exanimation_document_open_spec(
    text: &str,
    path: &Path,
) -> Result<ExAnimationDocumentOpenSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMEXDOC1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = ExAnimationDocumentOpenSpec {
        animation: spec_text::take_path(&mut fields, "animation", base)?,
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
    fn interpretation_is_bound() {
        let value = parse_exanimation_document_open_spec("LMEXDOC1\nanimation Animation 日本語.lmexan\nsize-modes modes.bin\nmaximum-records 32\n", Path::new("specs/open.txt")).unwrap();
        assert_eq!(value.animation, Path::new("specs/Animation 日本語.lmexan"));
        assert_eq!(value.maximum_records, 32);
    }
}
