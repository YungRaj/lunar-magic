//! Standard-LoROM extended Layer 3/sprite runtime recovered from PE resource `$206`.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN: usize = 0x370;
pub const SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START: usize = 0x0008_5aa6;
pub const SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_END: usize = 0x0010_0000;

const RESOURCE_HEX: &str = concat!(
    "40ffd0ff80ff01c0b00120016001ff3febc220cdd713102538e51ccdf20b100bcdf00b3006e2a06ba9ff6be220bd7a16",
    "2904d0f4ad9b0dc980f0ed4a6be2206ba505c97bd00fa554290d4bf49787f4c9805c9c8301c8a6025cdaa9022901050a",
    "85096b2901050a9d93175cd5ab022901050a99b80f5c59ab02050a9d2a1eca5c65aa0222d887902901050a2204fd00a5",
    "005c6aa902b7ce4829f05c60a902a9018555adf40b29030aaac221bf438790e90f008df00bbf4b8790186910008df20b",
    "a9ffff8dee0be220af0cf30e850caf0df30e850daf0ef30e850eaf0ff30e1869be850fc230a0010064006402a5ce8df6",
    "0c3a3a8504a90000850685088d360de220840aa5088507b7cec9ffd018adf50b8920f07ac8b7cec9fff00ac9fef06f0a",
    "8506c880e2aa0a0a0a29100a8502c8b7ce290f0a0402c8a50fd0198a290c4a4aebb7ce5aa8b70ce902c22129ff006301",
    "a868e220c8e608a502c500f0aac220a600bdf60ce8e89df60ce40290f7a600bd360de8e89d360de40290f7a50a65049d",
    "f60ca5069d360de22086004c4488a600e03e00f01ec220bdf60ce8e89df60ce03e00d0f6a600bd360de8e89d360de03e",
    "00d0f6e2306b8501adf40b29030aaac221a5008550a51c29f0ff85467f43879085528548a546187f4b8790854638e910",
    "00854aa51a29f0ff38e93000854c18695001854ee220a549290114490448a54b2901144b044aa55b29010aaaa9a08545",
    "7445adf40b1035a550cdee0b8dee0bd004a9400445a548cdef0b8def0bd004a9200445a9602545c960f03fc920d00da5",
    "51300f0ac940900ca23e8009b51b3a0a1002a900aac220d4cebdf60c85cebd360de220850aebaa4bf4b689f488b8a001",
    "5c2ca8026885ce6885cf5c4ba802adf50b8920f009c8b7ce1008c9fff0215c4ba8020a850ac88011e8c8c8100c981865",
    "ce85cea0009002e6cfb7cec9fff0cf85540a0a0a29108502c8b7ce8500290f05028501c5511062a90f1400a920244530",
    "3ad038a50ac549d024a55429f1c548f00ac54ad026a50ac54bd020c220a500c54c3016c54ee22010125c56a802c54bd0",
    "0aa55429f1c54af0e2e2205c46a80224453012c54ff002b00ca9202545d006a90f140080ae5c4ba802d0e4a50029f085",
    "00c550d096244530197090a5542901050aeba55429f0c220c55230bdc546e22010b95c56a802585350524954452d4745",
    "4e31202020202020202020204c4d0101"
);

#[must_use]
pub fn smw_us_v1_layer3_extended_runtime_payload() -> PatchPayload {
    let mut bytes = decode_resource();
    // Revision-specific mapped operands patched by Lunar Magic after resource selection.
    bytes[0x4c..0x4e].copy_from_slice(&[0x02, 0xdb]);
    bytes[0x18c..0x18e].copy_from_slice(&[0xaf, 0xdb]);
    let fixups = [
        (0x84, 0x95, PatchFixupEncoding::Long24LowBank),
        (0xac, 0, PatchFixupEncoding::Long24LowBank),
        (0xb6, 8, PatchFixupEncoding::Long24LowBank),
        (0x1cd, 0, PatchFixupEncoding::Long24LowBank),
        (0x1d8, 8, PatchFixupEncoding::Long24LowBank),
        (0x269, 0x273, PatchFixupEncoding::Low16),
    ]
    .into_iter()
    .map(|(offset, target_addend, encoding)| PatchFixup {
        offset,
        target_payload: 0,
        target_addend,
        encoding,
    })
    .collect();
    PatchPayload { bytes, fixups }
}

#[must_use]
pub fn smw_us_v1_layer3_extended_runtime_writes() -> Vec<PatchWrite> {
    vec![
        fixed(
            0x0000_c08c,
            &[0x9d, 0x7b, 0x18, 0x29, 1, 0x9d, 0xd4, 0x14],
            &[0xea; 8],
        ),
        fixed(0x0000_c0e2, &[1], &[0xff]),
        hook(
            0x0001_2826,
            &[0x30, 0x23, 0x85, 1, 0xa2, 0],
            &[0x5c, 0, 0, 0, 0xea, 0xea],
            0x1b6,
        ),
        hook(
            0x0001_2830,
            &[0xc9, 0xff, 0xf0, 0x17],
            &[0x5c, 0, 0, 0],
            0x2ab,
        ),
        hook(
            0x0001_2834,
            &[0x0a, 0x0a, 0x0a, 0x29],
            &[0x5c, 0, 0, 0],
            0x28e,
        ),
        hook(0x0001_2838, &[0x10, 0x85, 2, 0xc8], &[0x5c, 0, 0, 0], 0x40),
        hook(
            0x0001_2846,
            &[0xc8, 0xc8, 0xe8, 0x80],
            &[0x5c, 0, 0, 0],
            0x298,
        ),
        fixed(0x0001_284d, &[0xfd], &[0xe6]),
        hook(
            0x0001_295b,
            &[0xb7, 0xce, 0x48, 0x29],
            &[0x5c, 0, 0, 0],
            0x83,
        ),
        fixed(0x0001_2968, &[0xa5], &[0x6b]),
        fixed(0x0001_29d7, &[0xc8, 0xa6, 2], &[0x4c, 0x38, 0xa8]),
        hook(
            0x0001_2a61,
            &[0x9d, 0x2a, 0x1e, 0xca],
            &[0x5c, 0, 0, 0],
            0x79,
        ),
        hook(
            0x0001_2b54,
            &[0x29, 1, 0x99, 0xb8, 0x0f],
            &[0x5c, 0, 0, 0, 0xea],
            0x6e,
        ),
        hook(
            0x0001_2bd0,
            &[0x29, 1, 0x9d, 0x93, 0x17],
            &[0x5c, 0, 0, 0, 0xea],
            0x63,
        ),
        hook(0x0001_2f3d, &[0x29, 1, 0x85, 9], &[0x22, 0, 0, 0], 0x5c),
        hook(0x0001_2fa7, &[0x29, 1, 0x85, 9], &[0x22, 0, 0, 0], 0x5c),
        hook(0x0001_2c64, &[0xa9, 1, 0x85, 0x55], &[0x22, 0, 0, 0], 0x9e),
        hook(0x0001_2ca4, &[0xa9, 1, 0x85, 0x55], &[0x22, 0, 0, 0], 0x9e),
        split_hook(0x0000_ac40, 0xd4),
        split_hook(0x0001_503a, 0xd4),
        split_hook(0x0001_7ed6, 0x2a),
        split_hook(0x0001_b86c, 0xd4),
    ]
}

#[must_use]
pub fn smw_us_v1_layer3_extended_runtime_installation_plan() -> RelocatablePatchPlan {
    RelocatablePatchPlan {
        description: "install SMW US Layer 3 extended runtime".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START
                ..SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_END,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![smw_us_v1_layer3_extended_runtime_payload()],
        writes: smw_us_v1_layer3_extended_runtime_writes(),
    }
}

fn split_hook(offset: usize, table: u8) -> PatchWrite {
    hook(
        offset,
        &[
            0x18,
            0x69,
            0x50,
            0xbd,
            table,
            if table == 0x2a { 0x1e } else { 0x14 },
            0x69,
            0,
            0xc9,
            2,
        ],
        &[
            0xea,
            0xea,
            0xeb,
            0xbd,
            table,
            if table == 0x2a { 0x1e } else { 0x14 },
            0x22,
            0,
            0,
            0,
        ],
        0x10,
    )
}

fn fixed(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn hook(offset: usize, expected: &[u8], replacement: &[u8], addend: usize) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: vec![PatchFixup {
            offset: replacement.iter().position(|byte| *byte == 0).unwrap(),
            target_payload: 0,
            target_addend: addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

fn decode_resource() -> Vec<u8> {
    fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => unreachable!("static resource hex"),
        }
    }
    let bytes = RESOURCE_HEX.as_bytes();
    let decoded = bytes
        .chunks_exact(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect::<Vec<_>>();
    debug_assert_eq!(decoded.len(), SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN);
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchError};
    use lm_rom::{RomImage, SnesChecksum, pc_to_snes};
    use std::{fs, path::PathBuf};

    fn fixtures() -> (RomImage, RomImage) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        (
            RomImage::from_bytes(fs::read(fixture.join("before.smc")).unwrap()).unwrap(),
            RomImage::from_bytes(fs::read(fixture.join("after.smc")).unwrap()).unwrap(),
        )
    }

    #[test]
    fn typed_payload_relocations_reproduce_complete_wine_block() {
        let (_, after) = fixtures();
        let payload = smw_us_v1_layer3_extended_runtime_payload();
        let installed_pc = SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START + 8;
        let base = pc_to_snes(Mapper::LoRom, installed_pc).unwrap() & 0x7f_ffff;
        let mut resolved = payload.bytes;
        for fixup in payload.fixups {
            let target = base + u32::try_from(fixup.target_addend).unwrap();
            match fixup.encoding {
                PatchFixupEncoding::Long24LowBank => {
                    resolved[fixup.offset..fixup.offset + 3]
                        .copy_from_slice(&target.to_le_bytes()[..3]);
                }
                PatchFixupEncoding::Low16 => {
                    let low = u16::try_from(target & 0xffff).unwrap();
                    resolved[fixup.offset..fixup.offset + 2].copy_from_slice(&low.to_le_bytes());
                }
                encoding => panic!("unexpected extended-runtime encoding {encoding:?}"),
            }
        }
        assert_eq!(
            resolved,
            after
                .read(installed_pc, SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN)
                .unwrap()
        );
    }

    #[test]
    fn every_write_matches_pristine_and_wine_evidence() {
        let (before, after) = fixtures();
        let installed_pc = SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START + 8;
        let base = pc_to_snes(Mapper::LoRom, installed_pc).unwrap() & 0x7f_ffff;
        let writes = smw_us_v1_layer3_extended_runtime_writes();
        assert_eq!(writes.len(), 22);
        for write in writes {
            assert_eq!(
                before.read(write.offset, write.expected.len()).unwrap(),
                write.expected
            );
            let mut resolved = write.replacement;
            for fixup in write.fixups {
                let target = base + u32::try_from(fixup.target_addend).unwrap();
                resolved[fixup.offset..fixup.offset + 3]
                    .copy_from_slice(&target.to_le_bytes()[..3]);
            }
            assert_eq!(
                after.read(write.offset, resolved.len()).unwrap(),
                resolved,
                "extended-runtime write at {:#x}",
                write.offset
            );
        }
    }

    #[test]
    fn plan_is_checksum_valid_failure_atomic_and_one_step_undoable() {
        let (before, after) = fixtures();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_extended_runtime_installation_plan();
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START
        );
        assert_eq!(
            project
                .rom
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN
                )
                .unwrap(),
            after
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN
                )
                .unwrap()
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);

        let (mut before, _) = fixtures();
        before.write(0x0001_b86c, &[0xff]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch {
                index: 21,
                offset: 0x0001_b86c
            })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }
}
