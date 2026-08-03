use crate::{
    ControllerSnapshot, EditorMode, OverworldControllerEdit, OverworldLayerId, PreparedRomCommit,
};
use lm_overworld::{OverworldEditError, OverworldLayer};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SmwUsV1MainOverworldLayer2Error,
    SmwUsV1MainOverworldLayer2SaveOptions, load_smw_us_v1_main_overworld_layer2,
    save_smw_us_v1_main_overworld_layer2,
};
use lm_project::{Project, RomMutation, TransactionError};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, Region, RomError, RomImage, SupportedGame};
use std::fmt;

/// Revision-bound editor for the gameplay-consumed SMW US main-overworld Layer 2 tilemap.
#[derive(Clone, Debug)]
pub struct SmwMainOverworldLayer2Controller {
    revision: u64,
    source_file_bytes: Vec<u8>,
    baseline: OverworldLayer,
    layer: OverworldLayer,
}

impl SmwMainOverworldLayer2Controller {
    /// Loads the authentic two-stream main-map representation from an overworld snapshot.
    ///
    /// # Errors
    ///
    /// Rejects the wrong editor mode, any identity other than SMW US revision 0 `LoROM`, or an
    /// unauthenticated/corrupt Layer 2 runtime representation.
    pub fn decode(
        snapshot: &ControllerSnapshot,
    ) -> Result<Self, SmwMainOverworldLayer2ControllerError> {
        if snapshot.mode != EditorMode::Overworld {
            return Err(SmwMainOverworldLayer2ControllerError::WrongMode(
                snapshot.mode,
            ));
        }
        if snapshot.identity.game != SupportedGame::SuperMarioWorld
            || snapshot.identity.region != Region::NorthAmerica
            || snapshot.identity.revision != 0
            || snapshot.identity.mapper != Mapper::LoRom
        {
            return Err(SmwMainOverworldLayer2ControllerError::UnsupportedIdentity);
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())?;
        let loaded = load_smw_us_v1_main_overworld_layer2(&Project::new(image))?;
        Ok(Self {
            revision: snapshot.revision,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: loaded.layer.clone(),
            layer: loaded.layer,
        })
    }

    #[must_use]
    pub const fn layer(&self) -> &OverworldLayer {
        &self.layer
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.layer != self.baseline
    }

    /// Applies a batch containing only playable Layer 2 tile edits, atomically.
    ///
    /// # Errors
    ///
    /// Rejects another overworld domain or any coordinate outside the authentic 128x64 map.
    pub fn apply_edits(
        &mut self,
        edits: &[OverworldControllerEdit],
    ) -> Result<(), SmwMainOverworldLayer2ControllerError> {
        let mut staged = self.layer.clone();
        for (command, edit) in edits.iter().enumerate() {
            let OverworldControllerEdit::SetLayerTile {
                layer: OverworldLayerId::Layer2,
                x,
                y,
                tile,
            } = edit
            else {
                return Err(SmwMainOverworldLayer2ControllerError::UnsupportedEdit { command });
            };
            staged
                .set_tile(*x, *y, *tile)
                .map_err(|error| SmwMainOverworldLayer2ControllerError::Edit { command, error })?;
        }
        self.layer = staged;
        Ok(())
    }

    /// Serializes the edited map into one revision-checked application mutation.
    ///
    /// # Errors
    ///
    /// Returns an error for ROM decoding, allocation, compression, pointer, checksum, or mutation
    /// failures without changing the live application project.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        allocation: AllocationPolicy,
    ) -> Result<PreparedRomCommit, SmwMainOverworldLayer2ControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(Mapper::LoRom, before.len()),
            });
        }
        let mut project = Project::new(image);
        save_smw_us_v1_main_overworld_layer2(
            &mut project,
            &self.layer,
            SMW_US_V1_CHECKSUM_FIELD,
            &SmwUsV1MainOverworldLayer2SaveOptions {
                allocation,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )?;
        let reopened = load_smw_us_v1_main_overworld_layer2(&project)?;
        if reopened.layer != self.layer {
            return Err(SmwMainOverworldLayer2ControllerError::ReopenMismatch);
        }
        let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}

#[derive(Debug)]
pub enum SmwMainOverworldLayer2ControllerError {
    WrongMode(EditorMode),
    UnsupportedIdentity,
    UnsupportedEdit {
        command: usize,
    },
    Edit {
        command: usize,
        error: OverworldEditError,
    },
    Rom(RomError),
    Layer2(SmwUsV1MainOverworldLayer2Error),
    Mutation(TransactionError),
    ReopenMismatch,
}

impl fmt::Display for SmwMainOverworldLayer2ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "SMW main-overworld Layer 2 controller failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwMainOverworldLayer2ControllerError {}

impl From<RomError> for SmwMainOverworldLayer2ControllerError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<SmwUsV1MainOverworldLayer2Error> for SmwMainOverworldLayer2ControllerError {
    fn from(value: SmwUsV1MainOverworldLayer2Error) -> Self {
        Self::Layer2(value)
    }
}

impl From<TransactionError> for SmwMainOverworldLayer2ControllerError {
    fn from(value: TransactionError) -> Self {
        Self::Mutation(value)
    }
}
