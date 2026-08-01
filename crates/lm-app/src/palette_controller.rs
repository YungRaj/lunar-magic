use crate::EditorMode;
use crate::palette_edit_batch::apply_palette_edit_batch;
use lm_graphics::{
    Bgr555, Palette, PaletteBatchEditError, PaletteChange, PaletteMaskFile, PaletteOwnership,
    RawPaletteFileError, RawSnesPaletteFile, apply_raw_palette_import,
};
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
    RawImport(RawPaletteFileError),
    SupportedPaletteShape {
        installed: usize,
        supported: usize,
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

    /// Applies Lunar Magic's exact raw-palette import semantics through immutable ownership.
    ///
    /// Only colors whose final value differs are submitted to the ownership-aware edit batch, so
    /// selected protected entries may remain byte-exact while any attempted protected mutation
    /// rejects the complete import atomically.
    ///
    /// # Errors
    ///
    /// Returns a raw-format shape error or the first ownership failure without changing staged
    /// palette state.
    pub fn import_raw_palette(
        &mut self,
        source: &RawSnesPaletteFile,
        mask: &PaletteMaskFile,
    ) -> Result<(), PaletteControllerError> {
        let mut imported = self.palette.clone();
        apply_raw_palette_import(&mut imported, source, mask)
            .map_err(PaletteControllerError::RawImport)?;
        self.apply_imported_palette(imported)
    }

    /// Imports the recovered 256-color TPL/RGB order into the installed 257-word payload.
    ///
    /// Installed word 1 is the separately owned backdrop and is retained. Supported-file word 0
    /// maps to installed word 0; words 1–255 map to installed words 2–256. Lunar Magic clears the
    /// first color of rows 1–15 after this import while leaving supported-file word 0 intact.
    ///
    /// # Errors
    ///
    /// Returns a shape or ownership error atomically.
    pub fn import_supported_palette(
        &mut self,
        source: &Palette,
    ) -> Result<(), PaletteControllerError> {
        self.import_supported_palette_with_mask(source, &PaletteMaskFile::all_selected())
    }

    /// Imports selected TPL/RGB colors using their natural supported-file `.palmask` indices.
    ///
    /// Supported index 0 maps to installed word 0 and indices 1–255 map to installed words
    /// 2–256. Installed backdrop word 1 is never selected by this file order. Selected row-zero
    /// entries 16, 32, …, 240 are cleared after transfer, while unselected entries are retained.
    ///
    /// # Errors
    ///
    /// Returns a shape or ownership error atomically.
    pub fn import_supported_palette_with_mask(
        &mut self,
        source: &Palette,
        mask: &PaletteMaskFile,
    ) -> Result<(), PaletteControllerError> {
        if self.palette.colors.len() != RawSnesPaletteFile::COLOR_COUNT
            || source.colors.len() != 256
        {
            return Err(PaletteControllerError::SupportedPaletteShape {
                installed: self.palette.colors.len(),
                supported: source.colors.len(),
            });
        }
        let mut imported = self.palette.clone();
        for (supported, color) in source.colors.iter().copied().enumerate() {
            if mask.is_selected(supported).unwrap_or(false) {
                let installed = if supported == 0 { 0 } else { supported + 1 };
                imported.colors[installed] = color;
            }
        }
        for row in 1..16 {
            let supported = row * Palette::COLORS_PER_ROW;
            if mask.is_selected(supported).unwrap_or(false) {
                imported.colors[supported + 1] = Bgr555(0);
            }
        }
        self.apply_imported_palette(imported)
    }

    /// Exports the recovered installed payload in natural 256-color TPL/RGB order.
    ///
    /// # Errors
    ///
    /// Returns a shape error unless the installed payload contains exactly 257 words.
    pub fn supported_palette(&self) -> Result<Palette, PaletteControllerError> {
        if self.palette.colors.len() != RawSnesPaletteFile::COLOR_COUNT {
            return Err(PaletteControllerError::SupportedPaletteShape {
                installed: self.palette.colors.len(),
                supported: 256,
            });
        }
        let mut colors = Vec::with_capacity(256);
        colors.push(self.palette.colors[0]);
        colors.extend_from_slice(&self.palette.colors[2..]);
        Ok(Palette { colors })
    }

    fn apply_imported_palette(&mut self, imported: Palette) -> Result<(), PaletteControllerError> {
        let changes = self
            .palette
            .colors
            .iter()
            .zip(imported.colors)
            .enumerate()
            .filter_map(|(index, (current, color))| {
                (*current != color).then_some(PaletteChange { index, color })
            })
            .collect();
        self.apply_edits(&[PaletteControllerEdit::ApplyChanges(changes)])
    }
}

#[cfg(test)]
mod tests;
