use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_project::{
    Project, RomMutation, TransactionError, VanillaEntranceIoError, VanillaEntranceRomLayout,
    VanillaMainEntrance,
};
use lm_rom::{Mapper, RomError, RomImage};

#[derive(Debug)]
pub enum VanillaEntranceControllerError {
    WrongMode(EditorMode),
    MapperMismatch { snapshot: Mapper, layout: Mapper },
    Io(VanillaEntranceIoError),
    Rom(RomError),
    Mutation(TransactionError),
}

impl std::fmt::Display for VanillaEntranceControllerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "vanilla entrance controller failed: {self:?}")
    }
}

impl std::error::Error for VanillaEntranceControllerError {}

#[derive(Clone, Debug)]
pub struct VanillaEntranceController {
    revision: u64,
    slot: usize,
    layout: VanillaEntranceRomLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    baseline: VanillaMainEntrance,
    entrance: VanillaMainEntrance,
}

impl VanillaEntranceController {
    /// Loads the selected pristine-SMW level's main entrance.
    ///
    /// # Errors
    ///
    /// Rejects non-level modes, mapper disagreement, malformed ROMs, and inaccessible tables.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: VanillaEntranceRomLayout,
    ) -> Result<Self, VanillaEntranceControllerError> {
        let EditorMode::Level(slot) = snapshot.mode else {
            return Err(VanillaEntranceControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(VanillaEntranceControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(VanillaEntranceControllerError::Rom)?;
        let slot = usize::from(slot);
        let entrance = Project::new(image)
            .load_vanilla_main_entrance(slot, layout)
            .map_err(VanillaEntranceControllerError::Io)?;
        Ok(Self {
            revision: snapshot.revision,
            slot,
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: entrance,
            entrance,
        })
    }

    #[must_use]
    pub const fn entrance(&self) -> VanillaMainEntrance {
        self.entrance
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.entrance != self.baseline
    }

    pub fn set_entrance(&mut self, entrance: VanillaMainEntrance) {
        self.entrance = entrance;
    }

    /// Prepares one revision-bound checksum-inclusive entrance mutation.
    ///
    /// # Errors
    ///
    /// Rejects invalid source ROMs, table I/O failures, or mutation construction failures.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
    ) -> Result<PreparedRomCommit, VanillaEntranceControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(VanillaEntranceControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let mut project = Project::new(image);
        if self.is_modified() {
            project
                .save_vanilla_main_entrance(
                    self.slot,
                    self.entrance,
                    self.layout,
                    self.checksum_field,
                )
                .map_err(VanillaEntranceControllerError::Io)?;
        }
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(VanillaEntranceControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description: description.into(),
            mutation,
        })
    }
}
