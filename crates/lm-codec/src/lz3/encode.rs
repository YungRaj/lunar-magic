const DICTIONARY_CANDIDATES: usize = 256;

/// Produces a deterministic LZ3 stream using fills and all recovered dictionary transforms.
///
/// Match selection is bounded and prefers the greatest byte saving, then the longest match and
/// lowest command number. Dictionary operands use the compact relative representation when the
/// source is within 128 bytes and the absolute representation otherwise.
#[must_use]
pub fn encode_lz3(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut cursor = 0;
    let mut positions: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
    while cursor < input.len() {
        if let Some(candidate) = best_command(input, cursor, &positions) {
            emit_header(&mut output, candidate.command, candidate.len);
            match candidate.command {
                1 => output.push(input[cursor]),
                2 => output.extend_from_slice(&input[cursor..cursor + 2]),
                3 => {}
                4..=6 => emit_dictionary_operand(&mut output, cursor, candidate.source),
                _ => unreachable!("encoder candidate"),
            }
            index_positions(input, cursor, candidate.len, &mut positions);
            cursor += candidate.len;
            continue;
        }
        let start = cursor;
        cursor += 1;
        while cursor - start < 1024
            && cursor < input.len()
            && best_command(input, cursor, &positions).is_none()
        {
            positions[usize::from(input[cursor - 1])].push(cursor - 1);
            cursor += 1;
        }
        emit_header(&mut output, 0, cursor - start);
        output.extend_from_slice(&input[start..cursor]);
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
    source: usize,
    operand_len: usize,
}

fn best_command(input: &[u8], offset: usize, positions: &[Vec<usize>; 256]) -> Option<Candidate> {
    let fill = best_fill(input, offset).map(|(command, len)| Candidate {
        command,
        len,
        source: 0,
        operand_len: match command {
            3 => 0,
            2 => 2,
            _ => 1,
        },
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
    let mut best = None;
    for (command, sources) in [
        (4, positions[usize::from(target)].as_slice()),
        (5, positions[usize::from(target.reverse_bits())].as_slice()),
        (6, positions[usize::from(target)].as_slice()),
    ] {
        for &source in sources
            .iter()
            .rev()
            .filter(|source| {
                let distance = offset - **source;
                **source < 0x8000 || distance <= 128
            })
            .take(DICTIONARY_CANDIDATES)
        {
            let len = dictionary_match_len(input, offset, source, maximum, command);
            let distance = offset - source;
            let operand_len = if distance <= 128 { 1 } else { 2 };
            let minimum = operand_len + 1;
            if len < minimum {
                continue;
            }
            let candidate = Candidate {
                command,
                len,
                source,
                operand_len,
            };
            if best.is_none_or(|current: Candidate| {
                (
                    candidate.len.saturating_sub(candidate.operand_len),
                    candidate.len,
                    std::cmp::Reverse(candidate.command),
                    candidate.source,
                ) > (
                    current.len.saturating_sub(current.operand_len),
                    current.len,
                    std::cmp::Reverse(current.command),
                    current.source,
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
            let byte = input[address];
            let byte = if command == 5 {
                byte.reverse_bits()
            } else {
                byte
            };
            byte == input[offset + index]
        })
        .count()
}

fn index_positions(input: &[u8], offset: usize, len: usize, positions: &mut [Vec<usize>; 256]) {
    for index in offset..offset + len {
        positions[usize::from(input[index])].push(index);
    }
}

fn emit_dictionary_operand(output: &mut Vec<u8>, offset: usize, source: usize) {
    let distance = offset - source;
    if distance <= 128 {
        output.push(0x80 | u8::try_from(distance - 1).unwrap_or(0x7f));
    } else {
        output.extend_from_slice(&u16::try_from(source).unwrap_or(0x7fff).to_be_bytes());
    }
}

fn best_fill(input: &[u8], offset: usize) -> Option<(u8, usize)> {
    let remaining = &input[offset..];
    let maximum = remaining.len().min(1024);
    let byte_len = 1 + remaining[1..maximum]
        .iter()
        .take_while(|byte| **byte == remaining[0])
        .count();
    let word_len = if maximum >= 2 {
        2 + (2..maximum)
            .take_while(|index| remaining[*index] == remaining[*index & 1])
            .count()
    } else {
        0
    };
    let zero_len = if remaining[0] == 0 { byte_len } else { 0 };
    [(3, zero_len, 2), (1, byte_len, 3), (2, word_len, 4)]
        .into_iter()
        .filter(|(_, len, minimum)| len >= minimum)
        .max_by_key(|(command, len, _)| {
            let operand_len = match command {
                3 => 0,
                2 => 2,
                _ => 1,
            };
            (len.saturating_sub(operand_len), std::cmp::Reverse(*command))
        })
        .map(|(command, len, _)| (command, len))
}

fn emit_header(output: &mut Vec<u8>, command: u8, len: usize) {
    debug_assert!((1..=1024).contains(&len));
    let encoded = len - 1;
    if len <= 32 {
        output.push((command << 5) | encoded.to_le_bytes()[0]);
    } else {
        output.push(0xe0 | (command << 2) | u8::try_from((encoded >> 8) & 3).unwrap_or(0));
        output.push(encoded.to_le_bytes()[0]);
    }
}
