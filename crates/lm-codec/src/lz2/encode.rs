const DICTIONARY_CANDIDATES: usize = 256;

#[must_use]
pub fn encode_lz2_literals(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len() + input.len() / 32 + 1);
    for chunk in input.chunks(32) {
        result.push(u8::try_from(chunk.len() - 1).unwrap_or(31));
        result.extend_from_slice(chunk);
    }
    result.push(0xff);
    result
}

/// Encodes an `LC_LZ2` stream using every recovered command class.
///
/// The encoder is deterministic and prioritizes the shortest encoded command at each position.
#[must_use]
pub fn encode_lz2(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut cursor = 0;
    let mut positions: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
    while cursor < input.len() {
        if let Some(candidate) = best_command(input, cursor, &positions) {
            emit_header(&mut output, candidate.command, candidate.len);
            match candidate.command {
                1 | 3 => output.push(input[cursor]),
                2 => output.extend_from_slice(&input[cursor..cursor + 2]),
                4..=6 => output.extend_from_slice(&candidate.source.to_be_bytes()),
                _ => unreachable!("candidate command is validated"),
            }
            index_positions(input, cursor, candidate.len, &mut positions);
            cursor += candidate.len;
            continue;
        }

        let literal_start = cursor;
        cursor += 1;
        while cursor - literal_start < 1024
            && cursor < input.len()
            && best_command(input, cursor, &positions).is_none()
        {
            positions[usize::from(input[cursor - 1])].push(cursor - 1);
            cursor += 1;
        }
        let literal = &input[literal_start..cursor];
        emit_header(&mut output, 0, literal.len());
        output.extend_from_slice(literal);
        if positions[usize::from(input[cursor - 1])].last() != Some(&(cursor - 1)) {
            positions[usize::from(input[cursor - 1])].push(cursor - 1);
        }
    }
    output.push(0xff);
    output
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Candidate {
    command: u8,
    len: usize,
    source: u16,
    operand_len: usize,
}

fn best_command(input: &[u8], offset: usize, positions: &[Vec<usize>; 256]) -> Option<Candidate> {
    let fill = best_fill(input, offset).map(|(command, len)| Candidate {
        command,
        len,
        source: 0,
        operand_len: if command == 2 { 2 } else { 1 },
    });
    let dictionary = best_dictionary(input, offset, positions);
    [fill, dictionary]
        .into_iter()
        .flatten()
        .max_by_key(|candidate| {
            (
                candidate.len.saturating_sub(candidate.operand_len),
                candidate.len,
                std::cmp::Reverse(candidate.command),
            )
        })
}

fn best_dictionary(
    input: &[u8],
    offset: usize,
    positions: &[Vec<usize>; 256],
) -> Option<Candidate> {
    let target = input[offset];
    let maximum = (input.len() - offset).min(1024);
    let normal_sources = &positions[usize::from(target)];
    let mut best = None;
    // Lunar Magic 3.63's standard-GFX LZ2 reader dispatches command classes 4 through 7 to the
    // same forward-copy routine. Emit only class 4 so encoded streams have identical semantics in
    // both the original editor and the Rust decoder.
    for (command, sources) in [(4, normal_sources.as_slice())] {
        for &source in sources
            .iter()
            .rev()
            .filter(|source| u16::try_from(**source).is_ok())
            .take(DICTIONARY_CANDIDATES)
        {
            let len = dictionary_match_len(input, offset, source, maximum, command);
            if len < 3 {
                continue;
            }
            let candidate = Candidate {
                command,
                len,
                source: u16::try_from(source).unwrap_or(u16::MAX),
                operand_len: 2,
            };
            if best.is_none_or(|current: Candidate| {
                (
                    candidate.len,
                    std::cmp::Reverse(candidate.command),
                    std::cmp::Reverse(candidate.source),
                ) > (
                    current.len,
                    std::cmp::Reverse(current.command),
                    std::cmp::Reverse(current.source),
                )
            }) {
                best = Some(candidate);
            }
        }
    }
    best
}

fn dictionary_match_len(
    input: &[u8],
    offset: usize,
    source: usize,
    maximum: usize,
    command: u8,
) -> usize {
    (0..maximum)
        .take_while(|index| {
            let address = if command == 6 {
                let Some(address) = source.checked_sub(*index) else {
                    return false;
                };
                address
            } else {
                source + index
            };
            let source_byte = input[address];
            let source_byte = if command == 5 {
                source_byte.reverse_bits()
            } else {
                source_byte
            };
            source_byte == input[offset + index]
        })
        .count()
}

fn index_positions(input: &[u8], offset: usize, len: usize, positions: &mut [Vec<usize>; 256]) {
    for index in offset..offset + len {
        positions[usize::from(input[index])].push(index);
    }
}

fn best_fill(input: &[u8], offset: usize) -> Option<(u8, usize)> {
    let remaining = &input[offset..];
    let maximum = remaining.len().min(1024);
    let byte_len = 1 + remaining[1..maximum]
        .iter()
        .take_while(|byte| **byte == remaining[0])
        .count();
    let increment_len = (1..maximum)
        .take_while(|index| remaining[*index] == remaining[0].wrapping_add(index.to_le_bytes()[0]))
        .count()
        + 1;
    let word_len = if maximum >= 2 {
        (2..maximum)
            .take_while(|index| remaining[*index] == remaining[*index & 1])
            .count()
            + 2
    } else {
        0
    };

    // Headers cost one or two bytes. These thresholds always beat equivalent literals.
    [(1, byte_len, 3), (3, increment_len, 3), (2, word_len, 4)]
        .into_iter()
        .filter(|(_, len, minimum)| len >= minimum)
        .max_by_key(|(command, len, _)| {
            let operand_len = if *command == 2 { 2 } else { 1 };
            len.saturating_sub(operand_len)
        })
        .map(|(command, len, _)| (command, len))
}

fn emit_header(output: &mut Vec<u8>, command: u8, len: usize) {
    debug_assert!((1..=1024).contains(&len));
    let encoded_len = len - 1;
    if len <= 32 {
        output.push((command << 5) | encoded_len.to_le_bytes()[0]);
    } else {
        output.push(0xe0 | (command << 2) | u8::try_from((encoded_len >> 8) & 3).unwrap_or(0));
        output.push(encoded_len.to_le_bytes()[0]);
    }
}
