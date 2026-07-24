use crate::{Observation, sha256_hex};
use lm_graphics::{
    DecodedGraphicsRemapCommandStream, GraphicsRemapEnd, GraphicsRemapPayload, GraphicsRemapStride,
};
use std::fmt::Write;

#[must_use]
pub fn observe_graphics_remap(decoded: &DecodedGraphicsRemapCommandStream) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "graphics-remap/consumed", &decoded.consumed);
    put(
        &mut result,
        "graphics-remap/command-count",
        &decoded.stream.commands.len(),
    );
    match decoded.stream.end {
        GraphicsRemapEnd::Terminator(bytes) => {
            put(&mut result, "graphics-remap/end", &"terminator");
            put(&mut result, "graphics-remap/terminator", &hex(&bytes));
        }
        GraphicsRemapEnd::StreamLimit => put(&mut result, "graphics-remap/end", &"stream-limit"),
    }
    for (index, command) in decoded.stream.commands.iter().enumerate() {
        let base = format!("graphics-remap/commands/{index:04x}");
        put(
            &mut result,
            &format!("{base}/destination-word"),
            &command.destination_word,
        );
        put(
            &mut result,
            &format!("{base}/stride"),
            &match command.stride {
                GraphicsRemapStride::Linear => "linear",
                GraphicsRemapStride::Column => "column",
            },
        );
        match &command.payload {
            GraphicsRemapPayload::Literal(bytes) => {
                put(&mut result, &format!("{base}/kind"), &"literal");
                put(&mut result, &format!("{base}/output-bytes"), &bytes.len());
                put(
                    &mut result,
                    &format!("{base}/payload-sha256"),
                    &sha256_hex(bytes),
                );
            }
            GraphicsRemapPayload::Repeat {
                value,
                output_bytes,
            } => {
                put(&mut result, &format!("{base}/kind"), &"repeat");
                put(&mut result, &format!("{base}/output-bytes"), output_bytes);
                put(&mut result, &format!("{base}/value"), &hex(value));
            }
        }
    }
    result
}

fn put(result: &mut Observation, path: &str, value: &(impl ToString + ?Sized)) {
    result
        .insert(path, value.to_string())
        .expect("graphics remap observation paths are unique");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{
        GraphicsRemapCommand, GraphicsRemapCommandStream, GraphicsRemapPayload, GraphicsRemapStride,
    };

    #[test]
    fn observation_addresses_every_command_and_end_condition() {
        let decoded = DecodedGraphicsRemapCommandStream {
            stream: GraphicsRemapCommandStream {
                commands: vec![GraphicsRemapCommand {
                    destination_word: 0x123,
                    stride: GraphicsRemapStride::Column,
                    payload: GraphicsRemapPayload::Repeat {
                        value: [0x34, 0x12],
                        output_bytes: 5,
                    },
                }],
                end: GraphicsRemapEnd::Terminator([0xfe, 1, 2, 3]),
            },
            consumed: 10,
        };
        let observed = observe_graphics_remap(&decoded);
        assert_eq!(
            observed.get("graphics-remap/commands/0000/destination-word"),
            Some("291")
        );
        assert_eq!(
            observed.get("graphics-remap/commands/0000/value"),
            Some("3412")
        );
        assert_eq!(observed.get("graphics-remap/terminator"), Some("fe010203"));
    }
}
