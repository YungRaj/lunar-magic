//! Fixed-shape special overworld event reveal records.

use crate::EventReveal;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialEventRevealTable {
    pub reveals: [EventReveal; Self::ENTRY_COUNT],
    pub directions: [u8; Self::ENTRY_COUNT],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialEventRevealPlanes {
    pub sources: Vec<u8>,
    pub destinations: Vec<u8>,
    pub directions: Vec<u8>,
}

impl SpecialEventRevealTable {
    pub const ENTRY_COUNT: usize = 24;
    pub const WORD_PLANE_LEN: usize = Self::ENTRY_COUNT * 2;

    /// Decodes Lunar Magic's little-endian sources, big-endian destinations, and direction bytes.
    ///
    /// Invalid source tile numbers normalize to zero, matching the recovered native loader.
    ///
    /// # Errors
    ///
    /// Rejects any plane that does not have the exact native 24-entry shape.
    pub fn decode(
        sources: &[u8],
        destinations: &[u8],
        directions: &[u8],
    ) -> Result<Self, SpecialEventRevealError> {
        if sources.len() != Self::WORD_PLANE_LEN
            || destinations.len() != Self::WORD_PLANE_LEN
            || directions.len() != Self::ENTRY_COUNT
        {
            return Err(SpecialEventRevealError::Shape {
                sources: sources.len(),
                destinations: destinations.len(),
                directions: directions.len(),
            });
        }
        let reveals = std::array::from_fn(|index| {
            let word = index * 2;
            let source_tile = u16::from_le_bytes([sources[word], sources[word + 1]]);
            EventReveal {
                source_tile: if source_tile <= 0x07ff {
                    source_tile
                } else {
                    0
                },
                destination_tile: u16::from_be_bytes([destinations[word], destinations[word + 1]]),
            }
        });
        let directions = directions
            .try_into()
            .map_err(|_| SpecialEventRevealError::Shape {
                sources: sources.len(),
                destinations: destinations.len(),
                directions: directions.len(),
            })?;
        Ok(Self {
            reveals,
            directions,
        })
    }

    /// Encodes all three exact native planes.
    ///
    /// # Errors
    ///
    /// Rejects a source tile above `$07FF`, which the native loader would otherwise normalize and
    /// therefore fail to reopen semantically.
    pub fn encode(&self) -> Result<SpecialEventRevealPlanes, SpecialEventRevealError> {
        let mut sources = Vec::with_capacity(Self::WORD_PLANE_LEN);
        let mut destinations = Vec::with_capacity(Self::WORD_PLANE_LEN);
        for (index, reveal) in self.reveals.iter().enumerate() {
            if reveal.source_tile > 0x07ff {
                return Err(SpecialEventRevealError::InvalidSource {
                    index,
                    tile: reveal.source_tile,
                });
            }
            sources.extend_from_slice(&reveal.source_tile.to_le_bytes());
            destinations.extend_from_slice(&reveal.destination_tile.to_be_bytes());
        }
        Ok(SpecialEventRevealPlanes {
            sources,
            destinations,
            directions: self.directions.to_vec(),
        })
    }
}

impl Default for SpecialEventRevealTable {
    fn default() -> Self {
        Self {
            reveals: [EventReveal::default(); Self::ENTRY_COUNT],
            directions: [0; Self::ENTRY_COUNT],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialEventRevealError {
    Shape {
        sources: usize,
        destinations: usize,
        directions: usize,
    },
    InvalidSource {
        index: usize,
        tile: u16,
    },
}

impl std::fmt::Display for SpecialEventRevealError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid special overworld event-reveal table: {self:?}"
        )
    }
}

impl std::error::Error for SpecialEventRevealError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_native_planes_round_trip_with_mixed_endianness() {
        let mut table = SpecialEventRevealTable::default();
        table.reveals[0] = EventReveal {
            source_tile: 0x0123,
            destination_tile: 0x0456,
        };
        table.directions[0] = 0x87;
        let planes = table.encode().unwrap();
        assert_eq!(&planes.sources[..2], [0x23, 1]);
        assert_eq!(&planes.destinations[..2], [4, 0x56]);
        assert_eq!(planes.directions[0], 0x87);
        assert_eq!(
            SpecialEventRevealTable::decode(
                &planes.sources,
                &planes.destinations,
                &planes.directions
            )
            .unwrap(),
            table
        );
    }
}
