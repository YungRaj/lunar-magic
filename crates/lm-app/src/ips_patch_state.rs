use crate::{AppError, AppState, FrontendEffect};
use lm_rom::{RomIdentity, RomImage, apply_ips, detect_identity};

impl AppState {
    pub(crate) fn apply_ips_patch(
        &mut self,
        expected_revision: u64,
        patch: &[u8],
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let identity = project
            .identity
            .as_ref()
            .ok_or(AppError::NoProject)?
            .clone();
        let target = apply_ips(project.rom.logical_bytes(), patch)?;
        if target == project.rom.logical_bytes() {
            return Ok(Vec::new());
        }
        let target_identity = detect_identity(&RomImage::from_bytes(target.clone())?)?;
        if !stable_identity_matches(&identity, &target_identity) {
            return Err(AppError::IpsIdentityMismatch);
        }
        self.ensure_project_revision_capacity()?;
        let description = format!(
            "Apply IPS patch ({} → {} logical bytes)",
            project.rom.logical_len(),
            target.len()
        );
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed =
            project.apply_logical_replacement(description.clone(), identity.mapper, &target)?;
        debug_assert!(changed);
        self.revision_profile = None;
        self.advance_project_revision()?;
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

fn stable_identity_matches(before: &RomIdentity, after: &RomIdentity) -> bool {
    before.game == after.game
        && before.mapper == after.mapper
        && before.region == after.region
        && before.revision == after.revision
        && before.map_mode == after.map_mode
        && before.cartridge_type == after.cartridge_type
        && before.internal_header_offset == after.internal_header_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_rom::{SnesChecksum, compute_snes_checksum, create_ips};
    use std::{fs, path::PathBuf};

    fn original() -> Vec<u8> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read(root.join("Super Mario World (USA).sfc")).unwrap()
    }

    #[test]
    fn exact_patch_is_revisioned_checksum_coherent_and_undoable() {
        let source = original();
        let mut target = source.clone();
        target[0x12345] ^= 0x5a;
        let checksum = compute_snes_checksum(&target, 0x7fdc).unwrap();
        target[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let patch = create_ips(&source, &target).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::ApplyIpsPatch {
            rev: app.project_revision(),
            patch,
        })
        .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), target);
        let identity = app.project().unwrap().identity.as_ref().unwrap();
        assert_eq!(identity.stored_checksum, checksum);
        assert_eq!(identity.computed_checksum, checksum);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
    }

    #[test]
    fn aligned_shrink_and_redo_are_exact() {
        let target = original();
        let mut source = target.clone();
        source.resize(0x10_0000, 0xff);
        let patch = create_ips(&source, &target).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::ApplyIpsPatch { rev: 0, patch })
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), target);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), target);
    }

    #[test]
    fn malformed_stale_and_identity_changing_patches_are_atomic() {
        let source = original();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        assert!(
            app.dispatch(Command::ApplyIpsPatch {
                rev: 0,
                patch: b"bad".to_vec(),
            })
            .is_err()
        );
        let mut foreign = source.clone();
        foreign[0x7fc0] = b'X';
        let patch = create_ips(&source, &foreign).unwrap();
        assert!(
            app.dispatch(Command::ApplyIpsPatch { rev: 0, patch })
                .is_err()
        );
        assert!(
            app.dispatch(Command::ApplyIpsPatch {
                rev: 1,
                patch: b"PATCHEOF".to_vec(),
            })
            .is_err()
        );
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().save_snapshot(), source);
    }

    #[test]
    fn no_op_patch_preserves_revision_and_history() {
        let source = original();
        let patch = create_ips(&source, &source).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        assert!(
            app.dispatch(Command::ApplyIpsPatch { rev: 0, patch })
                .unwrap()
                .is_empty()
        );
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        assert_eq!(
            SnesChecksum::decode(&source, 0x7fdc).unwrap(),
            app.project()
                .unwrap()
                .identity
                .as_ref()
                .unwrap()
                .stored_checksum
        );
    }
}
