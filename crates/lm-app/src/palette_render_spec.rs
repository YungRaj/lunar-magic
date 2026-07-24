use crate::{
    spec_text::{self, SpecError},
    viewport_spec::{self, ViewportSpec},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaletteDocumentRenderSpec {
    pub columns: usize,
    pub cell_size: usize,
    pub output: PathBuf,
    pub viewport: Option<ViewportSpec>,
    pub overlays: Option<PathBuf>,
}

pub(crate) fn parse_palette_document_render_spec(
    text: &str,
    path: &Path,
) -> Result<PaletteDocumentRenderSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, "LMPALDR1")?;
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let value = PaletteDocumentRenderSpec {
        columns: spec_text::take_usize(&mut fields, "columns")?,
        cell_size: spec_text::take_usize(&mut fields, "cell-size")?,
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
    fn grid_is_typed() {
        let value = parse_palette_document_render_spec(
            "LMPALDR1\ncolumns 16\ncell-size 12\noutput Palette Grid.png\n",
            Path::new("specs/render.txt"),
        )
        .unwrap();
        assert_eq!((value.columns, value.cell_size), (16, 12));
        assert_eq!(value.output, Path::new("specs/Palette Grid.png"));
        assert_eq!(value.viewport, None);
    }
}
