use super::{
    Map16DocumentController, Map16DocumentControllerError, Map16DocumentSaveSnapshot, PendingSave,
};

impl Map16DocumentController {
    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid programmatic data, and request counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<Map16DocumentSaveSnapshot, Map16DocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(Map16DocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(Map16DocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(Map16DocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(Map16DocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges only the exact pending snapshot, retaining later edits as dirty.
    ///
    /// # Errors
    ///
    /// A missing or mismatched request leaves controller state retryable.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), Map16DocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(Map16DocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(Map16DocumentControllerError::StaleSave {
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
    /// A missing or mismatched request preserves the pending save.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), Map16DocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(Map16DocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(Map16DocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}
