use crate::{Observation, observe_palette};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile, SmwPaletteFileError};
use std::fmt::Write;

/// Produces a backend- and color-addressable observation of a native `.smwpal` file.
///
/// # Errors
///
/// Returns a color-data error if the file's main region cannot be decoded as SNES words.
pub fn observe_smw_palette(file: &SmwPaletteFile) -> Result<Observation, SmwPaletteFileError> {
    let mut result = Observation::new();
    put(
        &mut result,
        "smw-palette/backend",
        match file.backend() {
            SmwPaletteBackend::Legacy => "legacy",
            SmwPaletteBackend::Expanded => "expanded",
        },
    );
    put(
        &mut result,
        "smw-palette/palette-byte-length",
        file.palette_bytes().len(),
    );
    put(
        &mut result,
        "smw-palette/auxiliary",
        hex(file.auxiliary_bytes()),
    );
    for (path, value) in observe_palette(&file.palette()?).entries() {
        let suffix = path.strip_prefix("palette/").unwrap_or(path);
        put(&mut result, &format!("smw-palette/{suffix}"), value);
    }
    Ok(result)
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

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").expect("String writes cannot fail");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanded_observation_retains_backend_colors_and_auxiliary_region() {
        let mut bytes = vec![0; SmwPaletteFile::EXPANDED_FILE_LEN];
        bytes[0..2].copy_from_slice(&0x1234_u16.to_le_bytes());
        bytes[0x800..].fill(0xab);
        let observed = observe_smw_palette(&SmwPaletteFile::decode(&bytes).unwrap()).unwrap();
        assert_eq!(observed.get("smw-palette/backend"), Some("expanded"));
        assert_eq!(observed.get("smw-palette/colors/0000/bgr555"), Some("4660"));
        assert_eq!(
            observed.get("smw-palette/auxiliary"),
            Some("abababababababababababababababab")
        );
    }
}
