//! Lunar Magic's ROM-adjacent `.sscov` overworld sprite display sidecar.

use std::{collections::BTreeMap, fmt, fmt::Write};

pub const SSCOV_MAX_BYTES: usize = 1 << 20;
pub const SSCOV_MAX_SPRITE_MAP16_TILE: u16 = 0x0cff;
pub const SSCOV_MAX_PARTS: usize = 0x100;
pub const SSCOV_MAX_ABSOLUTE_OFFSET: i32 = 0x2fff;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpriteSidecar {
    pub tooltips: BTreeMap<u16, NativeOverworldSpriteTooltip>,
    pub appearances: BTreeMap<u16, NativeOverworldSpriteAppearance>,
    pub graphics_ranges: Vec<NativeOverworldSpriteRange>,
    pub palette_ranges: Vec<NativeOverworldSpriteRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpriteTooltip {
    pub disable_original_position_text: bool,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpriteAppearance {
    pub shadow: bool,
    pub display: NativeOverworldSpriteDisplay,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldSpriteDisplay {
    Tiles(Vec<NativeOverworldSpriteMap16Part>),
    Label { x: i16, y: i16, text: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpriteMap16Part {
    pub x: i16,
    pub y: i16,
    pub tile: u16,
    pub translucent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeOverworldSpriteRange {
    pub kind: u16,
    pub first_tile: u16,
    pub last_tile: u16,
    pub base: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldSpriteSidecarError {
    TooLarge(usize),
    InvalidUtf8,
    InvalidSpriteId(u32),
    UnsupportedType(u32),
    EmptyAppearance(u16),
    TooManyParts {
        sprite_id: u16,
        count: usize,
    },
    InvalidPart {
        sprite_id: u16,
        value: String,
    },
    OffsetOutOfRange {
        sprite_id: u16,
        axis: char,
        value: i32,
    },
    TileOutOfRange {
        sprite_id: u16,
        tile: u32,
    },
    InvalidRange(String),
    RangeOutOfBounds {
        first: u32,
        last: u32,
        base: u32,
    },
}

impl fmt::Display for NativeOverworldSpriteSidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic .sscov sidecar: {self:?}")
    }
}

impl std::error::Error for NativeOverworldSpriteSidecarError {}

impl NativeOverworldSpriteSidecar {
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeOverworldSpriteSidecarError> {
        if bytes.len() > SSCOV_MAX_BYTES {
            return Err(NativeOverworldSpriteSidecarError::TooLarge(bytes.len()));
        }
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        let text = std::str::from_utf8(bytes)
            .map_err(|_| NativeOverworldSpriteSidecarError::InvalidUtf8)?;
        let mut output = Self {
            tooltips: BTreeMap::new(),
            appearances: BTreeMap::new(),
            graphics_ranges: Vec::new(),
            palette_ranges: Vec::new(),
        };
        for raw_line in text.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            let Some((id_text, kind_text, payload)) = split_header(line) else {
                continue;
            };
            let Ok(id) = u32::from_str_radix(id_text, 16) else {
                continue;
            };
            let Ok(kind) = u32::from_str_radix(kind_text, 16) else {
                continue;
            };
            match id {
                0x10000 => output.graphics_ranges.extend(parse_ranges(payload, kind)?),
                0x20000 => output.palette_ranges.extend(parse_ranges(payload, kind)?),
                0..=0xff => output.decode_sprite_line(id as u16, kind, payload)?,
                _ => return Err(NativeOverworldSpriteSidecarError::InvalidSpriteId(id)),
            }
        }
        Ok(output)
    }

    pub fn encode(&self) -> Result<Vec<u8>, NativeOverworldSpriteSidecarError> {
        let mut output = String::new();
        for (&sprite_id, tooltip) in &self.tooltips {
            let (id, custom) = encode_sprite_id(sprite_id)?;
            let kind = custom | u32::from(tooltip.disable_original_position_text);
            writeln!(
                output,
                "{id:02X}\t{kind:X}\t{}",
                encode_escapes(&tooltip.text)
            )
            .unwrap();
        }
        for (&sprite_id, appearance) in &self.appearances {
            let (id, custom) = encode_sprite_id(sprite_id)?;
            let kind = custom | 2 | u32::from(appearance.shadow);
            write!(output, "{id:02X}\t{kind:X}\t").unwrap();
            match &appearance.display {
                NativeOverworldSpriteDisplay::Tiles(parts) => {
                    if parts.is_empty() {
                        return Err(NativeOverworldSpriteSidecarError::EmptyAppearance(
                            sprite_id,
                        ));
                    }
                    if parts.len() > SSCOV_MAX_PARTS {
                        return Err(NativeOverworldSpriteSidecarError::TooManyParts {
                            sprite_id,
                            count: parts.len(),
                        });
                    }
                    for (index, part) in parts.iter().enumerate() {
                        validate_offsets(sprite_id, i32::from(part.x), i32::from(part.y))?;
                        if part.tile > SSCOV_MAX_SPRITE_MAP16_TILE {
                            return Err(NativeOverworldSpriteSidecarError::TileOutOfRange {
                                sprite_id,
                                tile: u32::from(part.tile),
                            });
                        }
                        if index != 0 {
                            output.push(' ');
                        }
                        let tile = part.tile | if part.translucent { 0x8000 } else { 0 };
                        write!(output, "{},{},{tile:X}", part.x, part.y).unwrap();
                    }
                }
                NativeOverworldSpriteDisplay::Label { x, y, text } => {
                    validate_offsets(sprite_id, i32::from(*x), i32::from(*y))?;
                    write!(output, "{x},{y},*{}*", encode_escapes(text)).unwrap();
                }
            }
            output.push('\n');
        }
        encode_ranges(&mut output, 0x10000, &self.graphics_ranges)?;
        encode_ranges(&mut output, 0x20000, &self.palette_ranges)?;
        if output.len() > SSCOV_MAX_BYTES {
            return Err(NativeOverworldSpriteSidecarError::TooLarge(output.len()));
        }
        Ok(output.into_bytes())
    }

    fn decode_sprite_line(
        &mut self,
        sprite_id: u16,
        kind: u32,
        payload: &str,
    ) -> Result<(), NativeOverworldSpriteSidecarError> {
        if kind & !0x13 != 0 || kind & 0x0c != 0 {
            return Err(NativeOverworldSpriteSidecarError::UnsupportedType(kind));
        }
        let sprite_id = sprite_id + if kind & 0x10 != 0 { 0x100 } else { 0 };
        if kind & 2 == 0 {
            self.tooltips.insert(
                sprite_id,
                NativeOverworldSpriteTooltip {
                    disable_original_position_text: kind & 1 != 0,
                    text: decode_escapes(payload),
                },
            );
            return Ok(());
        }
        let display = if let Some((x, y, label)) = parse_label(sprite_id, payload)? {
            NativeOverworldSpriteDisplay::Label { x, y, text: label }
        } else {
            let mut parts = Vec::new();
            for value in payload.split_ascii_whitespace() {
                if parts.len() == SSCOV_MAX_PARTS {
                    return Err(NativeOverworldSpriteSidecarError::TooManyParts {
                        sprite_id,
                        count: parts.len() + 1,
                    });
                }
                parts.push(parse_part(sprite_id, value)?);
            }
            if parts.is_empty() {
                return Err(NativeOverworldSpriteSidecarError::EmptyAppearance(
                    sprite_id,
                ));
            }
            NativeOverworldSpriteDisplay::Tiles(parts)
        };
        self.appearances.insert(
            sprite_id,
            NativeOverworldSpriteAppearance {
                shadow: kind & 1 != 0,
                display,
            },
        );
        Ok(())
    }
}

fn split_header(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim_start_matches(char::is_whitespace);
    let id_end = line.find(char::is_whitespace)?;
    let id = &line[..id_end];
    let remainder = line[id_end..].trim_start_matches(char::is_whitespace);
    let kind_end = remainder.find(char::is_whitespace)?;
    let kind = &remainder[..kind_end];
    let payload = remainder[kind_end..].trim_start_matches(char::is_whitespace);
    (!payload.is_empty()).then_some((id, kind, payload))
}

fn parse_part(
    sprite_id: u16,
    value: &str,
) -> Result<NativeOverworldSpriteMap16Part, NativeOverworldSpriteSidecarError> {
    let mut fields = value.split(',');
    let parsed = (|| {
        let x = fields.next()?.parse::<i32>().ok()?;
        let y = fields.next()?.parse::<i32>().ok()?;
        let tile = u32::from_str_radix(fields.next()?, 16).ok()?;
        (fields.next().is_none()).then_some((x, y, tile))
    })();
    let Some((x, y, tile)) = parsed else {
        return Err(NativeOverworldSpriteSidecarError::InvalidPart {
            sprite_id,
            value: value.into(),
        });
    };
    validate_offsets(sprite_id, x, y)?;
    let base_tile = tile & 0x7fff;
    if base_tile > u32::from(SSCOV_MAX_SPRITE_MAP16_TILE) || tile & !0xffff != 0 {
        return Err(NativeOverworldSpriteSidecarError::TileOutOfRange { sprite_id, tile });
    }
    Ok(NativeOverworldSpriteMap16Part {
        x: x as i16,
        y: y as i16,
        tile: base_tile as u16,
        translucent: tile & 0x8000 != 0,
    })
}

fn parse_label(
    sprite_id: u16,
    payload: &str,
) -> Result<Option<(i16, i16, String)>, NativeOverworldSpriteSidecarError> {
    let mut fields = payload.splitn(3, ',');
    let Some(x_text) = fields.next() else {
        return Ok(None);
    };
    let Some(y_text) = fields.next() else {
        return Ok(None);
    };
    let Some(label) = fields.next() else {
        return Ok(None);
    };
    if !label.starts_with('*') || !label.ends_with('*') || label.len() < 2 {
        return Ok(None);
    }
    let x = x_text
        .parse::<i32>()
        .map_err(|_| NativeOverworldSpriteSidecarError::InvalidPart {
            sprite_id,
            value: payload.into(),
        })?;
    let y = y_text
        .parse::<i32>()
        .map_err(|_| NativeOverworldSpriteSidecarError::InvalidPart {
            sprite_id,
            value: payload.into(),
        })?;
    validate_offsets(sprite_id, x, y)?;
    Ok(Some((
        x as i16,
        y as i16,
        decode_escapes(&label[1..label.len() - 1]),
    )))
}

fn validate_offsets(
    sprite_id: u16,
    x: i32,
    y: i32,
) -> Result<(), NativeOverworldSpriteSidecarError> {
    for (axis, value) in [('x', x), ('y', y)] {
        if value.unsigned_abs() > SSCOV_MAX_ABSOLUTE_OFFSET as u32 {
            return Err(NativeOverworldSpriteSidecarError::OffsetOutOfRange {
                sprite_id,
                axis,
                value,
            });
        }
    }
    Ok(())
}

fn parse_ranges(
    payload: &str,
    kind: u32,
) -> Result<Vec<NativeOverworldSpriteRange>, NativeOverworldSpriteSidecarError> {
    if kind >= 0x10000 {
        return Err(NativeOverworldSpriteSidecarError::UnsupportedType(kind));
    }
    let mut output = Vec::new();
    for value in payload.split_ascii_whitespace() {
        let Some((range, base)) = value.split_once(',') else {
            return Err(NativeOverworldSpriteSidecarError::InvalidRange(
                value.into(),
            ));
        };
        let Some((first, last)) = range.split_once('-') else {
            return Err(NativeOverworldSpriteSidecarError::InvalidRange(
                value.into(),
            ));
        };
        let values = (|| {
            Some((
                u32::from_str_radix(first, 16).ok()?,
                u32::from_str_radix(last, 16).ok()?,
                u32::from_str_radix(base, 16).ok()?,
            ))
        })();
        let Some((first, last, base)) = values else {
            return Err(NativeOverworldSpriteSidecarError::InvalidRange(
                value.into(),
            ));
        };
        if first > last || last > 0x0bff || base > 0xffff {
            return Err(NativeOverworldSpriteSidecarError::RangeOutOfBounds { first, last, base });
        }
        output.push(NativeOverworldSpriteRange {
            kind: kind as u16,
            first_tile: first as u16,
            last_tile: last as u16,
            base: base as u16,
        });
    }
    Ok(output)
}

fn encode_sprite_id(sprite_id: u16) -> Result<(u16, u32), NativeOverworldSpriteSidecarError> {
    match sprite_id {
        0..=0xff => Ok((sprite_id, 0)),
        0x100..=0x1ff => Ok((sprite_id - 0x100, 0x10)),
        _ => Err(NativeOverworldSpriteSidecarError::InvalidSpriteId(
            u32::from(sprite_id),
        )),
    }
}

fn encode_ranges(
    output: &mut String,
    id: u32,
    ranges: &[NativeOverworldSpriteRange],
) -> Result<(), NativeOverworldSpriteSidecarError> {
    for range in ranges {
        if range.kind as u32 >= 0x10000
            || range.first_tile > range.last_tile
            || range.last_tile > 0x0bff
        {
            return Err(NativeOverworldSpriteSidecarError::RangeOutOfBounds {
                first: u32::from(range.first_tile),
                last: u32::from(range.last_tile),
                base: u32::from(range.base),
            });
        }
        writeln!(
            output,
            "{id:X}\t{:X}\t{:X}-{:X},{:X}",
            range.kind, range.first_tile, range.last_tile, range.base
        )
        .unwrap();
    }
    Ok(())
}

fn decode_escapes(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some('n') => output.push('\n'),
                Some('\\') => output.push('\\'),
                Some(_) => output.push(' '),
                None => output.push(' '),
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn encode_escapes(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\n' | '\r' => output.push_str("\\n"),
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_overworld_forms_decode_with_custom_ids_shadows_and_ranges() {
        let decoded = NativeOverworldSpriteSidecar::decode(
            b"\xEF\xBB\xBFAB\t0\tA custom tooltip.\\nSecond line.\r\n\
AB\t3\t0,0,105 -8,8,8105 8,8,105\n\
2A\t12\t0,0,C7C\n\
2B\t13\t0,0,*Custom\\nLabel*\n\
10000\t0\t230-23F,0 240-24F,80\n\
20000\t0\t230-24F,20\n",
        )
        .unwrap();
        assert_eq!(
            decoded.tooltips[&0xab].text,
            "A custom tooltip.\nSecond line."
        );
        assert!(decoded.appearances[&0xab].shadow);
        let NativeOverworldSpriteDisplay::Tiles(parts) = &decoded.appearances[&0xab].display else {
            panic!("expected tiles");
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].x, -8);
        assert!(parts[1].translucent);
        assert_eq!(parts[1].tile, 0x105);
        assert!(decoded.appearances.contains_key(&0x12a));
        assert!(decoded.appearances[&0x12b].shadow);
        assert_eq!(
            decoded.appearances[&0x12b].display,
            NativeOverworldSpriteDisplay::Label {
                x: 0,
                y: 0,
                text: "Custom\nLabel".into(),
            }
        );
        assert_eq!(decoded.graphics_ranges.len(), 2);
        assert_eq!(decoded.palette_ranges[0].last_tile, 0x24f);
        assert_eq!(
            NativeOverworldSpriteSidecar::decode(&decoded.encode().unwrap()).unwrap(),
            decoded
        );
    }

    #[test]
    fn later_sprite_entries_replace_earlier_original_state() {
        let decoded = NativeOverworldSpriteSidecar::decode(
            b"01  0   first\n01 1 second\n01 2 0,0,1\n01 3 8,8,2\n",
        )
        .unwrap();
        assert_eq!(decoded.tooltips[&1].text, "second");
        assert!(decoded.tooltips[&1].disable_original_position_text);
        assert_eq!(
            decoded.appearances[&1],
            NativeOverworldSpriteAppearance {
                shadow: true,
                display: NativeOverworldSpriteDisplay::Tiles(vec![
                    NativeOverworldSpriteMap16Part {
                        x: 8,
                        y: 8,
                        tile: 2,
                        translucent: false,
                    },
                ]),
            }
        );
    }

    #[test]
    fn malformed_or_unbounded_native_values_are_rejected() {
        assert!(matches!(
            NativeOverworldSpriteSidecar::decode(&vec![b' '; SSCOV_MAX_BYTES + 1]),
            Err(NativeOverworldSpriteSidecarError::TooLarge(_))
        ));
        assert!(matches!(
            NativeOverworldSpriteSidecar::decode(b"01 2 12288,0,1\n"),
            Err(NativeOverworldSpriteSidecarError::OffsetOutOfRange { .. })
        ));
        assert!(matches!(
            NativeOverworldSpriteSidecar::decode(b"01 2 0,0,D00\n"),
            Err(NativeOverworldSpriteSidecarError::TileOutOfRange { .. })
        ));
        assert!(matches!(
            NativeOverworldSpriteSidecar::decode(b"10000 0 BFF-C00,0\n"),
            Err(NativeOverworldSpriteSidecarError::RangeOutOfBounds { .. })
        ));
    }
}
