//! Asynchronous, revision-bound loading for explicit ROM-allocation ownership evidence.

use crate::document_loader::{BoundedRead, DocumentLoader, LoadedDocument};
use eframe::egui;
use lm_project::{RatsOwnershipManifest, RatsOwnershipManifestFile};

#[derive(Default)]
pub(crate) struct RomOwnershipLoader {
    loader: DocumentLoader,
    revision: Option<u64>,
}

impl RomOwnershipLoader {
    pub(crate) const fn is_running(&self) -> bool {
        self.loader.is_running()
    }

    pub(crate) fn choose_and_start(&mut self, revision: u64) -> Result<bool, String> {
        let Some(path) = crate::dialogs::choose_rats_ownership() else {
            return Ok(false);
        };
        let maximum = u64::try_from(RatsOwnershipManifestFile::MAX_FILE_LEN)
            .map_err(|_| "RATS manifest size bound is not representable".to_owned())?;
        self.loader.start(vec![BoundedRead::new(
            path,
            maximum,
            "RATS ownership manifest",
        )])?;
        self.revision = Some(revision);
        Ok(true)
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        current_revision: u64,
    ) -> Option<Result<RatsOwnershipManifest, String>> {
        let result = self.loader.show(context)?;
        let loaded_revision = self.revision.take();
        Some(result.and_then(|loaded| {
            let loaded_revision = loaded_revision
                .ok_or_else(|| "RATS manifest load lost its project revision".to_string())?;
            crate::rom_load::ensure_current_revision(
                loaded_revision,
                current_revision,
                "RATS ownership evidence",
            )?;
            decode_loaded(loaded)
        }))
    }
}

fn decode_loaded(loaded: LoadedDocument) -> Result<RatsOwnershipManifest, String> {
    let [(_, bytes)] = loaded.into_exact::<1>("RATS manifest")?;
    RatsOwnershipManifestFile::decode(&bytes)
        .map(|file| file.0)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::RatsBlock;
    use std::path::PathBuf;

    #[test]
    fn decodes_one_canonical_manifest_and_rejects_bad_groups() {
        let manifest = RatsOwnershipManifest {
            owned: vec![RatsBlock {
                header_offset: 0x100,
                payload: 0x108..0x120,
            }],
            retained: Vec::new(),
        };
        let bytes = RatsOwnershipManifestFile(manifest.clone())
            .encode()
            .unwrap();
        assert_eq!(
            decode_loaded(LoadedDocument {
                files: vec![(PathBuf::from("ownership.lmrats"), bytes)],
            })
            .unwrap(),
            manifest
        );
        assert!(decode_loaded(LoadedDocument { files: Vec::new() }).is_err());
        assert!(
            decode_loaded(LoadedDocument {
                files: vec![(PathBuf::from("ownership.lmrats"), b"bad".to_vec())],
            })
            .is_err()
        );
    }
}
