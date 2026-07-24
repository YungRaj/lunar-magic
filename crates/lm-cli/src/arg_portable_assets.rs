use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    enum Kind {
        Graphics,
        Map16Page,
        Layer3Plane,
        AnimationFrame,
    }
    let paths = |input: usize, normalized: Option<usize>, observation: Option<usize>| {
        (
            PathBuf::from(&args[input]),
            normalized.map(|index| PathBuf::from(&args[index])),
            observation.map(|index| PathBuf::from(&args[index])),
        )
    };
    let (kind, normalized, observation) = match text {
        [command, _] if command == "graphics-file" => (Kind::Graphics, None, None),
        [command, _, _] if command == "graphics-file" => (Kind::Graphics, Some(2), None),
        [command, _, _, _] if command == "graphics-file" => (Kind::Graphics, Some(2), Some(3)),
        [command, _] if command == "map16-page-file" => (Kind::Map16Page, None, None),
        [command, _, _] if command == "map16-page-file" => (Kind::Map16Page, Some(2), None),
        [command, _, _, _] if command == "map16-page-file" => (Kind::Map16Page, Some(2), Some(3)),
        [command, _] if command == "layer3-plane-file" => (Kind::Layer3Plane, None, None),
        [command, _, _] if command == "layer3-plane-file" => (Kind::Layer3Plane, Some(2), None),
        [command, _, _, _] if command == "layer3-plane-file" => {
            (Kind::Layer3Plane, Some(2), Some(3))
        }
        [command, _] if command == "animation-frame-file" => (Kind::AnimationFrame, None, None),
        [command, _, _] if command == "animation-frame-file" => {
            (Kind::AnimationFrame, Some(2), None)
        }
        [command, _, _, _] if command == "animation-frame-file" => {
            (Kind::AnimationFrame, Some(2), Some(3))
        }
        _ => return None,
    };
    let (input, normalized_output, observation) = paths(1, normalized, observation);
    Some(match kind {
        Kind::Graphics => Command::GraphicsFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Map16Page => Command::Map16PageFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Layer3Plane => Command::Layer3PlaneFile {
            input,
            normalized_output,
            observation,
        },
        Kind::AnimationFrame => Command::AnimationFrameFile {
            input,
            normalized_output,
            observation,
        },
    })
}
