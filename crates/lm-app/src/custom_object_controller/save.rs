use super::{
    CustomObjectControllerError, CustomObjectLibraryController, CustomObjectSaveSnapshot,
    PendingCustomObjectSave,
};

impl CustomObjectLibraryController {
    /// Captures exact paired bytes for frontend persistence and reserves the save slot.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectControllerError::SavePending`] for overlapping saves or a library
    /// encoding error for invalid programmatic state.
    pub fn begin_save(&mut self) -> Result<CustomObjectSaveSnapshot, CustomObjectControllerError> {
        if self.pending_save.is_some() {
            return Err(CustomObjectControllerError::SavePending);
        }
        let (data, descriptions) = self
            .library
            .encode()
            .map_err(CustomObjectControllerError::Library)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(CustomObjectControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingCustomObjectSave {
            request_id,
            library: self.library.clone(),
        });
        Ok(CustomObjectSaveSnapshot {
            request_id,
            revision: self.revision,
            data_path: self.data_path.clone(),
            descriptions_path: self.descriptions_path.clone(),
            data,
            descriptions,
        })
    }

    /// Acknowledges that a frontend atomically persisted the exact pending snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectControllerError`] if no save is pending or its token is wrong. A
    /// newer document revision remains modified even if an older snapshot reached disk.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), CustomObjectControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(CustomObjectControllerError::NoPendingSave)?;
        if request_id != pending.request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(CustomObjectControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.library;
        Ok(())
    }

    /// Releases a failed or cancelled frontend persistence attempt for an immediate retry.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectControllerError::NoPendingSave`] if there is nothing to cancel.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), CustomObjectControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(CustomObjectControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(CustomObjectControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}
