use crate::Submap;
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldLevelName {
    pub level: u16,
    /// Exact tile values, including spacing and terminator-like values owned by the source format.
    pub tiles: [u8; Self::TILE_COUNT],
    /// Revision-specific bits not owned by the portable editor.
    pub raw_flags: u8,
}

impl OverworldLevelName {
    pub const TILE_COUNT: usize = 19;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerStart {
    pub player: u8,
    pub x: u16,
    pub y: u16,
    pub submap: Submap,
    pub raw_flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmapSettings {
    pub submap: Submap,
    pub music: u8,
    pub palette: u8,
    pub layer1_scroll: u8,
    pub layer2_scroll: u8,
    pub raw_flags: u16,
    /// Bytes retained for revision fields whose semantics have not yet been established.
    pub unknown: [u8; 5],
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverworldMetadata {
    pub level_names: Vec<OverworldLevelName>,
    pub player_starts: Vec<PlayerStart>,
    pub submap_settings: Vec<SubmapSettings>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataError {
    TooManyLevelNames(usize),
    TooManyPlayerStarts(usize),
    TooManySubmapSettings(usize),
    DuplicateLevel(u16),
    DuplicatePlayer(u8),
    DuplicateSubmap(Submap),
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld metadata: {self:?}")
    }
}

impl std::error::Error for MetadataError {}

impl OverworldMetadata {
    pub const MAX_LEVEL_NAMES: usize = 512;
    pub const MAX_PLAYER_STARTS: usize = 4;
    pub const MAX_SUBMAP_SETTINGS: usize = 7;

    /// Validates bounded collections and the stable key for every record.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] for count limits or duplicate levels, players, or submaps.
    pub fn validate(&self) -> Result<(), MetadataError> {
        if self.level_names.len() > Self::MAX_LEVEL_NAMES {
            return Err(MetadataError::TooManyLevelNames(self.level_names.len()));
        }
        if self.player_starts.len() > Self::MAX_PLAYER_STARTS {
            return Err(MetadataError::TooManyPlayerStarts(self.player_starts.len()));
        }
        if self.submap_settings.len() > Self::MAX_SUBMAP_SETTINGS {
            return Err(MetadataError::TooManySubmapSettings(
                self.submap_settings.len(),
            ));
        }
        let mut levels = HashSet::with_capacity(self.level_names.len());
        for name in &self.level_names {
            if !levels.insert(name.level) {
                return Err(MetadataError::DuplicateLevel(name.level));
            }
        }
        let mut players = HashSet::with_capacity(self.player_starts.len());
        for start in &self.player_starts {
            if !players.insert(start.player) {
                return Err(MetadataError::DuplicatePlayer(start.player));
            }
        }
        let mut submaps = HashSet::with_capacity(self.submap_settings.len());
        for settings in &self.submap_settings {
            if !submaps.insert(settings.submap) {
                return Err(MetadataError::DuplicateSubmap(settings.submap));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_keys_must_be_unique() {
        let mut metadata = OverworldMetadata {
            level_names: vec![
                OverworldLevelName {
                    level: 0x105,
                    tiles: [0; OverworldLevelName::TILE_COUNT],
                    raw_flags: 0,
                },
                OverworldLevelName {
                    level: 0x105,
                    tiles: [1; OverworldLevelName::TILE_COUNT],
                    raw_flags: 1,
                },
            ],
            ..OverworldMetadata::default()
        };
        assert_eq!(
            metadata.validate(),
            Err(MetadataError::DuplicateLevel(0x105))
        );
        metadata.level_names.truncate(1);
        metadata.player_starts = vec![
            PlayerStart {
                player: 0,
                x: 1,
                y: 2,
                submap: Submap::Main,
                raw_flags: 0,
            },
            PlayerStart {
                player: 0,
                x: 3,
                y: 4,
                submap: Submap::StarWorld,
                raw_flags: 0,
            },
        ];
        assert_eq!(metadata.validate(), Err(MetadataError::DuplicatePlayer(0)));
    }
}
