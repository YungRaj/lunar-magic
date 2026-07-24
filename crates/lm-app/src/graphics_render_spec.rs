use crate::{
    spec_text::{self, SpecError},
    viewport_spec::{self, ViewportSpec},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GraphicsDocumentRenderSpec {
    pub palette: PathBuf,
    pub palette_row: usize,
    pub columns: usize,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

pub(crate) fn parse_graphics_document_render_spec(
    text: &str,
    path: &Path,
) -> Result<GraphicsDocumentRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMGFXDR1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = GraphicsDocumentRenderSpec {
        palette: spec_text::take_path(&mut fields, "palette", base)?,
        palette_row: spec_text::take_usize(&mut fields, "palette-row")?,
        columns: spec_text::take_usize(&mut fields, "columns")?,
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
    fn layout_is_typed() {
        let value = parse_graphics_document_render_spec("LMGFXDR1\npalette Palette 日本語.lmpal\npalette-row 3\ncolumns 16\noutput GFX Sheet.png\n", Path::new("specs/render.txt")).unwrap();
        assert_eq!((value.palette_row, value.columns), (3, 16));
        assert_eq!(value.output, Path::new("specs/GFX Sheet.png"));
        assert_eq!(value.viewport, None);
    }
}
