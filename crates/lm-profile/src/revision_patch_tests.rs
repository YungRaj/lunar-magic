use super::*;
use lm_project::{PatchFixup, PatchFixupEncoding};
use lm_rom::RomImage;

fn template() -> RevisionPatchTemplate {
    RevisionPatchTemplate {
        name: "SMW US Layer 3 clean-room runtime".into(),
        game: SupportedGame::SuperMarioWorld,
        region: Region::NorthAmerica,
        revision: 0,
        mapper: Mapper::LoRom,
        payloads: vec![
            PatchPayload {
                bytes: vec![0xaa; 8],
                fixups: vec![PatchFixup {
                    offset: 2,
                    target_payload: 1,
                    target_addend: 1,
                    encoding: PatchFixupEncoding::Long24,
                }],
            },
            PatchPayload {
                bytes: vec![0xbb; 5],
                fixups: Vec::new(),
            },
        ],
        writes: vec![PatchWrite {
            offset: 0x1234,
            expected: vec![0xea; 4],
            replacement: vec![0x22, 0, 0, 0],
            fixups: vec![PatchFixup {
                offset: 1,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }],
        }],
    }
}

#[test]
fn canonical_template_round_trips_exactly() {
    let expected = template();
    let encoded = expected.encode().unwrap();
    assert_eq!(RevisionPatchTemplate::decode(&encoded).unwrap(), expected);
    assert_eq!(
        RevisionPatchTemplate::decode(&encoded)
            .unwrap()
            .encode()
            .unwrap(),
        encoded
    );
}

#[test]
fn template_is_bound_to_all_stable_profile_identity_fields() {
    let profile = crate::test_support::profile();
    let mut expected = template();
    expected.game = profile.game;
    expected.region = profile.region;
    expected.revision = profile.revision;
    expected.mapper = profile.mapper;
    expected.ensure_profile(&profile).unwrap();
    for mutate in 0..4 {
        let mut invalid = expected.clone();
        match mutate {
            0 => {
                invalid.game = if profile.game == SupportedGame::SuperMarioWorld {
                    SupportedGame::AllStarsAndWorld
                } else {
                    SupportedGame::SuperMarioWorld
                };
            }
            1 => {
                invalid.region = if profile.region == Region::Japan {
                    Region::NorthAmerica
                } else {
                    Region::Japan
                };
            }
            2 => invalid.revision = profile.revision.wrapping_add(1),
            _ => {
                invalid.mapper = if profile.mapper == Mapper::ExLoRom {
                    Mapper::LoRom
                } else {
                    Mapper::ExLoRom
                };
            }
        }
        assert_eq!(
            invalid.ensure_profile(&profile),
            Err(RevisionPatchTemplateError::ProfileMismatch)
        );
    }
}

#[test]
fn framing_counts_shapes_and_bounds_are_strict() {
    let encoded = template().encode().unwrap();
    assert!(RevisionPatchTemplate::decode(&encoded[..encoded.len() - 1]).is_err());
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(matches!(
        RevisionPatchTemplate::decode(&trailing),
        Err(RevisionPatchTemplateError::TrailingBytes(1))
    ));
    let mut magic = encoded;
    magic[0] = 0;
    assert_eq!(
        RevisionPatchTemplate::decode(&magic),
        Err(RevisionPatchTemplateError::WrongMagic)
    );
    let mut empty = template();
    empty.payloads[0].bytes.clear();
    assert_eq!(
        empty.encode(),
        Err(RevisionPatchTemplateError::EmptyPayload(0))
    );
    let mut unequal = template();
    unequal.writes[0].expected.pop();
    assert_eq!(
        unequal.encode(),
        Err(RevisionPatchTemplateError::WriteLengthMismatch(0))
    );
    let mut bad_fixup = template();
    bad_fixup.payloads[0].fixups[0].target_payload = 9;
    assert!(matches!(
        bad_fixup.encode(),
        Err(RevisionPatchTemplateError::InvalidFixup { owner: 0, index: 0 })
    ));
    let mut unsupported_split_fixup = template();
    unsupported_split_fixup.payloads[0].fixups[0].encoding = PatchFixupEncoding::Bank8;
    assert!(matches!(
        unsupported_split_fixup.encode(),
        Err(RevisionPatchTemplateError::InvalidFixup { owner: 0, index: 0 })
    ));
    let oversized = vec![0; RevisionPatchTemplate::MAX_FILE_LEN + 1];
    assert!(matches!(
        RevisionPatchTemplate::decode(&oversized),
        Err(RevisionPatchTemplateError::TooLarge { .. })
    ));
}

#[test]
fn installation_plan_uses_profile_wide_protection_and_no_incidental_addresses() {
    let profile = crate::test_support::profile();
    let mut expected = template();
    expected.game = profile.game;
    expected.region = profile.region;
    expected.revision = profile.revision;
    expected.mapper = profile.mapper;
    let rom = RomImage::from_bytes(vec![0xff; 0x3_0000]).unwrap();

    let plan = expected
        .installation_plan(&profile, &rom, 0x2_0000..0x3_0000, 0x7fc0, 0x7fdc, 0xff)
        .unwrap();

    assert_eq!(plan.mapper, profile.mapper);
    assert_eq!(plan.payloads, expected.payloads);
    assert_eq!(plan.writes, expected.writes);
    assert_eq!(plan.allocation.search, 0x2_0000..0x3_0000);
    assert!(
        plan.allocation
            .protected
            .iter()
            .any(|range| range.0 == (0x7fc0..0x8000))
    );
}

#[test]
fn installation_plan_can_validate_mapper_aligned_growth_without_mutating_source() {
    let profile = crate::test_support::profile();
    let mut expected = template();
    expected.game = profile.game;
    expected.region = profile.region;
    expected.revision = profile.revision;
    expected.mapper = profile.mapper;
    let rom = RomImage::from_bytes(vec![0xff; 0x3_0000]).unwrap();
    let plan = expected
        .installation_plan(&profile, &rom, 0x3_0000..0x4_0000, 0x7fc0, 0x7fdc, 0xff)
        .unwrap();
    assert_eq!(rom.logical_len(), 0x3_0000);
    assert_eq!(plan.allocation.search, 0x3_0000..0x4_0000);
}
