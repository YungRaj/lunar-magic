//! Transactional installation and updates for special-event reveal tables.

use crate::{
    PayloadPointer, PayloadSaveError, PayloadSaveRequest, Project, RelocatablePatchError,
    RelocatablePatchPlan, SpecialEventRevealPatchError, SpecialEventRevealPatchLocator,
    SpecialEventRevealStorage,
    overworld_special_event_patch::assert_special_event_install_plan_shape,
};
use lm_overworld::{SpecialEventRevealError, SpecialEventRevealTable};
use lm_rats::AllocationPolicy;

#[derive(Debug)]
pub enum SpecialEventRevealSaveError {
    Detection(SpecialEventRevealPatchError),
    Table(SpecialEventRevealError),
    InstallPlan,
    Install(RelocatablePatchError),
    Save(PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for SpecialEventRevealSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native special-event reveal save failed: {self:?}"
        )
    }
}

impl std::error::Error for SpecialEventRevealSaveError {}

impl From<SpecialEventRevealPatchError> for SpecialEventRevealSaveError {
    fn from(value: SpecialEventRevealPatchError) -> Self {
        Self::Detection(value)
    }
}

impl From<SpecialEventRevealError> for SpecialEventRevealSaveError {
    fn from(value: SpecialEventRevealError) -> Self {
        Self::Table(value)
    }
}

impl From<RelocatablePatchError> for SpecialEventRevealSaveError {
    fn from(value: RelocatablePatchError) -> Self {
        Self::Install(value)
    }
}

impl From<PayloadSaveError> for SpecialEventRevealSaveError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Installs the complete compatibility runtime from pristine storage or updates all three
    /// owned planes and both source-pointer owners as one checksum-valid undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects malformed current ownership/runtime state, lossy tables, a mismatched installation
    /// plan, allocation or hook failures, and semantic reopen disagreement.
    pub fn save_special_event_reveals_detected(
        &mut self,
        table: &SpecialEventRevealTable,
        locator: SpecialEventRevealPatchLocator,
        installation_plan: &RelocatablePatchPlan,
        update_allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, SpecialEventRevealSaveError> {
        let loaded = self.load_special_event_reveals_detected(locator)?;
        if loaded.table == *table {
            return Ok(false);
        }
        let planes = table.encode()?;
        match loaded.storage {
            SpecialEventRevealStorage::Fixed => {
                assert_special_event_install_plan_shape(installation_plan)?;
                if installation_plan.mapper != locator.mapper
                    || installation_plan.checksum_field != checksum_field
                    || installation_plan.payloads[0].bytes != planes.sources
                    || installation_plan.payloads[1].bytes != planes.destinations
                    || installation_plan.payloads[2].bytes != planes.directions
                {
                    return Err(SpecialEventRevealSaveError::InstallPlan);
                }
                self.install_relocatable_patch(installation_plan)?;
            }
            SpecialEventRevealStorage::Expanded {
                source,
                destination,
                directions: previous_directions,
                full_runtime,
                ..
            } => {
                let request = |description: &str,
                               payload: Vec<u8>,
                               pointer: usize,
                               previous_block| PayloadSaveRequest {
                    description: description.into(),
                    maximum_payload_len: payload.len(),
                    payload,
                    pointer: PayloadPointer::contiguous_low_bank(pointer),
                    mapper: locator.mapper,
                    allocation_policy: update_allocation.clone(),
                    previous_block,
                    reuse_identical: true,
                    erase_fill: fill,
                };
                let requests = vec![
                    request(
                        "special-event sources",
                        planes.sources.clone(),
                        locator.source_operand,
                        Some(source.clone()),
                    ),
                    request(
                        "special-event runtime source",
                        planes.sources,
                        full_runtime.payload.start + 0x26,
                        Some(source),
                    ),
                    request(
                        "special-event destinations",
                        planes.destinations,
                        locator.destination_operand,
                        Some(destination),
                    ),
                    request(
                        "special-event directions",
                        planes.directions,
                        locator.direction_operand,
                        Some(previous_directions),
                    ),
                ];
                self.save_tagged_payloads_with_checksum(
                    "save native special-event reveals",
                    &requests,
                    checksum_field,
                )?;
            }
        }
        if self.load_special_event_reveals_detected(locator)?.table != *table {
            return Err(SpecialEventRevealSaveError::ReopenMismatch);
        }
        Ok(true)
    }
}
