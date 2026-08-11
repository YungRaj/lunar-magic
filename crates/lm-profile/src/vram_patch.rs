//! Lunar Magic's current per-ROM VRAM patch state for pristine SMW US revision 0.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, RomImage, pc_to_snes, snes_to_pc};
use std::io::Read;

use crate::{SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_LM_VRAM_VERSION_OFFSET};

/// Headerless location of the installed runtime's owning JML.
///
/// Lunar Magic's active headered descriptor contains `$000003E2`; removing the copier header
/// yields this logical offset. Its three-byte operand points at the RATS payload start.
pub const SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK: usize = 0x0000_01e2;

/// Headerless location used by `CheckVramPatchSignatureByte` (`00469880`).
///
/// Lunar Magic's active headered descriptor contains `$000027A2`.
pub const SMW_US_V1_VRAM_PATCH_SECONDARY_HOOK: usize = 0x0000_25a2;

const JML: u8 = 0x5c;
const OWNER_TRAILER_LEN: usize = 4;
const OWNER_MAGIC: u16 = 0x4d4c;
const CURRENT_GENERATION: u16 = 0x0115;
const NORMAL_PAYLOAD_LEN: usize = 0x3390;
const NORMAL_METADATA_LEN: usize = 0x0720;
const NORMAL_RESOURCE: &str = include_str!("assets/vram_patch_normal_lm363.bin.gz.b64");
const OPTIONAL_EXTERNAL_POINTER: [u8; 3] = [0x5b, 0x28, 0x6b];
const NORMAL_SEARCH_START: usize = 0x0008_0000;

const NORMAL_HOOKS: &[(usize, &[u8], u8, usize)] = &[
    (0x0000_01e2, &[0xf0, 0x62, 0x4c, 0x7a], 0x5c, 0x0000),
    (0x0000_25a2, &[0x9c, 0x3a, 0x14, 0x20], 0x5c, 0x2285),
    (0x0000_0209, &[0x22, 0xad, 0x87, 0x00], 0x22, 0x2414),
    (0x0002_80c7, &[0x22, 0xad, 0x87, 0x00], 0x22, 0x2414),
    (0x0002_80bf, &[0x22, 0xec, 0x88, 0x05], 0x22, 0x261e),
    (0x0002_80c3, &[0x22, 0x55, 0x89, 0x05], 0x22, 0x2692),
    (0x0000_76e4, &[0xe9, 0x0c, 0x00, 0x8d], 0x5c, 0x2f98),
    (0x0000_77e8, &[0xa2, 0x07, 0xb5, 0x1a], 0x5c, 0x2fbc),
    (0x0002_80a9, &[0x85, 0x4d, 0x85, 0x4f], 0x5c, 0x2fe9),
    (0x0002_86f7, &[0xe2, 0x20, 0xa5, 0x5b], 0x5c, 0x3007),
    (0x0000_0751, &[0xc2, 0x20, 0xa5, 0x03], 0x5c, 0x22ca),
];

#[derive(Debug)]
pub enum SmwUsV1VramPatchBuildError {
    InvalidBase64,
    Gzip(std::io::Error),
    ResourceLength(usize),
    Metadata,
    UnsupportedSection([u8; 4]),
    Range,
    Mapping(RomError),
}

impl std::fmt::Display for SmwUsV1VramPatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SMW-US VRAM patch build failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1VramPatchBuildError {}

impl From<RomError> for SmwUsV1VramPatchBuildError {
    fn from(value: RomError) -> Self {
        Self::Mapping(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1VramPatchState {
    /// Neither authenticated fixed hook is installed. Lunar Magic offers None and defaults to
    /// Normal; choosing either is deferred until the next level save.
    Absent,
    /// A recognized, RATS-owned Lunar Magic runtime is installed.
    Installed {
        version: u8,
        generation: u16,
        owner: RatsBlock,
        requires_replacement: bool,
    },
    /// Some installation evidence exists, but its version, ownership, or generation is not one
    /// Lunar Magic 3.63 can safely replace. The original disables all choices in this state.
    Unknown {
        version: u8,
        primary_hook: bool,
        secondary_hook: bool,
    },
}

/// Relocates Lunar Magic 3.63's authenticated ordinary-LoROM Normal VRAM runtime to one RATS
/// payload offset.
///
/// Resource `$1FD` contains a `$3390`-byte runtime followed by `$720` bytes of `LMRE/LOC1`
/// relocation metadata. The current resource uses `INT2`, `INT3`, `TAB3`, and one `OPT3` request;
/// its external fixed-address requests are already encoded for pristine SMW-US LoROM.
///
/// # Errors
///
/// Rejects malformed embedded evidence, unsupported relocation sections, out-of-range requests,
/// or a destination that cannot be represented by LoROM.
pub fn smw_us_v1_normal_vram_patch_payload(
    payload_offset: usize,
) -> Result<Vec<u8>, SmwUsV1VramPatchBuildError> {
    let resource = normal_resource()?;
    let metadata = &resource[NORMAL_PAYLOAD_LEN..];
    if &metadata[..8] != b"LMRELOC1" {
        return Err(SmwUsV1VramPatchBuildError::Metadata);
    }
    let origin = read_u32(metadata, 8)?;
    if origin != 0x001f_8000 || read_u32(metadata, 12)? as usize + 16 > metadata.len() {
        return Err(SmwUsV1VramPatchBuildError::Metadata);
    }
    let mut payload = resource[..NORMAL_PAYLOAD_LEN].to_vec();
    let end = read_u32(metadata, 12)? as usize + 16;
    let mut cursor = 16;
    while cursor + 8 <= end {
        let tag: [u8; 4] = metadata[cursor..cursor + 4].try_into().unwrap();
        let len = read_u32(metadata, cursor + 4)? as usize;
        let data_start = cursor + 8;
        let data_end = data_start
            .checked_add(len)
            .filter(|end| *end <= metadata.len())
            .ok_or(SmwUsV1VramPatchBuildError::Range)?;
        let data = &metadata[data_start..data_end];
        match &tag {
            b"INT2" => relocate_grouped(data, origin, payload_offset, 2, &mut payload)?,
            b"INT3" => relocate_grouped(data, origin, payload_offset, 3, &mut payload)?,
            b"TAB3" => relocate_ranges(data, origin, payload_offset, &mut payload)?,
            b"OPT3" => {
                write_grouped_constant(data, origin, OPTIONAL_EXTERNAL_POINTER, &mut payload)?
            }
            b"EXPA" | b"EXT1" | b"EXT3" => {}
            b"REND" => break,
            _ => return Err(SmwUsV1VramPatchBuildError::UnsupportedSection(tag)),
        }
        cursor = data_end;
    }
    Ok(payload)
}

/// Builds Lunar Magic 3.63's exact ordinary-LoROM Normal VRAM runtime transaction.
///
/// The embedded `$1FD` resource remains relocatable: all `INT2`, `INT3`, and `TAB3` requests are
/// represented as project fixups, while the eleven fixed ROM hooks and metadata version byte use
/// the authenticated pristine SMW-US revision-0 preconditions.
///
/// # Errors
///
/// Rejects malformed or unsupported relocation metadata in the authenticated resource.
pub fn smw_us_v1_normal_vram_patch_installation_plan(
    image_len: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1VramPatchBuildError> {
    let resource = normal_resource()?;
    let metadata = &resource[NORMAL_PAYLOAD_LEN..];
    let (origin, end) = metadata_header(metadata)?;
    let mut payload = resource[..NORMAL_PAYLOAD_LEN].to_vec();
    let mut fixups = Vec::new();
    let mut cursor = 16;
    while cursor + 8 <= end {
        let tag: [u8; 4] = metadata[cursor..cursor + 4].try_into().unwrap();
        let len = read_u32(metadata, cursor + 4)? as usize;
        let data_start = cursor + 8;
        let data_end = data_start
            .checked_add(len)
            .filter(|end| *end <= metadata.len())
            .ok_or(SmwUsV1VramPatchBuildError::Range)?;
        let data = &metadata[data_start..data_end];
        match &tag {
            b"INT2" => {
                collect_grouped_fixups(data, origin, PatchFixupEncoding::Low16, &mut fixups)?
            }
            b"INT3" => collect_grouped_fixups(
                data,
                origin,
                PatchFixupEncoding::Long24LowBank,
                &mut fixups,
            )?,
            b"TAB3" => collect_range_fixups(data, origin, &payload, &mut fixups)?,
            b"OPT3" => {
                let mut overrides = Vec::new();
                visit_groups(data, |_identifier, locations| {
                    for location in locations {
                        overrides.push(
                            location
                                .checked_sub(origin)
                                .ok_or(SmwUsV1VramPatchBuildError::Range)?
                                as usize,
                        );
                    }
                    Ok(())
                })?;
                fixups.retain(|fixup| {
                    let fixup_end = fixup.offset + fixup.encoding.encoded_len();
                    !overrides
                        .iter()
                        .any(|offset| fixup.offset < offset + 3 && *offset < fixup_end)
                });
                write_grouped_constant(data, origin, OPTIONAL_EXTERNAL_POINTER, &mut payload)?
            }
            b"EXPA" | b"EXT1" | b"EXT3" => {}
            b"REND" => break,
            _ => return Err(SmwUsV1VramPatchBuildError::UnsupportedSection(tag)),
        }
        cursor = data_end;
    }

    let mut writes = NORMAL_HOOKS
        .iter()
        .map(|&(offset, expected, opcode, target_addend)| PatchWrite {
            offset,
            expected: expected.to_vec(),
            replacement: vec![opcode, 0, 0, 0],
            fixups: vec![PatchFixup {
                offset: 1,
                target_payload: 0,
                target_addend,
                encoding: PatchFixupEncoding::Long24LowBank,
            }],
        })
        .collect::<Vec<_>>();
    writes.extend([
        PatchWrite {
            offset: 0x0002_80d3,
            expected: vec![0xa5, 0x47, 0x4a],
            replacement: vec![0x4c, 0xfb, 0x80],
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: 0x0002_879d,
            expected: vec![0xe2, 0x30, 0xa5, 0x55, 0xaa],
            replacement: vec![0xc2, 0x30, 0x4c, 0xc8, 0x87],
            fixups: Vec::new(),
        },
        PatchWrite {
            offset: SMW_US_V1_LM_VRAM_VERSION_OFFSET,
            expected: vec![0xff],
            replacement: vec![0x01],
            fixups: Vec::new(),
        },
    ]);

    let search_end = if image_len <= NORMAL_SEARCH_START {
        NORMAL_SEARCH_START + 0x8000
    } else {
        image_len
    };
    Ok(RelocatablePatchPlan {
        description: "install SMW US v1 Normal VRAM patch".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(NORMAL_SEARCH_START..search_end),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: payload,
            fixups,
        }],
        writes,
    })
}

fn normal_resource() -> Result<Vec<u8>, SmwUsV1VramPatchBuildError> {
    let compressed = decode_base64(NORMAL_RESOURCE)?;
    let mut resource = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut resource)
        .map_err(SmwUsV1VramPatchBuildError::Gzip)?;
    if resource.len() != NORMAL_PAYLOAD_LEN + NORMAL_METADATA_LEN {
        return Err(SmwUsV1VramPatchBuildError::ResourceLength(resource.len()));
    }
    Ok(resource)
}

fn metadata_header(metadata: &[u8]) -> Result<(u32, usize), SmwUsV1VramPatchBuildError> {
    if metadata.get(..8) != Some(b"LMRELOC1") {
        return Err(SmwUsV1VramPatchBuildError::Metadata);
    }
    let origin = read_u32(metadata, 8)?;
    let end = read_u32(metadata, 12)? as usize + 16;
    if origin != 0x001f_8000 || end > metadata.len() {
        return Err(SmwUsV1VramPatchBuildError::Metadata);
    }
    Ok((origin, end))
}

fn collect_grouped_fixups(
    data: &[u8],
    origin: u32,
    encoding: PatchFixupEncoding,
    output: &mut Vec<PatchFixup>,
) -> Result<(), SmwUsV1VramPatchBuildError> {
    visit_groups(data, |target, locations| {
        let target_addend = target
            .checked_sub(origin)
            .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize;
        for location in locations {
            output.push(PatchFixup {
                offset: location
                    .checked_sub(origin)
                    .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize,
                target_payload: 0,
                target_addend,
                encoding,
            });
        }
        Ok(())
    })
}

fn collect_range_fixups(
    data: &[u8],
    origin: u32,
    payload: &[u8],
    output: &mut Vec<PatchFixup>,
) -> Result<(), SmwUsV1VramPatchBuildError> {
    if !data.len().is_multiple_of(8) {
        return Err(SmwUsV1VramPatchBuildError::Range);
    }
    for pair in data.chunks_exact(8) {
        let start = u32::from_le_bytes(pair[..4].try_into().unwrap())
            .checked_sub(origin)
            .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize;
        let count = u32::from_le_bytes(pair[4..].try_into().unwrap()) as usize;
        for index in 0..count {
            let offset = start
                .checked_add(
                    index
                        .checked_mul(3)
                        .ok_or(SmwUsV1VramPatchBuildError::Range)?,
                )
                .ok_or(SmwUsV1VramPatchBuildError::Range)?;
            let current = payload
                .get(offset..offset + 3)
                .ok_or(SmwUsV1VramPatchBuildError::Range)?;
            let target = u32::from(current[0])
                | (u32::from(current[1]) << 8)
                | (u32::from(current[2]) << 16);
            output.push(PatchFixup {
                offset,
                target_payload: 0,
                target_addend: target
                    .checked_sub(origin)
                    .ok_or(SmwUsV1VramPatchBuildError::Range)?
                    as usize,
                encoding: PatchFixupEncoding::Long24LowBank,
            });
        }
    }
    Ok(())
}

fn relocate_grouped(
    data: &[u8],
    origin: u32,
    payload_offset: usize,
    width: usize,
    payload: &mut [u8],
) -> Result<(), SmwUsV1VramPatchBuildError> {
    visit_groups(data, |target, locations| {
        let relative = target
            .checked_sub(origin)
            .ok_or(SmwUsV1VramPatchBuildError::Range)?;
        let target_pc = payload_offset
            .checked_add(relative as usize)
            .ok_or(SmwUsV1VramPatchBuildError::Range)?;
        // Lunar Magic emits the low-bank LoROM mirror (`$10xxxx` at this oracle location), while
        // `lm-rom` deliberately returns the canonical high-bank mirror (`$90xxxx`).
        let address = pc_to_snes(Mapper::LoRom, target_pc)? & 0x007f_ffff;
        let bytes = address.to_le_bytes();
        for location in locations {
            let offset = location
                .checked_sub(origin)
                .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize;
            copy_at(payload, offset, &bytes[..width])?;
        }
        Ok(())
    })
}

fn relocate_ranges(
    data: &[u8],
    origin: u32,
    payload_offset: usize,
    payload: &mut [u8],
) -> Result<(), SmwUsV1VramPatchBuildError> {
    if !data.len().is_multiple_of(8) {
        return Err(SmwUsV1VramPatchBuildError::Range);
    }
    for pair in data.chunks_exact(8) {
        let start = u32::from_le_bytes(pair[..4].try_into().unwrap())
            .checked_sub(origin)
            .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize;
        let count = u32::from_le_bytes(pair[4..].try_into().unwrap()) as usize;
        for index in 0..count {
            let offset = start
                .checked_add(
                    index
                        .checked_mul(3)
                        .ok_or(SmwUsV1VramPatchBuildError::Range)?,
                )
                .ok_or(SmwUsV1VramPatchBuildError::Range)?;
            let current = payload
                .get(offset..offset + 3)
                .ok_or(SmwUsV1VramPatchBuildError::Range)?;
            let address = u32::from(current[0])
                | (u32::from(current[1]) << 8)
                | (u32::from(current[2]) << 16);
            let relative = address
                .checked_sub(origin)
                .ok_or(SmwUsV1VramPatchBuildError::Range)?;
            let relocated = (pc_to_snes(
                Mapper::LoRom,
                payload_offset
                    .checked_add(relative as usize)
                    .ok_or(SmwUsV1VramPatchBuildError::Range)?,
            )? & 0x007f_ffff)
                .to_le_bytes();
            copy_at(payload, offset, &relocated[..3])?;
        }
    }
    Ok(())
}

fn write_grouped_constant(
    data: &[u8],
    origin: u32,
    value: [u8; 3],
    payload: &mut [u8],
) -> Result<(), SmwUsV1VramPatchBuildError> {
    visit_groups(data, |_identifier, locations| {
        for location in locations {
            let offset = location
                .checked_sub(origin)
                .ok_or(SmwUsV1VramPatchBuildError::Range)? as usize;
            copy_at(payload, offset, &value)?;
        }
        Ok(())
    })
}

fn visit_groups(
    data: &[u8],
    mut visit: impl FnMut(u32, Vec<u32>) -> Result<(), SmwUsV1VramPatchBuildError>,
) -> Result<(), SmwUsV1VramPatchBuildError> {
    let mut cursor = 0;
    while cursor < data.len() {
        if cursor + 8 > data.len() {
            return Err(SmwUsV1VramPatchBuildError::Range);
        }
        let identifier = u32::from_le_bytes(data[cursor..cursor + 4].try_into().unwrap());
        let len = u32::from_le_bytes(data[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if !len.is_multiple_of(4) || cursor + 8 + len > data.len() {
            return Err(SmwUsV1VramPatchBuildError::Range);
        }
        let locations = data[cursor + 8..cursor + 8 + len]
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        visit(identifier, locations)?;
        cursor += 8 + len;
    }
    Ok(())
}

fn copy_at(
    output: &mut [u8],
    offset: usize,
    bytes: &[u8],
) -> Result<(), SmwUsV1VramPatchBuildError> {
    let target = output
        .get_mut(offset..offset + bytes.len())
        .ok_or(SmwUsV1VramPatchBuildError::Range)?;
    target.copy_from_slice(bytes);
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, SmwUsV1VramPatchBuildError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(SmwUsV1VramPatchBuildError::Range)?
            .try_into()
            .unwrap(),
    ))
}

fn decode_base64(text: &str) -> Result<Vec<u8>, SmwUsV1VramPatchBuildError> {
    let mut output = Vec::new();
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in text.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(SmwUsV1VramPatchBuildError::InvalidBase64),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits).wrapping_sub(1);
        }
    }
    Ok(output)
}

/// Detects the exact modern VRAM-patch states needed by command `$24E8`.
///
/// The current generation stores `LM` plus generation `$0115` in the final four payload bytes.
/// Generation `$0114` remains recognized but must be replaced on the next level save. A JML with
/// no exact RATS owner, an unsupported generation, or an unknown version is deliberately not
/// treated as absence.
///
/// # Errors
///
/// Returns a ROM range or mapping error if the fixed SMW-US fields cannot be read or the installed
/// JML operand cannot be represented by LoROM.
pub fn detect_smw_us_v1_vram_patch(rom: &RomImage) -> Result<SmwUsV1VramPatchState, RomError> {
    let primary = rom.read(SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK, 4)?;
    let secondary = rom.read(SMW_US_V1_VRAM_PATCH_SECONDARY_HOOK, 1)?[0] == JML;
    let version = rom.read(SMW_US_V1_LM_VRAM_VERSION_OFFSET, 1)?[0];
    let primary_hook = primary[0] == JML;

    if !primary_hook && !secondary {
        return Ok(SmwUsV1VramPatchState::Absent);
    }
    if !primary_hook {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    }

    let address =
        u32::from(primary[1]) | (u32::from(primary[2]) << 8) | (u32::from(primary[3]) << 16);
    let payload = snes_to_pc(Mapper::LoRom, address)?;
    let Some(header) = payload.checked_sub(lm_rats::HEADER_LEN) else {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    };
    let Ok(owner) = parse_at(rom.logical_bytes(), header) else {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    };
    if owner.payload.start != payload || owner.payload.len() < OWNER_TRAILER_LEN {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    }
    let trailer = &rom.logical_bytes()[owner.payload.end - OWNER_TRAILER_LEN..owner.payload.end];
    let magic = u16::from_le_bytes([trailer[0], trailer[1]]);
    let generation = u16::from_le_bytes([trailer[2], trailer[3]]);
    if magic != OWNER_MAGIC || !matches!(generation, 0x0114 | CURRENT_GENERATION) {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    }
    if !matches!(version, 1..=3) {
        return Ok(SmwUsV1VramPatchState::Unknown {
            version,
            primary_hook,
            secondary_hook: secondary,
        });
    }

    Ok(SmwUsV1VramPatchState::Installed {
        version,
        generation,
        owner,
        requires_replacement: generation != CURRENT_GENERATION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rats::make_header;
    use lm_rom::pc_to_snes;
    use std::{fs, path::PathBuf};

    fn fixture(name: &str) -> RomImage {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/level-save-000")
                    .join(name),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn exact_lm363_first_save_is_current_normal_runtime() {
        let state = detect_smw_us_v1_vram_patch(&fixture("after.smc")).unwrap();
        let SmwUsV1VramPatchState::Installed {
            version,
            generation,
            owner,
            requires_replacement,
        } = state
        else {
            panic!("expected installed VRAM runtime");
        };
        assert_eq!(version, 1);
        assert_eq!(generation, 0x0115);
        assert_eq!(owner.header_offset, 0x08095a);
        assert_eq!(owner.payload, 0x080962..0x083cf2);
        assert!(!requires_replacement);
    }

    #[test]
    fn relocated_resource_matches_the_exact_lm363_installed_payload() {
        let after = fixture("after.smc");
        let expected = &after.logical_bytes()[0x080962..0x083cf2];
        let actual = smw_us_v1_normal_vram_patch_payload(0x080962).unwrap();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(
            actual
                .iter()
                .zip(expected)
                .position(|(left, right)| left != right),
            None,
            "relocated payload differs from the authenticated installation"
        );
        assert_ne!(
            smw_us_v1_normal_vram_patch_payload(0x088008).unwrap(),
            expected
        );
    }

    #[test]
    fn pristine_install_is_relocatable_and_reopens_as_current_normal() {
        let before = fixture("before.smc");
        let oracle = fixture("after.smc");
        let plan = smw_us_v1_normal_vram_patch_installation_plan(before.logical_len()).unwrap();
        let mut project = Project::new(before);
        let result = project.install_relocatable_patch(&plan).unwrap();
        let block = &result.blocks[0];
        assert_eq!(block.header_offset, 0x080000);
        assert_eq!(block.payload.start, 0x080008);
        let expected_payload = smw_us_v1_normal_vram_patch_payload(block.payload.start).unwrap();
        let actual_payload = &project.rom.logical_bytes()[block.payload.clone()];
        let mismatch = actual_payload
            .iter()
            .zip(&expected_payload)
            .position(|(left, right)| left != right);
        assert_eq!(
            mismatch,
            None,
            "relocatable payload differs from direct relocation: {:?} != {:?}",
            mismatch.map(|offset| &actual_payload[offset..offset + 3]),
            mismatch.map(|offset| &expected_payload[offset..offset + 3]),
        );
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&project.rom).unwrap(),
            SmwUsV1VramPatchState::Installed {
                version: 1,
                generation: CURRENT_GENERATION,
                requires_replacement: false,
                ..
            }
        ));
        for &(offset, _, opcode, addend) in NORMAL_HOOKS {
            let address = (pc_to_snes(Mapper::LoRom, block.payload.start + addend).unwrap()
                & 0x007f_ffff)
                .to_le_bytes();
            assert_eq!(
                project.rom.read(offset, 4).unwrap(),
                &[opcode, address[0], address[1], address[2]],
                "fixed hook differs at {offset:#x}"
            );
        }
        for (offset, len) in [(0x0002_80d3, 3), (0x0002_879d, 5)] {
            assert_eq!(
                project.rom.read(offset, len).unwrap(),
                oracle.read(offset, len).unwrap(),
                "fixed branch differs at {offset:#x}"
            );
        }
    }

    #[test]
    fn exact_vanilla_rom_is_absent_even_with_uninitialized_version_byte() {
        assert_eq!(
            detect_smw_us_v1_vram_patch(&fixture("before.smc")).unwrap(),
            SmwUsV1VramPatchState::Absent
        );
    }

    #[test]
    fn recognized_old_generation_requests_replacement_but_corruption_is_unknown() {
        let mut bytes = fixture("before.smc").logical_bytes().to_vec();
        bytes.resize(0x10_0000, 0xff);
        let payload = 0x08_1000;
        let header = payload - lm_rats::HEADER_LEN;
        let mut body = vec![0xea; 0x40];
        let trailer = body.len() - 4;
        body[trailer..].copy_from_slice(&[b'L', b'M', 0x14, 0x01]);
        bytes[header..payload].copy_from_slice(&make_header(body.len()).unwrap());
        bytes[payload..payload + body.len()].copy_from_slice(&body);
        let pointer = pc_to_snes(Mapper::LoRom, payload).unwrap().to_le_bytes();
        bytes[SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK..SMW_US_V1_VRAM_PATCH_PRIMARY_HOOK + 4]
            .copy_from_slice(&[JML, pointer[0], pointer[1], pointer[2]]);
        bytes[SMW_US_V1_VRAM_PATCH_SECONDARY_HOOK] = JML;
        bytes[SMW_US_V1_LM_VRAM_VERSION_OFFSET] = 1;
        let old = RomImage::from_bytes(bytes.clone()).unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&old).unwrap(),
            SmwUsV1VramPatchState::Installed {
                generation: 0x0114,
                requires_replacement: true,
                ..
            }
        ));

        bytes[payload + body.len() - 4] ^= 1;
        let corrupt = RomImage::from_bytes(bytes).unwrap();
        assert!(matches!(
            detect_smw_us_v1_vram_patch(&corrupt).unwrap(),
            SmwUsV1VramPatchState::Unknown { .. }
        ));
    }
}
