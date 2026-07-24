//! Independently verified Lunar Magic 3.63 secondary-exit reader fragments.
//!
//! The complete pristine installation also depends on the shared Lfix3 hook network. These typed
//! fragments keep relocatable ROM pointers distinct from fixed WRAM operands while that larger
//! dependency graph is recovered.

use lm_rom::{Mapper, RomError, pc_to_snes};

pub const SMW_US_V1_SECONDARY_EXIT_FIRST_READER_LEN: usize = 0x20;
pub const SMW_US_V1_SECONDARY_EXIT_SECOND_READER_LEN: usize = 0x50;
pub const SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT_LEN: usize = 0x30;
pub const SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT_LEN: usize = 0x20;

pub const SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT: [u8; 0x30] = [
    0xbd, 0xd8, 0x19, 0x89, 0x04, 0xf0, 0x1a, 0x48, 0x48, 0x29, 0x02, 0x4a, 0x8d, 0x93, 0x1b, 0x68,
    0x29, 0x08, 0x0a, 0x0a, 0x0a, 0x8d, 0x2a, 0x19, 0x68, 0x4a, 0x08, 0x4a, 0x4a, 0x4a, 0x28, 0x2a,
    0x6b, 0x9c, 0x2a, 0x19, 0xad, 0xbf, 0x13, 0xc9, 0x25, 0xa9, 0x00, 0x2a, 0x6b, 0xff, 0xff, 0xff,
];

pub const SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT: [u8; 0x20] = [
    0xa5, 0x0a, 0x29, 0x1f, 0xaa, 0xc2, 0x20, 0xa7, 0x65, 0xe6, 0x65, 0xe6, 0x65, 0xe2, 0x20, 0x9d,
    0xb8, 0x19, 0xeb, 0x9d, 0xd8, 0x19, 0x60, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x00, 0x01,
];

#[must_use]
pub const fn smw_us_v1_secondary_exit_first_reader() -> [u8; 0x20] {
    let mut bytes = [0xff; 0x20];
    let code = [
        0xbf, 0x00, 0xf8, 0x05, 0x85, 0x0e, 0x6b, 0xbf, 0x00, 0xfa, 0x05, 0x85, 0x00, 0x6b, 0xbf,
        0x00, 0xfc, 0x05, 0x85, 0x01, 0x6b,
    ];
    let mut index = 0;
    while index < code.len() {
        bytes[index] = code[index];
        index += 1;
    }
    bytes[0x1c] = 0x4c;
    bytes[0x1d] = 0x4d;
    bytes[0x1e] = 0x00;
    bytes[0x1f] = 0x01;
    bytes
}

/// Builds the second reader with the two RATS-owned plane payload addresses.
///
/// # Errors
///
/// Returns an address-mapping error for an unrepresentable `LoROM` payload offset.
pub fn smw_us_v1_secondary_exit_second_reader(
    plane_four: usize,
    plane_five: usize,
) -> Result<[u8; 0x50], RomError> {
    let mut bytes = [0xff; 0x50];
    bytes[..15].copy_from_slice(&[
        0xbf, 0x00, 0xfe, 0x05, 0x6b, 0xbf, 0x00, 0x88, 0x00, 0x6b, 0xbf, 0x00, 0x8a, 0x00, 0x6b,
    ]);
    bytes[0x30..0x3e].copy_from_slice(&[
        0xf0, 0x0b, 0xc9, 0x05, 0xb0, 0x07, 0x3a, 0x6d, 0xea, 0x1d, 0x8d, 0xea, 0x1d, 0x6b,
    ]);
    bytes[0x4c..].copy_from_slice(&[0x4c, 0x4d, 0x10, 0x01]);
    write_low_bank_pointer(&mut bytes[6..9], plane_four)?;
    write_low_bank_pointer(&mut bytes[11..14], plane_five)?;
    Ok(bytes)
}

fn write_low_bank_pointer(output: &mut [u8], logical_offset: usize) -> Result<(), RomError> {
    let address = pc_to_snes(Mapper::LoRom, logical_offset)? & 0x7f_ffff;
    output.copy_from_slice(&address.to_le_bytes()[..3]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SMW_US_V1_SECONDARY_EXIT_FIRST_READER, SMW_US_V1_SECONDARY_EXIT_SECOND_READER,
        smw_us_v1_secondary_exit_locator,
    };
    use lm_project::{Project, SecondaryExitStorage};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn builders_match_four_independently_relocated_lm363_fixtures() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for fixture in [
            "level-save-000",
            "level-save-105",
            "palette-install-positive",
            "exanimation-install-positive",
        ] {
            let bytes =
                fs::read(root.join(format!("oracle-work/lm363/pristine-us/{fixture}/after.smc")))
                    .unwrap();
            let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
            let loaded = project
                .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
                .unwrap();
            let SecondaryExitStorage::Installed { tagged_planes, .. } = loaded.storage else {
                panic!("fixture did not contain installed secondary exits");
            };
            assert_eq!(
                project
                    .rom
                    .read(SMW_US_V1_SECONDARY_EXIT_FIRST_READER, 0x20)
                    .unwrap(),
                smw_us_v1_secondary_exit_first_reader()
            );
            assert_eq!(
                project
                    .rom
                    .read(SMW_US_V1_SECONDARY_EXIT_SECOND_READER, 0x50)
                    .unwrap(),
                smw_us_v1_secondary_exit_second_reader(
                    tagged_planes[0].payload.start,
                    tagged_planes[1].payload.start,
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn fixed_support_fragments_match_the_real_installation() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            image.read(0x0002_dc50, 0x30).unwrap(),
            SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT
        );
        assert_eq!(
            image.read(0x0006_e1b0, 0x20).unwrap(),
            SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT
        );
    }
}
