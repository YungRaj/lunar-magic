use eframe::egui;
use lm_app::{
    AppState, ControllerSnapshot, ExtendedUiTextKey, LocalizationCatalog, MwlBatchExportMode,
    ProfiledControllerSnapshot,
};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

enum BatchSource {
    Installed(ProfiledControllerSnapshot),
    Builtin(ControllerSnapshot),
}

fn run_export(
    source: BatchSource,
    template: &std::path::Path,
    mode: MwlBatchExportMode,
    cancelled: &AtomicBool,
) -> Result<Option<usize>, String> {
    let exported = match source {
        BatchSource::Installed(profiled) => {
            lm_app::export_smw_us_v1_installed_mwl_batch_until(&profiled, mode, || {
                cancelled.load(Ordering::Relaxed)
            })
        }
        BatchSource::Builtin(snapshot) => {
            lm_app::export_builtin_smw_us_v1_mwl_batch_until(&snapshot, mode, || {
                cancelled.load(Ordering::Relaxed)
            })
        }
    }?;
    match exported {
        Some(documents) if !cancelled.load(Ordering::Relaxed) => {
            lm_app::publish_mwl_batch_new(template, &documents).map(Some)
        }
        Some(_) | None => Ok(None),
    }
}

struct RunningExport {
    template: PathBuf,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<usize>, String>>,
}

#[derive(Default)]
pub(crate) struct RomMwlBatchExportDialog {
    running: Option<RunningExport>,
    result: Option<Result<Option<usize>, String>>,
}

impl RomMwlBatchExportDialog {
    pub(crate) const fn is_open(&self) -> bool {
        self.running.is_some() || self.result.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState, mode: MwlBatchExportMode) {
        if self.is_open() {
            return;
        }
        let Some(template) = crate::dialogs::choose_mwl_batch_template() else {
            return;
        };
        let source = match app.profiled_controller_snapshot() {
            Ok(profiled) => BatchSource::Installed(profiled),
            Err(lm_app::AppError::NoRevisionProfile) => match app.controller_snapshot() {
                Ok(snapshot) => BatchSource::Builtin(snapshot),
                Err(error) => {
                    self.result = Some(Err(error.to_string()));
                    return;
                }
            },
            Err(error) => {
                self.result = Some(Err(error.to_string()));
                return;
            }
        };
        if let Err(error) = self.start(source, template, mode) {
            self.result = Some(Err(error));
        }
    }

    fn start(
        &mut self,
        source: BatchSource,
        template: PathBuf,
        mode: MwlBatchExportMode,
    ) -> Result<(), String> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_template = template.clone();
        let (sender, result) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("lm-rom-mwl-batch-export".into())
            .spawn(move || {
                let exported = run_export(source, &worker_template, mode, &worker_cancelled);
                let _ignored = sender.send(exported);
            });
        match spawn {
            Ok(_worker) => {
                self.running = Some(RunningExport {
                    template,
                    cancelled,
                    result,
                });
                Ok(())
            }
            Err(error) => Err(format!("could not start batch MWL export: {error}")),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Relaxed);
            return !application;
        }
        self.result = None;
        true
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(running) = &self.running {
            match running.result.try_recv() {
                Ok(result) => {
                    self.result = Some(result);
                    self.running = None;
                }
                Err(TryRecvError::Empty) => context.request_repaint(),
                Err(TryRecvError::Disconnected) => {
                    self.result = Some(Err("batch MWL export worker disconnected".into()));
                    self.running = None;
                }
            }
        }
        let mut cancel = false;
        let mut close = false;
        if let Some(running) = &self.running {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::MwlBatchExportProgressTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    text(catalog, ExtendedUiTextKey::MwlBatchExportTemplateFormat)
                        .replace("{path}", &running.template.display().to_string()),
                );
                ui.label(text(catalog, ExtendedUiTextKey::MwlBatchExportAtomicNotice));
                if running.cancelled.load(Ordering::Relaxed) {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::MwlBatchExportCancellationRequested,
                    ));
                } else if ui
                    .button(text(catalog, ExtendedUiTextKey::MwlBatchExportCancel))
                    .clicked()
                {
                    cancel = true;
                }
            });
            cancel |= context.input(|input| input.key_pressed(egui::Key::Escape));
        } else if let Some(result) = &self.result {
            egui::Window::new(text(catalog, ExtendedUiTextKey::MwlBatchExportResultTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    match result {
                        Ok(Some(count)) => {
                            ui.label(
                                text(catalog, ExtendedUiTextKey::MwlBatchExportCompletedFormat)
                                    .replace("{count}", &count.to_string()),
                            );
                        }
                        Ok(None) => {
                            ui.label(text(catalog, ExtendedUiTextKey::MwlBatchExportCancelled));
                        }
                        Err(error) => {
                            ui.colored_label(egui::Color32::RED, error);
                        }
                    };
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::MwlBatchExportClose))
                        .clicked()
                    {
                        close = true;
                    }
                });
        }
        if cancel {
            if let Some(running) = &self.running {
                running.cancelled.store(true, Ordering::Relaxed);
            }
        }
        if close {
            self.result = None;
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::{BatchSource, RomMwlBatchExportDialog, run_export};
    use lm_app::{
        ControllerSnapshot, EditorMode, ExtendedUiTextKey, MwlBatchExportMode,
        ProfiledControllerSnapshot,
    };
    use lm_project::{
        ExAnimationRomLayout, InstalledExAnimationRomLayout, InstalledLayout, LevelPointerTable,
    };
    use lm_rom::RomImage;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn complete_batch_export_form_uses_every_typed_key() {
        let source = include_str!("rom_mwl_batch_export_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("MwlBatchExport"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for literal in [
            "Window::new(\"Exporting Multiple MWL Levels\")",
            "Window::new(\"MWL Batch Export\")",
            "ui.button(\"Cancel\")",
            "ui.button(\"Close\")",
        ] {
            assert!(!source.contains(literal));
        }
    }

    fn installed_fixture(headered: bool) -> ProfiledControllerSnapshot {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let physical = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let rom_bytes = if headered {
            physical
        } else {
            physical_image.logical_bytes().to_vec()
        };
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.level.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        profile.layer2 = Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap());
        profile.palette = lm_profile::smw_us_v1_custom_palette_layout();
        profile.palette_installation = InstalledLayout::Unconditional(profile.palette);
        profile.exanimation = ExAnimationRomLayout {
            mapper: lm_rom::Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x8138b,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        };
        profile.exanimation_installation =
            InstalledLayout::Unconditional(InstalledExAnimationRomLayout {
                payload: profile.exanimation,
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: None,
            });
        profile.exanimation_feature_installation = InstalledLayout::Absent;
        profile.expanded_settings = Some(lm_profile::smw_us_v1_expanded_settings_layout());
        profile.map16.mapper = lm_rom::Mapper::LoRom;
        profile.graphics.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.layers.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.event_reveals.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.endpoints.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.messages.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.sprites.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.palette.mapper = lm_rom::Mapper::LoRom;
        profile.overworld.animation.mapper = lm_rom::Mapper::LoRom;
        profile.validate().unwrap();
        ProfiledControllerSnapshot {
            snapshot: ControllerSnapshot {
                revision: 0,
                mode: EditorMode::Level(0),
                identity: lm_rom::detect_identity(&image).unwrap(),
                document_path: None,
                rom_bytes,
            },
            profile,
        }
    }

    #[test]
    fn default_dialog_is_closed_and_close_is_idempotent() {
        let mut dialog = RomMwlBatchExportDialog::default();
        assert!(!dialog.is_open());
        assert!(dialog.request_close(false));
        assert!(!dialog.is_open());
    }

    #[test]
    fn builtin_worker_exports_all_512_vanilla_levels() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom_bytes = fs::read(root.join("sysLMRestore/smwOrig.smc")).unwrap();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let snapshot = ControllerSnapshot {
            revision: 0,
            mode: EditorMode::Level(0x105),
            identity: lm_rom::detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lm-native-builtin-batch-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let mut dialog = RomMwlBatchExportDialog::default();
        dialog
            .start(
                BatchSource::Builtin(snapshot),
                directory.join("Vanilla.mwl"),
                MwlBatchExportMode::All,
            )
            .unwrap();
        let result = dialog
            .running
            .as_ref()
            .unwrap()
            .result
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(result, Some(0x200));
        assert!(directory.join("Vanilla 000.mwl").is_file());
        assert!(directory.join("Vanilla 1FF.mwl").is_file());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0x200);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn installed_worker_exports_only_lunar_magic_modified_selection() {
        let mut headered_bytes = None;
        for headered in [true, false] {
            let directory = temporary_directory(if headered {
                "installed-modified-headered"
            } else {
                "installed-modified-headerless"
            });
            let result = run_export(
                BatchSource::Installed(installed_fixture(headered)),
                &directory.join("Installed.mwl"),
                MwlBatchExportMode::Modified,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(result, Some(1));
            assert!(directory.join("Installed 000.mwl").is_file());
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
            let bytes = fs::read(directory.join("Installed 000.mwl")).unwrap();
            if let Some(expected) = &headered_bytes {
                assert_eq!(&bytes, expected);
            } else {
                headered_bytes = Some(bytes);
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn installed_worker_exports_all_512_levels_across_copier_header_variants() {
        let mut headered_hashes = None;
        for headered in [true, false] {
            let directory = temporary_directory(if headered {
                "installed-all-headered"
            } else {
                "installed-all-headerless"
            });
            let result = run_export(
                BatchSource::Installed(installed_fixture(headered)),
                &directory.join("Installed.mwl"),
                MwlBatchExportMode::All,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(result, Some(0x200));
            assert!(directory.join("Installed 000.mwl").is_file());
            assert!(directory.join("Installed 1FF.mwl").is_file());
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 0x200);
            let hashes = (0..0x200)
                .map(|level| {
                    lm_oracle::sha256_hex(
                        &fs::read(directory.join(format!("Installed {level:03X}.mwl"))).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            if let Some(expected) = &headered_hashes {
                assert_eq!(&hashes, expected);
            } else {
                headered_hashes = Some(hashes);
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn worker_cancellation_never_publishes_partial_output() {
        let directory = temporary_directory("cancelled");
        let result = run_export(
            BatchSource::Installed(installed_fixture(true)),
            &directory.join("Cancelled.mwl"),
            MwlBatchExportMode::Modified,
            &AtomicBool::new(true),
        )
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(directory).unwrap();
    }

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lm-native-mwl-batch-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }
}
