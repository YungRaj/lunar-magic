use crate::{AppError, AppState, FrontendEffect, RevisionProfile};

impl AppState {
    pub(crate) fn install_revision_profile(
        &mut self,
        mut profile: RevisionProfile,
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
        if let Some(compression) = detected_smw_graphics_compression(
            identity,
            &self.project.as_ref().ok_or(AppError::NoProject)?.rom,
        )? {
            profile.graphics.compression = compression;
        }
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

fn detected_smw_graphics_compression(
    identity: &lm_rom::RomIdentity,
    rom: &lm_rom::RomImage,
) -> Result<
    Option<lm_project::GraphicsCompression>,
    lm_profile::SmwUsV1GraphicsCompressionDetectError,
> {
    if identity.game != lm_rom::SupportedGame::SuperMarioWorld
        || identity.region != lm_rom::Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != lm_rom::Mapper::LoRom
        || !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(rom)
    {
        return Ok(None);
    }
    Ok(Some(
        match lm_profile::detect_smw_us_v1_graphics_compression_mode(rom)? {
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz3 => lm_project::GraphicsCompression::Lz3,
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original
            | lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed => {
                lm_project::GraphicsCompression::Lz2
            }
        },
    ))
}

#[cfg(test)]
mod compression_tests {
    use super::*;

    #[test]
    #[ignore = "requires retained Rust-produced installed-SMW LZ3 ROM"]
    fn reopened_profile_codec_follows_authenticated_rom_runtime() {
        let bytes = std::fs::read(std::env::var_os("LM_LZ3_APP_RUST_OUTPUT").unwrap()).unwrap();
        let rom = lm_rom::RomImage::from_bytes(bytes).unwrap();
        let identity = lm_rom::detect_identity(&rom).unwrap();
        assert_eq!(
            detected_smw_graphics_compression(&identity, &rom).unwrap(),
            Some(lm_project::GraphicsCompression::Lz3)
        );
    }
}
