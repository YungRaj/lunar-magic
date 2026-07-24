use crate::Submap;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PathDirection {
    Up,
    Right,
    Down,
    Left,
}

impl PathDirection {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Right => Self::Left,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
        }
    }

    pub(crate) const fn encoded(self) -> u8 {
        match self {
            Self::Up => 0,
            Self::Right => 1,
            Self::Down => 2,
            Self::Left => 3,
        }
    }

    pub(crate) const fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Up),
            1 => Some(Self::Right),
            2 => Some(Self::Down),
            3 => Some(Self::Left),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathNode {
    pub id: u16,
    pub x: u16,
    pub y: u16,
    pub submap: Submap,
    pub level: Option<u16>,
    /// Revision-specific bits not owned by the portable path editor.
    pub raw_flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathEdge {
    pub from: u16,
    pub to: u16,
    pub direction: PathDirection,
    pub exit_index: Option<u8>,
    /// Bit zero means the edge is deliberately one-way. Other bits remain unowned.
    pub raw_flags: u8,
}

impl PathEdge {
    pub const ONE_WAY_FLAG: u8 = 1;

    #[must_use]
    pub const fn is_one_way(self) -> bool {
        self.raw_flags & Self::ONE_WAY_FLAG != 0
    }

    pub fn set_one_way(&mut self, one_way: bool) {
        if one_way {
            self.raw_flags |= Self::ONE_WAY_FLAG;
        } else {
            self.raw_flags &= !Self::ONE_WAY_FLAG;
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverworldPathGraph {
    pub nodes: Vec<PathNode>,
    pub edges: Vec<PathEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathGraphError {
    TooManyNodes(usize),
    TooManyEdges(usize),
    DuplicateNode(u16),
    MissingFrom { edge: usize, node: u16 },
    MissingTo { edge: usize, node: u16 },
    SelfEdge { edge: usize, node: u16 },
    DuplicateEdge { first: usize, duplicate: usize },
    MissingReciprocal { edge: usize },
}

impl std::fmt::Display for PathGraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld path graph: {self:?}")
    }
}

impl std::error::Error for PathGraphError {}

impl OverworldPathGraph {
    pub const MAX_NODES: usize = 4096;
    pub const MAX_EDGES: usize = 8192;

    /// Validates stable identifiers and every edge destination without requiring reciprocal paths.
    ///
    /// # Errors
    ///
    /// Returns [`PathGraphError`] for limits, duplicate IDs/edges, self edges, or stale endpoints.
    pub fn validate(&self) -> Result<(), PathGraphError> {
        if self.nodes.len() > Self::MAX_NODES {
            return Err(PathGraphError::TooManyNodes(self.nodes.len()));
        }
        if self.edges.len() > Self::MAX_EDGES {
            return Err(PathGraphError::TooManyEdges(self.edges.len()));
        }
        let mut ids = HashSet::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if !ids.insert(node.id) {
                return Err(PathGraphError::DuplicateNode(node.id));
            }
        }
        for (edge_index, edge) in self.edges.iter().enumerate() {
            if !ids.contains(&edge.from) {
                return Err(PathGraphError::MissingFrom {
                    edge: edge_index,
                    node: edge.from,
                });
            }
            if !ids.contains(&edge.to) {
                return Err(PathGraphError::MissingTo {
                    edge: edge_index,
                    node: edge.to,
                });
            }
            if edge.from == edge.to {
                return Err(PathGraphError::SelfEdge {
                    edge: edge_index,
                    node: edge.from,
                });
            }
            if let Some(first) = self.edges[..edge_index].iter().position(|candidate| {
                candidate.from == edge.from && candidate.direction == edge.direction
            }) {
                return Err(PathGraphError::DuplicateEdge {
                    first,
                    duplicate: edge_index,
                });
            }
        }
        Ok(())
    }

    /// Additionally requires every edge not marked one-way to have an opposite reverse edge.
    ///
    /// # Errors
    ///
    /// Returns graph validation errors or [`PathGraphError::MissingReciprocal`].
    pub fn validate_reciprocal(&self) -> Result<(), PathGraphError> {
        self.validate()?;
        for (index, edge) in self.edges.iter().enumerate() {
            if !edge.is_one_way()
                && !self.edges.iter().any(|candidate| {
                    candidate.from == edge.to
                        && candidate.to == edge.from
                        && candidate.direction == edge.direction.opposite()
                })
            {
                return Err(PathGraphError::MissingReciprocal { edge: index });
            }
        }
        Ok(())
    }

    /// Removes a node and all incident edges as one semantic edit.
    #[must_use]
    pub fn remove_node(&mut self, id: u16) -> Option<PathNode> {
        let index = self.nodes.iter().position(|node| node.id == id)?;
        let node = self.nodes.remove(index);
        self.edges.retain(|edge| edge.from != id && edge.to != id);
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u16) -> PathNode {
        PathNode {
            id,
            x: id,
            y: id + 1,
            submap: Submap::Main,
            level: Some(id),
            raw_flags: 0x80,
        }
    }

    fn edge(from: u16, to: u16, direction: PathDirection) -> PathEdge {
        PathEdge {
            from,
            to,
            direction,
            exit_index: None,
            raw_flags: 0,
        }
    }

    #[test]
    fn reciprocal_and_one_way_edges_are_distinguished() {
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2)],
            edges: vec![edge(1, 2, PathDirection::Right)],
        };
        assert_eq!(
            graph.validate_reciprocal(),
            Err(PathGraphError::MissingReciprocal { edge: 0 })
        );
        graph.edges[0].set_one_way(true);
        graph.validate_reciprocal().unwrap();
        graph.edges[0].set_one_way(false);
        graph.edges.push(edge(2, 1, PathDirection::Left));
        graph.validate_reciprocal().unwrap();
    }

    #[test]
    fn stale_duplicate_and_self_destinations_are_rejected() {
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2)],
            edges: vec![edge(1, 3, PathDirection::Right)],
        };
        assert!(matches!(
            graph.validate(),
            Err(PathGraphError::MissingTo { node: 3, .. })
        ));
        graph.edges = vec![edge(1, 1, PathDirection::Right)];
        assert!(matches!(
            graph.validate(),
            Err(PathGraphError::SelfEdge { .. })
        ));
        graph.edges = vec![
            edge(1, 2, PathDirection::Right),
            edge(1, 2, PathDirection::Right),
        ];
        assert!(matches!(
            graph.validate(),
            Err(PathGraphError::DuplicateEdge { .. })
        ));
    }

    #[test]
    fn node_removal_cannot_leave_stale_edges() {
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2), node(3)],
            edges: vec![
                edge(1, 2, PathDirection::Right),
                edge(2, 3, PathDirection::Down),
            ],
        };
        assert_eq!(graph.remove_node(2), Some(node(2)));
        assert!(graph.edges.is_empty());
        graph.validate().unwrap();
    }
}
