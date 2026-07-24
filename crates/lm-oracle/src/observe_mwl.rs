use crate::{Observation, observe_expanded_settings, sha256_hex};
use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind};

const SECTIONS: [(MwlSectionKind, &str); MwlFile::SECTION_COUNT] = [
    (MwlSectionKind::LevelHeader, "level-header"),
    (MwlSectionKind::Layer1, "layer1"),
    (MwlSectionKind::Layer2, "layer2"),
    (MwlSectionKind::Sprites, "sprites"),
    (MwlSectionKind::Palette, "palette"),
    (MwlSectionKind::SecondaryExits, "secondary-exits"),
    (MwlSectionKind::ExAnimation, "exanimation"),
    (MwlSectionKind::ExpandedHeader, "expanded-header"),
];

/// Produces a canonical snapshot of proven MWL container fields and opaque section identities.
///
/// Section payloads are represented by byte lengths and SHA-256 identities. The fixed level
/// header and common two-word payload prefixes are exposed only when their recovered shapes decode
/// successfully; malformed semantic shapes remain observable rather than being guessed.
#[must_use]
pub fn observe_mwl(file: &MwlFile) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "mwl/version", file.version);
    put(&mut result, "mwl/flags", file.flags);
    put(&mut result, "mwl/attribution", hex(&file.attribution));
    for (kind, name) in SECTIONS {
        let bytes = file.section(kind);
        let base = format!("mwl/sections/{name}");
        put(&mut result, &format!("{base}/length"), bytes.len());
        put(&mut result, &format!("{base}/sha256"), sha256_hex(bytes));
        if kind == MwlSectionKind::LevelHeader {
            match MwlLevelHeaderSection::decode(bytes) {
                Ok(header) => {
                    put(&mut result, &format!("{base}/shape"), "fixed-40");
                    put(
                        &mut result,
                        &format!("{base}/level-number"),
                        header.level_number(),
                    );
                }
                Err(_) => put(&mut result, &format!("{base}/shape"), "opaque"),
            }
        } else if kind == MwlSectionKind::ExpandedHeader {
            match file.expanded_settings_section() {
                Ok(settings) => {
                    put(&mut result, &format!("{base}/shape"), "expanded-settings");
                    merge(
                        &mut result,
                        &format!("{base}/decoded"),
                        &observe_expanded_settings(&settings),
                    );
                }
                Err(_) => put(&mut result, &format!("{base}/shape"), "opaque"),
            }
        } else if !bytes.is_empty() {
            match file.payload_section(kind) {
                Ok(payload) => {
                    put(&mut result, &format!("{base}/shape"), "payload-prefix");
                    put(
                        &mut result,
                        &format!("{base}/metadata-0"),
                        payload.metadata[0],
                    );
                    put(
                        &mut result,
                        &format!("{base}/metadata-1"),
                        payload.metadata[1],
                    );
                    put(
                        &mut result,
                        &format!("{base}/source-snes-address"),
                        format!("{:#08x}", payload.metadata[1] & 0x00ff_ffff),
                    );
                    put(
                        &mut result,
                        &format!("{base}/payload-length"),
                        payload.payload.len(),
                    );
                    put(
                        &mut result,
                        &format!("{base}/payload-sha256"),
                        sha256_hex(&payload.payload),
                    );
                }
                Err(_) => put(&mut result, &format!("{base}/shape"), "opaque"),
            }
        } else {
            put(&mut result, &format!("{base}/shape"), "empty");
        }
    }
    result
}

fn merge(result: &mut Observation, prefix: &str, source: &Observation) {
    for (path, value) in source.entries() {
        result
            .insert(format!("{prefix}/{path}"), value)
            .expect("MWL expanded-settings paths are unique");
    }
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_observation_value())
        .expect("MWL observation paths are unique");
}

trait ObservationValue {
    fn into_observation_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_observation_value(self) -> String {
        self.to_string()
    }
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
    use lm_level::MwlPayloadSection;

    #[test]
    fn observations_cover_known_fields_and_opaque_byte_identity() {
        let mut file = MwlFile {
            version: 0x363,
            flags: 0x1234,
            ..MwlFile::default()
        };
        let mut header = MwlLevelHeaderSection([0xaa; 0x40]);
        header.set_level_number(0x105);
        file.set_section(MwlSectionKind::LevelHeader, header.0.to_vec());
        file.set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: [7, 9],
                payload: vec![1, 2, 3],
            },
        )
        .unwrap();
        file.set_section(MwlSectionKind::ExpandedHeader, vec![0xff]);
        let observation = observe_mwl(&file);
        assert_eq!(observation.get("mwl/version"), Some("867"));
        assert_eq!(
            observation.get("mwl/sections/level-header/level-number"),
            Some("261")
        );
        assert_eq!(observation.get("mwl/sections/layer1/metadata-1"), Some("9"));
        assert_eq!(
            observation.get("mwl/sections/layer1/source-snes-address"),
            Some("0x000009")
        );
        assert_eq!(
            observation.get("mwl/sections/layer1/payload-sha256"),
            Some(sha256_hex(&[1, 2, 3]).as_str())
        );
        assert_eq!(
            observation.get("mwl/sections/expanded-header/shape"),
            Some("opaque")
        );
        assert_eq!(
            Observation::from_text(&observation.to_text()).unwrap(),
            observation
        );
    }

    #[test]
    fn one_section_byte_changes_only_its_identity() {
        let mut file = MwlFile::default();
        file.set_section(MwlSectionKind::Sprites, vec![0; 8]);
        let before = observe_mwl(&file);
        file.sections[MwlSectionKind::Sprites as usize].bytes[7] = 1;
        let differences = before.differences(&observe_mwl(&file));
        assert_eq!(differences.len(), 2);
        assert_eq!(differences[0].path, "mwl/sections/sprites/metadata-1");
        assert_eq!(differences[1].path, "mwl/sections/sprites/sha256");
    }

    #[test]
    fn relocation_changes_address_identity_but_not_payload_identity() {
        let mut file = MwlFile::default();
        file.set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: [0, 0x06_88dd],
                payload: vec![1, 2, 3],
            },
        )
        .unwrap();
        let before = observe_mwl(&file);
        file.set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: [0, 0x10_8008],
                payload: vec![1, 2, 3],
            },
        )
        .unwrap();
        let after = observe_mwl(&file);
        assert_eq!(
            before.get("mwl/sections/layer1/payload-sha256"),
            after.get("mwl/sections/layer1/payload-sha256")
        );
        let paths: Vec<_> = before
            .differences(&after)
            .into_iter()
            .map(|difference| difference.path)
            .collect();
        assert_eq!(
            paths,
            [
                "mwl/sections/layer1/metadata-1",
                "mwl/sections/layer1/sha256",
                "mwl/sections/layer1/source-snes-address",
            ]
        );
    }
}
