use crate::spec_text::{self, SpecError};
use lm_app::NativeMap16SidecarDocumentKind;
use std::path::{Path, PathBuf};

pub(crate) struct NativeMap16SidecarSpec {
    pub kind: NativeMap16SidecarDocumentKind,
    pub file: PathBuf,
}

pub(crate) fn parse(text: &str, path: &Path) -> Result<NativeMap16SidecarSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMN16DC1")?;
    let kind = match spec_text::take_string(&mut fields, "kind")?.as_str() {
        "m16" => NativeMap16SidecarDocumentKind::M16,
        "s16" => NativeMap16SidecarDocumentKind::S16,
        _ => {
            return Err(spec_text::error(
                "native Map16 sidecar kind must be m16 or s16",
            ));
        }
    };
    let file = spec_text::take_path(
        &mut fields,
        "file",
        path.parent().unwrap_or_else(|| Path::new("")),
    )?;
    spec_text::reject_unknown(&fields)?;
    Ok(NativeMap16SidecarSpec { kind, file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_and_relative_unicode_path_are_bound() {
        let spec = parse(
            "LMN16DC1\nkind s16\nfile Sidecars/Sprites 日本語.s16\n",
            Path::new("project/open.txt"),
        )
        .unwrap();
        assert_eq!(spec.kind, NativeMap16SidecarDocumentKind::S16);
        assert_eq!(spec.file, Path::new("project/Sidecars/Sprites 日本語.s16"));
    }

    #[test]
    fn bad_kind_missing_duplicate_and_unknown_fields_fail() {
        assert!(parse("LMN16DC1\nkind bad\nfile x\n", Path::new("x")).is_err());
        assert!(parse("LMN16DC1\nkind m16\n", Path::new("x")).is_err());
        assert!(parse("LMN16DC1\nkind m16\nfile x\nfile y\n", Path::new("x")).is_err());
        assert!(parse("LMN16DC1\nkind m16\nfile x\nextra y\n", Path::new("x")).is_err());
    }
}
