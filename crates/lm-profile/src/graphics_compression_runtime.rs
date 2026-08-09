//! Authenticated Lunar Magic graphics-compression runtime detection for SMW US revision 0.

use lm_codec::{encode_lz2, encode_lz3};
use lm_project::{
    GraphicsCompression, GraphicsIoError, GraphicsRomLayout, LevelPointerTable, PatchFixup,
    PatchFixupEncoding, PatchPayload, PatchWrite, PayloadLoadError, PayloadReadPolicy, Project,
    RatsOwnershipManifest, RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rats::{HEADER_LEN, HeaderError, parse_at};
use lm_rom::{Mapper, RomError, RomImage, SnesPointer24, detect_identity, snes_to_pc};

pub const SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET: usize = 0x07_ffeb;
pub const SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET: usize = 0x0038_e3;

const ORIGINAL_HOOK: [u8; 5] = [0x20, 0x83, 0xb9, 0xc9, 0xff];
const INSTALLED_OPCODE: u8 = 0x22;
const INSTALLED_RETURN: u8 = 0x60;
const SPEED_RUNTIME_LEN: usize = 0x1c0;
const LZ3_RUNTIME_LEN: usize = 0x2ab;
const SA1_LZ3_RUNTIME_LEN: usize = 0x30c;
const SPEED_RUNTIME_CRC32: u32 = 0x5d3c_ac46;
const LZ3_RUNTIME_CRC32: u32 = 0xdcb7_727e;
const SA1_LZ3_RUNTIME_CRC32: u32 = 0x520e_eb36;
const SA1_LZ2_SPEED_OWNER_LEN: usize = 0x4806;
const SA1_LZ2_SPEED_RUNTIME_ADDEND: usize = 0x32ba;
const SA1_LZ2_SPEED_RUNTIME_LEN: usize = 0x154c;
const SA1_LZ2_SPEED_RUNTIME_CRC32: u32 = 0x5d96_54d6;
const RUNTIME_TRAILER: [u8; 4] = *b"LM\x01\x01";
const SPEED_RUNTIME_HEX: &str = include_str!("assets/graphics_compression_lz2_speed.hex");
const LZ3_RUNTIME_HEX: &str = include_str!("assets/graphics_compression_lz3.hex");
const SA1_LZ3_RUNTIME_HEX: &str = include_str!("assets/graphics_compression_lz3_sa1.hex");

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
    UnsupportedMapper(Mapper),
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
    ExGraphics(crate::SmwUsV1ExGraphicsError),
    EventTilemaps(crate::SmwUsV1EventTilemapLoadError),
    Payload(PayloadLoadError),
    Rom(RomError),
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

impl From<crate::SmwUsV1ExGraphicsError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: crate::SmwUsV1ExGraphicsError) -> Self {
        Self::ExGraphics(value)
    }
}

impl From<crate::SmwUsV1EventTilemapLoadError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: crate::SmwUsV1EventTilemapLoadError) -> Self {
        Self::EventTilemaps(value)
    }
}

impl From<PayloadLoadError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Payload(value)
    }
}

impl From<RomError> for SmwUsV1GraphicsCompressionMigrationError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

#[derive(Clone, Debug)]
pub struct SmwUsV1GraphicsCompressionReplacementPlan {
    pub plan: RelocatablePatchPlan,
    pub obsolete: RatsOwnershipManifest,
}

/// Detects the exact original, fast-LZ2, or LZ3 runtime selected by Lunar Magic metadata.
///
/// Installed modes require agreement between the metadata nibble, the fixed JSL/RTS hook, exact
/// RATS ownership, payload length, runtime trailer, and a checksum over the complete immutable
/// runtime. This prevents a profile from selecting a decoder based on the metadata byte alone.
pub fn detect_smw_us_v1_graphics_compression_mode(
    image: &RomImage,
) -> Result<SmwUsV1GraphicsCompressionMode, SmwUsV1GraphicsCompressionDetectError> {
    let mapper = compression_mapper(image)?;
    let base = mapper_body_base(mapper);
    let bytes = image.logical_bytes();
    let metadata_offset = base + SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET;
    let hook_offset = base + SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET;
    let metadata =
        *bytes
            .get(metadata_offset)
            .ok_or(SmwUsV1GraphicsCompressionDetectError::Truncated {
                offset: metadata_offset,
                len: 1,
            })?;
    let hook = read_array::<5>(bytes, hook_offset)?;
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
    if mode == SmwUsV1GraphicsCompressionMode::Lz2Original && mapper != Mapper::Sa1 {
        return (hook == ORIGINAL_HOOK).then_some(mode).ok_or(
            SmwUsV1GraphicsCompressionDetectError::OriginalHookMismatch(hook),
        );
    }
    if hook[0] != INSTALLED_OPCODE || hook[4] != INSTALLED_RETURN {
        return Err(SmwUsV1GraphicsCompressionDetectError::InstalledHookMismatch(hook));
    }
    let runtime_offset = snes_to_pc(mapper, u32::from_le_bytes([hook[1], hook[2], hook[3], 0]))
        .map_err(SmwUsV1GraphicsCompressionDetectError::RuntimeAddress)?;
    if mapper == Mapper::Sa1 && mode != SmwUsV1GraphicsCompressionMode::Lz3 {
        let owner = lm_rats::scan(bytes)
            .into_iter()
            .find(|block| block.payload.contains(&runtime_offset))
            .ok_or(SmwUsV1GraphicsCompressionDetectError::RuntimeBeforeHeader(
                runtime_offset,
            ))?;
        let expected = owner.payload.start + SA1_LZ2_SPEED_RUNTIME_ADDEND;
        if owner.payload.len() != SA1_LZ2_SPEED_OWNER_LEN || runtime_offset != expected {
            return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeOwnership {
                expected,
                actual: runtime_offset,
            });
        }
        let runtime = bytes
            .get(runtime_offset..runtime_offset + SA1_LZ2_SPEED_RUNTIME_LEN)
            .ok_or(SmwUsV1GraphicsCompressionDetectError::Truncated {
                offset: runtime_offset,
                len: SA1_LZ2_SPEED_RUNTIME_LEN,
            })?;
        let actual_crc = crc32(runtime);
        if actual_crc != SA1_LZ2_SPEED_RUNTIME_CRC32 {
            return Err(SmwUsV1GraphicsCompressionDetectError::RuntimeChecksum {
                expected: SA1_LZ2_SPEED_RUNTIME_CRC32,
                actual: actual_crc,
            });
        }
        return Ok(mode);
    }
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
        SmwUsV1GraphicsCompressionMode::Lz3 if mapper == Mapper::Sa1 => {
            (SA1_LZ3_RUNTIME_LEN, SA1_LZ3_RUNTIME_CRC32)
        }
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
    let mapper = compression_mapper(image)?;
    let base = mapper_body_base(mapper);
    let mode = detect_smw_us_v1_graphics_compression_mode(image)?;
    if mode != SmwUsV1GraphicsCompressionMode::Lz2Original {
        return Err(SmwUsV1GraphicsCompressionDetectError::SourceMode {
            expected: SmwUsV1GraphicsCompressionMode::Lz2Original,
            actual: mode,
        });
    }
    let metadata = image.logical_bytes()[base + SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET];
    let runtime = decode_hex(SPEED_RUNTIME_HEX);
    debug_assert_eq!(runtime.len(), SPEED_RUNTIME_LEN);
    debug_assert_eq!(crc32(&runtime), SPEED_RUNTIME_CRC32);
    Ok(RelocatablePatchPlan {
        description: "change SMW graphics compression from LZ2 Orig to LZ2 Speed".into(),
        mapper,
        allocation,
        checksum_field,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: runtime,
            fixups: vec![],
        }],
        writes: vec![
            PatchWrite {
                offset: base + SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET,
                expected: ORIGINAL_HOOK.to_vec(),
                replacement: vec![INSTALLED_OPCODE, 0, 0, 0, INSTALLED_RETURN],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: long_pointer_encoding(mapper),
                }],
            },
            PatchWrite {
                offset: base + SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET,
                expected: vec![metadata],
                replacement: vec![(metadata & 0xf0) | 1],
                fixups: vec![],
            },
        ],
    })
}

/// Builds Lunar Magic's complete installed-SMW LZ3 conversion.
///
/// The transaction preserves all 50 ordinary files, the shared-bank GFX33/GFX32 startup pair,
/// every populated compressed ExAnimation/ExGFX slot, and both installed overworld-event streams.
/// It rewrites their pointer tables, installs the authenticated LZ3 decoder, updates the metadata
/// nibble and ROM-size header, and repairs the checksum as one failure-atomic plan.
pub fn smw_us_v1_lz3_installation_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionMigrationError> {
    smw_us_v1_graphics_compression_installation_plan(
        image,
        allocation,
        checksum_field,
        SmwUsV1GraphicsCompressionMode::Lz3,
        None,
    )
}

/// Builds Lunar Magic's complete installed-SMW `LZ3` to `LZ2 Orig` conversion.
///
/// Every graphics-dependent stream is recompressed in the same atomic transaction as restoring
/// the original decoder hook and metadata. No installed decompressor remains reachable afterward.
pub fn smw_us_v1_lz2_original_installation_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionMigrationError> {
    smw_us_v1_graphics_compression_installation_plan(
        image,
        allocation,
        checksum_field,
        SmwUsV1GraphicsCompressionMode::Lz2Original,
        None,
    )
}

/// Builds Lunar Magic's complete installed-SMW `LZ3` to `LZ2 Speed` conversion.
pub fn smw_us_v1_lz2_speed_migration_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionMigrationError> {
    smw_us_v1_graphics_compression_installation_plan(
        image,
        allocation,
        checksum_field,
        SmwUsV1GraphicsCompressionMode::Lz2Speed,
        None,
    )
}

/// Builds a same-size codec replacement by proving ownership of every currently referenced
/// compression-owned RATS block and selecting a reclaimed bank for the shared startup payloads.
pub fn smw_us_v1_compact_graphics_compression_migration_plan(
    image: &RomImage,
    checksum_field: usize,
    target_mode: SmwUsV1GraphicsCompressionMode,
) -> Result<SmwUsV1GraphicsCompressionReplacementPlan, SmwUsV1GraphicsCompressionMigrationError> {
    let obsolete = smw_us_v1_graphics_compression_ownership(image)?;
    let bytes = image.logical_bytes();
    let first_bank = (0x80_000..image.logical_len())
        .step_by(0x8000)
        .find(|start| {
            let end = (*start + 0x8000).min(bytes.len());
            (*start..end).all(|offset| {
                matches!(bytes[offset], 0x00 | 0xff)
                    || obsolete
                        .owned
                        .iter()
                        .any(|block| block.full_range().contains(&offset))
            })
        })
        .ok_or(SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy)?;
    let allocation = AllocationPolicy {
        search: first_bank..image.logical_len(),
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected: Vec::new(),
    };
    let plan = smw_us_v1_graphics_compression_installation_plan(
        image,
        allocation,
        checksum_field,
        target_mode,
        Some(&obsolete),
    )?;
    Ok(SmwUsV1GraphicsCompressionReplacementPlan { plan, obsolete })
}

fn smw_us_v1_graphics_compression_installation_plan(
    image: &RomImage,
    allocation: AllocationPolicy,
    checksum_field: usize,
    target_mode: SmwUsV1GraphicsCompressionMode,
    obsolete: Option<&RatsOwnershipManifest>,
) -> Result<RelocatablePatchPlan, SmwUsV1GraphicsCompressionMigrationError> {
    let mapper = compression_mapper(image)?;
    let base = mapper_body_base(mapper);
    let source_mode = detect_smw_us_v1_graphics_compression_mode(image)?;
    let (source_compression, target_compression, target_event_compression) = match target_mode {
        SmwUsV1GraphicsCompressionMode::Lz3
            if source_mode != SmwUsV1GraphicsCompressionMode::Lz3 =>
        {
            (
                GraphicsCompression::Lz2,
                GraphicsCompression::Lz3,
                lm_project::EventTilemapCompression::Lz3,
            )
        }
        SmwUsV1GraphicsCompressionMode::Lz2Original
            if source_mode == SmwUsV1GraphicsCompressionMode::Lz3 =>
        {
            (
                GraphicsCompression::Lz3,
                GraphicsCompression::Lz2,
                lm_project::EventTilemapCompression::Lz2,
            )
        }
        SmwUsV1GraphicsCompressionMode::Lz2Speed
            if source_mode == SmwUsV1GraphicsCompressionMode::Lz3 =>
        {
            (
                GraphicsCompression::Lz3,
                GraphicsCompression::Lz2,
                lm_project::EventTilemapCompression::Lz2,
            )
        }
        _ => {
            return Err(SmwUsV1GraphicsCompressionDetectError::SourceMode {
                expected: match target_mode {
                    SmwUsV1GraphicsCompressionMode::Lz3 => {
                        SmwUsV1GraphicsCompressionMode::Lz2Original
                    }
                    SmwUsV1GraphicsCompressionMode::Lz2Original
                    | SmwUsV1GraphicsCompressionMode::Lz2Speed => {
                        SmwUsV1GraphicsCompressionMode::Lz3
                    }
                },
                actual: source_mode,
            }
            .into());
        }
    };
    let encode = |raw: &[u8]| match target_compression {
        GraphicsCompression::Lz2 => encode_lz2(raw),
        GraphicsCompression::Lz3 => encode_lz3(raw),
    };
    let project = Project::new(image.clone());
    let mut ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout_for_mapper(mapper);
    ordinary_layout.compression = source_compression;
    let ordinary = (0..ordinary_layout.pointers.entries)
        .map(|slot| {
            let raw = project
                .load_graphics_file(slot, ordinary_layout)?
                .encode()
                .map_err(GraphicsIoError::from)?;
            Ok(encode(&raw))
        })
        .collect::<Result<Vec<_>, SmwUsV1GraphicsCompressionMigrationError>>()?;
    let mut special = crate::smw_us_v1_special_graphics_layouts_for_mapper(image, mapper)?;
    special.gfx33.compression = source_compression;
    special.gfx32.compression = source_compression;
    let gfx33 = encode(
        &project
            .load_graphics_file(0, special.gfx33)?
            .encode()
            .map_err(GraphicsIoError::from)?,
    );
    let gfx32 = encode(
        &project
            .load_graphics_file(0, special.gfx32)?
            .encode()
            .map_err(GraphicsIoError::from)?,
    );
    let runtime = match target_mode {
        SmwUsV1GraphicsCompressionMode::Lz2Original => None,
        SmwUsV1GraphicsCompressionMode::Lz2Speed => Some(decode_hex(SPEED_RUNTIME_HEX)),
        SmwUsV1GraphicsCompressionMode::Lz3 if mapper == Mapper::Sa1 => {
            Some(decode_hex(SA1_LZ3_RUNTIME_HEX))
        }
        SmwUsV1GraphicsCompressionMode::Lz3 => Some(decode_hex(LZ3_RUNTIME_HEX)),
    };
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
        || runtime.as_ref().map_or(0, Vec::len)
            + gfx33.len()
            + gfx32.len()
            + (2 + usize::from(runtime.is_some())) * HEADER_LEN
            > 0x8000
        || image.logical_bytes()[occupied_first_bank.clone()]
            .iter()
            .enumerate()
            .any(|(relative, byte)| {
                !allocation.fill_bytes.contains(byte)
                    && obsolete.is_none_or(|manifest| {
                        let offset = occupied_first_bank.start + relative;
                        !manifest
                            .owned
                            .iter()
                            .any(|block| block.full_range().contains(&offset))
                    })
            })
    {
        return Err(SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy);
    }
    let mut payloads = Vec::with_capacity(53);
    let runtime_payload_index = runtime.map(|bytes| {
        let index = payloads.len();
        payloads.push(PatchPayload {
            bytes,
            fixups: Vec::new(),
        });
        index
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
    let target_len = image.logical_len().max(allocation.search.end);
    let maximum_len = if matches!(mapper, Mapper::ExLoRom | Mapper::Sa1) {
        0x80_0000
    } else {
        0x40_0000
    };
    if !target_len.is_power_of_two() || !(0x10_0000..=maximum_len).contains(&target_len) {
        return Err(SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy);
    }
    let rom_size = u8::try_from(target_len.ilog2() - 10)
        .map_err(|_| SmwUsV1GraphicsCompressionMigrationError::SpecialGraphicsAllocationPolicy)?;
    let rom_size_offset = base + crate::smw_us_v1_exgraphics::SMW_US_V1_ROM_SIZE_OFFSET;
    if bytes[rom_size_offset] != rom_size {
        writes.push(PatchWrite {
            offset: rom_size_offset,
            expected: vec![bytes[rom_size_offset]],
            replacement: vec![rom_size],
            fixups: Vec::new(),
        });
    }
    for slot in 0..ordinary_layout.pointers.entries {
        let target_payload = gfx32_payload_index + 1 + slot;
        for (offset, encoding) in [
            (
                base + crate::SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET + slot,
                PatchFixupEncoding::Low8,
            ),
            (
                base + crate::SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET + slot,
                PatchFixupEncoding::High8,
            ),
            (
                base + crate::SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET + slot,
                bank_pointer_encoding(mapper),
            ),
        ] {
            writes.push(pointer_write(bytes, offset, target_payload, 0, encoding));
        }
    }
    writes.extend([
        pointer_write(
            bytes,
            base + crate::SMW_US_V1_GFX33_STARTUP_POINTER_LOW_OFFSET,
            gfx33_payload_index,
            0,
            PatchFixupEncoding::Low16,
        ),
        pointer_write(
            bytes,
            base + crate::SMW_US_V1_GFX32_STARTUP_POINTER_LOW_OFFSET,
            gfx32_payload_index,
            0,
            PatchFixupEncoding::Low16,
        ),
        pointer_write(
            bytes,
            base + crate::SMW_US_V1_SPECIAL_GRAPHICS_STARTUP_POINTER_BANK_OFFSET,
            gfx33_payload_index,
            0,
            bank_pointer_encoding(mapper),
        ),
    ]);
    for file_number in 0x80_u16..=exgraphics_scan_end(image, mapper)? {
        let route = crate::smw_us_v1_exgraphics_pointer_in_rom(image, file_number, mapper)?;
        if route.encoding != crate::SmwUsV1ExGraphicsEncoding::Lz2 {
            continue;
        }
        let pointer = image
            .read(route.pointer_offset, 3)
            .map_err(GraphicsIoError::from)?;
        if pointer == [0; 3] || pointer == [0xff; 3] {
            continue;
        }
        let raw = project.load_decompressed_graphics_file(
            0,
            GraphicsRomLayout {
                mapper,
                pointers: LevelPointerTable {
                    offset: route.pointer_offset,
                    entries: 1,
                    stride: 3,
                },
                split_pointer_planes: None,
                compression: source_compression,
                maximum_compressed_len: 0x8000,
                maximum_decompressed_len: 0x1000,
            },
        )?;
        if !matches!(raw.len(), 0x800 | 0xc00 | 0x1000) {
            return Err(crate::SmwUsV1ExGraphicsError::InvalidRawLength {
                file_number,
                actual: raw.len(),
            }
            .into());
        }
        let target_payload = payloads.len();
        payloads.push(PatchPayload {
            bytes: encode(&raw),
            fixups: Vec::new(),
        });
        writes.push(pointer_write(
            bytes,
            route.pointer_offset,
            target_payload,
            0,
            long_pointer_encoding(mapper),
        ));
    }
    let event_tilemaps = (mapper != Mapper::Sa1)
        .then(|| crate::load_smw_us_v1_event_tilemaps_for_mapper(&project, mapper))
        .transpose()?;
    if let Some(crate::LoadedSmwUsV1EventTilemaps {
        buffers,
        storage: crate::SmwUsV1EventTilemapStorage::Installed(source_event_compression),
    }) = event_tilemaps
    {
        let primary_payload = payloads.len();
        payloads.push(PatchPayload {
            bytes: encode(&buffers.encode_primary_stream()),
            fixups: Vec::new(),
        });
        let secondary_payload = payloads.len();
        payloads.push(PatchPayload {
            bytes: encode(&buffers.encode_secondary_high_stream()),
            fixups: Vec::new(),
        });
        for (low_offset, bank_offset, target_payload) in [
            (
                base + crate::SMW_US_V1_EVENT_TILEMAP_PRIMARY_LOW_WORD,
                base + crate::SMW_US_V1_EVENT_TILEMAP_PRIMARY_BANK,
                primary_payload,
            ),
            (
                base + crate::SMW_US_V1_EVENT_TILEMAP_SECONDARY_LOW_WORD,
                base + crate::SMW_US_V1_EVENT_TILEMAP_SECONDARY_BANK,
                secondary_payload,
            ),
        ] {
            writes.push(pointer_write(
                bytes,
                low_offset,
                target_payload,
                0,
                PatchFixupEncoding::Low16,
            ));
            writes.push(pointer_write(
                bytes,
                bank_offset,
                target_payload,
                0,
                bank_pointer_encoding(mapper),
            ));
        }
        debug_assert_ne!(source_event_compression, target_event_compression);
    }
    let hook_offset = base + SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET;
    let hook = read_array::<5>(bytes, hook_offset)?;
    let (replacement, fixups) = match runtime_payload_index {
        Some(target_payload) => (
            vec![INSTALLED_OPCODE, 0, 0, 0, INSTALLED_RETURN],
            vec![PatchFixup {
                offset: 1,
                target_payload,
                target_addend: 0,
                encoding: long_pointer_encoding(mapper),
            }],
        ),
        None => (ORIGINAL_HOOK.to_vec(), Vec::new()),
    };
    writes.push(PatchWrite {
        offset: hook_offset,
        expected: hook.to_vec(),
        replacement,
        fixups,
    });
    let metadata_offset = base + SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET;
    let metadata = bytes[metadata_offset];
    writes.push(PatchWrite {
        offset: metadata_offset,
        expected: vec![metadata],
        replacement: vec![
            (metadata & 0xf0)
                | match target_mode {
                    SmwUsV1GraphicsCompressionMode::Lz2Original => 0,
                    SmwUsV1GraphicsCompressionMode::Lz2Speed => 1,
                    SmwUsV1GraphicsCompressionMode::Lz3 => 2,
                },
        ],
        fixups: Vec::new(),
    });
    Ok(RelocatablePatchPlan {
        description: match target_mode {
            SmwUsV1GraphicsCompressionMode::Lz2Original => {
                "restore SMW LZ2 Orig decoder and migrate all graphics".into()
            }
            SmwUsV1GraphicsCompressionMode::Lz2Speed => {
                "install SMW LZ2 Speed decoder and migrate all graphics".into()
            }
            SmwUsV1GraphicsCompressionMode::Lz3 => {
                "install SMW LZ3 decoder and migrate all graphics".into()
            }
        },
        mapper,
        allocation,
        checksum_field,
        expansion_fill: 0xff,
        payloads,
        writes,
    })
}

fn smw_us_v1_graphics_compression_ownership(
    image: &RomImage,
) -> Result<RatsOwnershipManifest, SmwUsV1GraphicsCompressionMigrationError> {
    let mapper = compression_mapper(image)?;
    let base = mapper_body_base(mapper);
    let project = Project::new(image.clone());
    let mut owned = Vec::new();
    let mut push_pointer =
        |pointer: SnesPointer24| -> Result<(), SmwUsV1GraphicsCompressionMigrationError> {
            let loaded = match project.load_payload_from_pointer(
                pointer,
                mapper,
                &PayloadReadPolicy::Tagged,
            ) {
                Ok(loaded) => loaded,
                Err(PayloadLoadError::PointerNotTagged { .. }) => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            let block = loaded
                .block
                .expect("tagged payload reads always return their authenticated owner");
            if !owned.contains(&block) {
                owned.push(block);
            }
            Ok(())
        };

    let mode = detect_smw_us_v1_graphics_compression_mode(image)?;
    if mode != SmwUsV1GraphicsCompressionMode::Lz2Original {
        let hook = read_array::<5>(
            image.logical_bytes(),
            base + SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET,
        )?;
        push_pointer(
            SnesPointer24::new(u32::from_le_bytes([hook[1], hook[2], hook[3], 0]))
                .expect("three hook operand bytes are a 24-bit address"),
        )?;
    }

    let ordinary = crate::smw_us_v1_vanilla_graphics_layout_for_mapper(mapper);
    for slot in 0..ordinary.pointers.entries {
        push_pointer(ordinary.read_pointer(&project, slot)?)?;
    }
    let special = crate::smw_us_v1_special_graphics_layouts_for_mapper(image, mapper)?;
    push_pointer(special.gfx33.read_pointer(&project, 0)?)?;
    push_pointer(special.gfx32.read_pointer(&project, 0)?)?;

    for file_number in 0x80_u16..=exgraphics_scan_end(image, mapper)? {
        let route = crate::smw_us_v1_exgraphics_pointer_in_rom(image, file_number, mapper)?;
        if route.encoding != crate::SmwUsV1ExGraphicsEncoding::Lz2 {
            continue;
        }
        let pointer = image.read(route.pointer_offset, 3)?;
        if pointer == [0; 3] || pointer == [0xff; 3] {
            continue;
        }
        push_pointer(
            SnesPointer24::new(u32::from_le_bytes([pointer[0], pointer[1], pointer[2], 0]))
                .expect("three ExGFX pointer bytes are a 24-bit address"),
        )?;
    }

    if mapper != Mapper::Sa1
        && matches!(
            crate::load_smw_us_v1_event_tilemaps_for_mapper(&project, mapper)?.storage,
            crate::SmwUsV1EventTilemapStorage::Installed(_)
        )
    {
        for (low_offset, bank_offset) in [
            (
                base + crate::SMW_US_V1_EVENT_TILEMAP_PRIMARY_LOW_WORD,
                base + crate::SMW_US_V1_EVENT_TILEMAP_PRIMARY_BANK,
            ),
            (
                base + crate::SMW_US_V1_EVENT_TILEMAP_SECONDARY_LOW_WORD,
                base + crate::SMW_US_V1_EVENT_TILEMAP_SECONDARY_BANK,
            ),
        ] {
            let low = image.read(low_offset, 2)?;
            let bank = image.read(bank_offset, 1)?[0];
            push_pointer(
                SnesPointer24::new(
                    u32::from(low[0]) | (u32::from(low[1]) << 8) | (u32::from(bank) << 16),
                )
                .expect("split event pointer bytes are a 24-bit address"),
            )?;
        }
    }
    owned.sort_by_key(|block| block.header_offset);
    Ok(RatsOwnershipManifest {
        owned,
        retained: Vec::new(),
    })
}

fn compression_mapper(image: &RomImage) -> Result<Mapper, SmwUsV1GraphicsCompressionDetectError> {
    let mapper = detect_identity(image).map_or(Mapper::LoRom, |identity| identity.mapper);
    match mapper {
        Mapper::LoRom | Mapper::ExLoRom | Mapper::Sa1 => Ok(mapper),
    }
}

fn exgraphics_scan_end(
    image: &RomImage,
    mapper: Mapper,
) -> Result<u16, crate::SmwUsV1ExGraphicsError> {
    if mapper == Mapper::Sa1
        && image.read(
            crate::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
            crate::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER.len(),
        )? != crate::SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER
    {
        Ok(0xff)
    } else {
        Ok(0x0fff)
    }
}

const fn mapper_body_base(mapper: Mapper) -> usize {
    if matches!(mapper, Mapper::ExLoRom) {
        0x40_0000
    } else {
        0
    }
}

const fn long_pointer_encoding(mapper: Mapper) -> PatchFixupEncoding {
    if matches!(mapper, Mapper::LoRom) {
        PatchFixupEncoding::Long24LowBank
    } else {
        PatchFixupEncoding::Long24
    }
}

const fn bank_pointer_encoding(mapper: Mapper) -> PatchFixupEncoding {
    if matches!(mapper, Mapper::LoRom) {
        PatchFixupEncoding::Bank8LowBank
    } else {
        PatchFixupEncoding::Bank8
    }
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

    fn load_mapper_graphics(
        image: &RomImage,
        mapper: Mapper,
        compression: GraphicsCompression,
    ) -> (
        Vec<lm_graphics::GraphicsFile4bpp>,
        lm_graphics::GraphicsFile4bpp,
        lm_graphics::GraphicsFile4bpp,
        Vec<(u16, Vec<u8>)>,
    ) {
        let project = Project::new(image.clone());
        let mut ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout_for_mapper(mapper);
        ordinary_layout.compression = compression;
        let ordinary = (0..ordinary_layout.pointers.entries)
            .map(|slot| project.load_graphics_file(slot, ordinary_layout).unwrap())
            .collect();
        let mut special =
            crate::smw_us_v1_special_graphics_layouts_for_mapper(image, mapper).unwrap();
        special.gfx33.compression = compression;
        special.gfx32.compression = compression;
        let gfx33 = project.load_graphics_file(0, special.gfx33).unwrap();
        let gfx32 = project.load_graphics_file(0, special.gfx32).unwrap();
        let mut exgfx = Vec::new();
        for file_number in 0x80_u16..=exgraphics_scan_end(image, mapper).unwrap() {
            let route =
                crate::smw_us_v1_exgraphics_pointer_in_rom(image, file_number, mapper).unwrap();
            if route.encoding != crate::SmwUsV1ExGraphicsEncoding::Lz2 {
                continue;
            }
            let pointer = image.read(route.pointer_offset, 3).unwrap();
            if pointer == [0; 3] || pointer == [0xff; 3] {
                continue;
            }
            let bytes = project
                .load_decompressed_graphics_file(
                    0,
                    GraphicsRomLayout {
                        mapper,
                        pointers: LevelPointerTable {
                            offset: route.pointer_offset,
                            entries: 1,
                            stride: 3,
                        },
                        split_pointer_planes: None,
                        compression,
                        maximum_compressed_len: 0x8000,
                        maximum_decompressed_len: 0x1000,
                    },
                )
                .unwrap();
            exgfx.push((file_number, bytes));
        }
        (ordinary, gfx33, gfx32, exgfx)
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 ExLoROM LZ2/LZ3 conversion captures"]
    fn exlorom_codec_replacement_preserves_every_graphics_stream_and_undoes() {
        let lz2_bytes = fs::read(std::env::var_os("LM_EXLOROM_LZ2_ROM").unwrap()).unwrap();
        let lz3_oracle = RomImage::from_bytes(
            fs::read(std::env::var_os("LM_EXLOROM_LZ3_ROM").unwrap()).unwrap(),
        )
        .unwrap();
        let lz2 = RomImage::from_bytes(lz2_bytes.clone()).unwrap();
        assert_eq!(detect_identity(&lz2).unwrap().mapper, Mapper::ExLoRom);
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&lz2).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Original
        );
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&lz3_oracle).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        let expected = load_mapper_graphics(&lz2, Mapper::ExLoRom, GraphicsCompression::Lz2);
        assert_eq!(
            load_mapper_graphics(&lz3_oracle, Mapper::ExLoRom, GraphicsCompression::Lz3),
            expected
        );

        let replacement = smw_us_v1_compact_graphics_compression_migration_plan(
            &lz2,
            0x7fdc,
            SmwUsV1GraphicsCompressionMode::Lz3,
        )
        .unwrap();
        assert_eq!(replacement.plan.mapper, Mapper::ExLoRom);
        assert!(
            replacement
                .plan
                .writes
                .iter()
                .all(|write| { write.offset >= 0x40_0000 || write.offset >= 0x7f_0000 })
        );
        let inactive_mirror = lz2.clone();
        let mut project = Project::new(lz2);
        project
            .replace_relocatable_patch(&replacement.plan, &replacement.obsolete, 0xff)
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        let identity = detect_identity(&project.rom).unwrap();
        assert!(
            identity.checksum_matches(),
            "stored={:?} computed={:?}",
            identity.stored_checksum,
            identity.computed_checksum
        );
        assert_eq!(
            load_mapper_graphics(&project.rom, Mapper::ExLoRom, GraphicsCompression::Lz3),
            expected
        );
        for range in [
            SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET
                ..SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET + 5,
            SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET
                ..SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET + 1,
            crate::SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET
                ..crate::SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET
                    + crate::SMW_US_V1_VANILLA_GRAPHICS_FILES,
        ] {
            assert_eq!(
                &project.rom.logical_bytes()[range.clone()],
                &inactive_mirror.logical_bytes()[range],
                "Rust rewrote the inactive ExLoROM compatibility mirror"
            );
        }

        let reverse = smw_us_v1_compact_graphics_compression_migration_plan(
            &project.rom,
            0x7fdc,
            SmwUsV1GraphicsCompressionMode::Lz2Original,
        )
        .unwrap();
        project
            .replace_relocatable_patch(&reverse.plan, &reverse.obsolete, 0xff)
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Original
        );
        assert_eq!(
            load_mapper_graphics(&project.rom, Mapper::ExLoRom, GraphicsCompression::Lz2),
            expected
        );
        project.history.undo(&mut project.rom).unwrap();
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), lz2_bytes);
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack LZ2-Speed/LZ3 captures"]
    fn sa1_codec_migration_preserves_all_standard_graphics_and_undoes() {
        let source_bytes = fs::read(std::env::var_os("LM_SA1_LZ2_SPEED_ROM").unwrap()).unwrap();
        let source = RomImage::from_bytes(source_bytes.clone()).unwrap();
        let oracle =
            RomImage::from_bytes(fs::read(std::env::var_os("LM_SA1_LZ3_ROM").unwrap()).unwrap())
                .unwrap();
        assert_eq!(detect_identity(&source).unwrap().mapper, Mapper::Sa1);
        assert!(matches!(
            detect_smw_us_v1_graphics_compression_mode(&source).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Original | SmwUsV1GraphicsCompressionMode::Lz2Speed
        ));
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&oracle).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        let expected = load_mapper_graphics(&source, Mapper::Sa1, GraphicsCompression::Lz2);
        assert_eq!(
            load_mapper_graphics(&oracle, Mapper::Sa1, GraphicsCompression::Lz3),
            expected
        );
        let replacement = smw_us_v1_compact_graphics_compression_migration_plan(
            &source,
            0x7fdc,
            SmwUsV1GraphicsCompressionMode::Lz3,
        )
        .unwrap();
        assert_eq!(replacement.plan.mapper, Mapper::Sa1);
        let mut project = Project::new(source);
        project
            .replace_relocatable_patch(&replacement.plan, &replacement.obsolete, 0xff)
            .unwrap();
        assert_eq!(project.rom.logical_len(), 0x20_0000);
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        assert!(detect_identity(&project.rom).unwrap().checksum_matches());
        assert_eq!(
            load_mapper_graphics(&project.rom, Mapper::Sa1, GraphicsCompression::Lz3),
            expected
        );
        if let Some(path) = std::env::var_os("LM_SA1_LZ3_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), source_bytes);
    }

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
        let source_events = crate::load_smw_us_v1_event_tilemaps(&source_project).unwrap();
        let plan = smw_us_v1_lz3_installation_plan(
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
        assert_eq!(project.rom.logical_len(), 0x20_0000);
        assert_eq!(
            project.rom.logical_bytes()[crate::smw_us_v1_exgraphics::SMW_US_V1_ROM_SIZE_OFFSET],
            0x0b
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
        let target_events = crate::load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(target_events.buffers, source_events.buffers);
        assert_eq!(
            target_events.storage,
            match source_events.storage {
                crate::SmwUsV1EventTilemapStorage::Pristine => {
                    crate::SmwUsV1EventTilemapStorage::Pristine
                }
                crate::SmwUsV1EventTilemapStorage::Installed(_) => {
                    crate::SmwUsV1EventTilemapStorage::Installed(
                        lm_project::EventTilemapCompression::Lz3,
                    )
                }
            }
        );
        if let Some(path) = std::env::var_os("LM_LZ3_STANDARD_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), original);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 Fast-LoROM LZ2/LZ3 conversion oracles"]
    fn fast_lorom_lz3_migration_matches_across_copier_header_variants() {
        let oracle = RomImage::from_bytes(
            fs::read(std::env::var_os("LM_FAST_LZ3_ORACLE_ROM").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(oracle.logical_bytes()[0x7fd5], 0x30);
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&oracle).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz3
        );
        let oracle_project = Project::new(oracle.clone());
        let mut oracle_ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout();
        oracle_ordinary_layout.compression = GraphicsCompression::Lz3;
        let oracle_ordinary = (0..oracle_ordinary_layout.pointers.entries)
            .map(|slot| {
                oracle_project
                    .load_graphics_file(slot, oracle_ordinary_layout)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let mut oracle_special = crate::smw_us_v1_special_graphics_layouts(&oracle).unwrap();
        oracle_special.gfx33.compression = GraphicsCompression::Lz3;
        oracle_special.gfx32.compression = GraphicsCompression::Lz3;
        let oracle_gfx33 = oracle_project
            .load_graphics_file(0, oracle_special.gfx33)
            .unwrap();
        let oracle_gfx32 = oracle_project
            .load_graphics_file(0, oracle_special.gfx32)
            .unwrap();

        let mut logical_result = None;
        for (variable, expected_header) in [
            ("LM_FAST_LZ2_HEADERLESS_ROM", lm_rom::CopierHeader::Absent),
            ("LM_FAST_LZ2_HEADERED_ROM", lm_rom::CopierHeader::Present),
        ] {
            let original = fs::read(std::env::var_os(variable).unwrap()).unwrap();
            let image = RomImage::from_bytes(original.clone()).unwrap();
            assert_eq!(image.copier_header(), expected_header);
            assert_eq!(image.logical_bytes()[0x7fd5], 0x30);
            assert_eq!(
                detect_smw_us_v1_graphics_compression_mode(&image).unwrap(),
                SmwUsV1GraphicsCompressionMode::Lz2Original
            );
            let plan = smw_us_v1_lz3_installation_plan(
                &image,
                AllocationPolicy::lorom(0x10_0000..0x20_0000),
                0x7fdc,
            )
            .unwrap();
            let mut project = Project::new(image);
            project.install_relocatable_patch(&plan).unwrap();
            assert_eq!(project.rom.copier_header(), expected_header);
            assert_eq!(project.rom.logical_bytes()[0x7fd5], 0x30);
            assert!(
                lm_rom::detect_identity(&project.rom)
                    .unwrap()
                    .checksum_matches()
            );
            assert_eq!(
                detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
                SmwUsV1GraphicsCompressionMode::Lz3
            );
            let mut target_layout = crate::smw_us_v1_vanilla_graphics_layout();
            target_layout.compression = GraphicsCompression::Lz3;
            for (slot, expected) in oracle_ordinary.iter().enumerate() {
                assert_eq!(
                    project.load_graphics_file(slot, target_layout).unwrap(),
                    *expected,
                    "{variable} GFX{slot:02X}"
                );
            }
            let mut target_special =
                crate::smw_us_v1_special_graphics_layouts(&project.rom).unwrap();
            target_special.gfx33.compression = GraphicsCompression::Lz3;
            target_special.gfx32.compression = GraphicsCompression::Lz3;
            assert_eq!(
                project.load_graphics_file(0, target_special.gfx33).unwrap(),
                oracle_gfx33,
                "{variable} GFX33"
            );
            assert_eq!(
                project.load_graphics_file(0, target_special.gfx32).unwrap(),
                oracle_gfx32,
                "{variable} GFX32"
            );
            match &logical_result {
                Some(expected) => assert_eq!(project.rom.logical_bytes(), expected),
                None => logical_result = Some(project.rom.logical_bytes().to_vec()),
            }
            project.history.undo(&mut project.rom).unwrap();
            assert_eq!(project.rom.as_file_bytes(), original);
        }
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ3 installed-graphics ROM"]
    fn standard_lz2_original_reversal_preserves_all_52_graphics_and_undoes() {
        let original = fs::read(std::env::var_os("LM_LZ3_ROM").unwrap()).unwrap();
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let logical_len = image.logical_len();
        let source_project = Project::new(image.clone());
        let mut ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout();
        ordinary_layout.compression = GraphicsCompression::Lz3;
        let ordinary = (0..ordinary_layout.pointers.entries)
            .map(|slot| {
                source_project
                    .load_graphics_file(slot, ordinary_layout)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let mut special = crate::smw_us_v1_special_graphics_layouts(&image).unwrap();
        special.gfx33.compression = GraphicsCompression::Lz3;
        special.gfx32.compression = GraphicsCompression::Lz3;
        let gfx33 = source_project.load_graphics_file(0, special.gfx33).unwrap();
        let gfx32 = source_project.load_graphics_file(0, special.gfx32).unwrap();
        let source_events = crate::load_smw_us_v1_event_tilemaps(&source_project).unwrap();
        let replacement = smw_us_v1_compact_graphics_compression_migration_plan(
            &image,
            0x7fdc,
            SmwUsV1GraphicsCompressionMode::Lz2Original,
        )
        .unwrap();
        let mut project = Project::new(image);
        project
            .replace_relocatable_patch(&replacement.plan, &replacement.obsolete, 0xff)
            .unwrap();
        assert_eq!(project.rom.logical_len(), logical_len);
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Original
        );
        let mut target_layout = ordinary_layout;
        target_layout.compression = GraphicsCompression::Lz2;
        for (slot, expected) in ordinary.iter().enumerate() {
            assert_eq!(
                project.load_graphics_file(slot, target_layout).unwrap(),
                *expected
            );
        }
        let mut target_special = crate::smw_us_v1_special_graphics_layouts(&project.rom).unwrap();
        target_special.gfx33.compression = GraphicsCompression::Lz2;
        target_special.gfx32.compression = GraphicsCompression::Lz2;
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx33).unwrap(),
            gfx33
        );
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx32).unwrap(),
            gfx32
        );
        let target_events = crate::load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(target_events.buffers, source_events.buffers);
        assert_eq!(
            target_events.storage,
            match source_events.storage {
                crate::SmwUsV1EventTilemapStorage::Pristine => {
                    crate::SmwUsV1EventTilemapStorage::Pristine
                }
                crate::SmwUsV1EventTilemapStorage::Installed(_) => {
                    crate::SmwUsV1EventTilemapStorage::Installed(
                        lm_project::EventTilemapCompression::Lz2,
                    )
                }
            }
        );
        if let Some(path) = std::env::var_os("LM_LZ2_REVERSE_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), original);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ3 installed-graphics ROM"]
    fn direct_lz2_speed_migration_preserves_all_52_graphics_and_undoes() {
        let original = fs::read(std::env::var_os("LM_LZ3_ROM").unwrap()).unwrap();
        let image = RomImage::from_bytes(original.clone()).unwrap();
        let source = Project::new(image.clone());
        let mut ordinary_layout = crate::smw_us_v1_vanilla_graphics_layout();
        ordinary_layout.compression = GraphicsCompression::Lz3;
        let ordinary = (0..ordinary_layout.pointers.entries)
            .map(|slot| source.load_graphics_file(slot, ordinary_layout).unwrap())
            .collect::<Vec<_>>();
        let mut special = crate::smw_us_v1_special_graphics_layouts(&image).unwrap();
        special.gfx33.compression = GraphicsCompression::Lz3;
        special.gfx32.compression = GraphicsCompression::Lz3;
        let gfx33 = source.load_graphics_file(0, special.gfx33).unwrap();
        let gfx32 = source.load_graphics_file(0, special.gfx32).unwrap();
        let replacement = smw_us_v1_compact_graphics_compression_migration_plan(
            &image,
            0x7fdc,
            SmwUsV1GraphicsCompressionMode::Lz2Speed,
        )
        .unwrap();
        let mut project = Project::new(image);
        project
            .replace_relocatable_patch(&replacement.plan, &replacement.obsolete, 0xff)
            .unwrap();
        assert_eq!(
            detect_smw_us_v1_graphics_compression_mode(&project.rom).unwrap(),
            SmwUsV1GraphicsCompressionMode::Lz2Speed
        );
        let mut target_layout = ordinary_layout;
        target_layout.compression = GraphicsCompression::Lz2;
        for (slot, expected) in ordinary.iter().enumerate() {
            assert_eq!(
                project.load_graphics_file(slot, target_layout).unwrap(),
                *expected
            );
        }
        let mut target_special = crate::smw_us_v1_special_graphics_layouts(&project.rom).unwrap();
        target_special.gfx33.compression = GraphicsCompression::Lz2;
        target_special.gfx32.compression = GraphicsCompression::Lz2;
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx33).unwrap(),
            gfx33
        );
        assert_eq!(
            project.load_graphics_file(0, target_special.gfx32).unwrap(),
            gfx32
        );
        if let Some(path) = std::env::var_os("LM_LZ2_SPEED_DIRECT_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), original);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ2-Orig ROM with ExGFX80"]
    fn lz3_component_recompresses_populated_exgfx_and_preserves_empty_slots() {
        let original = fs::read(std::env::var_os("LM_LZ2_EXGFX_ROM").unwrap()).unwrap();
        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let before = project.rom.as_file_bytes().to_vec();
        let route = crate::smw_us_v1_exgraphics_pointer(0x80).unwrap();
        let source_layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: route.pointer_offset,
                entries: 1,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x1000,
        };
        let raw = project
            .load_decompressed_graphics_file(0, source_layout)
            .unwrap();
        let empty_route = crate::smw_us_v1_exgraphics_pointer(0x81).unwrap();
        let empty_pointer = project
            .rom
            .read(empty_route.pointer_offset, 3)
            .unwrap()
            .to_vec();
        let plan = smw_us_v1_lz3_installation_plan(
            &project.rom,
            AllocationPolicy::lorom(0x18_0000..0x20_0000),
            0x7fdc,
        )
        .unwrap();
        project.install_relocatable_patch(&plan).unwrap();
        let target_layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: route.pointer_offset,
                entries: 1,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz3,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x1000,
        };
        assert_eq!(
            project
                .load_decompressed_graphics_file(0, target_layout)
                .unwrap(),
            raw
        );
        assert_eq!(
            project.rom.read(empty_route.pointer_offset, 3).unwrap(),
            empty_pointer
        );
        if let Some(path) = std::env::var_os("LM_LZ3_EXGFX_RUST_OUTPUT") {
            fs::write(path, project.rom.as_file_bytes()).unwrap();
        }
        let lz3 = project.rom.as_file_bytes().to_vec();
        let reverse = smw_us_v1_lz2_original_installation_plan(
            &project.rom,
            AllocationPolicy::lorom(project.rom.logical_len()..0x40_0000),
            0x7fdc,
        )
        .unwrap();
        project.install_relocatable_patch(&reverse).unwrap();
        let target_layout = GraphicsRomLayout {
            compression: GraphicsCompression::Lz2,
            ..target_layout
        };
        assert_eq!(
            project
                .load_decompressed_graphics_file(0, target_layout)
                .unwrap(),
            raw
        );
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), lz3);
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), before);
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ2-Orig installed-graphics ROM"]
    fn lz3_component_recompresses_both_installed_overworld_event_streams() {
        let original = fs::read(std::env::var_os("LM_LZ2_ORIGINAL_ROM").unwrap()).unwrap();
        let mut project = Project::new(RomImage::from_bytes(original).unwrap());
        let mut buffers = lm_overworld::EventTilemapBuffers::default();
        buffers.primary_bytes_mut()[0x123] = 0x45;
        buffers.primary_bytes_mut()[0x923] = 0x67;
        buffers.secondary_high_bytes_mut()[0x321] = 0x89;
        let event_plan = crate::smw_us_v1_event_tilemap_installation_plan(
            &buffers,
            lm_project::EventTilemapCompression::Lz2,
        );
        project
            .install_event_tilemap_buffers(
                &buffers,
                crate::smw_us_v1_event_tilemap_locator(),
                lm_project::EventTilemapCompression::Lz2,
                &event_plan,
            )
            .unwrap();
        let before = project.rom.as_file_bytes().to_vec();
        let plan = smw_us_v1_lz3_installation_plan(
            &project.rom,
            AllocationPolicy::lorom(0x10_0000..0x20_0000),
            0x7fdc,
        )
        .unwrap();
        project.install_relocatable_patch(&plan).unwrap();
        let loaded = crate::load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(loaded.buffers, buffers);
        assert_eq!(
            loaded.storage,
            crate::SmwUsV1EventTilemapStorage::Installed(lm_project::EventTilemapCompression::Lz3)
        );
        let lz3 = project.rom.as_file_bytes().to_vec();
        let reverse = smw_us_v1_lz2_original_installation_plan(
            &project.rom,
            AllocationPolicy::lorom(project.rom.logical_len()..0x40_0000),
            0x7fdc,
        )
        .unwrap();
        project.install_relocatable_patch(&reverse).unwrap();
        let loaded = crate::load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(loaded.buffers, buffers);
        assert_eq!(
            loaded.storage,
            crate::SmwUsV1EventTilemapStorage::Installed(lm_project::EventTilemapCompression::Lz2)
        );
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), lz3);
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), before);
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
