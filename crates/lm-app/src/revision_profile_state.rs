use crate::{AppError, AppState, FrontendEffect, RevisionProfile};

impl AppState {
    pub(crate) fn install_revision_profile(
        &mut self,
        profile: RevisionProfile,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        profile.validate()?;
        let identity = self
            .project
            .as_ref()
            .ok_or(AppError::NoProject)?
            .identity
            .as_ref()
            .ok_or(AppError::NoProject)?;
        profile.ensure_identity(identity)?;
        profile.audit_rom(&self.project.as_ref().ok_or(AppError::NoProject)?.rom)?;
        if self.revision_profile.as_ref() == Some(&profile) {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let name = profile.name.clone();
        self.revision_profile = Some(profile);
        self.selection = None;
        self.advance_project_revision()?;
        self.status = format!("Revision profile: {name}");
        Ok(vec![FrontendEffect::RevisionProfileChanged {
            name: Some(name),
            revision: self.project_revision,
        }])
    }

    pub(crate) fn clear_revision_profile(&mut self) -> Result<Vec<FrontendEffect>, AppError> {
        self.project.as_ref().ok_or(AppError::NoProject)?;
        if self.revision_profile.is_none() {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        self.revision_profile = None;
        self.selection = None;
        self.advance_project_revision()?;
        self.status = "Revision profile cleared".into();
        Ok(vec![FrontendEffect::RevisionProfileChanged {
            name: None,
            revision: self.project_revision,
        }])
    }
}
