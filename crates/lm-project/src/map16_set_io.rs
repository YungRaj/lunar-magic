use crate::{
    Map16IoError, Map16RomLayout, PayloadReclamation, PayloadSaveError, PayloadSaveRequest,
    Project, RatsOwnershipManifest, SavedMap16Page,
};
use lm_level::{Map16Page, Map16Set, Map16SetError};
use lm_rats::{AllocationPolicy, RatsBlock};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16SetSaveOptions {
    pub graphics_allocation: AllocationPolicy,
    pub acts_like_allocation: AllocationPolicy,
    pub previous_graphics: Vec<Option<RatsBlock>>,
    pub previous_acts_like: Vec<Option<RatsBlock>>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedMap16Set {
    pub pages: Vec<SavedMap16Page>,
}

#[derive(Debug)]
pub enum Map16SetIoError {
    TableCount { graphics: usize, acts_like: usize },
    PageCount { actual: usize, expected: usize },
    AggregateSizeOverflow { pages: usize },
    Model(Map16SetError),
    PreviousBlockCount { actual: usize, expected: usize },
    Page(Map16IoError),
    Save(PayloadSaveError),
}

impl fmt::Display for Map16SetIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 set I/O failed: {self:?}")
    }
}

impl std::error::Error for Map16SetIoError {}

impl From<Map16IoError> for Map16SetIoError {
    fn from(value: Map16IoError) -> Self {
        Self::Page(value)
    }
}

impl From<PayloadSaveError> for Map16SetIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<Map16SetError> for Map16SetIoError {
    fn from(value: Map16SetError) -> Self {
        Self::Model(value)
    }
}

impl Project {
    /// Loads every page declared by parallel Map16 pointer tables.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetIoError`] when table counts differ or any page fails to load.
    pub fn load_map16_set(&self, layout: Map16RomLayout) -> Result<Map16Set, Map16SetIoError> {
        validate_table_counts(layout)?;
        let pages = (0..layout.graphics.entries)
            .map(|page| self.load_map16_page(page, layout))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Map16Set { pages })
    }

    /// Saves every graphics/Acts Like page pair as one atomic undo step.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetIoError`] for shape, previous-allocation, table, or save failures. Empty
    /// previous-block vectors mean that every page is currently untagged.
    pub fn save_map16_set(
        &mut self,
        set: &Map16Set,
        layout: Map16RomLayout,
        options: &Map16SetSaveOptions,
    ) -> Result<SavedMap16Set, Map16SetIoError> {
        self.save_map16_set_group(set, layout, options, None, None)
    }

    /// Saves the complete set and repairs the SNES checksum in the same atomic undo step.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetIoError`] when validation, allocation, mapping, or checksum repair fails.
    pub fn save_map16_set_with_checksum(
        &mut self,
        set: &Map16Set,
        layout: Map16RomLayout,
        checksum_field: usize,
        options: &Map16SetSaveOptions,
    ) -> Result<SavedMap16Set, Map16SetIoError> {
        self.save_map16_set_group(set, layout, options, Some(checksum_field), None)
    }

    /// Saves the complete set, reclaims exactly owned displaced page planes, and repairs the SNES
    /// checksum in the same atomic undo step.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetIoError`] when model, table, ownership, allocation, overlap, mapping, or
    /// checksum validation fails. The project and history remain unchanged on failure.
    pub fn save_map16_set_with_checksum_and_reclamation(
        &mut self,
        set: &Map16Set,
        layout: Map16RomLayout,
        options: &Map16SetSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<SavedMap16Set, Map16SetIoError> {
        self.save_map16_set_group(
            set,
            layout,
            options,
            Some(reclamation.checksum_field),
            Some(reclamation.manifest),
        )
    }

    fn save_map16_set_group(
        &mut self,
        set: &Map16Set,
        layout: Map16RomLayout,
        options: &Map16SetSaveOptions,
        checksum_field: Option<usize>,
        reclamation_manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<SavedMap16Set, Map16SetIoError> {
        validate_table_counts(layout)?;
        let expected = layout.graphics.entries;
        if set.pages.len() != expected {
            return Err(Map16SetIoError::PageCount {
                actual: set.pages.len(),
                expected,
            });
        }
        let (resolution_limit, request_count) = aggregate_counts(set.pages.len())?;
        set.validate_acts_like(resolution_limit)?;
        validate_previous_count(&options.previous_graphics, expected)?;
        validate_previous_count(&options.previous_acts_like, expected)?;
        let mut requests = Vec::with_capacity(request_count);
        for (page_number, page) in set.pages.iter().enumerate() {
            let (graphics, acts_like) = page
                .encode()
                .map_err(|error| Map16IoError::WrongPageSize(error.tiles))?;
            requests.push(PayloadSaveRequest {
                description: format!("save Map16 page {page_number:02x} graphics"),
                payload: graphics,
                pointer: layout
                    .graphics
                    .pointer_offset(page_number)
                    .map_err(Map16IoError::from)?
                    .into(),
                mapper: layout.mapper,
                allocation_policy: options.graphics_allocation.clone(),
                previous_block: previous(&options.previous_graphics, page_number),
                reuse_identical: options.reuse_identical,
                maximum_payload_len: Map16Set::GRAPHICS_PAGE_LEN,
                erase_fill: options.erase_fill,
            });
            requests.push(PayloadSaveRequest {
                description: format!("save Map16 page {page_number:02x} acts-like"),
                payload: acts_like,
                pointer: layout
                    .acts_like
                    .pointer_offset(page_number)
                    .map_err(Map16IoError::from)?
                    .into(),
                mapper: layout.mapper,
                allocation_policy: options.acts_like_allocation.clone(),
                previous_block: previous(&options.previous_acts_like, page_number),
                reuse_identical: options.reuse_identical,
                maximum_payload_len: Map16Set::ACTS_LIKE_PAGE_LEN,
                erase_fill: options.erase_fill,
            });
        }
        let results = match (checksum_field, reclamation_manifest) {
            (Some(field), Some(manifest)) => self
                .save_tagged_payloads_with_checksum_and_reclamation(
                    "save complete Map16 set",
                    &requests,
                    field,
                    manifest,
                )?,
            (Some(field), None) => self.save_tagged_payloads_with_checksum(
                "save complete Map16 set",
                &requests,
                field,
            )?,
            (None, None) => self.save_tagged_payloads("save complete Map16 set", &requests)?,
            (None, Some(_)) => unreachable!("reclamation API always supplies a checksum field"),
        };
        let pages = results
            .chunks_exact(2)
            .map(|pair| SavedMap16Page {
                graphics: pair[0].clone(),
                acts_like: pair[1].clone(),
            })
            .collect();
        Ok(SavedMap16Set { pages })
    }
}

fn aggregate_counts(pages: usize) -> Result<(usize, usize), Map16SetIoError> {
    let tile_count = pages
        .checked_mul(Map16Page::TILE_COUNT)
        .ok_or(Map16SetIoError::AggregateSizeOverflow { pages })?;
    let resolution_limit = tile_count
        .checked_add(1)
        .ok_or(Map16SetIoError::AggregateSizeOverflow { pages })?;
    let request_count = pages
        .checked_mul(2)
        .ok_or(Map16SetIoError::AggregateSizeOverflow { pages })?;
    Ok((resolution_limit, request_count))
}

fn validate_table_counts(layout: Map16RomLayout) -> Result<(), Map16SetIoError> {
    if layout.graphics.entries == layout.acts_like.entries {
        Ok(())
    } else {
        Err(Map16SetIoError::TableCount {
            graphics: layout.graphics.entries,
            acts_like: layout.acts_like.entries,
        })
    }
}

fn validate_previous_count(
    blocks: &[Option<RatsBlock>],
    expected: usize,
) -> Result<(), Map16SetIoError> {
    if blocks.is_empty() || blocks.len() == expected {
        Ok(())
    } else {
        Err(Map16SetIoError::PreviousBlockCount {
            actual: blocks.len(),
            expected,
        })
    }
}

fn previous(blocks: &[Option<RatsBlock>], index: usize) -> Option<RatsBlock> {
    blocks.get(index).cloned().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LevelPointerTable;
    use lm_level::{Map16Page, Map16Tile, Subtile};
    use lm_rats::ProtectedRange;
    use lm_rom::{Mapper, RomImage};

    fn layout() -> Map16RomLayout {
        Map16RomLayout {
            mapper: Mapper::LoRom,
            graphics: LevelPointerTable {
                offset: 0x20,
                entries: 2,
                stride: 3,
            },
            acts_like: LevelPointerTable {
                offset: 0x30,
                entries: 2,
                stride: 3,
            },
        }
    }

    fn set() -> Map16Set {
        let mut first = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        first[0].acts_like = 0x130;
        let mut second = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        second[0].top_left = Subtile(7);
        second[0x30].acts_like = 0x130;
        Map16Set {
            pages: vec![
                Map16Page::new(first).unwrap(),
                Map16Page::new(second).unwrap(),
            ],
        }
    }

    fn options() -> Map16SetSaveOptions {
        let policy = AllocationPolicy {
            search: 0x100..0x8000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x36)],
        };
        Map16SetSaveOptions {
            graphics_allocation: policy.clone(),
            acts_like_allocation: policy,
            previous_graphics: Vec::new(),
            previous_acts_like: Vec::new(),
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn complete_set_saves_loads_and_undoes_atomically() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let saved = project
            .save_map16_set(&set(), layout(), &options())
            .unwrap();
        assert_eq!(saved.pages.len(), 2);
        assert_eq!(project.load_map16_set(layout()).unwrap(), set());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn shape_error_happens_before_any_allocation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let short = Map16Set {
            pages: set().pages[..1].to_vec(),
        };
        assert!(matches!(
            project.save_map16_set(&short, layout(), &options()),
            Err(Map16SetIoError::PageCount {
                actual: 1,
                expected: 2
            })
        ));
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn aggregate_counts_are_exact_and_never_saturate() {
        assert_eq!(aggregate_counts(2).unwrap(), (513, 4));
        let overflowing_pages = usize::MAX / Map16Page::TILE_COUNT + 1;
        assert!(matches!(
            aggregate_counts(overflowing_pages),
            Err(Map16SetIoError::AggregateSizeOverflow { pages })
                if pages == overflowing_pages
        ));
    }

    #[test]
    fn invalid_complete_acts_like_graph_is_rejected_before_allocation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = set();
        invalid.pages[1].tiles[0x30].acts_like = 0x300;
        assert!(matches!(
            project.save_map16_set(&invalid, layout(), &options()),
            Err(Map16SetIoError::Model(
                Map16SetError::ActsLikeOutOfRange { .. }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
