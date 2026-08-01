use crate::{AppError, AppState, FrontendEffect};
use lm_profile::RevisionPatchTemplate;
use lm_rom::{Mapper, Region, SupportedGame};
use std::ops::Range;

impl AppState {
    pub(crate) fn install(
        &mut self,
        expected_revision: u64,
        layer3: bool,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if layer3 {
            self.install_layer3(expected_revision)
        } else {
            self.install_settings(expected_revision)
        }
    }

    pub(crate) fn install_layer3(
        &mut self,
        expected_revision: u64,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Layer3IdentityMismatch);
        }
        let settings_installed = lm_profile::load_smw_us_v1_overworld_settings(project)
            .map_err(|error| AppError::NativeOverworldSettingsStorage(error.to_string()))?
            .installed;
        let plans = if settings_installed {
            vec![
                lm_profile::smw_us_v1_complete_layer3_installation_plan()
                    .map_err(lm_profile::CompleteLayer3BuildError::Runtime)?,
            ]
        } else {
            lm_profile::smw_us_v1_complete_layer3_feature_plans()?
        };
        project
            .install_relocatable_patch_group("install complete SMW US Layer 3 feature", &plans)?;
        self.advance_project_revision()?;
        let description = "Install complete SMW US Layer 3 runtime".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_settings(
        &mut self,
        expected_revision: u64,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::ExpandedSettingsIdentityMismatch);
        }
        let plan = lm_profile::smw_us_v1_expanded_settings_installation_plan()?;
        project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = "Install SMW US expanded level settings".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_lfix3(
        &mut self,
        expected_revision: u64,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::Lfix3IdentityMismatch);
        }
        let plan = lm_profile::smw_us_v1_builtin_lfix3_installation_plan()?;
        project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = "Install SMW US Lfix3 core runtime".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn install_revision_patch(
        &mut self,
        expected_revision: u64,
        template: &RevisionPatchTemplate,
        search: Range<usize>,
        fill: u8,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let profile = self
            .revision_profile
            .as_ref()
            .ok_or(AppError::NoRevisionProfile)?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        let plan = template.installation_plan(
            profile,
            &project.rom,
            search,
            identity.internal_header_offset,
            identity.internal_header_offset + 0x1c,
            fill,
        )?;
        let result = project.install_relocatable_patch(&plan)?;
        self.advance_project_revision()?;
        let description = format!(
            "Install revision patch {} ({} payloads)",
            template.name,
            result.blocks.len()
        );
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_profile::RevisionProfile;
    use lm_project::{PatchFixup, PatchPayload, PatchWrite};
    use lm_rom::{Mapper, pc_to_snes};
    use std::{fs, path::PathBuf};

    fn write_pointer(bytes: &mut [u8], offset: usize, mapper: Mapper, target: usize) {
        bytes[offset..offset + 3]
            .copy_from_slice(&pc_to_snes(mapper, target).unwrap().to_le_bytes()[..3]);
    }

    fn fixture(profile: &RevisionProfile) -> Vec<u8> {
        let mut bytes = vec![0xff; 0x4_0000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = match profile.mapper {
            Mapper::LoRom => 0x20,
            Mapper::ExLoRom => 0x32,
            Mapper::Sa1 => 0x23,
        };
        bytes[0x7fd9] = 1;
        bytes[0x7fdb] = profile.revision;
        for table in [
            profile.level.layer1,
            profile.level.sprites.low_or_contiguous_table(),
            profile.map16.graphics,
            profile.map16.acts_like,
            profile.graphics.pointers,
            profile.palette.pointers,
            profile.exanimation.pointers,
            profile.overworld.layers.layer1,
            profile.overworld.layers.layer2,
            profile.overworld.event_reveals.sources,
            profile.overworld.event_reveals.destinations,
            profile.overworld.endpoints.pointers,
            profile.overworld.messages.pointers,
            profile.overworld.sprites.pointers,
            profile.overworld.palette.pointers,
            profile.overworld.animation.pointers,
        ] {
            for index in 0..table.entries {
                write_pointer(
                    &mut bytes,
                    table.offset + index * table.stride,
                    profile.mapper,
                    0x1_0000,
                );
            }
        }
        bytes
    }

    fn template(profile: &RevisionProfile) -> RevisionPatchTemplate {
        RevisionPatchTemplate {
            name: "test runtime".into(),
            game: profile.game,
            region: profile.region,
            revision: profile.revision,
            mapper: profile.mapper,
            payloads: vec![PatchPayload {
                bytes: vec![0xaa; 8],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 0x80,
                expected: vec![0xff; 4],
                replacement: vec![0x22, 0, 0, 0],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: lm_project::PatchFixupEncoding::Long24,
                }],
            }],
        }
    }

    #[test]
    fn application_install_is_revision_checked_undoable_and_failure_atomic() {
        let profile = lm_profile::test_support::profile();
        let mut app = AppState::default();
        app.load_rom(fixture(&profile)).unwrap();
        app.dispatch(Command::InstallRevisionProfile(Box::new(profile.clone())))
            .unwrap();
        let revision = app.controller_snapshot().unwrap().revision;

        let effects = app
            .dispatch(Command::InstallRevisionPatch {
                expected_revision: revision,
                template: Box::new(template(&profile)),
                search: 0x3_0000..0x4_0000,
                fill: 0xff,
            })
            .unwrap();

        assert_eq!(effects.len(), 1);
        assert_eq!(app.project().unwrap().rom.read(0x80, 1).unwrap(), &[0x22]);
        let installed_revision = app.controller_snapshot().unwrap().revision;
        assert!(installed_revision > revision);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.read(0x80, 4).unwrap(),
            &[0xff; 4]
        );
        let before = app.project().unwrap().rom.logical_bytes().to_vec();
        assert!(
            app.dispatch(Command::InstallRevisionPatch {
                expected_revision: revision,
                template: Box::new(template(&profile)),
                search: 0x3_0000..0x4_0000,
                fill: 0xff,
            })
            .is_err()
        );
        assert_eq!(app.project().unwrap().rom.logical_bytes(), before);
    }

    #[test]
    fn application_rejects_patch_without_project_or_profile() {
        let profile = lm_profile::test_support::profile();
        let mut app = AppState::default();
        assert!(
            app.dispatch(Command::InstallRevisionPatch {
                expected_revision: 0,
                template: Box::new(template(&profile)),
                search: 0x8000..0x1_0000,
                fill: 0xff,
            })
            .is_err()
        );
        assert!(app.project().is_none());
    }

    #[test]
    fn application_installs_complete_layer3_family_as_one_revision() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        let mut app = AppState::default();
        app.load_rom(before).unwrap();
        let original = app.project().unwrap().rom.logical_bytes().to_vec();
        let revision = app.controller_snapshot().unwrap().revision;
        let effects = app
            .dispatch(Command::InstallLayer3 { rev: revision })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert!(app.controller_snapshot().unwrap().revision > revision);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes(), original);
    }

    #[test]
    fn layer3_reuses_an_already_installed_expanded_settings_prerequisite() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();
        let settings_snapshot = app.project().unwrap().save_snapshot();
        app.dispatch(Command::InstallLayer3 { rev: 1 }).unwrap();
        assert_eq!(app.project().unwrap().history.undo_len(), 2);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), settings_snapshot);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
