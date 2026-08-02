use crate::Observation;
use lm_level::ExpandedLevelSettingsRecord;
use std::fmt::Write;

/// Produces a lossless word- and byte-addressable observation of one installed settings record.
///
/// # Panics
///
/// Panics only if the fixed, internally unique observation paths collide, which indicates a bug.
#[must_use]
pub fn observe_expanded_settings(record: &ExpandedLevelSettingsRecord) -> Observation {
    let mut observation = Observation::new();
    observation
        .insert(
            "expanded-settings/byte-length",
            record.encoded().len().to_string(),
        )
        .expect("expanded-settings observation paths are unique");
    observation
        .insert("expanded-settings/raw", hex(record.encoded()))
        .expect("expanded-settings observation paths are unique");
    for index in 0..ExpandedLevelSettingsRecord::WORD_COUNT {
        observation
            .insert(
                format!("expanded-settings/words/{index:02x}"),
                record.word(index).expect("bounded word index").to_string(),
            )
            .expect("expanded-settings observation paths are unique");
    }
    let layer3 = record
        .layer3_tilemap_graphics_descriptor()
        .expect("fixed Layer 3 descriptor word is in range");
    let mode = record.layer3_expanded_mode_flags();
    for (path, value) in [
        (
            "expanded-settings/layer3/tilemap-enabled",
            record.layer3_tilemap_enabled().to_string(),
        ),
        ("expanded-settings/layer3/file", layer3.file().to_string()),
        (
            "expanded-settings/layer3/length-selector",
            layer3.length_selector().to_string(),
        ),
        (
            "expanded-settings/layer3/offset-selector",
            layer3.offset_selector().to_string(),
        ),
        (
            "expanded-settings/layer3/destination-byte-offset",
            layer3.destination_byte_offset().to_string(),
        ),
        (
            "expanded-settings/layer3/effective-byte-length",
            layer3.effective_byte_length().to_string(),
        ),
        (
            "expanded-settings/layer3/mode-packed",
            mode.packed().to_string(),
        ),
        (
            "expanded-settings/layer3/mode-enabled",
            mode.enabled().to_string(),
        ),
        (
            "expanded-settings/layer3/alternate-source-route",
            mode.alternate_layer3_source_route()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned()),
        ),
        (
            "expanded-settings/layer3/primary-additive-input",
            mode.primary_layer3_additive_input()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned()),
        ),
    ] {
        observation
            .insert(path, value)
            .expect("expanded-settings observation paths are unique");
    }
    observation
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_retains_every_byte_and_little_endian_word() {
        let bytes = std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap());
        let observed =
            observe_expanded_settings(&ExpandedLevelSettingsRecord::decode(&bytes).unwrap());
        assert_eq!(observed.get("expanded-settings/byte-length"), Some("32"));
        assert_eq!(observed.get("expanded-settings/words/00"), Some("256"));
        assert_eq!(observed.get("expanded-settings/words/0f"), Some("7966"));
        assert_eq!(observed.get("expanded-settings/raw").unwrap().len(), 64);
        assert_eq!(observed.get("expanded-settings/layer3/file"), Some("770"));
        assert_eq!(
            observed.get("expanded-settings/layer3/mode-packed"),
            Some("286331153")
        );
        assert_eq!(
            observed.get("expanded-settings/layer3/mode-enabled"),
            Some("true")
        );
        assert_eq!(
            observed.get("expanded-settings/layer3/alternate-source-route"),
            Some("false")
        );
        assert_eq!(
            observed.get("expanded-settings/layer3/primary-additive-input"),
            Some("false")
        );
    }
}
