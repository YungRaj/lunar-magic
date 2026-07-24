use crate::spec_text::{self, SpecError};
use std::path::{Path, PathBuf};

pub(crate) const MAGIC: &str = "LMRPINS1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionPatchInstallSpec {
    pub template: PathBuf,
    pub search: std::ops::Range<usize>,
    pub fill: u8,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<RevisionPatchInstallSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, MAGIC)?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let template = spec_text::take_path(&mut fields, "template", base)?;
    let search_start = take_hex(&mut fields, "search-start")?;
    let search_end = take_hex(&mut fields, "search-end")?;
    let fill = u8::try_from(take_hex(&mut fields, "fill")?)
        .map_err(|_| spec_text::error("revision patch fill exceeds one byte"))?;
    spec_text::reject_unknown(&fields)?;
    if search_start >= search_end {
        return Err(spec_text::error(
            "revision patch search range must be nonempty",
        ));
    }
    Ok(RevisionPatchInstallSpec {
        template,
        search: search_start..search_end,
        fill,
    })
}

fn take_hex(fields: &mut spec_text::Fields, key: &str) -> Result<usize, SpecError> {
    let value = spec_text::take_string(fields, key)?;
    usize::from_str_radix(value.trim_start_matches("0x"), 16).map_err(|_| {
        spec_text::error(format!(
            "revision patch specification has invalid hexadecimal {key}: {value:?}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_template_and_rejects_bad_ranges_or_fill() {
        let spec = parse(
            "LMRPINS1\ntemplate runtimes/layer3.lmpatch\nsearch-start 300000\nsearch-end 400000\nfill ff\n",
            Path::new("profiles/install.txt"),
        )
        .unwrap();
        assert_eq!(spec.template, Path::new("profiles/runtimes/layer3.lmpatch"));
        assert_eq!(spec.search, 0x30_0000..0x40_0000);
        assert_eq!(spec.fill, 0xff);
        assert!(
            parse(
                "LMRPINS1\ntemplate p\nsearch-start 20\nsearch-end 10\nfill ff\n",
                Path::new("s"),
            )
            .is_err()
        );
        assert!(
            parse(
                "LMRPINS1\ntemplate p\nsearch-start 10\nsearch-end 20\nfill 100\n",
                Path::new("s"),
            )
            .is_err()
        );
    }
}
