use crate::{Observation, observe_palette};
use lm_graphics::{PaletteMaskFile, RawSnesPaletteFile};

/// Produces a color-addressable snapshot of a raw 257-word Lunar Magic palette.
#[must_use]
pub fn observe_raw_palette(file: &RawSnesPaletteFile) -> Observation {
    prefixed_palette("raw-palette", &file.palette)
}

/// Produces an entry-addressable snapshot of a lossless `.palmask` selection sidecar.
#[must_use]
pub fn observe_palette_mask(mask: &PaletteMaskFile) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "palette-mask/entry-count",
        mask.entries().len(),
    );
    for (index, raw) in mask.entries().iter().enumerate() {
        let base = format!("palette-mask/entries/{index:04x}");
        put(&mut result, &format!("{base}/raw"), *raw);
        put(&mut result, &format!("{base}/selected"), *raw != 0);
    }
    result
}

fn prefixed_palette(prefix: &str, palette: &lm_graphics::Palette) -> Observation {
    let mut result = Observation::new();
    for (path, value) in observe_palette(palette).entries() {
        let suffix = path.strip_prefix("palette/").unwrap_or(path);
        put(&mut result, &format!("{prefix}/{suffix}"), value);
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("native palette observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_palette_and_mask_observations_are_addressable() {
        let raw = RawSnesPaletteFile::decode(&vec![0x01; RawSnesPaletteFile::FILE_LEN]).unwrap();
        let observed = observe_raw_palette(&raw);
        assert_eq!(observed.get("raw-palette/color-count"), Some("257"));
        assert_eq!(observed.get("raw-palette/colors/0100/bgr555"), Some("257"));

        let mut bytes = vec![0; PaletteMaskFile::FILE_LEN];
        bytes[256] = 0x80;
        let observed = observe_palette_mask(&PaletteMaskFile::decode(&bytes).unwrap());
        assert_eq!(observed.get("palette-mask/entries/0100/raw"), Some("128"));
        assert_eq!(
            observed.get("palette-mask/entries/0100/selected"),
            Some("true")
        );
    }
}
