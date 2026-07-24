use crate::EditorMode;
use crate::palette_edit_batch::apply_palette_edit_batch;
use lm_graphics::{Bgr555, Palette, PaletteBatchEditError, PaletteChange, PaletteOwnership};
use lm_project::{PaletteIoError, PaletteRomLayout, TransactionError};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError};
use std::fmt;

mod commit;
mod load;

/// One ordered ownership-aware mutation in an exact SNES palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteControllerEdit {
    ApplyChanges(Vec<PaletteChange>),
    ReplaceRange { start: usize, colors: Vec<Bgr555> },
}

#[derive(Debug)]
pub enum PaletteControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    Rom(RomError),
    Io(PaletteIoError),
    Edit {
        command: usize,
        error: PaletteBatchEditError,
    },
    Mutation(TransactionError),
}

impl fmt::Display for PaletteControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "palette controller failed: {self:?}")
    }
}

impl std::error::Error for PaletteControllerError {}

/// One exact native palette decoded from an immutable application snapshot.
#[derive(Clone, Debug)]
pub struct PaletteController {
    revision: u64,
    palette_number: usize,
    layout: PaletteRomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    baseline: Palette,
    palette: Palette,
    ownership: PaletteOwnership,
    previous_block: Option<RatsBlock>,
}

impl PaletteController {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.palette != self.baseline
    }

    #[must_use]
    pub const fn ownership(&self) -> &PaletteOwnership {
        &self.ownership
    }

    /// Applies ordered exact-word edits to a staged palette under its immutable ownership map.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteControllerError::Edit`] with the failing command. A failure rolls back all
    /// preceding commands in this call and preserves every original raw color word.
    pub fn apply_edits(
        &mut self,
        edits: &[PaletteControllerEdit],
    ) -> Result<(), PaletteControllerError> {
        apply_palette_edit_batch(&mut self.palette, &self.ownership, edits)
            .map_err(|(command, error)| PaletteControllerError::Edit { command, error })
    }
}

#[cfg(test)]
mod tests;
