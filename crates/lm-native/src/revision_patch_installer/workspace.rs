use crate::{level_editor_forms::parse_hex_u8, rom_allocation};
use lm_app::{Command, RevisionProfile};
use lm_profile::RevisionPatchTemplate;

pub(super) struct RevisionPatchWorkspace {
    revision: u64,
    pub template: RevisionPatchTemplate,
    pub search_start: String,
    pub search_end: String,
    pub fill: String,
}

impl RevisionPatchWorkspace {
    pub(super) fn new(
        revision: u64,
        profile: &RevisionProfile,
        template: RevisionPatchTemplate,
    ) -> Result<Self, String> {
        template
            .ensure_profile(profile)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            revision,
            template,
            search_start: "080000".into(),
            search_end: "400000".into(),
            fill: "FF".into(),
        })
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare(&self, project_revision: u64) -> Result<Command, String> {
        if self.is_stale(project_revision) {
            return Err(
                "the ROM changed after the revision patch was loaded; choose it again".into(),
            );
        }
        Ok(Command::InstallRevisionPatch {
            expected_revision: self.revision,
            template: Box::new(self.template.clone()),
            search: rom_allocation::parse_search_range(&self.search_start, &self.search_end)?,
            fill: parse_hex_u8(&self.fill, "patch expansion fill byte")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{PatchPayload, PatchWrite};

    fn template(profile: &RevisionProfile) -> RevisionPatchTemplate {
        RevisionPatchTemplate {
            name: "native test patch".into(),
            game: profile.game,
            region: profile.region,
            revision: profile.revision,
            mapper: profile.mapper,
            payloads: vec![PatchPayload {
                bytes: vec![0xaa],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 0x100,
                expected: vec![0xff],
                replacement: vec![0x00],
                fixups: Vec::new(),
            }],
        }
    }

    #[test]
    fn decoded_template_is_profile_and_revision_bound() {
        let profile = lm_profile::test_support::profile();
        let canonical =
            RevisionPatchTemplate::decode(&template(&profile).encode().unwrap()).unwrap();
        let mut workspace = RevisionPatchWorkspace::new(12, &profile, canonical).unwrap();
        assert!(matches!(
            workspace.prepare(12).unwrap(),
            Command::InstallRevisionPatch {
                expected_revision: 12,
                ..
            }
        ));
        assert!(workspace.prepare(13).is_err());
        workspace.search_end = "080000".into();
        assert!(workspace.prepare(12).is_err());
        workspace.search_end = "400000".into();
        workspace.fill = "100".into();
        assert!(workspace.prepare(12).is_err());
    }

    #[test]
    fn foreign_template_is_rejected_before_a_workspace_exists() {
        let profile = lm_profile::test_support::profile();
        let mut foreign = template(&profile);
        foreign.revision ^= 1;
        assert!(RevisionPatchWorkspace::new(0, &profile, foreign).is_err());
    }
}
