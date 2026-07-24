use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CustomSpriteDocumentSpec {
    pub data: PathBuf,
    pub sprite_lengths: PathBuf,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<CustomSpriteDocumentSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMSPDOC1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let data = spec_text::take_path(&mut fields, "data", base)?;
    let sprite_lengths = spec_text::take_path(&mut fields, "sprite-lengths", base)?;
    spec_text::reject_unknown(&fields)?;
    Ok(CustomSpriteDocumentSpec {
        data,
        sprite_lengths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_both_paths_relative_to_specification() {
        let spec = parse(
            "LMSPDOC1\ndata sprites/Level 日本語.mw2\nsprite-lengths tables/lengths.bin\n",
            Path::new("project/open.txt"),
        )
        .unwrap();
        assert_eq!(spec.data, Path::new("project/sprites/Level 日本語.mw2"));
        assert_eq!(spec.sprite_lengths, Path::new("project/tables/lengths.bin"));
    }

    #[test]
    fn missing_duplicate_and_unknown_fields_fail() {
        assert!(parse("LMSPDOC1\ndata x\n", Path::new("x")).is_err());
        assert!(
            parse(
                "LMSPDOC1\ndata x\ndata y\nsprite-lengths z\n",
                Path::new("x")
            )
            .is_err()
        );
        assert!(
            parse(
                "LMSPDOC1\ndata x\nsprite-lengths z\nunknown y\n",
                Path::new("x")
            )
            .is_err()
        );
    }
}
