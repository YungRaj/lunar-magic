//! Versioned portable serialization for semantic overworld navigation graphs.

use crate::{OverworldPathGraph, PathDirection, PathEdge, PathGraphError, PathNode, Submap};

const MAGIC: &[u8; 8] = b"LMOWPATH";
const VERSION: u16 = 1;
const HEADER_LEN: usize = 16;
const NODE_LEN: usize = 10;
const EDGE_LEN: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    WrongLength { expected: usize, actual: usize },
    TooManyNodes(usize),
    TooManyEdges(usize),
    InvalidSubmap { node: usize, value: u8 },
    InvalidDirection { edge: usize, value: u8 },
    Overflow,
    Graph(PathGraphError),
}

impl std::fmt::Display for PathFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "overworld path file error: {self:?}")
    }
}

impl std::error::Error for PathFileError {}

impl From<PathGraphError> for PathFileError {
    fn from(value: PathGraphError) -> Self {
        Self::Graph(value)
    }
}

impl OverworldPathGraph {
    pub const MAX_FILE_LEN: usize =
        HEADER_LEN + Self::MAX_NODES * NODE_LEN + Self::MAX_EDGES * EDGE_LEN;

    /// Encodes this graph as deterministic `LMOWPATH` bytes after structural validation.
    ///
    /// # Errors
    ///
    /// Returns [`PathFileError`] for invalid graphs or counts not representable by the format.
    pub fn encode_file(&self) -> Result<Vec<u8>, PathFileError> {
        self.validate()?;
        let node_count = u16::try_from(self.nodes.len())
            .map_err(|_| PathFileError::TooManyNodes(self.nodes.len()))?;
        let edge_count = u16::try_from(self.edges.len())
            .map_err(|_| PathFileError::TooManyEdges(self.edges.len()))?;
        let capacity = encoded_len(self.nodes.len(), self.edges.len())?;
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&node_count.to_le_bytes());
        bytes.extend_from_slice(&edge_count.to_le_bytes());
        bytes.extend_from_slice(&[0; 2]);
        for node in &self.nodes {
            bytes.extend_from_slice(&node.id.to_le_bytes());
            bytes.extend_from_slice(&node.x.to_le_bytes());
            bytes.extend_from_slice(&node.y.to_le_bytes());
            bytes.push(node.submap.encoded());
            bytes.extend_from_slice(&node.level.unwrap_or(u16::MAX).to_le_bytes());
            bytes.push(node.raw_flags);
        }
        for edge in &self.edges {
            bytes.extend_from_slice(&edge.from.to_le_bytes());
            bytes.extend_from_slice(&edge.to.to_le_bytes());
            bytes.push(edge.direction.encoded());
            bytes.push(edge.exit_index.unwrap_or(u8::MAX));
            bytes.push(edge.raw_flags);
            bytes.push(0);
        }
        Ok(bytes)
    }

    /// Decodes one complete bounded `LMOWPATH` graph and validates every reference.
    ///
    /// # Errors
    ///
    /// Returns [`PathFileError`] for framing, size, enum, reference, or uniqueness failures.
    pub fn decode_file(bytes: &[u8]) -> Result<Self, PathFileError> {
        let header = bytes.get(..HEADER_LEN).ok_or(PathFileError::Truncated)?;
        if &header[..8] != MAGIC {
            return Err(PathFileError::WrongMagic);
        }
        let version = read_u16(header, 8);
        if version != VERSION {
            return Err(PathFileError::UnsupportedVersion(version));
        }
        if header[14..16] != [0; 2] {
            return Err(PathFileError::ReservedBytes);
        }
        let node_count = usize::from(read_u16(header, 10));
        let edge_count = usize::from(read_u16(header, 12));
        if node_count > Self::MAX_NODES {
            return Err(PathFileError::TooManyNodes(node_count));
        }
        if edge_count > Self::MAX_EDGES {
            return Err(PathFileError::TooManyEdges(edge_count));
        }
        let expected = encoded_len(node_count, edge_count)?;
        if bytes.len() != expected {
            return Err(PathFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut offset = HEADER_LEN;
        let mut nodes = Vec::with_capacity(node_count);
        for node in 0..node_count {
            let record = &bytes[offset..offset + NODE_LEN];
            offset += NODE_LEN;
            let submap = Submap::decode(record[6]).ok_or(PathFileError::InvalidSubmap {
                node,
                value: record[6],
            })?;
            let level = read_u16(record, 7);
            nodes.push(PathNode {
                id: read_u16(record, 0),
                x: read_u16(record, 2),
                y: read_u16(record, 4),
                submap,
                level: (level != u16::MAX).then_some(level),
                raw_flags: record[9],
            });
        }
        let mut edges = Vec::with_capacity(edge_count);
        for edge in 0..edge_count {
            let record = &bytes[offset..offset + EDGE_LEN];
            offset += EDGE_LEN;
            if record[7] != 0 {
                return Err(PathFileError::ReservedBytes);
            }
            let direction =
                PathDirection::decode(record[4]).ok_or(PathFileError::InvalidDirection {
                    edge,
                    value: record[4],
                })?;
            edges.push(PathEdge {
                from: read_u16(record, 0),
                to: read_u16(record, 2),
                direction,
                exit_index: (record[5] != u8::MAX).then_some(record[5]),
                raw_flags: record[6],
            });
        }
        let graph = Self { nodes, edges };
        graph.validate()?;
        Ok(graph)
    }
}

fn encoded_len(nodes: usize, edges: usize) -> Result<usize, PathFileError> {
    HEADER_LEN
        .checked_add(nodes.checked_mul(NODE_LEN).ok_or(PathFileError::Overflow)?)
        .and_then(|len| len.checked_add(edges.checked_mul(EDGE_LEN)?))
        .ok_or(PathFileError::Overflow)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> OverworldPathGraph {
        OverworldPathGraph {
            nodes: vec![
                PathNode {
                    id: 7,
                    x: 0x123,
                    y: 0x456,
                    submap: Submap::ForestOfIllusion,
                    level: Some(0x105),
                    raw_flags: 0xa0,
                },
                PathNode {
                    id: 8,
                    x: 9,
                    y: 10,
                    submap: Submap::StarWorld,
                    level: None,
                    raw_flags: 0x40,
                },
            ],
            edges: vec![PathEdge {
                from: 7,
                to: 8,
                direction: PathDirection::Left,
                exit_index: Some(0xfe),
                raw_flags: 0x81,
            }],
        }
    }

    #[test]
    fn exact_graph_and_unowned_flags_round_trip() {
        let expected = graph();
        let bytes = expected.encode_file().unwrap();
        assert_eq!(OverworldPathGraph::decode_file(&bytes).unwrap(), expected);
        assert_eq!(
            OverworldPathGraph::decode_file(&bytes)
                .unwrap()
                .encode_file()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn truncation_reserved_enums_and_stale_destinations_are_rejected() {
        let bytes = graph().encode_file().unwrap();
        for end in 0..bytes.len() {
            assert!(OverworldPathGraph::decode_file(&bytes[..end]).is_err());
        }
        let mut reserved = bytes.clone();
        reserved[14] = 1;
        assert_eq!(
            OverworldPathGraph::decode_file(&reserved),
            Err(PathFileError::ReservedBytes)
        );
        let mut direction = bytes.clone();
        direction[HEADER_LEN + 2 * NODE_LEN + 4] = 4;
        assert!(matches!(
            OverworldPathGraph::decode_file(&direction),
            Err(PathFileError::InvalidDirection { .. })
        ));
        let mut stale = bytes;
        stale[HEADER_LEN + 2 * NODE_LEN + 2] = 99;
        assert!(matches!(
            OverworldPathGraph::decode_file(&stale),
            Err(PathFileError::Graph(PathGraphError::MissingTo { .. }))
        ));
    }
}
