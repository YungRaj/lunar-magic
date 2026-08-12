use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Debug)]
struct IpsCreateCompletion {
    result: Result<usize, String>,
}

#[derive(Debug)]
struct RunningIpsCreate {
    before: PathBuf,
    after: PathBuf,
    output: PathBuf,
    completion: Receiver<IpsCreateCompletion>,
}

#[derive(Default)]
pub(crate) struct IpsCreateDialog {
    running: Option<RunningIpsCreate>,
    completed: Option<String>,
    error: Option<String>,
}

impl IpsCreateDialog {
    pub(crate) const fn is_busy(&self) -> bool {
        self.running.is_some()
    }

    pub(crate) const fn has_open_workflow(&self) -> bool {
        self.running.is_some() || self.completed.is_some() || self.error.is_some()
    }

    pub(crate) fn choose_and_start(
        &mut self,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<bool, String> {
        if self.running.is_some() {
            return Err("an IPS creation workflow is already active".into());
        }
        let Some(before) = crate::dialogs::choose_ips_source_rom(&text(
            catalog,
            ExtendedUiTextKey::IpsCreateOriginalPrompt,
        )) else {
            return Ok(false);
        };
        let Some(after) = crate::dialogs::choose_ips_source_rom(&text(
            catalog,
            ExtendedUiTextKey::IpsCreateModifiedPrompt,
        )) else {
            return Ok(false);
        };
        let suggested = after
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map_or_else(|| "patch.ips".into(), |stem| format!("{stem}.ips"));
        let Some(output) = crate::dialogs::choose_ips_output(&suggested) else {
            return Ok(false);
        };
        validate_distinct_paths(&before, &after, &output)?;

        let worker_before = before.clone();
        let worker_after = after.clone();
        let worker_output = output.clone();
        let (sender, completion) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-create-ips".into())
            .spawn(move || {
                let result = create_and_publish_ips(&worker_before, &worker_after, &worker_output);
                let _send_result = sender.send(IpsCreateCompletion { result });
            })
            .map_err(|error| format!("could not create IPS worker: {error}"))?;
        self.running = Some(RunningIpsCreate {
            before,
            after,
            output,
            completion,
        });
        self.completed = None;
        self.error = None;
        Ok(true)
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        self.poll(catalog);
        if let Some(running) = &self.running {
            egui::Window::new(text(catalog, ExtendedUiTextKey::IpsCreateTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::IpsCreateOriginalFormat,
                        "{path}",
                        &running.before.display().to_string(),
                    ));
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::IpsCreateModifiedFormat,
                        "{path}",
                        &running.after.display().to_string(),
                    ));
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::IpsCreateOutputFormat,
                        "{path}",
                        &running.output.display().to_string(),
                    ));
                    ui.label(text(catalog, ExtendedUiTextKey::IpsCreateProgress));
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(message) = self.completed.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::IpsCreateCompletedTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(message);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::IpsCreateOk))
                        .clicked()
                    {
                        self.completed = None;
                    }
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::IpsCreateErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.colored_label(egui::Color32::RED, error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::IpsCreateOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    fn poll(&mut self, catalog: Option<&LocalizationCatalog>) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let result = match running.completion.try_recv() {
            Ok(completion) => Some(completion.result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("IPS worker stopped without reporting a result".into()))
            }
        };
        let Some(result) = result else {
            return;
        };
        let output = running.output.clone();
        self.running = None;
        match result {
            Ok(bytes) => {
                self.completed = Some(
                    text(catalog, ExtendedUiTextKey::IpsCreateCompletedFormat)
                        .replace("{path}", &output.display().to_string())
                        .replace("{bytes}", &bytes.to_string()),
                );
            }
            Err(error) => self.error = Some(error),
        }
    }

    #[cfg(test)]
    fn wait_for_test(&mut self) {
        let running = self.running.take().expect("IPS worker is running");
        let completion = running
            .completion
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("IPS worker reports completion");
        match completion.result {
            Ok(bytes) => {
                self.completed = Some(format!(
                    "Created {} ({} bytes).",
                    running.output.display(),
                    bytes
                ));
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn format_text(
    catalog: Option<&LocalizationCatalog>,
    key: ExtendedUiTextKey,
    placeholder: &str,
    value: &str,
) -> String {
    text(catalog, key).replace(placeholder, value)
}

fn create_and_publish_ips(before: &Path, after: &Path, output: &Path) -> Result<usize, String> {
    validate_distinct_paths(before, after, output)?;
    let before_bytes = crate::dialogs::read_regular_bounded(
        before,
        lm_rom::MAX_IPS_IMAGE_LEN as u64,
        "original IPS image",
    )
    .map_err(|error| error.to_string())?;
    let after_bytes = crate::dialogs::read_regular_bounded(
        after,
        lm_rom::MAX_IPS_IMAGE_LEN as u64,
        "modified IPS image",
    )
    .map_err(|error| error.to_string())?;
    let before_image = crate::ips_compat::lunar_magic_ips_image(&before_bytes)?;
    let after_image = crate::ips_compat::lunar_magic_ips_image(&after_bytes)?;
    let patch =
        lm_rom::create_ips(&before_image, &after_image).map_err(|error| error.to_string())?;
    if output.exists() {
        lm_app::file_persistence::replace_existing(output, &patch)
    } else {
        lm_app::file_persistence::write_new(output, &patch)
    }
    .map_err(|error| error.to_string())?;
    Ok(patch.len())
}

fn validate_distinct_paths(before: &Path, after: &Path, output: &Path) -> Result<(), String> {
    let before = canonical_input(before)?;
    let after = canonical_input(after)?;
    let output = canonical_destination(output)?;
    if before == after {
        return Err("the original and modified IPS images must differ".into());
    }
    if output == before || output == after {
        return Err("the IPS output must differ from both input images".into());
    }
    Ok(())
}

fn canonical_input(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))
}

fn canonical_destination(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .ok_or_else(|| "IPS output has no file name".to_string())?;
    let parent = std::fs::canonicalize(path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|error| format!("could not resolve IPS output directory: {error}"))?;
    Ok(parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn complete_ips_create_form_uses_every_typed_key() {
        let source = include_str!("ips_create_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("IpsCreate"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for hard_coded_caption in [
            "choose_ips_source_rom(\"Select Original ROM\")",
            "Window::new(\"Create IPS Patch\")",
            "Window::new(\"IPS patch created\")",
            "Window::new(\"IPS creation error\")",
        ] {
            assert!(!source.contains(hard_coded_caption));
        }
    }

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-native-ips-create-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn worker_creates_a_round_trip_patch_without_replacing_inputs() {
        let directory = directory();
        let before = directory.join("before.smc");
        let after = directory.join("after.smc");
        let output = directory.join("change.ips");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut changed = original.clone();
        changed[0x1234] ^= 0x5a;
        let checksum = lm_rom::compute_snes_checksum(&changed, 0x7fdc).unwrap();
        changed[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        fs::write(&before, &original).unwrap();
        fs::write(&after, &changed).unwrap();

        let (sender, completion) = mpsc::channel();
        let worker_before = before.clone();
        let worker_after = after.clone();
        let worker_output = output.clone();
        std::thread::spawn(move || {
            let result = create_and_publish_ips(&worker_before, &worker_after, &worker_output);
            sender.send(IpsCreateCompletion { result }).unwrap();
        });
        let mut dialog = IpsCreateDialog {
            running: Some(RunningIpsCreate {
                before: before.clone(),
                after: after.clone(),
                output: output.clone(),
                completion,
            }),
            ..IpsCreateDialog::default()
        };
        dialog.wait_for_test();
        assert!(dialog.error.is_none());
        let patch = fs::read(&output).unwrap();
        let normalized = crate::ips_compat::lunar_magic_ips_image(&original).unwrap();
        let patched = lm_rom::apply_ips(&normalized, &patch).unwrap();
        assert_eq!(&patched[lm_rom::COPIER_HEADER_LEN..], changed.as_slice());
        assert_eq!(fs::read(&before).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aliases_and_output_collisions_are_rejected_without_mutation() {
        let directory = directory();
        let before = directory.join("before.smc");
        let after = directory.join("after.smc");
        fs::write(&before, [1, 2, 3]).unwrap();
        fs::write(&after, [1, 2, 4]).unwrap();
        assert!(validate_distinct_paths(&before, &before, &directory.join("x.ips")).is_err());
        assert!(validate_distinct_paths(&before, &after, &before).is_err());
        assert_eq!(fs::read(&before).unwrap(), [1, 2, 3]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn headerless_roms_are_normalized_to_lunar_magics_headered_ips_coordinates() {
        let logical = crate::test_support::pristine_smw_us_rom_bytes();
        let normalized = crate::ips_compat::lunar_magic_ips_image(&logical).unwrap();
        assert_eq!(normalized.len(), logical.len() + lm_rom::COPIER_HEADER_LEN);
        assert_eq!(
            &normalized[..lm_rom::COPIER_HEADER_LEN],
            &lm_profile::smw_us_v1_lunar_magic_copier_header()
        );
        assert_eq!(&normalized[lm_rom::COPIER_HEADER_LEN..], logical);
    }
}
