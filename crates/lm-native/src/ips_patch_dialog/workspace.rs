use lm_app::{AppState, Command};
use lm_rom::{RomImage, apply_ips};

pub(super) struct IpsPatchWorkspace {
    revision: u64,
    patch: Vec<u8>,
    pub source_len: usize,
    pub target_len: usize,
    pub changed_bytes: usize,
}

impl IpsPatchWorkspace {
    pub(super) fn load(app: &AppState, patch: Vec<u8>) -> Result<Self, String> {
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let image = RomImage::from_bytes(snapshot.rom_bytes).map_err(|error| error.to_string())?;
        let source = image.logical_bytes();
        let target = apply_ips(source, &patch).map_err(|error| error.to_string())?;
        let changed_bytes = source
            .iter()
            .zip(&target)
            .filter(|(left, right)| left != right)
            .count()
            + source.len().abs_diff(target.len());
        Ok(Self {
            revision: snapshot.revision,
            patch,
            source_len: source.len(),
            target_len: target.len(),
            changed_bytes,
        })
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare(&self, project_revision: u64) -> Result<Command, String> {
        if self.is_stale(project_revision) {
            return Err("the ROM changed after this IPS patch was loaded; choose it again".into());
        }
        if self.changed_bytes == 0 {
            return Err("the IPS patch does not change the open ROM".into());
        }
        Ok(Command::ApplyIpsPatch {
            rev: self.revision,
            patch: self.patch.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{compute_snes_checksum, create_ips};
    use std::{fs, path::PathBuf};

    #[test]
    fn real_rom_preview_routes_exact_patch_and_rejects_stale_revision() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut target = source.clone();
        target[0x23456] ^= 0x80;
        let checksum = compute_snes_checksum(&target, 0x7fdc).unwrap();
        target[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let patch = create_ips(&source, &target).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        let workspace = IpsPatchWorkspace::load(&app, patch).unwrap();
        assert_eq!(workspace.source_len, source.len());
        assert_eq!(workspace.target_len, target.len());
        assert!(workspace.changed_bytes >= 1);
        assert!(workspace.prepare(1).is_err());
        app.dispatch(workspace.prepare(0).unwrap()).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), target);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
    }

    #[test]
    fn malformed_and_no_op_patches_never_create_a_commit() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        assert!(IpsPatchWorkspace::load(&app, b"bad".to_vec()).is_err());
        let patch = create_ips(&source, &source).unwrap();
        let workspace = IpsPatchWorkspace::load(&app, patch).unwrap();
        assert_eq!(workspace.changed_bytes, 0);
        assert!(workspace.prepare(0).is_err());
    }
}
