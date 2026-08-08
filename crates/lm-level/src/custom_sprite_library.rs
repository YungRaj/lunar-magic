mod format;

use format::{
    decode_descriptions, decode_placements, encode_descriptions, encoded_data_len,
    encoded_description_len, validate_entry, validate_sizes,
};

use crate::{DescriptionFormat, SpriteLengthTable, SpriteRecord};
use std::fmt;

pub const MAX_CUSTOM_SPRITE_SIDECAR_LEN: usize = 0x8000;

/// One synchronized custom placement from Lunar Magic's `.mw2` and `.mwt` sidecars.
///
/// A placement may contain several sprite records. Bit zero of the first record starts a new
/// placement; retaining every record byte also retains that native boundary marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSpriteEntry {
    pub sprites: Vec<SpriteRecord>,
    pub description: String,
}

impl CustomSpriteEntry {
    /// Constructs one non-empty placement while enforcing the native text boundary.
    ///
    /// # Errors
    ///
    /// Rejects empty/malformed records, internal placement markers, or invalid descriptions.
    pub fn new(
        sprites: Vec<SpriteRecord>,
        description: String,
    ) -> Result<Self, CustomSpriteLibraryError> {
        validate_entry(&sprites, &description)?;
        Ok(Self {
            sprites,
            description,
        })
    }
}

/// Lossless paired custom-sprite placement library used by `.mw2` and `.mwt` files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSpriteLibrary {
    header: u8,
    entries: Vec<CustomSpriteEntry>,
    description_format: DescriptionFormat,
}

impl CustomSpriteLibrary {
    /// Decodes one synchronized sidecar pair using the revision's four sprite-length tables.
    ///
    /// # Errors
    ///
    /// Rejects oversized, malformed, unterminated, trailing, or unsynchronized sidecars.
    pub fn decode(
        data: &[u8],
        descriptions: &[u8],
        lengths: &SpriteLengthTable,
    ) -> Result<Self, CustomSpriteLibraryError> {
        if data.len() > MAX_CUSTOM_SPRITE_SIDECAR_LEN {
            return Err(CustomSpriteLibraryError::DataTooLarge);
        }
        if descriptions.len() > MAX_CUSTOM_SPRITE_SIDECAR_LEN {
            return Err(CustomSpriteLibraryError::DescriptionsTooLarge);
        }
        let (header, placements) = decode_placements(data, lengths)?;
        let (description_values, description_format) =
            decode_descriptions(descriptions, placements.len())?;
        if placements.len() != description_values.len() {
            return Err(CustomSpriteLibraryError::EntryCountMismatch {
                placements: placements.len(),
                descriptions: description_values.len(),
            });
        }
        let entries = placements
            .into_iter()
            .zip(description_values)
            .map(|(sprites, description)| CustomSpriteEntry::new(sprites, description))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            header,
            entries,
            description_format,
        })
    }

    #[must_use]
    pub const fn header(&self) -> u8 {
        self.header
    }

    #[must_use]
    pub fn entries(&self) -> &[CustomSpriteEntry] {
        &self.entries
    }

    /// Returns the placements Lunar Magic publishes in its Add Sprites picker.
    ///
    /// `PopulateSpritePlacementDescriptionList` commits a row only when it consumes LF. The
    /// lossless editor retains an unterminated final description, but the original picker omits
    /// that one tail placement.
    #[must_use]
    pub fn lunar_magic_picker_entries(&self) -> &[CustomSpriteEntry] {
        let hidden_tail =
            usize::from(!self.description_format.trailing_line_ending && !self.entries.is_empty());
        &self.entries[..self.entries.len() - hidden_tail]
    }

    #[must_use]
    pub const fn description_format(&self) -> DescriptionFormat {
        self.description_format
    }

    pub fn set_header(&mut self, header: u8) {
        self.header = header;
    }

    /// Changes text framing after validating the resulting native buffer size.
    ///
    /// # Errors
    ///
    /// Rejects framing that would exceed the recovered 32-KiB limit.
    pub fn set_description_format(
        &mut self,
        format: DescriptionFormat,
    ) -> Result<(), CustomSpriteLibraryError> {
        encoded_description_len(&self.entries, format)?;
        self.description_format = format;
        Ok(())
    }

    /// Inserts a paired placement and description atomically.
    ///
    /// # Errors
    ///
    /// Rejects invalid indexes, entries, or resulting sidecar sizes.
    pub fn insert(
        &mut self,
        index: usize,
        entry: CustomSpriteEntry,
    ) -> Result<(), CustomSpriteLibraryError> {
        if index > self.entries.len() {
            return Err(CustomSpriteLibraryError::InvalidIndex(index));
        }
        validate_entry(&entry.sprites, &entry.description)?;
        let mut staged = self.entries.clone();
        staged.insert(index, entry);
        validate_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(())
    }

    /// Appends a paired placement after validating both resulting sidecars.
    ///
    /// # Errors
    ///
    /// Rejects invalid entries or resulting sidecar sizes.
    pub fn push(&mut self, entry: CustomSpriteEntry) -> Result<(), CustomSpriteLibraryError> {
        self.insert(self.entries.len(), entry)
    }

    /// Replaces one paired placement atomically and returns its previous value.
    ///
    /// # Errors
    ///
    /// Rejects an absent index, invalid entry, or resulting sidecar size.
    pub fn replace(
        &mut self,
        index: usize,
        entry: CustomSpriteEntry,
    ) -> Result<CustomSpriteEntry, CustomSpriteLibraryError> {
        let Some(previous) = self.entries.get(index).cloned() else {
            return Err(CustomSpriteLibraryError::InvalidIndex(index));
        };
        validate_entry(&entry.sprites, &entry.description)?;
        let mut staged = self.entries.clone();
        staged[index] = entry;
        validate_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(previous)
    }

    /// Removes and returns one paired placement.
    ///
    /// # Errors
    ///
    /// Rejects an absent index.
    pub fn remove(&mut self, index: usize) -> Result<CustomSpriteEntry, CustomSpriteLibraryError> {
        if index >= self.entries.len() {
            return Err(CustomSpriteLibraryError::InvalidIndex(index));
        }
        Ok(self.entries.remove(index))
    }

    /// Moves a placement and its synchronized description as one value.
    ///
    /// # Errors
    ///
    /// Rejects an absent index or moving an unmarked first entry behind another placement.
    pub fn move_entry(&mut self, from: usize, to: usize) -> Result<(), CustomSpriteLibraryError> {
        if from >= self.entries.len() {
            return Err(CustomSpriteLibraryError::InvalidIndex(from));
        }
        if to >= self.entries.len() {
            return Err(CustomSpriteLibraryError::InvalidIndex(to));
        }
        if from != to {
            let mut staged = self.entries.clone();
            let entry = staged.remove(from);
            staged.insert(to, entry);
            validate_sizes(&staged, self.description_format)?;
            self.entries = staged;
        }
        Ok(())
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .description
                    .to_lowercase()
                    .contains(&query)
                    .then_some(index)
            })
            .collect()
    }

    /// Encodes both sidecars byte-for-byte, including header, record boundary bits, and text
    /// framing. Every entry after the first must retain a set boundary bit on its first record;
    /// the native parser deliberately ignores that bit for the first entry.
    ///
    /// # Errors
    ///
    /// Rejects invalid placement boundaries or either sidecar exceeding its native limit.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), CustomSpriteLibraryError> {
        validate_sizes(&self.entries, self.description_format)?;
        let mut data = Vec::with_capacity(encoded_data_len(&self.entries)?);
        data.push(self.header);
        for entry in &self.entries {
            for sprite in &entry.sprites {
                data.extend_from_slice(&sprite.encoded);
            }
        }
        data.push(0xff);
        Ok((
            data,
            encode_descriptions(&self.entries, self.description_format)?,
        ))
    }

    /// Encodes after checking every record against the revision's selected length table.
    ///
    /// # Errors
    ///
    /// Rejects unknown or mismatched record widths in addition to [`Self::encode`] failures.
    pub fn encode_checked(
        &self,
        lengths: &SpriteLengthTable,
    ) -> Result<(Vec<u8>, Vec<u8>), CustomSpriteLibraryError> {
        for (entry, placement) in self.entries.iter().enumerate() {
            for (sprite, record) in placement.sprites.iter().enumerate() {
                let expected = lengths
                    .record_len(&record.encoded)
                    .ok_or(CustomSpriteLibraryError::UnknownSpriteLength { entry, sprite })?;
                if expected != record.encoded.len() {
                    return Err(CustomSpriteLibraryError::SpriteLengthMismatch {
                        entry,
                        sprite,
                        expected,
                        actual: record.encoded.len(),
                    });
                }
            }
        }
        self.encode()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomSpriteLibraryError {
    DataTooLarge,
    DescriptionsTooLarge,
    MissingHeader,
    MissingTerminator,
    TrailingData(usize),
    MalformedSprite {
        offset: usize,
    },
    UnknownSpriteLength {
        entry: usize,
        sprite: usize,
    },
    SpriteLengthMismatch {
        entry: usize,
        sprite: usize,
        expected: usize,
        actual: usize,
    },
    EmptyPlacement,
    MissingPlacementBoundary,
    UnexpectedPlacementBoundary,
    InvalidDescriptionEncoding,
    InvalidDescription,
    MixedLineEndings,
    EntryCountMismatch {
        placements: usize,
        descriptions: usize,
    },
    InvalidIndex(usize),
}

impl fmt::Display for CustomSpriteLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid custom-sprite sidecar library: {self:?}")
    }
}

impl std::error::Error for CustomSpriteLibraryError {}

#[cfg(test)]
#[path = "custom_sprite_library_tests.rs"]
mod tests;
