use crate::{
    spec_text::{self, SpecError},
    viewport_spec::{self, ViewportSpec},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OverworldRenderSpec {
    pub overworld: PathBuf,
    pub size_modes: PathBuf,
    pub maximum_animation_records: usize,
    pub map16: PathBuf,
    pub graphics: PathBuf,
    pub appearances: Option<PathBuf>,
    pub animation_frame: Option<PathBuf>,
    pub completed_reveals: usize,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OverworldDocumentOpenSpec {
    pub overworld: PathBuf,
    pub size_modes: PathBuf,
    pub maximum_animation_records: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OverworldDocumentRenderSpec {
    pub map16: PathBuf,
    pub graphics: PathBuf,
    pub appearances: Option<PathBuf>,
    pub animation_frame: Option<PathBuf>,
    pub completed_reveals: usize,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

pub(crate) fn parse_overworld_render_spec(
    text: &str,
    path: &Path,
) -> Result<OverworldRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMOWRND1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = OverworldRenderSpec {
        overworld: spec_text::take_path(&mut fields, "overworld", base)?,
        size_modes: spec_text::take_path(&mut fields, "size-modes", base)?,
        maximum_animation_records: spec_text::take_usize(&mut fields, "maximum-animation-records")?,
        map16: spec_text::take_path(&mut fields, "map16", base)?,
        graphics: spec_text::take_path(&mut fields, "graphics", base)?,
        appearances: spec_text::take_optional_path(&mut fields, "appearances", base),
        animation_frame: spec_text::take_optional_path(&mut fields, "animation-frame", base),
        completed_reveals: spec_text::take_usize(&mut fields, "completed-reveals")?,
        output: spec_text::take_path(&mut fields, "output", base)?,
        viewport: viewport_spec::take_optional(&mut fields)?,
        overlays: spec_text::take_optional_path(&mut fields, "overlays", base),
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

pub(crate) fn parse_overworld_document_open_spec(
    text: &str,
    path: &Path,
) -> Result<OverworldDocumentOpenSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMOWDOC1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = OverworldDocumentOpenSpec {
        overworld: spec_text::take_path(&mut fields, "overworld", base)?,
        size_modes: spec_text::take_path(&mut fields, "size-modes", base)?,
        maximum_animation_records: spec_text::take_usize(&mut fields, "maximum-animation-records")?,
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

pub(crate) fn parse_overworld_document_render_spec(
    text: &str,
    path: &Path,
) -> Result<OverworldDocumentRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMOWDRN1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = OverworldDocumentRenderSpec {
        map16: spec_text::take_path(&mut fields, "map16", base)?,
        graphics: spec_text::take_path(&mut fields, "graphics", base)?,
        appearances: spec_text::take_optional_path(&mut fields, "appearances", base),
        animation_frame: spec_text::take_optional_path(&mut fields, "animation-frame", base),
        completed_reveals: spec_text::take_usize(&mut fields, "completed-reveals")?,
        output: spec_text::take_path(&mut fields, "output", base)?,
        viewport: viewport_spec::take_optional(&mut fields)?,
        overlays: spec_text::take_optional_path(&mut fields, "overlays", base),
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standalone_optional_paths_and_numbers_are_typed() {
        let value = parse_overworld_render_spec("LMOWRND1\noverworld world\nsize-modes modes\nmaximum-animation-records 32\nmap16 map\ngraphics gfx\nappearances app\nanimation-frame frame\ncompleted-reveals 7\noutput out\n", Path::new("spec.txt")).unwrap();
        assert_eq!(
            (value.maximum_animation_records, value.completed_reveals),
            (32, 7)
        );
        assert_eq!(value.appearances, Some(PathBuf::from("app")));
        assert_eq!(value.viewport, None);
    }
    #[test]
    fn document_specs_separate_open_from_render() {
        let open = parse_overworld_document_open_spec("LMOWDOC1\noverworld World 日本語.lmow\nsize-modes modes\nmaximum-animation-records 32\n", Path::new("specs/open.txt")).unwrap();
        assert_eq!(open.overworld, Path::new("specs/World 日本語.lmow"));
        let render = parse_overworld_document_render_spec(
            "LMOWDRN1\nmap16 map\ngraphics gfx\ncompleted-reveals 2\noutput World.png\n",
            Path::new("specs/render.txt"),
        )
        .unwrap();
        assert_eq!(render.output, Path::new("specs/World.png"));
    }
    #[test]
    fn standalone_and_document_specs_share_exact_viewport_fields() {
        let camera = "viewport-origin-x -16\nviewport-origin-y 8\nviewport-width 320\nviewport-height 224\nzoom-numerator 2\nzoom-denominator 1\n";
        let standalone = parse_overworld_render_spec(
            &format!("LMOWRND1\noverworld world\nsize-modes modes\nmaximum-animation-records 32\nmap16 map\ngraphics gfx\ncompleted-reveals 0\noutput out\n{camera}"),
            Path::new("spec.txt"),
        )
        .unwrap();
        let document = parse_overworld_document_render_spec(
            &format!(
                "LMOWDRN1\nmap16 map\ngraphics gfx\ncompleted-reveals 0\noutput out\n{camera}"
            ),
            Path::new("spec.txt"),
        )
        .unwrap();
        assert_eq!(standalone.viewport, document.viewport);
        assert_eq!(document.viewport.unwrap().camera.zoom(), (2, 1));
    }
    #[test]
    fn invalid_decimal_fails() {
        assert!(
            parse_overworld_render_spec("LMOWRND1\ncompleted-reveals nope\n", Path::new("x"))
                .is_err()
        );
    }
}
