//! Authenticated Lunar Magic graphics-compression runtime detection for SMW US revision 0.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rats::{HEADER_LEN, HeaderError, parse_at};
use lm_rom::{Mapper, RomError, RomImage, snes_to_pc};

pub const SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET: usize = 0x07_ffeb;
pub const SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET: usize = 0x0038_e3;

const ORIGINAL_HOOK: [u8; 5] = [0x20, 0x83, 0xb9, 0xc9, 0xff];
const INSTALLED_OPCODE: u8 = 0x22;
const INSTALLED_RETURN: u8 = 0x60;
const SPEED_RUNTIME_LEN: usize = 0x1c0;
const LZ3_RUNTIME_LEN: usize = 0x2ab;
const SPEED_RUNTIME_CRC32: u32 = 0x5d3c_ac46;
const LZ3_RUNTIME_CRC32: u32 = 0xdcb7_727e;
const RUNTIME_TRAILER: [u8; 4] = *b"LM\x01\x01";
const SPEED_RUNTIME_HEX: &str = include_str!("assets/graphics_compression_lz2_speed.hex");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1GraphicsCompressionMode {
    Lz2Original,
    Lz2Speed,
    Lz3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1GraphicsCompressionDetectError {
    Truncated {
        offset: usize,
        len: usize,
    },
    UnknownMetadata(u8),
    OriginalHookMismatch([u8; 5]),
    InstalledHookMismatch([u8; 5]),
    RuntimeAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership {
        expected: usize,
        actual: usize,
    },
    RuntimeLength {
        expected: usize,
        actual: usize,
    },
    RuntimeTrailer,
    RuntimeChecksum {
        expected: u32,
        actual: u32,
    },
    SourceMode {
        expected: SmwUsV1GraphicsCompressionMode,
        actual: SmwUsV1GraphicsCompressionMode,
    },
}

impl std::fmt::Display for SmwUsV1GraphicsCompressionDetectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot authenticate SMW-US graphics compression runtime: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1GraphicsCompressionDetectError {}

/// Detects the exact original, fast-LZ2, or LZ3 runtime selected by Lunar Magic metadata.
///
/// Installed modes require agreement between the metadata nibble, the fixed JSL/RTS hook, exact
/// RATS ownership, payload length, runtime trailer, and a checksum over the complete immutable
/// runtime. This prevents a profile from selecting a decoder based on the metadata byte alone.
pub fn detect_smw_us_v1_graphics_compression_mode(
    image: &RomImage,
) -> Result<SmwUsV1GraphicsCompressionMode, SmwUsV1GraphicsCompressionDetectError> {
    let bytes = image.logical_bytes();
    let metadata = *bytes
        .get(SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET)
        .ok_or(SmwUsV1GraphicsCompressionDetectError::Truncated {
            offset: SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET,
            len: 1,
        })?;
    let hook = read_array::<5>(bytes, SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET)?;
    let mode = match metadata & 0x0f {
        0 => SmwUsV1GraphicsCompressionMode::Lz2Original,
        1 => SmwUsV1GraphicsCompressionMode::Lz2Speed,
        2 => SmwUsV1GraphicsCompressionMode::Lz3,
        value => {
            return Err(SmwUsV1GraphicsCompressionDetectError::UnknownMetadata(
                value,
            ));
        }
    };
    if mode == SmwUsV1GraphicsCompressionMode::Lz2Original {
        return (hook == ORIGINAL_HOOK).then_some(mode).ok_or(
            SmwUsV1GraphicsCompressionDetectError::OriginalHookMismatch(hook),
        );
    }
    if hook[0] != INSTALLED_OPCODE || hook[4] != INSTALLED_RETURN {
        return Err(SmwUsV1GraphicsCompressionDetectError::InstalledHookMismatch(hook));
    }
    let runtime_offset = snes_to_pc(
        Mapper::LoRom,
        u32::from_le_bytes([hook[1], hook[2], hook[3], 0]),
    )
    .map_err(SmwUsV1GraphicsCompressionDetectError::RuntimeAddress)?;
    let header_offset = runtime_offset.checked_sub(HEADER_LEN).ok_or(
        SmwUsV1GraphicsCompressionDetectError::RuntimeBeforeHeader(runtime_offset),
    )?;
    let block = parse_at(bytes, header_offset)
        .map_err(SmwUsV1GraphicsCompressionDetectError::RuntimeHeader)?;
    if block.payload.start != runtime_offset {
        return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeOwnership {
            expected: runtime_offset,
            actual: block.payload.start,
        });
    }
    let (expected_len, expected_crc) = match mode {
        SmwUsV1GraphicsCompressionMode::Lz2Speed => (SPEED_RUNTIME_LEN, SPEED_RUNTIME_CRC32),
        SmwUsV1GraphicsCompressionMode::Lz3 => (LZ3_RUNTIME_LEN, LZ3_RUNTIME_CRC32),
        SmwUsV1GraphicsCompressionMode::Lz2Original => unreachable!(),
    };
    if block.payload.len() != expected_len {
        return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeLength {
            expected: expected_len,
            actual: block.payload.len(),
        });
    }
    let runtime = &bytes[block.payload];
    if !runtime.ends_with(&RUNTIME_TRAILER) {
        return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeTrailer);
    }
    let actual_crc = crc32(runtime);
    if actual_crc != expected_crc {
        return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeChecksum {
            expected: expected_crc,
            actual: actual_crc,
        });
    }
    Ok(mode)
}

/// Builds Lunar Magic's failure-atomic `LZ2 Orig` to `LZ2 Speed` runtime-only conversion.
///
/// Both modes use identical LZ2 payloads, so no graphics or dependent table is recompressed. The
/// transaction installs the authenticated fast decompressor, binds the fixed JSL to its allocated
/// payload, changes only the metadata low nibble, and repairs the checksum.
pub fn smw_us_v1_lz2_speed_installation_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionDetectError> {
    let mode = detect_smw_us_v1_graphics_compression_mode(image)?;
    if mode != SmwUsV1GraphicsCompressionMode::Lz2Original {
        return Err(SmwUsV1GraphicsCompressionDetectError::SourceMode {
            expected: SmwUsV1GraphicsCompressionMode::Lz2Original,
            actual: mode,
        });
    }
    let metadata = image.logical_bytes()[SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET];
    let runtime = decode_hex(SPEED_RUNTIME_HEX);
    debug_assert_eq!(runtime.len(), SPEED_RUNTIME_LEN);
    debug_assert_eq!(crc32(&runtime), SPEED_RUNTIME_CRC32);
    Ok(RelocatablePatchPlan {
        description: "change SMW graphics compression from LZ2 Orig to LZ2 Speed".into(),
        mapper: Mapper::LoRom,
        allocation,
        checksum_field,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: runtime,
            fixups: vec![],
        }],
        writes: vec![
            PatchWrite {
                offset: SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET,
                expected: ORIGINAL_HOOK.to_vec(),
                replacement: vec![INSTALLED_OPCODE, 0, 0, 0, INSTALLED_RETURN],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: PatchFixupEncoding::Long24LowBank,
                }],
            },
            PatchWrite {
                offset: SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET,
                expected: vec![metadata],
                replacement: vec![(metadata & 0xf0) | 1],
                fixups: vec![],
            },
        ],
    })
}

fn decode_hex(source: &str) -> Vec<u8> {
    let digits = source
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
        _ => unreachable!("embedded runtime is lowercase hexadecimal"),
    }
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
) -> Result<[u8; N], SmwUsV1GraphicsCompressionDetectError> {
    bytes
        .get(offset..offset.saturating_add(N))
        .and_then(|slice| slice.try_into().ok())
        .ok_or(SmwUsV1GraphicsCompressionDetectError::Truncated { offset, len: N })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut value = 0xffff_ffff_u32;
    for &byte in bytes {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(value & 1));
        }
    }
    !value
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use std::fs;

    #[test]
    fn original_mode_requires_metadata_and_the_complete_fixed_hook() {
        let mut image = RomImage::from_bytes(vec![0xff; 0x80_000]).unwrap();
        image
            .write(SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET, &[0])
            .unwrap();
        image
            .write(SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET, &ORIGINAL_HOOK)
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&image).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Original
        );
        image
            .write(SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET + 2, &[0])
            .unwrap();
        assert!(matches!(
            detect_smw_us_v1_graphics_compression_mode(&image),
            Err(SmwUsV1GraphicsCompressionDetectError::OriginalHookMismatch(
                _
            ))
        ));
    }

    #[test]
    fn metadata_never_selects_an_unrecognized_decoder() {
        let mut image = RomImage::from_bytes(vec![0xff; 0x80_000]).unwrap();
        image
            .write(SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET, &[3])
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&image),
            Err(SmwUsV1GraphicsCompressionDetectError::UnknownMetadata(3))
        );
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 compression-conversion ROMs"]
    fn retained_lunar_magic_modes_authenticate_and_corruption_rejects() {
        for (variable, expected) in [
            (
                "LM_LZ2_ORIGINAL_ROM",
                SmwUsV1GraphicsCompressionMode::Lz2Original,
            ),
            ("LM_LZ2_SPEED_ROM", SmwUsV1GraphicsCompressionMode::Lz2Speed),
            ("LM_LZ3_ROM", SmwUsV1GraphicsCompressionMode::Lz3),
        ] {
            let path = std::env::var_os(variable).expect(variable);
            let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
            assert_eq!(
                detect_smw_us_v1_graphics_compression_mode(&image).unwrap(),
                expected
            );
            if expected != SmwUsV1GraphicsCompressionMode::Lz2Original {
                let hook = image
                    .read(SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET, 5)
                    .unwrap();
                let runtime = snes_to_pc(
                    Mapper::LoRom,
                    u32::from_le_bytes([hook[1], hook[2], hook[3], 0]),
                )
                .unwrap();
                let mut corrupt = image.clone();
                let changed = corrupt.read(runtime + 1, 1).unwrap()[0] ^ 1;
                corrupt.write(runtime + 1, &[changed]).unwrap();
                assert!(matches!(
                    detect_smw_us_v1_graphics_compression_mode(&corrupt),
                    Err(SmwUsV1GraphicsCompressionDetectError::RuntimeChecksum { .. })
                ));
            }
        }
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 original/speed conversion ROMs"]
    fn rust_lz2_speed_install_matches_lunar_magic_except_canonical_checksum_fields() {
        let original = fs::read(std::env::var_os("LM_LZ2_ORIGINAL_ROM").unwrap()).unwrap();
        let expected = fs::read(std::env::var_os("LM_LZ2_SPEED_ROM").unwrap()).unwrap();
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let plan = smw_us_v1_lz2_speed_installation_plan(
            &image,
            AllocationPolicy::lorom(0x80028..image.logical_len()),
            0x7fdc,
        )
        .unwrap();
        let mut project = Project::new(image);
        project.install_relocatable_patch(&plan).unwrap();
        if let Some(path) = std::env::var_os("LM_LZ2_SPEED_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        // Lunar Magic preserves the prior checksum through its optional compensation word; the
        // Rust transaction canonicalizes the checksum fields instead. All executable/runtime and
        // metadata bytes are otherwise byte-exact.
        assert_eq!(
            differing_ranges(project.rom.as_file_bytes(), &expected),
            [0x81dc..0x81e0, 0x7f1b7..0x7f1b9]
        );
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Speed
        );
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), original);
    }

    fn differing_ranges(left: &[u8], right: &[u8]) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let mut start = None;
        for index in 0..left.len().max(right.len()) {
            if left.get(index) != right.get(index) {
                start.get_or_insert(index);
            } else if let Some(begin) = start.take() {
                ranges.push(begin..index);
            }
        }
        if let Some(begin) = start {
            ranges.push(begin..left.len().max(right.len()));
        }
        ranges
    }
}
