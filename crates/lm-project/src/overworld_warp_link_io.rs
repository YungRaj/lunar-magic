//! Direct native overworld warp/exit-link table I/O.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldWarpLinkRomLayout {
    pub mapper: Mapper,
    pub source_vertical_offset: usize,
    pub source_horizontal_offset: usize,
    pub destination_vertical_offset: usize,
    pub destination_horizontal_offset: usize,
    pub entries: usize,
}

#[derive(Debug)]
pub enum OverworldWarpLinkIoError {
    InvalidEntryCount(usize),
    LinkCount { actual: usize, expected: usize },
    LengthOverflow,
    MapperImageShape { mapper: Mapper, image_len: usize },
    MapperMismatch { expected: Mapper, actual: Mapper },
    OverlappingRanges,
    Table(OverworldWarpLinkTableError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for OverworldWarpLinkIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native overworld warp-link I/O failed: {self:?}")
    }
}

impl std::error::Error for OverworldWarpLinkIoError {}

impl From<OverworldWarpLinkTableError> for OverworldWarpLinkIoError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<RomError> for OverworldWarpLinkIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldWarpLinkIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads all four fixed native warp coordinate planes.
    ///
    /// # Errors
    ///
    /// Rejects invalid layout shapes, mapper disagreement, out-of-range tables, and bad planes.
    pub fn load_overworld_warp_links(
        &self,
        layout: OverworldWarpLinkRomLayout,
    ) -> Result<OverworldWarpLinkTable, OverworldWarpLinkIoError> {
        let plane_len = validate_layout(self, layout)?;
        Ok(OverworldWarpLinkTable::decode_planes(
            self.rom.read(layout.source_vertical_offset, plane_len)?,
            self.rom.read(layout.source_horizontal_offset, plane_len)?,
            self.rom
                .read(layout.destination_vertical_offset, plane_len)?,
            self.rom
                .read(layout.destination_horizontal_offset, plane_len)?,
        )?)
    }

    /// Saves all four direct planes and repairs the checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects shape, mapper, table, overlap, checksum, or transaction failures before mutation.
    pub fn save_overworld_warp_links(
        &mut self,
        table: &OverworldWarpLinkTable,
        layout: OverworldWarpLinkRomLayout,
        checksum_field: usize,
    ) -> Result<bool, OverworldWarpLinkIoError> {
        validate_layout(self, layout)?;
        if table.links.len() != layout.entries {
            return Err(OverworldWarpLinkIoError::LinkCount {
                actual: table.links.len(),
                expected: layout.entries,
            });
        }
        let planes = table.encode_planes()?;
        let mut writes = vec![
            RomWrite {
                offset: layout.source_vertical_offset,
                bytes: planes.source_vertical,
            },
            RomWrite {
                offset: layout.source_horizontal_offset,
                bytes: planes.source_horizontal,
            },
            RomWrite {
                offset: layout.destination_vertical_offset,
                bytes: planes.destination_vertical,
            },
            RomWrite {
                offset: layout.destination_horizontal_offset,
                bytes: planes.destination_horizontal,
            },
        ];
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(OverworldWarpLinkIoError::LengthOverflow)?;
        if writes.iter().any(|write| {
            let end = write.offset.saturating_add(write.bytes.len());
            write.offset < checksum_end && checksum_field < end
        }) {
            return Err(OverworldWarpLinkIoError::OverlappingRanges);
        }
        self.writes_would_change(&writes)
            .map_err(|error| match error {
                TransactionError::OverlappingWrites { .. } => {
                    OverworldWarpLinkIoError::OverlappingRanges
                }
                other => OverworldWarpLinkIoError::Transaction(other),
            })?;
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save native overworld warp links", &writes)?)
    }
}

fn validate_layout(
    project: &Project,
    layout: OverworldWarpLinkRomLayout,
) -> Result<usize, OverworldWarpLinkIoError> {
    if layout.entries == 0 || layout.entries > OverworldWarpLinkTable::MAX_LINKS {
        return Err(OverworldWarpLinkIoError::InvalidEntryCount(layout.entries));
    }
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(OverworldWarpLinkIoError::MapperImageShape {
            mapper: layout.mapper,
            image_len: project.rom.logical_len(),
        });
    }
    if let Some(identity) = &project.identity
        && identity.mapper != layout.mapper
    {
        return Err(OverworldWarpLinkIoError::MapperMismatch {
            expected: identity.mapper,
            actual: layout.mapper,
        });
    }
    let plane_len = layout
        .entries
        .checked_mul(2)
        .ok_or(OverworldWarpLinkIoError::LengthOverflow)?;
    for offset in [
        layout.source_vertical_offset,
        layout.source_horizontal_offset,
        layout.destination_vertical_offset,
        layout.destination_horizontal_offset,
    ] {
        let end = offset
            .checked_add(plane_len)
            .ok_or(OverworldWarpLinkIoError::LengthOverflow)?;
        if end > project.rom.logical_len() {
            return Err(OverworldWarpLinkIoError::Rom(RomError::RangeOutOfBounds {
                offset,
                len: plane_len,
                image_len: project.rom.logical_len(),
            }));
        }
    }
    Ok(plane_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink};
    use lm_rom::{RomImage, SnesChecksum};

    fn layout() -> OverworldWarpLinkRomLayout {
        OverworldWarpLinkRomLayout {
            mapper: Mapper::LoRom,
            source_vertical_offset: 0x100,
            source_horizontal_offset: 0x110,
            destination_vertical_offset: 0x120,
            destination_horizontal_offset: 0x130,
            entries: 2,
        }
    }

    fn table() -> OverworldWarpLinkTable {
        OverworldWarpLinkTable {
            links: (0_u16..2)
                .map(|index| OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: index,
                        horizontal_tile: index + 1,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: index + 2,
                        horizontal_tile: index + 3,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn direct_save_load_checksum_and_undo_are_one_operation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        assert!(
            project
                .save_overworld_warp_links(&table(), layout(), 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_overworld_warp_links(layout()).unwrap(),
            table()
        );
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.read(0x100, 4).unwrap(), &[0xff; 4]);
    }

    #[test]
    fn wrong_count_and_overlap_preserve_the_project() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.rom.logical_bytes().to_vec();
        let mut short = table();
        short.links.pop();
        assert!(matches!(
            project.save_overworld_warp_links(&short, layout(), 0x7fdc),
            Err(OverworldWarpLinkIoError::LinkCount { .. })
        ));
        let mut overlap = layout();
        overlap.source_horizontal_offset = 0x102;
        assert!(matches!(
            project.save_overworld_warp_links(&table(), overlap, 0x7fdc),
            Err(OverworldWarpLinkIoError::OverlappingRanges)
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
    }
}
