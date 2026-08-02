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
