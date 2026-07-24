//! Direct native overworld player-start block I/O.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::{NativeOverworldPlayerStartError, NativeOverworldPlayerStarts};
use lm_rom::{Mapper, RomError, compute_snes_checksum};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldPlayerStartRomLayout {
    pub mapper: Mapper,
    pub options_offset: usize,
    pub custom_start_patch_offset: usize,
    pub pristine_patch: [u8; 3],
    pub enabled_patch: [u8; 3],
}

#[derive(Debug)]
pub enum OverworldPlayerStartIoError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    PatchPrecondition([u8; 3]),
    Table(NativeOverworldPlayerStartError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for OverworldPlayerStartIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld player-start I/O failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPlayerStartIoError {}

impl From<NativeOverworldPlayerStartError> for OverworldPlayerStartIoError {
    fn from(value: NativeOverworldPlayerStartError) -> Self {
        Self::Table(value)
    }
}

impl From<RomError> for OverworldPlayerStartIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldPlayerStartIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads the exact native 22-byte runtime-options block containing both player starts.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, out-of-range data, and malformed submap values.
    pub fn load_overworld_player_starts(
        &self,
        layout: OverworldPlayerStartRomLayout,
    ) -> Result<NativeOverworldPlayerStarts, OverworldPlayerStartIoError> {
        validate_mapper(self, layout.mapper)?;
        Ok(NativeOverworldPlayerStarts::decode(self.rom.read(
            layout.options_offset,
            NativeOverworldPlayerStarts::ENCODED_LEN,
        )?)?)
    }

    /// Saves both starts, enables Lunar Magic's custom-start path when necessary, and repairs the
    /// checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects lossy models, mapper disagreement, unknown patch bytes, ROM bounds, or transaction
    /// failures before mutation.
    pub fn save_overworld_player_starts(
        &mut self,
        starts: &NativeOverworldPlayerStarts,
        layout: OverworldPlayerStartRomLayout,
        checksum_field: usize,
    ) -> Result<bool, OverworldPlayerStartIoError> {
        validate_mapper(self, layout.mapper)?;
        let encoded = starts.encode()?;
        let observed: [u8; 3] = self
            .rom
            .read(layout.custom_start_patch_offset, 3)?
            .try_into()
            .map_err(|_| RomError::RangeOutOfBounds {
                offset: layout.custom_start_patch_offset,
                len: 3,
                image_len: self.rom.logical_len(),
            })?;
        if observed != layout.pristine_patch && observed != layout.enabled_patch {
            return Err(OverworldPlayerStartIoError::PatchPrecondition(observed));
        }
        let mut writes = vec![RomWrite {
            offset: layout.options_offset,
            bytes: encoded.to_vec(),
        }];
        if !starts.is_vanilla() && observed != layout.enabled_patch {
            writes.push(RomWrite {
                offset: layout.custom_start_patch_offset,
                bytes: layout.enabled_patch.to_vec(),
            });
        }
        if !self.writes_would_change(&writes)? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save native overworld player starts", &writes)?)
    }
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), OverworldPlayerStartIoError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(OverworldPlayerStartIoError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}
