use crate::spec_text::{self, SpecError};
use lm_level::{Layer3ExpandedModeFlags, Layer3TilemapGraphicsDescriptor};

pub(crate) const MAGIC: &str = "LMMWLL31";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MwlLayer3SettingsSpec {
    pub enabled: bool,
    pub descriptor: Layer3TilemapGraphicsDescriptor,
    pub expanded_mode: Option<Layer3ExpandedModeFlags>,
}

pub(crate) fn parse(text: &str) -> Result<MwlLayer3SettingsSpec, SpecError> {
    let mut fields = spec_text::parse_fields(text, MAGIC)?;
    let enabled = match spec_text::take_string(&mut fields, "enabled")?.as_str() {
        "true" => true,
        "false" => false,
        value => {
            return Err(spec_text::error(format!(
                "MWL Layer 3 enabled must be true or false, got {value:?}"
            )));
        }
    };
    let file = take_hex(&mut fields, "file")?;
    let length_selector = take_hex(&mut fields, "length-selector")?;
    let offset_selector = take_hex(&mut fields, "offset-selector")?;
    let expanded_mode = fields
        .remove("expanded-mode")
        .map(|value| {
            u32::from_str_radix(value.trim_start_matches("0x"), 16)
                .map(Layer3ExpandedModeFlags::from_packed)
                .map_err(|_| {
                    spec_text::error(format!(
                        "MWL Layer 3 expanded-mode is not hexadecimal: {value:?}"
                    ))
                })
        })
        .transpose()?;
    spec_text::reject_unknown(&fields)?;
    let descriptor = Layer3TilemapGraphicsDescriptor::new(
        file,
        u8::try_from(length_selector)
            .map_err(|_| spec_text::error("Layer 3 length selector is too large"))?,
        u8::try_from(offset_selector)
            .map_err(|_| spec_text::error("Layer 3 offset selector is too large"))?,
    )
    .map_err(|error| spec_text::error(error.to_string()))?;
    Ok(MwlLayer3SettingsSpec {
        enabled,
        descriptor,
        expanded_mode,
    })
}

fn take_hex(fields: &mut spec_text::Fields, key: &str) -> Result<u16, SpecError> {
    let value = spec_text::take_string(fields, key)?;
    u16::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|_| spec_text::error(format!("MWL Layer 3 {key} is not hexadecimal: {value:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_fields_and_rejects_bad_or_extra_values() {
        let value =
            parse("LMMWLL31\nenabled true\nfile abc\nlength-selector 2\noffset-selector 3\n")
                .unwrap();
        assert!(value.enabled);
        assert_eq!(value.descriptor.packed(), 0xeabc);
        assert_eq!(value.expanded_mode, None);
        let value = parse("LMMWLL31\nenabled true\nfile abc\nlength-selector 2\noffset-selector 3\nexpanded-mode 89abcdef\n").unwrap();
        assert_eq!(value.expanded_mode.unwrap().packed(), 0x89ab_cdef);
        assert!(parse("wrong\n").is_err());
        assert!(
            parse("LMMWLL31\nenabled yes\nfile 1\nlength-selector 0\noffset-selector 0\n").is_err()
        );
        assert!(parse("LMMWLL31\nenabled true\nfile 1\nlength-selector 0\noffset-selector 0\nexpanded-mode 100000000\n").is_err());
        assert!(
            parse("LMMWLL31\nenabled true\nfile 1000\nlength-selector 0\noffset-selector 0\n")
                .is_err()
        );
        assert!(
            parse(
                "LMMWLL31\nenabled true\nfile 1\nlength-selector 0\noffset-selector 0\nextra 1\n"
            )
            .is_err()
        );
    }
}
