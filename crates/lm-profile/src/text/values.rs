use super::RevisionProfileError;
use crate::RevisionProfile;
use crate::text_schema::{EXPANDED_SETTINGS_KEYS, INSTALLATION_KEYS, LAYER2_KEYS, SCALARS, TABLES};
use lm_project::GraphicsCompression;
use lm_rom::{Mapper, Region, SupportedGame};
use std::collections::BTreeMap;

pub(super) fn parse_values(input: &str) -> Result<BTreeMap<String, String>, RevisionProfileError> {
    if input.len() > RevisionProfile::MAX_TEXT_LEN {
        return Err(RevisionProfileError::TextTooLong {
            actual: input.len(),
            maximum: RevisionProfile::MAX_TEXT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(RevisionProfileError::MissingMagic)?
        .trim();
    if magic.len() > RevisionProfile::MAX_LINE_LEN {
        return Err(RevisionProfileError::LineTooLong {
            line: 1,
            actual: magic.len(),
            maximum: RevisionProfile::MAX_LINE_LEN,
        });
    }
    if magic != RevisionProfile::MAGIC {
        return Err(RevisionProfileError::UnsupportedVersion(magic.into()));
    }
    let known = known_keys();
    let mut values = BTreeMap::new();
    for (index, raw) in lines.enumerate() {
        let line_number = index + 2;
        if line_number > RevisionProfile::MAX_LINES {
            return Err(RevisionProfileError::TooManyLines {
                maximum: RevisionProfile::MAX_LINES,
            });
        }
        if raw.len() > RevisionProfile::MAX_LINE_LEN {
            return Err(RevisionProfileError::LineTooLong {
                line: line_number,
                actual: raw.len(),
                maximum: RevisionProfile::MAX_LINE_LEN,
            });
        }
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or(RevisionProfileError::MalformedLine(line_number))?;
        let key = key.trim();
        if !known.iter().any(|known| *known == key) {
            return Err(RevisionProfileError::UnknownKey {
                line: line_number,
                key: key.into(),
            });
        }
        if values
            .insert(key.to_owned(), value.trim().to_owned())
            .is_some()
        {
            return Err(RevisionProfileError::DuplicateKey(key.into()));
        }
    }
    Ok(values)
}

fn known_keys() -> Vec<String> {
    let mut keys = vec![
        "name".into(),
        "game".into(),
        "region".into(),
        "revision".into(),
        "mapper".into(),
        "graphics.compression".into(),
        "sprite_lengths".into(),
        "exanimation_double_size_modes".into(),
        "level.sprites.encoding".into(),
        "level.sprites.bank_offset".into(),
        "level.sprites.bank_stride".into(),
    ];
    for table in TABLES {
        for suffix in ["offset", "entries", "stride"] {
            keys.push(format!("{table}.{suffix}"));
        }
    }
    keys.extend(SCALARS.into_iter().map(str::to_owned));
    keys.extend(EXPANDED_SETTINGS_KEYS.into_iter().map(str::to_owned));
    keys.extend(LAYER2_KEYS.into_iter().map(str::to_owned));
    keys.extend(INSTALLATION_KEYS.into_iter().map(str::to_owned));
    keys
}

pub(super) fn take(
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<String, RevisionProfileError> {
    values
        .remove(key)
        .ok_or_else(|| RevisionProfileError::MissingKey(key.into()))
}

pub(super) fn parse_graphics_compression(
    value: &str,
) -> Result<GraphicsCompression, RevisionProfileError> {
    match value {
        "lz2" => Ok(GraphicsCompression::Lz2),
        "lz3" => Ok(GraphicsCompression::Lz3),
        _ => Err(RevisionProfileError::InvalidGraphicsCompression(
            value.into(),
        )),
    }
}

pub(super) fn number(
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<usize, RevisionProfileError> {
    let value = take(values, key)?;
    let result = value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| usize::from_str_radix(hex, 16));
    result.map_err(|_| RevisionProfileError::InvalidNumber {
        key: key.into(),
        value,
    })
}

pub(super) fn signed_number(
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<isize, RevisionProfileError> {
    let value = take(values, key)?;
    let (negative, magnitude) = value
        .strip_prefix('-')
        .map_or((false, value.as_str()), |rest| (true, rest));
    let magnitude = magnitude
        .strip_prefix("0x")
        .map_or_else(
            || magnitude.parse::<usize>(),
            |hex| usize::from_str_radix(hex, 16),
        )
        .map_err(|_| RevisionProfileError::InvalidNumber {
            key: key.into(),
            value: value.clone(),
        })?;
    let magnitude =
        isize::try_from(magnitude).map_err(|_| RevisionProfileError::InvalidNumber {
            key: key.into(),
            value: value.clone(),
        })?;
    if negative {
        magnitude
            .checked_neg()
            .ok_or(RevisionProfileError::InvalidNumber {
                key: key.into(),
                value,
            })
    } else {
        Ok(magnitude)
    }
}

pub(super) fn byte(
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<u8, RevisionProfileError> {
    let value = number(values, key)?;
    u8::try_from(value).map_err(|_| RevisionProfileError::InvalidNumber {
        key: key.into(),
        value: value.to_string(),
    })
}

pub(super) fn boolean(
    values: &mut BTreeMap<String, String>,
    key: &str,
) -> Result<bool, RevisionProfileError> {
    let value = take(values, key)?;
    match value.as_str() {
        "0" | "false" => Ok(false),
        "1" | "true" => Ok(true),
        _ => Err(RevisionProfileError::InvalidBoolean {
            key: key.into(),
            value,
        }),
    }
}

pub(super) fn parse_mapper(value: &str) -> Result<Mapper, RevisionProfileError> {
    match value {
        "lorom" => Ok(Mapper::LoRom),
        "exlorom" => Ok(Mapper::ExLoRom),
        "sa1" => Ok(Mapper::Sa1),
        _ => Err(RevisionProfileError::InvalidMapper(value.into())),
    }
}

pub(super) fn parse_game(value: &str) -> Result<SupportedGame, RevisionProfileError> {
    match value {
        "super-mario-world" => Ok(SupportedGame::SuperMarioWorld),
        "all-stars-and-world" => Ok(SupportedGame::AllStarsAndWorld),
        _ => Err(RevisionProfileError::InvalidGame(value.into())),
    }
}

pub(super) fn parse_region(value: &str) -> Result<Region, RevisionProfileError> {
    match value {
        "japan" => Ok(Region::Japan),
        "north-america" => Ok(Region::NorthAmerica),
        _ => Err(RevisionProfileError::InvalidRegion(value.into())),
    }
}

pub(super) fn hex(
    value: &str,
    key: &'static str,
    expected: usize,
) -> Result<Vec<u8>, RevisionProfileError> {
    if value.len() != expected * 2 {
        return Err(RevisionProfileError::InvalidTableLength {
            key,
            actual: value.len() / 2,
            expected,
        });
    }
    (0..expected)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| RevisionProfileError::InvalidHex { key })
        })
        .collect()
}
