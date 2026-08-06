//! Authenticated Lunar Magic graphics-compression runtime detection for SMW US revision 0.

use lm_codec::encode_lz3;
use lm_project::{
    GraphicsCompression, GraphicsIoError, PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite,
    Project, RelocatablePatchPlan,
};
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
const LZ3_RUNTIME_HEX: &str = include_str!("assets/graphics_compression_lz3.hex");

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

#[derive(Debug)]
pub enum SmwUsV1GraphicsCompressionMigrationError {
    Detect(SmwUsV1GraphicsCompressionDetectError),
    Graphics(GraphicsIoError),
    Special(crate::SmwUsV1SpecialGraphicsLayoutError),
    SpecialGraphicsAllocationPolicy,
}

impl std::fmt::Display for SmwUsV1GraphicsCompressionMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot migrate SMW-US graphics compression: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1GraphicsCompressionMigrationError {}

impl From<SmwUsV1GraphicsCompressionDetectError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: SmwUsV1GraphicsCompressionDetectError) -> Self {
        Self::Detect(value)
    }
}

impl From<GraphicsIoError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: GraphicsIoError) -> Self {
        Self::Graphics(value)
    }
}

impl From<crate::SmwUsV1SpecialGraphicsLayoutError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: crate::SmwUsV1SpecialGraphicsLayoutError) -> Self {
        Self::Special(value)
    }
}

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

/// Builds the standard-GFX and decoder portion of Lunar Magic's LZ3 conversion.
///
/// This component deliberately has no application command of its own: ExGFX, ExAnimation, and
/// installed overworld-event streams must join it before the complete conversion is exposed. It
/// preserves all 50 ordinary files plus the shared-bank GFX33/GFX32 startup pair, rewrites every
/// split pointer plane, installs the authenticated LZ3 decoder, and updates the metadata nibble.
pub fn smw_us_v1_standard_gfx_lz3_installation_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionMigrationError> {
    let source_mode = detect_smw_us_v1_graphics_compression_mode(image)?;
    if source_mode == SmwUsV1GraphicsCompressionMode::Lz3 {
        return Err(SmwUsV1GraphicsCompressionDetectError::SourceMode {
            expected: SmwUsV1GraphicsCompressionMode::Lz2Original,
            actual: source_mode,
        }
        .into());
    }
    let project = Project::new(image.clone());
    let mut ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout();
    ordinary_layout.compression = GraphicsCompression::Lz2;
    let ordinary = (0..ordinary_layout.pointers.entries)
        .map(|slot| {
            let raw = project
                .load_graphics_file(slot, ordinary_layout)?
                .encode()
                .map_err(GraphicsIoError::from)?;
            Ok(encode_lz3(&raw))
        })
        .collect::<Result<Vec<_>, SmwUsV1GraphicsCompressionMigrationError>>()?;
    let mut special = crate::smw_us_v1_special_graphics_layouts(image)?;
    special.gfx33.compression = GraphicsCompression::Lz2;
    special.gfx32.compression = GraphicsCompression::Lz2;
    let gfx33 = encode_lz3(
        &project
            .load_graphics_file(0, special.gfx33)?
            .encode()
            .map_err(GraphicsIoError::from)?,
    );
    let gfx32 = encode_lz3(
        &project
            .load_graphics_file(0, special.gfx32)?
            .encode()
            .map_err(GraphicsIoError::from)?,
    );
    let runtime = decode_hex(LZ3_RUNTIME_HEX);
    debug_assert_eq!(runtime.len(), LZ3_RUNTIME_LEN);
    debug_assert_eq!(crc32(&runtime), LZ3_RUNTIME_CRC32);
    let first_bank_end = allocation
        .search
        .start
        .checked_add(0x8000)
        .ok_or(SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy)?;
    let image_len = image.logical_len();
    let occupied_first_bank = allocation.search.start.min(image_len)..first_bank_end.min(image_len);
    if allocation.bank_size != Some(0x8000)
        || allocation.search.start & 0x7fff != 0
        || allocation.search.end < first_bank_end
        || LZ3_RUNTIME_LEN + gfx33.len() + gfx32.len() + 3 * HEADER_LEN > 0x8000
        || image.logical_bytes()[occupied_first_bank]
            .iter()
            .any(|byte| !allocation.fill_bytes.contains(byte))
    {
        return Err(SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy);
    }
    let mut payloads = Vec::with_capacity(53);
    payloads.push(PatchPayload {
        bytes: runtime,
        fixups: Vec::new(),
    });
    let gfx33_payload_index = payloads.len();
    payloads.push(PatchPayload {
        bytes: gfx33,
        fixups: Vec::new(),
    });
    let gfx32_payload_index = payloads.len();
    payloads.push(PatchPayload {
        bytes: gfx32,
        fixups: Vec::new(),
    });
    payloads.extend(ordinary.into_iter().map(|bytes| PatchPayload {
        bytes,
        fixups: Vec::new(),
    }));

    let bytes = image.logical_bytes();
    let mut writes = Vec::with_capacity(ordinary_layout.pointers.entries * 3 + 6);
    for slot in 0..ordinary_layout.pointers.entries {
        let target_payload = 3 + slot;
        for (offset, encoding) in [
            (
                crate::SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET + slot,
                PatchFixupEncoding::Low8,
            ),
            (
                crate::SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET + slot,
                PatchFixupEncoding::High8,
            ),
            (
                crate::SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET + slot,
                PatchFixupEncoding::Bank8LowBank,
            ),
        ] {
            writes.push(pointer_write(bytes, offset, target_payload, 0, encoding));
        }
    }
    writes.extend([
        pointer_write(
            bytes,
            crate::SMW_US_V1_GFX33_STARTUP_POINTER_LOW_OFFSET,
            gfx33_payload_index,
            0,
            PatchFixupEncoding::Low16,
        ),
        pointer_write(
            bytes,
            crate::SMW_US_V1_GFX32_STARTUP_POINTER_LOW_OFFSET,
            gfx32_payload_index,
            0,
            PatchFixupEncoding::Low16,
        ),
        pointer_write(
            bytes,
            crate::SMW_US_V1_SPECIAL_GRAPHICS_STARTUP_POINTER_BANK_OFFSET,
            gfx33_payload_index,
            0,
            PatchFixupEncoding::Bank8LowBank,
        ),
    ]);
    let hook = read_array::<5>(bytes, SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET)?;
    writes.push(PatchWrite {
        offset: SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET,
        expected: hook.to_vec(),
        replacement: vec![INSTALLED_OPCODE, 0, 0, 0, INSTALLED_RETURN],
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload: 0,
            target_addend: 0,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    });
    let metadata = bytes[SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET];
    writes.push(PatchWrite {
        offset: SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET,
        expected: vec![metadata],
        replacement: vec![(metadata & 0xf0) | 2],
        fixups: Vec::new(),
    });
    Ok(RelocatablePatchPlan {
        description: "install SMW LZ3 decoder and migrate standard graphics".into(),
        mapper: Mapper::LoRom,
        allocation,
        checksum_field,
        expansion_fill: 0xff,
        payloads,
        writes,
    })
}

fn pointer_write(
    bytes: &[u8],
    offset: usize,
    target_payload: usize,
    target_addend: usize,
    encoding: PatchFixupEncoding,
) -> PatchWrite {
    let len = encoding.encoded_len();
    PatchWrite {
        offset,
        expected: bytes[offset..offset + len].to_vec(),
        replacement: vec![0; len],
        fixups: vec![PatchFixup {
            offset: 0,
            target_payload,
            target_addend,
            encoding,
        }],
    }
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

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ2-Orig installed-graphics ROM"]
    fn standard_lz3_component_preserves_all_52_graphics_and_undoes() {
        let original = fs::read(std::env::var_os("LM_LZ2_ORIGINAL_ROM").unwrap()).unwrap();
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let source_project = Project::new(image.clone());
        let ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout();
        let ordinary = (0..ordinary_layout.pointers.entries)
            .map(|slot| {
                source_project
                    .load_graphics_file(slot, ordinary_layout)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let special = crate::smw_us_v1_special_graphics_layouts(&image).unwrap();
        let gfx33 = source_project.load_graphics_file(0, special.gfx33).unwrap();
        let gfx32 = source_project.load_graphics_file(0, special.gfx32).unwrap();
        let plan = smw_us_v1_standard_gfx_lz3_installation_plan(
            &image,
            AllocationPolicy::lorom(0x10_0000..0x20_0000),
            0x7fdc,
        )
        .unwrap();
        let mut project = Project::new(image);
        project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        let mut target_layout = ordinary_layout;
        target_layout.compression = GraphicsCompression::Lz3;
        for (slot, expected) in ordinary.iter().enumerate() {
            assert_eq!(
                project.load_graphics_file(slot, target_layout).unwrap(),
                *expected
            );
        }
        let mut target_special = crate::smw_us_v1_special_graphics_layouts(&project.rom).unwrap();
        target_special.gfx33.compression = GraphicsCompression::Lz3;
        target_special.gfx32.compression = GraphicsCompression::Lz3;
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx33).unwrap(),
            gfx33
        );
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx32).unwrap(),
            gfx32
        );
        if let Some(path) = std::env::var_os("LM_LZ3_STANDARD_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
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
