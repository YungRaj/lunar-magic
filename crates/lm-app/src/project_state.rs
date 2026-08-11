use crate::{
    AppError, AppState, ControllerSnapshot, FrontendEffect, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_project::{EditKind, RomMutation, RomWrite};

impl AppState {
    pub(crate) fn change_history(&mut self, undo: bool) -> Result<Vec<FrontendEffect>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        if (undo && project.history.can_undo()) || (!undo && project.history.can_redo()) {
            self.ensure_project_revision_capacity()?;
        }
        let history_kind = if undo {
            project.history.undo_kind()
        } else {
            project.history.redo_kind()
        };
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed = if undo {
            project.undo()?
        } else {
            project.redo()?
        };
        let (description, empty_status) = if undo {
            ("Undo", "Nothing to undo")
        } else {
            ("Redo", "Nothing to redo")
        };
        self.status = if changed {
            format!("{description} completed")
        } else {
            empty_status.into()
        };
        if changed {
            if let Some(EditKind::GraphicsCompressionMigration { source, target }) = history_kind {
                self.set_profile_graphics_compression(if undo { source } else { target });
            }
            self.advance_project_revision()?;
        }
        Ok(changed
            .then(|| FrontendEffect::ProjectChanged {
                description: description.into(),
                mode: self.mode,
                revision: self.project_revision,
            })
            .into_iter()
            .collect())
    }

    fn set_profile_graphics_compression(&mut self, compression: lm_project::GraphicsCompression) {
        if let Some(profile) = self.revision_profile.as_mut() {
            profile.graphics.compression = compression;
        }
    }

    pub(crate) fn commit_rom_writes(
        &mut self,
        expected_revision: u64,
        description: String,
        writes: &[RomWrite],
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if description.trim().is_empty() {
            return Err(AppError::EmptyEditDescription);
        }
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        let writes = self.checksum_policy_writes(writes);
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        if !project.writes_would_change(&writes)? {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed = project.apply_writes(description.clone(), &writes)?;
        debug_assert!(changed);
        self.advance_project_revision()?;
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn commit_rom_mutation(
        &mut self,
        expected_revision: u64,
        description: String,
        mutation: &RomMutation,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if description.trim().is_empty() {
            return Err(AppError::EmptyEditDescription);
        }
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        let mutation = self.checksum_policy_mutation(mutation);
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        if !project.mutation_would_change(&mutation)? {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed = project.apply_mutation(description.clone(), &mutation)?;
        debug_assert!(changed);
        self.advance_project_revision()?;
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    fn checksum_policy_mutation(&self, mutation: &RomMutation) -> RomMutation {
        let mut mutation = mutation.clone();
        mutation.writes = self.checksum_policy_writes(&mutation.writes);
        mutation
    }

    fn checksum_policy_writes(&self, writes: &[RomWrite]) -> Vec<RomWrite> {
        if self.maintain_checksum() {
            return writes.to_vec();
        }
        let Some(project) = self.project.as_ref() else {
            return writes.to_vec();
        };
        let Some(identity) = project.identity.as_ref() else {
            return writes.to_vec();
        };
        exclude_write_range(
            writes,
            identity.internal_header_offset + 0x1c..identity.internal_header_offset + 0x20,
        )
    }

    pub(crate) fn advance_project_revision(&mut self) -> Result<(), AppError> {
        self.project_revision = self
            .project_revision
            .checked_add(1)
            .ok_or(AppError::ProjectRevisionOverflow)?;
        Ok(())
    }

    pub(crate) fn ensure_project_revision_capacity(&self) -> Result<(), AppError> {
        if self.project_revision == u64::MAX {
            Err(AppError::ProjectRevisionOverflow)
        } else {
            Ok(())
        }
    }

    /// Returns the revision controllers must attach to decoded edit results.
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    /// Captures one self-consistent input for asynchronous decoding or rendering.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NoProject`] if no document is open. A supported open project always
    /// has an identity, so absence is treated as the same invalid shell state.
    pub fn controller_snapshot(&self) -> Result<ControllerSnapshot, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let identity = project.identity.clone().ok_or(AppError::NoProject)?;
        Ok(ControllerSnapshot {
            revision: self.project_revision,
            mode: self.mode,
            identity,
            document_path: self.document_path.clone(),
            rom_bytes: project.rom.as_file_bytes().to_vec(),
        })
    }

    /// Captures ROM bytes and the active validated profile under one application revision.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NoProject`] without an open ROM or [`AppError::NoRevisionProfile`]
    /// until a matching external profile has been installed.
    pub fn profiled_controller_snapshot(&self) -> Result<ProfiledControllerSnapshot, AppError> {
        let snapshot = self.controller_snapshot()?;
        let profile = self
            .revision_profile
            .clone()
            .ok_or(AppError::NoRevisionProfile)?;
        debug_assert!(profile.matches_identity(&snapshot.identity));
        Ok(ProfiledControllerSnapshot { snapshot, profile })
    }

    /// Returns the currently installed external revision metadata.
    #[must_use]
    pub const fn revision_profile(&self) -> Option<&RevisionProfile> {
        self.revision_profile.as_ref()
    }

    /// Returns the level whose view state remains active while another editor is open.
    ///
    /// Lunar Magic's graphics editor operates on the globally active level even though the
    /// graphics window has its own selected GFX page. Keeping these identities separate prevents
    /// graphics-page numbers from being interpreted as level numbers.
    #[must_use]
    pub fn current_level(&self) -> Option<u16> {
        self.level_navigation.current().map(|state| state.level)
    }
}

fn exclude_write_range(writes: &[RomWrite], excluded: std::ops::Range<usize>) -> Vec<RomWrite> {
    let mut retained = Vec::with_capacity(writes.len());
    for write in writes {
        let end = write.offset.saturating_add(write.bytes.len());
        if end <= excluded.start || excluded.end <= write.offset {
            retained.push(write.clone());
            continue;
        }
        if write.offset < excluded.start {
            retained.push(RomWrite {
                offset: write.offset,
                bytes: write.bytes[..excluded.start - write.offset].to_vec(),
            });
        }
        if excluded.end < end {
            retained.push(RomWrite {
                offset: excluded.end,
                bytes: write.bytes[excluded.end - write.offset..].to_vec(),
            });
        }
    }
    retained
}
