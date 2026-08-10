//! Lunar Magic's Edit → Remap Direct Map16 Access grammar.

const TILE_COUNT: usize = 0x8000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectMap16RemapProgram {
    targets: Vec<Option<u16>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectMap16RemapError {
    Empty,
    MissingDestination(String),
    InvalidSource(String),
    InvalidDestination(String),
    DescendingRange { start: u16, end: u16 },
    InvalidRectangle { start: u16, end: u16 },
    TargetOutsideNamespace(i32),
}

impl std::fmt::Display for DirectMap16RemapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Direct Map16 remap: {self:?}")
    }
}

impl std::error::Error for DirectMap16RemapError {}

#[derive(Clone, Copy)]
enum Source {
    Linear { start: u16, end: u16 },
    Rectangle { start: u16, end: u16 },
}

#[derive(Clone, Copy)]
enum Destination {
    Fixed(u16),
    Offset(i32),
    Moving(u16),
}

impl DirectMap16RemapProgram {
    /// Parses comma/whitespace-separated source/destination pairs. Values are hexadecimal.
    /// Later pairs supersede earlier mappings of the same pre-remap source value.
    pub fn parse(script: &str) -> Result<Self, DirectMap16RemapError> {
        let tokens: Vec<_> = script
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|token| !token.is_empty())
            .collect();
        if tokens.is_empty() {
            return Err(DirectMap16RemapError::Empty);
        }
        let mut targets = vec![None; TILE_COUNT];
        for pair in tokens.chunks(2) {
            let source_text = pair[0];
            let destination_text = pair
                .get(1)
                .ok_or_else(|| DirectMap16RemapError::MissingDestination(source_text.into()))?;
            install_mapping(
                &mut targets,
                parse_source(source_text)?,
                parse_destination(destination_text)?,
            )?;
        }
        Ok(Self { targets })
    }

    #[must_use]
    pub fn remap(&self, source: u16) -> Option<u16> {
        self.targets.get(usize::from(source)).copied().flatten()
    }
}

fn parse_hex(text: &str) -> Option<u16> {
    u16::from_str_radix(text.strip_prefix('$').unwrap_or(text), 16)
        .ok()
        .filter(|value| *value < 0x8000)
}

fn parse_source(text: &str) -> Result<Source, DirectMap16RemapError> {
    let (rectangle, text) = text
        .strip_prefix('R')
        .or_else(|| text.strip_prefix('r'))
        .map_or((false, text), |rest| (true, rest));
    let (start, end) = match text.split_once('-') {
        Some((start, end)) => (
            parse_hex(start).ok_or_else(|| DirectMap16RemapError::InvalidSource(text.into()))?,
            parse_hex(end).ok_or_else(|| DirectMap16RemapError::InvalidSource(text.into()))?,
        ),
        None => {
            let value =
                parse_hex(text).ok_or_else(|| DirectMap16RemapError::InvalidSource(text.into()))?;
            (value, value)
        }
    };
    if end < start {
        return Err(DirectMap16RemapError::DescendingRange { start, end });
    }
    if rectangle {
        if (end & 0x0f) < (start & 0x0f) {
            return Err(DirectMap16RemapError::InvalidRectangle { start, end });
        }
        Ok(Source::Rectangle { start, end })
    } else {
        Ok(Source::Linear { start, end })
    }
}

fn parse_destination(text: &str) -> Result<Destination, DirectMap16RemapError> {
    if let Some(rest) = text.strip_prefix('M').or_else(|| text.strip_prefix('m')) {
        return parse_hex(rest)
            .map(Destination::Moving)
            .ok_or_else(|| DirectMap16RemapError::InvalidDestination(text.into()));
    }
    if let Some(rest) = text.strip_prefix('+') {
        return parse_hex(rest)
            .map(|value| Destination::Offset(i32::from(value)))
            .ok_or_else(|| DirectMap16RemapError::InvalidDestination(text.into()));
    }
    if let Some(rest) = text.strip_prefix('-') {
        return parse_hex(rest)
            .map(|value| Destination::Offset(-i32::from(value)))
            .ok_or_else(|| DirectMap16RemapError::InvalidDestination(text.into()));
    }
    parse_hex(text)
        .map(Destination::Fixed)
        .ok_or_else(|| DirectMap16RemapError::InvalidDestination(text.into()))
}

fn checked_target(target: i32) -> Result<u16, DirectMap16RemapError> {
    u16::try_from(target)
        .ok()
        .filter(|value| *value < 0x8000)
        .ok_or(DirectMap16RemapError::TargetOutsideNamespace(target))
}

fn install_mapping(
    targets: &mut [Option<u16>],
    source: Source,
    destination: Destination,
) -> Result<(), DirectMap16RemapError> {
    let (start, end, rectangle) = match source {
        Source::Linear { start, end } => (start, end, false),
        Source::Rectangle { start, end } => (start, end, true),
    };
    let width = i32::from((end & 0x0f) - (start & 0x0f) + 1);
    let sources: Vec<u16> = if rectangle {
        (start >> 4..=end >> 4)
            .flat_map(|row| (start & 0x0f..=end & 0x0f).map(move |column| (row << 4) | column))
            .collect()
    } else {
        (start..=end).collect()
    };
    for (ordinal, source) in sources.into_iter().enumerate() {
        let target = match destination {
            Destination::Fixed(value) => i32::from(value),
            Destination::Offset(delta) => i32::from(source) + delta,
            Destination::Moving(value) if rectangle => {
                let ordinal = i32::try_from(ordinal).unwrap_or(i32::MAX);
                i32::from(value) + (ordinal / width) * 16 + ordinal % width
            }
            Destination::Moving(value) => i32::from(value) + i32::from(source) - i32::from(start),
        };
        targets[usize::from(source)] = Some(checked_target(target)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_examples_and_prestate_duplicate_precedence_are_exact() {
        let program =
            DirectMap16RemapProgram::parse("100,25 110-111,+25 120-121,-25 130-131,M125 100,30")
                .unwrap();
        assert_eq!(program.remap(0x100), Some(0x30));
        assert_eq!(program.remap(0x110), Some(0x135));
        assert_eq!(program.remap(0x111), Some(0x136));
        assert_eq!(program.remap(0x120), Some(0xfb));
        assert_eq!(program.remap(0x121), Some(0xfc));
        assert_eq!(program.remap(0x130), Some(0x125));
        assert_eq!(program.remap(0x131), Some(0x126));
        assert_eq!(program.remap(0x30), None);
    }

    #[test]
    fn rectangle_preserves_two_dimensional_offsets() {
        let program = DirectMap16RemapProgram::parse("R100-111,M25").unwrap();
        assert_eq!(program.remap(0x100), Some(0x25));
        assert_eq!(program.remap(0x101), Some(0x26));
        assert_eq!(program.remap(0x110), Some(0x35));
        assert_eq!(program.remap(0x111), Some(0x36));
        assert_eq!(program.remap(0x102), None);
    }

    #[test]
    fn malformed_and_out_of_namespace_programs_are_atomic_errors() {
        assert!(matches!(
            DirectMap16RemapProgram::parse("100"),
            Err(DirectMap16RemapError::MissingDestination(_))
        ));
        assert!(matches!(
            DirectMap16RemapProgram::parse("100,-101"),
            Err(DirectMap16RemapError::TargetOutsideNamespace(-1))
        ));
        assert!(matches!(
            DirectMap16RemapProgram::parse("R10F-110,M20"),
            Err(DirectMap16RemapError::InvalidRectangle { .. })
        ));
    }
}
