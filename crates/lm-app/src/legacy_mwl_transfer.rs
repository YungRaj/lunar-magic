use lm_level::LegacyMwlManifest;
use lm_project::LegacyMwlBundle;
use std::path::{Path, PathBuf};

/// Resolves the manifest-declared payloads beside the selected legacy `.mwl` document.
///
/// The manifest model has already rejected absolute paths and path components, so these remain
/// siblings of `manifest_path`.
#[must_use]
pub fn legacy_mwl_sidecar_paths(
    manifest_path: &Path,
    manifest: &LegacyMwlManifest,
) -> Vec<PathBuf> {
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut paths = vec![
        parent.join(&manifest.layer1.file_name),
        parent.join(&manifest.layer2.file_name),
        parent.join(&manifest.sprites.file_name),
    ];
    if manifest.layer1.flags & 1 != 0
        && let Ok(name) = manifest.palette_file_name()
    {
        paths.push(parent.join(name));
    }
    paths
}

/// Publishes a complete legacy multi-file level without replacing existing files.
///
/// Every document is staged before any destination appears. A collision or publication failure
/// rolls back files created by this call.
///
/// # Errors
///
/// Rejects an invalid manifest, a missing required palette, aliased destinations, pre-existing
/// destinations, and staging or publication failures.
pub fn publish_legacy_mwl_bundle_new(
    manifest_path: &Path,
    bundle: &LegacyMwlBundle,
) -> Result<(), String> {
    let manifest_bytes = bundle
        .manifest
        .encode()
        .map_err(|error| error.to_string())?;
    let sidecars = legacy_mwl_sidecar_paths(manifest_path, &bundle.manifest);
    let mut documents = vec![
        (manifest_path.to_path_buf(), manifest_bytes.as_slice()),
        (sidecars[0].clone(), bundle.layer1.as_slice()),
        (sidecars[1].clone(), bundle.layer2.as_slice()),
        (sidecars[2].clone(), bundle.sprites.as_slice()),
    ];
    if bundle.manifest.layer1.flags & 1 != 0 {
        let palette = bundle
            .palette
            .as_deref()
            .ok_or("legacy MWL declares a custom palette but has no palette payload")?;
        documents.push((sidecars[3].clone(), palette));
    }
    let references = documents
        .iter()
        .map(|(path, bytes)| (path.as_path(), *bytes))
        .collect::<Vec<_>>();
    crate::file_persistence::write_new_group(&references).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{LegacyMwlManifest, LegacyMwlSidecar};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn bundle() -> LegacyMwlBundle {
        LegacyMwlBundle {
            manifest: LegacyMwlManifest {
                version: LegacyMwlManifest::CURRENT_VERSION,
                attribution: LegacyMwlBundle::ATTRIBUTION.into(),
                level_number: 0x105,
                header: [0; 5],
                layer1: LegacyMwlSidecar {
                    flags: 0,
                    source_address: 1,
                    file_name: "Level 105.mw0".into(),
                },
                layer2: LegacyMwlSidecar {
                    flags: 0,
                    source_address: 2,
                    file_name: "Level 105.mw1".into(),
                },
                sprites: LegacyMwlSidecar {
                    flags: 0,
                    source_address: 3,
                    file_name: "Level 105.mw2".into(),
                },
                secondary_exits: Vec::new(),
            },
            layer1: vec![1],
            layer2: vec![2],
            sprites: vec![3],
            palette: None,
        }
    }

    fn temporary_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-legacy-mwl-publication-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn grouped_publication_creates_exact_siblings_and_rolls_back_on_collision() {
        let directory = temporary_directory();
        let manifest = directory.join("Level 105.mwl");
        fs::write(directory.join("Level 105.mw1"), b"occupied").unwrap();
        assert!(publish_legacy_mwl_bundle_new(&manifest, &bundle()).is_err());
        assert!(!manifest.exists());
        assert!(!directory.join("Level 105.mw0").exists());
        assert_eq!(
            fs::read(directory.join("Level 105.mw1")).unwrap(),
            b"occupied"
        );
        fs::remove_file(directory.join("Level 105.mw1")).unwrap();
        publish_legacy_mwl_bundle_new(&manifest, &bundle()).unwrap();
        assert_eq!(fs::read(directory.join("Level 105.mw0")).unwrap(), [1]);
        assert_eq!(fs::read(directory.join("Level 105.mw1")).unwrap(), [2]);
        assert_eq!(fs::read(directory.join("Level 105.mw2")).unwrap(), [3]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn custom_palette_publication_adds_exact_mw3_and_keeps_group_atomic() {
        let directory = temporary_directory();
        let manifest = directory.join("Level 105.mwl");
        let mut value = bundle();
        value.manifest.layer1.flags |= 1;
        value.palette = Some(
            (0..LegacyMwlBundle::PALETTE_BYTES)
                .map(|index| index as u8)
                .collect(),
        );
        fs::write(directory.join("Level 105.mw3"), b"occupied").unwrap();
        assert!(publish_legacy_mwl_bundle_new(&manifest, &value).is_err());
        assert!(!manifest.exists());
        assert!(!directory.join("Level 105.mw0").exists());
        fs::remove_file(directory.join("Level 105.mw3")).unwrap();

        publish_legacy_mwl_bundle_new(&manifest, &value).unwrap();
        assert_eq!(
            fs::read(directory.join("Level 105.mw3")).unwrap(),
            value.palette.unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
