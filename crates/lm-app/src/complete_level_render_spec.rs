use crate::viewport_spec::ViewportSpec;
use lm_app::LevelViewport;
use lm_level::{DscDisplayContext, DscMaterializationContext};
use lm_render::PortableLevelRenderDimensions;
use std::fmt;
use std::path::{Path, PathBuf};

const MAGIC: &str = "LMBNDR1";
pub const MAX_SPEC_BYTES: usize = 64 * 1024;
const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteLevelRenderSpec {
    pub map16: PathBuf,
    pub graphics: PathBuf,
    pub palette: PathBuf,
    pub appearances: Option<PathBuf>,
    pub layer3_plane: Option<PathBuf>,
    pub output: PathBuf,
    pub dimensions: PortableLevelRenderDimensions,
    pub viewport: Option<CompleteLevelViewportSpec>,
    pub overlays: Option<PathBuf>,
    pub dsc: Option<CompleteLevelDscSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteLevelDscSpec {
    pub path: PathBuf,
    pub context: DscMaterializationContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteLevelViewportSpec {
    pub camera: LevelViewport,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteLevelRenderSpecError(String);

impl fmt::Display for CompleteLevelRenderSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CompleteLevelRenderSpecError {}

/// Parses a bounded line-oriented render specification relative to its own directory.
///
/// Path values consume the remainder of their line, preserving spaces and Unicode exactly.
///
/// # Errors
///
/// Rejects wrong magic, excessive lines, unknown/duplicate fields, missing values, malformed
/// dimensions, and missing required fields.
pub fn parse_complete_level_render_spec(
    text: &str,
    spec_path: &Path,
) -> Result<CompleteLevelRenderSpec, CompleteLevelRenderSpecError> {
    if text.len() > MAX_SPEC_BYTES {
        return Err(error("complete-level render specification is too large"));
    }
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(error("complete-level render specification has wrong magic"));
    }
    let base = spec_path.parent().unwrap_or_else(|| Path::new(""));
    let mut values = Values::default();
    for (index, line) in lines.enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(error(format!(
                "render specification line {} is too long",
                index + 2
            )));
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| error(format!("line {} has no value", index + 2)))?;
        let value = value.trim();
        if value.is_empty() {
            return Err(error(format!("line {} has an empty value", index + 2)));
        }
        values.set(key, value, base, index + 2)?;
    }
    values.finish()
}

#[derive(Default)]
struct Values {
    map16: Option<PathBuf>,
    graphics: Option<PathBuf>,
    palette: Option<PathBuf>,
    appearances: Option<PathBuf>,
    layer3_plane: Option<PathBuf>,
    output: Option<PathBuf>,
    layer1_width: Option<usize>,
    layer1_height: Option<usize>,
    layer2_width: Option<usize>,
    layer2_height: Option<usize>,
    viewport_origin_x: Option<i64>,
    viewport_origin_y: Option<i64>,
    viewport_width: Option<u32>,
    viewport_height: Option<u32>,
    zoom_numerator: Option<u32>,
    zoom_denominator: Option<u32>,
    overlays: Option<PathBuf>,
    dsc: Option<PathBuf>,
    dsc_custom_display: Option<bool>,
    dsc_special_markers: Option<bool>,
    dsc_first_feature: Option<bool>,
    dsc_first_suppressed: Option<bool>,
    dsc_second_feature: Option<bool>,
    dsc_level_mode: Option<u8>,
}

impl Values {
    fn set(
        &mut self,
        key: &str,
        value: &str,
        base: &Path,
        line: usize,
    ) -> Result<(), CompleteLevelRenderSpecError> {
        match key {
            "map16" => set_once(&mut self.map16, resolve(base, value), key, line),
            "graphics" => set_once(&mut self.graphics, resolve(base, value), key, line),
            "palette" => set_once(&mut self.palette, resolve(base, value), key, line),
            "appearances" => set_once(&mut self.appearances, resolve(base, value), key, line),
            "layer3-plane" => set_once(&mut self.layer3_plane, resolve(base, value), key, line),
            "output" => set_once(&mut self.output, resolve(base, value), key, line),
            "layer1-width" => set_number(&mut self.layer1_width, value, key, line),
            "layer1-height" => set_number(&mut self.layer1_height, value, key, line),
            "layer2-width" => set_number(&mut self.layer2_width, value, key, line),
            "layer2-height" => set_number(&mut self.layer2_height, value, key, line),
            "viewport-origin-x" => set_number(&mut self.viewport_origin_x, value, key, line),
            "viewport-origin-y" => set_number(&mut self.viewport_origin_y, value, key, line),
            "viewport-width" => set_number(&mut self.viewport_width, value, key, line),
            "viewport-height" => set_number(&mut self.viewport_height, value, key, line),
            "zoom-numerator" => set_number(&mut self.zoom_numerator, value, key, line),
            "zoom-denominator" => set_number(&mut self.zoom_denominator, value, key, line),
            "overlays" => set_once(&mut self.overlays, resolve(base, value), key, line),
            "dsc" => set_once(&mut self.dsc, resolve(base, value), key, line),
            "dsc-custom-display" => set_switch(&mut self.dsc_custom_display, value, key, line),
            "dsc-special-markers" => set_switch(&mut self.dsc_special_markers, value, key, line),
            "dsc-first-feature" => set_switch(&mut self.dsc_first_feature, value, key, line),
            "dsc-first-suppressed" => set_switch(&mut self.dsc_first_suppressed, value, key, line),
            "dsc-second-feature" => set_switch(&mut self.dsc_second_feature, value, key, line),
            "dsc-level-mode" => set_number(&mut self.dsc_level_mode, value, key, line),
            _ => Err(error(format!("line {line} has unknown field {key:?}"))),
        }
    }

    fn finish(self) -> Result<CompleteLevelRenderSpec, CompleteLevelRenderSpecError> {
        let viewport = viewport(
            self.viewport_origin_x,
            self.viewport_origin_y,
            self.viewport_width,
            self.viewport_height,
            self.zoom_numerator,
            self.zoom_denominator,
        )?;
        let dsc = dsc_spec(DscFields {
            path: self.dsc,
            custom_display: self.dsc_custom_display,
            special_markers: self.dsc_special_markers,
            first_feature: self.dsc_first_feature,
            first_suppressed: self.dsc_first_suppressed,
            second_feature: self.dsc_second_feature,
            level_mode: self.dsc_level_mode,
        })?;
        Ok(CompleteLevelRenderSpec {
            map16: required(self.map16, "map16")?,
            graphics: required(self.graphics, "graphics")?,
            palette: required(self.palette, "palette")?,
            appearances: self.appearances,
            layer3_plane: self.layer3_plane,
            output: required(self.output, "output")?,
            dimensions: PortableLevelRenderDimensions {
                layer1_width: required(self.layer1_width, "layer1-width")?,
                layer1_height: required(self.layer1_height, "layer1-height")?,
                layer2_width: required(self.layer2_width, "layer2-width")?,
                layer2_height: required(self.layer2_height, "layer2-height")?,
            },
            viewport,
            overlays: self.overlays,
            dsc,
        })
    }
}

fn set_switch(
    target: &mut Option<bool>,
    value: &str,
    key: &str,
    line: usize,
) -> Result<(), CompleteLevelRenderSpecError> {
    let value = match value {
        "0" => false,
        "1" => true,
        _ => return Err(error(format!("line {line} has invalid switch {value:?}"))),
    };
    set_once(target, value, key, line)
}

struct DscFields {
    path: Option<PathBuf>,
    custom_display: Option<bool>,
    special_markers: Option<bool>,
    first_feature: Option<bool>,
    first_suppressed: Option<bool>,
    second_feature: Option<bool>,
    level_mode: Option<u8>,
}

fn dsc_spec(
    fields: DscFields,
) -> Result<Option<CompleteLevelDscSpec>, CompleteLevelRenderSpecError> {
    let supplied = [
        fields.path.is_some(),
        fields.custom_display.is_some(),
        fields.special_markers.is_some(),
        fields.first_feature.is_some(),
        fields.first_suppressed.is_some(),
        fields.second_feature.is_some(),
        fields.level_mode.is_some(),
    ];
    if supplied.iter().all(|value| !value) {
        return Ok(None);
    }
    if !supplied.iter().all(|value| *value) {
        return Err(error(
            "DSC render fields must be supplied as a complete group",
        ));
    }
    Ok(Some(CompleteLevelDscSpec {
        path: fields.path.expect("group checked"),
        context: DscMaterializationContext {
            custom_display_enabled: fields.custom_display.expect("group checked"),
            special_markers_enabled: fields.special_markers.expect("group checked"),
            display: DscDisplayContext {
                first_feature_enabled: fields.first_feature.expect("group checked"),
                first_feature_suppressed: fields.first_suppressed.expect("group checked"),
                second_feature_enabled: fields.second_feature.expect("group checked"),
            },
            level_mode: fields.level_mode.expect("group checked"),
        },
    }))
}

fn resolve(base: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn set_once<T>(
    target: &mut Option<T>,
    value: T,
    key: &str,
    line: usize,
) -> Result<(), CompleteLevelRenderSpecError> {
    if target.replace(value).is_some() {
        return Err(error(format!("line {line} duplicates field {key:?}")));
    }
    Ok(())
}

fn set_number<T: std::str::FromStr>(
    target: &mut Option<T>,
    value: &str,
    key: &str,
    line: usize,
) -> Result<(), CompleteLevelRenderSpecError> {
    let value = value
        .parse()
        .map_err(|_| error(format!("line {line} has invalid number {value:?}")))?;
    set_once(target, value, key, line)
}

fn viewport(
    origin_x: Option<i64>,
    origin_y: Option<i64>,
    width: Option<u32>,
    height: Option<u32>,
    zoom_numerator: Option<u32>,
    zoom_denominator: Option<u32>,
) -> Result<Option<CompleteLevelViewportSpec>, CompleteLevelRenderSpecError> {
    ViewportSpec::from_optional_parts(
        origin_x,
        origin_y,
        width,
        height,
        zoom_numerator,
        zoom_denominator,
    )
    .map(|value| {
        value.map(|value| CompleteLevelViewportSpec {
            camera: value.camera,
            width: value.width,
            height: value.height,
        })
    })
    .map_err(|error_value| error(error_value.to_string()))
}

fn required<T>(value: Option<T>, key: &'static str) -> Result<T, CompleteLevelRenderSpecError> {
    value.ok_or_else(|| error(format!("render specification is missing {key}")))
}

fn error(message: impl Into<String>) -> CompleteLevelRenderSpecError {
    CompleteLevelRenderSpecError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_preserve_spaces_unicode_and_are_spec_relative() {
        let spec = parse_complete_level_render_spec(
            "LMBNDR1\nmap16 assets/My Map16 日本語.lm16set\ngraphics assets/all.gfx\npalette assets/colors.pal\noutput renders/Level 105.png\nlayer1-width 16\nlayer1-height 27\nlayer2-width 16\nlayer2-height 27\n",
            Path::new("project/specs/render.txt"),
        )
        .unwrap();
        assert_eq!(
            spec.map16,
            Path::new("project/specs/assets/My Map16 日本語.lm16set")
        );
        assert_eq!(
            spec.output,
            Path::new("project/specs/renders/Level 105.png")
        );
        assert_eq!(spec.viewport, None);
        assert_eq!(spec.dsc, None);
    }

    #[test]
    fn parses_complete_signed_viewport_state() {
        let spec = parse_complete_level_render_spec(
            "LMBNDR1\nmap16 m\ngraphics g\npalette p\noutput o\nlayer1-width 1\nlayer1-height 1\nlayer2-width 1\nlayer2-height 1\nviewport-origin-x -8\nviewport-origin-y 12\nviewport-width 640\nviewport-height 448\nzoom-numerator 3\nzoom-denominator 2\n",
            Path::new("render.txt"),
        )
        .unwrap();
        let viewport = spec.viewport.unwrap();
        assert_eq!(viewport.camera.origin, lm_render::Point { x: -8, y: 12 });
        assert_eq!(viewport.camera.zoom(), (3, 2));
        assert_eq!((viewport.width, viewport.height), (640, 448));
    }

    #[test]
    fn malformed_duplicate_unknown_and_missing_fields_fail() {
        assert!(parse_complete_level_render_spec("wrong\n", Path::new("x")).is_err());
        assert!(
            parse_complete_level_render_spec("LMBNDR1\nmap16 a\nmap16 b\n", Path::new("x"))
                .is_err()
        );
        assert!(parse_complete_level_render_spec("LMBNDR1\nunknown x\n", Path::new("x")).is_err());
        assert!(
            parse_complete_level_render_spec("LMBNDR1\nlayer1-width nope\n", Path::new("x"))
                .is_err()
        );
        assert!(parse_complete_level_render_spec("LMBNDR1\n", Path::new("x")).is_err());
        let base = "LMBNDR1\nmap16 m\ngraphics g\npalette p\noutput o\nlayer1-width 1\nlayer1-height 1\nlayer2-width 1\nlayer2-height 1\n";
        assert!(
            parse_complete_level_render_spec(
                &format!("{base}viewport-origin-x 0\n"),
                Path::new("x")
            )
            .is_err()
        );
        assert!(parse_complete_level_render_spec(
            &format!("{base}viewport-origin-x 0\nviewport-origin-y 0\nviewport-width 0\nviewport-height 1\nzoom-numerator 1\nzoom-denominator 1\n"),
            Path::new("x")
        )
        .is_err());
        assert!(parse_complete_level_render_spec(
            &format!("{base}viewport-origin-x 0\nviewport-origin-y 0\nviewport-width 1\nviewport-height 1\nzoom-numerator 51\nzoom-denominator 1\n"),
            Path::new("x")
        )
        .is_err());
    }

    #[test]
    fn dsc_context_is_all_or_nothing_and_switches_are_strict() {
        let base = "LMBNDR1\nmap16 m\ngraphics g\npalette p\noutput o\nlayer1-width 1\nlayer1-height 1\nlayer2-width 0\nlayer2-height 0\n";
        assert!(
            parse_complete_level_render_spec(
                &format!("{base}dsc custom.dsc\n"),
                Path::new("spec/render.txt")
            )
            .is_err()
        );
        assert!(
            parse_complete_level_render_spec(
                &format!("{base}dsc custom.dsc\ndsc-custom-display yes\n"),
                Path::new("spec/render.txt")
            )
            .is_err()
        );
        let spec = parse_complete_level_render_spec(
            &format!("{base}dsc custom.dsc\ndsc-custom-display 1\ndsc-special-markers 0\ndsc-first-feature 1\ndsc-first-suppressed 0\ndsc-second-feature 1\ndsc-level-mode 13\n"),
            Path::new("spec/render.txt"),
        )
        .unwrap();
        let dsc = spec.dsc.unwrap();
        assert_eq!(dsc.path, Path::new("spec/custom.dsc"));
        assert!(dsc.context.custom_display_enabled);
        assert_eq!(dsc.context.level_mode, 13);
    }
}
