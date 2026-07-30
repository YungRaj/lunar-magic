use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadReclamation,
    PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RatsOwnershipManifest,
};
use lm_level::{BinaryError, Map16Page};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Map16RomLayout {
    pub mapper: Mapper,
    pub graphics: LevelPointerTable,
    pub acts_like: LevelPointerTable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16SaveOptions {
    pub graphics_allocation: AllocationPolicy,
    pub acts_like_allocation: AllocationPolicy,
    pub previous_graphics: Option<RatsBlock>,
    pub previous_acts_like: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedMap16Page {
    pub graphics: PayloadSaveResult,
    pub acts_like: PayloadSaveResult,
}

#[derive(Debug)]
pub enum Map16IoError {
    Layout(LevelLoadError),
    Load(PayloadLoadError),
    Decode(BinaryError),
    WrongPageSize(usize),
    Save(PayloadSaveError),
}

impl fmt::Display for Map16IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 I/O failed: {self:?}")
    }
}

impl std::error::Error for Map16IoError {}

impl From<LevelLoadError> for Map16IoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<PayloadLoadError> for Map16IoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<BinaryError> for Map16IoError {
    fn from(value: BinaryError) -> Self {
        Self::Decode(value)
    }
}

impl From<PayloadSaveError> for Map16IoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads one Map16 page from vanilla fixed-size data or relocated RATS payloads.
    ///
    /// # Errors
    ///
    /// Returns [`Map16IoError`] for invalid page/table bounds, pointers, payloads, or tile data.
    pub fn load_map16_page(
        &self,
        page: usize,
        layout: Map16RomLayout,
    ) -> Result<Map16Page, Map16IoError> {
        let graphics = self.load_payload(
            layout.graphics.pointer_offset(page)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x800 },
        )?;
        let acts_like = self.load_payload(
            layout.acts_like.pointer_offset(page)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x200 },
        )?;
        Ok(Map16Page::decode(&graphics.bytes, &acts_like.bytes)?)
    }

    /// Saves both halves of one Map16 page in one atomic, undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`Map16IoError`] for invalid table bounds or failed allocation/pointer updates.
    pub fn save_map16_page(
        &mut self,
        page_number: usize,
        page: &Map16Page,
        layout: Map16RomLayout,
        options: &Map16SaveOptions,
    ) -> Result<SavedMap16Page, Map16IoError> {
        self.save_map16_page_group(page_number, page, layout, options, None, None)
    }

    /// Saves both page payloads and repairs the SNES checksum in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`Map16IoError`] when shape validation, allocation, mapping, or checksum repair fails.
    pub fn save_map16_page_with_checksum(
        &mut self,
        page_number: usize,
        page: &Map16Page,
        layout: Map16RomLayout,
        checksum_field: usize,
        options: &Map16SaveOptions,
    ) -> Result<SavedMap16Page, Map16IoError> {
        self.save_map16_page_group(
            page_number,
            page,
            layout,
            options,
            Some(checksum_field),
            None,
        )
    }

    /// Saves both page planes, reclaims exactly owned displaced blocks, and repairs checksum as
    /// one undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`Map16IoError`] for shape, ownership, allocation, overlap, mapping, or checksum
    /// failure without mutation.
    pub fn save_map16_page_with_checksum_and_reclamation(
        &mut self,
        page_number: usize,
        page: &Map16Page,
        layout: Map16RomLayout,
        options: &Map16SaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<SavedMap16Page, Map16IoError> {
        self.save_map16_page_group(
            page_number,
            page,
            layout,
            options,
            Some(reclamation.checksum_field),
            Some(reclamation.manifest),
        )
    }

    fn save_map16_page_group(
        &mut self,
        page_number: usize,
        page: &Map16Page,
        layout: Map16RomLayout,
        options: &Map16SaveOptions,
        checksum_field: Option<usize>,
        reclamation_manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<SavedMap16Page, Map16IoError> {
        let requests = map16_page_save_requests(page_number, page, layout, options)?;
        let description = format!("save complete Map16 page {page_number:02x}");
        let mut results = match (checksum_field, reclamation_manifest) {
            (Some(field), Some(manifest)) => self
                .save_tagged_payloads_with_checksum_and_reclamation(
                    description,
                    &requests,
                    field,
                    manifest,
                )?,
            (Some(field), None) => {
                self.save_tagged_payloads_with_checksum(description, &requests, field)?
            }
            (None, None) => self.save_tagged_payloads(description, &requests)?,
            (None, Some(_)) => unreachable!("reclamation API always supplies a checksum field"),
        };
        Ok(SavedMap16Page {
            graphics: results.remove(0),
            acts_like: results.remove(0),
        })
    }
}

pub(crate) fn map16_page_save_requests(
    page_number: usize,
    page: &Map16Page,
    layout: Map16RomLayout,
    options: &Map16SaveOptions,
) -> Result<[PayloadSaveRequest; 2], Map16IoError> {
    if page.tiles.len() != Map16Page::TILE_COUNT {
        return Err(Map16IoError::WrongPageSize(page.tiles.len()));
    }
    let (graphics, acts_like) = page
        .encode()
        .map_err(|error| Map16IoError::WrongPageSize(error.tiles))?;
    Ok([
        PayloadSaveRequest {
            description: format!("save Map16 page {page_number:02x} graphics"),
            payload: graphics,
            pointer: layout.graphics.pointer_offset(page_number)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.graphics_allocation.clone(),
            previous_block: options.previous_graphics.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: 0x800,
            erase_fill: options.erase_fill,
        },
        PayloadSaveRequest {
            description: format!("save Map16 page {page_number:02x} acts-like"),
            payload: acts_like,
            pointer: layout.acts_like.pointer_offset(page_number)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.acts_like_allocation.clone(),
            previous_block: options.previous_acts_like.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: 0x200,
            erase_fill: options.erase_fill,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Map16Tile, Subtile};
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

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

    fn policy() -> AllocationPolicy {
        AllocationPolicy {
            search: 0x100..0x8000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x36)],
        }
    }

    fn page() -> Map16Page {
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[7] = Map16Tile {
            top_left: Subtile(0x4321),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0x130,
        };
        Map16Page::new(tiles).unwrap()
    }

    #[test]
    fn saves_reloads_and_undoes_both_page_halves() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = Map16SaveOptions {
            graphics_allocation: policy(),
            acts_like_allocation: policy(),
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        project
            .save_map16_page(1, &page(), layout(), &options)
            .unwrap();
        assert_eq!(project.load_map16_page(1, layout()).unwrap(), page());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn rejects_page_outside_declared_tables_without_mutation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = Map16SaveOptions {
            graphics_allocation: policy(),
            acts_like_allocation: policy(),
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        assert!(
            project
                .save_map16_page(2, &page(), layout(), &options)
                .is_err()
        );
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn malformed_public_page_shape_is_rejected_before_allocation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = Map16SaveOptions {
            graphics_allocation: policy(),
            acts_like_allocation: policy(),
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        assert!(matches!(
            project.save_map16_page(0, &Map16Page { tiles: vec![] }, layout(), &options),
            Err(Map16IoError::WrongPageSize(0))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn loads_fixed_size_page_from_an_untagged_rom() {
        let expected = page();
        let (graphics, acts_like) = expected.encode().unwrap();
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x90, 0x80]);
        bytes[0x30..0x33].copy_from_slice(&[0x00, 0xa0, 0x80]);
        bytes[0x1000..0x1800].copy_from_slice(&graphics);
        bytes[0x2000..0x2200].copy_from_slice(&acts_like);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(project.load_map16_page(0, layout()).unwrap(), expected);
    }
}
