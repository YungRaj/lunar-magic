//! Lossless Lunar Magic custom-sprite display metadata (`.ssc`).

mod parse;

use std::fmt;

pub const MAX_SSC_SOURCE_LEN: usize = 4 * 1024 * 1024;
pub const MAX_SSC_DISPLAY_TILES: usize = 0x200;
pub const MAX_SSC_PALETTE_RECORDS: usize = 0x40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SscSpriteSelector {
    pub sprite_number: u8,
    pub extra_bits: u8,
    pub index: u16,
    pub width: u8,
    pub height: u8,
    pub record_length: Option<u8>,
    pub alternate: bool,
    pub global_slot: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SscDisplayTile {
    pub x: i16,
    pub y: i16,
    pub tile: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SscRemapRange {
    pub first: u16,
    pub last: u16,
    pub target: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SscDirective {
    Description(String),
    Display(Vec<SscDisplayTile>),
    Palette(Vec<[u16; 4]>),
    TileRemap {
        mode: u8,
        ranges: Vec<SscRemapRange>,
    },
    PaletteRemap(Vec<SscRemapRange>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SscEntry {
    pub selector: Option<SscSpriteSelector>,
    pub flags: u32,
    pub directive: SscDirective,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SscSidecar {
    source: Vec<u8>,
    entries: Vec<SscEntry>,
}

impl SscSidecar {
    /// Parses valid records and retains every source byte for exact persistence.
    ///
    /// Malformed lines are skipped independently, matching the native loader.
    ///
    /// # Errors
    ///
    /// Rejects sources larger than [`MAX_SSC_SOURCE_LEN`].
    pub fn decode(source: &[u8]) -> Result<Self, SscSidecarError> {
        if source.len() > MAX_SSC_SOURCE_LEN {
            return Err(SscSidecarError::SourceTooLarge(source.len()));
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
    pub fn entries(&self) -> &[SscEntry] {
        &self.entries
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SscSidecarError {
    SourceTooLarge(usize),
}

impl fmt::Display for SscSidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid SSC sidecar: {self:?}")
    }
}

impl std::error::Error for SscSidecarError {}

#[cfg(test)]
mod tests;
