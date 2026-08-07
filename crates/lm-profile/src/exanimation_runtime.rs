use std::fmt;

/// Core allocation copied by `InstallExpandedExAnimationRuntime` (`0045CAF0`).
pub const EXPANDED_EXANIMATION_RUNTIME_CORE_LEN: usize = 0xc30;
/// Optional mapper-specific suffix copied from executable data `$005B5754`.
pub const EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN: usize = 0x20;
/// Separately allocated 512-entry, three-byte per-level pointer table.
pub const EXPANDED_EXANIMATION_POINTER_TABLE_LEN: usize = 0x600;

const MAPPING_BYTE_OFFSETS: [usize; 2] = [0x5c, 0x66];
const SNES_POINTER_OFFSETS: [usize; 8] = [0xdf, 0xea, 0x5b0, 0x601, 0xa7e, 0xaaa, 0xacf, 0xaf4];
const IRAM_WORD_OFFSETS: [usize; 12] = [
    0x550, 0x59b, 0x5bf, 0x5c3, 0x5d6, 0x63e, 0x6c4, 0x79e, 0x7ac, 0x825, 0x833, 0x8b0,
];
const LOCAL_WORD_TABLE_OFFSET: usize = 0xb4a;
const LOCAL_WORD_TABLE_ENTRIES: usize = 108;
const TEMPLATE_LOCAL_WORD_BASE: u16 = 0x8000;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedExAnimationRuntimeError {
    InvalidTemplateHex,
    WrongTemplateLength(usize),
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
    #[ignore = "requires retained Lunar Magic 3.63 ExAnimation installation ROM"]
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
