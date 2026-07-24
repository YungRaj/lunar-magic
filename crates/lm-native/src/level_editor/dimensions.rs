use lm_level::CompleteLevelFile;
use lm_render::PortableLevelRenderDimensions;

pub(super) fn suggested_dimensions(level: &CompleteLevelFile) -> [String; 4] {
    let dimensions = |len: usize| {
        if len == 0 {
            (0, 0)
        } else if len % 16 == 0 {
            (16, len / 16)
        } else {
            (len.max(1), 1)
        }
    };
    let layer1 = dimensions(level.0.layer1.raw_tilemap.len());
    let layer2 = dimensions(level.0.layer2.raw_tilemap.len());
    [
        layer1.0.to_string(),
        layer1.1.to_string(),
        layer2.0.to_string(),
        layer2.1.to_string(),
    ]
}

pub(super) fn parse_dimensions(
    fields: &[String; 4],
    level: &CompleteLevelFile,
) -> Result<PortableLevelRenderDimensions, String> {
    let values = fields
        .iter()
        .map(|field| {
            field
                .trim()
                .parse::<usize>()
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (name, width, height, actual) in [
        (
            "Layer 1",
            values[0],
            values[1],
            level.0.layer1.raw_tilemap.len(),
        ),
        (
            "Layer 2",
            values[2],
            values[3],
            level.0.layer2.raw_tilemap.len(),
        ),
    ] {
        let expected = width
            .checked_mul(height)
            .ok_or("tilemap dimensions overflow")?;
        let empty = width == 0 && height == 0 && actual == 0;
        if (!empty && (width == 0 || height == 0)) || expected != actual {
            return Err(format!(
                "{name} dimensions require {expected} tiles but the document contains {actual}"
            ));
        }
        if width > usize::from(u16::MAX) || height > usize::from(u16::MAX) {
            return Err(format!(
                "{name} dimensions exceed the editor coordinate bound"
            ));
        }
    }
    Ok(PortableLevelRenderDimensions {
        layer1_width: values[0],
        layer1_height: values[1],
        layer2_width: values[2],
        layer2_height: values[3],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_must_match_both_exact_tilemap_shapes() {
        let mut level = CompleteLevelFile(lm_level::Level::default());
        level.0.layer1.raw_tilemap = vec![0; 32];
        level.0.layer2.raw_tilemap = vec![0; 8];
        assert!(
            parse_dimensions(&["16".into(), "2".into(), "4".into(), "2".into()], &level).is_ok()
        );
        assert!(
            parse_dimensions(&["16".into(), "1".into(), "4".into(), "2".into()], &level).is_err()
        );
        level.0.layer2.raw_tilemap.clear();
        assert!(
            parse_dimensions(&["16".into(), "2".into(), "0".into(), "0".into()], &level).is_ok()
        );
    }
}
