//! Authenticated Lunar Magic 3.63 FastROM conversion primitives for SMW US revision 0.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};

const ERSANIO_JSL_SOURCES: &[u32] = &[
    0x808203, 0x808209, 0x808e6b, 0x809325, 0x8094a3, 0x8094f2, 0x809529, 0x80953f, 0x809557,
    0x809560, 0x8095a4, 0x8095af, 0x8095b5, 0x8095bc, 0x8095ca, 0x8095ce, 0x8095ef, 0x8095f6,
    0x809636, 0x80967f, 0x8096e5, 0x8096f4, 0x809712, 0x809716, 0x8097d8, 0x809860, 0x80986c,
    0x80988c, 0x809947, 0x809a58, 0x809a5c, 0x809a6a, 0x809aa4, 0x809b89, 0x809bc0, 0x809bc4,
    0x809e13, 0x809fe4, 0x80a08f, 0x80a101, 0x80a126, 0x80a149, 0x80a15a, 0x80a161, 0x80a165,
    0x80a1c7, 0x80a1df, 0x80a299, 0x80a29d, 0x80a2a1, 0x80a2a5, 0x80a2d8, 0x80a2e2, 0x80a2e6,
    0x80a5ab, 0x80a5bf, 0x80a5fd, 0x80a82f, 0x80a873, 0x80a9a6, 0x80aa6b, 0x80aa7a, 0x80ab44,
    0x80af31, 0x80c485, 0x80c58a, 0x80c595, 0x80c5c0, 0x80c773, 0x80c87d, 0x80c883, 0x80c889,
    0x80c944, 0x80cd8b, 0x80d19d, 0x80d1a1, 0x80da7f, 0x80e2c6, 0x80e99c, 0x80ebbd, 0x80ec86,
    0x80ecab, 0x80ed02, 0x80ee72, 0x80ee7f, 0x80ef8c, 0x80efb2, 0x80effb, 0x80f159, 0x80f1f1,
    0x80f243, 0x80f25c, 0x80f27b, 0x80f285, 0x80f2b1, 0x80f2d1, 0x80f317, 0x80f33f, 0x80f35f,
    0x80f367, 0x80f36f, 0x80f38b, 0x80f4dd, 0x80f5b2, 0x80f5f8, 0x80f629, 0x80f9c3, 0x80fa75,
    0x80fb60, 0x80fbdb, 0x80fbf3, 0x80fc01, 0x80fc0e, 0x80fc1e, 0x80fc8f, 0x80fcbc, 0x8180ba,
    0x818133, 0x81816d, 0x818179, 0x81836e, 0x81849b, 0x8184c6, 0x818575, 0x81858e, 0x8185c8,
    0x81875e, 0x818789, 0x81878e, 0x818793, 0x818798, 0x81879d, 0x8187a2, 0x8187a7, 0x8187ac,
    0x8187b1, 0x8187b6, 0x8187bb, 0x8187c0, 0x8187c5, 0x8187ca, 0x8187cf, 0x8187d4, 0x8187d9,
    0x8187de, 0x8187e3, 0x8187e8, 0x8187ed, 0x8187f2, 0x8187f7, 0x8187fc, 0x818801, 0x818806,
    0x81880b, 0x818810, 0x818815, 0x81881a, 0x81881f, 0x818824, 0x818829, 0x81882e, 0x818833,
    0x818838, 0x818842, 0x81884d, 0x818858, 0x818863, 0x81886a, 0x818874, 0x81887f, 0x81888a,
    0x818893, 0x8188cd, 0x8188d1, 0x818ab8, 0x818abe, 0x818ac3, 0x818adf, 0x818c34, 0x818d96,
    0x818f38, 0x818f50, 0x818fbd, 0x819207, 0x81920c, 0x819228, 0x819347, 0x819380, 0x819533,
    0x819594, 0x8195d2, 0x819675, 0x8196e1, 0x8196f6, 0x819798, 0x8199c9, 0x8199fe, 0x819ac0,
    0x819fcf, 0x81a068, 0x81a079, 0x81a162, 0x81a24c, 0x81a250, 0x81a2dc, 0x81a300, 0x81a306,
    0x81a355, 0x81a5ea, 0x81a619, 0x81a667, 0x81a670, 0x81a6bb, 0x81a6c1, 0x81a824, 0x81a828,
    0x81a82c, 0x81a847, 0x81a85a, 0x81a8dd, 0x81a8e1, 0x81a917, 0x81a924, 0x81a931, 0x81a938,
    0x81a96f, 0x81a9a4, 0x81ab28, 0x81ab64, 0x81acfc, 0x81ad01, 0x81ae43, 0x81ae91, 0x81aeb9,
    0x81b05e, 0x81b062, 0x81b070, 0x81b08e, 0x81b101, 0x81b149, 0x81b159, 0x81b18d, 0x81b933,
    0x81b937, 0x81bade, 0x81bafd, 0x81bb04, 0x81bb22, 0x81bc71, 0x81bc97, 0x81bcb5, 0x81bde6,
    0x81be08, 0x81be1e, 0x81bf53, 0x81c11f, 0x81c128, 0x81c17a, 0x81c1e9, 0x81c29a, 0x81c2a0,
    0x81c335, 0x81c340, 0x81c4cf, 0x81c4ec, 0x81c550, 0x81c576, 0x81c592, 0x81c5a3, 0x81c5a7,
    0x81c604, 0x81c63c, 0x81c69c, 0x81c9e3, 0x81cac2, 0x81cac6, 0x81cd2a, 0x81cd56, 0x81cd69,
    0x81cdd8, 0x81ce0e, 0x81ce61, 0x81ce6e, 0x81d036, 0x81d0a4, 0x81d119, 0x81d2bd, 0x81d2c4,
    0x81d319, 0x81d36a, 0x81d3e3, 0x81d67c, 0x81d75e, 0x81d9c9, 0x81ddd9, 0x81ddf5, 0x81e0d7,
    0x81e111, 0x81e1ed, 0x81e1fc, 0x81e201, 0x81e205, 0x81e2d4, 0x81e33f, 0x81e3a1, 0x81e536,
    0x81e5bf, 0x81e5ee, 0x81e5f7, 0x81e604, 0x81e848, 0x81ea21, 0x81ea52, 0x81ec84, 0x81ecc5,
    0x81ed2e, 0x81ed32, 0x81ed64, 0x81ef13, 0x81f025, 0x81f0a8, 0x81f0c7, 0x81f0d8, 0x81f2f9,
    0x81f589, 0x81f590, 0x81f595, 0x81f5bc, 0x81f5c1, 0x81f5e7, 0x81f66a, 0x81f66f, 0x81f673,
    0x81f77d, 0x81f855, 0x81f867, 0x81f86c, 0x81f884, 0x81fa47, 0x81fac3, 0x81fb36, 0x81fb6a,
    0x81fcba, 0x81fcd3, 0x81fcf0, 0x81fd05, 0x81fd30, 0x81fd80, 0x81fdc3, 0x81ffb9, 0x82804d,
    0x8280bc, 0x828152, 0x828157, 0x82815b, 0x8283a7, 0x8284ac, 0x82855f, 0x82857b, 0x82859d,
    0x8285ef, 0x82862f, 0x828651, 0x828779, 0x828784, 0x8287e8, 0x8288fd, 0x82894f, 0x8289c0,
    0x828a3c, 0x828a7d, 0x828b01, 0x828b35, 0x828b61, 0x828b94, 0x828cec, 0x828cf0, 0x828cf6,
    0x82905e, 0x8291e8, 0x829366, 0x8293de, 0x8293ee, 0x829451, 0x829457, 0x82949e, 0x82961a,
    0x82962c, 0x829643, 0x8296c7, 0x829aa8, 0x829b27, 0x82a0d4, 0x82a0db, 0x82a11e, 0x82a132,
    0x82a3fe, 0x82a405, 0x82a412, 0x82a43d, 0x82a4ae, 0x82a5b4, 0x82a6eb, 0x82a70a, 0x82a75f,
    0x82a76d, 0x82a884, 0x82a9b9, 0x82a9c9, 0x82aa40, 0x82aae2, 0x82ac40, 0x82ace9, 0x82acf1,
    0x82adfb, 0x82af45, 0x82af55, 0x82afaf, 0x82afbf, 0x82b008, 0x82b03c, 0x82b051, 0x82b055,
    0x82b082, 0x82b097, 0x82b09b, 0x82b115, 0x82b126, 0x82b161, 0x82b171, 0x82b183, 0x82b1c2,
    0x82b1d2, 0x82b1d6, 0x82b20d, 0x82b21d, 0x82b221, 0x82b24d, 0x82b291, 0x82b295, 0x82b2a8,
    0x82b2dc, 0x82b2ec, 0x82b2f0, 0x82b32f, 0x82b33b, 0x82b34d, 0x82b3ac, 0x82b3e5, 0x82b40f,
    0x82b4a1, 0x82b4d5, 0x82b681, 0x82b6ba, 0x82b6e8, 0x82b7a7, 0x82b7c2, 0x82b7c7, 0x82b7cb,
    0x82b82e, 0x82b84b, 0x82b87d, 0x82b8a3, 0x82b9b8, 0x82b9e5, 0x82bae8, 0x82bb06, 0x82bbbd,
    0x82bbfb, 0x82bc09, 0x82bcdb, 0x82bcdf, 0x82bd1d, 0x82bd46, 0x82bd60, 0x82bd86, 0x82bdc4,
    0x82be54, 0x82bfcd, 0x82bfd8, 0x82bffc, 0x82c026, 0x82c053, 0x82c19a, 0x82c1ba, 0x82c261,
    0x82c265, 0x82c2b9, 0x82c2c1, 0x82c2d6, 0x82c2de, 0x82c338, 0x82c4d0, 0x82c5bc, 0x82c5e4,
    0x82c6e1, 0x82c7a2, 0x82c7bf, 0x82c7d7, 0x82c7db, 0x82c815, 0x82cc09, 0x82cc29, 0x82cd7b,
    0x82cd7f, 0x82cdf4, 0x82d20b, 0x82d3ea, 0x82d40b, 0x82d42e, 0x82d4d7, 0x82d59f, 0x82d71e,
    0x82d732, 0x82d746, 0x82d750, 0x82d8c4, 0x82d930, 0x82d95d, 0x82d961, 0x82d980, 0x82da72,
    0x82dbe0, 0x82dc09, 0x82dcc8, 0x82dccc, 0x82dcdd, 0x82dd8f, 0x82ddbb, 0x82dec1, 0x82dee6,
    0x82deec, 0x82df9d, 0x82dfaf, 0x82dfbe, 0x82e0cd, 0x82e0e4, 0x82e114, 0x82e129, 0x82e132,
    0x82e191, 0x82e23f, 0x82e245, 0x82e2d9, 0x82e2de, 0x82e34d, 0x82e41f, 0x82e429, 0x82e449,
    0x82e45e, 0x82e467, 0x82e48f, 0x82e54a, 0x82e554, 0x82e5e8, 0x82e618, 0x82e61f, 0x82e623,
    0x82e6e7, 0x82e727, 0x82e732, 0x82e736, 0x82e743, 0x82e7c9, 0x82e7e6, 0x82e828, 0x82e8b5,
    0x82e8dd, 0x82e8fd, 0x82e959, 0x82e95f, 0x82e9ca, 0x82ea8c, 0x82ea91, 0x82ea95, 0x82eaa0,
    0x82eada, 0x82eaf2, 0x82eb19, 0x82eb50, 0x82eb7f, 0x82ed93, 0x82edc6, 0x82edf6, 0x82ee48,
    0x82ee57, 0x82ef26, 0x82ef9d, 0x82f03f, 0x82f097, 0x82f270, 0x82f27c, 0x82f286, 0x82f296,
    0x82f2b5, 0x82f333, 0x82f373, 0x82f3ab, 0x82f3b9, 0x82f3ce, 0x82f50b, 0x82f55f, 0x82f821,
    0x82f8f2, 0x82f9a1, 0x82f9e6, 0x82f9ea, 0x82f9fa, 0x82fa1a, 0x82fbd6, 0x82ff6c, 0x838012,
    0x83801d, 0x838029, 0x83802d, 0x83805c, 0x838087, 0x83808b, 0x8380ac, 0x838182, 0x838186,
    0x838197, 0x8381a9, 0x8381f7, 0x8381fc, 0x838200, 0x83822d, 0x8383bd, 0x83844f, 0x838472,
    0x838476, 0x8384bf, 0x8384ca, 0x83851e, 0x838522, 0x838526, 0x838540, 0x83856c, 0x838573,
    0x838582, 0x8385ef, 0x838636, 0x838642, 0x838649, 0x8386fa, 0x83871b, 0x83871f, 0x838769,
    0x83878f, 0x838793, 0x8387d7, 0x838826, 0x83889b, 0x8388a3, 0x8388c7, 0x8388cb, 0x8388cf,
    0x8388d5, 0x838964, 0x83897a, 0x838996, 0x8389a4, 0x838a48, 0x838a57, 0x838a5b, 0x838a6f,
    0x838a7f, 0x838ad4, 0x838b09, 0x838ba5, 0x838c5c, 0x838c65, 0x838c6c, 0x838c70, 0x838d61,
    0x838d6f, 0x838da8, 0x838ddc, 0x838de3, 0x838df0, 0x838dfa, 0x838e6c, 0x838e9c, 0x838ea3,
    0x838ee5, 0x838ef6, 0x838f02, 0x838f68, 0x838f84, 0x838fc2, 0x838fc6, 0x838fca, 0x838fea,
    0x838ff2, 0x83906c, 0x8390dc, 0x8390e3, 0x83911f, 0x839123, 0x839129, 0x83920f, 0x839214,
    0x83923e, 0x839244, 0x839267, 0x839284, 0x8392a3, 0x8392ce, 0x8392d2, 0x8392d9, 0x839337,
    0x839367, 0x83939f, 0x839434, 0x83947f, 0x839488, 0x83949a, 0x83950e, 0x83956e, 0x839581,
    0x839585, 0x8395a3, 0x8395a7, 0x8395d9, 0x8395e8, 0x839612, 0x839646, 0x8396f1, 0x839700,
    0x839704, 0x839713, 0x839722, 0x83978c, 0x8397f4, 0x83987a, 0x83987e, 0x83989f, 0x8398be,
    0x839902, 0x8399ee, 0x839a3c, 0x839a77, 0x839ac6, 0x839aee, 0x839bd4, 0x839c2f, 0x839c3a,
    0x839c58, 0x839c5c, 0x839c62, 0x839cbf, 0x839d8a, 0x839d8e, 0x839d99, 0x839ea4, 0x839f2d,
    0x839f3f, 0x839f48, 0x839f9b, 0x839fd1, 0x83a002, 0x83a006, 0x83a056, 0x83a06d, 0x83a0ec,
    0x83a0f1, 0x83a113, 0x83a259, 0x83a2a1, 0x83a2f8, 0x83a31d, 0x83a321, 0x83a328, 0x83a432,
    0x83a647, 0x83a79e, 0x83a821, 0x83a914, 0x83ac8d, 0x83acab, 0x83acda, 0x83ad23, 0x83ad3c,
    0x83ad79, 0x83adcc, 0x83add0, 0x83add4, 0x83ae0f, 0x83afdc, 0x83b063, 0x83b098, 0x83b0c0,
    0x83b0cc, 0x83b0d2, 0x83b0f5, 0x83b0ff, 0x83b103, 0x83b10c, 0x83b110, 0x83b13d, 0x83b15b,
    0x83b16d, 0x83b171, 0x83b175, 0x83b2a2, 0x83b2a9, 0x83b2bb, 0x83b2bf, 0x83b3e7, 0x83b437,
    0x83c01e, 0x83c083, 0x83c0a7, 0x83c0ad, 0x83c1f9, 0x83c240, 0x83c2d4, 0x83c301, 0x83c312,
    0x83c31c, 0x83c34c, 0x83c35e, 0x83c362, 0x83c3ae, 0x83c449, 0x83c7f0, 0x83c818, 0x83c85b,
    0x83c949, 0x83cbb3, 0x83cbc0, 0x83cc25, 0x83cc94, 0x83ccc4, 0x83ccf4, 0x83cd32, 0x83ce09,
    0x83ce5a, 0x83ce83, 0x83cea2, 0x83cea7, 0x83ceb3, 0x83ceb9, 0x83cebd, 0x83ced7, 0x83ceed,
    0x83d77a, 0x83ddc8, 0x84827a, 0x848579, 0x8485c5, 0x8485cf, 0x8498f6, 0x849dfd, 0x84d714,
    0x84d718, 0x84daf4, 0x84dbae, 0x84e573, 0x84eb4b, 0x84f3e6, 0x84f3fa, 0x84f526, 0x84f590,
    0x84f610, 0x84f85b, 0x858098, 0x8580bf, 0x8580c3, 0x8580c7, 0x8586e5, 0x8586ec, 0x858730,
    0x858770, 0x85881f, 0x858888, 0x8588f1, 0x85895a, 0x85bc83, 0x85bcb4, 0x85bcec, 0x85bd13,
    0x85cc0a, 0x85dafb, 0x87f267, 0x87f79a, 0x87f7d2, 0x87f7d6, 0x87fc3e, 0x8c943f, 0x8ca1da,
    0x8ca75f, 0x8ca785, 0x8ca7b4, 0x8cae0a, 0x8cc9a1, 0x8cc9bc, 0x8cc9d2, 0x8cc9ec, 0x8cca1b,
    0x8cca45, 0x8cca6a, 0x8cca75, 0x8da10b, 0x8da41a, 0x8da451, 0x8dab4c, 0x8dc196, 0x8dc346,
    0x8dcd96, 0x8dcf58, 0x8dd076, 0x8dd996, 0x8ddaf6, 0x8ddd8f, 0x8de896,
];

const PACKED_POINTER_TABLES: &[(u32, usize)] = &[
    (0x84857d, 0x0d),
    (0x858823, 0x20),
    (0x85888c, 0x20),
    (0x8588f5, 0x20),
    (0x85895e, 0x20),
    (0x85daff, 0x03),
    (0x8ca1de, 0x05),
    (0x8da10f, 0x100),
    (0x8da41e, 0x0f),
    (0x8da455, 0x3f),
    (0x8dab50, 0x0a),
    (0x8dc19a, 0x3f),
    (0x8dc34a, 0x02),
    (0x8dcd9a, 0x3f),
    (0x8dcf5c, 0x06),
    (0x8dd07a, 0x02),
    (0x8dd99a, 0x3f),
    (0x8ddafa, 0x04),
    (0x8ddd93, 0x02),
    (0x8de89a, 0x3f),
];

const BANK_LOAD_SOURCES: &[u32] = &[
    0x80c586, 0x80f155, 0x80f1ed, 0x80f23f, 0x80f9bf, 0x81883e, 0x818849, 0x818854, 0x81885f,
    0x818870, 0x81887b, 0x818886, 0x818adb, 0x8199f9, 0x819fcb, 0x81ae3e, 0x81d7b1, 0x81d8c9,
    0x81e844, 0x82b7bc, 0x82e489, 0x838227, 0x8485c1, 0x8ca75b,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FastRomPatchError {
    Address { address: u32, source: RomError },
    Truncated { address: u32, offset: usize },
    FixedRange { offset: usize, len: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1FastRomPatchState {
    Absent,
    Installed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1FastRomPatchDetectError {
    Truncated { offset: usize, len: usize },
    PartialOrModified,
    RuntimeAddress(RomError),
    RuntimeOwner(lm_rats::HeaderError),
    RuntimePayload,
}

impl std::fmt::Display for SmwUsV1FastRomPatchDetectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid SMW US FastROM patch: {self:?}")
    }
}

impl std::error::Error for SmwUsV1FastRomPatchDetectError {}

impl std::fmt::Display for FastRomPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot apply SMW FastROM patch: {self:?}")
    }
}

impl std::error::Error for FastRomPatchError {}

/// Applies Lunar Magic 3.63's authenticated Ersanio JSL-bank conversion pass.
///
/// Entries whose original instruction has already changed are deliberately skipped, matching the
/// native tool's guarded behavior. The caller stages this together with the speed hook, metadata,
/// checksum, and any level changes before publishing one transaction.
pub fn apply_smw_us_v1_fastrom_jsl_pass(bytes: &mut [u8]) -> Result<usize, FastRomPatchError> {
    let mut changed = 0;
    for &address in ERSANIO_JSL_SOURCES {
        let offset = snes_to_pc(Mapper::LoRom, address)
            .map_err(|source| FastRomPatchError::Address { address, source })?;
        let instruction = bytes
            .get_mut(offset..offset + 4)
            .ok_or(FastRomPatchError::Truncated { address, offset })?;
        if instruction[0] == 0x22 && instruction[2] & 0x80 != 0 && instruction[3] < 0x10 {
            instruction[3] |= 0x80;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Converts the two additional original-game pointer families recovered from `$0049082A` through
/// `$00490C08`: twenty packed 24-bit tables and twenty-four `LDA #bank / PHA / PLB` sites.
pub fn apply_smw_us_v1_fastrom_pointer_passes(
    bytes: &mut [u8],
) -> Result<usize, FastRomPatchError> {
    let mut changed = 0;
    for &(address, count) in PACKED_POINTER_TABLES {
        let offset = resolve(address)?;
        let end = offset
            .checked_add(
                count
                    .checked_mul(3)
                    .ok_or(FastRomPatchError::Truncated { address, offset })?,
            )
            .ok_or(FastRomPatchError::Truncated { address, offset })?;
        let table = bytes
            .get_mut(offset..end)
            .ok_or(FastRomPatchError::Truncated { address, offset })?;
        for pointer in table.chunks_exact_mut(3) {
            if pointer[1] & 0x80 != 0 && pointer[2] < 0x10 {
                pointer[2] |= 0x80;
                changed += 1;
            }
        }
    }
    for &address in BANK_LOAD_SOURCES {
        let offset = resolve(address)?;
        let instruction = bytes
            .get_mut(offset..offset + 4)
            .ok_or(FastRomPatchError::Truncated { address, offset })?;
        if instruction[0] == 0xa9
            && instruction[1] < 0x10
            && instruction[2] == 0x48
            && instruction[3] == 0xab
        {
            instruction[1] |= 0x80;
            changed += 1;
        }
    }
    Ok(changed)
}

fn resolve(address: u32) -> Result<usize, FastRomPatchError> {
    snes_to_pc(Mapper::LoRom, address)
        .map_err(|source| FastRomPatchError::Address { address, source })
}

const TRAMPOLINE_OFFSET: usize = 0x003a4e;
const MAP_MODE_OFFSET: usize = 0x007fd5;
const FIRST_HOOK_WORD_OFFSET: usize = 0x007fea;
const SECOND_HOOK_WORD_OFFSET: usize = 0x007ffc;
const PATCH_MARKER_OFFSET: usize = 0x007f_fef;

/// Authenticates the complete fixed hook and dynamically owned runtime contract.
pub fn detect_smw_us_v1_fastrom_patch(
    bytes: &[u8],
) -> Result<SmwUsV1FastRomPatchState, SmwUsV1FastRomPatchDetectError> {
    let map_mode = detect_exact(bytes, MAP_MODE_OFFSET, 1)?[0];
    let first_hook = detect_exact(bytes, FIRST_HOOK_WORD_OFFSET, 2)?;
    let second_hook = detect_exact(bytes, SECOND_HOOK_WORD_OFFSET, 2)?;
    let trampoline = detect_exact(bytes, TRAMPOLINE_OFFSET, 8)?;
    let marker = detect_exact(bytes, PATCH_MARKER_OFFSET, 1)?[0];
    let absent = map_mode == 0x20
        && first_hook == [0x6a, 0x81]
        && second_hook == [0x00, 0x80]
        && trampoline.iter().all(|byte| *byte == 0xff)
        && marker != lm_rom::LunarMagicRomMetadata::FASTROM_MARKER;
    if absent {
        return Ok(SmwUsV1FastRomPatchState::Absent);
    }
    if map_mode != 0x30
        || first_hook != [0x4e, 0xba]
        || second_hook != [0x52, 0xba]
        || trampoline[..5] != [0x5c, 0x6a, 0x81, 0x80, 0x5c]
        || marker != lm_rom::LunarMagicRomMetadata::FASTROM_MARKER
    {
        return Err(SmwUsV1FastRomPatchDetectError::PartialOrModified);
    }
    let runtime_address =
        u32::from(trampoline[5]) | u32::from(trampoline[6]) << 8 | u32::from(trampoline[7]) << 16;
    let payload_offset = snes_to_pc(Mapper::LoRom, runtime_address)
        .map_err(SmwUsV1FastRomPatchDetectError::RuntimeAddress)?;
    let header_offset = payload_offset
        .checked_sub(lm_rats::HEADER_LEN)
        .ok_or(SmwUsV1FastRomPatchDetectError::RuntimePayload)?;
    let block = lm_rats::parse_at(bytes, header_offset)
        .map_err(SmwUsV1FastRomPatchDetectError::RuntimeOwner)?;
    if block.payload.start != payload_offset
        || detect_exact(bytes, block.payload.start, block.payload.len())?
            != [
                0x78, 0xa9, 0x01, 0x8d, 0x0d, 0x42, 0x5c, 0x00, 0x80, 0x80, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xff,
            ]
    {
        return Err(SmwUsV1FastRomPatchDetectError::RuntimePayload);
    }
    Ok(SmwUsV1FastRomPatchState::Installed)
}

fn detect_exact(
    bytes: &[u8],
    offset: usize,
    len: usize,
) -> Result<&[u8], SmwUsV1FastRomPatchDetectError> {
    bytes
        .get(offset..offset.saturating_add(len))
        .ok_or(SmwUsV1FastRomPatchDetectError::Truncated { offset, len })
}

/// Builds the failure-atomic core FastROM installation plan for ordinary SMW US revision 0.
/// The caller supplies the profile-wide protected allocation policy used for the 16-byte runtime.
pub fn smw_us_v1_fastrom_patch_plan(
    bytes: &[u8],
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, FastRomPatchError> {
    let mut converted = bytes.to_vec();
    apply_smw_us_v1_fastrom_jsl_pass(&mut converted)?;
    apply_smw_us_v1_fastrom_pointer_passes(&mut converted)?;

    let mut writes = changed_byte_writes(bytes, &converted);
    let first_target = exact(bytes, FIRST_HOOK_WORD_OFFSET, 2)?;
    let second_target = exact(bytes, SECOND_HOOK_WORD_OFFSET, 2)?;
    let trampoline_address = pc_to_snes(Mapper::LoRom, TRAMPOLINE_OFFSET).map_err(|source| {
        FastRomPatchError::Address {
            address: u32::try_from(TRAMPOLINE_OFFSET).unwrap_or(u32::MAX),
            source,
        }
    })?;
    let trampoline_low = (trampoline_address as u16).to_le_bytes();
    let second_hook = u16::from_le_bytes(trampoline_low)
        .wrapping_add(4)
        .to_le_bytes();

    let runtime = vec![
        0x78,
        0xa9,
        0x01,
        0x8d,
        0x0d,
        0x42,
        0x5c,
        second_target[0],
        second_target[1],
        0x80,
        0xff,
        0xff,
        0xff,
        0xff,
        0xff,
        0xff,
    ];
    writes.extend([
        fixed_write(MAP_MODE_OFFSET, exact(bytes, MAP_MODE_OFFSET, 1)?, &[0x30]),
        fixed_write(FIRST_HOOK_WORD_OFFSET, first_target, &trampoline_low),
        fixed_write(SECOND_HOOK_WORD_OFFSET, second_target, &second_hook),
        PatchWrite {
            offset: TRAMPOLINE_OFFSET,
            expected: exact(bytes, TRAMPOLINE_OFFSET, 8)?.to_vec(),
            replacement: vec![0x5c, first_target[0], first_target[1], 0x80, 0x5c, 0, 0, 0],
            fixups: vec![PatchFixup {
                offset: 5,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }],
        },
        fixed_write(
            PATCH_MARKER_OFFSET,
            exact(bytes, PATCH_MARKER_OFFSET, 1)?,
            &[lm_rom::LunarMagicRomMetadata::FASTROM_MARKER],
        ),
    ]);

    Ok(RelocatablePatchPlan {
        description: "enable SMW US FastROM speed and apply patch".into(),
        mapper: Mapper::LoRom,
        allocation,
        checksum_field,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: runtime,
            fixups: Vec::new(),
        }],
        writes,
    })
}

fn exact(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], FastRomPatchError> {
    bytes
        .get(offset..offset.saturating_add(len))
        .ok_or(FastRomPatchError::FixedRange { offset, len })
}

fn fixed_write(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn changed_byte_writes(before: &[u8], after: &[u8]) -> Vec<PatchWrite> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter_map(|(offset, (&expected, &replacement))| {
            (expected != replacement).then(|| fixed_write(offset, &[expected], &[replacement]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pristine_smw_matches_the_recovered_jsl_conversion_count_and_is_idempotent() {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = lm_rom::RomImage::from_bytes(bytes).unwrap();
        bytes = image.logical_bytes().to_vec();
        assert_eq!(apply_smw_us_v1_fastrom_jsl_pass(&mut bytes), Ok(916));
        assert_eq!(apply_smw_us_v1_fastrom_jsl_pass(&mut bytes), Ok(0));
    }

    #[test]
    fn modified_entries_are_skipped_but_truncated_images_reject() {
        let first = snes_to_pc(Mapper::LoRom, ERSANIO_JSL_SOURCES[0]).unwrap();
        let mut bytes = vec![0; first + 4];
        bytes[first..first + 4].copy_from_slice(&[0x22, 0x34, 0x80, 0x0c]);
        assert_eq!(
            apply_smw_us_v1_fastrom_jsl_pass(&mut bytes),
            Err(FastRomPatchError::Truncated {
                address: ERSANIO_JSL_SOURCES[1],
                offset: snes_to_pc(Mapper::LoRom, ERSANIO_JSL_SOURCES[1]).unwrap(),
            })
        );
        assert_eq!(bytes[first + 3], 0x8c);
    }

    #[test]
    fn pristine_pointer_families_match_recovered_counts_and_are_idempotent() {
        let image =
            lm_rom::RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        assert_eq!(apply_smw_us_v1_fastrom_pointer_passes(&mut bytes), Ok(771));
        assert_eq!(apply_smw_us_v1_fastrom_pointer_passes(&mut bytes), Ok(0));
    }

    #[test]
    fn retained_level_save_builds_an_atomic_core_plan() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                .unwrap();
        let image = lm_rom::RomImage::from_bytes(bytes).unwrap();
        let logical = image.logical_bytes();
        let plan = smw_us_v1_fastrom_patch_plan(
            logical,
            AllocationPolicy::lorom(0x8_7e6f..logical.len()),
            crate::SMW_US_V1_CHECKSUM_FIELD,
        )
        .unwrap();
        assert_eq!(plan.payloads[0].bytes.len(), 16);
        assert_eq!(
            plan.writes
                .iter()
                .filter(|write| write.replacement == [0x30])
                .count(),
            1
        );
        assert!(plan.writes.len() > 1_600);
    }

    #[test]
    fn core_plan_installs_reopens_checksum_and_undoes_as_one_edit() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            std::fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                .unwrap();
        let mut project = lm_project::Project::open_supported(
            lm_rom::RomImage::from_bytes(original.clone()).unwrap(),
        )
        .unwrap();
        let logical_len = project.rom.logical_len();
        let plan = smw_us_v1_fastrom_patch_plan(
            project.rom.logical_bytes(),
            AllocationPolicy::lorom(0x8_7e6f..logical_len),
            crate::SMW_US_V1_CHECKSUM_FIELD,
        )
        .unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].header_offset, 0x87e6f);
        assert_eq!(
            &project.rom.logical_bytes()[result.blocks[0].payload.clone()],
            plan.payloads[0].bytes
        );
        let reopened = lm_rom::RomImage::from_bytes(project.save_snapshot()).unwrap();
        assert_eq!(
            detect_smw_us_v1_fastrom_patch(reopened.logical_bytes()),
            Ok(SmwUsV1FastRomPatchState::Installed)
        );
        assert_eq!(reopened.logical_bytes()[MAP_MODE_OFFSET], 0x30);
        assert_eq!(
            reopened.logical_bytes()[PATCH_MARKER_OFFSET],
            lm_rom::LunarMagicRomMetadata::FASTROM_MARKER
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
        let restored = lm_rom::RomImage::from_bytes(original).unwrap();
        assert_eq!(
            detect_smw_us_v1_fastrom_patch(restored.logical_bytes()),
            Ok(SmwUsV1FastRomPatchState::Absent)
        );
    }

    #[test]
    fn detector_rejects_every_owned_runtime_region() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            std::fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                .unwrap();
        let mut project =
            lm_project::Project::open_supported(lm_rom::RomImage::from_bytes(original).unwrap())
                .unwrap();
        let len = project.rom.logical_len();
        let plan = smw_us_v1_fastrom_patch_plan(
            project.rom.logical_bytes(),
            AllocationPolicy::lorom(0x8_7e6f..len),
            crate::SMW_US_V1_CHECKSUM_FIELD,
        )
        .unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();
        for offset in [
            MAP_MODE_OFFSET,
            FIRST_HOOK_WORD_OFFSET,
            SECOND_HOOK_WORD_OFFSET,
            TRAMPOLINE_OFFSET,
            PATCH_MARKER_OFFSET,
            result.blocks[0].payload.start,
        ] {
            let mut corrupt = project.rom.logical_bytes().to_vec();
            corrupt[offset] ^= 1;
            assert!(detect_smw_us_v1_fastrom_patch(&corrupt).is_err());
        }
    }
}
