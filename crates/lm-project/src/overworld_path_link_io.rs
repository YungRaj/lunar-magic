//! Direct native overworld path-link table I/O.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::{OverworldPathLinkTable, OverworldPathLinkTableError};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldPathLinkRomLayout {
    pub mapper: Mapper,
    pub source_offset: usize,
    pub destination_offset: usize,
    pub target_offset: usize,
    pub entries: usize,
}

#[derive(Debug)]
pub enum OverworldPathLinkIoError {
    InvalidEntryCount(usize),
    LinkCount { actual: usize, expected: usize },
    LengthOverflow,
    MapperImageShape { mapper: Mapper, image_len: usize },
    MapperMismatch { expected: Mapper, actual: Mapper },
    OverlappingRanges,
    Table(OverworldPathLinkTableError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for OverworldPathLinkIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native overworld path-link I/O failed: {self:?}")
    }
}

impl std::error::Error for OverworldPathLinkIoError {}

impl From<OverworldPathLinkTableError> for OverworldPathLinkIoError {
    fn from(value: OverworldPathLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<RomError> for OverworldPathLinkIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldPathLinkIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads all three planes of one fixed native path-link table.
    ///
    /// # Errors
    ///
    /// Rejects invalid layout shapes, mapper disagreement, out-of-range tables, and bad planes.
    pub fn load_overworld_path_links(
        &self,
        layout: OverworldPathLinkRomLayout,
    ) -> Result<OverworldPathLinkTable, OverworldPathLinkIoError> {
        let lengths = validate_layout(self, layout)?;
        Ok(OverworldPathLinkTable::decode_planes(
            self.rom.read(layout.source_offset, lengths.endpoint)?,
            self.rom.read(layout.destination_offset, lengths.endpoint)?,
            self.rom.read(layout.target_offset, lengths.target)?,
        )?)
    }

    /// Saves all three direct planes and repairs the checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects shape, mapper, table, overlap, checksum, or transaction failures before mutation.
    pub fn save_overworld_path_links(
        &mut self,
        table: &OverworldPathLinkTable,
        layout: OverworldPathLinkRomLayout,
        checksum_field: usize,
    ) -> Result<bool, OverworldPathLinkIoError> {
        validate_layout(self, layout)?;
        if table.links.len() != layout.entries {
            return Err(OverworldPathLinkIoError::LinkCount {
                actual: table.links.len(),
                expected: layout.entries,
            });
        }
        let planes = table.encode_planes()?;
        let mut writes = vec![
            RomWrite {
                offset: layout.source_offset,
                bytes: planes.sources,
            },
            RomWrite {
                offset: layout.destination_offset,
                bytes: planes.destinations,
            },
            RomWrite {
                offset: layout.target_offset,
                bytes: planes.targets,
            },
        ];
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(OverworldPathLinkIoError::LengthOverflow)?;
        if writes.iter().any(|write| {
            let end = write.offset.saturating_add(write.bytes.len());
            write.offset < checksum_end && checksum_field < end
        }) {
            return Err(OverworldPathLinkIoError::OverlappingRanges);
        }
        self.writes_would_change(&writes)
            .map_err(|error| match error {
                TransactionError::OverlappingWrites { .. } => {
                    OverworldPathLinkIoError::OverlappingRanges
                }
                other => OverworldPathLinkIoError::Transaction(other),
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
        Ok(self.apply_writes("save native overworld path links", &writes)?)
    }
}

#[derive(Clone, Copy)]
struct PlaneLengths {
    endpoint: usize,
    target: usize,
}

fn validate_layout(
    project: &Project,
    layout: OverworldPathLinkRomLayout,
) -> Result<PlaneLengths, OverworldPathLinkIoError> {
    if layout.entries == 0 || layout.entries > OverworldPathLinkTable::MAX_LINKS {
        return Err(OverworldPathLinkIoError::InvalidEntryCount(layout.entries));
    }
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(OverworldPathLinkIoError::MapperImageShape {
            mapper: layout.mapper,
            image_len: project.rom.logical_len(),
        });
    }
    if let Some(identity) = &project.identity
        && identity.mapper != layout.mapper
    {
        return Err(OverworldPathLinkIoError::MapperMismatch {
            expected: identity.mapper,
            actual: layout.mapper,
        });
    }
    let endpoint = layout
        .entries
        .checked_mul(5)
        .ok_or(OverworldPathLinkIoError::LengthOverflow)?;
    let target = layout
        .entries
        .checked_mul(2)
        .ok_or(OverworldPathLinkIoError::LengthOverflow)?;
    for (offset, len) in [
        (layout.source_offset, endpoint),
        (layout.destination_offset, endpoint),
        (layout.target_offset, target),
    ] {
        let end = offset
            .checked_add(len)
            .ok_or(OverworldPathLinkIoError::LengthOverflow)?;
        if end > project.rom.logical_len() {
            return Err(OverworldPathLinkIoError::Rom(RomError::RangeOutOfBounds {
                offset,
                len,
                image_len: project.rom.logical_len(),
            }));
        }
    }
    Ok(PlaneLengths { endpoint, target })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldEndpoint, OverworldPathLink, OverworldPathTarget};
    use lm_rom::{RomImage, SnesChecksum};

    fn layout() -> OverworldPathLinkRomLayout {
        OverworldPathLinkRomLayout {
            mapper: Mapper::LoRom,
            source_offset: 0x100,
            destination_offset: 0x120,
            target_offset: 0x140,
            entries: 2,
        }
    }

    fn table() -> OverworldPathLinkTable {
        OverworldPathLinkTable {
            links: (0_u8..2)
                .map(|index| OverworldPathLink {
                    source: OverworldEndpoint {
                        x: u16::from(index),
                        y: u16::from(index + 1),
                        submap: index,
                    },
                    destination: OverworldEndpoint {
                        x: u16::from(index + 2),
                        y: u16::from(index + 3),
                        submap: index + 4,
                    },
                    target: OverworldPathTarget {
                        y_tile: index + 5,
                        x_tile: index + 6,
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
                .save_overworld_path_links(&table(), layout(), 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_overworld_path_links(layout()).unwrap(),
            table()
        );
        assert_eq!(
            project.rom.read(layout().source_offset, 5).unwrap(),
            &[1, 0, 0, 0, 0]
        );
        assert_eq!(
            project.rom.read(layout().destination_offset, 5).unwrap(),
            &[3, 0, 2, 0, 4]
        );
        assert_eq!(
            project.rom.read(layout().target_offset, 2).unwrap(),
            &[5, 6]
        );
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.read(0x100, 10).unwrap(), &[0xff; 10]);
    }

    #[test]
    fn wrong_count_and_late_overlap_preserve_the_project() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.rom.logical_bytes().to_vec();
        let mut short = table();
        short.links.pop();
        assert!(matches!(
            project.save_overworld_path_links(&short, layout(), 0x7fdc),
            Err(OverworldPathLinkIoError::LinkCount { .. })
        ));
        let mut overlap = layout();
        overlap.destination_offset = 0x105;
        assert!(matches!(
            project.save_overworld_path_links(&table(), overlap, 0x7fdc),
            Err(OverworldPathLinkIoError::OverlappingRanges)
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
    }
}
