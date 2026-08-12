use super::{BatchImageSource, render_batch_level_canvas};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LevelImageFormat {
    Png,
    Bmp,
}

impl LevelImageFormat {
    pub(super) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Bmp => "bmp",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LevelImageBatchOptions {
    pub modified_only: bool,
    pub auto_set_screens: bool,
}

impl Default for LevelImageBatchOptions {
    fn default() -> Self {
        Self {
            modified_only: true,
            auto_set_screens: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LevelImageBatchReport {
    pub exported: usize,
    pub skipped_unrenderable: usize,
}

struct RunningBatch {
    template: PathBuf,
    format: LevelImageFormat,
    options: LevelImageBatchOptions,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<LevelImageBatchReport>, String>>,
}

impl RunningBatch {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct LevelImageBatchWorker {
    running: Option<RunningBatch>,
}

impl LevelImageBatchWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        source: BatchImageSource,
        template: PathBuf,
        format: LevelImageFormat,
        options: LevelImageBatchOptions,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a level image batch is already running".into());
        }
        let total = source.profile.level.layer1.entries;
        if total == 0 || total > usize::from(u16::MAX) + 1 {
            return Err(format!(
                "profile declares unsupported level image count {total}"
            ));
        }
        let completed = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_template = template.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-level-image-batch".into())
            .spawn(move || {
                let result = export_batch(
                    &source,
                    &worker_template,
                    format,
                    options,
                    &worker_completed,
                    &worker_cancelled,
                );
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create level-image worker: {error}"))?;
        self.running = Some(RunningBatch {
            template,
            format,
            options,
            total,
            completed,
            cancelled,
            result,
        });
        Ok(())
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Option<LevelImageBatchReport>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new(super::text(catalog, Key::RomNativeAssetsImageBatchTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        super::text(catalog, Key::RomNativeAssetsImageBatchPathFormat)
                            .replace("{format}", &running.format.extension().to_uppercase())
                            .replace("{path}", &running.template.display().to_string()),
                    );
                    ui.label(if running.options.modified_only {
                        super::text(catalog, Key::RomNativeAssetsImageBatchModifiedSelection)
                    } else {
                        super::text(catalog, Key::RomNativeAssetsImageBatchAllSelection)
                    });
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / running.total as f32).text(
                            super::text(catalog, Key::RomNativeAssetsImageBatchProgressFormat)
                                .replace("{completed}", &completed.to_string())
                                .replace("{total}", &running.total.to_string()),
                        ),
                    );
                    ui.label(super::text(catalog, Key::RomNativeAssetsImageBatchNotice));
                    if cancellation_requested {
                        ui.label(super::text(catalog, Key::RomNativeAssetsMwlBatchCancelling));
                    } else if ui
                        .button(super::text(catalog, Key::RomNativeAssetsCancel))
                        .clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape))
                    {
                        running.request_cancel();
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<Option<LevelImageBatchReport>, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                self.running = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                Some(Err(
                    "level-image worker stopped without reporting a result".into()
                ))
            }
        }
    }
}

fn export_batch(
    source: &BatchImageSource,
    template: &std::path::Path,
    format: LevelImageFormat,
    options: LevelImageBatchOptions,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<LevelImageBatchReport>, String> {
    let total = source.profile.level.layer1.entries;
    let mut group = lm_app::file_persistence::NewFileGroup::new();
    let mut report = LevelImageBatchReport {
        exported: 0,
        skipped_unrenderable: 0,
    };
    for slot in 0..total {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let level = u16::try_from(slot).map_err(|error| error.to_string())?;
        if options.modified_only
            && !lm_app::native_level_is_in_expanded_area(
                &source.image,
                source.profile.mapper,
                source.profile.level.layer1,
                slot,
            )?
        {
            completed.store(slot + 1, Ordering::Relaxed);
            continue;
        }
        let canvas = match render_batch_level_canvas(source, level, options.auto_set_screens) {
            Ok(canvas) => canvas,
            Err(_) => {
                report.skipped_unrenderable += 1;
                completed.store(slot + 1, Ordering::Relaxed);
                continue;
            }
        };
        let bytes = match format {
            LevelImageFormat::Png => lm_render::encode_png(&canvas).map_err(|e| e.to_string())?,
            LevelImageFormat::Bmp => lm_render::encode_bmp(&canvas).map_err(|e| e.to_string())?,
        };
        let destination = batch_output_path(template, level, format)?;
        group
            .stage(&destination, &bytes)
            .map_err(|error| format!("level {level:03X}: {error}"))?;
        completed.store(slot + 1, Ordering::Relaxed);
        report.exported += 1;
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    if report.exported != 0 {
        group.publish().map_err(|error| error.to_string())?;
    }
    Ok(Some(report))
}

fn batch_output_path(
    template: &std::path::Path,
    level: u16,
    format: LevelImageFormat,
) -> Result<PathBuf, String> {
    let stem = template
        .file_stem()
        .ok_or("level-image batch template requires a file name")?;
    let mut name = stem.to_os_string();
    name.push(format!(" {level:03X}.{}", format.extension()));
    Ok(template.with_file_name(name))
}

#[cfg(test)]
pub(super) mod tests {
    use super::{
        BatchImageSource, LevelImageBatchOptions, LevelImageFormat, RunningBatch,
        batch_output_path, export_batch, render_batch_level_canvas,
    };
    use lm_app::{ControllerSnapshot, EditorMode};
    use lm_project::{
        ExAnimationRomLayout, InstalledExAnimationRomLayout, InstalledLayout, LevelPointerTable,
    };
    use lm_rom::{Mapper, RomImage};
    use std::path::{Path, PathBuf};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(in crate::rom_level_assets_editor) fn installed_source(headered: bool) -> BatchImageSource {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let physical = std::fs::read(
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
        profile.mapper = Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.level.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        profile.layer2 = Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap());
        profile.palette = lm_profile::smw_us_v1_custom_palette_layout();
        profile.palette_installation = InstalledLayout::Unconditional(profile.palette);
        profile.exanimation = ExAnimationRomLayout {
            mapper: Mapper::LoRom,
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
        profile.map16.mapper = Mapper::LoRom;
        profile.map16.graphics.offset = 0x180000;
        profile.map16.acts_like.offset = 0x181000;
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile.overworld.layers.mapper = Mapper::LoRom;
        profile.overworld.layers.layer1.offset = 0x182000;
        profile.overworld.layers.layer2.offset = 0x183000;
        profile.overworld.event_reveals.mapper = Mapper::LoRom;
        profile.overworld.event_reveals.sources.offset = 0x184000;
        profile.overworld.event_reveals.destinations.offset = 0x185000;
        profile.overworld.endpoints.mapper = Mapper::LoRom;
        profile.overworld.endpoints.pointers.offset = 0x186000;
        profile.overworld.messages.mapper = Mapper::LoRom;
        profile.overworld.messages.pointers.offset = 0x187000;
        profile.overworld.sprites.mapper = Mapper::LoRom;
        profile.overworld.sprites.pointers.offset = 0x188000;
        profile.overworld.palette.mapper = Mapper::LoRom;
        profile.overworld.palette.pointers.offset = 0x189000;
        profile.overworld.animation.mapper = Mapper::LoRom;
        profile.overworld.animation.pointers.offset = 0x18a000;
        profile.validate().unwrap();
        BatchImageSource {
            snapshot: ControllerSnapshot {
                revision: 0,
                mode: EditorMode::Level(0),
                identity: lm_rom::detect_identity(&image).unwrap(),
                document_path: None,
                rom_bytes,
            },
            profile,
            image,
            ownership: lm_graphics::PaletteOwnership::editable(257),
            animation_phase: Some(2),
            special_world_passed: false,
            visibility: crate::application::LevelViewVisibility::default(),
            gfx_display_override: Default::default(),
        }
    }

    #[test]
    fn batch_names_use_the_original_template_and_uppercase_level_numbers() {
        assert_eq!(
            batch_output_path(
                Path::new("/tmp/My Export.png"),
                0x00a,
                LevelImageFormat::Png
            )
            .unwrap(),
            Path::new("/tmp/My Export 00A.png")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/世界.bmp"), 0x105, LevelImageFormat::Bmp).unwrap(),
            Path::new("/tmp/世界 105.bmp")
        );
        assert!(batch_output_path(Path::new("/"), 0, LevelImageFormat::Png).is_err());
    }

    #[test]
    fn original_batch_defaults_select_modified_levels_without_auto_sizing() {
        assert_eq!(
            LevelImageBatchOptions::default(),
            LevelImageBatchOptions {
                modified_only: true,
                auto_set_screens: false,
            }
        );
    }

    #[test]
    fn cancellation_request_is_shared_with_the_render_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningBatch {
            template: PathBuf::from("images/Levels.png"),
            format: LevelImageFormat::Png,
            options: LevelImageBatchOptions::default(),
            total: 0x200,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn installed_full_level_variants_encode_identically_without_copier_header() {
        let mut expected = None;
        for headered in [true, false] {
            let source = installed_source(headered);
            let source = &source;
            let encoded = [0x000, 0x01c, 0x109]
                .into_iter()
                .flat_map(|level| {
                    [false, true].map(move |auto_set_screens| {
                        let canvas =
                            render_batch_level_canvas(&source, level, auto_set_screens).unwrap();
                        assert!(canvas.width() >= 256);
                        assert!(canvas.height() >= 224);
                        (
                            level,
                            auto_set_screens,
                            lm_oracle::sha256_hex(&lm_render::encode_png(&canvas).unwrap()),
                            lm_oracle::sha256_hex(&lm_render::encode_bmp(&canvas).unwrap()),
                        )
                    })
                })
                .collect::<Vec<_>>();
            if let Some(expected) = &expected {
                assert_eq!(&encoded, expected);
            } else {
                expected = Some(encoded);
            }
        }
    }

    #[test]
    fn installed_modified_batch_publishes_only_the_wine_selected_level() {
        let directory = temporary_directory("modified");
        let completed = AtomicUsize::new(0);
        let report = export_batch(
            &installed_source(true),
            &directory.join("Levels.png"),
            LevelImageFormat::Png,
            LevelImageBatchOptions::default(),
            &completed,
            &AtomicBool::new(false),
        )
        .unwrap()
        .unwrap();
        assert_eq!(report.exported, 1);
        assert_eq!(report.skipped_unrenderable, 0);
        assert_eq!(completed.load(Ordering::Relaxed), 0x200);
        assert!(directory.join("Levels 000.png").is_file());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cancelled_batch_never_publishes_a_partial_image_group() {
        let directory = temporary_directory("cancelled");
        let result = export_batch(
            &installed_source(true),
            &directory.join("Levels.bmp"),
            LevelImageFormat::Bmp,
            LevelImageBatchOptions {
                modified_only: false,
                auto_set_screens: false,
            },
            &AtomicUsize::new(0),
            &AtomicBool::new(true),
        )
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir(directory).unwrap();
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lm-native-level-image-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        directory
    }
}
