use super::{
    MAX_SSC_DISPLAY_TILES, MAX_SSC_PALETTE_RECORDS, SscDirective, SscDisplayTile, SscEntry,
    SscRemapRange, SscSpriteSelector,
};

pub(super) fn line(line: &[u8]) -> Option<SscEntry> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let mut fields = line.splitn(3, |byte| *byte == b'\t');
    let number = hex(fields.next()?)?;
    let flags = hex(fields.next()?)?;
    let payload = fields.next().unwrap_or_default();

    if number == 0x1_0000 && flags < 0x1_0000 {
        return Some(SscEntry {
            selector: None,
            flags,
            directive: SscDirective::TileRemap {
                mode: u8::try_from(flags & 3).ok()?,
                ranges: remap_ranges(payload, tile_target_bias(flags & 3)),
            },
        });
    }
    if number == 0x2_0000 && flags < 0x1_0000 {
        return Some(SscEntry {
            selector: None,
            flags,
            directive: SscDirective::PaletteRemap(remap_ranges(payload, 0)),
        });
    }
    if number >= 0x100 || flags >= 0x100_0000 {
        return None;
    }

    let directive = if flags & 8 != 0 {
        SscDirective::Palette(palette(payload))
    } else if flags & 2 != 0 {
        SscDirective::Display(display(payload))
    } else if flags & 4 == 0 {
        SscDirective::Description(description(payload))
    } else {
        return None;
    };
    Some(SscEntry {
        selector: Some(selector(u8::try_from(number).ok()?, flags)),
        flags,
        directive,
    })
}

fn selector(sprite_number: u8, flags: u32) -> SscSpriteSelector {
    let record = u8::try_from((flags >> 16) & 0x1f).unwrap_or_default();
    SscSpriteSelector {
        sprite_number,
        extra_bits: u8::try_from((flags >> 4) & 3).unwrap_or_default(),
        index: u16::from(sprite_number) + u16::try_from((flags & 0x30) << 4).unwrap_or_default(),
        width: u8::try_from((flags >> 8) & 0xf).unwrap_or_default(),
        height: u8::try_from((flags >> 12) & 0xf).unwrap_or_default(),
        record_length: (record != 0).then_some(record.clamp(3, 15)),
        alternate: flags & 1 != 0,
        global_slot: flags & 4 != 0,
    }
}

fn description(payload: &[u8]) -> String {
    let mut output = Vec::with_capacity(payload.len());
    let mut index = 0;
    while index < payload.len() && output.len() < 0x5ff {
        let byte = payload[index];
        index += 1;
        if byte == b'\\' && index < payload.len() {
            let escaped = payload[index];
            index += 1;
            output.push(match escaped {
                b'\\' => b'\\',
                b'n' => b'\n',
                _ => b' ',
            });
        } else {
            output.push(byte);
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn display(payload: &[u8]) -> Vec<SscDisplayTile> {
    let mut result = Vec::new();
    for item in payload.split(|byte| *byte == b';') {
        if result.len() >= MAX_SSC_DISPLAY_TILES {
            break;
        }
        let Some((x, rest)) = decimal_field(item) else {
            continue;
        };
        let Some((y, value)) = decimal_field(rest) else {
            continue;
        };
        let value = trim(value);
        if let Some(text) = value.strip_prefix(b"*").and_then(|v| v.strip_suffix(b"*")) {
            append_text(&mut result, x, y, text);
        } else if let Some(tile) = hex(value).and_then(|v| u16::try_from(v & 0x7fff).ok()) {
            result.push(SscDisplayTile { x, y, tile });
        }
    }
    result
}

fn append_text(result: &mut Vec<SscDisplayTile>, x: i16, mut y: i16, text: &[u8]) {
    let mut column = 0_i16;
    for &byte in text {
        if result.len() >= MAX_SSC_DISPLAY_TILES {
            break;
        }
        if byte == b'\n' {
            y = y.saturating_add(8);
            column = 0;
            continue;
        }
        let glyph_x = x.saturating_add(column.saturating_mul(8));
        if column & 1 == 0 && result.len() + 1 < MAX_SSC_DISPLAY_TILES {
            result.push(SscDisplayTile {
                x: glyph_x,
                y,
                tile: 0x3c7c,
            });
        }
        result.push(SscDisplayTile {
            x: glyph_x,
            y,
            tile: 0x3c00 + u16::from(byte),
        });
        column = column.saturating_add(1);
    }
}

fn palette(payload: &[u8]) -> Vec<[u16; 4]> {
    payload
        .split(|byte| *byte == b';')
        .filter_map(|item| {
            let mut words = item.split(|byte| *byte == b',').map(trim).map(hex);
            Some([
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
            ])
        })
        .take(MAX_SSC_PALETTE_RECORDS)
        .collect()
}

fn remap_ranges(payload: &[u8], bias: u16) -> Vec<SscRemapRange> {
    payload
        .split(|byte| *byte == b';')
        .filter_map(|item| {
            let (range, target) = split_once(item, b',')?;
            let (first, last) = split_once(range, b'-')?;
            let first = u16::try_from(hex(first)?).ok()?;
            let last = u16::try_from(hex(last)?).ok()?;
            let target = u16::try_from(hex(target)?).ok()?.checked_add(bias)?;
            (first <= last && first < 0x3c00 && target < 0x4000).then_some(SscRemapRange {
                first,
                last: last.min(0x3bff),
                target,
            })
        })
        .take(0x3c00)
        .collect()
}

const fn tile_target_bias(mode: u32) -> u16 {
    match mode {
        0 => 0x2000,
        1 => 0,
        2 => 0x0400,
        _ => 0x0900,
    }
}

fn decimal_field(input: &[u8]) -> Option<(i16, &[u8])> {
    let (value, rest) = split_once(input, b',')?;
    Some((std::str::from_utf8(trim(value)).ok()?.parse().ok()?, rest))
}

fn split_once(input: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    let at = input.iter().position(|byte| *byte == needle)?;
    Some((&input[..at], &input[at + 1..]))
}

fn hex(input: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(trim(input)).ok()?;
    (!text.is_empty()).then(|| u32::from_str_radix(text, 16).ok())?
}

fn trim(mut input: &[u8]) -> &[u8] {
    while input.first().is_some_and(u8::is_ascii_whitespace) {
        input = &input[1..];
    }
    while input.last().is_some_and(u8::is_ascii_whitespace) {
        input = &input[..input.len() - 1];
    }
    input
}
