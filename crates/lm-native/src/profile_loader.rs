//! Non-blocking bounded loading for identity-bound revision profiles.

use crate::document_loader::{BoundedRead, DocumentLoader, LoadedDocument};
use eframe::egui;
use lm_app::RevisionProfile;
use std::path::Path;

/// Loads a startup-supplied profile before the native event loop exists.
pub(crate) fn read(path: &Path) -> Result<RevisionProfile, Box<dyn std::error::Error>> {
    Ok(RevisionProfile::read_from(std::fs::File::open(path)?)?)
}

#[derive(Default)]
pub(crate) struct ProfileLoader {
    loader: DocumentLoader,
}

impl ProfileLoader {
    pub(crate) fn is_running(&self) -> bool {
        self.loader.is_running()
    }

    pub(crate) fn choose_and_start(&mut self) -> Result<bool, String> {
        let Some(path) = crate::dialogs::choose_revision_profile() else {
            return Ok(false);
        };
        self.loader.start(vec![BoundedRead::new(
            path,
            RevisionProfile::MAX_TEXT_LEN as u64,
            "revision profile",
        )])?;
        Ok(true)
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<RevisionProfile, String>> {
        self.loader.show(context).map(|result| decode(result?))
    }
}

fn decode(loaded: LoadedDocument) -> Result<RevisionProfile, String> {
    let [(_, bytes)] = loaded.into_exact::<1>("revision-profile")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("revision profile is not UTF-8: {error}"))?;
    RevisionProfile::parse(text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn loaded(files: Vec<Vec<u8>>) -> LoadedDocument {
        LoadedDocument {
            files: files
                .into_iter()
                .enumerate()
                .map(|(index, bytes)| (PathBuf::from(index.to_string()), bytes))
                .collect(),
        }
    }

    #[test]
    fn rejects_non_utf8_before_profile_parsing() {
        let error = decode(loaded(vec![vec![0xff]])).unwrap_err();
        assert!(error.contains("not UTF-8"));
    }

    #[test]
    fn rejects_empty_and_ambiguous_groups() {
        assert!(decode(loaded(Vec::new())).is_err());
        assert!(decode(loaded(vec![Vec::new(), Vec::new()])).is_err());
    }

    #[test]
    fn rejects_malformed_utf8_profile_text() {
        assert!(decode(loaded(vec![b"LMREVPRO1\n".to_vec()])).is_err());
    }
}
