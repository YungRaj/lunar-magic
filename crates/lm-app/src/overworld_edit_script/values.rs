use super::OverworldEditScriptError;
use lm_app::OverworldLayerId;
use lm_graphics::PaletteEntryOwner;
use lm_overworld::Submap;

pub(super) fn parse_owner(
    line: usize,
    value: &str,
) -> Result<PaletteEntryOwner, OverworldEditScriptError> {
    match value {
        "editable" => Ok(PaletteEntryOwner::Editable),
        "fixed" => Ok(PaletteEntryOwner::Fixed),
        _ => value.strip_prefix("exanimation:").map_or_else(
            || {
                Err(OverworldEditScriptError::InvalidOwner {
                    line,
                    value: value.into(),
                })
            },
            |record| {
                Ok(PaletteEntryOwner::ExAnimation {
                    record: hex(line, record)?,
                })
            },
        ),
    }
}

pub(super) fn parse_layer(
    line: usize,
    value: &str,
) -> Result<OverworldLayerId, OverworldEditScriptError> {
    match value {
        "1" => Ok(OverworldLayerId::Layer1),
        "2" => Ok(OverworldLayerId::Layer2),
        _ => Err(OverworldEditScriptError::InvalidLayer {
            line,
            value: value.into(),
        }),
    }
}

pub(super) fn parse_submap(line: usize, value: &str) -> Result<Submap, OverworldEditScriptError> {
    let value = hex::<u8>(line, value)?;
    Submap::decode(value).ok_or_else(|| OverworldEditScriptError::InvalidSubmap {
        line,
        value: format!("{value:02x}"),
    })
}

pub(super) fn parse_bytes(line: usize, value: &str) -> Result<Vec<u8>, OverworldEditScriptError> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OverworldEditScriptError::InvalidExtra {
            line,
            value: value.into(),
        });
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                OverworldEditScriptError::InvalidExtra {
                    line,
                    value: value.into(),
                }
            })
        })
        .collect()
}

pub(super) fn hex<T>(line: usize, value: &str) -> Result<T, OverworldEditScriptError>
where
    T: TryFrom<u64>,
{
    let value = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(value, 16)
        .ok()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| OverworldEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}
