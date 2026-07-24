use crate::{Observation, sha256_hex};
use lm_profile::RevisionPatchTemplate;
use lm_rom::{Mapper, Region, SupportedGame};

#[must_use]
pub fn observe_revision_patch(template: &RevisionPatchTemplate) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "revision-patch/name", &template.name);
    put(&mut result, "revision-patch/game", game(template.game));
    put(
        &mut result,
        "revision-patch/region",
        region(template.region),
    );
    put(&mut result, "revision-patch/revision", &template.revision);
    put(
        &mut result,
        "revision-patch/mapper",
        mapper(template.mapper),
    );
    put(
        &mut result,
        "revision-patch/payload-count",
        &template.payloads.len(),
    );
    put(
        &mut result,
        "revision-patch/write-count",
        &template.writes.len(),
    );
    for (index, payload) in template.payloads.iter().enumerate() {
        let base = format!("revision-patch/payloads/{index:02x}");
        put(&mut result, &format!("{base}/length"), &payload.bytes.len());
        put(
            &mut result,
            &format!("{base}/sha256"),
            &sha256_hex(&payload.bytes),
        );
        observe_fixups(&mut result, &base, &payload.fixups);
    }
    for (index, write) in template.writes.iter().enumerate() {
        let base = format!("revision-patch/writes/{index:02x}");
        put(&mut result, &format!("{base}/offset"), &write.offset);
        put(
            &mut result,
            &format!("{base}/length"),
            &write.replacement.len(),
        );
        put(
            &mut result,
            &format!("{base}/expected-sha256"),
            &sha256_hex(&write.expected),
        );
        put(
            &mut result,
            &format!("{base}/replacement-sha256"),
            &sha256_hex(&write.replacement),
        );
        observe_fixups(&mut result, &base, &write.fixups);
    }
    result
}

fn observe_fixups(result: &mut Observation, base: &str, fixups: &[lm_project::PatchFixup]) {
    put(result, &format!("{base}/fixup-count"), &fixups.len());
    for (index, fixup) in fixups.iter().enumerate() {
        let path = format!("{base}/fixups/{index:04x}");
        put(result, &format!("{path}/offset"), &fixup.offset);
        put(
            result,
            &format!("{path}/target-payload"),
            &fixup.target_payload,
        );
        put(
            result,
            &format!("{path}/target-addend"),
            &fixup.target_addend,
        );
    }
}

fn put(result: &mut Observation, path: &str, value: &(impl ToString + ?Sized)) {
    result
        .insert(path, value.to_string())
        .expect("revision patch observation paths are unique");
}

const fn game(value: SupportedGame) -> &'static str {
    match value {
        SupportedGame::SuperMarioWorld => "super-mario-world",
        SupportedGame::AllStarsAndWorld => "all-stars-and-world",
    }
}

const fn region(value: Region) -> &'static str {
    match value {
        Region::Japan => "japan",
        Region::NorthAmerica => "north-america",
    }
}

const fn mapper(value: Mapper) -> &'static str {
    match value {
        Mapper::LoRom => "lorom",
        Mapper::ExLoRom => "exlorom",
        Mapper::Sa1 => "sa1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{PatchFixup, PatchPayload, PatchWrite};

    #[test]
    fn observation_is_address_independent_but_body_exact() {
        let template = RevisionPatchTemplate {
            name: "runtime".into(),
            game: SupportedGame::SuperMarioWorld,
            region: Region::NorthAmerica,
            revision: 0,
            mapper: Mapper::LoRom,
            payloads: vec![PatchPayload {
                bytes: vec![1, 2, 3, 4],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 0x123,
                expected: vec![0xea; 4],
                replacement: vec![0x22, 0, 0, 0],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 2,
                    encoding: lm_project::PatchFixupEncoding::Long24,
                }],
            }],
        };
        let observed = observe_revision_patch(&template);
        assert_eq!(observed.get("revision-patch/payload-count"), Some("1"));
        assert_eq!(observed.get("revision-patch/writes/00/offset"), Some("291"));
        assert_eq!(
            observed.get("revision-patch/writes/00/fixups/0000/target-addend"),
            Some("2")
        );
        assert_eq!(
            observed.get("revision-patch/payloads/00/sha256"),
            Some(sha256_hex(&[1, 2, 3, 4]).as_str())
        );
    }
}
