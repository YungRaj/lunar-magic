use crate::{
    spec_text::{self, SpecError},
    viewport_spec::{self, ViewportSpec},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Map16RenderSpec {
    pub graphics: PathBuf,
    pub palette: PathBuf,
    pub page: PathBuf,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Map16DocumentRenderSpec {
    pub graphics: PathBuf,
    pub palette: PathBuf,
    pub page: usize,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Map16PageDocumentRenderSpec {
    pub graphics: PathBuf,
    pub palette: PathBuf,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

pub(crate) fn parse_map16_render_spec(
    text: &str,
    path: &Path,
) -> Result<Map16RenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMM16R1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = Map16RenderSpec {
        graphics: spec_text::take_path(&mut fields, "graphics", base)?,
        palette: spec_text::take_path(&mut fields, "palette", base)?,
        page: spec_text::take_path(&mut fields, "page", base)?,
        output: spec_text::take_path(&mut fields, "output", base)?,
        viewport: viewport_spec::take_optional(&mut fields)?,
        overlays: spec_text::take_optional_path(&mut fields, "overlays", base),
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

pub(crate) fn parse_map16_document_render_spec(
    text: &str,
    path: &Path,
) -> Result<Map16DocumentRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMM16DR1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = Map16DocumentRenderSpec {
        graphics: spec_text::take_path(&mut fields, "graphics", base)?,
        palette: spec_text::take_path(&mut fields, "palette", base)?,
        page: spec_text::take_usize(&mut fields, "page")?,
        output: spec_text::take_path(&mut fields, "output", base)?,
        viewport: viewport_spec::take_optional(&mut fields)?,
        overlays: spec_text::take_optional_path(&mut fields, "overlays", base),
    };
    spec_text::reject_unknown(&fields)?;
    Ok(value)
}

pub(crate) fn parse_map16_page_document_render_spec(
    text: &str,
    path: &Path,
) -> Result<Map16PageDocumentRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMPGDR1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = Map16PageDocumentRenderSpec {
        graphics: spec_text::take_path(&mut fields, "graphics", base)?,
        palette: spec_text::take_path(&mut fields, "palette", base)?,
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
    fn paths_are_relative_and_document_page_is_typed() {
        let standalone = parse_map16_render_spec("LMM16R1\ngraphics assets/GFX 日本語.lmgfx\npalette colors.lmpal\npage Page 01.map16\noutput Page 01.png\n", Path::new("specs/render.txt")).unwrap();
        assert_eq!(
            standalone.graphics,
            Path::new("specs/assets/GFX 日本語.lmgfx")
        );
        let document = parse_map16_document_render_spec(
            "LMM16DR1\ngraphics gfx\npalette pal\npage 3\noutput out.png\n",
            Path::new("specs/render.txt"),
        )
        .unwrap();
        assert_eq!(document.page, 3);
        assert_eq!(document.output, Path::new("specs/out.png"));
        let page_document = parse_map16_page_document_render_spec(
            "LMPGDR1\ngraphics assets/gfx\npalette pal\noutput Page 日本語.png\n",
            Path::new("specs/render.txt"),
        )
        .unwrap();
        assert_eq!(page_document.graphics, Path::new("specs/assets/gfx"));
        assert_eq!(page_document.output, Path::new("specs/Page 日本語.png"));
        assert_eq!(standalone.viewport, None);
    }

    #[test]
    fn all_map16_preview_forms_share_camera_validation() {
        let camera = "viewport-origin-x -8\nviewport-origin-y 4\nviewport-width 32\nviewport-height 24\nzoom-numerator 2\nzoom-denominator 1\n";
        let standalone = parse_map16_render_spec(
            &format!("LMM16R1\ngraphics g\npalette p\npage m\noutput o\n{camera}"),
            Path::new("x"),
        )
        .unwrap();
        let set = parse_map16_document_render_spec(
            &format!("LMM16DR1\ngraphics g\npalette p\npage 0\noutput o\n{camera}"),
            Path::new("x"),
        )
        .unwrap();
        let page = parse_map16_page_document_render_spec(
            &format!("LMPGDR1\ngraphics g\npalette p\noutput o\n{camera}"),
            Path::new("x"),
        )
        .unwrap();
        assert_eq!(standalone.viewport, set.viewport);
        assert_eq!(set.viewport, page.viewport);
    }
}
