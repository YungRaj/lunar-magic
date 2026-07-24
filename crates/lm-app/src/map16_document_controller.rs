use crate::portable_value_history::PortableValueHistory;
use lm_level::{
    Map16Address, Map16EditError, Map16Page, Map16Quadrant, Map16SetFile, Map16SetFileError,
    Map16Tile, Subtile,
};
use std::fmt;
use std::path::PathBuf;

mod edit;
mod save;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16DocumentEdit {
    ReplaceTiles {
        replacements: Vec<(Map16Address, Map16Tile)>,
        resolution_limit: usize,
    },
    SetSubtile {
        address: Map16Address,
        quadrant: Map16Quadrant,
        subtile: Subtile,
        resolution_limit: usize,
    },
    SetActsLike {
        address: Map16Address,
        acts_like: u16,
        resolution_limit: usize,
    },
    AppendPage {
        page: Map16Page,
        resolution_limit: usize,
    },
    RemoveLastPage {
        resolution_limit: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16DocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: Map16SetFile,
}

/// Revisioned toolkit-neutral owner of one complete portable `LM16SET1` document.
#[derive(Clone, Debug)]
pub struct Map16DocumentController {
    path: PathBuf,
    value: Map16SetFile,
    saved: Map16SetFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<Map16SetFile>,
}

impl Map16DocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded complete Map16 artifact.
    ///
    /// # Errors
    ///
    /// Returns a file error for malformed framing, shapes, graphs, or excessive page counts.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, Map16DocumentControllerError> {
        let value = Map16SetFile::decode(bytes).map_err(Map16DocumentControllerError::File)?;
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
    pub const fn value(&self) -> &Map16SetFile {
        &self.value
    }

    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
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

    /// Restores the previous canonical Map16 set as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, Map16DocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical Map16 set as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, Map16DocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, Map16DocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Map16DocumentControllerError::StaleRevision {
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
            .ok_or(Map16DocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }
}

#[derive(Debug)]
pub enum Map16DocumentControllerError {
    File(Map16SetFileError),
    Edit {
        command: usize,
        error: Map16EditError,
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

impl fmt::Display for Map16DocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "complete Map16 document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for Map16DocumentControllerError {}

#[cfg(test)]
mod tests;
