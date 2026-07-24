use crate::Observation;
use lm_graphics::{RgbChannelExpansion, RgbPaletteFile};

/// Produces an RGB- and converted-BGR555-addressable observation of a native `.pal` file.
#[must_use]
pub fn observe_rgb_palette(file: &RgbPaletteFile) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "rgb-palette/expansion",
        match file.detected_expansion {
            RgbChannelExpansion::HighBits => "high-bits",
            RgbChannelExpansion::ReplicatedBits => "replicated-bits",
        },
    );
    put(&mut result, "rgb-palette/color-count", file.colors.len());
    let converted = file.to_snes_palette();
    for (index, (rgb, bgr)) in file.colors.iter().zip(&converted.colors).enumerate() {
        let base = format!("rgb-palette/colors/{index:04x}");
        put(&mut result, &format!("{base}/red"), rgb.red);
        put(&mut result, &format!("{base}/green"), rgb.green);
        put(&mut result, &format!("{base}/blue"), rgb.blue);
        put(&mut result, &format!("{base}/bgr555"), bgr.0);
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("RGB palette observation paths are unique");
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
    fn observation_retains_rgb_and_detected_conversion() {
        let mut bytes = vec![0; RgbPaletteFile::FILE_LEN];
        bytes[..3].copy_from_slice(&[0xf8, 0x80, 0x40]);
        let observed = observe_rgb_palette(&RgbPaletteFile::decode(&bytes).unwrap());
        assert_eq!(observed.get("rgb-palette/expansion"), Some("high-bits"));
        assert_eq!(observed.get("rgb-palette/colors/0000/red"), Some("248"));
        assert_eq!(observed.get("rgb-palette/colors/0000/bgr555"), Some("8735"));
    }
}
