use crate::arg_values::{parse_mapper, parse_number};
use crate::command_types::{
    Command, ExAnimationTransferCommand, ExpandedSettingsTransferCommand, Map16TransferCommand,
    OverworldTransferCommand, PaletteTransferCommand,
};
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

#[path = "arg_transfers/exanimation.rs"]
mod exanimation;
#[path = "arg_transfers/expanded_settings.rs"]
mod expanded_settings;
#[path = "arg_transfers/graphics.rs"]
mod graphics;
#[path = "arg_transfers/level.rs"]
mod level;
#[path = "arg_transfers/map16.rs"]
mod map16;
#[path = "arg_transfers/overworld.rs"]
mod overworld;
#[path = "arg_transfers/palette.rs"]
mod palette;

pub(crate) use exanimation::parse_exanimation_transfer;
pub(crate) use expanded_settings::parse_expanded_settings_transfer;
pub(crate) use graphics::parse_graphics_transfer;
pub(crate) use level::parse_level_transfer;
pub(crate) use map16::parse_map16_transfer;
pub(crate) use overworld::parse_overworld_transfer;
pub(crate) use palette::parse_palette_transfer;
