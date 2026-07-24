use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecentDocuments {
    paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecentDocumentsError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    NonZeroReserved,
    TooManyPaths(usize),
    PathTooLong { index: usize, bytes: usize },
    EmptyPath(usize),
    DuplicatePath(usize),
    InvalidUtf8(usize),
    NonUnicodePath(usize),
    TrailingBytes(usize),
    LengthOverflow,
}

impl fmt::Display for RecentDocumentsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid recent-document list: {self:?}")
    }
}

impl std::error::Error for RecentDocumentsError {}

impl RecentDocuments {
    pub const MAGIC: [u8; 8] = *b"LMRECNT1";
    pub const VERSION: u16 = 1;
    pub const MAX_PATHS: usize = 10;
    pub const MAX_PATH_BYTES: usize = 32 * 1024;
    pub const MAX_FILE_BYTES: usize = 12 + Self::MAX_PATHS * (4 + Self::MAX_PATH_BYTES);

    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Moves a nonempty path to the front and trims the oldest entry.
    ///
    /// Empty paths are ignored because they cannot identify a reopenable document. Equality is
    /// exact and platform-native; no filesystem canonicalization or I/O occurs.
    pub fn note(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return;
        }
        self.paths.retain(|existing| existing != &path);
        self.paths.insert(0, path);
        self.paths.truncate(Self::MAX_PATHS);
    }

    /// Encodes a portable UTF-8 recent-document list.
    ///
    /// # Errors
    ///
    /// Returns [`RecentDocumentsError`] for non-Unicode or overlong platform paths. No path is
    /// opened or canonicalized.
    pub fn encode(&self) -> Result<Vec<u8>, RecentDocumentsError> {
        validate_paths(&self.paths)?;
        let mut output = Vec::new();
        output.extend_from_slice(&Self::MAGIC);
        output.extend_from_slice(&Self::VERSION.to_le_bytes());
        output.extend_from_slice(
            &u16::try_from(self.paths.len())
                .map_err(|_| RecentDocumentsError::TooManyPaths(self.paths.len()))?
                .to_le_bytes(),
        );
        for (index, path) in self.paths.iter().enumerate() {
            let text = path
                .to_str()
                .ok_or(RecentDocumentsError::NonUnicodePath(index))?;
            output.extend_from_slice(
                &u32::try_from(text.len())
                    .map_err(|_| RecentDocumentsError::PathTooLong {
                        index,
                        bytes: text.len(),
                    })?
                    .to_le_bytes(),
            );
            output.extend_from_slice(text.as_bytes());
        }
        Ok(output)
    }

    /// Decodes an exact bounded UTF-8 recent-document list.
    ///
    /// # Errors
    ///
    /// Returns [`RecentDocumentsError`] for malformed framing, invalid paths, duplicates, limits,
    /// or trailing data.
    pub fn decode(bytes: &[u8]) -> Result<Self, RecentDocumentsError> {
        if bytes.len() > Self::MAX_FILE_BYTES {
            return Err(RecentDocumentsError::LengthOverflow);
        }
        let header = bytes.get(..12).ok_or(RecentDocumentsError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(RecentDocumentsError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(RecentDocumentsError::UnsupportedVersion(version));
        }
        let count = usize::from(u16::from_le_bytes([header[10], header[11]]));
        if count > Self::MAX_PATHS {
            return Err(RecentDocumentsError::TooManyPaths(count));
        }
        let mut offset = 12_usize;
        let mut paths = Vec::with_capacity(count);
        for index in 0..count {
            let length = read_length(bytes, &mut offset)?;
            if length > Self::MAX_PATH_BYTES {
                return Err(RecentDocumentsError::PathTooLong {
                    index,
                    bytes: length,
                });
            }
            let end = offset
                .checked_add(length)
                .ok_or(RecentDocumentsError::LengthOverflow)?;
            let encoded = bytes
                .get(offset..end)
                .ok_or(RecentDocumentsError::Truncated)?;
            let text = std::str::from_utf8(encoded)
                .map_err(|_| RecentDocumentsError::InvalidUtf8(index))?;
            paths.push(PathBuf::from(text));
            offset = end;
        }
        if offset != bytes.len() {
            return Err(RecentDocumentsError::TrailingBytes(bytes.len() - offset));
        }
        validate_paths(&paths)?;
        Ok(Self { paths })
    }
}

fn read_length(bytes: &[u8], offset: &mut usize) -> Result<usize, RecentDocumentsError> {
    let value = bytes
        .get(*offset..offset.saturating_add(4))
        .ok_or(RecentDocumentsError::Truncated)?;
    *offset = offset
        .checked_add(4)
        .ok_or(RecentDocumentsError::LengthOverflow)?;
    usize::try_from(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
        .map_err(|_| RecentDocumentsError::LengthOverflow)
}

fn validate_paths(paths: &[PathBuf]) -> Result<(), RecentDocumentsError> {
    if paths.len() > RecentDocuments::MAX_PATHS {
        return Err(RecentDocumentsError::TooManyPaths(paths.len()));
    }
    for (index, path) in paths.iter().enumerate() {
        if path.as_os_str().is_empty() {
            return Err(RecentDocumentsError::EmptyPath(index));
        }
        let text = path
            .to_str()
            .ok_or(RecentDocumentsError::NonUnicodePath(index))?;
        if text.len() > RecentDocuments::MAX_PATH_BYTES {
            return Err(RecentDocumentsError::PathTooLong {
                index,
                bytes: text.len(),
            });
        }
        if paths[..index].contains(path) {
            return Err(RecentDocumentsError::DuplicatePath(index));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn mru_deduplicates_trims_and_round_trips_unicode() {
        let mut recent = RecentDocuments::default();
        for index in 0..12 {
            recent.note(format!("ハック/{index}.smc"));
        }
        assert_eq!(recent.paths().len(), 10);
        assert_eq!(recent.paths()[0], Path::new("ハック/11.smc"));
        recent.note("ハック/5.smc");
        assert_eq!(recent.paths()[0], Path::new("ハック/5.smc"));
        assert_eq!(recent.paths().len(), 10);
        assert_eq!(
            RecentDocuments::decode(&recent.encode().unwrap()).unwrap(),
            recent
        );
    }

    #[test]
    fn every_truncation_trailing_duplicate_and_invalid_utf8_fail() {
        let mut recent = RecentDocuments::default();
        recent.note("one.smc");
        recent.note("two.smc");
        let bytes = recent.encode().unwrap();
        for end in 0..bytes.len() {
            assert!(RecentDocuments::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            RecentDocuments::decode(&trailing),
            Err(RecentDocumentsError::TrailingBytes(1))
        );
        let mut duplicate = bytes;
        let first_length = u32::from_le_bytes(duplicate[12..16].try_into().unwrap());
        let first_end = 16 + usize::try_from(first_length).unwrap();
        let first_path = duplicate[16..first_end].to_vec();
        duplicate[first_end + 4..].copy_from_slice(&first_path);
        assert!(matches!(
            RecentDocuments::decode(&duplicate),
            Err(RecentDocumentsError::DuplicatePath(1))
        ));
        let invalid = [
            RecentDocuments::MAGIC.as_slice(),
            &RecentDocuments::VERSION.to_le_bytes(),
            &1_u16.to_le_bytes(),
            &1_u32.to_le_bytes(),
            &[0xff],
        ]
        .concat();
        assert_eq!(
            RecentDocuments::decode(&invalid),
            Err(RecentDocumentsError::InvalidUtf8(0))
        );
    }
}
