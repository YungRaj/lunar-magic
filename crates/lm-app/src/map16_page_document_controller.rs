use crate::portable_value_history::PortableValueHistory;
use lm_level::{Map16Page, Map16PageFile, Map16PageFileError, Map16Quadrant, Map16Tile, Subtile};
use std::fmt;
use std::path::PathBuf;

/// One page-scoped edit. Acts Like graph validation deliberately belongs to complete sets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16PageDocumentEdit {
    ReplaceTile {
        tile: usize,
        value: Map16Tile,
    },
    SetSubtile {
        tile: usize,
        quadrant: Map16Quadrant,
        value: Subtile,
    },
    SetActsLike {
        tile: usize,
        value: u16,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16PageDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: Map16PageFile,
}

/// Revisioned toolkit-neutral owner for one standalone `LM16PAGE` artifact.
#[derive(Clone, Debug)]
pub struct Map16PageDocumentController {
    path: PathBuf,
    value: Map16PageFile,
    saved: Map16PageFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<Map16PageFile>,
}

impl Map16PageDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact standalone page.
    ///
    /// # Errors
    ///
    /// Returns a file error for malformed framing or page shape.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, Map16PageDocumentControllerError> {
        let value = Map16PageFile::decode(bytes).map_err(Map16PageDocumentControllerError::File)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &Map16PageFile {
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
    pub const fn save_pending(&self) -> bool {
        self.pending_save.is_some()
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies an ordered page-local batch atomically and canonically reopens the result.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, out-of-range tiles, invalid public shapes, and overflow.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[Map16PageDocumentEdit],
    ) -> Result<(), Map16PageDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Map16PageDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged.page, edit).map_err(|tile| {
                Map16PageDocumentControllerError::TileOutOfRange { command, tile }
            })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Map16PageDocumentControllerError::RevisionOverflow)?;
        let reopened = canonical_reopen(&staged)?;
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical page as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, Map16PageDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical page as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, Map16PageDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, Map16PageDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Map16PageDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if if undo {
            !self.history.can_undo()
        } else {
            !self.history.can_redo()
        } {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Map16PageDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid public state, or request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<Map16PageDocumentSaveSnapshot, Map16PageDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(Map16PageDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(Map16PageDocumentControllerError::File)?;
        if Map16PageFile::decode(&bytes).map_err(Map16PageDocumentControllerError::File)?
            != self.value
        {
            return Err(Map16PageDocumentControllerError::NonCanonicalEncoding);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(Map16PageDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(Map16PageDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges only the exact immutable snapshot written by a frontend.
    ///
    /// # Errors
    ///
    /// Missing or mismatched acknowledgements retain a pending snapshot.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), Map16PageDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(Map16PageDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(Map16PageDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases one failed persistence request without moving the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects missing or mismatched requests.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), Map16PageDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(Map16PageDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(Map16PageDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

fn apply_edit(page: &mut Map16Page, edit: &Map16PageDocumentEdit) -> Result<(), usize> {
    let tile_index = match edit {
        Map16PageDocumentEdit::ReplaceTile { tile, .. }
        | Map16PageDocumentEdit::SetSubtile { tile, .. }
        | Map16PageDocumentEdit::SetActsLike { tile, .. } => *tile,
    };
    let tile = page.tiles.get_mut(tile_index).ok_or(tile_index)?;
    match edit {
        Map16PageDocumentEdit::ReplaceTile { value, .. } => *tile = *value,
        Map16PageDocumentEdit::SetSubtile {
            quadrant, value, ..
        } => match quadrant {
            Map16Quadrant::TopLeft => tile.top_left = *value,
            Map16Quadrant::TopRight => tile.top_right = *value,
            Map16Quadrant::BottomLeft => tile.bottom_left = *value,
            Map16Quadrant::BottomRight => tile.bottom_right = *value,
        },
        Map16PageDocumentEdit::SetActsLike { value, .. } => tile.acts_like = *value,
    }
    Ok(())
}

fn canonical_reopen(
    value: &Map16PageFile,
) -> Result<Map16PageFile, Map16PageDocumentControllerError> {
    let bytes = value
        .encode()
        .map_err(Map16PageDocumentControllerError::File)?;
    let reopened = Map16PageFile::decode(&bytes).map_err(Map16PageDocumentControllerError::File)?;
    if reopened != *value {
        return Err(Map16PageDocumentControllerError::NonCanonicalEncoding);
    }
    Ok(reopened)
}

#[derive(Debug)]
pub enum Map16PageDocumentControllerError {
    File(Map16PageFileError),
    TileOutOfRange { command: usize, tile: usize },
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for Map16PageDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 page document controller failed: {self:?}")
    }
}

impl std::error::Error for Map16PageDocumentControllerError {}

#[cfg(test)]
#[path = "map16_page_document_controller_tests.rs"]
mod tests;
