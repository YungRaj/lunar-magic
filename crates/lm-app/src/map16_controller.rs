use crate::EditorMode;
use lm_level::{Map16Address, Map16EditError, Map16Quadrant, Map16Set, Map16Tile, Subtile};
use lm_project::{Map16RomLayout, Map16SetIoError, TransactionError};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError};
use std::fmt;

mod commit;
mod load;

/// One ordered mutation in a snapshot-bound complete Map16 workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16ControllerEdit {
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
}

#[derive(Debug)]
pub enum Map16ControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    Rom(RomError),
    Io(Map16SetIoError),
    Edit {
        command: usize,
        error: Map16EditError,
    },
    Mutation(TransactionError),
}

impl fmt::Display for Map16ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 controller failed: {self:?}")
    }
}

impl std::error::Error for Map16ControllerError {}

/// Complete native Map16 planes decoded from one immutable application snapshot.
#[derive(Clone, Debug)]
pub struct Map16Controller {
    revision: u64,
    layout: Map16RomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    baseline: Map16Set,
    set: Map16Set,
    previous_graphics: Vec<Option<RatsBlock>>,
    previous_acts_like: Vec<Option<RatsBlock>>,
}

impl Map16Controller {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn set(&self) -> &Map16Set {
        &self.set
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.set != self.baseline
    }

    /// Applies a mixed Map16 edit batch to a staged complete workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Map16ControllerError::Edit`] with the failing command. Any failure preserves the
    /// original controller state, including changes from earlier commands in this call.
    pub fn apply_edits(
        &mut self,
        edits: &[Map16ControllerEdit],
    ) -> Result<(), Map16ControllerError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.set.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = match edit {
                Map16ControllerEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                } => staged.replace_tiles(replacements, *resolution_limit),
                Map16ControllerEdit::SetSubtile {
                    address,
                    quadrant,
                    subtile,
                    resolution_limit,
                } => staged.set_subtile(*address, *quadrant, *subtile, *resolution_limit),
                Map16ControllerEdit::SetActsLike {
                    address,
                    acts_like,
                    resolution_limit,
                } => staged.set_acts_like(*address, *acts_like, *resolution_limit),
            };
            result.map_err(|error| Map16ControllerError::Edit { command, error })?;
        }
        self.set = staged;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
