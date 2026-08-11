use crate::{AppError, AppState, EditorMode};

/// Exact dirty-ROM state retained by a native frontend across an abnormal exit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySnapshot {
    pub revision: u64,
    pub level: Option<u16>,
    pub saved_baseline: Vec<u8>,
    pub current_rom: Vec<u8>,
}

impl AppState {
    /// Captures a recovery record only while the open ROM differs from its save baseline.
    #[must_use]
    pub fn recovery_snapshot(&self) -> Option<RecoverySnapshot> {
        let project = self.project.as_ref()?;
        project.is_modified().then(|| RecoverySnapshot {
            revision: self.project_revision,
            level: self.current_level(),
            saved_baseline: project.saved_baseline_snapshot(),
            current_rom: project.save_snapshot(),
        })
    }

    /// Captures recovery state after applying one revision-bound editor mutation to an isolated
    /// clone of the current project.
    ///
    /// Native editor forms use this for changes that are intentionally staged outside
    /// [`AppState`]. The live project, its history, and its save baseline remain untouched.
    ///
    /// # Errors
    ///
    /// Returns an error when no project is open or the staged mutation is incompatible with the
    /// current ROM snapshot.
    pub fn recovery_snapshot_with_mutation(
        &self,
        mutation: &lm_project::RomMutation,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        staged
            .apply_mutation("Stage editor state for crash recovery", mutation)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Validates and captures an editor-composed physical ROM without publishing it to the live
    /// application project.
    pub fn recovery_snapshot_with_current_rom(
        &self,
        current_rom: Vec<u8>,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let saved_baseline = project.saved_baseline_snapshot();
        let staged =
            lm_project::Project::open_recovered(saved_baseline.clone(), current_rom.clone())
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        Ok(staged.is_modified().then(|| RecoverySnapshot {
            revision: self.project_revision,
            level,
            saved_baseline,
            current_rom,
        }))
    }

    /// Composes an optional staged ROM mutation and staged native overworld route links on one
    /// isolated project before validating the recovery image.
    pub fn recovery_snapshot_with_overworld_path_links(
        &self,
        mutation: Option<&lm_project::RomMutation>,
        links: &lm_overworld::OverworldPathLinkTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        if let Some(mutation) = mutation {
            staged
                .apply_mutation("Stage overworld terrain for crash recovery", mutation)
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        }
        crate::overworld_path_link_state::replace_native_path_links_in_project(&mut staged, links)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Restores a recovery record as an unnamed, dirty project.
    ///
    /// The original path is deliberately not restored. The first explicit save therefore uses
    /// Save As and cannot silently replace the source ROM.
    ///
    /// # Errors
    ///
    /// Returns an error when a project is already open, either image is malformed, the current
    /// ROM is unsupported, or the record contains no unsaved change.
    pub fn load_recovery(&mut self, snapshot: RecoverySnapshot) -> Result<(), AppError> {
        if self.project.is_some() {
            return Err(AppError::ProjectAlreadyOpen);
        }
        if snapshot.level.is_some_and(|level| level > 0x1ff) {
            return Err(AppError::Recovery(
                "recovery record contains an invalid level".into(),
            ));
        }
        let project =
            lm_project::Project::open_recovered(snapshot.saved_baseline, snapshot.current_rom)
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        if !project.is_modified() {
            return Err(AppError::Recovery(
                "recovery record does not contain unsaved changes".into(),
            ));
        }
        self.install_project(project, None);
        if let Some(level) = snapshot.level {
            self.mode = EditorMode::Level(level);
            self.level_navigation.visit(level);
        }
        self.status = "Recovered unsaved ROM; use Save As to preserve it".into();
        Ok(())
    }
}
