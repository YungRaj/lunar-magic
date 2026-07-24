//! Native path-link tables used by the SMW overworld engine.

use crate::{FixedTableEncodingError, OverworldEndpoint};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldPathTarget {
    pub y_tile: u8,
    pub x_tile: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldPathLink {
    pub source: OverworldEndpoint,
    pub destination: OverworldEndpoint,
    pub target: OverworldPathTarget,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverworldPathLinkTable {
    pub links: Vec<OverworldPathLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldPathLinkPlanes {
    pub sources: Vec<u8>,
    pub destinations: Vec<u8>,
    pub targets: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldPathLinkTableError {
    MisalignedSourcePlane(usize),
    MisalignedDestinationPlane(usize),
    MisalignedTargetPlane(usize),
    PlaneCountMismatch {
        sources: usize,
        destinations: usize,
        targets: usize,
    },
    TooManyLinks(usize),
    Encode(FixedTableEncodingError),
}

impl std::fmt::Display for OverworldPathLinkTableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld path-link table: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPathLinkTableError {}

impl From<FixedTableEncodingError> for OverworldPathLinkTableError {
    fn from(value: FixedTableEncodingError) -> Self {
        Self::Encode(value)
    }
}

impl OverworldPathLinkTable {
    pub const MAX_LINKS: usize = 128;
    pub const TARGET_LEN: usize = 2;

    /// Decodes the three native planes: five-byte source endpoints, five-byte destination
    /// endpoints, then two-byte engine target coordinates.
    ///
    /// # Errors
    ///
    /// Rejects partial records, unequal plane counts, and more than 128 links.
    pub fn decode_planes(
        sources: &[u8],
        destinations: &[u8],
        targets: &[u8],
    ) -> Result<Self, OverworldPathLinkTableError> {
        if sources.len() % OverworldEndpoint::ENCODED_LEN != 0 {
            return Err(OverworldPathLinkTableError::MisalignedSourcePlane(
                sources.len(),
            ));
        }
        if destinations.len() % OverworldEndpoint::ENCODED_LEN != 0 {
            return Err(OverworldPathLinkTableError::MisalignedDestinationPlane(
                destinations.len(),
            ));
        }
        if targets.len() % Self::TARGET_LEN != 0 {
            return Err(OverworldPathLinkTableError::MisalignedTargetPlane(
                targets.len(),
            ));
        }
        let source_count = sources.len() / OverworldEndpoint::ENCODED_LEN;
        let destination_count = destinations.len() / OverworldEndpoint::ENCODED_LEN;
        let target_count = targets.len() / Self::TARGET_LEN;
        if source_count != destination_count || source_count != target_count {
            return Err(OverworldPathLinkTableError::PlaneCountMismatch {
                sources: source_count,
                destinations: destination_count,
                targets: target_count,
            });
        }
        if source_count > Self::MAX_LINKS {
            return Err(OverworldPathLinkTableError::TooManyLinks(source_count));
        }
        let source_records = OverworldEndpoint::decode_all(sources)
            .map_err(OverworldPathLinkTableError::MisalignedSourcePlane)?;
        let destination_records = OverworldEndpoint::decode_all(destinations)
            .map_err(OverworldPathLinkTableError::MisalignedDestinationPlane)?;
        let links = source_records
            .into_iter()
            .zip(destination_records)
            .zip(targets.chunks_exact(Self::TARGET_LEN))
            .map(|((source, destination), target)| OverworldPathLink {
                source,
                destination,
                target: OverworldPathTarget {
                    y_tile: target[0],
                    x_tile: target[1],
                },
            })
            .collect();
        Ok(Self { links })
    }

    /// Encodes all three planes without changing entry order or sentinel values.
    ///
    /// # Errors
    ///
    /// Rejects more than 128 links or aggregate-size overflow.
    pub fn encode_planes(&self) -> Result<OverworldPathLinkPlanes, OverworldPathLinkTableError> {
        if self.links.len() > Self::MAX_LINKS {
            return Err(OverworldPathLinkTableError::TooManyLinks(self.links.len()));
        }
        let sources = self
            .links
            .iter()
            .map(|link| link.source)
            .collect::<Vec<_>>();
        let destinations = self
            .links
            .iter()
            .map(|link| link.destination)
            .collect::<Vec<_>>();
        let mut targets =
            Vec::with_capacity(self.links.len().checked_mul(Self::TARGET_LEN).ok_or(
                FixedTableEncodingError {
                    records: self.links.len(),
                    record_len: Self::TARGET_LEN,
                },
            )?);
        for link in &self.links {
            targets.extend_from_slice(&[link.target.y_tile, link.target.x_tile]);
        }
        Ok(OverworldPathLinkPlanes {
            sources: OverworldEndpoint::encode_all(&sources)?,
            destinations: OverworldEndpoint::encode_all(&destinations)?,
            targets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(value: u16) -> OverworldEndpoint {
        OverworldEndpoint {
            x: value,
            y: value + 1,
            submap: u8::try_from(value).unwrap(),
        }
    }

    #[test]
    fn planes_round_trip_without_normalizing_sentinels() {
        let table = OverworldPathLinkTable {
            links: vec![
                OverworldPathLink {
                    source: endpoint(1),
                    destination: endpoint(2),
                    target: OverworldPathTarget {
                        y_tile: 3,
                        x_tile: 4,
                    },
                },
                OverworldPathLink {
                    source: OverworldEndpoint {
                        x: 0xffff,
                        y: 0xffff,
                        submap: 0xff,
                    },
                    destination: endpoint(5),
                    target: OverworldPathTarget {
                        y_tile: 0xff,
                        x_tile: 0xff,
                    },
                },
            ],
        };
        let planes = table.encode_planes().unwrap();
        assert_eq!(
            OverworldPathLinkTable::decode_planes(
                &planes.sources,
                &planes.destinations,
                &planes.targets
            )
            .unwrap(),
            table
        );
    }

    #[test]
    fn partial_mismatched_and_excessive_planes_are_rejected() {
        assert_eq!(
            OverworldPathLinkTable::decode_planes(&[0; 4], &[], &[]),
            Err(OverworldPathLinkTableError::MisalignedSourcePlane(4))
        );
        assert!(matches!(
            OverworldPathLinkTable::decode_planes(&[0; 5], &[], &[]),
            Err(OverworldPathLinkTableError::PlaneCountMismatch { .. })
        ));
        let excessive = OverworldPathLinkTable {
            links: vec![
                OverworldPathLink {
                    source: endpoint(0),
                    destination: endpoint(0),
                    target: OverworldPathTarget {
                        y_tile: 0,
                        x_tile: 0,
                    },
                };
                OverworldPathLinkTable::MAX_LINKS + 1
            ],
        };
        assert!(matches!(
            excessive.encode_planes(),
            Err(OverworldPathLinkTableError::TooManyLinks(129))
        ));
    }
}
