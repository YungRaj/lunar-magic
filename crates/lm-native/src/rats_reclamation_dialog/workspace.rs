use crate::level_editor_forms::parse_hex_u8;
use lm_app::{AppState, Command};
use lm_project::RatsOwnershipManifest;

pub(super) struct RatsReclamationWorkspace {
    revision: u64,
    manifest: RatsOwnershipManifest,
    pub fill: String,
    pub reclaimed_blocks: usize,
    pub reclaimed_bytes: usize,
    pub retained_blocks: usize,
}

impl RatsReclamationWorkspace {
    pub(super) fn load(app: &AppState, manifest: RatsOwnershipManifest) -> Result<Self, String> {
        let plan = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())?
            .plan_rats_reclamation(&manifest, 0xff)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            revision: app.project_revision(),
            retained_blocks: manifest.retained.len(),
            manifest,
            fill: "FF".into(),
            reclaimed_blocks: plan.reclaimed.len(),
            reclaimed_bytes: plan.reclaimed_bytes,
        })
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare(&self, project_revision: u64) -> Result<Command, String> {
        if self.is_stale(project_revision) {
            return Err(
                "the ROM changed after the ownership manifest was loaded; reopen it".into(),
            );
        }
        if self.reclaimed_blocks == 0 {
            return Err(
                "the manifest retains every owned block; there is nothing to reclaim".into(),
            );
        }
        Ok(Command::ReclaimOwnedRats {
            rev: self.revision,
            manifest: Box::new(self.manifest.clone()),
            fill: parse_hex_u8(&self.fill, "RATS reclamation fill byte")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::RatsOwnershipManifest;
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator};
    use std::path::PathBuf;

    fn fixture() -> (AppState, RatsOwnershipManifest) {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        bytes.resize(0x10_0000, 0xff);
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x8_0000..0x10_0000));
        let dead = allocator.allocate(&[1, 2, 3]).unwrap();
        let live = allocator.allocate(&[4, 5]).unwrap();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        (
            app,
            RatsOwnershipManifest {
                owned: vec![dead, live.clone()],
                retained: vec![live],
            },
        )
    }

    #[test]
    fn validated_preview_and_command_share_manifest_identity() {
        let (app, manifest) = fixture();
        let mut workspace = RatsReclamationWorkspace::load(&app, manifest).unwrap();
        assert_eq!(workspace.reclaimed_blocks, 1);
        assert_eq!(workspace.retained_blocks, 1);
        assert!(workspace.reclaimed_bytes > 8);
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::ReclaimOwnedRats { fill: 0xff, .. }
        ));
        assert!(workspace.prepare(app.project_revision() + 1).is_err());
        workspace.fill = "100".into();
        assert!(workspace.prepare(app.project_revision()).is_err());
    }
}
