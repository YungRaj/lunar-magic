use crate::portable_value_history::PortableValueHistory;
use lm_level::{Map16Tile, NativeMap16SidecarError, S16OvSidecar};
use lm_overworld::{
    NativeOverworldSpriteAppearance, NativeOverworldSpriteRange, NativeOverworldSpriteSidecar,
    NativeOverworldSpriteSidecarError, NativeOverworldSpriteTooltip,
};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldAppearanceValue {
    pub definitions: NativeOverworldSpriteSidecar,
    pub sprite_map16: S16OvSidecar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldAppearanceEdit {
    SetTooltip {
        sprite_id: u16,
        value: Option<NativeOverworldSpriteTooltip>,
    },
    SetAppearance {
        sprite_id: u16,
        value: Option<NativeOverworldSpriteAppearance>,
    },
    ReplaceGraphicsRanges(Vec<NativeOverworldSpriteRange>),
    ReplacePaletteRanges(Vec<NativeOverworldSpriteRange>),
    SetCustomMap16 {
        native_tile: u16,
        value: Map16Tile,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOverworldAppearanceSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub definitions_path: PathBuf,
    pub sprite_map16_path: PathBuf,
    pub definitions: Vec<u8>,
    pub sprite_map16: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: NativeOverworldAppearanceValue,
}

#[derive(Clone, Debug)]
pub struct NativeOverworldAppearanceController {
    definitions_path: PathBuf,
    sprite_map16_path: PathBuf,
    value: NativeOverworldAppearanceValue,
    saved: NativeOverworldAppearanceValue,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<NativeOverworldAppearanceValue>,
}

impl NativeOverworldAppearanceController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one native `.sscov`/`.s16ov` pair without converting away native-only fields.
    pub fn decode(
        definitions_path: PathBuf,
        sprite_map16_path: PathBuf,
        definitions: &[u8],
        sprite_map16: &[u8],
    ) -> Result<Self, NativeOverworldAppearanceControllerError> {
        if definitions_path == sprite_map16_path {
            return Err(NativeOverworldAppearanceControllerError::AliasedPaths);
        }
        let value = NativeOverworldAppearanceValue {
            definitions: NativeOverworldSpriteSidecar::decode(definitions)
                .map_err(NativeOverworldAppearanceControllerError::Definitions)?,
            sprite_map16: S16OvSidecar::decode(sprite_map16)
                .map_err(NativeOverworldAppearanceControllerError::Map16)?,
        };
        let value = canonical_reopen(&value)?;
        Ok(Self {
            definitions_path,
            sprite_map16_path,
            saved: value.clone(),
            value,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &NativeOverworldAppearanceValue {
        &self.value
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.value != self.saved
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    #[must_use]
    pub const fn save_pending(&self) -> bool {
        self.pending_save.is_some()
    }

    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[NativeOverworldAppearanceEdit],
    ) -> Result<(), NativeOverworldAppearanceControllerError> {
        self.require_revision(expected_revision)?;
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged, edit).map_err(|error| {
                NativeOverworldAppearanceControllerError::Edit { command, error }
            })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(NativeOverworldAppearanceControllerError::RevisionOverflow)?;
        let reopened = canonical_reopen(&staged)?;
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeOverworldAppearanceControllerError> {
        self.navigate_history(expected_revision, true)
    }

    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeOverworldAppearanceControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, NativeOverworldAppearanceControllerError> {
        self.require_revision(expected_revision)?;
        if if undo {
            !self.can_undo()
        } else {
            !self.can_redo()
        } {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(NativeOverworldAppearanceControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    pub fn begin_save(
        &mut self,
    ) -> Result<NativeOverworldAppearanceSaveSnapshot, NativeOverworldAppearanceControllerError>
    {
        if self.pending_save.is_some() {
            return Err(NativeOverworldAppearanceControllerError::SavePending);
        }
        let (definitions, sprite_map16) = canonical_bytes(&self.value)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(NativeOverworldAppearanceControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(NativeOverworldAppearanceSaveSnapshot {
            request_id,
            revision: self.revision,
            definitions_path: self.definitions_path.clone(),
            sprite_map16_path: self.sprite_map16_path.clone(),
            definitions,
            sprite_map16,
        })
    }

    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeOverworldAppearanceControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(NativeOverworldAppearanceControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(NativeOverworldAppearanceControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeOverworldAppearanceControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(NativeOverworldAppearanceControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(NativeOverworldAppearanceControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }

    fn require_revision(
        &self,
        expected: u64,
    ) -> Result<(), NativeOverworldAppearanceControllerError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(NativeOverworldAppearanceControllerError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }
}

fn apply_edit(
    value: &mut NativeOverworldAppearanceValue,
    edit: &NativeOverworldAppearanceEdit,
) -> Result<(), NativeOverworldAppearanceEditError> {
    match edit {
        NativeOverworldAppearanceEdit::SetTooltip {
            sprite_id,
            value: new,
        } => {
            validate_sprite_id(*sprite_id)?;
            if let Some(new) = new {
                value.definitions.tooltips.insert(*sprite_id, new.clone());
            } else {
                value.definitions.tooltips.remove(sprite_id);
            }
        }
        NativeOverworldAppearanceEdit::SetAppearance {
            sprite_id,
            value: new,
        } => {
            validate_sprite_id(*sprite_id)?;
            if let Some(new) = new {
                value
                    .definitions
                    .appearances
                    .insert(*sprite_id, new.clone());
            } else {
                value.definitions.appearances.remove(sprite_id);
            }
        }
        NativeOverworldAppearanceEdit::ReplaceGraphicsRanges(ranges) => {
            value.definitions.graphics_ranges = ranges.clone();
        }
        NativeOverworldAppearanceEdit::ReplacePaletteRanges(ranges) => {
            value.definitions.palette_ranges = ranges.clone();
        }
        NativeOverworldAppearanceEdit::SetCustomMap16 {
            native_tile,
            value: tile,
        } => {
            if !(0x400..0xc00).contains(native_tile) {
                return Err(NativeOverworldAppearanceEditError::Map16TileOutOfRange(
                    *native_tile,
                ));
            }
            if tile.acts_like != 0 {
                return Err(NativeOverworldAppearanceEditError::Map16ActsLike(
                    tile.acts_like,
                ));
            }
            let local = usize::from(*native_tile) - S16OvSidecar::FIRST_NATIVE_TILE;
            for (entry, bytes) in tile.encode_graphics().chunks_exact(4).enumerate() {
                value
                    .sprite_map16
                    .set_entry(
                        local * 2 + entry,
                        u32::from_le_bytes(bytes.try_into().unwrap()),
                    )
                    .map_err(NativeOverworldAppearanceEditError::Map16)?;
            }
        }
    }
    Ok(())
}

const fn validate_sprite_id(sprite_id: u16) -> Result<(), NativeOverworldAppearanceEditError> {
    if sprite_id <= 0x17f {
        Ok(())
    } else {
        Err(NativeOverworldAppearanceEditError::SpriteIdOutOfRange(
            sprite_id,
        ))
    }
}

fn canonical_bytes(
    value: &NativeOverworldAppearanceValue,
) -> Result<(Vec<u8>, Vec<u8>), NativeOverworldAppearanceControllerError> {
    Ok((
        value
            .definitions
            .encode()
            .map_err(NativeOverworldAppearanceControllerError::Definitions)?,
        value.sprite_map16.encode(),
    ))
}

fn canonical_reopen(
    value: &NativeOverworldAppearanceValue,
) -> Result<NativeOverworldAppearanceValue, NativeOverworldAppearanceControllerError> {
    let (definitions, sprite_map16) = canonical_bytes(value)?;
    let reopened = NativeOverworldAppearanceValue {
        definitions: NativeOverworldSpriteSidecar::decode(&definitions)
            .map_err(NativeOverworldAppearanceControllerError::Definitions)?,
        sprite_map16: S16OvSidecar::decode(&sprite_map16)
            .map_err(NativeOverworldAppearanceControllerError::Map16)?,
    };
    if reopened == *value {
        Ok(reopened)
    } else {
        Err(NativeOverworldAppearanceControllerError::NonCanonicalEncoding)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldAppearanceEditError {
    SpriteIdOutOfRange(u16),
    Map16TileOutOfRange(u16),
    Map16ActsLike(u16),
    Map16(NativeMap16SidecarError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeOverworldAppearanceControllerError {
    AliasedPaths,
    Definitions(NativeOverworldSpriteSidecarError),
    Map16(NativeMap16SidecarError),
    Edit {
        command: usize,
        error: NativeOverworldAppearanceEditError,
    },
    NonCanonicalEncoding,
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for NativeOverworldAppearanceControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native overworld appearance controller failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeOverworldAppearanceControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::Subtile;
    use lm_overworld::{NativeOverworldSpriteDisplay, NativeOverworldSpriteMap16Part};

    fn controller() -> NativeOverworldAppearanceController {
        NativeOverworldAppearanceController::decode(
            "sprites.sscov".into(),
            "sprites.s16ov".into(),
            b"01\t3\t-2,4,8400\n01\t0\tOriginal\\nTooltip\n10000\t0\t400-4FF,20\n",
            &[1, 0, 2, 0, 3, 0, 4, 0],
        )
        .unwrap()
    }

    fn tile(base: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(base),
            top_right: Subtile(base + 1),
            bottom_left: Subtile(base + 2),
            bottom_right: Subtile(base + 3),
            acts_like: 0,
        }
    }

    #[test]
    fn mixed_native_only_edits_are_atomic_revisioned_and_reopen_exactly() {
        let mut controller = controller();
        controller
            .apply_edits(
                0,
                &[
                    NativeOverworldAppearanceEdit::SetTooltip {
                        sprite_id: 0x101,
                        value: Some(NativeOverworldSpriteTooltip {
                            disable_original_position_text: true,
                            text: "Custom".into(),
                        }),
                    },
                    NativeOverworldAppearanceEdit::SetAppearance {
                        sprite_id: 0x101,
                        value: Some(NativeOverworldSpriteAppearance {
                            shadow: true,
                            display: NativeOverworldSpriteDisplay::Label {
                                x: -7,
                                y: 9,
                                text: "Native Label".into(),
                            },
                        }),
                    },
                    NativeOverworldAppearanceEdit::SetCustomMap16 {
                        native_tile: 0xbff,
                        value: tile(0x20),
                    },
                ],
            )
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert!(controller.is_modified());
        assert_eq!(
            controller.value().sprite_map16.native_tile(0xbff),
            Some(tile(0x20))
        );
        assert_eq!(
            controller.value().sprite_map16.loaded_len(),
            S16OvSidecar::CAPACITY
        );
        let snapshot = controller.begin_save().unwrap();
        let reopened = NativeOverworldAppearanceController::decode(
            snapshot.definitions_path.clone(),
            snapshot.sprite_map16_path.clone(),
            &snapshot.definitions,
            &snapshot.sprite_map16,
        )
        .unwrap();
        assert_eq!(reopened.value(), controller.value());
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(!controller.is_modified());
    }

    #[test]
    fn late_invalid_map16_edit_preserves_both_sidecars_and_history() {
        let mut controller = controller();
        let before = controller.value().clone();
        assert!(matches!(
            controller.apply_edits(
                0,
                &[
                    NativeOverworldAppearanceEdit::SetAppearance {
                        sprite_id: 2,
                        value: Some(NativeOverworldSpriteAppearance {
                            shadow: false,
                            display: NativeOverworldSpriteDisplay::Tiles(vec![
                                NativeOverworldSpriteMap16Part {
                                    x: 0,
                                    y: 0,
                                    tile: 1,
                                    translucent: true,
                                },
                            ]),
                        }),
                    },
                    NativeOverworldAppearanceEdit::SetCustomMap16 {
                        native_tile: 0x3ff,
                        value: tile(1),
                    },
                ],
            ),
            Err(NativeOverworldAppearanceControllerError::Edit { command: 1, .. })
        ));
        assert_eq!(controller.value(), &before);
        assert_eq!(controller.revision(), 0);
        assert!(!controller.can_undo());
    }

    #[test]
    fn invalid_range_replacement_is_canonically_rejected_without_history() {
        let mut controller = controller();
        let before = controller.value().clone();
        assert!(matches!(
            controller.apply_edits(
                0,
                &[NativeOverworldAppearanceEdit::ReplacePaletteRanges(vec![
                    NativeOverworldSpriteRange {
                        kind: 0xffff,
                        first_tile: 0x900,
                        last_tile: 0x800,
                        base: 0xffff,
                    },
                ])],
            ),
            Err(NativeOverworldAppearanceControllerError::Definitions(
                NativeOverworldSpriteSidecarError::RangeOutOfBounds { .. }
            ))
        ));
        assert_eq!(controller.value(), &before);
        assert_eq!(controller.revision(), 0);
        assert!(!controller.can_undo());
    }

    #[test]
    fn paired_history_and_save_tokens_retain_native_semantics() {
        let mut controller = controller();
        let original = controller.value().clone();
        controller
            .apply_edits(
                0,
                &[NativeOverworldAppearanceEdit::SetAppearance {
                    sprite_id: 1,
                    value: None,
                }],
            )
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        assert!(matches!(
            controller.acknowledge_save(snapshot.request_id + 1),
            Err(NativeOverworldAppearanceControllerError::StaleSave { .. })
        ));
        assert!(controller.save_pending());
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.value(), &original);
        assert!(controller.redo(2).unwrap());
        assert!(!controller.value().definitions.appearances.contains_key(&1));
    }

    #[test]
    fn aliases_stale_revisions_and_unrepresentable_fields_are_rejected() {
        assert!(matches!(
            NativeOverworldAppearanceController::decode("same".into(), "same".into(), b"", b""),
            Err(NativeOverworldAppearanceControllerError::AliasedPaths)
        ));
        let mut controller = controller();
        assert!(matches!(
            controller.apply_edits(1, &[]),
            Err(NativeOverworldAppearanceControllerError::StaleRevision { .. })
        ));
        assert!(matches!(
            controller.apply_edits(
                0,
                &[NativeOverworldAppearanceEdit::SetCustomMap16 {
                    native_tile: 0x400,
                    value: Map16Tile {
                        acts_like: 1,
                        ..tile(0)
                    },
                }],
            ),
            Err(NativeOverworldAppearanceControllerError::Edit {
                error: NativeOverworldAppearanceEditError::Map16ActsLike(1),
                ..
            })
        ));
    }
}
