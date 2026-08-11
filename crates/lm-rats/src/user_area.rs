use crate::{HEADER_LEN, parse_at};
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatsConflict {
    pub first_range: Range<usize>,
    pub nested_range: Range<usize>,
    pub overlapped_space: usize,
}

/// Lunar Magic-compatible accounting for the expandable/user-owned portion of a ROM.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RomUserAreaScan {
    pub rat_protected_space: usize,
    pub unprotected_map16: usize,
    pub unprotected_used_space: usize,
    pub unusable_space: usize,
    pub free_space: usize,
    pub total_user_space: usize,
    pub conflicting_rats: usize,
    pub conflicting_space: usize,
    pub rat_structures: usize,
    pub largest_free_32kb_bank: usize,
    pub largest_free_area: usize,
    pub conflicting_offsets: Vec<usize>,
    pub conflicts: Vec<RatsConflict>,
}

/// Scans the logical ROM interval `user_start..bytes.len()` using Lunar Magic's RATS and
/// free-space rules. `unprotected_map16` identifies the optional pre-1.64 Map16 allocation;
/// pass `None` for modern ROMs.
#[must_use]
pub fn scan_rom_user_area(
    bytes: &[u8],
    user_start: usize,
    unprotected_map16: Option<Range<usize>>,
) -> RomUserAreaScan {
    let user_start = user_start.min(bytes.len());
    let user = user_start..bytes.len();
    let mut structures = Vec::new();
    for offset in user.clone() {
        if let Ok(block) = parse_at(bytes, offset)
            && block.payload.end <= user.end
        {
            structures.push(block.full_range());
        }
    }

    let mut protected = Vec::<Range<usize>>::new();
    let mut conflicting_offsets = Vec::new();
    let mut conflicts = Vec::new();
    let mut conflicting_space = 0usize;
    let mut active = 0..0;
    for range in &structures {
        if range.start < active.end {
            conflicting_offsets.push(range.start);
            conflicting_space = conflicting_space.saturating_add(range.len());
            conflicts.push(RatsConflict {
                first_range: active.clone(),
                nested_range: range.clone(),
                overlapped_space: active.end.min(range.end).saturating_sub(range.start),
            });
        }
        if range.end > active.end {
            active = range.clone();
        }
        merge_range(&mut protected, range.clone());
    }

    let map16 = unprotected_map16.and_then(|range| intersect(range, user.clone()));
    let mut result = RomUserAreaScan {
        total_user_space: user.len(),
        conflicting_rats: conflicting_offsets.len(),
        conflicting_space,
        rat_structures: structures.len(),
        conflicting_offsets,
        conflicts,
        ..RomUserAreaScan::default()
    };
    result.rat_protected_space = protected.iter().map(Range::len).sum();

    let mut cursor = user.start;
    for range in &protected {
        classify_unprotected(bytes, cursor..range.start, map16.as_ref(), &mut result);
        cursor = range.end;
    }
    classify_unprotected(bytes, cursor..user.end, map16.as_ref(), &mut result);
    result
}

fn merge_range(ranges: &mut Vec<Range<usize>>, mut incoming: Range<usize>) {
    let mut index = 0;
    while index < ranges.len() {
        if ranges[index].end < incoming.start || incoming.end < ranges[index].start {
            index += 1;
            continue;
        }
        let existing = ranges.remove(index);
        incoming.start = incoming.start.min(existing.start);
        incoming.end = incoming.end.max(existing.end);
    }
    let insert = ranges.partition_point(|range| range.start < incoming.start);
    ranges.insert(insert, incoming);
}

fn intersect(left: Range<usize>, right: Range<usize>) -> Option<Range<usize>> {
    let range = left.start.max(right.start)..left.end.min(right.end);
    (!range.is_empty()).then_some(range)
}

fn classify_unprotected(
    bytes: &[u8],
    range: Range<usize>,
    map16: Option<&Range<usize>>,
    result: &mut RomUserAreaScan,
) {
    let mut cursor = range.start;
    while cursor < range.end {
        if let Some(map16) = map16
            && cursor >= map16.start
            && cursor < map16.end
        {
            let end = range.end.min(map16.end);
            result.unprotected_map16 += end - cursor;
            cursor = end;
            continue;
        }
        if bytes[cursor] != 0 {
            result.unprotected_used_space += 1;
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < range.end
            && bytes[cursor] == 0
            && map16.is_none_or(|map16| cursor < map16.start || cursor >= map16.end)
        {
            cursor += 1;
        }
        let length = cursor - start;
        if length <= HEADER_LEN {
            result.unusable_space += length;
        } else {
            result.free_space += length;
            result.largest_free_area = result.largest_free_area.max(length);
            result.largest_free_32kb_bank = result
                .largest_free_32kb_bank
                .max(largest_bank_payload(start, cursor));
        }
    }
}

fn largest_bank_payload(start: usize, end: usize) -> usize {
    const BANK: usize = 0x8000;
    if end <= start + HEADER_LEN {
        return 0;
    }
    let first_boundary = start.div_ceil(BANK) * BANK;
    let mut best = end.min(first_boundary.max(start + HEADER_LEN)) - start;
    best = best.saturating_sub(HEADER_LEN);
    let full_bank_start = first_boundary.max(start + HEADER_LEN);
    if end.saturating_sub(full_bank_start) >= BANK {
        return BANK;
    }
    if first_boundary < end {
        best = best.max((end - first_boundary).min(BANK));
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make_header;

    #[test]
    fn classifies_protected_used_small_gaps_and_free_space() {
        let mut bytes = vec![0xff; 0x20];
        bytes.extend_from_slice(&make_header(3).unwrap());
        bytes.extend_from_slice(&[1, 2, 3]);
        bytes.extend_from_slice(&[0; 8]);
        bytes.push(7);
        bytes.extend_from_slice(&[0; 12]);
        let report = scan_rom_user_area(&bytes, 0x20, None);
        assert_eq!(report.rat_protected_space, 11);
        assert_eq!(report.unprotected_used_space, 1);
        assert_eq!(report.unusable_space, 8);
        assert_eq!(report.free_space, 12);
        assert_eq!(report.total_user_space, 32);
        assert_eq!(report.rat_structures, 1);
        assert_eq!(report.largest_free_area, 12);
    }

    #[test]
    fn reports_nested_rats_and_counts_the_protected_union_once() {
        let mut bytes = vec![0; 80];
        bytes[..8].copy_from_slice(&make_header(40).unwrap());
        bytes[16..24].copy_from_slice(&make_header(8).unwrap());
        let report = scan_rom_user_area(&bytes, 0, None);
        assert_eq!(report.rat_structures, 2);
        assert_eq!(report.conflicting_rats, 1);
        assert_eq!(report.conflicting_offsets, vec![16]);
        assert_eq!(report.conflicting_space, 16);
        assert_eq!(
            report.conflicts,
            vec![RatsConflict {
                first_range: 0..48,
                nested_range: 16..32,
                overlapped_space: 16,
            }]
        );
        assert_eq!(report.rat_protected_space, 48);
        assert_eq!(report.free_space, 32);
    }

    #[test]
    fn full_zero_bank_accepts_a_header_immediately_before_the_boundary() {
        let bytes = vec![0; 0x10008];
        let report = scan_rom_user_area(&bytes, 0, None);
        assert_eq!(report.largest_free_32kb_bank, 0x8000);
        assert_eq!(report.largest_free_area, bytes.len());
    }

    #[test]
    fn removes_legacy_map16_from_other_unprotected_categories() {
        let mut bytes = vec![0; 64];
        bytes[20..28].fill(0x55);
        let report = scan_rom_user_area(&bytes, 0, Some(16..32));
        assert_eq!(report.unprotected_map16, 16);
        assert_eq!(report.unprotected_used_space, 0);
        assert_eq!(report.free_space, 48);
    }
}
