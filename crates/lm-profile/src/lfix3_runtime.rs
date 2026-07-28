//! Relocation metadata for Lunar Magic's shared `$510`-byte Lfix3 runtime.
//!
//! The executable stores this body in a pre-relocation form. Most operands in its two dense tables
//! are offsets from the eventual payload, while a small set of mapper-sensitive instructions is
//! rewritten before allocation. Keeping that distinction explicit is required before the runtime
//! can participate in the complete pristine secondary-exit installation plan.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload};

pub const SMW_US_V1_LFIX3_RUNTIME_LEN: usize = 0x510;
const EMBEDDED_TEMPLATE_HEX: &str = include_str!("assets/lfix3_runtime.hex");

const SELF_LONG24: [(usize, usize); 2] = [(0x33, 0x233), (0x47, 0x233)];
const SELF_LOW16_SINGLES: [usize; 9] = [
    0x11a, 0x129, 0x17e, 0x18d, 0x192, 0x1d5, 0x1ea, 0x30a, 0x318,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lfix3RuntimeLengthError {
    pub actual: usize,
}

impl std::fmt::Display for Lfix3RuntimeLengthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Lfix3 runtime template has length {:#x}, expected {:#x}",
            self.actual, SMW_US_V1_LFIX3_RUNTIME_LEN
        )
    }
}

impl std::error::Error for Lfix3RuntimeLengthError {}

/// Returns the recovered pre-relocation runtime template bundled with this revision profile.
#[must_use]
pub fn smw_us_v1_lfix3_runtime_template() -> Vec<u8> {
    let digits = EMBEDDED_TEMPLATE_HEX
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    digits
        .chunks_exact(2)
        .map(|pair| (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]))
        .collect()
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("embedded template is lowercase hexadecimal"),
    }
}

/// Converts the recovered pre-relocation body into a relocatable patch payload.
///
/// The returned fixups all target payload zero. Callers combining this body with preceding
/// payloads must rebase `target_payload` in the same way as other profile components.
///
/// # Errors
///
/// Rejects a template whose length is not exactly `$510`.
pub fn smw_us_v1_lfix3_runtime_payload(
    template: &[u8],
) -> Result<PatchPayload, Lfix3RuntimeLengthError> {
    if template.len() != SMW_US_V1_LFIX3_RUNTIME_LEN {
        return Err(Lfix3RuntimeLengthError {
            actual: template.len(),
        });
    }
    let mut bytes = template.to_vec();
    // LoROM form selected by Lunar Magic's mapper-aware preprocessor.
    bytes[0x5b..0x5e].copy_from_slice(&[0xad, 0xd7, 0x13]);
    bytes[0x109..0x10c].copy_from_slice(&[0xad, 0xd7, 0x13]);
    for offset in [0x65, 0xc2, 0xfa, 0x113] {
        bytes[offset] = 0x50;
    }

    let mut fixups = Vec::with_capacity(2 + SELF_LOW16_SINGLES.len() + 32 + 64);
    for (offset, target_addend) in SELF_LONG24 {
        fixups.push(PatchFixup {
            offset,
            target_payload: 0,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        });
    }
    for offset in SELF_LOW16_SINGLES
        .into_iter()
        .chain((0x1f3..0x233).step_by(2))
        .chain((0x38b..0x40b).step_by(2))
    {
        let encoded_addend =
            usize::from(u16::from_le_bytes([template[offset], template[offset + 1]]));
        // Descriptor-selected aliases in Lunar Magic's preprocessing table.
        let target_addend = match offset {
            0x39b => 0x1c3,
            0x3db => 0x1d7,
            _ => encoded_addend,
        };
        fixups.push(PatchFixup {
            offset,
            target_payload: 0,
            target_addend,
            encoding: PatchFixupEncoding::Low16,
        });
    }
    Ok(PatchPayload { bytes, fixups })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchPlan};
    use lm_rats::AllocationPolicy;
    use lm_rom::{Mapper, RomImage};
    use std::{fs, path::PathBuf};

    #[test]
    fn recovered_relocations_reproduce_real_lm363_payload_bytes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, SMW_US_V1_LFIX3_RUNTIME_LEN);
        let payload = smw_us_v1_lfix3_runtime_payload(template).unwrap();
        assert_eq!(payload.fixups.len(), 107);

        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let plan = RelocatablePatchPlan {
            description: "test Lfix3 relocation".into(),
            mapper: Mapper::LoRom,
            allocation: AllocationPolicy::lorom(0x0008_0029..0x0010_0000),
            checksum_field: 0x7fdc,
            expansion_fill: 0xff,
            payloads: vec![payload],
            writes: Vec::new(),
        };
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks[0].payload.start, 0x0008_0031);

        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let fixture = RomImage::from_bytes(fixture).unwrap();
        assert_eq!(
            project
                .rom
                .read(0x0008_0031, SMW_US_V1_LFIX3_RUNTIME_LEN)
                .unwrap(),
            fixture
                .read(0x0008_0031, SMW_US_V1_LFIX3_RUNTIME_LEN)
                .unwrap()
        );
    }

    #[test]
    fn wrong_template_length_is_rejected_before_indexing() {
        assert_eq!(
            smw_us_v1_lfix3_runtime_payload(&[0; 4]).unwrap_err(),
            Lfix3RuntimeLengthError { actual: 4 }
        );
    }

    #[test]
    fn bundled_template_is_exactly_the_supplied_lm363_resource() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        assert_eq!(
            smw_us_v1_lfix3_runtime_template(),
            pe_rva(&executable, 0x1b_7f78, SMW_US_V1_LFIX3_RUNTIME_LEN)
        );
    }

    fn pe_rva(image: &[u8], rva: usize, len: usize) -> &[u8] {
        let pe =
            usize::try_from(u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap())).unwrap();
        let section_count = usize::from(u16::from_le_bytes(
            image[pe + 6..pe + 8].try_into().unwrap(),
        ));
        let optional_len = usize::from(u16::from_le_bytes(
            image[pe + 20..pe + 22].try_into().unwrap(),
        ));
        let sections = pe + 24 + optional_len;
        for index in 0..section_count {
            let entry = sections + index * 40;
            let virtual_size = usize::try_from(u32::from_le_bytes(
                image[entry + 8..entry + 12].try_into().unwrap(),
            ))
            .unwrap();
            let virtual_address = usize::try_from(u32::from_le_bytes(
                image[entry + 12..entry + 16].try_into().unwrap(),
            ))
            .unwrap();
            let raw = usize::try_from(u32::from_le_bytes(
                image[entry + 20..entry + 24].try_into().unwrap(),
            ))
            .unwrap();
            if (virtual_address..virtual_address + virtual_size).contains(&rva) {
                let start = raw + rva - virtual_address;
                return &image[start..start + len];
            }
        }
        panic!("RVA not present in PE sections");
    }
}
