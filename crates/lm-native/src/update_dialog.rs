use eframe::egui;
use lm_app::{LocalizationCatalog, UiTextKey};
use lm_update::{MAX_ARCHIVE_BYTES, MAX_MANIFEST_BYTES, UpdateManifest, Version};
use std::{fs, path::PathBuf};

use crate::frontend_ui::localized_text;

struct Offer {
    manifest: UpdateManifest,
    archive: PathBuf,
}

struct StagedOffer {
    manifest: UpdateManifest,
    archive: PathBuf,
}

struct InstalledOffer {
    directory: PathBuf,
    executable: PathBuf,
}

#[derive(Default)]
pub(crate) struct UpdateDialog {
    offer: Option<Offer>,
    staged: Option<StagedOffer>,
    installed: Option<InstalledOffer>,
    error: Option<String>,
}

impl UpdateDialog {
    pub(crate) fn choose_manifest(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select verified update manifest")
            .add_filter("Lunar Magic Rust update", &["update"])
            .pick_file()
        else {
            return;
        };
        match load_offer(path) {
            Ok(offer) => {
                self.offer = Some(offer);
                self.staged = None;
                self.installed = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if self.offer.is_some() {
            let mut cancel = false;
            let mut stage = false;
            let offer = self.offer.as_ref().unwrap();
            egui::Window::new(localized_text(catalog, UiTextKey::UpdateAvailableTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdateVersionFormat,
                        "{version}",
                        &offer.manifest.version.to_string(),
                    ));
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdatePlatformFormat,
                        "{platform}",
                        &offer.manifest.target,
                    ));
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdateArchiveFormat,
                        "{archive}",
                        &offer.manifest.archive,
                    ));
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdateSizeFormat,
                        "{bytes}",
                        &offer.manifest.length.to_string(),
                    ));
                    ui.label(localized_text(catalog, UiTextKey::UpdateRunningSafeNotice));
                    ui.horizontal(|ui| {
                        cancel = ui
                            .button(localized_text(catalog, UiTextKey::CommonCancel))
                            .clicked();
                        stage = ui
                            .button(localized_text(catalog, UiTextKey::UpdateChooseStageFolder))
                            .clicked();
                    });
                });
            if cancel {
                self.offer = None;
            } else if stage
                && let Some(directory) = rfd::FileDialog::new()
                    .set_title("Choose update staging folder")
                    .pick_folder()
            {
                match stage_offer(offer, &directory) {
                    Ok(path) => {
                        self.staged = Some(StagedOffer {
                            manifest: offer.manifest.clone(),
                            archive: path,
                        });
                        self.offer = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if self.staged.is_some() {
            let staged = self.staged.as_ref().unwrap();
            let mut close = false;
            let mut install = false;
            egui::Window::new(localized_text(catalog, UiTextKey::UpdateStagedTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(localized_text(catalog, UiTextKey::UpdateStagedReady));
                    ui.label(staged.archive.display().to_string());
                    ui.label(localized_text(
                        catalog,
                        UiTextKey::UpdateImmutableInstallNotice,
                    ));
                    ui.horizontal(|ui| {
                        close = ui
                            .button(localized_text(catalog, UiTextKey::UpdateKeepStaged))
                            .clicked();
                        install = ui
                            .button(localized_text(catalog, UiTextKey::UpdateChooseInstallRoot))
                            .clicked();
                    });
                });
            if close {
                self.staged = None;
            } else if install
                && let Some(root) = rfd::FileDialog::new()
                    .set_title("Choose launcher install root")
                    .pick_folder()
            {
                match install_offer(staged, &root) {
                    Ok(installed) => {
                        self.installed = Some(installed);
                        self.staged = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if let Some(installed) = self.installed.as_ref() {
            let mut close = false;
            egui::Window::new(localized_text(catalog, UiTextKey::UpdateActivatedTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(localized_text(catalog, UiTextKey::UpdateRestartNotice));
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdateVersionDirectoryFormat,
                        "{path}",
                        &installed.directory.display().to_string(),
                    ));
                    ui.label(format_field(
                        catalog,
                        UiTextKey::UpdateSelectedExecutableFormat,
                        "{path}",
                        &installed.executable.display().to_string(),
                    ));
                    ui.label(localized_text(catalog, UiTextKey::UpdateRollbackNotice));
                    close = ui
                        .button(localized_text(catalog, UiTextKey::CommonOk))
                        .clicked();
                });
            if close {
                self.installed = None;
            }
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(localized_text(
                catalog,
                UiTextKey::UpdateVerificationFailedTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(error);
                if ui
                    .button(localized_text(catalog, UiTextKey::CommonOk))
                    .clicked()
                {
                    self.error = None;
                }
            });
        }
    }
}

fn format_field(
    catalog: Option<&LocalizationCatalog>,
    key: UiTextKey,
    placeholder: &str,
    value: &str,
) -> String {
    localized_text(catalog, key).replace(placeholder, value)
}

fn load_offer(path: PathBuf) -> Result<Offer, String> {
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES as u64 {
        return Err("update manifest is not a bounded regular file".into());
    }
    let manifest = UpdateManifest::decode(&fs::read(&path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let archive = path
        .parent()
        .ok_or("update manifest has no parent directory")?
        .join(&manifest.archive);
    let metadata = fs::metadata(&archive).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err("update archive is not a bounded regular file".into());
    }
    let current: Version = env!("CARGO_PKG_VERSION")
        .parse()
        .map_err(|error: lm_update::UpdateError| error.to_string())?;
    manifest
        .verify_archive_reader(
            current,
            env!("LM_BUILD_TARGET"),
            fs::File::open(&archive).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    Ok(Offer { manifest, archive })
}

fn stage_offer(offer: &Offer, directory: &std::path::Path) -> Result<PathBuf, String> {
    let current = env!("CARGO_PKG_VERSION")
        .parse()
        .map_err(|error: lm_update::UpdateError| error.to_string())?;
    let archive = fs::File::open(&offer.archive).map_err(|error| error.to_string())?;
    offer
        .manifest
        .stage_archive_reader(current, env!("LM_BUILD_TARGET"), archive, directory)
        .map_err(|error| error.to_string())
}

fn install_offer(staged: &StagedOffer, root: &std::path::Path) -> Result<InstalledOffer, String> {
    let directory = staged
        .manifest
        .extract_staged_archive(&staged.archive, root)
        .map_err(|error| error.to_string())?;
    if let Err(error) = lm_update::activate_version(root, &directory) {
        let _cleanup = fs::remove_dir_all(&directory);
        return Err(error.to_string());
    }
    let executable = lm_update::resolve_current(root).map_err(|error| error.to_string())?;
    Ok(InstalledOffer {
        directory,
        executable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};
    use std::io::Write;

    #[test]
    fn update_dialog_surface_has_no_literal_widget_text() {
        let source = include_str!("update_dialog.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "update dialog bypasses typed localization with {literal_widget}"
            );
        }
        for key in [
            UiTextKey::UpdateAvailableTitle,
            UiTextKey::UpdateChooseStageFolder,
            UiTextKey::UpdateStagedTitle,
            UiTextKey::UpdateChooseInstallRoot,
            UiTextKey::UpdateActivatedTitle,
            UiTextKey::UpdateVerificationFailedTitle,
        ] {
            assert!(source.contains(&format!("UiTextKey::{key:?}")));
        }
    }

    #[test]
    fn update_dynamic_fields_use_installed_translation_templates() {
        let catalog = LocalizationCatalog::new(
            "zz-test",
            UiTextKey::ALL.map(|key| {
                let text = match key {
                    UiTextKey::UpdateVersionFormat => "Versio={version}",
                    UiTextKey::UpdateVersionDirectoryFormat => "Dosier={path}",
                    _ => key.english(),
                };
                (key, text.to_owned())
            }),
        )
        .unwrap();
        assert_eq!(
            format_field(
                Some(&catalog),
                UiTextKey::UpdateVersionFormat,
                "{version}",
                "2.4.0"
            ),
            "Versio=2.4.0"
        );
        assert_eq!(
            format_field(
                Some(&catalog),
                UiTextKey::UpdateVersionDirectoryFormat,
                "{path}",
                "/versions/2.4.0"
            ),
            "Dosier=/versions/2.4.0"
        );
    }

    fn portable_bundle(native: &[u8]) -> Vec<u8> {
        fn octal(field: &mut [u8], value: u64) {
            let text = format!("{value:o}");
            field.fill(b'0');
            let start = field.len() - text.len() - 1;
            field[start..start + text.len()].copy_from_slice(text.as_bytes());
            field[field.len() - 1] = 0;
        }

        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let prefix = format!("lunar-magic-rust-0.1.1-{}", env!("LM_BUILD_TARGET"));
        let entries = [
            (
                format!("{prefix}/lm-launcher{suffix}"),
                b"launcher".as_slice(),
            ),
            (format!("{prefix}/lm-native{suffix}"), native),
            (format!("{prefix}/lm-cli{suffix}"), b"cli".as_slice()),
            (
                format!("{prefix}/lm-libretro{suffix}"),
                b"backend".as_slice(),
            ),
            (
                format!("{prefix}/RELEASE-MANIFEST.txt"),
                b"manifest".as_slice(),
            ),
        ];
        let mut tar = Vec::new();
        for (name, bytes) in entries {
            let mut header = [0_u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            octal(&mut header[100..108], 0o644);
            octal(&mut header[124..136], bytes.len() as u64);
            header[148..156].fill(b' ');
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let sum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
            header[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
            header[154] = 0;
            header[155] = b' ';
            tar.extend_from_slice(&header);
            tar.extend_from_slice(bytes);
            tar.resize(tar.len().next_multiple_of(512), 0);
        }
        tar.resize(tar.len() + 1024, 0);
        let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
        gzip.write_all(&tar).unwrap();
        gzip.finish().unwrap()
    }

    fn staged_offer(directory: &std::path::Path, native: &[u8]) -> StagedOffer {
        let bytes = portable_bundle(native);
        let archive = directory.join("bundle.tar.gz");
        fs::write(&archive, &bytes).unwrap();
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        let manifest = UpdateManifest::decode(
            format!(
                "LMUPDATE1\nversion 0.1.1\ntarget {}\narchive bundle.tar.gz\nlength {}\nsha256 {hex}\n",
                env!("LM_BUILD_TARGET"),
                bytes.len()
            )
            .as_bytes(),
        )
        .unwrap();
        StagedOffer { manifest, archive }
    }

    #[test]
    fn local_offer_requires_consent_then_stages_exact_verified_bytes() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let archive = source.path().join("bundle.tar.gz");
        fs::write(&archive, b"abc").unwrap();
        let manifest = source.path().join("bundle.tar.gz.update");
        fs::write(
            &manifest,
            format!(
                "LMUPDATE1\nversion 0.1.1\ntarget {}\narchive bundle.tar.gz\nlength 3\nsha256 ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n",
                env!("LM_BUILD_TARGET")
            ),
        )
        .unwrap();

        let offer = load_offer(manifest).unwrap();
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
        let staged = stage_offer(&offer, destination.path()).unwrap();
        assert_eq!(fs::read(staged).unwrap(), b"abc");
    }

    #[test]
    fn local_offer_rejects_tampering_before_presenting_consent() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("bundle.tar.gz"), b"abd").unwrap();
        let manifest = source.path().join("bundle.tar.gz.update");
        fs::write(
            &manifest,
            format!(
                "LMUPDATE1\nversion 0.1.1\ntarget {}\narchive bundle.tar.gz\nlength 3\nsha256 ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n",
                env!("LM_BUILD_TARGET")
            ),
        )
        .unwrap();
        assert!(load_offer(manifest).is_err());
    }

    #[test]
    fn explicit_install_consent_extracts_and_activates_new_version() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let staged = staged_offer(source.path(), b"native executable");

        let installed = install_offer(&staged, root.path()).unwrap();

        assert_eq!(
            installed.executable,
            lm_update::resolve_current(root.path()).unwrap()
        );
        assert_eq!(
            fs::read(&installed.executable).unwrap(),
            b"native executable"
        );
        assert_eq!(installed.directory.parent(), Some(root.path()));
        assert!(root.path().join("LMCURRENT1").is_file());
    }

    #[test]
    fn failed_activation_removes_new_version_and_keeps_selector_absent() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let staged = staged_offer(source.path(), b"");

        assert!(install_offer(&staged, root.path()).is_err());

        assert!(!root.path().join("LMCURRENT1").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
