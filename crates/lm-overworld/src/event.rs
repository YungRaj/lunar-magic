#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventId(pub u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventTileChange {
    pub event: EventId,
    pub x: u16,
    pub y: u16,
    pub before: u16,
    pub after: u16,
    pub raw_flags: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventReveal {
    pub source_tile: u16,
    pub destination_tile: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventRevealTable {
    pub entries: Vec<EventReveal>,
}

impl EventRevealTable {
    pub const MAX_ENTRIES: usize = 0xff;
    pub const MAX_TILE: u16 = 0x07ff;

    /// Decodes the parallel reveal tables. Sources are little-endian; destinations use the
    /// big-endian order written by Lunar Magic. Invalid source indexes are normalized to zero,
    /// matching the recovered loader.
    ///
    /// # Errors
    ///
    /// Returns [`EventTableError`] for unequal, odd, or oversized planes.
    pub fn decode(sources: &[u8], destinations: &[u8]) -> Result<Self, EventTableError> {
        if sources.len() != destinations.len() || sources.len() % 2 != 0 {
            return Err(EventTableError::PlaneSize {
                sources: sources.len(),
                destinations: destinations.len(),
            });
        }
        let count = sources.len() / 2;
        if count > Self::MAX_ENTRIES {
            return Err(EventTableError::TooManyEntries(count));
        }
        let entries = sources
            .chunks_exact(2)
            .zip(destinations.chunks_exact(2))
            .map(|(source, destination)| {
                let source_tile = u16::from_le_bytes([source[0], source[1]]);
                EventReveal {
                    source_tile: if source_tile <= Self::MAX_TILE {
                        source_tile
                    } else {
                        0
                    },
                    destination_tile: u16::from_be_bytes([destination[0], destination[1]]),
                }
            })
            .collect();
        Ok(Self { entries })
    }

    /// Validates a publicly constructed reveal table before persistence.
    ///
    /// Lunar Magic's native loader normalizes invalid source tiles to zero. Rejecting them before
    /// encoding prevents an apparently successful save from changing meaning when reopened.
    ///
    /// # Errors
    ///
    /// Returns [`EventTableError`] when the table exceeds the native count or contains an invalid
    /// source tile.
    pub fn validate(&self) -> Result<(), EventTableError> {
        if self.entries.len() > Self::MAX_ENTRIES {
            return Err(EventTableError::TooManyEntries(self.entries.len()));
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.source_tile > Self::MAX_TILE {
                return Err(EventTableError::InvalidSourceTile {
                    index,
                    tile: entry.source_tile,
                });
            }
        }
        Ok(())
    }

    /// Validates and encodes both native reveal planes.
    ///
    /// # Errors
    ///
    /// Returns [`EventTableError`] when the public model cannot round-trip through the native
    /// decoder without normalization.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), EventTableError> {
        self.validate()?;
        let mut sources = Vec::with_capacity(self.entries.len() * 2);
        let mut destinations = Vec::with_capacity(self.entries.len() * 2);
        for entry in &self.entries {
            sources.extend_from_slice(&entry.source_tile.to_le_bytes());
            destinations.extend_from_slice(&entry.destination_tile.to_be_bytes());
        }
        Ok((sources, destinations))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventTableError {
    PlaneSize { sources: usize, destinations: usize },
    TooManyEntries(usize),
    InvalidSourceTile { index: usize, tile: u16 },
}

impl std::fmt::Display for EventTableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld event table: {self:?}")
    }
}

impl std::error::Error for EventTableError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventNumberMap {
    mapping: [u8; Self::ENTRY_COUNT],
    stored_len: usize,
}

impl EventNumberMap {
    pub const ENTRY_COUNT: usize = 256;
    pub const VANILLA_LEN: usize = 0x60;

    /// Loads a fixed or allocated mapping and zero-fills absent entries.
    ///
    /// # Errors
    ///
    /// Returns the supplied length above 256 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() > Self::ENTRY_COUNT {
            return Err(bytes.len());
        }
        let mut mapping = [0; Self::ENTRY_COUNT];
        mapping[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            mapping,
            stored_len: bytes.len(),
        })
    }

    /// Converts the recovered legacy source/destination pair list.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless it consists of complete pairs.
    pub fn decode_legacy_pairs(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() % 2 != 0 {
            return Err(bytes.len());
        }
        let mut mapping = [0; Self::ENTRY_COUNT];
        let mut stored_len = Self::VANILLA_LEN;
        for pair in bytes.chunks_exact(2) {
            mapping[usize::from(pair[0])] = pair[1];
            stored_len = stored_len.max(usize::from(pair[0]) + 1);
        }
        Ok(Self {
            mapping,
            stored_len,
        })
    }

    #[must_use]
    pub const fn get(&self, event: u8) -> u8 {
        self.mapping[event as usize]
    }

    pub fn set(&mut self, event: u8, mapped: u8) {
        self.mapping[usize::from(event)] = mapped;
        self.stored_len = self.stored_len.max(usize::from(event) + 1);
    }

    #[must_use]
    pub fn encode(&self) -> &[u8] {
        &self.mapping[..self.stored_len]
    }

    #[must_use]
    pub const fn stored_len(&self) -> usize {
        self.stored_len
    }

    #[must_use]
    pub const fn uses_extended_events(&self) -> bool {
        self.stored_len > Self::VANILLA_LEN
    }
}

impl Default for EventNumberMap {
    fn default() -> Self {
        Self {
            mapping: [0; Self::ENTRY_COUNT],
            stored_len: Self::VANILLA_LEN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_planes_round_trip_endianness() {
        let table = EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 0x123,
                    destination_tile: 0x456,
                },
                EventReveal {
                    source_tile: 0x7ff,
                    destination_tile: 0x789,
                },
            ],
        };
        let (sources, destinations) = table.encode().unwrap();
        assert_eq!(sources, [0x23, 1, 0xff, 7]);
        assert_eq!(destinations, [4, 0x56, 7, 0x89]);
        assert_eq!(
            EventRevealTable::decode(&sources, &destinations).unwrap(),
            table
        );
    }

    #[test]
    fn invalid_source_is_normalized_like_lunar_magic() {
        let table = EventRevealTable::decode(&[0x00, 0x08], &[0, 1]).unwrap();
        assert_eq!(table.entries[0].source_tile, 0);
    }

    #[test]
    fn encoding_rejects_public_model_that_would_normalize_on_reopen() {
        let table = EventRevealTable {
            entries: vec![EventReveal {
                source_tile: EventRevealTable::MAX_TILE + 1,
                destination_tile: 1,
            }],
        };
        assert_eq!(
            table.encode(),
            Err(EventTableError::InvalidSourceTile {
                index: 0,
                tile: 0x800,
            })
        );
    }

    #[test]
    fn legacy_event_pairs_convert_to_current_map() {
        let mut map = EventNumberMap::decode_legacy_pairs(&[1, 9, 2, 8]).unwrap();
        assert_eq!(map.get(1), 9);
        map.set(0x80, 7);
        assert!(map.uses_extended_events());
        assert_eq!(map.encode()[0x80], 7);
    }

    #[test]
    fn every_legacy_source_survives_conversion_at_the_required_extent() {
        for source in 0_u8..=u8::MAX {
            let value = source.wrapping_mul(37).wrapping_add(11);
            let map = EventNumberMap::decode_legacy_pairs(&[source, value]).unwrap();
            let expected_len = EventNumberMap::VANILLA_LEN.max(usize::from(source) + 1);
            assert_eq!(map.stored_len(), expected_len);
            assert_eq!(map.encode().len(), expected_len);
            assert_eq!(map.encode()[usize::from(source)], value);
            assert_eq!(map.uses_extended_events(), source >= 0x60);
            assert_eq!(EventNumberMap::decode(map.encode()).unwrap(), map);
        }
    }
}
