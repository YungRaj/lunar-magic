use crate::Observation;
use lm_graphics::MaterializedAnimationFrame;

/// Produces a target-addressable snapshot of one provider-resolved animation tick.
#[must_use]
pub fn observe_materialized_animation_frame(frame: &MaterializedAnimationFrame) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "animation-frame/tick", &frame.tick);
    put(
        &mut result,
        "animation-frame/tile-overrides/count",
        &frame.tile_overrides.len(),
    );
    let mut tiles: Vec<_> = frame.tile_overrides.iter().collect();
    tiles.sort_unstable_by_key(|entry| entry.tile_index);
    for entry in tiles {
        put(
            &mut result,
            &format!(
                "animation-frame/tile-overrides/{:08x}/pixels",
                entry.tile_index
            ),
            &hex(entry.tile.pixels()),
        );
    }
    put(
        &mut result,
        "animation-frame/palette-overrides/count",
        &frame.palette_overrides.len(),
    );
    let mut colors: Vec<_> = frame.palette_overrides.iter().collect();
    colors.sort_unstable_by_key(|entry| entry.color_index);
    for entry in colors {
        put(
            &mut result,
            &format!(
                "animation-frame/palette-overrides/{:08x}/bgr555",
                entry.color_index
            ),
            &entry.color.0,
        );
    }
    result
}

fn put(result: &mut Observation, path: &str, value: &(impl ToString + ?Sized)) {
    result
        .insert(path, value.to_string())
        .expect("observation paths are unique");
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, IndexedTile, MaterializedPaletteOverride, MaterializedTileOverride};

    #[test]
    fn observes_tick_and_overrides_by_absolute_target() {
        let frame = MaterializedAnimationFrame {
            tick: 17,
            tile_overrides: vec![MaterializedTileOverride {
                tile_index: 0x123,
                tile: IndexedTile::new([5; IndexedTile::PIXEL_COUNT]),
            }],
            palette_overrides: vec![MaterializedPaletteOverride {
                color_index: 0x42,
                color: Bgr555(0x1234),
            }],
        };
        let observed = observe_materialized_animation_frame(&frame);
        assert_eq!(observed.get("animation-frame/tick"), Some("17"));
        assert_eq!(
            observed.get("animation-frame/palette-overrides/00000042/bgr555"),
            Some("4660")
        );
        assert_eq!(
            observed
                .get("animation-frame/tile-overrides/00000123/pixels")
                .unwrap()
                .len(),
            IndexedTile::PIXEL_COUNT * 2
        );
    }
}
