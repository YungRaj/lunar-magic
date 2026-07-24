use crate::{
    MetadataError, OverworldLevelName, OverworldMetadata, PlayerStart, Submap, SubmapSettings,
};
use std::{collections::BTreeSet, fmt};

/// One stable-key metadata operation suitable for undoable editor batches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataEdit {
    UpsertLevelName(OverworldLevelName),
    RemoveLevelName(u16),
    UpsertPlayerStart(PlayerStart),
    RemovePlayerStart(u8),
    UpsertSubmapSettings(SubmapSettings),
    RemoveSubmapSettings(Submap),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MetadataKey {
    Level(u16),
    Player(u8),
    Submap(u8),
}

impl MetadataEdit {
    const fn key(&self) -> MetadataKey {
        match self {
            Self::UpsertLevelName(value) => MetadataKey::Level(value.level),
            Self::RemoveLevelName(level) => MetadataKey::Level(*level),
            Self::UpsertPlayerStart(value) => MetadataKey::Player(value.player),
            Self::RemovePlayerStart(player) => MetadataKey::Player(*player),
            Self::UpsertSubmapSettings(value) => MetadataKey::Submap(value.submap.encoded()),
            Self::RemoveSubmapSettings(submap) => MetadataKey::Submap(submap.encoded()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataEditError {
    DuplicateTarget,
    MissingLevelName(u16),
    MissingPlayerStart(u8),
    MissingSubmapSettings(Submap),
    Metadata(MetadataError),
}

impl fmt::Display for MetadataEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld metadata edit: {self:?}")
    }
}

impl std::error::Error for MetadataEditError {}

impl From<MetadataError> for MetadataEditError {
    fn from(error: MetadataError) -> Self {
        Self::Metadata(error)
    }
}

impl OverworldMetadata {
    /// Applies unique stable-key operations atomically while preserving unaffected record order.
    ///
    /// Upserts replace an existing record in place or append a new key. Removals require the key
    /// to exist. Two operations targeting the same domain/key are rejected because their order
    /// would otherwise affect the result. The complete staged metadata is validated before commit.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataEditError`] for duplicate command targets, missing removals, collection
    /// limits, or invalid final key uniqueness. Failure leaves the metadata unchanged.
    pub fn apply_edits(&mut self, edits: &[MetadataEdit]) -> Result<(), MetadataEditError> {
        self.validate()?;
        let mut keys = BTreeSet::new();
        for edit in edits {
            if !keys.insert(edit.key()) {
                return Err(MetadataEditError::DuplicateTarget);
            }
        }
        if edits.is_empty() {
            return Ok(());
        }

        let mut staged = self.clone();
        for edit in edits {
            match edit {
                MetadataEdit::UpsertLevelName(value) => {
                    upsert(&mut staged.level_names, value.clone(), |entry| {
                        entry.level == value.level
                    });
                }
                MetadataEdit::RemoveLevelName(level) => {
                    remove(&mut staged.level_names, |entry| entry.level == *level)
                        .ok_or(MetadataEditError::MissingLevelName(*level))?;
                }
                MetadataEdit::UpsertPlayerStart(value) => {
                    upsert(&mut staged.player_starts, *value, |entry| {
                        entry.player == value.player
                    });
                }
                MetadataEdit::RemovePlayerStart(player) => {
                    remove(&mut staged.player_starts, |entry| entry.player == *player)
                        .ok_or(MetadataEditError::MissingPlayerStart(*player))?;
                }
                MetadataEdit::UpsertSubmapSettings(value) => {
                    upsert(&mut staged.submap_settings, *value, |entry| {
                        entry.submap == value.submap
                    });
                }
                MetadataEdit::RemoveSubmapSettings(submap) => {
                    remove(&mut staged.submap_settings, |entry| entry.submap == *submap)
                        .ok_or(MetadataEditError::MissingSubmapSettings(*submap))?;
                }
            }
        }
        staged.validate()?;
        *self = staged;
        Ok(())
    }
}

fn upsert<T>(values: &mut Vec<T>, value: T, matches: impl Fn(&T) -> bool) {
    if let Some(index) = values.iter().position(matches) {
        values[index] = value;
    } else {
        values.push(value);
    }
}

fn remove<T>(values: &mut Vec<T>, matches: impl Fn(&T) -> bool) -> Option<T> {
    values
        .iter()
        .position(matches)
        .map(|index| values.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(level: u16, tile: u8, raw_flags: u8) -> OverworldLevelName {
        OverworldLevelName {
            level,
            tiles: [tile; OverworldLevelName::TILE_COUNT],
            raw_flags,
        }
    }

    fn metadata() -> OverworldMetadata {
        OverworldMetadata {
            level_names: vec![name(1, 1, 0x80), name(2, 2, 0x40)],
            player_starts: vec![PlayerStart {
                player: 0,
                x: 10,
                y: 20,
                submap: Submap::Main,
                raw_flags: 0xa0,
            }],
            submap_settings: vec![SubmapSettings {
                submap: Submap::Main,
                music: 1,
                palette: 2,
                layer1_scroll: 3,
                layer2_scroll: 4,
                raw_flags: 0x8123,
                unknown: [5, 6, 7, 8, 9],
            }],
        }
    }

    #[test]
    fn mixed_batch_replaces_in_place_appends_and_removes() {
        let mut metadata = metadata();
        metadata
            .apply_edits(&[
                MetadataEdit::UpsertLevelName(name(1, 9, 0x81)),
                MetadataEdit::RemoveLevelName(2),
                MetadataEdit::UpsertPlayerStart(PlayerStart {
                    player: 1,
                    x: 30,
                    y: 40,
                    submap: Submap::StarWorld,
                    raw_flags: 0x55,
                }),
                MetadataEdit::UpsertSubmapSettings(SubmapSettings {
                    submap: Submap::Main,
                    music: 7,
                    ..metadata.submap_settings[0]
                }),
            ])
            .unwrap();
        assert_eq!(metadata.level_names, [name(1, 9, 0x81)]);
        assert_eq!(
            metadata
                .player_starts
                .iter()
                .map(|value| value.player)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(metadata.submap_settings[0].music, 7);
        assert_eq!(metadata.submap_settings[0].unknown, [5, 6, 7, 8, 9]);
    }

    #[test]
    fn duplicate_and_missing_operations_leave_every_domain_unchanged() {
        let mut metadata = metadata();
        let original = metadata.clone();
        assert_eq!(
            metadata.apply_edits(&[
                MetadataEdit::RemoveLevelName(1),
                MetadataEdit::UpsertLevelName(name(1, 8, 0)),
            ]),
            Err(MetadataEditError::DuplicateTarget)
        );
        assert_eq!(metadata, original);
        assert_eq!(
            metadata.apply_edits(&[
                MetadataEdit::UpsertPlayerStart(PlayerStart {
                    player: 1,
                    x: 0,
                    y: 0,
                    submap: Submap::Main,
                    raw_flags: 0,
                }),
                MetadataEdit::RemoveSubmapSettings(Submap::StarWorld),
            ]),
            Err(MetadataEditError::MissingSubmapSettings(Submap::StarWorld))
        );
        assert_eq!(metadata, original);
    }

    #[test]
    fn count_limit_failure_is_atomic() {
        let mut metadata = OverworldMetadata {
            level_names: (0..OverworldMetadata::MAX_LEVEL_NAMES)
                .map(|level| name(u16::try_from(level).unwrap(), 0, 0))
                .collect(),
            ..OverworldMetadata::default()
        };
        let original = metadata.clone();
        assert!(matches!(
            metadata.apply_edits(&[MetadataEdit::UpsertLevelName(name(0x8000, 1, 0))]),
            Err(MetadataEditError::Metadata(
                MetadataError::TooManyLevelNames(_)
            ))
        ));
        assert_eq!(metadata, original);
    }

    #[test]
    fn edited_metadata_round_trips_exactly() {
        let mut metadata = metadata();
        metadata
            .apply_edits(&[MetadataEdit::UpsertLevelName(name(1, 0xee, 0x82))])
            .unwrap();
        let encoded = metadata.encode_file().unwrap();
        assert_eq!(OverworldMetadata::decode_file(&encoded).unwrap(), metadata);
    }
}
