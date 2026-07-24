//! Transactional native event-reveal saves using detected fixed or expanded ownership.

use crate::{
    EventRevealSaveOptions, OverworldEventRevealLocator, OverworldEventRevealPatchError,
    OverworldEventRevealStorage, Project, SavedEventRevealTable,
    overworld_event_io::event_save_requests,
};
use lm_overworld::EventRevealTable;
use lm_rats::AllocationPolicy;

#[derive(Debug)]
pub enum OverworldEventRevealSaveError {
    Detection(OverworldEventRevealPatchError),
    Io(crate::EventRevealIoError),
    Save(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for OverworldEventRevealSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld event-reveal save failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldEventRevealSaveError {}

impl From<OverworldEventRevealPatchError> for OverworldEventRevealSaveError {
    fn from(value: OverworldEventRevealPatchError) -> Self {
        Self::Detection(value)
    }
}

impl From<crate::EventRevealIoError> for OverworldEventRevealSaveError {
    fn from(value: crate::EventRevealIoError) -> Self {
        Self::Io(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldEventRevealSaveError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Saves a complete main event-reveal table, repointing both planes and repairing the checksum.
    ///
    /// Fixed inputs become tagged on first save. Expanded inputs provide exact prior ownership for
    /// copy-on-write relocation. The whole operation is one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects malformed current storage, invalid table semantics, allocation/pointer/checksum
    /// failures, and any semantic reopen mismatch without publishing a partial project edit.
    pub fn save_overworld_event_reveals_detected(
        &mut self,
        table: &EventRevealTable,
        locator: OverworldEventRevealLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<SavedEventRevealTable, OverworldEventRevealSaveError> {
        let loaded = self.load_overworld_event_reveals_detected(locator)?;
        let (previous_sources, previous_destinations) = match loaded.storage {
            OverworldEventRevealStorage::Fixed => (None, None),
            OverworldEventRevealStorage::TransferredSources { source_block } => {
                (Some(source_block), None)
            }
            OverworldEventRevealStorage::Expanded {
                source_block,
                destination_block,
            } => (Some(source_block), Some(destination_block)),
        };
        let layout = crate::EventRevealRomLayout {
            entries_per_slot: table.entries.len(),
            ..loaded.layout
        };
        let options = EventRevealSaveOptions {
            source_allocation: allocation.clone(),
            destination_allocation: allocation.clone(),
            previous_sources,
            previous_destinations,
            reuse_identical: false,
            erase_fill: fill,
        };
        let requests = event_save_requests(0, table, layout, &options)?;
        let mut results = self.save_tagged_payloads_with_checksum(
            "save native overworld event reveals",
            &requests,
            checksum_field,
        )?;
        if self.load_overworld_event_reveals_detected(locator)?.table != *table {
            return Err(OverworldEventRevealSaveError::ReopenMismatch);
        }
        Ok(SavedEventRevealTable {
            sources: results.remove(0),
            destinations: results.remove(0),
        })
    }
}
