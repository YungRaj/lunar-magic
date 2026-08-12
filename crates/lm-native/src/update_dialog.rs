use eframe::egui;
use lm_update::{MAX_ARCHIVE_BYTES, MAX_MANIFEST_BYTES, UpdateManifest, Version};
use std::{fs, path::PathBuf};

struct Offer {
    manifest: UpdateManifest,
    archive: PathBuf,
}

#[derive(Default)]
pub(crate) struct UpdateDialog {
    offer: Option<Offer>,
    staged: Option<PathBuf>,
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
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if self.offer.is_some() {
            let mut cancel = false;
            let mut stage = false;
            let offer = self.offer.as_ref().unwrap();
            egui::Window::new("Verified update available")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Version: {}", offer.manifest.version));
                    ui.label(format!("Platform: {}", offer.manifest.target));
                    ui.label(format!("Archive: {}", offer.manifest.archive));
                    ui.label(format!("Size: {} bytes", offer.manifest.length));
                    ui.label("The current application will not be replaced automatically.");
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        stage = ui
                            .button("Choose folder and stage verified archive")
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
                        self.staged = Some(path);
                        self.offer = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if let Some(path) = self.staged.clone() {
            egui::Window::new("Update staged")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("The verified archive is ready for manual installation after exit.");
                    ui.label(path.display().to_string());
                    if ui.button("OK").clicked() {
                        self.staged = None;
                    }
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Update verification failed")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
