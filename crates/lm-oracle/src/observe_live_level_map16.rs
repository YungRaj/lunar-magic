use crate::{Observation, ObservationError, sha256_hex};
use std::fmt;

/// Number of 16-bit cells in Lunar Magic's live rendered-level Map16 cache.
pub const LIVE_LEVEL_MAP16_CELLS: usize = 0x3800;
pub const LIVE_LEVEL_MAP16_BYTES: usize = LIVE_LEVEL_MAP16_CELLS * 2;
const CHUNK_BYTES: usize = 0x100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveLevelMap16ObservationError {
    Length { actual: usize, expected: usize },
    Observation,
}

impl fmt::Display for LiveLevelMap16ObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid live rendered-level Map16 cache: {self:?}"
        )
    }
}

impl std::error::Error for LiveLevelMap16ObservationError {}

impl From<ObservationError> for LiveLevelMap16ObservationError {
    fn from(_: ObservationError) -> Self {
        Self::Observation
    }
}

/// Produces a deterministic, independently localized observation of Lunar Magic's live cache.
///
/// The input is the byte-exact debugger dump beginning at `g_awLevelMap16Tiles`. Chunks make a
/// renderer mismatch localizable without embedding the copyrighted source ROM or editor binary.
///
/// # Errors
///
/// Rejects any dump that is not exactly 0x3800 little-endian words.
pub fn observe_live_level_map16(
    bytes: &[u8],
) -> Result<Observation, LiveLevelMap16ObservationError> {
    if bytes.len() != LIVE_LEVEL_MAP16_BYTES {
        return Err(LiveLevelMap16ObservationError::Length {
            actual: bytes.len(),
            expected: LIVE_LEVEL_MAP16_BYTES,
        });
    }
    let mut observation = Observation::new();
    observation.insert("live-level-map16/sha256", sha256_hex(bytes))?;
    observation.insert("live-level-map16/cells", LIVE_LEVEL_MAP16_CELLS.to_string())?;
    for (index, chunk) in bytes.chunks_exact(CHUNK_BYTES).enumerate() {
        observation.insert(
            format!("live-level-map16/chunks/{index:03x}/sha256"),
            sha256_hex(chunk),
        )?;
    }
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_cache_is_hashed_in_localizable_chunks() {
        let mut bytes = vec![0x25; LIVE_LEVEL_MAP16_BYTES];
        let baseline = observe_live_level_map16(&bytes).unwrap();
        assert_eq!(baseline.get("live-level-map16/cells"), Some("14336"));
        bytes[CHUNK_BYTES + 3] ^= 1;
        let changed = observe_live_level_map16(&bytes).unwrap();
        let differences = baseline.differences(&changed);
        assert!(
            differences
                .iter()
                .any(|difference| difference.path == "live-level-map16/chunks/001/sha256")
        );
        assert!(
            !differences
                .iter()
                .any(|difference| difference.path == "live-level-map16/chunks/000/sha256")
        );
    }

    #[test]
    fn partial_debugger_dumps_are_rejected() {
        assert_eq!(
            observe_live_level_map16(&[0; 2]),
            Err(LiveLevelMap16ObservationError::Length {
                actual: 2,
                expected: LIVE_LEVEL_MAP16_BYTES,
            })
        );
    }
}
