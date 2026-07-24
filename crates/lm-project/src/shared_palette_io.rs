//! Exact fixed-table I/O for Lunar Magic's shared SMW palette backends.

use crate::{Project, RomWrite, TransactionError};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile, SmwPaletteFileError};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedPaletteRomLayout {
    pub mapper: Mapper,
    pub table_offset: usize,
    pub expanded_marker_offset: usize,
    pub expanded_marker: u8,
}

#[derive(Debug)]
pub enum SharedPaletteIoError {
    MapperImageShape,
    Bounds(RomError),
    Codec(SmwPaletteFileError),
    BackendMismatch {
        installed: SmwPaletteBackend,
        supplied: SmwPaletteBackend,
    },
    OffsetOverflow,
    ChecksumOverlap,
    Transaction(TransactionError),
}

impl std::fmt::Display for SharedPaletteIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "shared palette I/O failed: {self:?}")
    }
}

impl std::error::Error for SharedPaletteIoError {}

impl From<RomError> for SharedPaletteIoError {
    fn from(value: RomError) -> Self {
        Self::Bounds(value)
    }
}

impl From<SmwPaletteFileError> for SharedPaletteIoError {
    fn from(value: SmwPaletteFileError) -> Self {
        Self::Codec(value)
    }
}

impl From<TransactionError> for SharedPaletteIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads the marker-selected shared palette and converts ROM ordering to `.smwpal` ordering.
    ///
    /// # Errors
    ///
    /// Rejects mapper/image disagreement, bounds failures, or malformed palette data.
    pub fn load_shared_palette(
        &self,
        layout: SharedPaletteRomLayout,
    ) -> Result<SmwPaletteFile, SharedPaletteIoError> {
        validate_mapper(self, layout.mapper)?;
        let expanded =
            self.rom.read(layout.expanded_marker_offset, 1)?[0] == layout.expanded_marker;
        if expanded {
            let bytes = self
                .rom
                .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)?;
            SmwPaletteFile::expanded(
                bytes[SmwPaletteFile::EXPANDED_AUXILIARY_LEN..].to_vec(),
                bytes[..SmwPaletteFile::EXPANDED_AUXILIARY_LEN].to_vec(),
            )
            .map_err(Into::into)
        } else {
            SmwPaletteFile::legacy(
                self.rom
                    .read(layout.table_offset, SmwPaletteFile::LEGACY_PALETTE_LEN)?
                    .to_vec(),
            )
            .map_err(Into::into)
        }
    }

    /// Replaces a shared palette in its already-installed backend and repairs the checksum.
    ///
    /// This refuses backend conversion because the expanded backend requires Lunar Magic's two
    /// runtime hooks and `$600`-byte runtime, not merely a longer table write.
    ///
    /// # Errors
    ///
    /// Rejects backend disagreement, mapper/bounds/checksum overlap, or transaction failures
    /// without changing the project.
    pub fn save_shared_palette(
        &mut self,
        palette: &SmwPaletteFile,
        layout: SharedPaletteRomLayout,
        checksum_field: usize,
    ) -> Result<bool, SharedPaletteIoError> {
        let installed = self.load_shared_palette(layout)?.backend();
        if installed != palette.backend() {
            return Err(SharedPaletteIoError::BackendMismatch {
                installed,
                supplied: palette.backend(),
            });
        }
        let bytes = match palette.backend() {
            SmwPaletteBackend::Legacy => palette.palette_bytes().to_vec(),
            SmwPaletteBackend::Expanded => {
                let mut bytes = Vec::with_capacity(SmwPaletteFile::EXPANDED_FILE_LEN);
                bytes.extend_from_slice(palette.auxiliary_bytes());
                bytes.extend_from_slice(palette.palette_bytes());
                bytes
            }
        };
        let table_end = layout
            .table_offset
            .checked_add(bytes.len())
            .ok_or(SharedPaletteIoError::OffsetOverflow)?;
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(SharedPaletteIoError::OffsetOverflow)?;
        if layout.table_offset < checksum_end && checksum_field < table_end {
            return Err(SharedPaletteIoError::ChecksumOverlap);
        }
        let table_write = RomWrite {
            offset: layout.table_offset,
            bytes,
        };
        if !self.writes_would_change(std::slice::from_ref(&table_write))? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        staged.write(table_write.offset, &table_write.bytes)?;
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        Ok(self.apply_writes(
            "save shared SMW palettes",
            &[
                table_write,
                RomWrite {
                    offset: checksum_field,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )?)
    }
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), SharedPaletteIoError> {
    if mapper_supports_image_len(mapper, project.rom.logical_len()) {
        Ok(())
    } else {
        Err(SharedPaletteIoError::MapperImageShape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    const TABLE: usize = 0x100;
    const MARKER: usize = 0x20;
    const CHECKSUM: usize = 0x7fdc;

    fn layout() -> SharedPaletteRomLayout {
        SharedPaletteRomLayout {
            mapper: Mapper::LoRom,
            table_offset: TABLE,
            expanded_marker_offset: MARKER,
            expanded_marker: 0xc2,
        }
    }

    #[test]
    fn expanded_rom_and_file_orderings_are_inverse() {
        let mut bytes = vec![0xff; 0x8000];
        bytes[MARKER] = 0xc2;
        for (index, byte) in bytes[TABLE..TABLE + 0x810].iter_mut().enumerate() {
            *byte = index.to_le_bytes()[0];
        }
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let palette = project.load_shared_palette(layout()).unwrap();
        assert_eq!(
            palette.auxiliary_bytes(),
            &project.rom.logical_bytes()[TABLE..TABLE + 0x10]
        );
        assert_eq!(
            palette.palette_bytes(),
            &project.rom.logical_bytes()[TABLE + 0x10..TABLE + 0x810]
        );
    }

    #[test]
    fn save_reopen_checksum_and_undo_are_atomic() {
        let mut bytes = vec![0xff; 0x8000];
        bytes[TABLE..TABLE + 0x7e2].fill(0);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let before = project.save_snapshot();
        let palette = SmwPaletteFile::legacy(vec![0x55; 0x7e2]).unwrap();
        assert!(
            project
                .save_shared_palette(&palette, layout(), CHECKSUM)
                .unwrap()
        );
        assert_eq!(project.load_shared_palette(layout()).unwrap(), palette);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), before);
    }

    #[test]
    fn backend_conversion_is_rejected_without_mutation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.save_snapshot();
        let expanded = SmwPaletteFile::expanded(vec![0; 0x800], vec![0; 0x10]).unwrap();
        assert!(matches!(
            project.save_shared_palette(&expanded, layout(), CHECKSUM),
            Err(SharedPaletteIoError::BackendMismatch { .. })
        ));
        assert_eq!(project.save_snapshot(), before);
    }
}
