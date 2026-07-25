//! Lossless Lunar Magic custom-object metadata (`.osc`).

mod parse;

use std::fmt;

pub const MAX_OSC_SOURCE_LEN: usize = 4 * 1024 * 1024;
pub const MAX_OSC_DISPLAY_TILES: usize = 0x3800;
pub const MAX_OSC_VALUE_RECORDS: usize = 0x40;
pub const MAX_OSC_ATTRIBUTES: usize = 0x0f;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OscObjectSelector {
    pub object_type: u8,
    pub parameter: u8,
    pub variant: u8,
    pub index: u16,
    pub width: u8,
    pub height: u8,
    pub record_length: Option<u8>,
    pub alternate_linear: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OscDisplayTile {
    pub x: i16,
    pub y: i16,
    pub tile: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OscDirective {
    Description(String),
    Display(Vec<OscDisplayTile>),
    Values(Vec<[u16; 8]>),
    Attributes(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscEntry {
    pub selectors: Vec<OscObjectSelector>,
    pub flags: u32,
    pub directive: OscDirective,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscSidecar {
    source: Vec<u8>,
    entries: Vec<OscEntry>,
}

impl OscSidecar {
    /// Parses valid records and retains every source byte for exact persistence.
    ///
    /// Malformed lines are retained in [`Self::source`] but omitted from [`Self::entries`].
    ///
    /// # Errors
    ///
    /// Rejects sources larger than [`MAX_OSC_SOURCE_LEN`].
    pub fn decode(source: &[u8]) -> Result<Self, OscSidecarError> {
        if source.len() > MAX_OSC_SOURCE_LEN {
            return Err(OscSidecarError::SourceTooLarge(source.len()));
        }
        let body = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
        let entries = body
            .split(|byte| *byte == b'\n')
            .filter_map(parse::line)
            .collect();
        Ok(Self {
            source: source.to_vec(),
            entries,
        })
    }

    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    #[must_use]
    pub fn encode_lossless(&self) -> Vec<u8> {
        self.source.clone()
    }

    #[must_use]
    pub fn entries(&self) -> &[OscEntry] {
        &self.entries
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OscSidecarError {
    SourceTooLarge(usize),
}

impl fmt::Display for OscSidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid OSC sidecar: {self:?}")
    }
}

impl std::error::Error for OscSidecarError {}

#[cfg(test)]
mod tests;
