use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeAssetsDocumentOpenSpec {
    pub document: PathBuf,
    pub profile: PathBuf,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<NativeAssetsDocumentOpenSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMNADOC1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = NativeAssetsDocumentOpenSpec {
        document: spec_text::take_path(&mut fields, "document", base)?,
        profile: spec_text::take_path(&mut fields, "profile", base)?,
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_both_interpretation_bound_paths() {
        let value = parse(
            "LMNADOC1\ndocument Aggregate 日本語.lmnat\nprofile Profiles/US.txt\n",
            Path::new("project/open.txt"),
        )
        .unwrap();
        assert_eq!(value.document, Path::new("project/Aggregate 日本語.lmnat"));
        assert_eq!(value.profile, Path::new("project/Profiles/US.txt"));
        assert!(parse("LMNADOC1\ndocument x\n", Path::new("open.txt")).is_err());
    }
}
