use crate::NATIVE_LAYER2_TILEMAP_LEN;
use std::fmt;

const REMAP_ENTRY_COUNT: usize = 0x8000;
const DISPLAY_BIAS: u32 = 0x8000;
const MAX_INDEX: u16 = 0x7fff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TermKind {
    Absolute,
    Add,
    Subtract,
    Matrix,
    Rectangle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemapTerm {
    kind: TermKind,
    first: u16,
    last: u16,
    range: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemapOperation {
    source: RemapTerm,
    destination: RemapTerm,
}

/// A parsed Lunar Magic level-background tile-remapping program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLayer2RemapProgram {
    operations: Vec<RemapOperation>,
}

/// Result of applying a remapping program to one native Layer 2 tilemap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLayer2RemapResult {
    /// Aggregate storage-index edits suitable for the native level-assets controller.
    pub edits: Vec<(usize, u16)>,
    /// The Map16 bank Lunar Magic would select after a cross-bank mapping.
    pub active_bank: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLayer2RemapError {
    ScriptTooLong(usize),
    InvalidHex { position: usize },
    ValueOutOfRange { position: usize, value: u32 },
    MissingDestination { position: usize },
    TilemapLength(usize),
    ActiveBank(u8),
    SelectionIndex(usize),
    DuplicateSelectionIndex(usize),
    Offset(i32),
}

impl fmt::Display for NativeLayer2RemapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Layer 2 remap program: {self:?}")
    }
}

impl std::error::Error for NativeLayer2RemapError {}

impl NativeLayer2RemapProgram {
    /// Lunar Magic's dialog supplies a 16 KiB input buffer including its terminator.
    pub const MAX_SCRIPT_LEN: usize = 0x3fff;

    /// Parses Lunar Magic's comma/newline-delimited source/destination term pairs.
    ///
    /// Unprefixed values use the editor's displayed `$8000`–`$FFFF` tile domain. `+` and `-`
    /// introduce saturating relative transformations, `M` emits sequential values, and `R`
    /// interprets a source range as a rectangle in a 16-column tile page.
    ///
    /// # Errors
    ///
    /// Rejects oversized input, malformed hexadecimal terms, values outside the native 15-bit
    /// remap table, and an unmatched final source term.
    pub fn parse(script: &str) -> Result<Self, NativeLayer2RemapError> {
        if script.len() > Self::MAX_SCRIPT_LEN {
            return Err(NativeLayer2RemapError::ScriptTooLong(script.len()));
        }
        let bytes = script.as_bytes();
        let mut cursor = 0;
        let mut operations = Vec::new();
        while let Some(source) = parse_term(bytes, &mut cursor)? {
            let destination = parse_term(bytes, &mut cursor)?
                .ok_or(NativeLayer2RemapError::MissingDestination { position: cursor })?;
            operations.push(RemapOperation {
                source,
                destination,
            });
        }
        Ok(Self { operations })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Applies this program to all cells or to the supplied native storage indexes.
    ///
    /// The global offset reproduces Lunar Magic's post-program offset control and is saturating.
    /// Tile words are interpreted through `active_bank`, remapped in the 15-bit domain, and stored
    /// as normalized 12-bit Map16 indexes. The last cross-bank primary result selects the returned
    /// bank, matching the recovered native loop.
    ///
    /// # Errors
    ///
    /// Rejects malformed tilemap storage, banks outside `$0`–`$7`, offsets outside the signed
    /// 15-bit domain, or a selection containing an out-of-range storage index.
    pub fn apply(
        &self,
        bytes: &[u8],
        active_bank: u8,
        global_offset: i32,
        selection: Option<&[usize]>,
    ) -> Result<NativeLayer2RemapResult, NativeLayer2RemapError> {
        if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
            return Err(NativeLayer2RemapError::TilemapLength(bytes.len()));
        }
        if active_bank >= 8 {
            return Err(NativeLayer2RemapError::ActiveBank(active_bank));
        }
        if !(-0x7fff..=0x7fff).contains(&global_offset) {
            return Err(NativeLayer2RemapError::Offset(global_offset));
        }
        let mapping = self.build_mapping(global_offset);
        let indexes = selection.map_or_else(
            || (0..NATIVE_LAYER2_TILEMAP_LEN / 2).collect(),
            <[usize]>::to_vec,
        );
        let mut edits = Vec::new();
        let mut resulting_bank = active_bank;
        let mut seen = [false; NATIVE_LAYER2_TILEMAP_LEN / 2];
        for index in indexes {
            let offset = index
                .checked_mul(2)
                .filter(|offset| offset + 1 < bytes.len())
                .ok_or(NativeLayer2RemapError::SelectionIndex(index))?;
            if std::mem::replace(&mut seen[index], true) {
                return Err(NativeLayer2RemapError::DuplicateSelectionIndex(index));
            }
            let word = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            let source = usize::from(word & 0x0fff) + usize::from(active_bank) * 0x1000;
            let mapped = mapping[source];
            let mapped_bank = mapped.to_le_bytes()[1] >> 4;
            if mapped_bank != active_bank {
                resulting_bank = mapped_bank;
            }
            let value = mapped & 0x0fff;
            if value != word {
                edits.push((index, value));
            }
        }
        Ok(NativeLayer2RemapResult {
            edits,
            active_bank: resulting_bank,
        })
    }

    fn build_mapping(&self, global_offset: i32) -> Box<[u16]> {
        let mut mapping = (0..REMAP_ENTRY_COUNT)
            .map(|index| u16::try_from(index).unwrap_or(MAX_INDEX))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        for operation in &self.operations {
            apply_operation(&mut mapping, *operation);
        }
        if global_offset != 0 {
            for value in mapping.as_mut() {
                *value = saturating_index(i32::from(*value) + global_offset);
            }
        }
        mapping
    }
}

fn parse_term(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<Option<RemapTerm>, NativeLayer2RemapError> {
    skip_leading_separators(bytes, cursor);
    if *cursor >= bytes.len() {
        return Ok(None);
    }
    let term_position = *cursor;
    let kind = match bytes[*cursor] {
        b'+' => {
            *cursor += 1;
            TermKind::Add
        }
        b'-' => {
            *cursor += 1;
            TermKind::Subtract
        }
        b'M' | b'm' => {
            *cursor += 1;
            TermKind::Matrix
        }
        b'R' | b'r' => {
            *cursor += 1;
            TermKind::Rectangle
        }
        _ => TermKind::Absolute,
    };
    skip_ascii_whitespace(bytes, cursor);
    let first_raw = parse_hex(bytes, cursor, term_position)?;
    scan_to_term_delimiter(bytes, cursor);
    let mut range = false;
    let mut last_raw = first_raw;
    if bytes.get(*cursor) == Some(&b'-') {
        range = true;
        *cursor += 1;
        skip_ascii_whitespace(bytes, cursor);
        last_raw = parse_hex(bytes, cursor, *cursor)?;
        scan_to_pair_delimiter(bytes, cursor);
    }
    consume_pair_delimiter(bytes, cursor);
    let first = normalize_term_value(kind, first_raw, term_position)?;
    let last = normalize_term_value(kind, last_raw, term_position)?;
    Ok(Some(RemapTerm {
        kind,
        first,
        last,
        range,
    }))
}

fn parse_hex(
    bytes: &[u8],
    cursor: &mut usize,
    position: usize,
) -> Result<u32, NativeLayer2RemapError> {
    let start = *cursor;
    let mut value = 0_u32;
    while let Some(digit) = bytes.get(*cursor).and_then(|byte| hex_digit(*byte)) {
        value = value
            .checked_mul(16)
            .and_then(|value| value.checked_add(u32::from(digit)))
            .ok_or(NativeLayer2RemapError::ValueOutOfRange {
                position,
                value: u32::MAX,
            })?;
        *cursor += 1;
    }
    if *cursor == start {
        return Err(NativeLayer2RemapError::InvalidHex { position });
    }
    Ok(value)
}

fn normalize_term_value(
    kind: TermKind,
    value: u32,
    position: usize,
) -> Result<u16, NativeLayer2RemapError> {
    let normalized = match kind {
        TermKind::Add | TermKind::Subtract => value,
        TermKind::Absolute | TermKind::Matrix | TermKind::Rectangle => value
            .checked_sub(DISPLAY_BIAS)
            .ok_or(NativeLayer2RemapError::ValueOutOfRange { position, value })?,
    };
    if normalized > u32::from(MAX_INDEX) {
        return Err(NativeLayer2RemapError::ValueOutOfRange { position, value });
    }
    Ok(u16::try_from(normalized).expect("bounded remap term"))
}

fn apply_operation(mapping: &mut [u16], operation: RemapOperation) {
    if operation.source.kind == TermKind::Rectangle {
        apply_rectangle(mapping, operation);
    } else {
        apply_linear(mapping, operation);
    }
}

fn apply_linear(mapping: &mut [u16], operation: RemapOperation) {
    let first = usize::from(operation.source.first);
    let last = usize::from(operation.source.last);
    if first > last {
        return;
    }
    match operation.destination.kind {
        TermKind::Add | TermKind::Subtract => {
            let delta = signed_delta(operation.destination);
            for value in &mut mapping[first..=last] {
                *value = saturating_index(i32::from(*value) + delta);
            }
        }
        TermKind::Matrix => {
            let mut value = operation.destination.first;
            for target in &mut mapping[first..=last] {
                *target = value;
                value = value.saturating_add(1).min(MAX_INDEX);
            }
        }
        TermKind::Absolute | TermKind::Rectangle if operation.destination.range => {
            let mut value = operation.destination.first;
            for target in &mut mapping[first..=last] {
                *target = value;
                if value < operation.destination.last {
                    value += 1;
                }
            }
        }
        TermKind::Absolute | TermKind::Rectangle => {
            mapping[first..=last].fill(operation.destination.first);
        }
    }
}

fn apply_rectangle(mapping: &mut [u16], operation: RemapOperation) {
    let first_x = usize::from(operation.source.first & 0x0f);
    let last_x = usize::from(operation.source.last & 0x0f);
    let first_y = usize::from(operation.source.first >> 4);
    let last_y = usize::from(operation.source.last >> 4);
    if first_x > last_x || first_y > last_y {
        return;
    }
    let delta = signed_delta(operation.destination);
    for y in first_y..=last_y {
        for x in first_x..=last_x {
            let index = y * 16 + x;
            mapping[index] = match operation.destination.kind {
                TermKind::Add | TermKind::Subtract => {
                    saturating_index(i32::from(mapping[index]) + delta)
                }
                TermKind::Matrix => {
                    let row = usize::from(operation.destination.first >> 4) + (y - first_y);
                    let column = usize::from(operation.destination.first & 0x0f) + (x - first_x);
                    saturating_index(i32::try_from(row * 16 + column).unwrap_or(i32::MAX))
                }
                TermKind::Absolute | TermKind::Rectangle => operation.destination.first,
            };
        }
    }
}

fn signed_delta(term: RemapTerm) -> i32 {
    let value = i32::from(term.first);
    if term.kind == TermKind::Subtract {
        -value
    } else {
        value
    }
}

fn saturating_index(value: i32) -> u16 {
    u16::try_from(value.clamp(0, i32::from(MAX_INDEX))).unwrap_or(MAX_INDEX)
}

fn skip_leading_separators(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn scan_to_term_delimiter(bytes: &[u8], cursor: &mut usize) {
    while bytes
        .get(*cursor)
        .is_some_and(|byte| !matches!(*byte, b',' | b'\n' | b'-'))
    {
        *cursor += 1;
    }
}

fn scan_to_pair_delimiter(bytes: &[u8], cursor: &mut usize) {
    while bytes
        .get(*cursor)
        .is_some_and(|byte| !matches!(*byte, b',' | b'\n'))
    {
        *cursor += 1;
    }
}

fn consume_pair_delimiter(bytes: &[u8], cursor: &mut usize) {
    if bytes
        .get(*cursor)
        .is_some_and(|byte| matches!(*byte, b',' | b'\n'))
    {
        *cursor += 1;
    }
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tilemap(words: impl IntoIterator<Item = u16>) -> Vec<u8> {
        let mut bytes = vec![0; NATIVE_LAYER2_TILEMAP_LEN];
        for (pair, word) in bytes.chunks_exact_mut(2).zip(words) {
            pair.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_all_recovered_prefixes_and_rejects_native_domain_errors() {
        let program =
            NativeLayer2RemapProgram::parse("8000-800F,8010-801F\n8020,+2\nR8030-8042,M8050")
                .unwrap();
        assert_eq!(program.operations.len(), 3);
        assert_eq!(
            NativeLayer2RemapProgram::parse("7FFF,8000"),
            Err(NativeLayer2RemapError::ValueOutOfRange {
                position: 0,
                value: 0x7fff
            })
        );
        assert!(matches!(
            NativeLayer2RemapProgram::parse("8000"),
            Err(NativeLayer2RemapError::MissingDestination { .. })
        ));
        assert_eq!(
            NativeLayer2RemapProgram::parse("8000,,8001"),
            Err(NativeLayer2RemapError::InvalidHex { position: 5 })
        );
    }

    #[test]
    fn linear_replace_range_matrix_and_saturating_offsets_match_native_rules() {
        let program = NativeLayer2RemapProgram::parse(
            "8000-8003,8010-8011\n8010-8013,M8020\n8020,+7FFF\nFFFF,+1\n8001,-7FFF",
        )
        .unwrap();
        let mapping = program.build_mapping(0);
        assert_eq!(&mapping[0..4], &[0x10, 0, 0x11, 0x11]);
        assert_eq!(&mapping[0x10..0x14], &[0x20, 0x21, 0x22, 0x23]);
        assert_eq!(mapping[0x20], 0x7fff);
        assert_eq!(mapping[0x7fff], 0x7fff);
    }

    #[test]
    fn rectangle_fill_matrix_and_relative_operations_use_sixteen_column_pages() {
        let program =
            NativeLayer2RemapProgram::parse("R8011-8023,M8045\nR8045-8057,+2\nR8060-8071,8088")
                .unwrap();
        let mapping = program.build_mapping(0);
        assert_eq!(mapping[0x11], 0x45);
        assert_eq!(mapping[0x13], 0x47);
        assert_eq!(mapping[0x21], 0x55);
        assert_eq!(mapping[0x23], 0x57);
        assert_eq!(mapping[0x45], 0x47);
        assert_eq!(mapping[0x57], 0x59);
        assert_eq!(mapping[0x60], 0x88);
        assert_eq!(mapping[0x71], 0x88);
    }

    #[test]
    fn apply_honors_selection_global_offset_bank_and_twelve_bit_storage() {
        let bytes = tilemap([0x000, 0x001, 0x002, 0xfff]);
        let program = NativeLayer2RemapProgram::parse("9000-9002,A000-A002").unwrap();
        let selected = [0, 2];
        let result = program.apply(&bytes, 1, 1, Some(&selected)).unwrap();
        assert_eq!(result.edits, [(0, 0x001), (2, 0x003)]);
        assert_eq!(result.active_bank, 2);
    }

    #[test]
    fn apply_rejects_bad_storage_bank_offset_and_selection_before_editing() {
        let program = NativeLayer2RemapProgram::parse("").unwrap();
        assert!(matches!(
            program.apply(&[0; 2], 0, 0, None),
            Err(NativeLayer2RemapError::TilemapLength(2))
        ));
        assert_eq!(
            program.apply(&[0; NATIVE_LAYER2_TILEMAP_LEN], 8, 0, None),
            Err(NativeLayer2RemapError::ActiveBank(8))
        );
        assert_eq!(
            program.apply(&[0; NATIVE_LAYER2_TILEMAP_LEN], 0, 0x8000, None),
            Err(NativeLayer2RemapError::Offset(0x8000))
        );
        assert_eq!(
            program.apply(&[0; NATIVE_LAYER2_TILEMAP_LEN], 0, 0, Some(&[0x400])),
            Err(NativeLayer2RemapError::SelectionIndex(0x400))
        );
        assert_eq!(
            program.apply(&[0; NATIVE_LAYER2_TILEMAP_LEN], 0, 0, Some(&[3, 3])),
            Err(NativeLayer2RemapError::DuplicateSelectionIndex(3))
        );
    }
}
