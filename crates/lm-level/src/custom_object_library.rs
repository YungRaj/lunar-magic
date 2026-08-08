use crate::ObjectRecord;
use std::fmt;

mod codec;

use codec::{
    decode_descriptions, decode_objects, encoded_data_len, encoded_description_len,
    validate_encoded_sizes,
};

pub const MAX_CUSTOM_OBJECT_SIDECAR_LEN: usize = 0x8000;
pub const CUSTOM_OBJECT_HEADER_LEN: usize = 5;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

/// Text framing retained from a Lunar Magic `.mw0t` custom-object description sidecar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptionFormat {
    pub utf8_bom: bool,
    pub line_ending: LineEnding,
    pub trailing_line_ending: bool,
}

impl Default for DescriptionFormat {
    fn default() -> Self {
        Self {
            utf8_bom: true,
            line_ending: LineEnding::CrLf,
            trailing_line_ending: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub(super) const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// One synchronized entry from Lunar Magic's `.mw0` and `.mw0t` sidecars.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomObjectEntry {
    pub object: ObjectRecord,
    /// Remaining records pasted with the primary object as one custom collection entry.
    pub additional_objects: Vec<ObjectRecord>,
    pub description: String,
}

impl CustomObjectEntry {
    /// Constructs an entry while enforcing the native text-sidecar boundary.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidDescription`] for embedded line separators,
    /// NULs, or descriptions longer than 1,024 encoded bytes.
    pub fn new(
        mut object: ObjectRecord,
        description: String,
    ) -> Result<Self, CustomObjectLibraryError> {
        validate_description(&description)?;
        object
            .set_raw_advances_screen(false)
            .map_err(|_| CustomObjectLibraryError::InvalidGroupBoundary)?;
        Ok(Self {
            object,
            additional_objects: Vec::new(),
            description,
        })
    }

    /// Constructs one native multi-object collection entry.
    ///
    /// # Errors
    ///
    /// Rejects an empty group or a description outside Lunar Magic's text boundary.
    pub fn new_group(
        mut objects: Vec<ObjectRecord>,
        description: String,
    ) -> Result<Self, CustomObjectLibraryError> {
        if objects.is_empty() {
            return Err(CustomObjectLibraryError::EmptyObjectGroup);
        }
        validate_description(&description)?;
        for object in &mut objects {
            object
                .set_raw_advances_screen(false)
                .map_err(|_| CustomObjectLibraryError::InvalidGroupBoundary)?;
        }
        let object = objects.remove(0);
        Ok(Self {
            object,
            additional_objects: objects,
            description,
        })
    }

    #[must_use]
    pub fn objects(&self) -> impl Iterator<Item = &ObjectRecord> {
        std::iter::once(&self.object).chain(&self.additional_objects)
    }
}

/// Lossless paired custom-object library used by `.mw0` and `.mw0t` files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomObjectLibrary {
    data_header: [u8; CUSTOM_OBJECT_HEADER_LEN],
    entries: Vec<CustomObjectEntry>,
    description_format: DescriptionFormat,
}

impl CustomObjectLibrary {
    /// Decodes a synchronized native sidecar pair.
    ///
    /// The binary sidecar has five retained reserved bytes followed by a terminated sequence of
    /// variable-width level-object records. A set new-screen bit on a record begins the next
    /// multi-object collection entry. The text sidecar is UTF-8 with an optional BOM and either
    /// consistent LF or CRLF separators. Text framing is retained for byte-stable re-encoding.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for oversized, malformed, non-UTF-8, mismatched, or
    /// non-canonical sidecars.
    pub fn decode(data: &[u8], descriptions: &[u8]) -> Result<Self, CustomObjectLibraryError> {
        if data.len() > MAX_CUSTOM_OBJECT_SIDECAR_LEN {
            return Err(CustomObjectLibraryError::DataTooLarge);
        }
        if descriptions.len() > MAX_CUSTOM_OBJECT_SIDECAR_LEN {
            return Err(CustomObjectLibraryError::DescriptionsTooLarge);
        }

        let (data_header, object_groups) = decode_objects(data)?;
        let (text, utf8_bom) = descriptions
            .strip_prefix(UTF8_BOM)
            .map_or((descriptions, false), |text| (text, true));
        let text = std::str::from_utf8(text)
            .map_err(|_| CustomObjectLibraryError::InvalidDescriptionEncoding)?;
        let (description_values, line_ending, trailing_line_ending) =
            decode_descriptions(text, object_groups.len())?;
        if object_groups.len() != description_values.len() {
            return Err(CustomObjectLibraryError::EntryCountMismatch {
                objects: object_groups.len(),
                descriptions: description_values.len(),
            });
        }

        let entries = object_groups
            .into_iter()
            .zip(description_values)
            .map(|(objects, description)| CustomObjectEntry::new_group(objects, description))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            data_header,
            entries,
            description_format: DescriptionFormat {
                utf8_bom,
                line_ending,
                trailing_line_ending,
            },
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[CustomObjectEntry] {
        &self.entries
    }

    /// Returns the five native reserved bytes retained from the `.mw0` file.
    #[must_use]
    pub const fn data_header(&self) -> &[u8; CUSTOM_OBJECT_HEADER_LEN] {
        &self.data_header
    }

    #[must_use]
    pub const fn description_format(&self) -> DescriptionFormat {
        self.description_format
    }

    /// Changes retained text framing after proving that the resulting sidecar remains bounded.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::DescriptionsTooLarge`] if the selected framing would
    /// exceed the native 32-KiB sidecar buffer.
    pub fn set_description_format(
        &mut self,
        format: DescriptionFormat,
    ) -> Result<(), CustomObjectLibraryError> {
        encoded_description_len(&self.entries, format)?;
        self.description_format = format;
        Ok(())
    }

    /// Appends a synchronized object and description atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] if either resulting sidecar exceeds its native bound.
    pub fn push(&mut self, entry: CustomObjectEntry) -> Result<(), CustomObjectLibraryError> {
        self.insert(self.entries.len(), entry)
    }

    /// Inserts a synchronized entry atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for an invalid index or either size limit.
    pub fn insert(
        &mut self,
        index: usize,
        entry: CustomObjectEntry,
    ) -> Result<(), CustomObjectLibraryError> {
        if index > self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        }
        validate_description(&entry.description)?;
        let mut staged = self.entries.clone();
        staged.insert(index, entry);
        validate_encoded_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(())
    }

    /// Replaces a synchronized entry atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for an invalid index or either size limit.
    pub fn replace(
        &mut self,
        index: usize,
        entry: CustomObjectEntry,
    ) -> Result<CustomObjectEntry, CustomObjectLibraryError> {
        let Some(previous) = self.entries.get(index).cloned() else {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        };
        validate_description(&entry.description)?;
        let mut staged = self.entries.clone();
        staged[index] = entry;
        validate_encoded_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(previous)
    }

    /// Removes an object and its paired description.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidIndex`] for an absent entry.
    pub fn remove(&mut self, index: usize) -> Result<CustomObjectEntry, CustomObjectLibraryError> {
        if index >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        }
        Ok(self.entries.remove(index))
    }

    /// Moves an object and its description together.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidIndex`] if either index is absent.
    pub fn move_entry(&mut self, from: usize, to: usize) -> Result<(), CustomObjectLibraryError> {
        if from >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(from));
        }
        if to >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(to));
        }
        if from != to {
            let entry = self.entries.remove(from);
            self.entries.insert(to, entry);
        }
        Ok(())
    }

    /// Finds descriptions using Unicode lowercase matching while retaining library order.
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

    /// Encodes both synchronized sidecars with their retained text framing.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] if programmatic state exceeds either native bound.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), CustomObjectLibraryError> {
        validate_encoded_sizes(&self.entries, self.description_format)?;
        let mut data = Vec::with_capacity(encoded_data_len(&self.entries)?);
        data.extend_from_slice(&self.data_header);
        for (entry_index, entry) in self.entries.iter().enumerate() {
            for (object_index, object) in entry.objects().enumerate() {
                let mut object = object.clone();
                object
                    .set_raw_advances_screen(entry_index != 0 && object_index == 0)
                    .map_err(|_| CustomObjectLibraryError::InvalidGroupBoundary)?;
                data.extend_from_slice(object.encoded());
            }
        }
        data.push(0xff);

        let mut descriptions = Vec::with_capacity(encoded_description_len(
            &self.entries,
            self.description_format,
        )?);
        if self.description_format.utf8_bom {
            descriptions.extend_from_slice(UTF8_BOM);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            descriptions.extend_from_slice(entry.description.as_bytes());
            if index + 1 < self.entries.len() || self.description_format.trailing_line_ending {
                descriptions.extend_from_slice(self.description_format.line_ending.bytes());
            }
        }
        Ok((data, descriptions))
    }
}

fn validate_description(description: &str) -> Result<(), CustomObjectLibraryError> {
    if description.len() > codec::MAX_DESCRIPTION_LEN || description.contains(['\0', '\r', '\n']) {
        Err(CustomObjectLibraryError::InvalidDescription)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomObjectLibraryError {
    DataTooLarge,
    DescriptionsTooLarge,
    MissingTerminator,
    TrailingObjectBytes(usize),
    MalformedObject { offset: usize },
    InvalidDescriptionEncoding,
    InvalidDescription,
    MissingHeader,
    EmptyObjectGroup,
    InvalidGroupBoundary,
    MixedLineEndings,
    EntryCountMismatch { objects: usize, descriptions: usize },
    InvalidIndex(usize),
}

impl fmt::Display for CustomObjectLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid custom-object sidecar library: {self:?}")
    }
}

impl std::error::Error for CustomObjectLibraryError {}

#[cfg(test)]
#[path = "custom_object_library_tests.rs"]
mod tests;
