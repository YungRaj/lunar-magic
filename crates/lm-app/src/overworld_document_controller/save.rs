use super::{
    OverworldDocumentController, OverworldDocumentControllerError, OverworldDocumentSaveSnapshot,
    PendingSave,
};

impl OverworldDocumentController {
    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid data, and request counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<OverworldDocumentSaveSnapshot, OverworldDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(OverworldDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode(&self.double_size_modes)
            .map_err(OverworldDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(OverworldDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(OverworldDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges only the exact immutable pending snapshot.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve retryable state.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), OverworldDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(OverworldDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(OverworldDocumentControllerError::StaleSave {
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
    /// Missing or mismatched requests preserve the pending save.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), OverworldDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(OverworldDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(OverworldDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}
