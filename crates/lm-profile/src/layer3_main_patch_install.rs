//! Transactional installation of the `$4C0` Layer 3 main patch.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START: usize = 0x0008_1a05;
pub const SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_END: usize = 0x0010_0000;

const RESOURCE_HEX: &str = concat!(
    "ad5e144ab010af1ac07f3007ade31b3aaae86ba9006baf1ac07faa2904f00404408004a90414408a2908c220f012ad9d",
    "0d29fbff0900048d9d0d8d2c218d2e218a290300eb4a4ac9c000d003a900018d6a14af1bc07f8501e220ad5e1429f8c2",
    "200a0ac900806a8d6c14ac5f1498290f00240110030910000aaa8e5f14bfd7830048f002a200fc57839829f000240150",
    "030900014a4a4aaa8e6014bfd7830048f0078aae0314f001aafc9783ad5e144a4a900aa5228d781ba5248d7a1bae0001",
    "e01df03968f00c8d5a143003a90000aa8e5d1468f00c8d58143003a90000aa8e5c14ae0314d012a65b100ead131429f0",
    "f0f006a980800ce60ba20080046868a2018ed513e2204c0680ad6a14852260ad6a1418651a852260a51a4a186d6a1485",
    "2260a51a4a80f3a51a4a4a80eda51a4a4a4a80e6a51a4a4a4a4a80dea51a4a4a4a4a4a80d5a980001ce60ba90000802f",
    "ae0314d037a69dd02eaebd178aaa10030900ff8504aee60b30dbae5c148a186d5814aa8e5c142900ff100309ff00eb18",
    "652218650485226064268012a55b4ab0f7a65ecaf0f2a9800038e51a8526a69dd033aebd178aaa10030900ff8504ae5c",
    "148a186d5814aa8e5c142900ff100309ff00eb18652218650485228a186d5814ebaa8ebf1760ad6c14852460a51c4a18",
    "6d6c14852460a51c4a80f3a51c4a4a80eda51c4a4a4a80e6a51c4a4a4a4a80dea51c4a4a4a4a4a80d5a980001ce70ba9",
    "0000802fae0314d041a69dd02eaebc178aaa10030900ff8504aee70b30dbae5d148a186d5a14aa8e5d142900ff100309",
    "ff00eb186524186504852460ae0314d04aad6c1418651c8524602c0d19701c207b82d424ad6c1448ae5d14da207b82fa",
    "8e5d14688d6c1468852460a69dd01cae5d148a186d5a14aa8e5d142900ff100309ff00eb186d6c148d6c14ad6c141865",
    "1c303bc9180190328502290f00490800186908018524a55b4aa9b0019005a55e2900ff38e900013021c90001901cc502",
    "9002a50238e51c852860852480f664248502ad5e14290400d0e8603bc90030a51ab014a2058d04428e0642ebeb186d6a",
    "146d1442852260a2018e5022a205c2318d51228e53226d6a146d06238522e210603bc90030a51cb014a2058d04428e06",
    "42ebeb186d6c146d1442852460a2018e5022a205c2318d51228e53226d6c146d06238524e2106009810f811881228134",
    "81eb825081508150815081508150815081508150815081508150810f810f810f810f810f810f8127812d813c810f810f",
    "810f810f810f81d6814c82dc81e681f88121831482148214821482148214821482148214821482148214824c824c824c",
    "824c824c824c82eb81f18100824c824c824c824c824c820000000000000000000000004000800000010002c0ff80ff00",
    "ff00fe00fd00fc0003000400000000000000000000000000000000000000000000000000000000ad31193006ad5e144a",
    "b010686868ad0314f0045c94c4055c14c4054ac220ae5f14b010f003fc5783ae6014f003fc9783e2206bf011ad781b85",
    "2248fc5783a5228d781b688522ae6014f011ad7a1b852448fc9783a5248d7a1b688524e2206bffffffffffffffffffff",
    "af1ac07f3005a90685126bc2216869030048e2206bffffffffffffffffffffff9c9416ad5e142904f002a9258d9316a9",
    "005cba9401202020202020204c4d0401"
);

#[must_use]
pub fn smw_us_v1_layer3_main_patch_payload() -> PatchPayload {
    let mut bytes = decode_resource();
    bytes[0x2b9..0x2bc].copy_from_slice(&[0xad, 0xd7, 0x13]);
    let mut fixups = vec![
        fixup(0x7e, 0x3d7, PatchFixupEncoding::Long24LowBank),
        fixup(0x87, 0x357, PatchFixupEncoding::Low16),
        fixup(0x9c, 0x3d7, PatchFixupEncoding::Long24LowBank),
        fixup(0xaa, 0x397, PatchFixupEncoding::Low16),
        fixup(0x107, 6, PatchFixupEncoding::Low16),
        fixup(0x260, 0x27b, PatchFixupEncoding::Low16),
        fixup(0x26d, 0x27b, PatchFixupEncoding::Low16),
    ];
    for index in 0..64 {
        let offset = 0x357 + index * 2;
        let encoded = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        fixups.push(fixup(
            offset,
            usize::from(encoded - 0x8000),
            PatchFixupEncoding::Low16,
        ));
    }
    for (offset, addend) in [
        (0x43d, 0x357),
        (0x445, 0x397),
        (0x453, 0x357),
        (0x469, 0x397),
    ] {
        fixups.push(fixup(offset, addend, PatchFixupEncoding::Low16));
    }
    PatchPayload { bytes, fixups }
}

#[must_use]
pub fn smw_us_v1_layer3_main_patch_writes() -> Vec<PatchWrite> {
    vec![
        entry(
            0x0000_201f,
            &[0xad, 0xe3, 0x1b, 0xf0, 0x20, 0x3a],
            &[0x22, 0, 0, 0, 0xf0, 0x1f],
            0,
        ),
        entry(0x0000_2153, &[0xa9, 6, 0x85, 0x12], &[0x22, 0, 0, 0], 0x480),
        entry(
            0x0000_94b6,
            &[0xa9, 0, 0x8d, 0x93, 0x16],
            &[0x5c, 0, 0, 0, 0x60],
            0x4a0,
        ),
        entry(
            0x0002_c40c,
            &[0xad, 3, 0x14, 0xf0, 3],
            &[0x22, 0, 0, 0, 0x60],
            0x417,
        ),
    ]
}

#[must_use]
pub fn smw_us_v1_layer3_main_patch_installation_plan() -> RelocatablePatchPlan {
    RelocatablePatchPlan {
        description: "install SMW US Layer 3 main patch".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START
                ..SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_END,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![smw_us_v1_layer3_main_patch_payload()],
        writes: smw_us_v1_layer3_main_patch_writes(),
    }
}

fn fixup(offset: usize, target_addend: usize, encoding: PatchFixupEncoding) -> PatchFixup {
    PatchFixup {
        offset,
        target_payload: 0,
        target_addend,
        encoding,
    }
}

fn entry(offset: usize, expected: &[u8], replacement: &[u8], addend: usize) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: vec![fixup(1, addend, PatchFixupEncoding::Long24LowBank)],
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
    let encoded = RESOURCE_HEX.as_bytes();
    encoded
        .chunks_exact(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN;
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

    fn resolve(payload: PatchPayload, installed_pc: usize) -> Vec<u8> {
        let base = pc_to_snes(Mapper::LoRom, installed_pc).unwrap() & 0x7f_ffff;
        let mut bytes = payload.bytes;
        for fixup in payload.fixups {
            let target = base + u32::try_from(fixup.target_addend).unwrap();
            match fixup.encoding {
                PatchFixupEncoding::Long24LowBank => bytes[fixup.offset..fixup.offset + 3]
                    .copy_from_slice(&target.to_le_bytes()[..3]),
                PatchFixupEncoding::Low16 => {
                    let low = u16::try_from(target & 0xffff).unwrap();
                    bytes[fixup.offset..fixup.offset + 2].copy_from_slice(&low.to_le_bytes());
                }
                encoding => panic!("unexpected main-patch encoding {encoding:?}"),
            }
        }
        bytes
    }

    #[test]
    fn source_resource_and_all_relocations_reproduce_complete_wine_payload() {
        let (_, after) = fixtures();
        let installed_pc = SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START + 8;
        let payload = smw_us_v1_layer3_main_patch_payload();
        assert_eq!(payload.bytes.len(), SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN);
        assert_eq!(payload.fixups.len(), 75);
        assert_eq!(
            resolve(payload, installed_pc),
            after
                .read(installed_pc, SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN)
                .unwrap()
        );
    }

    #[test]
    fn entries_and_transaction_match_wine_and_undo_atomically() {
        let (before, after) = fixtures();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_main_patch_installation_plan();
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START
        );
        assert_eq!(
            project
                .rom
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN
                )
                .unwrap(),
            after
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN
                )
                .unwrap()
        );
        for write in &plan.writes {
            assert_eq!(
                project
                    .rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                after.read(write.offset, write.replacement.len()).unwrap()
            );
        }
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);

        let (mut before, _) = fixtures();
        before.write(0x0002_c40c, &[0xff]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch {
                index: 3,
                offset: 0x0002_c40c
            })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }
}
