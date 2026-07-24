//! Recovered graphics-remap command stream codec and scratch-map interpreter.

pub const GRAPHICS_REMAP_WORDS: usize = 0x8000;
pub const GRAPHICS_REMAP_STREAM_LIMIT: usize = 0x8000;
pub const GRAPHICS_REMAP_MAX_PREFIX_LEN: usize = 0xc003;
const MAX_RAW_LENGTH: usize = 0x3fff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsRemapStride {
    Linear,
    Column,
}

impl GraphicsRemapStride {
    const fn words(self) -> usize {
        match self {
            Self::Linear => 1,
            Self::Column => 0x20,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsRemapPayload {
    Literal(Vec<u8>),
    Repeat { value: [u8; 2], output_bytes: u16 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsRemapCommand {
    pub destination_word: u16,
    pub stride: GraphicsRemapStride,
    pub payload: GraphicsRemapPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsRemapEnd {
    Terminator([u8; 4]),
    StreamLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsRemapCommandStream {
    pub commands: Vec<GraphicsRemapCommand>,
    pub end: GraphicsRemapEnd,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedGraphicsRemapCommandStream {
    pub stream: GraphicsRemapCommandStream,
    pub consumed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsRemapError {
    Truncated {
        offset: usize,
        required: usize,
        remaining: usize,
    },
    DestinationWord(u16),
    EmptyLiteral,
    LiteralTooLong(usize),
    RepeatLength(u16),
    StreamLimitEndBeforeLimit(usize),
    CommandAfterStreamLimit {
        command: usize,
    },
    WrongScratchWordCount(usize),
    Overflow,
}

impl std::fmt::Display for GraphicsRemapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "graphics remap stream error: {self:?}")
    }
}

impl std::error::Error for GraphicsRemapError {}

impl GraphicsRemapCommandStream {
    /// Decodes one command-stream prefix with the exact recovered `$8000`-byte consumption limit.
    ///
    /// A header whose first byte has bit 7 set terminates immediately. Otherwise decoding stops
    /// after the first complete command that reaches or crosses the stream limit.
    ///
    /// # Errors
    ///
    /// Rejects every truncated header or command payload.
    pub fn decode_prefix(
        input: &[u8],
    ) -> Result<DecodedGraphicsRemapCommandStream, GraphicsRemapError> {
        let mut offset = 0;
        let mut commands = Vec::new();
        loop {
            let header = take_array::<4>(input, &mut offset)?;
            if header[0] & 0x80 != 0 {
                return Ok(DecodedGraphicsRemapCommandStream {
                    stream: Self {
                        commands,
                        end: GraphicsRemapEnd::Terminator(header),
                    },
                    consumed: offset,
                });
            }
            let destination_word = u16::from(header[1]) | (u16::from(header[0] & 0x7f) << 8);
            let raw_length = usize::from(header[3]) | (usize::from(header[2] & 0x3f) << 8);
            let stride = if header[2] & 0x80 == 0 {
                GraphicsRemapStride::Linear
            } else {
                GraphicsRemapStride::Column
            };
            let payload = if header[2] & 0x40 == 0 {
                let length = raw_length + 1;
                GraphicsRemapPayload::Literal(take(input, &mut offset, length)?.to_vec())
            } else {
                GraphicsRemapPayload::Repeat {
                    value: take_array::<2>(input, &mut offset)?,
                    output_bytes: u16::try_from(raw_length + 2)
                        .map_err(|_| GraphicsRemapError::Overflow)?,
                }
            };
            commands.push(GraphicsRemapCommand {
                destination_word,
                stride,
                payload,
            });
            if offset >= GRAPHICS_REMAP_STREAM_LIMIT {
                return Ok(DecodedGraphicsRemapCommandStream {
                    stream: Self {
                        commands,
                        end: GraphicsRemapEnd::StreamLimit,
                    },
                    consumed: offset,
                });
            }
        }
    }

    /// Encodes the exact command representation, including noncanonical terminator bytes.
    ///
    /// # Errors
    ///
    /// Rejects fields that cannot be represented and stream-limit endings that do not match the
    /// recovered consumption rule.
    pub fn encode(&self) -> Result<Vec<u8>, GraphicsRemapError> {
        let mut output = Vec::new();
        for (command_index, command) in self.commands.iter().enumerate() {
            if output.len() >= GRAPHICS_REMAP_STREAM_LIMIT {
                return Err(GraphicsRemapError::CommandAfterStreamLimit {
                    command: command_index,
                });
            }
            encode_command(command, &mut output)?;
        }
        match self.end {
            GraphicsRemapEnd::Terminator(header) => {
                if output.len() >= GRAPHICS_REMAP_STREAM_LIMIT {
                    return Err(GraphicsRemapError::CommandAfterStreamLimit {
                        command: self.commands.len(),
                    });
                }
                let mut exact = header;
                exact[0] |= 0x80;
                output.extend_from_slice(&exact);
            }
            GraphicsRemapEnd::StreamLimit => {
                if output.len() < GRAPHICS_REMAP_STREAM_LIMIT {
                    return Err(GraphicsRemapError::StreamLimitEndBeforeLimit(output.len()));
                }
            }
        }
        Ok(output)
    }

    /// Applies every command to an exact `$8000`-word scratch map.
    ///
    /// Literal pairs become little-endian words. A final odd literal byte replaces the low byte of
    /// its destination word. Repeat commands store complete words; their final odd byte replaces
    /// the high byte of the following destination word, exactly matching the recovered x86 helper.
    ///
    /// # Errors
    ///
    /// Rejects scratch maps with any other shape. Command construction invariants are validated
    /// before mutation, so failure leaves the scratch map unchanged.
    pub fn apply(&self, scratch: &mut [u16]) -> Result<(), GraphicsRemapError> {
        if scratch.len() != GRAPHICS_REMAP_WORDS {
            return Err(GraphicsRemapError::WrongScratchWordCount(scratch.len()));
        }
        self.encode()?;
        for command in &self.commands {
            apply_command(command, scratch);
        }
        Ok(())
    }
}

fn encode_command(
    command: &GraphicsRemapCommand,
    output: &mut Vec<u8>,
) -> Result<(), GraphicsRemapError> {
    if usize::from(command.destination_word) >= GRAPHICS_REMAP_WORDS {
        return Err(GraphicsRemapError::DestinationWord(
            command.destination_word,
        ));
    }
    let (raw_length, repeat) = match &command.payload {
        GraphicsRemapPayload::Literal(bytes) => {
            if bytes.is_empty() {
                return Err(GraphicsRemapError::EmptyLiteral);
            }
            if bytes.len() > MAX_RAW_LENGTH + 1 {
                return Err(GraphicsRemapError::LiteralTooLong(bytes.len()));
            }
            (bytes.len() - 1, false)
        }
        GraphicsRemapPayload::Repeat { output_bytes, .. } => {
            let length = usize::from(*output_bytes);
            if !(2..=MAX_RAW_LENGTH + 2).contains(&length) {
                return Err(GraphicsRemapError::RepeatLength(*output_bytes));
            }
            (length - 2, true)
        }
    };
    let destination = command.destination_word;
    let mut flags = u8::try_from(raw_length >> 8).map_err(|_| GraphicsRemapError::Overflow)?;
    if repeat {
        flags |= 0x40;
    }
    if command.stride == GraphicsRemapStride::Column {
        flags |= 0x80;
    }
    output.extend_from_slice(&[
        u8::try_from(destination >> 8).map_err(|_| GraphicsRemapError::Overflow)?,
        u8::try_from(destination & 0xff).map_err(|_| GraphicsRemapError::Overflow)?,
        flags,
        u8::try_from(raw_length & 0xff).map_err(|_| GraphicsRemapError::Overflow)?,
    ]);
    match &command.payload {
        GraphicsRemapPayload::Literal(bytes) => output.extend_from_slice(bytes),
        GraphicsRemapPayload::Repeat { value, .. } => output.extend_from_slice(value),
    }
    Ok(())
}

fn apply_command(command: &GraphicsRemapCommand, scratch: &mut [u16]) {
    let stride = command.stride.words();
    let destination = usize::from(command.destination_word);
    match &command.payload {
        GraphicsRemapPayload::Literal(bytes) => {
            for (index, pair) in bytes.chunks_exact(2).enumerate() {
                scratch[(destination + index * stride) & 0x7fff] =
                    u16::from_le_bytes([pair[0], pair[1]]);
            }
            if bytes.len() & 1 != 0 {
                let index = bytes.len() / 2;
                let target = &mut scratch[(destination + index * stride) & 0x7fff];
                *target = (*target & 0xff00) | u16::from(bytes[bytes.len() - 1]);
            }
        }
        GraphicsRemapPayload::Repeat {
            value,
            output_bytes,
        } => {
            let words = usize::from(*output_bytes) / 2;
            let value = u16::from_le_bytes(*value);
            for index in 0..words {
                scratch[(destination + index * stride) & 0x7fff] = value;
            }
            if output_bytes & 1 != 0 {
                let target = &mut scratch[(destination + words * stride) & 0x7fff];
                *target = (*target & 0x00ff) | ((value & 0x00ff) << 8);
            }
        }
    }
}

fn take<'a>(
    input: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], GraphicsRemapError> {
    let remaining = input.len().saturating_sub(*offset);
    if remaining < length {
        return Err(GraphicsRemapError::Truncated {
            offset: *offset,
            required: length,
            remaining,
        });
    }
    let end = offset
        .checked_add(length)
        .ok_or(GraphicsRemapError::Overflow)?;
    let result = &input[*offset..end];
    *offset = end;
    Ok(result)
}

fn take_array<const N: usize>(
    input: &[u8],
    offset: &mut usize,
) -> Result<[u8; N], GraphicsRemapError> {
    take(input, offset, N)?
        .try_into()
        .map_err(|_| GraphicsRemapError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_repeats_strides_wrap_and_odd_bytes_match_recovered_operations() {
        let stream = GraphicsRemapCommandStream {
            commands: vec![
                GraphicsRemapCommand {
                    destination_word: 0x7fff,
                    stride: GraphicsRemapStride::Linear,
                    payload: GraphicsRemapPayload::Literal(vec![0x34, 0x12, 0x56]),
                },
                GraphicsRemapCommand {
                    destination_word: 2,
                    stride: GraphicsRemapStride::Column,
                    payload: GraphicsRemapPayload::Repeat {
                        value: [0xcd, 0xab],
                        output_bytes: 5,
                    },
                },
            ],
            end: GraphicsRemapEnd::Terminator([0xff, 1, 2, 3]),
        };
        let mut scratch = vec![0x9876; GRAPHICS_REMAP_WORDS];
        stream.apply(&mut scratch).unwrap();
        assert_eq!(scratch[0x7fff], 0x1234);
        assert_eq!(scratch[0], 0x9856);
        assert_eq!(scratch[2], 0xabcd);
        assert_eq!(scratch[0x22], 0xabcd);
        assert_eq!(scratch[0x42], 0xcd76);
    }

    #[test]
    fn exact_noncanonical_terminator_and_trailing_bytes_round_trip_as_prefix() {
        let bytes = [0, 5, 0, 2, 1, 2, 3, 0xfe, 9, 8, 7, 0xaa, 0xbb];
        let decoded = GraphicsRemapCommandStream::decode_prefix(&bytes).unwrap();
        assert_eq!(decoded.consumed, 11);
        assert_eq!(decoded.stream.encode().unwrap(), bytes[..11]);
        assert_eq!(
            decoded.stream.end,
            GraphicsRemapEnd::Terminator([0xfe, 9, 8, 7])
        );
    }

    #[test]
    fn every_truncated_prefix_is_rejected_until_a_complete_terminator() {
        let stream = GraphicsRemapCommandStream {
            commands: vec![GraphicsRemapCommand {
                destination_word: 0x123,
                stride: GraphicsRemapStride::Linear,
                payload: GraphicsRemapPayload::Literal(vec![1, 2, 3, 4]),
            }],
            end: GraphicsRemapEnd::Terminator([0x80, 0, 0, 0]),
        };
        let bytes = stream.encode().unwrap();
        for end in 0..bytes.len() {
            assert!(GraphicsRemapCommandStream::decode_prefix(&bytes[..end]).is_err());
        }
        assert_eq!(
            GraphicsRemapCommandStream::decode_prefix(&bytes)
                .unwrap()
                .stream,
            stream
        );
    }

    #[test]
    fn command_crossing_consumption_limit_ends_without_terminator() {
        let literal = GraphicsRemapCommand {
            destination_word: 0,
            stride: GraphicsRemapStride::Linear,
            payload: GraphicsRemapPayload::Literal(vec![0x5a; 0x4000]),
        };
        let stream = GraphicsRemapCommandStream {
            commands: vec![literal.clone(), literal],
            end: GraphicsRemapEnd::StreamLimit,
        };
        let bytes = stream.encode().unwrap();
        assert_eq!(bytes.len(), 0x8008);
        let decoded = GraphicsRemapCommandStream::decode_prefix(&bytes).unwrap();
        assert_eq!(decoded.consumed, bytes.len());
        assert_eq!(decoded.stream, stream);
    }

    #[test]
    fn validation_failure_does_not_mutate_scratch() {
        let stream = GraphicsRemapCommandStream {
            commands: vec![GraphicsRemapCommand {
                destination_word: 0,
                stride: GraphicsRemapStride::Linear,
                payload: GraphicsRemapPayload::Literal(Vec::new()),
            }],
            end: GraphicsRemapEnd::Terminator([0x80, 0, 0, 0]),
        };
        let mut scratch = vec![7; GRAPHICS_REMAP_WORDS];
        let original = scratch.clone();
        assert_eq!(
            stream.apply(&mut scratch),
            Err(GraphicsRemapError::EmptyLiteral)
        );
        assert_eq!(scratch, original);
    }
}
