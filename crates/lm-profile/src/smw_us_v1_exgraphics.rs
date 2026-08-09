//! Native Lunar Magic ExGFX pointer domains for SMW US revision 0.

use lm_codec::encode_lz2;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomError, RomImage};

pub const SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET: usize = 0x001471;
pub const SMW_US_V1_EXGFX_RUNTIME_HOOK: [u8; 5] = [0x22, 0xc0, 0xf9, 0x0f, 0xea];
pub const SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET: usize = 0x07d274;
pub const SMW_US_V1_EXGFX_TABLE_BASE_OPERAND: [u8; 3] = [0xcd, 0xff, 0x10];
pub const SMW_US_V1_RESERVED_EXGFX_POINTER_OFFSET: usize = 0x01bcc0;
pub const SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET: usize = 0x07f600;
pub const SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET: usize = 0x088000;
pub const SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET: usize = 0x07efb1;
pub const SMW_US_V1_EXGFX_EXPANSION_MARKER: [u8; 7] = [0x72, 0, 0, 0, 0, 0, 0];
pub const SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET: usize = 0x002a47;
pub const SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER: [u8; 2] = [0xea, 0xea];
pub const SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER: [u8; 2] = [0xf0, 0x03];
pub const SMW_US_V1_RESERVED_EXGFX_MARKER_OFFSET: usize = 0x07efb6;
pub const SMW_US_V1_RESERVED_EXGFX_MARKER: [u8; 2] = [0x22, 0];
pub const SMW_US_V1_ROM_SIZE_OFFSET: usize = 0x007fd7;
pub const SMW_US_V1_EXGFX_LOGICAL_LEN: usize = 0x20_0000;
pub const SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS: [usize; 2] = [0x002a8d, 0x002a91];
pub const SMW_US_V1_4BPP_GRAPHICS_MARKER: u8 = 0x32;

/// Reports whether Lunar Magic's regular-GFX conversion prerequisite is present.
///
/// Lunar Magic's `CheckGraphicsExtractionFormatMarker` reads the recovered `0x32` format marker.
/// The companion marker is checked as well so unrelated vanilla data cannot enable first-time
/// ExGFX insertion.
pub fn has_smw_us_v1_4bpp_graphics_prerequisite(rom: &RomImage) -> bool {
    SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS.iter().all(|offset| {
        rom.read(*offset, 1)
            .is_ok_and(|bytes| bytes == [SMW_US_V1_4BPP_GRAPHICS_MARKER])
    })
}

/// Reports the exact legacy ExGFX state that makes Lunar Magic warn before inserting regular GFX.
///
/// Lunar Magic 3.63 shows its `Graphics Format Change Warning!` only when an authenticated ExGFX
/// runtime is present while the two regular-GFX format markers still identify the old 3bpp
/// representation. Keeping this predicate separate from the insertion transaction prevents a
/// merely expanded ROM, a partial marker, or foreign bytes from enabling the destructive migration
/// prompt.
pub fn requires_smw_us_v1_4bpp_graphics_warning(rom: &RomImage) -> bool {
    !has_smw_us_v1_4bpp_graphics_prerequisite(rom)
        && probe_smw_us_v1_exgraphics_runtime(rom).is_ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1ExGraphicsEncoding {
    Raw2048,
    Lz2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmwUsV1ExGraphicsPointer {
    pub file_number: u16,
    pub pointer_offset: usize,
    pub encoding: SmwUsV1ExGraphicsEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1ExGraphicsRuntimeState {
    Ready,
    Expanded,
    ReservedOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1ExGraphicsError {
    Rom(RomError),
    UnsupportedRuntimeHook,
    UnsupportedTableBase,
    UnsupportedExpansionMarker([u8; 7]),
    UnsupportedGraphicsFormatMarker([u8; 2]),
    FileNumber(u16),
    PointerOffsetOverflow,
    EmptyFiles,
    MixedEncoding,
    DuplicateFileNumber(u16),
    InvalidRawLength { file_number: u16, actual: usize },
    InvalidReservedLength { file_number: u16, actual: usize },
}

impl std::fmt::Display for SmwUsV1ExGraphicsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SMW US v1 ExGFX runtime failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1ExGraphicsError {}

impl From<RomError> for SmwUsV1ExGraphicsError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Authenticates the shared expanded-settings hook and native extended-table relocation operand.
///
/// # Errors
///
/// Rejects truncated images, altered hooks, foreign relocation operands, and unknown installation
/// markers instead of treating unrelated ROM data as Lunar Magic's ExGFX runtime.
pub fn probe_smw_us_v1_exgraphics_runtime(
    rom: &RomImage,
) -> Result<SmwUsV1ExGraphicsRuntimeState, SmwUsV1ExGraphicsError> {
    if rom.read(
        SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET,
        SMW_US_V1_EXGFX_RUNTIME_HOOK.len(),
    )? != SMW_US_V1_EXGFX_RUNTIME_HOOK
    {
        return Err(SmwUsV1ExGraphicsError::UnsupportedRuntimeHook);
    }
    if rom.read(
        SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET,
        SMW_US_V1_EXGFX_TABLE_BASE_OPERAND.len(),
    )? != SMW_US_V1_EXGFX_TABLE_BASE_OPERAND
    {
        return Err(SmwUsV1ExGraphicsError::UnsupportedTableBase);
    }
    let marker: [u8; 7] = rom
        .read(
            SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET,
            SMW_US_V1_EXGFX_EXPANSION_MARKER.len(),
        )?
        .try_into()
        .expect("the exact marker length was requested");
    if marker == SMW_US_V1_EXGFX_EXPANSION_MARKER {
        let format_marker: [u8; 2] = rom
            .read(
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER.len(),
            )?
            .try_into()
            .expect("the exact graphics-format marker length was requested");
        if format_marker != SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER {
            return Err(SmwUsV1ExGraphicsError::UnsupportedGraphicsFormatMarker(
                format_marker,
            ));
        }
        return Ok(SmwUsV1ExGraphicsRuntimeState::Expanded);
    }
    if marker == [0xff, 0xff, 0xff, 0xff, 0xff, 0x22, 0] {
        let format_marker: [u8; 2] = rom
            .read(
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER.len(),
            )?
            .try_into()
            .expect("the exact graphics-format marker length was requested");
        if format_marker != SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER {
            return Err(SmwUsV1ExGraphicsError::UnsupportedGraphicsFormatMarker(
                format_marker,
            ));
        }
        return Ok(SmwUsV1ExGraphicsRuntimeState::ReservedOnly);
    }
    if marker == [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x1f] {
        let format_marker: [u8; 2] = rom
            .read(
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER.len(),
            )?
            .try_into()
            .expect("the exact graphics-format marker length was requested");
        if format_marker != SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER {
            return Err(SmwUsV1ExGraphicsError::UnsupportedGraphicsFormatMarker(
                format_marker,
            ));
        }
        return Ok(SmwUsV1ExGraphicsRuntimeState::Ready);
    }
    Err(SmwUsV1ExGraphicsError::UnsupportedExpansionMarker(marker))
}

/// Resolves Lunar Magic's three disjoint native ExGFX pointer domains.
///
/// # Errors
///
/// Rejects file numbers outside reserved `$60..$63` and ordinary `$80..$FFF`, plus arithmetic
/// overflow while deriving an exact three-byte pointer entry.
pub fn smw_us_v1_exgraphics_pointer(
    file_number: u16,
) -> Result<SmwUsV1ExGraphicsPointer, SmwUsV1ExGraphicsError> {
    let (base, index, encoding) = match file_number {
        0x60..=0x63 => (
            SMW_US_V1_RESERVED_EXGFX_POINTER_OFFSET,
            usize::from(file_number - 0x60),
            SmwUsV1ExGraphicsEncoding::Raw2048,
        ),
        0x80..=0xff => (
            SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET,
            usize::from(file_number - 0x80),
            SmwUsV1ExGraphicsEncoding::Lz2,
        ),
        0x100..=0xfff => (
            SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET,
            usize::from(file_number - 0x100),
            SmwUsV1ExGraphicsEncoding::Lz2,
        ),
        _ => return Err(SmwUsV1ExGraphicsError::FileNumber(file_number)),
    };
    let pointer_offset = index
        .checked_mul(3)
        .and_then(|delta| base.checked_add(delta))
        .ok_or(SmwUsV1ExGraphicsError::PointerOffsetOverflow)?;
    Ok(SmwUsV1ExGraphicsPointer {
        file_number,
        pointer_offset,
        encoding,
    })
}

/// Builds one relocatable native ExGFX insertion plan for a single encoding domain.
///
/// Reserved `$60..$63` files remain raw 0x800-byte RATS payloads. Ordinary `$80..$FFF` files are
/// LZ2-compressed. Lunar Magic allocates these domains separately, so callers stage two plans when
/// a directory contains both and publish the combined ROM mutation atomically.
///
/// # Errors
///
/// Rejects an unauthenticated runtime, empty/mixed/duplicate sets, malformed native depths, and
/// truncated pointer or marker storage.
pub fn smw_us_v1_exgraphics_installation_plan(
    rom: &RomImage,
    files: &[(u16, Vec<u8>)],
) -> Result<RelocatablePatchPlan, SmwUsV1ExGraphicsError> {
    let state = probe_smw_us_v1_exgraphics_runtime(rom)?;
    if files.is_empty() {
        return Err(SmwUsV1ExGraphicsError::EmptyFiles);
    }
    let mut ordered = files.to_vec();
    ordered.sort_unstable_by_key(|(file_number, _)| *file_number);
    if let Some(pair) = ordered.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(SmwUsV1ExGraphicsError::DuplicateFileNumber(pair[0].0));
    }
    let routes = ordered
        .iter()
        .map(|(file_number, _)| smw_us_v1_exgraphics_pointer(*file_number))
        .collect::<Result<Vec<_>, _>>()?;
    let encoding = routes[0].encoding;
    if routes.iter().any(|route| route.encoding != encoding) {
        return Err(SmwUsV1ExGraphicsError::MixedEncoding);
    }
    let payloads = ordered
        .iter()
        .zip(&routes)
        .map(|((file_number, raw), route)| match route.encoding {
            SmwUsV1ExGraphicsEncoding::Raw2048 => {
                if raw.len() != 0x800 {
                    return Err(SmwUsV1ExGraphicsError::InvalidReservedLength {
                        file_number: *file_number,
                        actual: raw.len(),
                    });
                }
                Ok(PatchPayload {
                    bytes: raw.clone(),
                    fixups: Vec::new(),
                })
            }
            SmwUsV1ExGraphicsEncoding::Lz2 => {
                if !matches!(raw.len(), 0x800 | 0xc00 | 0x1000) {
                    return Err(SmwUsV1ExGraphicsError::InvalidRawLength {
                        file_number: *file_number,
                        actual: raw.len(),
                    });
                }
                Ok(PatchPayload {
                    bytes: encode_lz2(raw),
                    fixups: Vec::new(),
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut writes = Vec::with_capacity(routes.len() + 2);
    if rom.logical_len() < SMW_US_V1_EXGFX_LOGICAL_LEN {
        writes.push(PatchWrite {
            offset: SMW_US_V1_ROM_SIZE_OFFSET,
            expected: rom.read(SMW_US_V1_ROM_SIZE_OFFSET, 1)?.to_vec(),
            replacement: vec![0x0b],
            fixups: Vec::new(),
        });
    }
    match (encoding, state) {
        (SmwUsV1ExGraphicsEncoding::Lz2, SmwUsV1ExGraphicsRuntimeState::Ready)
        | (SmwUsV1ExGraphicsEncoding::Lz2, SmwUsV1ExGraphicsRuntimeState::ReservedOnly) => {
            writes.push(marker_write(
                rom,
                SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET,
                &SMW_US_V1_EXGFX_EXPANSION_MARKER,
            )?);
            writes.push(marker_write(
                rom,
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                &SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER,
            )?);
        }
        (SmwUsV1ExGraphicsEncoding::Raw2048, SmwUsV1ExGraphicsRuntimeState::Ready) => {
            writes.push(marker_write(
                rom,
                SMW_US_V1_RESERVED_EXGFX_MARKER_OFFSET,
                &SMW_US_V1_RESERVED_EXGFX_MARKER,
            )?);
        }
        _ => {}
    }
    let initializes_compressed_tables = encoding == SmwUsV1ExGraphicsEncoding::Lz2
        && matches!(
            state,
            SmwUsV1ExGraphicsRuntimeState::Ready | SmwUsV1ExGraphicsRuntimeState::ReservedOnly
        );
    if initializes_compressed_tables {
        let mut ordinary = PatchWrite {
            offset: SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET,
            expected: rom
                .read(SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET, 0x80 * 3)?
                .to_vec(),
            replacement: vec![0; 0x80 * 3],
            fixups: Vec::new(),
        };
        let mut extended = PatchWrite {
            offset: SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET,
            expected: rom
                .read(SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET, 0xf00 * 3)?
                .to_vec(),
            replacement: vec![0; 0xf00 * 3],
            fixups: Vec::new(),
        };
        for (index, route) in routes.iter().enumerate() {
            let (table, offset) = if route.file_number <= 0xff {
                (&mut ordinary, usize::from(route.file_number - 0x80) * 3)
            } else {
                (&mut extended, usize::from(route.file_number - 0x100) * 3)
            };
            table.fixups.push(PatchFixup {
                offset,
                target_payload: index,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24LowBank,
            });
        }
        writes.push(ordinary);
        writes.push(extended);
    }
    for (index, route) in routes.iter().enumerate() {
        if initializes_compressed_tables {
            continue;
        }
        writes.push(PatchWrite {
            offset: route.pointer_offset,
            expected: rom.read(route.pointer_offset, 3)?.to_vec(),
            replacement: vec![0; 3],
            fixups: vec![PatchFixup {
                offset: 0,
                target_payload: index,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24LowBank,
            }],
        });
    }
    writes.sort_unstable_by_key(|write| write.offset);
    Ok(RelocatablePatchPlan {
        description: "insert native SMW US ExGFX files".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: match encoding {
                SmwUsV1ExGraphicsEncoding::Raw2048 => 0x08_0028..SMW_US_V1_EXGFX_LOGICAL_LEN,
                SmwUsV1ExGraphicsEncoding::Lz2 => 0x0f_fff8..SMW_US_V1_EXGFX_LOGICAL_LEN,
            },
            bank_size: None,
            fill_bytes: vec![0x00, 0xff],
            protected: Vec::new(),
        },
        checksum_field: 0x007fdc,
        expansion_fill: 0x00,
        payloads,
        writes,
    })
}

fn marker_write(
    rom: &RomImage,
    offset: usize,
    replacement: &[u8],
) -> Result<PatchWrite, SmwUsV1ExGraphicsError> {
    Ok(PatchWrite {
        offset,
        expected: rom.read(offset, replacement.len())?.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;

    #[test]
    fn native_pointer_domains_match_the_three_recovered_tables() {
        assert_eq!(
            smw_us_v1_exgraphics_pointer(0x60).unwrap(),
            SmwUsV1ExGraphicsPointer {
                file_number: 0x60,
                pointer_offset: 0x01bcc0,
                encoding: SmwUsV1ExGraphicsEncoding::Raw2048,
            }
        );
        assert_eq!(
            smw_us_v1_exgraphics_pointer(0x80).unwrap().pointer_offset,
            0x07f600
        );
        assert_eq!(
            smw_us_v1_exgraphics_pointer(0xff).unwrap().pointer_offset,
            0x07f77d
        );
        assert_eq!(
            smw_us_v1_exgraphics_pointer(0x100).unwrap().pointer_offset,
            0x088000
        );
        assert_eq!(
            smw_us_v1_exgraphics_pointer(0xfff).unwrap().pointer_offset,
            0x08acfd
        );
        for invalid in [0x00, 0x5f, 0x64, 0x7f, 0x1000] {
            assert!(matches!(
                smw_us_v1_exgraphics_pointer(invalid),
                Err(SmwUsV1ExGraphicsError::FileNumber(value)) if value == invalid
            ));
        }
    }

    #[test]
    fn probe_distinguishes_ready_reserved_and_expanded_states() {
        let mut bytes = vec![0xff; 0x09_0000];
        bytes[SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET..SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET + 5]
            .copy_from_slice(&SMW_US_V1_EXGFX_RUNTIME_HOOK);
        bytes[SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET
            ..SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET + 3]
            .copy_from_slice(&SMW_US_V1_EXGFX_TABLE_BASE_OPERAND);
        bytes[SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET + 6] = 0x1f;
        bytes[SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET
            ..SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET + 2]
            .copy_from_slice(&SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER);
        let mut image = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            probe_smw_us_v1_exgraphics_runtime(&image).unwrap(),
            SmwUsV1ExGraphicsRuntimeState::Ready
        );
        image
            .write(
                SMW_US_V1_RESERVED_EXGFX_MARKER_OFFSET,
                &SMW_US_V1_RESERVED_EXGFX_MARKER,
            )
            .unwrap();
        assert_eq!(
            probe_smw_us_v1_exgraphics_runtime(&image).unwrap(),
            SmwUsV1ExGraphicsRuntimeState::ReservedOnly
        );
        image
            .write(
                SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET,
                &SMW_US_V1_EXGFX_EXPANSION_MARKER,
            )
            .unwrap();
        image
            .write(
                SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
                &SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER,
            )
            .unwrap();
        assert_eq!(
            probe_smw_us_v1_exgraphics_runtime(&image).unwrap(),
            SmwUsV1ExGraphicsRuntimeState::Expanded
        );
    }

    #[test]
    fn four_bpp_prerequisite_requires_both_recovered_markers() {
        let mut image = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        assert!(!has_smw_us_v1_4bpp_graphics_prerequisite(&image));
        image
            .write(SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS[0], &[0x32])
            .unwrap();
        assert!(!has_smw_us_v1_4bpp_graphics_prerequisite(&image));
        image
            .write(SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS[1], &[0x32])
            .unwrap();
        assert!(has_smw_us_v1_4bpp_graphics_prerequisite(&image));
    }

    #[test]
    fn format_change_warning_requires_authenticated_exgfx_and_old_graphics_markers() {
        let mut image = ready_image();
        assert!(requires_smw_us_v1_4bpp_graphics_warning(&image));

        image
            .write(SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS[0], &[0x32])
            .unwrap();
        assert!(requires_smw_us_v1_4bpp_graphics_warning(&image));
        image
            .write(SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS[1], &[0x32])
            .unwrap();
        assert!(!requires_smw_us_v1_4bpp_graphics_warning(&image));

        let mut foreign = ready_image();
        foreign
            .write(SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET, &[0xff; 5])
            .unwrap();
        assert!(!requires_smw_us_v1_4bpp_graphics_warning(&foreign));
    }

    fn ready_image() -> RomImage {
        let mut bytes = vec![0xff; 0x10_0000];
        bytes[SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET..SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET + 5]
            .copy_from_slice(&SMW_US_V1_EXGFX_RUNTIME_HOOK);
        bytes[SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET
            ..SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET + 3]
            .copy_from_slice(&SMW_US_V1_EXGFX_TABLE_BASE_OPERAND);
        bytes[SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET + 6] = 0x1f;
        bytes[SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET
            ..SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET + 2]
            .copy_from_slice(&SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER);
        bytes[SMW_US_V1_ROM_SIZE_OFFSET] = 0x0a;
        bytes[0x080000..0x080008].copy_from_slice(b"STAR\x1f\0\xe0\xff");
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn ordinary_first_install_matches_recovered_expansion_pointer_and_marker() {
        let before = ready_image();
        let original = before.logical_bytes().to_vec();
        let plan =
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x80, vec![0; 0x800])]).unwrap();
        let mut project = Project::new(before);
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks[0].header_offset, 0x0f_fff8);
        assert_eq!(result.blocks[0].payload, 0x10_0000..0x10_0007);
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET, 3)
                .unwrap(),
            [0x00, 0x80, 0x20]
        );
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET, 7)
                .unwrap(),
            SMW_US_V1_EXGFX_EXPANSION_MARKER
        );
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET, 2)
                .unwrap(),
            SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER
        );
        assert!(
            project
                .rom
                .read(SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET + 3, 0x7f * 3)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert!(
            project
                .rom
                .read(SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET, 0xf00 * 3)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(project.rom.logical_len(), SMW_US_V1_EXGFX_LOGICAL_LEN);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn reserved_first_install_keeps_raw_payload_and_disjoint_table() {
        let before = ready_image();
        let plan =
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x60, vec![0; 0x800])]).unwrap();
        let mut project = Project::new(before);
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks[0].header_offset, 0x08_0028);
        assert_eq!(result.blocks[0].payload, 0x08_0030..0x08_0830);
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_RESERVED_EXGFX_POINTER_OFFSET, 3)
                .unwrap(),
            [0x30, 0x80, 0x10]
        );
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_RESERVED_EXGFX_MARKER_OFFSET, 2)
                .unwrap(),
            SMW_US_V1_RESERVED_EXGFX_MARKER
        );
    }

    #[test]
    fn native_install_rejects_mixed_duplicates_and_wrong_shapes() {
        let before = ready_image();
        assert!(matches!(
            smw_us_v1_exgraphics_installation_plan(
                &before,
                &[(0x60, vec![0; 0x800]), (0x80, vec![0; 0x800])]
            ),
            Err(SmwUsV1ExGraphicsError::MixedEncoding)
        ));
        assert!(matches!(
            smw_us_v1_exgraphics_installation_plan(
                &before,
                &[(0x80, vec![0; 0x800]), (0x80, vec![0; 0x800])]
            ),
            Err(SmwUsV1ExGraphicsError::DuplicateFileNumber(0x80))
        ));
        assert!(matches!(
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x60, vec![0; 0x801])]),
            Err(SmwUsV1ExGraphicsError::InvalidReservedLength { .. })
        ));
        assert!(matches!(
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x80, vec![0; 0x801])]),
            Err(SmwUsV1ExGraphicsError::InvalidRawLength { .. })
        ));
    }

    #[test]
    #[ignore = "requires retained pre/post Lunar Magic first-ExGFX ROMs"]
    fn retained_lunar_magic_first_exgfx80_install_is_byte_exact() {
        let before_path = std::env::var_os("LM_EXGFX_READY_ROM")
            .expect("LM_EXGFX_READY_ROM must name the pre-insertion ROM");
        let after_path = std::env::var_os("LM_EXGFX_ORACLE_ROM")
            .expect("LM_EXGFX_ORACLE_ROM must name the Lunar Magic result");
        let before = RomImage::from_bytes(std::fs::read(before_path).unwrap()).unwrap();
        let after = RomImage::from_bytes(std::fs::read(after_path).unwrap()).unwrap();
        let plan =
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x80, vec![0; 0x800])]).unwrap();
        let mut project = Project::new(before);
        project.install_relocatable_patch(&plan).unwrap();
        assert_rom_bytes_equal(project.rom.logical_bytes(), after.logical_bytes());
    }

    #[test]
    #[ignore = "requires retained pre/post Lunar Magic reserved-ExGFX ROMs"]
    fn retained_lunar_magic_first_exgfx60_install_is_byte_exact() {
        let before_path = std::env::var_os("LM_RESERVED_EXGFX_READY_ROM")
            .expect("LM_RESERVED_EXGFX_READY_ROM must name the pre-insertion ROM");
        let after_path = std::env::var_os("LM_RESERVED_EXGFX_ORACLE_ROM")
            .expect("LM_RESERVED_EXGFX_ORACLE_ROM must name the Lunar Magic result");
        let before = RomImage::from_bytes(std::fs::read(before_path).unwrap()).unwrap();
        let after = RomImage::from_bytes(std::fs::read(after_path).unwrap()).unwrap();
        let plan =
            smw_us_v1_exgraphics_installation_plan(&before, &[(0x60, vec![0; 0x800])]).unwrap();
        let mut project = Project::new(before);
        project.install_relocatable_patch(&plan).unwrap();
        assert_rom_bytes_equal(project.rom.logical_bytes(), after.logical_bytes());
    }

    fn assert_rom_bytes_equal(actual: &[u8], expected: &[u8]) {
        assert_eq!(actual.len(), expected.len(), "logical ROM lengths differ");
        let differences = actual
            .iter()
            .zip(expected)
            .enumerate()
            .filter(|(_, (actual, expected))| actual != expected)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "{} bytes differ; first={:#x}, last={:#x}",
            differences.len(),
            differences.first().copied().unwrap_or_default(),
            differences.last().copied().unwrap_or_default()
        );
    }
}
