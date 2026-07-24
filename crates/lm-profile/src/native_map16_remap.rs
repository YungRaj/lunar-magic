//! Lunar Magic 3.63's installed SMW US revision-0 Map16 remap tables.

use lm_project::Project;
use lm_rats::{HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_MAP16_REMAP_GROUPS: usize = 120;
pub const SMW_US_V1_MAP16_REMAP_RANGE_OFFSETS: usize = 0x26359;
pub const SMW_US_V1_MAP16_REMAP_RANGE_RECORDS_POINTER: usize = 0x2649f;
pub const SMW_US_V1_GROUPED_MAP16_RUNTIME_POINTER: usize = 0x269f8;
pub const SMW_US_V1_GROUPED_MAP16_OFFSETS_POINTER_IN_RUNTIME: usize = 0x0d;
pub const SMW_US_V1_GROUPED_MAP16_SOURCE_POINTER_IN_RUNTIME: usize = 0x22;
pub const SMW_US_V1_GROUPED_MAP16_DESTINATION_POINTER_IN_RUNTIME: usize = 0x28;
pub const SMW_US_V1_GROUPED_MAP16_FLAGS_POINTER_IN_RUNTIME: usize = 0x34;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Map16RemapRange {
    pub source_tile: u16,
    pub destination_tile: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupedMap16RemapRecord {
    pub flags: u8,
    pub source_tile: u16,
    pub destination_tile: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1Map16Remaps {
    pub range_groups: Vec<Vec<Map16RemapRange>>,
    pub record_groups: Vec<Vec<GroupedMap16RemapRecord>>,
    pub range_records_block: RatsBlock,
    pub grouped_runtime_block: RatsBlock,
    pub grouped_offsets_block: RatsBlock,
    pub grouped_flags_block: RatsBlock,
    pub grouped_source_block: RatsBlock,
    pub grouped_destination_block: RatsBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1Map16RemapError {
    Rom(RomError),
    Header(HeaderError),
    PointerNotTagged(usize),
    OffsetTableLength { expected: usize, actual: usize },
    OffsetsNotMonotonic,
    RecordPlaneLengthMismatch,
    InvalidRangeTile(u16),
    InvalidGroupedRecord(GroupedMap16RemapRecord),
}

impl fmt::Display for SmwUsV1Map16RemapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "installed SMW Map16 remap load failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1Map16RemapError {}

impl From<RomError> for SmwUsV1Map16RemapError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<HeaderError> for SmwUsV1Map16RemapError {
    fn from(value: HeaderError) -> Self {
        Self::Header(value)
    }
}

/// Loads both current remap formats written by Lunar Magic's overworld save path.
///
/// # Errors
///
/// Rejects missing RATS ownership, malformed pointers or offset tables, mismatched parallel
/// planes, and records that Lunar Magic's loader itself rejects.
pub fn load_smw_us_v1_installed_map16_remaps(
    project: &Project,
) -> Result<LoadedSmwUsV1Map16Remaps, SmwUsV1Map16RemapError> {
    let bytes = project.rom.logical_bytes();

    let range_offsets = read_words(
        bytes,
        SMW_US_V1_MAP16_REMAP_RANGE_OFFSETS,
        SMW_US_V1_MAP16_REMAP_GROUPS + 1,
    )?;
    let range_records_offset = read_pointer(bytes, SMW_US_V1_MAP16_REMAP_RANGE_RECORDS_POINTER)?;
    let range_records_block = exact_block(bytes, range_records_offset)?;
    let range_count = usize::from(*range_offsets.last().unwrap_or(&0));
    if range_records_block.payload.len() != range_count * 4 {
        return Err(SmwUsV1Map16RemapError::RecordPlaneLengthMismatch);
    }
    let range_groups = group_ranges(bytes, &range_offsets, range_records_block.payload.start)?;

    let grouped_runtime_offset = read_pointer(bytes, SMW_US_V1_GROUPED_MAP16_RUNTIME_POINTER)?;
    let grouped_runtime_block = exact_block(bytes, grouped_runtime_offset)?;
    let runtime_pointer = |relative| read_pointer(bytes, grouped_runtime_offset + relative);
    let grouped_offsets_offset =
        runtime_pointer(SMW_US_V1_GROUPED_MAP16_OFFSETS_POINTER_IN_RUNTIME)?;
    let grouped_flags_offset = runtime_pointer(SMW_US_V1_GROUPED_MAP16_FLAGS_POINTER_IN_RUNTIME)?;
    let grouped_source_offset = runtime_pointer(SMW_US_V1_GROUPED_MAP16_SOURCE_POINTER_IN_RUNTIME)?;
    let grouped_destination_offset =
        runtime_pointer(SMW_US_V1_GROUPED_MAP16_DESTINATION_POINTER_IN_RUNTIME)?;
    let grouped_offsets_block = exact_block(bytes, grouped_offsets_offset)?;
    let grouped_flags_block = exact_block(bytes, grouped_flags_offset)?;
    let grouped_source_block = exact_block(bytes, grouped_source_offset)?;
    let grouped_destination_block = exact_block(bytes, grouped_destination_offset)?;
    let grouped_offsets = read_words(
        bytes,
        grouped_offsets_offset,
        SMW_US_V1_MAP16_REMAP_GROUPS + 1,
    )?;
    if grouped_offsets_block.payload.len() != (SMW_US_V1_MAP16_REMAP_GROUPS + 1) * 2 {
        return Err(SmwUsV1Map16RemapError::OffsetTableLength {
            expected: (SMW_US_V1_MAP16_REMAP_GROUPS + 1) * 2,
            actual: grouped_offsets_block.payload.len(),
        });
    }
    let record_count = usize::from(*grouped_offsets.last().unwrap_or(&0)) / 2;
    if grouped_flags_block.payload.len() != record_count
        || grouped_source_block.payload.len() != record_count * 2
        || grouped_destination_block.payload.len() != record_count * 2
    {
        return Err(SmwUsV1Map16RemapError::RecordPlaneLengthMismatch);
    }
    let record_groups = group_records(
        bytes,
        &grouped_offsets,
        grouped_flags_offset,
        grouped_source_offset,
        grouped_destination_offset,
    )?;

    Ok(LoadedSmwUsV1Map16Remaps {
        range_groups,
        record_groups,
        range_records_block,
        grouped_runtime_block,
        grouped_offsets_block,
        grouped_flags_block,
        grouped_source_block,
        grouped_destination_block,
    })
}

fn read_pointer(bytes: &[u8], offset: usize) -> Result<usize, SmwUsV1Map16RemapError> {
    let pointer = bytes
        .get(offset..offset + 3)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: 3,
            image_len: bytes.len(),
        })?;
    Ok(snes_to_pc(
        Mapper::LoRom,
        u32::from_le_bytes([pointer[0], pointer[1], pointer[2], 0]),
    )?)
}

fn exact_block(bytes: &[u8], payload_offset: usize) -> Result<RatsBlock, SmwUsV1Map16RemapError> {
    let header = payload_offset
        .checked_sub(HEADER_LEN)
        .ok_or(SmwUsV1Map16RemapError::PointerNotTagged(payload_offset))?;
    let block = parse_at(bytes, header)?;
    if block.payload.start != payload_offset {
        return Err(SmwUsV1Map16RemapError::PointerNotTagged(payload_offset));
    }
    Ok(block)
}

fn read_words(
    bytes: &[u8],
    offset: usize,
    count: usize,
) -> Result<Vec<u16>, SmwUsV1Map16RemapError> {
    let len = count * 2;
    let source = bytes
        .get(offset..offset + len)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len,
            image_len: bytes.len(),
        })?;
    Ok(source
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}

fn group_ranges(
    bytes: &[u8],
    offsets: &[u16],
    records_offset: usize,
) -> Result<Vec<Vec<Map16RemapRange>>, SmwUsV1Map16RemapError> {
    offsets
        .windows(2)
        .map(|bounds| {
            let start = usize::from(bounds[0]);
            let end = usize::from(bounds[1]);
            if start > end {
                return Err(SmwUsV1Map16RemapError::OffsetsNotMonotonic);
            }
            (start..end)
                .map(|index| {
                    let words = read_words(bytes, records_offset + index * 4, 2)?;
                    let record = Map16RemapRange {
                        source_tile: words[0],
                        destination_tile: words[1],
                    };
                    if record.source_tile >= 0x4000 || record.destination_tile >= 0x4000 {
                        return Err(SmwUsV1Map16RemapError::InvalidRangeTile(
                            record.source_tile.max(record.destination_tile),
                        ));
                    }
                    Ok(record)
                })
                .collect()
        })
        .collect()
}

fn group_records(
    bytes: &[u8],
    offsets: &[u16],
    flags_offset: usize,
    source_offset: usize,
    destination_offset: usize,
) -> Result<Vec<Vec<GroupedMap16RemapRecord>>, SmwUsV1Map16RemapError> {
    offsets
        .windows(2)
        .map(|bounds| {
            let start = usize::from(bounds[0]) / 2;
            let end = usize::from(bounds[1]) / 2;
            if start > end || bounds[0] & 1 != 0 || bounds[1] & 1 != 0 {
                return Err(SmwUsV1Map16RemapError::OffsetsNotMonotonic);
            }
            (start..end)
                .map(|index| {
                    let flags =
                        *bytes
                            .get(flags_offset + index)
                            .ok_or(RomError::RangeOutOfBounds {
                                offset: flags_offset + index,
                                len: 1,
                                image_len: bytes.len(),
                            })?;
                    let record = GroupedMap16RemapRecord {
                        flags,
                        source_tile: read_words(bytes, source_offset + index * 2, 1)?[0],
                        destination_tile: read_words(bytes, destination_offset + index * 2, 1)?[0],
                    };
                    let valid = if record.flags & 1 == 0 {
                        record.destination_tile < 0x800
                    } else {
                        record.source_tile < 0x4000 && record.destination_tile < 0x4000
                    };
                    if !valid {
                        return Err(SmwUsV1Map16RemapError::InvalidGroupedRecord(record));
                    }
                    Ok(record)
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;
    use std::{fs, path::Path};

    #[test]
    fn loads_exact_wine_installed_remap_tables() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let loaded = load_smw_us_v1_installed_map16_remaps(&Project::new(image)).unwrap();

        assert_eq!(loaded.range_groups.len(), 120);
        assert_eq!(loaded.record_groups.len(), 120);
        assert_eq!(loaded.range_groups.iter().map(Vec::len).sum::<usize>(), 371);
        assert_eq!(loaded.record_groups.iter().map(Vec::len).sum::<usize>(), 44);
        assert_eq!(loaded.range_records_block.payload, 0x84090..0x8465c);
        assert_eq!(loaded.grouped_runtime_block.payload, 0x84664..0x84704);
        assert_eq!(loaded.grouped_offsets_block.payload, 0x8470c..0x847fe);
        assert_eq!(loaded.grouped_flags_block.payload, 0x84806..0x84832);
        assert_eq!(loaded.grouped_source_block.payload, 0x8483a..0x84892);
        assert_eq!(loaded.grouped_destination_block.payload, 0x8489a..0x848f2);
    }
}
