use crate::{Observation, observe_palette};
use lm_graphics::TplPaletteFile;

/// Produces a format- and color-addressable observation of a native version-2 TPL file.
///
/// # Panics
///
/// Panics only if fixed internally generated observation paths collide, which indicates a schema
/// regression.
#[must_use]
pub fn observe_tpl_palette(file: &TplPaletteFile) -> Observation {
    let mut result = Observation::new();
    result
        .insert("tpl-palette/version", TplPaletteFile::VERSION.to_string())
        .expect("fixed observation path is unique");
    for (path, value) in observe_palette(&file.palette).entries() {
        let suffix = path.strip_prefix("palette/").unwrap_or(path);
        result
            .insert(format!("tpl-palette/{suffix}"), value)
            .expect("TPL palette observation paths are unique");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    #[test]
    fn observation_includes_version_and_exact_words() {
        let file = TplPaletteFile {
            palette: Palette {
                colors: (0_u16..256).map(Bgr555).collect(),
            },
        };
        let observed = observe_tpl_palette(&file);
        assert_eq!(observed.get("tpl-palette/version"), Some("2"));
        assert_eq!(observed.get("tpl-palette/colors/00ff/bgr555"), Some("255"));
    }
}
