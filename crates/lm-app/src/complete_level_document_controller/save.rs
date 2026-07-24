use super::{
    CompleteLevelDocumentController, CompleteLevelDocumentControllerError,
    CompleteLevelDocumentSaveSnapshot, PendingSave,
};

impl CompleteLevelDocumentController {
    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid programmatic data, and request counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<CompleteLevelDocumentSaveSnapshot, CompleteLevelDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(CompleteLevelDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(CompleteLevelDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(CompleteLevelDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(CompleteLevelDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Marks the exact pending snapshot saved while retaining newer edits as modified.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched pending save without discarding a mismatched snapshot.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), CompleteLevelDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(CompleteLevelDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(CompleteLevelDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases the exact failed save without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched pending save without changing controller state.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), CompleteLevelDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(CompleteLevelDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(CompleteLevelDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}
