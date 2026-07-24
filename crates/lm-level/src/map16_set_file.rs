use crate::{Map16Set, Map16SetError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16SetFile {
    pub set: Map16Set,
}

#[derive(Debug)]
pub enum Map16SetFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    TooManyPages(usize),
    WrongPageSize { page: usize, tiles: usize },
    WrongLength { expected: usize, actual: usize },
    Decode(Map16SetError),
    Overflow,
}

impl std::fmt::Display for Map16SetFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Map16 set file error: {self:?}")
    }
}

impl std::error::Error for Map16SetFileError {}

impl From<Map16SetError> for Map16SetFileError {
    fn from(value: Map16SetError) -> Self {
        Self::Decode(value)
    }
}

impl Map16SetFile {
    pub const MAGIC: [u8; 8] = *b"LM16SET1";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN
        + Map16Set::MAX_PAGES * (Map16Set::GRAPHICS_PAGE_LEN + Map16Set::ACTS_LIKE_PAGE_LEN);

    /// Encodes all graphics pages followed by all Acts Like pages.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetFileError`] for excessive page counts, malformed pages, or overflow.
    pub fn encode(&self) -> Result<Vec<u8>, Map16SetFileError> {
        validate_shape(&self.set)?;
        let page_count = u16::try_from(self.set.pages.len())
            .map_err(|_| Map16SetFileError::TooManyPages(self.set.pages.len()))?;
        let expected = encoded_len(self.set.pages.len())?;
        let (graphics, acts_like) = self.set.encode()?;
        let mut output = Vec::with_capacity(expected);
        output.extend_from_slice(&Self::MAGIC);
        output.extend_from_slice(&Self::VERSION.to_le_bytes());
        output.extend_from_slice(&page_count.to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&graphics);
        output.extend_from_slice(&acts_like);
        Ok(output)
    }

    /// Decodes exactly one complete bounded Map16-set file.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetFileError`] for framing, page limits, length, or plane decoding errors.
    pub fn decode(bytes: &[u8]) -> Result<Self, Map16SetFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(Map16SetFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(Map16SetFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(Map16SetFileError::UnsupportedVersion(version));
        }
        if header[12..16] != [0; 4] {
            return Err(Map16SetFileError::ReservedBytes);
        }
        let page_count = usize::from(u16::from_le_bytes([header[10], header[11]]));
        if page_count > Map16Set::MAX_PAGES {
            return Err(Map16SetFileError::TooManyPages(page_count));
        }
        let expected = encoded_len(page_count)?;
        if bytes.len() != expected {
            return Err(Map16SetFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let graphics_len = page_count
            .checked_mul(Map16Set::GRAPHICS_PAGE_LEN)
            .ok_or(Map16SetFileError::Overflow)?;
        let graphics_end = Self::HEADER_LEN
            .checked_add(graphics_len)
            .ok_or(Map16SetFileError::Overflow)?;
        Ok(Self {
            set: Map16Set::decode(
                &bytes[Self::HEADER_LEN..graphics_end],
                &bytes[graphics_end..],
            )?,
        })
    }
}

fn validate_shape(set: &Map16Set) -> Result<(), Map16SetFileError> {
    match set.validate_shape() {
        Ok(()) => Ok(()),
        Err(Map16SetError::TooManyPages(count)) => Err(Map16SetFileError::TooManyPages(count)),
        Err(Map16SetError::WrongPageSize { page, tiles }) => {
            Err(Map16SetFileError::WrongPageSize { page, tiles })
        }
        Err(error) => Err(Map16SetFileError::Decode(error)),
    }
}

fn encoded_len(pages: usize) -> Result<usize, Map16SetFileError> {
    let per_page = Map16Set::GRAPHICS_PAGE_LEN
        .checked_add(Map16Set::ACTS_LIKE_PAGE_LEN)
        .ok_or(Map16SetFileError::Overflow)?;
    Map16SetFile::HEADER_LEN
        .checked_add(
            pages
                .checked_mul(per_page)
                .ok_or(Map16SetFileError::Overflow)?,
        )
        .ok_or(Map16SetFileError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Map16Page, Map16Tile, Subtile};

    fn file() -> Map16SetFile {
        let mut first = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        let mut second = first.clone();
        for (index, tile) in first.iter_mut().chain(&mut second).enumerate() {
            tile.acts_like = u16::try_from(index).unwrap();
        }
        second[3].top_left = Subtile(0xe321);
        Map16SetFile {
            set: Map16Set {
                pages: vec![
                    Map16Page::new(first).unwrap(),
                    Map16Page::new(second).unwrap(),
                ],
            },
        }
    }

    #[test]
    fn complete_planes_round_trip_deterministically() {
        let expected = file();
        let bytes = expected.encode().unwrap();
        assert_eq!(Map16SetFile::decode(&bytes).unwrap(), expected);
        assert_eq!(
            Map16SetFile::decode(&bytes).unwrap().encode().unwrap(),
            bytes
        );
    }

    #[test]
    fn every_truncation_trailing_and_reserved_data_are_rejected() {
        let bytes = file().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(Map16SetFile::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            Map16SetFile::decode(&trailing),
            Err(Map16SetFileError::WrongLength { .. })
        ));
        let mut reserved = bytes;
        reserved[12] = 1;
        assert!(matches!(
            Map16SetFile::decode(&reserved),
            Err(Map16SetFileError::ReservedBytes)
        ));
    }

    #[test]
    fn malformed_public_page_shape_cannot_encode() {
        let malformed = Map16SetFile {
            set: Map16Set {
                pages: vec![Map16Page { tiles: Vec::new() }],
            },
        };
        assert!(matches!(
            malformed.encode(),
            Err(Map16SetFileError::WrongPageSize { page: 0, tiles: 0 })
        ));
    }
}
