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

    /// Moves selected ordinary event-tile records by one shared, seam-aware tile displacement.
    ///
    /// Lunar Magic stores each destination as twice an internal main-overworld tile index rather
    /// than as a row-major coordinate. A normal source expands to a 6x6 event-tile footprint. If
    /// the requested displacement would put any selected footprint outside the main map, the
    /// native editor searches X toward zero, then Y toward zero, until the whole selection fits.
    /// This method reproduces that transaction and never partially moves a selection.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or out-of-range selection indexes and records whose stored destination
    /// cannot be decoded as a main-overworld event-tile anchor.
    pub fn relocate_selection(
        &mut self,
        selection: &[usize],
        requested_x: i16,
        requested_y: i16,
    ) -> Result<Option<(i16, i16)>, EventRevealMoveError> {
        if selection.is_empty() || (requested_x == 0 && requested_y == 0) {
            return Ok(None);
        }
        let mut unique = selection.to_vec();
        unique.sort_unstable();
        if let Some(pair) = unique.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(EventRevealMoveError::DuplicateIndex(pair[0]));
        }
        let mut anchors = Vec::with_capacity(unique.len());
        for &index in &unique {
            let reveal = self
                .entries
                .get(index)
                .ok_or(EventRevealMoveError::IndexOutOfBounds {
                    index,
                    len: self.entries.len(),
                })?;
            let packed = reveal.destination_tile >> 1;
            let (x, y) = decode_main_overworld_event_tile_index(packed).ok_or(
                EventRevealMoveError::InvalidDestination {
                    index,
                    destination: reveal.destination_tile,
                },
            )?;
            anchors.push((index, x, y));
        }

        // A displacement beyond the complete map cannot reveal a distinct candidate. Bounding it
        // here is equivalent to the native endpoint constraint and keeps public callers bounded.
        let requested_x = requested_x.clamp(-63, 63);
        let requested_y = requested_y.clamp(-127, 127);
        let step_x = if requested_x < 0 { 1 } else { -1 };
        let step_y = if requested_y < 0 { 1 } else { -1 };
        let mut y = requested_y;
        while y != step_y {
            let mut x = requested_x;
            while x != step_x {
                let destinations = anchors
                    .iter()
                    .map(|&(index, anchor_x, anchor_y)| {
                        let moved_x = i32::from(anchor_x) + i32::from(x);
                        let moved_y = i32::from(anchor_y) + i32::from(y);
                        let moved_x = u8::try_from(moved_x).ok()?;
                        let moved_y = u8::try_from(moved_y).ok()?;
                        let packed = encode_main_overworld_event_tile_index(moved_x, moved_y)?;
                        ordinary_event_footprint_fits(packed).then_some((index, packed << 1))
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(destinations) = destinations {
                    if x == 0 && y == 0 {
                        return Ok(None);
                    }
                    let mut staged = self.clone();
                    for (index, destination_tile) in destinations {
                        staged.entries[index].destination_tile = destination_tile;
                    }
                    *self = staged;
                    return Ok(Some((x, y)));
                }
                x += step_x;
            }
            y += step_y;
        }
        Ok(None)
    }
}

/// Converts Lunar Magic's ordinary main-overworld event coordinate to its internal tile index.
///
/// The main map is 64 tiles wide and 128 tiles high. Its two 32-row planes and the seam around row
/// 64 are stored in the exact order recovered from `ConvertOverworldTileCoordinatesToIndex`.
#[must_use]
pub fn encode_main_overworld_event_tile_index(x: u8, y: u8) -> Option<u16> {
    if x >= 64 || y >= 128 {
        return None;
    }
    let mut stored_y = u16::from(y);
    if stored_y == 0x40 {
        stored_y = 0x7f;
    } else if stored_y > 0x40 {
        stored_y -= 1;
    }
    let mut stored_x = u16::from(x);
    if stored_y >= 0x40 {
        stored_x = if stored_x < 2 {
            stored_x + 0x3e
        } else {
            stored_x - 2
        };
    }
    if stored_x > 0x1f {
        stored_x += 0x3e0;
    }
    let index = (((stored_y & !0x1f) + stored_y) * 0x20).checked_add(stored_x)?;
    (index < 0x2000).then_some(index)
}

/// Reverses [`encode_main_overworld_event_tile_index`].
#[must_use]
pub fn decode_main_overworld_event_tile_index(index: u16) -> Option<(u8, u8)> {
    if index >= 0x2000 {
        return None;
    }
    let mut low = index & 0x07ff;
    let right_plane = low > 0x03ff;
    if right_plane {
        low -= 0x0400;
    }
    let mut y = (index >> 11) * 0x20 + (low >> 5);
    let mut x = u16::from(right_plane) * 0x20 + (low & 0x1f);
    if y == 0x7f {
        y = 0x40;
    } else if y >= 0x40 {
        y += 1;
    }
    if y >= 0x40 {
        x = if x < 0x3e { x + 2 } else { x - 0x3e };
    }
    Some((u8::try_from(x).ok()?, u8::try_from(y).ok()?))
}

fn ordinary_event_footprint_fits(index: u16) -> bool {
    let mut row = u32::from(index) * 2;
    for _ in 0..6 {
        let mut destination = row;
        for _ in 0..6 {
            if destination >= 0x4000 {
                return false;
            }
            let next = destination + 2;
            destination = if next & 0x3f == 0 {
                ((destination + 1) & 0xffc0) + 0x800
            } else {
                next
            };
        }
        let next = row + 0x40;
        row = if next & 0x7c0 == 0 {
            (row & 0xf83f) + 0x1000
        } else {
            next
        };
    }
    true
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventRevealMoveError {
    DuplicateIndex(usize),
    IndexOutOfBounds { index: usize, len: usize },
    InvalidDestination { index: usize, destination: u16 },
}

impl std::fmt::Display for EventRevealMoveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld event-tile move: {self:?}")
    }
}

impl std::error::Error for EventRevealMoveError {}

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
    fn recovered_main_overworld_event_coordinates_are_bijective() {
        let mut indexes = std::collections::BTreeSet::new();
        for y in 0..128 {
            for x in 0..64 {
                let index = encode_main_overworld_event_tile_index(x, y).unwrap();
                assert!(indexes.insert(index), "duplicate {index:04x} for {x},{y}");
                assert_eq!(decode_main_overworld_event_tile_index(index), Some((x, y)));
            }
        }
        assert_eq!(indexes.len(), 0x2000);
        assert_eq!(indexes.first(), Some(&0));
        assert_eq!(indexes.last(), Some(&0x1fff));
        assert_eq!(encode_main_overworld_event_tile_index(64, 0), None);
        assert_eq!(encode_main_overworld_event_tile_index(0, 128), None);
        assert_eq!(decode_main_overworld_event_tile_index(0x2000), None);
    }

    #[test]
    fn selection_move_uses_one_constrained_native_displacement_atomically() {
        let destination = |x, y| encode_main_overworld_event_tile_index(x, y).unwrap() * 2;
        let mut table = EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 1,
                    destination_tile: destination(10, 10),
                },
                EventReveal {
                    source_tile: 2,
                    destination_tile: destination(58, 120),
                },
                EventReveal {
                    source_tile: 3,
                    destination_tile: destination(20, 20),
                },
            ],
        };
        let untouched = table.entries[2];
        // The second footprint crosses Lunar Magic's packed plane boundary for larger deltas. The
        // native X-first/Y-second search finds (2,3), shared by both selected records.
        assert_eq!(
            table.relocate_selection(&[0, 1], 9, 9).unwrap(),
            Some((2, 3))
        );
        assert_eq!(
            decode_main_overworld_event_tile_index(table.entries[0].destination_tile >> 1),
            Some((12, 13))
        );
        assert_eq!(
            decode_main_overworld_event_tile_index(table.entries[1].destination_tile >> 1),
            Some((60, 123))
        );
        assert_eq!(table.entries[2], untouched);

        let before = table.clone();
        assert_eq!(
            table.relocate_selection(&[0, 0], 1, 1),
            Err(EventRevealMoveError::DuplicateIndex(0))
        );
        assert_eq!(table, before);
        assert_eq!(
            table.relocate_selection(&[9], 1, 1),
            Err(EventRevealMoveError::IndexOutOfBounds { index: 9, len: 3 })
        );
        assert_eq!(table, before);
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
