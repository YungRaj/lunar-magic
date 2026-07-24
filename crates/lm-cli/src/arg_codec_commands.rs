use crate::arg_values::{ArgsError, parse_number};
use crate::command_types::{Command, PlanarOperation};
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_codec_observation(
    args: &[OsString],
    kind: &str,
    output_bound: &str,
) -> Result<Command, Box<dyn std::error::Error>> {
    let kind = match kind {
        "lz2" => lm_oracle::CodecObservationKind::Lz2,
        "lz3" => lm_oracle::CodecObservationKind::Lz3,
        "rle-terminated" => lm_oracle::CodecObservationKind::RleTerminated,
        "rle-sized" => lm_oracle::CodecObservationKind::RleSized,
        value => return Err(ArgsError(format!("unknown observed codec {value}")).into()),
    };
    Ok(Command::CodecObserve {
        kind,
        input: PathBuf::from(&args[2]),
        output_bound: usize::try_from(parse_number(output_bound)?)?,
        observation: PathBuf::from(&args[4]),
    })
}

pub(crate) fn parse_planar(
    args: &[OsString],
    operation: &str,
    bits_per_pixel: &str,
) -> Result<Command, Box<dyn std::error::Error>> {
    let operation = match operation {
        "decode" => PlanarOperation::Decode,
        "encode" => crate::command_types::PlanarOperation::Encode,
        value => return Err(ArgsError(format!("unknown planar operation {value}")).into()),
    };
    Ok(Command::Planar {
        operation,
        bits_per_pixel: u8::try_from(parse_number(bits_per_pixel)?)?,
        input: PathBuf::from(&args[3]),
        output: PathBuf::from(&args[4]),
    })
}
