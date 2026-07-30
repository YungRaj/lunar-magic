//! Authenticated pristine SMW-US installation of Lunar Magic's complete Map16 runtime.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{IpsError, Mapper, apply_ips};
use std::{fmt, io::Read};

const FIXED_PATCH_BASE64: &str = include_str!("assets/map16_runtime_fixed.ips.b64");
const AUXILIARY_PAYLOAD_GZIP_BASE64: &str = include_str!("assets/map16_auxiliary.bin.gz.b64");
const PRISTINE_LOGICAL_LEN: usize = 0x80_000;
const AUXILIARY_PAYLOAD_LEN: usize = 0x8000;
const AUXILIARY_BANK_OPERAND: usize = 0x37_626;

#[derive(Debug)]
pub enum SmwUsV1Map16RuntimeInstallBuildError {
    PristineLength(usize),
    InvalidEmbeddedBase64,
    AuxiliaryIo(std::io::Error),
    AuxiliaryLength(usize),
    Ips(IpsError),
    PatchedLength(usize),
    MissingAuxiliaryBankOperand,
}

impl fmt::Display for SmwUsV1Map16RuntimeInstallBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot build SMW-US Map16 runtime installation: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Map16RuntimeInstallBuildError {}

impl From<IpsError> for SmwUsV1Map16RuntimeInstallBuildError {
    fn from(value: IpsError) -> Self {
        Self::Ips(value)
    }
}

/// Builds the exact Lunar Magic 3.63 runtime transformation recovered from an unchanged complete
/// Map16 import into authenticated pristine SMW-US.
///
/// The bundled IPS data describes only fixed revision bytes within the original 512 KiB image.
/// The `$8000`-byte auxiliary table is allocated independently, and its one varying bank operand
/// is emitted as a typed relocation. The caller remains responsible for authenticating `pristine`
/// as SMW-US revision 0 before offering this operation.
///
/// # Errors
///
/// Rejects a non-pristine-sized source, malformed embedded patch data, an unexpected patched
/// length, or loss of the recovered auxiliary relocation site.
pub fn smw_us_v1_map16_runtime_installation_plan(
    pristine: &[u8],
    mut allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1Map16RuntimeInstallBuildError> {
    if pristine.len() != PRISTINE_LOGICAL_LEN {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::PristineLength(
            pristine.len(),
        ));
    }
    let patch = decode_base64(FIXED_PATCH_BASE64)?;
    let patched = apply_ips(pristine, &patch)?;
    if patched.len() != pristine.len() {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::PatchedLength(
            patched.len(),
        ));
    }
    let auxiliary = decode_auxiliary_payload()?;
    let writes = changed_patch_writes(pristine, &patched, checksum_field)?;
    // Lunar Magic places the eight-byte RATS header immediately before a complete `$8000`-byte
    // payload bank. The generic allocator's bank constraint includes the header, so reserve the
    // preceding partial bank explicitly and allocate this one exceptional cross-boundary block
    // without its ordinary same-bank rule.
    let first_payload = allocation.search.start + 0x8000;
    allocation.bank_size = None;
    allocation.protected.push(ProtectedRange(
        allocation.search.start..first_payload - lm_rats::HEADER_LEN,
    ));
    Ok(RelocatablePatchPlan {
        description: "Install Lunar Magic Map16 runtime".into(),
        mapper: Mapper::LoRom,
        allocation,
        checksum_field,
        expansion_fill: 0,
        payloads: vec![PatchPayload {
            bytes: auxiliary,
            fixups: Vec::new(),
        }],
        writes,
    })
}

fn decode_auxiliary_payload() -> Result<Vec<u8>, SmwUsV1Map16RuntimeInstallBuildError> {
    let compressed = decode_base64(AUXILIARY_PAYLOAD_GZIP_BASE64)?;
    let mut decoder = flate2::read::GzDecoder::new(compressed.as_slice()).take(32_769);
    let mut payload = Vec::with_capacity(AUXILIARY_PAYLOAD_LEN);
    decoder
        .read_to_end(&mut payload)
        .map_err(SmwUsV1Map16RuntimeInstallBuildError::AuxiliaryIo)?;
    if payload.len() != AUXILIARY_PAYLOAD_LEN {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::AuxiliaryLength(
            payload.len(),
        ));
    }
    Ok(payload)
}

fn changed_patch_writes(
    pristine: &[u8],
    patched: &[u8],
    checksum_field: usize,
) -> Result<Vec<PatchWrite>, SmwUsV1Map16RuntimeInstallBuildError> {
    let checksum = checksum_field..checksum_field + 4;
    let mut writes = Vec::new();
    let mut cursor = 0;
    let mut found_auxiliary_fixup = false;
    while cursor < pristine.len() {
        if pristine[cursor] == patched[cursor] || checksum.contains(&cursor) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < pristine.len()
            && pristine[cursor] != patched[cursor]
            && !checksum.contains(&cursor)
        {
            cursor += 1;
        }
        let mut fixups = Vec::new();
        if (start..cursor).contains(&AUXILIARY_BANK_OPERAND) {
            found_auxiliary_fixup = true;
            fixups.push(PatchFixup {
                offset: AUXILIARY_BANK_OPERAND - start,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Bank8LowBank,
            });
        }
        writes.push(PatchWrite {
            offset: start,
            expected: pristine[start..cursor].to_vec(),
            replacement: patched[start..cursor].to_vec(),
            fixups,
        });
    }
    if !found_auxiliary_fixup {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::MissingAuxiliaryBankOperand);
    }
    Ok(writes)
}

fn decode_base64(text: &str) -> Result<Vec<u8>, SmwUsV1Map16RuntimeInstallBuildError> {
    let symbols = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if symbols.is_empty() || symbols.len() % 4 != 0 {
        return Err(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64);
    }
    let mut decoded = Vec::with_capacity(symbols.len() / 4 * 3);
    for quartet in symbols.chunks_exact(4) {
        let padding = usize::from(quartet[3] == b'=') + usize::from(quartet[2] == b'=');
        let mut value = 0_u32;
        for (index, symbol) in quartet.iter().copied().enumerate() {
            let sextet = if symbol == b'=' {
                if index < 2 {
                    return Err(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64);
                }
                0
            } else {
                u32::from(
                    base64_sextet(symbol)
                        .ok_or(SmwUsV1Map16RuntimeInstallBuildError::InvalidEmbeddedBase64)?,
                )
            };
            value = value << 6 | sextet;
        }
        let bytes = value.to_be_bytes();
        decoded.extend_from_slice(&bytes[1..4 - padding]);
    }
    Ok(decoded)
}

const fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum, snes_to_pc};

    fn allocation() -> AllocationPolicy {
        AllocationPolicy {
            search: 0x80_000..0x10_0000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0, 0xff],
            protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
        }
    }

    #[test]
    fn embedded_patch_decodes_and_installs_the_relocated_wine_shape() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let plan =
            smw_us_v1_map16_runtime_installation_plan(&original, allocation(), 0x7fdc).unwrap();
        assert_eq!(plan.payloads[0].bytes.len(), 0x8000);
        let relocated = plan
            .writes
            .iter()
            .filter_map(|write| write.fixups.first().map(|fixup| (write, fixup)))
            .collect::<Vec<_>>();
        assert_eq!(relocated.len(), 1);
        assert_eq!(relocated[0].0.offset + relocated[0].1.offset, 0x37_626);
        assert_eq!(relocated[0].1.target_payload, 0);
        assert_eq!(relocated[0].1.encoding, PatchFixupEncoding::Bank8LowBank);

        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks[0].payload, 0x88_000..0x90_000);
        assert_eq!(
            snes_to_pc(Mapper::LoRom, result.snes_addresses[0]).unwrap(),
            0x88_000
        );
        assert_eq!(
            project.rom.logical_bytes()[super::super::native_map16_secondary::SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET],
            0x22
        );
        let secondary =
            super::super::native_map16_secondary::load_smw_us_v1_secondary_map16(&project).unwrap();
        assert!(secondary.installed);
        assert!(secondary.blocks.iter().all(Option::is_none));
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            checksum.encoded()
        );
    }

    #[test]
    fn malformed_source_is_rejected_before_a_plan_exists() {
        assert!(matches!(
            smw_us_v1_map16_runtime_installation_plan(&[0; 1], allocation(), 0x7fdc),
            Err(SmwUsV1Map16RuntimeInstallBuildError::PristineLength(1))
        ));
    }
}
