use crate::{CompleteLevelFile, Entrance, Level, Map16Tile, ScreenExit, SecondaryExit};
use std::{collections::BTreeSet, fmt};

/// An ordered sequence mutation. Every index observes earlier commands in the same batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceEdit<T> {
    Insert {
        index: usize,
        value: T,
    },
    Replace {
        index: usize,
        value: T,
    },
    Remove {
        index: usize,
    },
    /// Moves a value before an index in the ordering that exists when this command runs.
    MoveBefore {
        from: usize,
        before: usize,
    },
}

/// A stable-key mutation for one level-local Map16 override.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16OverrideEdit {
    /// Replaces an existing key in place or appends a new key.
    Upsert {
        index: u32,
        tile: Map16Tile,
    },
    Remove {
        index: u32,
    },
}

/// One command in an atomic batch spanning the auxiliary level domains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelAuxiliaryEdit {
    Entrance(SequenceEdit<Entrance>),
    ScreenExit(SequenceEdit<ScreenExit>),
    SecondaryExit(SequenceEdit<SecondaryExit>),
    Map16Override(Map16OverrideEdit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuxiliaryCollection {
    Entrances,
    ScreenExits,
    SecondaryExits,
    Map16Overrides,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelAuxiliaryEditError {
    IndexOutOfBounds {
        command: usize,
        collection: AuxiliaryCollection,
        index: usize,
        len: usize,
    },
    MissingMap16Override {
        command: usize,
        index: u32,
    },
    DuplicateMap16Override(u32),
    TooManyRecords {
        collection: AuxiliaryCollection,
        count: usize,
    },
}

impl fmt::Display for LevelAuxiliaryEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid atomic auxiliary level edit: {self:?}")
    }
}

impl std::error::Error for LevelAuxiliaryEditError {}

impl Level {
    /// Atomically edits entrances, screen exits, secondary exits, and keyed Map16 overrides.
    ///
    /// Commands are ordered and may span domains. Existing malformed duplicate override keys are
    /// rejected even for an empty batch. Upsert retains an existing override's ordering position
    /// and appends new keys deterministically. Any failure leaves every level domain unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`LevelAuxiliaryEditError`] for invalid indexes, a missing keyed removal, duplicate
    /// override keys, or a resulting collection beyond the bounded `LMLEVEL2` record count.
    pub fn apply_auxiliary_edits(
        &mut self,
        edits: &[LevelAuxiliaryEdit],
    ) -> Result<(), LevelAuxiliaryEditError> {
        validate(self)?;
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        for (command, edit) in edits.iter().enumerate() {
            match edit {
                LevelAuxiliaryEdit::Entrance(edit) => apply_sequence(
                    &mut staged.entrances,
                    edit,
                    command,
                    AuxiliaryCollection::Entrances,
                )?,
                LevelAuxiliaryEdit::ScreenExit(edit) => apply_sequence(
                    &mut staged.screen_exits,
                    edit,
                    command,
                    AuxiliaryCollection::ScreenExits,
                )?,
                LevelAuxiliaryEdit::SecondaryExit(edit) => apply_sequence(
                    &mut staged.secondary_exits,
                    edit,
                    command,
                    AuxiliaryCollection::SecondaryExits,
                )?,
                LevelAuxiliaryEdit::Map16Override(edit) => {
                    apply_override(&mut staged.map16_overrides, edit, command)?;
                }
            }
        }
        validate(&staged)?;
        *self = staged;
        Ok(())
    }
}

fn apply_sequence<T: Clone>(
    values: &mut Vec<T>,
    edit: &SequenceEdit<T>,
    command: usize,
    collection: AuxiliaryCollection,
) -> Result<(), LevelAuxiliaryEditError> {
    match edit {
        SequenceEdit::Insert { index, value } => {
            if *index > values.len() {
                return Err(index_error(command, collection, *index, values.len()));
            }
            values.insert(*index, value.clone());
        }
        SequenceEdit::Replace { index, value } => {
            let len = values.len();
            let Some(target) = values.get_mut(*index) else {
                return Err(index_error(command, collection, *index, len));
            };
            *target = value.clone();
        }
        SequenceEdit::Remove { index } => {
            if *index >= values.len() {
                return Err(index_error(command, collection, *index, values.len()));
            }
            values.remove(*index);
        }
        SequenceEdit::MoveBefore { from, before } => {
            let len = values.len();
            if *from >= len {
                return Err(index_error(command, collection, *from, len));
            }
            if *before > len {
                return Err(index_error(command, collection, *before, len));
            }
            if from != before && from.checked_add(1) != Some(*before) {
                let value = values.remove(*from);
                values.insert(if before > from { before - 1 } else { *before }, value);
            }
        }
    }
    Ok(())
}

fn apply_override(
    values: &mut Vec<(u32, Map16Tile)>,
    edit: &Map16OverrideEdit,
    command: usize,
) -> Result<(), LevelAuxiliaryEditError> {
    match edit {
        Map16OverrideEdit::Upsert { index, tile } => {
            if let Some((_, current)) = values.iter_mut().find(|(key, _)| key == index) {
                *current = *tile;
            } else {
                values.push((*index, *tile));
            }
        }
        Map16OverrideEdit::Remove { index } => {
            let Some(position) = values.iter().position(|(key, _)| key == index) else {
                return Err(LevelAuxiliaryEditError::MissingMap16Override {
                    command,
                    index: *index,
                });
            };
            values.remove(position);
        }
    }
    Ok(())
}

fn validate(level: &Level) -> Result<(), LevelAuxiliaryEditError> {
    validate_count(AuxiliaryCollection::Entrances, level.entrances.len())?;
    validate_count(AuxiliaryCollection::ScreenExits, level.screen_exits.len())?;
    validate_count(
        AuxiliaryCollection::SecondaryExits,
        level.secondary_exits.len(),
    )?;
    validate_count(
        AuxiliaryCollection::Map16Overrides,
        level.map16_overrides.len(),
    )?;
    let mut keys = BTreeSet::new();
    for (index, _) in &level.map16_overrides {
        if !keys.insert(*index) {
            return Err(LevelAuxiliaryEditError::DuplicateMap16Override(*index));
        }
    }
    Ok(())
}

fn validate_count(
    collection: AuxiliaryCollection,
    count: usize,
) -> Result<(), LevelAuxiliaryEditError> {
    if count > CompleteLevelFile::MAX_RECORDS {
        Err(LevelAuxiliaryEditError::TooManyRecords { collection, count })
    } else {
        Ok(())
    }
}

fn index_error(
    command: usize,
    collection: AuxiliaryCollection,
    index: usize,
    len: usize,
) -> LevelAuxiliaryEditError {
    LevelAuxiliaryEditError::IndexOutOfBounds {
        command,
        collection,
        index,
        len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompleteLevelFile, EntranceKind, Subtile};

    fn entrance(x: u16) -> Entrance {
        Entrance {
            kind: EntranceKind::Secondary,
            x,
            y: 2,
            screen: 3,
            action: 4,
            raw_flags: 0x8050,
        }
    }

    fn tile(value: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(value),
            top_right: Subtile(value + 1),
            bottom_left: Subtile(value + 2),
            bottom_right: Subtile(value + 3),
            acts_like: value + 4,
        }
    }

    #[test]
    fn cross_domain_batch_is_ordered_and_complete_file_round_trips() {
        let mut level = Level {
            entrances: vec![entrance(1), entrance(2)],
            screen_exits: vec![ScreenExit { encoded: 1 }],
            secondary_exits: vec![SecondaryExit::default()],
            map16_overrides: vec![(3, tile(10))],
            ..Level::default()
        };
        level
            .apply_auxiliary_edits(&[
                LevelAuxiliaryEdit::Entrance(SequenceEdit::MoveBefore { from: 1, before: 0 }),
                LevelAuxiliaryEdit::Entrance(SequenceEdit::Replace {
                    index: 1,
                    value: entrance(9),
                }),
                LevelAuxiliaryEdit::ScreenExit(SequenceEdit::Insert {
                    index: 1,
                    value: ScreenExit { encoded: 2 },
                }),
                LevelAuxiliaryEdit::SecondaryExit(SequenceEdit::Remove { index: 0 }),
                LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Upsert {
                    index: 3,
                    tile: tile(20),
                }),
                LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Upsert {
                    index: 4,
                    tile: tile(30),
                }),
            ])
            .unwrap();
        assert_eq!(
            level
                .entrances
                .iter()
                .map(|value| value.x)
                .collect::<Vec<_>>(),
            [2, 9]
        );
        assert_eq!(
            level
                .screen_exits
                .iter()
                .map(|value| value.encoded)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(level.secondary_exits.is_empty());
        assert_eq!(level.map16_overrides, [(3, tile(20)), (4, tile(30))]);
        let encoded = CompleteLevelFile(level.clone()).encode().unwrap();
        assert_eq!(CompleteLevelFile::decode(&encoded).unwrap().0, level);
    }

    #[test]
    fn late_cross_domain_failure_rolls_back_every_domain() {
        let mut level = Level {
            entrances: vec![entrance(1)],
            screen_exits: vec![ScreenExit { encoded: 1 }],
            map16_overrides: vec![(3, tile(10))],
            ..Level::default()
        };
        let original = level.clone();
        let error = level
            .apply_auxiliary_edits(&[
                LevelAuxiliaryEdit::Entrance(SequenceEdit::Remove { index: 0 }),
                LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Upsert {
                    index: 4,
                    tile: tile(20),
                }),
                LevelAuxiliaryEdit::ScreenExit(SequenceEdit::Remove { index: 9 }),
            ])
            .unwrap_err();
        assert!(matches!(
            error,
            LevelAuxiliaryEditError::IndexOutOfBounds {
                command: 2,
                collection: AuxiliaryCollection::ScreenExits,
                ..
            }
        ));
        assert_eq!(level, original);
    }

    #[test]
    fn missing_removal_and_duplicate_public_state_are_atomic() {
        let mut level = Level {
            map16_overrides: vec![(3, tile(10))],
            ..Level::default()
        };
        let original = level.clone();
        assert_eq!(
            level.apply_auxiliary_edits(&[LevelAuxiliaryEdit::Map16Override(
                Map16OverrideEdit::Remove { index: 4 },
            )]),
            Err(LevelAuxiliaryEditError::MissingMap16Override {
                command: 0,
                index: 4,
            })
        );
        assert_eq!(level, original);

        level.map16_overrides.push((3, tile(20)));
        let malformed = level.clone();
        assert_eq!(
            level.apply_auxiliary_edits(&[]),
            Err(LevelAuxiliaryEditError::DuplicateMap16Override(3))
        );
        assert_eq!(level, malformed);
    }

    #[test]
    fn collection_limit_failure_is_atomic() {
        let mut level = Level {
            screen_exits: vec![ScreenExit::default(); CompleteLevelFile::MAX_RECORDS],
            ..Level::default()
        };
        let original = level.clone();
        assert_eq!(
            level.apply_auxiliary_edits(&[LevelAuxiliaryEdit::ScreenExit(SequenceEdit::Insert {
                index: CompleteLevelFile::MAX_RECORDS,
                value: ScreenExit { encoded: 1 },
            })]),
            Err(LevelAuxiliaryEditError::TooManyRecords {
                collection: AuxiliaryCollection::ScreenExits,
                count: CompleteLevelFile::MAX_RECORDS + 1,
            })
        );
        assert_eq!(level, original);
    }
}
