use crate::{AppError, AppState, FrontendEffect};
use lm_project::LevelAccessRestrictionKeys;

impl AppState {
    pub(crate) fn restrict_level_access(
        &mut self,
        expected_revision: u64,
        title: &str,
        keys: LevelAccessRestrictionKeys,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let layout = match project.identity.as_ref().map(|identity| identity.mapper) {
            Some(lm_rom::Mapper::ExLoRom) => {
                lm_profile::smw_us_v1_exlorom_level_access_restriction_layout()
            }
            _ => lm_profile::smw_us_v1_level_access_restriction_layout(),
        };
        project.restrict_level_access(title, keys, layout)?;
        self.advance_project_revision()?;
        let description = "Restrict level access".to_owned();
        self.status =
            "Level access is restricted. Save this ROM to a new path and retain your backup."
                .into();
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}
