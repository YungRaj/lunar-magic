use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SpriteLengthSource {
    Standard,
    File(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeLevelDocumentOpenSpec {
    pub level: PathBuf,
    pub sprite_lengths: SpriteLengthSource,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<NativeLevelDocumentOpenSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMNLDOC1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let level = spec_text::take_path(&mut fields, "level", base)?;
    let sprite_lengths = match spec_text::take_string(&mut fields, "sprite-lengths")? {
        value if value == "standard" => SpriteLengthSource::Standard,
        value => SpriteLengthSource::File(base.join(value)),
    };
    spec_text::reject_unknown(&fields)?;
    Ok(NativeLevelDocumentOpenSpec {
        level,
        sprite_lengths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_standard_or_file_interpretation_relative_to_spec() {
        let standard = parse(
            "LMNLDOC1\nlevel Level 日本語.lmlvl\nsprite-lengths standard\n",
            Path::new("specs/open.txt"),
        )
        .unwrap();
        assert_eq!(standard.level, Path::new("specs/Level 日本語.lmlvl"));
        assert_eq!(standard.sprite_lengths, SpriteLengthSource::Standard);
        let file = parse(
            "LMNLDOC1\nlevel level.lmlvl\nsprite-lengths tables/lengths.bin\n",
            Path::new("specs/open.txt"),
        )
        .unwrap();
        assert_eq!(
            file.sprite_lengths,
            SpriteLengthSource::File("specs/tables/lengths.bin".into())
        );
    }

    #[test]
    fn missing_duplicate_and_unknown_fields_fail() {
        assert!(parse("LMNLDOC1\nlevel x\n", Path::new("open.txt")).is_err());
        assert!(
            parse(
                "LMNLDOC1\nlevel x\nlevel y\nsprite-lengths standard\n",
                Path::new("open.txt")
            )
            .is_err()
        );
        assert!(
            parse(
                "LMNLDOC1\nlevel x\nsprite-lengths standard\nunknown x\n",
                Path::new("open.txt")
            )
            .is_err()
        );
    }
}
