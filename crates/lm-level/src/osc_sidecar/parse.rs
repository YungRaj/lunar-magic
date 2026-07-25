use super::{
    MAX_OSC_ATTRIBUTES, MAX_OSC_DISPLAY_TILES, MAX_OSC_VALUE_RECORDS, OscDirective, OscDisplayTile,
    OscEntry, OscObjectSelector,
};

pub(super) fn line(line: &[u8]) -> Option<OscEntry> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let mut fields = line.splitn(4, |byte| *byte == b'\t');
    let object_type = u8::try_from(hex(fields.next()?)?).ok()?;
    let parameter = u8::try_from(hex(fields.next()?)?).ok()?;
    let flags = hex(fields.next()?)?;
    let payload = fields.next().unwrap_or_default();
    if object_type >= 0x40 || flags >= 0x100_0000 {
        return None;
    }
    let selectors = selectors(object_type, parameter, flags)?;
    let display = flags & 2 != 0;
    let values = flags & 8 != 0;
    let directive = match (display, values) {
        (false, false) => OscDirective::Description(description(payload)),
        (true, false) => OscDirective::Display(display_tiles(payload)),
        (false, true) => OscDirective::Values(value_records(payload)),
        (true, true) => OscDirective::Attributes(attributes(payload)),
    };
    Some(OscEntry {
        selectors,
        flags,
        directive,
    })
}

fn selectors(object_type: u8, parameter: u8, flags: u32) -> Option<Vec<OscObjectSelector>> {
    let width = u8::try_from((flags >> 8) & 0xf).ok()?;
    let height = u8::try_from((flags >> 12) & 0xf).ok()?;
    let length = u8::try_from((flags >> 16) & 0x1f).ok()?;
    let common = |variant, index| OscObjectSelector {
        object_type,
        parameter,
        variant,
        index,
        width,
        height,
        record_length: (length != 0).then_some(length.clamp(2, 15)),
        alternate_linear: flags & 4 != 0,
    };
    if object_type == 0 {
        return Some(vec![common(0, 0x140 + u16::from(parameter))]);
    }
    if object_type == 0x2d {
        return Some(vec![common(0, 0x240 + u16::from(parameter))]);
    }
    if flags & 1 != 0 {
        let variant = u8::try_from((flags >> 4) & 7).ok()?;
        return (variant <= 4).then(|| {
            vec![common(
                variant,
                u16::from(variant) * 0x40 + u16::from(object_type),
            )]
        });
    }
    Some(
        (0_u8..5)
            .map(|variant| common(variant, u16::from(variant) * 0x40 + u16::from(object_type)))
            .collect(),
    )
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

fn display_tiles(payload: &[u8]) -> Vec<OscDisplayTile> {
    payload
        .split(|byte| *byte == b';')
        .filter_map(|item| {
            let mut values = item.split(|byte| *byte == b',');
            let x = decimal(values.next()?)?;
            let y = decimal(values.next()?)?;
            let tile = u16::try_from(hex(values.next()?)? & 0x7fff).ok()?;
            (x.unsigned_abs() <= 0x37ff && y.unsigned_abs() <= 0x37ff).then_some(OscDisplayTile {
                x,
                y,
                tile,
            })
        })
        .take(MAX_OSC_DISPLAY_TILES)
        .collect()
}

fn value_records(payload: &[u8]) -> Vec<[u16; 8]> {
    payload
        .split(|byte| *byte == b';')
        .filter_map(|item| {
            let mut words = item.split(|byte| *byte == b',').map(hex);
            Some([
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
                u16::try_from(words.next()??).ok()?,
            ])
        })
        .take(MAX_OSC_VALUE_RECORDS)
        .collect()
}

fn attributes(payload: &[u8]) -> Vec<u8> {
    payload
        .split(|byte| *byte == b',' || *byte == b';')
        .filter_map(|value| u8::try_from(hex(value)?).ok())
        .take(MAX_OSC_ATTRIBUTES)
        .collect()
}

fn decimal(input: &[u8]) -> Option<i16> {
    std::str::from_utf8(trim(input)).ok()?.parse().ok()
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
