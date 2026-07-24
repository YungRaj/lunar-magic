use crate::{Observation, sha256_hex};
use lm_graphics::{
    CompactExAnimation, ExAnimationFrameEditError, ExAnimationRecord, ExAnimationSet,
    GraphicsFile4bpp, Palette, exanimation_frames,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationObservationError {
    SizeModeIndex {
        record: usize,
        index: usize,
        actual: usize,
    },
    Frames {
        record: usize,
        source: ExAnimationFrameEditError,
    },
}

impl fmt::Display for ExAnimationObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation observation failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationObservationError {}

/// Produces a tile-addressable snapshot of decoded 4bpp graphics.
#[must_use]
pub fn observe_graphics(graphics: &GraphicsFile4bpp) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "graphics/tile-count", graphics.tiles.len());
    for (index, tile) in graphics.tiles.iter().enumerate() {
        put_hex(
            &mut result,
            &format!("graphics/tiles/{index:04x}/pixels"),
            tile.pixels(),
        );
    }
    result
}

/// Produces a color-addressable snapshot of a decoded SNES palette.
#[must_use]
pub fn observe_palette(palette: &Palette) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "palette/color-count", palette.colors.len());
    for (index, color) in palette.colors.iter().enumerate() {
        put(
            &mut result,
            &format!("palette/colors/{index:04x}/bgr555"),
            color.0,
        );
    }
    result
}

/// Produces a slot-addressable snapshot of an expanded `ExAnimation` set.
#[must_use]
pub fn observe_exanimation(animation: &ExAnimationSet) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "exanimation/record-count",
        animation.records.len(),
    );
    put(
        &mut result,
        "exanimation/visible-slots",
        animation.visible_slots,
    );
    observe_records(&mut result, &animation.records);
    result
}

/// Produces a semantic snapshot of the compact ROM `ExAnimation` representation.
#[must_use]
pub fn observe_compact_exanimation(animation: &CompactExAnimation) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "exanimation/setting", animation.setting);
    put(
        &mut result,
        "exanimation/header-value",
        animation.header_value,
    );
    put(
        &mut result,
        "exanimation/trigger-mask",
        animation.trigger_mask,
    );
    for (index, value) in animation.trigger_values.iter().enumerate() {
        if animation.trigger_mask & (1 << index) != 0 {
            put(
                &mut result,
                &format!("exanimation/triggers/{index:02x}"),
                value,
            );
        }
    }
    put(
        &mut result,
        "exanimation/record-count",
        animation.records.len(),
    );
    observe_records(&mut result, &animation.records);
    result
}

/// Produces a frame/source-word-addressable compact `ExAnimation` snapshot under an exact
/// revision size-mode interpretation.
///
/// # Errors
///
/// Returns [`ExAnimationObservationError`] when an ordinary record's size-mode index is absent or
/// its frame payload cannot be decoded.
pub fn observe_compact_exanimation_with_modes(
    animation: &CompactExAnimation,
    double_size_modes: &[bool],
) -> Result<Observation, ExAnimationObservationError> {
    let mut result = observe_compact_exanimation(animation);
    for (record_index, record) in animation.records.iter().enumerate() {
        let base = format!("exanimation/records/{record_index:04x}");
        let ordinary = record.kind() != 0 && !(0x18..=0x1b).contains(&record.kind());
        put(&mut result, &format!("{base}/ordinary-frames"), ordinary);
        if !ordinary {
            continue;
        }
        let mode_index = usize::from(record.size_mode());
        let double_size = *double_size_modes.get(mode_index).ok_or(
            ExAnimationObservationError::SizeModeIndex {
                record: record_index,
                index: mode_index,
                actual: double_size_modes.len(),
            },
        )?;
        put(&mut result, &format!("{base}/double-size"), double_size);
        let frames = exanimation_frames(record, double_size).map_err(|source| {
            ExAnimationObservationError::Frames {
                record: record_index,
                source,
            }
        })?;
        for (frame_index, frame) in frames.iter().enumerate() {
            for (word_index, word) in frame.source_words.iter().enumerate() {
                put(
                    &mut result,
                    &format!("{base}/frames/{frame_index:04x}/source/{word_index:02x}"),
                    word,
                );
            }
        }
    }
    Ok(result)
}

fn observe_records(result: &mut Observation, records: &[ExAnimationRecord]) {
    for (index, record) in records.iter().enumerate() {
        let base = format!("exanimation/records/{index:04x}");
        put(result, &format!("{base}/kind"), record.kind());
        put(
            result,
            &format!("{base}/frame-count-minus-one"),
            record.frame_count_minus_one(),
        );
        put(result, &format!("{base}/size-mode"), record.size_mode());
        put(result, &format!("{base}/destination"), record.destination());
        put(
            result,
            &format!("{base}/destination-flag"),
            record.destination_flag(),
        );
        put(
            result,
            &format!("{base}/encoded-sha256"),
            sha256_hex(record.encoded()),
        );
    }
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_observation_value())
        .expect("adapter paths are unique");
}

trait ObservationValue {
    fn into_observation_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_observation_value(self) -> String {
        self.to_string()
    }
}

fn put_hex(result: &mut Observation, path: &str, bytes: &[u8]) {
    use std::fmt::Write;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(value, "{byte:02x}").expect("String writes cannot fail");
    }
    put(result, path, value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, ExAnimationRecord, IndexedTile};

    #[test]
    fn graphics_and_palette_changes_are_addressable() {
        let mut graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT])],
        };
        let before = observe_graphics(&graphics);
        graphics.tiles[0].set_pixel(3, 4, 7).unwrap();
        assert_eq!(
            before.differences(&observe_graphics(&graphics))[0].path,
            "graphics/tiles/0000/pixels"
        );

        let mut palette = Palette {
            colors: vec![Bgr555(1), Bgr555(2)],
        };
        let before = observe_palette(&palette);
        palette.colors[1] = Bgr555(3);
        assert_eq!(
            before.differences(&observe_palette(&palette))[0].path,
            "palette/colors/0001/bgr555"
        );
    }

    #[test]
    fn exanimation_observation_tracks_semantics_and_unknown_bytes() {
        let mut set = ExAnimationSet {
            records: vec![ExAnimationRecord::inactive()],
            visible_slots: 1,
        };
        let before = observe_exanimation(&set);
        set.records[0] = ExAnimationRecord::new(2, 0, 1, 0x123, false, &[3, 4], false).unwrap();
        let differences = before.differences(&observe_exanimation(&set));
        assert!(
            differences
                .iter()
                .any(|change| change.path.ends_with("/kind"))
        );
        assert!(
            differences
                .iter()
                .any(|change| change.path.ends_with("/encoded-sha256"))
        );
    }

    #[test]
    fn interpreted_observation_exposes_single_and_double_source_words() {
        let animation = CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![
                ExAnimationRecord::new(1, 1, 2, 0x100, false, &[1, 0, 2, 0], false).unwrap(),
                ExAnimationRecord::new(1, 0, 3, 0x101, false, &[3, 0, 4, 0], true).unwrap(),
                ExAnimationRecord::inactive(),
            ],
        };
        let mut modes = [false; 256];
        modes[3] = true;
        let observation = observe_compact_exanimation_with_modes(&animation, &modes).unwrap();
        assert_eq!(
            observation.get("exanimation/records/0000/frames/0001/source/00"),
            Some("2")
        );
        assert_eq!(
            observation.get("exanimation/records/0001/frames/0000/source/01"),
            Some("4")
        );
        assert_eq!(
            observation.get("exanimation/records/0002/ordinary-frames"),
            Some("false")
        );
    }

    #[test]
    fn interpreted_observation_rejects_missing_size_modes() {
        let animation = CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 7, 0x100, false, &[1, 0], false).unwrap()],
        };
        assert!(matches!(
            observe_compact_exanimation_with_modes(&animation, &[false; 7]),
            Err(ExAnimationObservationError::SizeModeIndex {
                record: 0,
                index: 7,
                actual: 7
            })
        ));
    }
}
