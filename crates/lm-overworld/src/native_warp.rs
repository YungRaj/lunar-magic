//! Native overworld warp/exit endpoint tables used by the SMW engine.

/// Semantic choice represented by control `$0066` in Lunar Magic's exit-link dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldWarpReturnChoice {
    /// Combo row 0: remove any setting associated with the selected tile.
    NoSetting,
    /// Combo row 1: retain a one-way exit without creating a return-table record.
    OneWay,
    /// Combo rows 2 through 257: use the selected native return-table record.
    Record(u8),
}

impl OverworldWarpReturnChoice {
    /// Decodes the exact owner-drawn combo row used by Lunar Magic 3.63.
    #[must_use]
    pub const fn from_lunar_magic_combo_index(index: u16) -> Option<Self> {
        match index {
            0 => Some(Self::NoSetting),
            1 => Some(Self::OneWay),
            2..=257 => Some(Self::Record((index - 2) as u8)),
            _ => None,
        }
    }

    /// Returns the original owner-drawn combo row.
    #[must_use]
    pub const fn lunar_magic_combo_index(self) -> u16 {
        match self {
            Self::NoSetting => 0,
            Self::OneWay => 1,
            Self::Record(index) => index as u16 + 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldWarpEndpoint {
    /// Packed vertical/submap coordinate. Its internal bit fields remain intentionally opaque.
    pub packed_vertical: u16,
    pub horizontal_tile: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldWarpLink {
    pub source: OverworldWarpEndpoint,
    pub destination: OverworldWarpEndpoint,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverworldWarpLinkTable {
    pub links: Vec<OverworldWarpLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldWarpLinkPlanes {
    pub source_vertical: Vec<u8>,
    pub source_horizontal: Vec<u8>,
    pub destination_vertical: Vec<u8>,
    pub destination_horizontal: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldWarpLinkTableError {
    MisalignedPlane { plane: &'static str, len: usize },
    PlaneCountMismatch([usize; 4]),
    TooManyLinks(usize),
    LengthOverflow,
}

impl std::fmt::Display for OverworldWarpLinkTableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld warp-link table: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpLinkTableError {}

impl OverworldWarpLinkTable {
    pub const MAX_LINKS: usize = 256;
    pub const WORD_LEN: usize = 2;

    /// Decodes the four native little-endian coordinate planes without interpreting packed bits.
    ///
    /// # Errors
    ///
    /// Rejects partial words, unequal plane counts, and more than 256 links.
    pub fn decode_planes(
        source_vertical: &[u8],
        source_horizontal: &[u8],
        destination_vertical: &[u8],
        destination_horizontal: &[u8],
    ) -> Result<Self, OverworldWarpLinkTableError> {
        let inputs = [
            ("source vertical", source_vertical),
            ("source horizontal", source_horizontal),
            ("destination vertical", destination_vertical),
            ("destination horizontal", destination_horizontal),
        ];
        for (plane, bytes) in inputs {
            if bytes.len() % Self::WORD_LEN != 0 {
                return Err(OverworldWarpLinkTableError::MisalignedPlane {
                    plane,
                    len: bytes.len(),
                });
            }
        }
        let counts = [
            source_vertical.len() / 2,
            source_horizontal.len() / 2,
            destination_vertical.len() / 2,
            destination_horizontal.len() / 2,
        ];
        if counts.iter().any(|count| *count != counts[0]) {
            return Err(OverworldWarpLinkTableError::PlaneCountMismatch(counts));
        }
        if counts[0] > Self::MAX_LINKS {
            return Err(OverworldWarpLinkTableError::TooManyLinks(counts[0]));
        }
        let word = |bytes: &[u8], index: usize| {
            u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
        };
        let links = (0..counts[0])
            .map(|index| OverworldWarpLink {
                source: OverworldWarpEndpoint {
                    packed_vertical: word(source_vertical, index),
                    horizontal_tile: word(source_horizontal, index),
                },
                destination: OverworldWarpEndpoint {
                    packed_vertical: word(destination_vertical, index),
                    horizontal_tile: word(destination_horizontal, index),
                },
            })
            .collect();
        Ok(Self { links })
    }

    /// Encodes all four planes exactly, preserving order and `0xffff` sentinels.
    ///
    /// # Errors
    ///
    /// Rejects more than 256 links and aggregate-size overflow.
    pub fn encode_planes(&self) -> Result<OverworldWarpLinkPlanes, OverworldWarpLinkTableError> {
        if self.links.len() > Self::MAX_LINKS {
            return Err(OverworldWarpLinkTableError::TooManyLinks(self.links.len()));
        }
        let capacity = self
            .links
            .len()
            .checked_mul(Self::WORD_LEN)
            .ok_or(OverworldWarpLinkTableError::LengthOverflow)?;
        let mut planes = OverworldWarpLinkPlanes {
            source_vertical: Vec::with_capacity(capacity),
            source_horizontal: Vec::with_capacity(capacity),
            destination_vertical: Vec::with_capacity(capacity),
            destination_horizontal: Vec::with_capacity(capacity),
        };
        for link in &self.links {
            planes
                .source_vertical
                .extend_from_slice(&link.source.packed_vertical.to_le_bytes());
            planes
                .source_horizontal
                .extend_from_slice(&link.source.horizontal_tile.to_le_bytes());
            planes
                .destination_vertical
                .extend_from_slice(&link.destination.packed_vertical.to_le_bytes());
            planes
                .destination_horizontal
                .extend_from_slice(&link.destination.horizontal_tile.to_le_bytes());
        }
        Ok(planes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_choice_covers_all_258_original_combo_rows_exactly() {
        assert_eq!(
            OverworldWarpReturnChoice::from_lunar_magic_combo_index(0),
            Some(OverworldWarpReturnChoice::NoSetting)
        );
        assert_eq!(
            OverworldWarpReturnChoice::from_lunar_magic_combo_index(1),
            Some(OverworldWarpReturnChoice::OneWay)
        );
        for index in u8::MIN..=u8::MAX {
            let choice = OverworldWarpReturnChoice::Record(index);
            assert_eq!(
                OverworldWarpReturnChoice::from_lunar_magic_combo_index(
                    choice.lunar_magic_combo_index()
                ),
                Some(choice)
            );
        }
        assert_eq!(
            OverworldWarpReturnChoice::from_lunar_magic_combo_index(258),
            None
        );
    }

    #[test]
    fn planes_round_trip_without_normalizing_sentinels() {
        let table = OverworldWarpLinkTable {
            links: vec![
                OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: 0x0210,
                        horizontal_tile: 7,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: 0x04a8,
                        horizontal_tile: 18,
                    },
                },
                OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: 0xffff,
                        horizontal_tile: 0xffff,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: 0xffff,
                        horizontal_tile: 0xffff,
                    },
                },
            ],
        };
        let planes = table.encode_planes().unwrap();
        assert_eq!(
            OverworldWarpLinkTable::decode_planes(
                &planes.source_vertical,
                &planes.source_horizontal,
                &planes.destination_vertical,
                &planes.destination_horizontal,
            )
            .unwrap(),
            table
        );
    }

    #[test]
    fn malformed_and_excessive_planes_are_rejected() {
        assert!(matches!(
            OverworldWarpLinkTable::decode_planes(&[0], &[], &[], &[]),
            Err(OverworldWarpLinkTableError::MisalignedPlane { .. })
        ));
        assert!(matches!(
            OverworldWarpLinkTable::decode_planes(&[0, 0], &[], &[], &[]),
            Err(OverworldWarpLinkTableError::PlaneCountMismatch(_))
        ));
        let excessive = OverworldWarpLinkTable {
            links: vec![
                OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: 0,
                        horizontal_tile: 0,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: 0,
                        horizontal_tile: 0,
                    },
                };
                OverworldWarpLinkTable::MAX_LINKS + 1
            ],
        };
        assert!(matches!(
            excessive.encode_planes(),
            Err(OverworldWarpLinkTableError::TooManyLinks(257))
        ));
    }
}
