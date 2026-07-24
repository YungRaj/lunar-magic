use crate::EditorMode;
use crate::graphics_edit_batch::apply_graphics_edit_batch;
use lm_graphics::{
    GraphicsEditError, GraphicsFile4bpp, GraphicsOwnership, GraphicsTileChange, IndexedTile,
};
use lm_project::{GraphicsIoError, GraphicsRomLayout, TransactionError};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError};
use std::fmt;

mod commit;
mod load;

/// One ordered ownership-aware mutation in a decoded graphics file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsControllerEdit {
    ApplyChanges(Vec<GraphicsTileChange>),
    ReplaceRange {
        start: usize,
        tiles: Vec<IndexedTile>,
    },
}

#[derive(Debug)]
pub enum GraphicsControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    Rom(RomError),
    Io(GraphicsIoError),
    Edit {
        command: usize,
        error: GraphicsEditError,
    },
    Mutation(TransactionError),
}

impl fmt::Display for GraphicsControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "graphics controller failed: {self:?}")
    }
}

impl std::error::Error for GraphicsControllerError {}

/// One native compressed graphics file decoded from an immutable application snapshot.
#[derive(Clone, Debug)]
pub struct GraphicsController {
    revision: u64,
    file_number: usize,
    layout: GraphicsRomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    baseline: GraphicsFile4bpp,
    graphics: GraphicsFile4bpp,
    ownership: GraphicsOwnership,
    previous_block: Option<RatsBlock>,
}

impl GraphicsController {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn graphics(&self) -> &GraphicsFile4bpp {
        &self.graphics
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.graphics != self.baseline
    }

    #[must_use]
    pub const fn ownership(&self) -> &GraphicsOwnership {
        &self.ownership
    }

    /// Applies an ordered graphics batch to a staged clone under the immutable ownership map.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsControllerError::Edit`] with the failing command; prior commands in this
    /// call are rolled back with the staged file.
    pub fn apply_edits(
        &mut self,
        edits: &[GraphicsControllerEdit],
    ) -> Result<(), GraphicsControllerError> {
        apply_graphics_edit_batch(&mut self.graphics, &self.ownership, edits)
            .map_err(|(command, error)| GraphicsControllerError::Edit { command, error })
    }
}

#[cfg(test)]
mod tests;
