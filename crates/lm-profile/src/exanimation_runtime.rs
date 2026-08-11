use std::fmt;

/// Core allocation copied by `InstallExpandedExAnimationRuntime` (`0045CAF0`).
pub const EXPANDED_EXANIMATION_RUNTIME_CORE_LEN: usize = 0xc30;
/// Optional mapper-specific suffix copied from executable data `$005B5754`.
pub const EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN: usize = 0x20;
/// Separately allocated 512-entry, three-byte per-level pointer table.
pub const EXPANDED_EXANIMATION_POINTER_TABLE_LEN: usize = 0x600;

pub(crate) const MAPPING_BYTE_OFFSETS: [usize; 2] = [0x5c, 0x66];
pub(crate) const SNES_POINTER_OFFSETS: [usize; 8] =
    [0xdf, 0xea, 0x5b0, 0x601, 0xa7e, 0xaaa, 0xacf, 0xaf4];
pub(crate) const IRAM_WORD_OFFSETS: [usize; 12] = [
    0x550, 0x59b, 0x5bf, 0x5c3, 0x5d6, 0x63e, 0x6c4, 0x79e, 0x7ac, 0x825, 0x833, 0x8b0,
];
pub(crate) const LOCAL_WORD_TABLE_OFFSET: usize = 0xb4a;
pub(crate) const LOCAL_WORD_TABLE_ENTRIES: usize = 108;
pub(crate) const TEMPLATE_LOCAL_WORD_BASE: u16 = 0x8000;
pub const OPTIONAL_SUFFIX_POINTER_OFFSET: usize = 0x78a;
pub const OPTIONAL_MAPPING_HELPER_POINTER_OFFSET: usize = 0x792;
pub const OPTIONAL_MAPPING_HELPER_SNES_ADDRESS: u32 = 0x7f_c020;
const MAPPER_IRAM_WORD_OFFSETS: [usize; 37] = [
    0x15b, 0x15e, 0x161, 0x166, 0x18a, 0x192, 0x19c, 0x1a4, 0x1ae, 0x1bb, 0x1c2, 0x4d8, 0x4db,
    0x4de, 0x6b5, 0x70b, 0x714, 0x718, 0x734, 0x743, 0x75a, 0x784, 0x7e6, 0x7f3, 0x870, 0x879,
    0x8a1, 0x8de, 0x8e2, 0x8e7, 0x8ec, 0x8f1, 0x8f6, 0x8fb, 0x904, 0x975, 0xa5e,
];
const MAPPER_IRAM_BYTE_WORD_OFFSETS: [usize; 3] = [0x47c, 0x78a, 0x792];

/// Every runtime-dependent scalar patched by the fresh installer after copying its `$C30` bytes.
///
/// The two mapping bytes and external SNES/IRAM targets are deliberately explicit. Their values
/// are revision- and mapper-owned inputs, not safe constants inferred from one installed ROM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedExAnimationRuntimeRelocations {
    pub mapping_bytes: [u8; MAPPING_BYTE_OFFSETS.len()],
    pub snes_pointers: [u32; SNES_POINTER_OFFSETS.len()],
    pub iram_words: [u16; IRAM_WORD_OFFSETS.len()],
    /// Low 16 bits of the mapped address for the allocation's internal code base at `+$4B0`.
    pub local_word_base: u16,
}

/// The two additional mapped pointer values installed only in the `$C50` runtime form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedExAnimationRuntimeOptionalRelocations {
    /// Mapper-encoded SNES address of the appended suffix at allocation `+$C30`.
    pub suffix_snes_pointer: u32,
    /// Mapper compatibility entry written by Lunar Magic as `$7FC020`.
    pub mapping_helper_snes_pointer: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedExAnimationRuntimeError {
    InvalidTemplateHex,
    WrongTemplateLength(usize),
    WrongOptionalTemplateLength(usize),
    SnesPointerOutOfRange {
        index: usize,
        value: u32,
    },
    LocalWordBelowBase {
        index: usize,
        value: u16,
    },
    LocalWordOverflow {
        index: usize,
        base: u16,
        relative: u16,
    },
    MapperRuntimeTooShort(usize),
    MapperIramWordOutOfRange {
        offset: usize,
        value: u16,
    },
    MapperIramByteWordOutOfRange {
        offset: usize,
        value: u16,
    },
}

impl fmt::Display for ExpandedExAnimationRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "expanded ExAnimation runtime failed: {self:?}")
    }
}

impl std::error::Error for ExpandedExAnimationRuntimeError {}

/// Materializes the exact relocation-free `$C30` template embedded in Lunar Magic 3.63.
///
/// The source spans the original executable ranges `$005B5298..$005B5408`,
/// `$005B5410..$005B5750`, and `$005B4B10..$005B5290`, concatenated in the stack-buffer order
/// passed to `AllocateRomSpaceWithExpansion`.
pub fn expanded_exanimation_runtime_template() -> Result<Vec<u8>, ExpandedExAnimationRuntimeError> {
    let encoded = include_str!("assets/exanimation_runtime_core.hex");
    let digits = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if digits.len() != EXPANDED_EXANIMATION_RUNTIME_CORE_LEN * 2 {
        return Err(ExpandedExAnimationRuntimeError::WrongTemplateLength(
            digits.len() / 2,
        ));
    }
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok(high << 4 | low)
        })
        .collect()
}

/// Materializes the exact optional `$20`-byte mapper suffix embedded at executable `$005B5754`.
///
/// `InstallExpandedExAnimationRuntime` appends this range after the ordinary `$C30` core when its
/// mapper-state predicate is true, producing one `$C50` allocation. Keeping it separate prevents
/// an ordinary LoROM install from silently acquiring mapper-only code and its `LM\0\1` marker.
pub fn expanded_exanimation_runtime_optional_suffix()
-> Result<Vec<u8>, ExpandedExAnimationRuntimeError> {
    let encoded = include_str!("assets/exanimation_runtime_optional.hex");
    let digits = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if digits.len() != EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN * 2 {
        return Err(ExpandedExAnimationRuntimeError::WrongOptionalTemplateLength(digits.len() / 2));
    }
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok(high << 4 | low)
        })
        .collect()
}

/// Reproduces the installer's contiguous stack-buffer order for either `$C30` or `$C50` form.
pub fn expanded_exanimation_runtime_template_with_optional_suffix(
    include_optional_suffix: bool,
) -> Result<Vec<u8>, ExpandedExAnimationRuntimeError> {
    let mut runtime = expanded_exanimation_runtime_template()?;
    if include_optional_suffix {
        runtime.extend(expanded_exanimation_runtime_optional_suffix()?);
    }
    Ok(runtime)
}

/// Applies the two recovered mapper-only 24-bit pointer relocations to the contiguous `$C50` form.
pub fn relocate_expanded_exanimation_runtime_with_optional_suffix(
    relocations: &ExpandedExAnimationRuntimeRelocations,
    optional: ExpandedExAnimationRuntimeOptionalRelocations,
) -> Result<Vec<u8>, ExpandedExAnimationRuntimeError> {
    let mut runtime = relocate_expanded_exanimation_runtime(relocations)?;
    relocate_expanded_exanimation_mapper_iram(&mut runtime)?;
    runtime.extend(expanded_exanimation_runtime_optional_suffix()?);
    for (index, (offset, value)) in [
        (OPTIONAL_SUFFIX_POINTER_OFFSET, optional.suffix_snes_pointer),
        (
            OPTIONAL_MAPPING_HELPER_POINTER_OFFSET,
            optional.mapping_helper_snes_pointer,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if value > 0x00ff_ffff {
            return Err(ExpandedExAnimationRuntimeError::SnesPointerOutOfRange {
                index: SNES_POINTER_OFFSETS.len() + index,
                value,
            });
        }
        runtime[offset..offset + 3].copy_from_slice(&value.to_le_bytes()[..3]);
    }
    Ok(runtime)
}

/// Applies the complete mapper-conditioned IRAM relocation pass recovered from `$0045CF74`
/// through `$0045E519`.
///
/// Lunar Magic first validates every ordinary word as `< $2000` and every compact IRAM word as
/// `< $0100`, then adds `$6000` or `$3000` respectively. This implementation preflights the whole
/// family before writing, so a late invalid operand cannot leave a partially transformed runtime.
pub fn relocate_expanded_exanimation_mapper_iram(
    runtime: &mut [u8],
) -> Result<(), ExpandedExAnimationRuntimeError> {
    if runtime.len() < EXPANDED_EXANIMATION_RUNTIME_CORE_LEN {
        return Err(ExpandedExAnimationRuntimeError::MapperRuntimeTooShort(
            runtime.len(),
        ));
    }
    for offset in MAPPER_IRAM_WORD_OFFSETS {
        let value = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]);
        if value >= 0x2000 {
            return Err(ExpandedExAnimationRuntimeError::MapperIramWordOutOfRange {
                offset,
                value,
            });
        }
    }
    for offset in MAPPER_IRAM_BYTE_WORD_OFFSETS {
        let value = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]);
        if value >= 0x100 {
            return Err(
                ExpandedExAnimationRuntimeError::MapperIramByteWordOutOfRange { offset, value },
            );
        }
    }
    for offset in MAPPER_IRAM_WORD_OFFSETS {
        let value = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]) + 0x6000;
        runtime[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for offset in MAPPER_IRAM_BYTE_WORD_OFFSETS {
        let value = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]) + 0x3000;
        runtime[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

/// Applies all Ghidra-recovered fresh-install relocations to the exact core template.
pub fn relocate_expanded_exanimation_runtime(
    relocations: &ExpandedExAnimationRuntimeRelocations,
) -> Result<Vec<u8>, ExpandedExAnimationRuntimeError> {
    let mut runtime = expanded_exanimation_runtime_template()?;
    for (offset, value) in MAPPING_BYTE_OFFSETS
        .into_iter()
        .zip(relocations.mapping_bytes)
    {
        runtime[offset] = value;
    }
    for (index, (offset, value)) in SNES_POINTER_OFFSETS
        .into_iter()
        .zip(relocations.snes_pointers)
        .enumerate()
    {
        if value > 0x00ff_ffff {
            return Err(ExpandedExAnimationRuntimeError::SnesPointerOutOfRange { index, value });
        }
        runtime[offset..offset + 3].copy_from_slice(&value.to_le_bytes()[..3]);
    }
    for (offset, value) in IRAM_WORD_OFFSETS.into_iter().zip(relocations.iram_words) {
        runtime[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for index in 0..LOCAL_WORD_TABLE_ENTRIES {
        let offset = LOCAL_WORD_TABLE_OFFSET + index * 2;
        let source = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]);
        let relative = source.checked_sub(TEMPLATE_LOCAL_WORD_BASE).ok_or(
            ExpandedExAnimationRuntimeError::LocalWordBelowBase {
                index,
                value: source,
            },
        )?;
        let relocated = relocations.local_word_base.checked_add(relative).ok_or(
            ExpandedExAnimationRuntimeError::LocalWordOverflow {
                index,
                base: relocations.local_word_base,
                relative,
            },
        )?;
        runtime[offset..offset + 2].copy_from_slice(&relocated.to_le_bytes());
    }
    Ok(runtime)
}

/// Builds Lunar Magic's empty current pointer table: low byte `$FF`, remaining bytes zero.
#[must_use]
pub fn empty_expanded_exanimation_pointer_table() -> Vec<u8> {
    let mut table = vec![0; EXPANDED_EXANIMATION_POINTER_TABLE_LEN];
    for pointer in table.chunks_exact_mut(3) {
        pointer[0] = 0xff;
    }
    table
}

fn hex_nibble(value: u8) -> Result<u8, ExpandedExAnimationRuntimeError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ExpandedExAnimationRuntimeError::InvalidTemplateHex),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn read_u24(bytes: &[u8], offset: usize) -> u32 {
        u32::from(bytes[offset])
            | u32::from(bytes[offset + 1]) << 8
            | u32::from(bytes[offset + 2]) << 16
    }

    #[test]
    fn template_and_empty_pointer_table_have_exact_recovered_shapes() {
        let template = expanded_exanimation_runtime_template().unwrap();
        assert_eq!(template.len(), 0xc30);
        assert_eq!(
            &template[..8],
            &[0xe2, 0x30, 0x8b, 0xa2, 0x7f, 0xda, 0xab, 0xa9]
        );
        assert_eq!(
            &template[0x170..0x178],
            &[0x9c, 0x26, 0x43, 0xc2, 0x20, 0xa9, 0x00, 0x43]
        );
        assert_eq!(
            &template[0x4b0..0x4b8],
            &[0x8b, 0xa2, 0x7f, 0xda, 0xab, 0xa4, 0x14, 0xcc]
        );
        let table = empty_expanded_exanimation_pointer_table();
        assert_eq!(table.len(), 0x600);
        assert!(table.chunks_exact(3).all(|pointer| pointer == [0xff, 0, 0]));
    }

    #[test]
    fn optional_mapper_suffix_matches_ghidra_and_follows_the_core_exactly() {
        let suffix = expanded_exanimation_runtime_optional_suffix().unwrap();
        assert_eq!(suffix.len(), EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN);
        assert_eq!(
            suffix,
            [
                0xa5, 0x03, 0x8d, 0x20, 0xc0, 0xa5, 0x05, 0x8d, 0x22, 0xc0, 0xa5, 0x08, 0x5c, 0x20,
                0xc0, 0x7f, 0x4c, 0x4d, 0x00, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xff, 0xff, 0xff,
            ]
        );
        let combined = expanded_exanimation_runtime_template_with_optional_suffix(true).unwrap();
        assert_eq!(
            combined.len(),
            EXPANDED_EXANIMATION_RUNTIME_CORE_LEN + EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN
        );
        assert_eq!(
            &combined[..EXPANDED_EXANIMATION_RUNTIME_CORE_LEN],
            expanded_exanimation_runtime_template().unwrap()
        );
        assert_eq!(&combined[EXPANDED_EXANIMATION_RUNTIME_CORE_LEN..], suffix);
        assert_eq!(
            expanded_exanimation_runtime_template_with_optional_suffix(false).unwrap(),
            expanded_exanimation_runtime_template().unwrap()
        );
    }

    #[test]
    fn optional_mapper_calls_are_typed_and_do_not_change_the_ordinary_core() {
        let relocations = ExpandedExAnimationRuntimeRelocations {
            mapping_bytes: [0x12, 0x34],
            snes_pointers: [0x80_8000; SNES_POINTER_OFFSETS.len()],
            iram_words: [0x1234; IRAM_WORD_OFFSETS.len()],
            local_word_base: 0x1000,
        };
        let ordinary = relocate_expanded_exanimation_runtime(&relocations).unwrap();
        let expanded = relocate_expanded_exanimation_runtime_with_optional_suffix(
            &relocations,
            ExpandedExAnimationRuntimeOptionalRelocations {
                suffix_snes_pointer: 0x40_c030,
                mapping_helper_snes_pointer: OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
            },
        )
        .unwrap();
        assert_eq!(expanded.len(), 0xc50);
        assert_eq!(
            &expanded[OPTIONAL_SUFFIX_POINTER_OFFSET..OPTIONAL_SUFFIX_POINTER_OFFSET + 3],
            &[0x30, 0xc0, 0x40]
        );
        assert_eq!(
            &expanded[OPTIONAL_MAPPING_HELPER_POINTER_OFFSET
                ..OPTIONAL_MAPPING_HELPER_POINTER_OFFSET + 3],
            &[0x20, 0xc0, 0x7f]
        );
        assert_ne!(
            &ordinary[OPTIONAL_SUFFIX_POINTER_OFFSET..OPTIONAL_SUFFIX_POINTER_OFFSET + 3],
            &expanded[OPTIONAL_SUFFIX_POINTER_OFFSET..OPTIONAL_SUFFIX_POINTER_OFFSET + 3]
        );
        assert!(matches!(
            relocate_expanded_exanimation_runtime_with_optional_suffix(
                &relocations,
                ExpandedExAnimationRuntimeOptionalRelocations {
                    suffix_snes_pointer: 0x0100_0000,
                    mapping_helper_snes_pointer: OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
                }
            ),
            Err(ExpandedExAnimationRuntimeError::SnesPointerOutOfRange { index: 8, .. })
        ));
    }

    #[test]
    fn mapper_iram_relocation_is_complete_checked_and_failure_atomic() {
        let mut runtime = vec![0x00; EXPANDED_EXANIMATION_RUNTIME_CORE_LEN];
        for (index, offset) in MAPPER_IRAM_WORD_OFFSETS.into_iter().enumerate() {
            runtime[offset..offset + 2].copy_from_slice(&(0x0100 + index as u16).to_le_bytes());
        }
        for (index, offset) in MAPPER_IRAM_BYTE_WORD_OFFSETS.into_iter().enumerate() {
            runtime[offset..offset + 2].copy_from_slice(&(index as u16).to_le_bytes());
        }
        relocate_expanded_exanimation_mapper_iram(&mut runtime).unwrap();
        for (index, offset) in MAPPER_IRAM_WORD_OFFSETS.into_iter().enumerate() {
            assert_eq!(
                u16::from_le_bytes([runtime[offset], runtime[offset + 1]]),
                0x6100 + index as u16
            );
        }
        for (index, offset) in MAPPER_IRAM_BYTE_WORD_OFFSETS.into_iter().enumerate() {
            assert_eq!(
                u16::from_le_bytes([runtime[offset], runtime[offset + 1]]),
                0x3000 + index as u16
            );
        }

        let mut invalid = vec![0x00; EXPANDED_EXANIMATION_RUNTIME_CORE_LEN];
        let last = *MAPPER_IRAM_WORD_OFFSETS.last().unwrap();
        invalid[last..last + 2].copy_from_slice(&0x2000u16.to_le_bytes());
        let before = invalid.clone();
        assert!(matches!(
            relocate_expanded_exanimation_mapper_iram(&mut invalid),
            Err(ExpandedExAnimationRuntimeError::MapperIramWordOutOfRange {
                offset,
                value: 0x2000
            }) if offset == last
        ));
        assert_eq!(invalid, before);
        assert!(matches!(
            relocate_expanded_exanimation_mapper_iram(&mut [0; 16]),
            Err(ExpandedExAnimationRuntimeError::MapperRuntimeTooShort(16))
        ));
    }

    #[test]
    fn invalid_external_and_internal_relocations_are_typed() {
        let mut relocations = ExpandedExAnimationRuntimeRelocations {
            mapping_bytes: [0; 2],
            snes_pointers: [0; 8],
            iram_words: [0; 12],
            local_word_base: 0,
        };
        relocations.snes_pointers[3] = 0x0100_0000;
        assert!(matches!(
            relocate_expanded_exanimation_runtime(&relocations),
            Err(ExpandedExAnimationRuntimeError::SnesPointerOutOfRange { index: 3, .. })
        ));
        relocations.snes_pointers[3] = 0;
        relocations.local_word_base = 0xffff;
        assert!(matches!(
            relocate_expanded_exanimation_runtime(&relocations),
            Err(ExpandedExAnimationRuntimeError::LocalWordOverflow { .. })
        ));
    }

    #[test]
    fn complete_relocated_template_matches_retained_lunar_magic_runtime() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc");
        assert!(path.is_file(), "missing {}", path.display());
        let physical = std::fs::read(path).unwrap();
        let logical = &physical[0x200..];
        let installed = &logical[0x80_549..0x80_549 + EXPANDED_EXANIMATION_RUNTIME_CORE_LEN];
        let template = expanded_exanimation_runtime_template().unwrap();
        let relocations = ExpandedExAnimationRuntimeRelocations {
            mapping_bytes: MAPPING_BYTE_OFFSETS.map(|offset| installed[offset]),
            snes_pointers: SNES_POINTER_OFFSETS.map(|offset| read_u24(installed, offset)),
            iram_words: IRAM_WORD_OFFSETS.map(|offset| read_u16(installed, offset)),
            local_word_base: read_u16(installed, LOCAL_WORD_TABLE_OFFSET)
                - (read_u16(&template, LOCAL_WORD_TABLE_OFFSET) - TEMPLATE_LOCAL_WORD_BASE),
        };
        assert_eq!(
            relocate_expanded_exanimation_runtime(&relocations).unwrap(),
            installed
        );
    }
}
