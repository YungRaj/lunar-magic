use crate::{AppError, AppState, FrontendEffect};
use lm_project::RatsOwnershipManifest;

impl AppState {
    pub(crate) fn reclaim_owned_rats(
        &mut self,
        expected_revision: u64,
        manifest: &RatsOwnershipManifest,
        fill: u8,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        let checksum_field = identity.internal_header_offset + 0x1c;
        let plan = project.plan_rats_reclamation(manifest, fill)?;
        if plan.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let description = format!(
            "Reclaim {} owned RATS block{} ({} bytes)",
            plan.reclaimed.len(),
            if plan.reclaimed.len() == 1 { "" } else { "s" },
            plan.reclaimed_bytes
        );
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        project.reclaim_owned_rats_with_checksum(
            description.clone(),
            manifest,
            fill,
            checksum_field,
        )?;
        self.advance_project_revision()?;
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
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator, parse_at};
    use lm_rom::{SnesChecksum, compute_snes_checksum};
    use std::{fs, path::PathBuf};

    fn app_with_owned_blocks() -> (AppState, Vec<u8>, lm_rats::RatsBlock, lm_rats::RatsBlock) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut bytes = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        bytes.resize(0x10_0000, 0xff);
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x8_0000..0x10_0000));
        let reclaimed = allocator.allocate(&[0x11; 32]).unwrap();
        let retained = allocator.allocate(&[0x22; 16]).unwrap();
        let original = bytes.clone();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        (app, original, reclaimed, retained)
    }

    #[test]
    fn ownership_proven_reclamation_repairs_checksum_and_undoes_exactly() {
        let (mut app, original, reclaimed, retained) = app_with_owned_blocks();
        let manifest = RatsOwnershipManifest {
            owned: vec![reclaimed.clone(), retained.clone()],
            retained: vec![retained.clone()],
        };
        app.dispatch(Command::ReclaimOwnedRats {
            rev: app.project_revision(),
            manifest: Box::new(manifest),
            fill: 0xa5,
        })
        .unwrap();
        let project = app.project().unwrap();
        assert!(
            project.rom.logical_bytes()[reclaimed.full_range()]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        assert_eq!(
            parse_at(project.rom.logical_bytes(), retained.header_offset).unwrap(),
            retained
        );
        let checksum_field = project.identity.as_ref().unwrap().internal_header_offset + 0x1c;
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), checksum_field).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), checksum_field).unwrap()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn stale_and_invalid_manifests_leave_revision_and_rom_unchanged() {
        let (mut app, original, reclaimed, _) = app_with_owned_blocks();
        let revision = app.project_revision();
        let manifest = RatsOwnershipManifest {
            owned: vec![reclaimed.clone()],
            retained: Vec::new(),
        };
        assert!(
            app.dispatch(Command::ReclaimOwnedRats {
                rev: revision + 1,
                manifest: Box::new(manifest.clone()),
                fill: 0xff,
            })
            .is_err()
        );
        let mut stale = manifest;
        stale.owned[0].payload.end += 1;
        assert!(
            app.dispatch(Command::ReclaimOwnedRats {
                rev: revision,
                manifest: Box::new(stale),
                fill: 0xff,
            })
            .is_err()
        );
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn empty_plan_is_a_revision_preserving_no_op() {
        let (mut app, original, _, retained) = app_with_owned_blocks();
        let revision = app.project_revision();
        let effects = app
            .dispatch(Command::ReclaimOwnedRats {
                rev: revision,
                manifest: Box::new(RatsOwnershipManifest {
                    owned: vec![retained.clone()],
                    retained: vec![retained],
                }),
                fill: 0xff,
            })
            .unwrap();
        assert!(effects.is_empty());
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
