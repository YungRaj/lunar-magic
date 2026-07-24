//! Failure-atomic stable-key editing for portable overworld navigation graphs.

use crate::{OverworldPathGraph, PathDirection, PathEdge, PathGraphError, PathNode};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathGraphEdit {
    UpsertNode(PathNode),
    RemoveNode(u16),
    UpsertEdge(PathEdge),
    RemoveEdge { from: u16, direction: PathDirection },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EditKey {
    Node(u16),
    Edge(u16, u8),
}

impl PathGraphEdit {
    const fn key(self) -> EditKey {
        match self {
            Self::UpsertNode(node) => EditKey::Node(node.id),
            Self::RemoveNode(id) => EditKey::Node(id),
            Self::UpsertEdge(edge) => EditKey::Edge(edge.from, direction_key(edge.direction)),
            Self::RemoveEdge { from, direction } => EditKey::Edge(from, direction_key(direction)),
        }
    }
}

const fn direction_key(direction: PathDirection) -> u8 {
    match direction {
        PathDirection::Up => 0,
        PathDirection::Right => 1,
        PathDirection::Down => 2,
        PathDirection::Left => 3,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathGraphEditError {
    Initial(PathGraphError),
    DuplicateTarget,
    MissingNode(u16),
    MissingEdge { from: u16, direction: PathDirection },
    Final(PathGraphError),
}

impl fmt::Display for PathGraphEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld path edit: {self:?}")
    }
}

impl std::error::Error for PathGraphEditError {}

impl OverworldPathGraph {
    /// Applies one ordered stable-key batch on a clone and optionally enforces reciprocity.
    ///
    /// Node upserts and edge upserts replace in place or append deterministically. Removing a node
    /// also removes all incident edges. Edge keys are `(from, direction)`, matching graph
    /// uniqueness. The initial graph needs structural validity; reciprocity is checked only after
    /// the complete batch so a script can repair a previously incomplete pair.
    ///
    /// # Errors
    ///
    /// Returns [`PathGraphEditError`] for duplicate command targets, missing removals, or invalid
    /// initial/final graphs without changing the receiver.
    pub fn apply_edits(
        &mut self,
        edits: &[PathGraphEdit],
        require_reciprocal: bool,
    ) -> Result<(), PathGraphEditError> {
        self.validate().map_err(PathGraphEditError::Initial)?;
        let mut keys = BTreeSet::new();
        for edit in edits {
            if !keys.insert(edit.key()) {
                return Err(PathGraphEditError::DuplicateTarget);
            }
        }
        if edits.is_empty() {
            return final_validate(self, require_reciprocal);
        }
        let mut staged = self.clone();
        for edit in edits {
            apply_edit(&mut staged, *edit)?;
        }
        final_validate(&staged, require_reciprocal)?;
        *self = staged;
        Ok(())
    }
}

fn apply_edit(
    graph: &mut OverworldPathGraph,
    edit: PathGraphEdit,
) -> Result<(), PathGraphEditError> {
    match edit {
        PathGraphEdit::UpsertNode(node) => {
            upsert(&mut graph.nodes, node, |value| value.id == node.id);
        }
        PathGraphEdit::RemoveNode(id) => {
            graph
                .remove_node(id)
                .ok_or(PathGraphEditError::MissingNode(id))?;
        }
        PathGraphEdit::UpsertEdge(edge) => upsert(&mut graph.edges, edge, |value| {
            value.from == edge.from && value.direction == edge.direction
        }),
        PathGraphEdit::RemoveEdge { from, direction } => {
            let index = graph
                .edges
                .iter()
                .position(|edge| edge.from == from && edge.direction == direction)
                .ok_or(PathGraphEditError::MissingEdge { from, direction })?;
            graph.edges.remove(index);
        }
    }
    Ok(())
}

fn upsert<T>(values: &mut Vec<T>, value: T, matches: impl Fn(&T) -> bool) {
    if let Some(index) = values.iter().position(matches) {
        values[index] = value;
    } else {
        values.push(value);
    }
}

fn final_validate(
    graph: &OverworldPathGraph,
    require_reciprocal: bool,
) -> Result<(), PathGraphEditError> {
    if require_reciprocal {
        graph.validate_reciprocal()
    } else {
        graph.validate()
    }
    .map_err(PathGraphEditError::Final)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Submap;

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
    fn mixed_batch_can_repair_reciprocity_and_preserves_order() {
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2)],
            edges: vec![edge(1, 2, PathDirection::Right)],
        };
        graph
            .apply_edits(
                &[
                    PathGraphEdit::UpsertNode(PathNode { x: 9, ..node(1) }),
                    PathGraphEdit::UpsertEdge(edge(2, 1, PathDirection::Left)),
                ],
                true,
            )
            .unwrap();
        assert_eq!(graph.nodes[0].x, 9);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn duplicate_missing_and_late_stale_edges_are_atomic() {
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2)],
            edges: Vec::new(),
        };
        let original = graph.clone();
        assert_eq!(
            graph.apply_edits(
                &[
                    PathGraphEdit::RemoveNode(1),
                    PathGraphEdit::UpsertNode(node(1))
                ],
                false
            ),
            Err(PathGraphEditError::DuplicateTarget)
        );
        assert_eq!(graph, original);
        assert!(matches!(
            graph.apply_edits(
                &[PathGraphEdit::UpsertEdge(edge(1, 9, PathDirection::Right))],
                false
            ),
            Err(PathGraphEditError::Final(PathGraphError::MissingTo { .. }))
        ));
        assert_eq!(graph, original);
        assert_eq!(
            graph.apply_edits(
                &[PathGraphEdit::RemoveEdge {
                    from: 1,
                    direction: PathDirection::Up
                }],
                false
            ),
            Err(PathGraphEditError::MissingEdge {
                from: 1,
                direction: PathDirection::Up
            })
        );
    }

    #[test]
    fn node_removal_cascades_incident_edges() {
        let mut first = edge(1, 2, PathDirection::Right);
        first.set_one_way(true);
        let mut graph = OverworldPathGraph {
            nodes: vec![node(1), node(2)],
            edges: vec![first],
        };
        graph
            .apply_edits(&[PathGraphEdit::RemoveNode(2)], true)
            .unwrap();
        assert_eq!(graph.nodes, [node(1)]);
        assert!(graph.edges.is_empty());
    }
}
