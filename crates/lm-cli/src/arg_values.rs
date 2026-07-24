use crate::command_types::{CodecOperation, Direction, ProfileExportKind, ProfileImportKind};
use lm_project::GraphicsCompression;
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArgsError(pub String);

impl fmt::Display for ArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ArgsError {}

pub(crate) fn parse_mapper(value: &str) -> Result<Mapper, ArgsError> {
    match value.to_ascii_lowercase().as_str() {
        "lorom" => Ok(Mapper::LoRom),
        "exlorom" => Ok(Mapper::ExLoRom),
        "sa1" | "sa-1" => Ok(Mapper::Sa1),
        _ => Err(ArgsError(format!("unknown mapper {value}"))),
    }
}
pub(crate) fn parse_profile_export_kind(value: &str) -> Result<ProfileExportKind, ArgsError> {
    match value {
        "native-assets" => Ok(ProfileExportKind::NativeAssets),
        "level" => Ok(ProfileExportKind::Level),
        "layer2" => Ok(ProfileExportKind::Layer2),
        "map16" => Ok(ProfileExportKind::Map16),
        "graphics" => Ok(ProfileExportKind::Graphics),
        "palette" => Ok(ProfileExportKind::Palette),
        "exanimation" => Ok(ProfileExportKind::ExAnimation),
        "expanded-settings" => Ok(ProfileExportKind::ExpandedSettings),
        "overworld" => Ok(ProfileExportKind::Overworld),
        _ => Err(ArgsError(format!("unknown profile export domain {value}"))),
    }
}
pub(crate) fn parse_profile_import_kind(value: &str) -> Result<ProfileImportKind, ArgsError> {
    match value {
        "native-assets" => Ok(ProfileImportKind::NativeAssets),
        "level" => Ok(ProfileImportKind::Level),
        "map16" => Ok(ProfileImportKind::Map16),
        "graphics" => Ok(ProfileImportKind::Graphics),
        "palette" => Ok(ProfileImportKind::Palette),
        "exanimation" => Ok(ProfileImportKind::ExAnimation),
        "expanded-settings" => Ok(ProfileImportKind::ExpandedSettings),
        "overworld" => Ok(ProfileImportKind::Overworld),
        _ => Err(ArgsError(format!("unknown profile import domain {value}"))),
    }
}
pub(crate) fn parse_direction(value: &str) -> Result<Direction, ArgsError> {
    match value {
        "snes-to-pc" => Ok(Direction::SnesToPc),
        "pc-to-snes" => Ok(Direction::PcToSnes),
        _ => Err(ArgsError(format!("unknown direction {value}"))),
    }
}
pub(crate) fn parse_codec_operation(value: &str) -> Result<CodecOperation, ArgsError> {
    match value {
        "lz2-decode" => Ok(CodecOperation::Lz2Decode),
        "lz2-encode" => Ok(CodecOperation::Lz2Encode),
        "lz3-decode" => Ok(CodecOperation::Lz3Decode),
        "lz3-encode" => Ok(CodecOperation::Lz3Encode),
        "rle-decode" => Ok(CodecOperation::RleDecode),
        "rle-encode" => Ok(CodecOperation::RleEncode),
        "rle-sized-encode" => Ok(CodecOperation::RleSizedEncode),
        _ => Err(ArgsError(format!("unknown codec operation {value}"))),
    }
}

pub(crate) fn parse_graphics_compression(value: &str) -> Result<GraphicsCompression, ArgsError> {
    match value {
        "lz2" => Ok(GraphicsCompression::Lz2),
        "lz3" => Ok(GraphicsCompression::Lz3),
        _ => Err(ArgsError(format!("unknown graphics compression {value}"))),
    }
}

pub(crate) fn parse_sprite_format(value: &str) -> Result<bool, ArgsError> {
    match value {
        "legacy" => Ok(false),
        "expanded" => Ok(true),
        _ => Err(ArgsError(format!("unknown sprite format {value}"))),
    }
}
pub(crate) fn parse_number(value: &str) -> Result<u32, ArgsError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    u32::from_str_radix(value, 16)
        .map_err(|_| ArgsError(format!("invalid hexadecimal value {value}")))
}

pub(crate) fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, ArgsError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() || value.len() % 2 != 0 {
        return Err(ArgsError(
            "hex byte string must have a positive even length".into(),
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| ArgsError(format!("invalid hex byte string {value}")))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| ArgsError(format!("invalid hex byte string {value}")))
        })
        .collect()
}
