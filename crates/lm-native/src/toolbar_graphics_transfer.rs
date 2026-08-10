use crate::{
    graphics_batch::GraphicsBatchWorker,
    rom_graphics_editor::{
        exgraphics_batch_source, pristine_special_graphics, standard_graphics_batch_source,
    },
};
use eframe::egui;
use lm_app::AppState;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuickGraphicsExtraction {
    Standard,
    ExGraphics,
}

#[derive(Default)]
pub(crate) struct ToolbarGraphicsTransfer {
    worker: GraphicsBatchWorker,
    pending_presentation: Option<GraphicsExtractionPresentation>,
    completion: Option<GraphicsExtractionCompletion>,
    error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GraphicsExtractionPresentation {
    action: QuickGraphicsExtraction,
    target: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GraphicsExtractionCompletion {
    title: &'static str,
    message: String,
}

impl ToolbarGraphicsTransfer {
    pub(crate) fn start(
        &mut self,
        app: &AppState,
        action: QuickGraphicsExtraction,
        joined_standard: bool,
        show_completion: bool,
    ) -> Result<(), String> {
        if self.worker.is_running() {
            return Err("a user-toolbar graphics extraction is already running".into());
        }
        let image_and_layout = graphics_image_and_layout(app)?;
        let target =
            quick_extraction_target(app.document_path.as_deref(), action, joined_standard)?;
        let source = match action {
            QuickGraphicsExtraction::Standard => standard_graphics_batch_source(
                image_and_layout.image,
                image_and_layout.layout,
                image_and_layout.smw_us_special,
            )?,
            QuickGraphicsExtraction::ExGraphics => {
                exgraphics_batch_source(image_and_layout.image, image_and_layout.layout)?
            }
        };
        ensure_target_parent(&target)?;
        let result = match (action, joined_standard) {
            (QuickGraphicsExtraction::Standard, true) => {
                self.worker.start_joined(source, target.clone())
            }
            _ => self.worker.start(source, target.clone()),
        };
        if result.is_ok() && show_completion {
            self.pending_presentation = Some(GraphicsExtractionPresentation { action, target });
        }
        result
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if let Some(result) = self.worker.show(context) {
            self.complete(result);
        }
        if let Some(completion) = self.completion.clone() {
            egui::Window::new(completion.title)
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(completion.message);
                    if ui.button("OK").clicked() {
                        self.completion = None;
                    }
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Graphics extraction error")
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

    fn complete(&mut self, result: Result<Option<usize>, String>) {
        let presentation = self.pending_presentation.take();
        match result {
            Ok(Some(count)) => {
                if let Some(presentation) = presentation {
                    self.completion = Some(match presentation.action {
                        QuickGraphicsExtraction::Standard => GraphicsExtractionCompletion {
                            title: "GFX Extraction Complete!",
                            message: format!(
                                "All GFX files have been extracted to:\n{}",
                                presentation.target.display()
                            ),
                        },
                        QuickGraphicsExtraction::ExGraphics => GraphicsExtractionCompletion {
                            title: "ExGFX Extraction Complete!",
                            message: format!(
                                "{count} ExGFX files have been extracted to:\n{}",
                                presentation.target.display()
                            ),
                        },
                    });
                }
            }
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
    }
}

struct GraphicsImageAndLayout {
    image: RomImage,
    layout: lm_project::GraphicsRomLayout,
    smw_us_special: bool,
}

fn graphics_image_and_layout(app: &AppState) -> Result<GraphicsImageAndLayout, String> {
    match app.profiled_controller_snapshot() {
        Ok(profiled) => {
            let image = RomImage::from_bytes(profiled.snapshot.rom_bytes)
                .map_err(|error| error.to_string())?;
            Ok(GraphicsImageAndLayout {
                smw_us_special: pristine_special_graphics(&profiled.profile),
                layout: profiled.profile.graphics,
                image,
            })
        }
        Err(lm_app::AppError::NoRevisionProfile) => {
            let snapshot = app
                .controller_snapshot()
                .map_err(|error| error.to_string())?;
            if snapshot.identity.game != SupportedGame::SuperMarioWorld
                || snapshot.identity.region != Region::NorthAmerica
                || snapshot.identity.revision != 0
                || snapshot.identity.mapper != Mapper::LoRom
            {
                return Err(
                    "quick GFX extraction without a revision profile requires SMW-US v1 LoROM"
                        .into(),
                );
            }
            Ok(GraphicsImageAndLayout {
                image: RomImage::from_bytes(snapshot.rom_bytes)
                    .map_err(|error| error.to_string())?,
                layout: lm_profile::smw_us_v1_vanilla_graphics_layout(),
                smw_us_special: true,
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

fn quick_extraction_target(
    rom_path: Option<&Path>,
    action: QuickGraphicsExtraction,
    joined_standard: bool,
) -> Result<PathBuf, String> {
    let rom_path = rom_path.ok_or("save the ROM to a named path before quick GFX extraction")?;
    let parent = rom_path
        .parent()
        .ok_or("the open ROM path has no parent directory")?;
    Ok(match (action, joined_standard) {
        (QuickGraphicsExtraction::Standard, true) => parent.join("Graphics").join("AllGFX.bin"),
        (QuickGraphicsExtraction::Standard, false) => parent.join("Graphics"),
        (QuickGraphicsExtraction::ExGraphics, _) => parent.join("ExGraphics"),
    })
}

fn ensure_target_parent(target: &Path) -> Result<(), String> {
    let directory = if target.extension().is_some() {
        target.parent().ok_or("graphics target has no parent")?
    } else {
        target
    };
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "graphics target {} exists but is not a directory",
            directory.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(directory).map_err(|error| {
                format!(
                    "cannot create graphics target directory {}: {error}",
                    directory.display()
                )
            })
        }
        Err(error) => Err(format!(
            "cannot inspect graphics target directory {}: {error}",
            directory.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_targets_use_lunar_magics_fixed_rom_sibling_names() {
        let rom = Path::new("/tmp/project/game.smc");
        assert_eq!(
            quick_extraction_target(Some(rom), QuickGraphicsExtraction::Standard, false).unwrap(),
            Path::new("/tmp/project/Graphics")
        );
        assert_eq!(
            quick_extraction_target(Some(rom), QuickGraphicsExtraction::Standard, true).unwrap(),
            Path::new("/tmp/project/Graphics/AllGFX.bin")
        );
        assert_eq!(
            quick_extraction_target(Some(rom), QuickGraphicsExtraction::ExGraphics, true).unwrap(),
            Path::new("/tmp/project/ExGraphics")
        );
        assert!(quick_extraction_target(None, QuickGraphicsExtraction::Standard, false).is_err());
    }

    #[test]
    fn pristine_quick_source_contains_the_complete_lunar_magic_standard_set() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let resolved = graphics_image_and_layout(&app).unwrap();
        let source = standard_graphics_batch_source(
            resolved.image,
            resolved.layout,
            resolved.smw_us_special,
        )
        .unwrap();
        assert_eq!(source.file_numbers, (0..0x34).collect::<Vec<_>>());
        assert_eq!(source.slots.len(), 0x34);
        assert_eq!(source.file_layouts.len(), 0x34);
    }

    #[test]
    fn quick_standard_action_publishes_the_complete_fixed_directory_set() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut transfer = ToolbarGraphicsTransfer::default();
        transfer
            .start(&app, QuickGraphicsExtraction::Standard, false, false)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let completion = loop {
            if let Some(result) = transfer.worker.poll() {
                break result.unwrap();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "graphics extraction worker timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(completion, Some(0x34));
        let graphics = root.path().join("Graphics");
        for file in 0..0x34 {
            let path = graphics.join(format!("GFX{file:02X}.bin"));
            assert!(path.is_file(), "missing {}", path.display());
        }
    }

    #[test]
    fn unavailable_exgraphics_rejects_before_creating_the_fixed_directory() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut transfer = ToolbarGraphicsTransfer::default();
        assert!(
            transfer
                .start(&app, QuickGraphicsExtraction::ExGraphics, false, false)
                .is_err()
        );
        assert!(!root.path().join("ExGraphics").exists());
    }

    #[test]
    fn ordinary_standard_extraction_presents_the_authenticated_completion_resource() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut transfer = ToolbarGraphicsTransfer::default();
        transfer
            .start(&app, QuickGraphicsExtraction::Standard, false, true)
            .unwrap();
        let result = loop {
            if let Some(result) = transfer.worker.poll() {
                break result;
            }
            std::thread::yield_now();
        };
        transfer.complete(result);
        assert_eq!(
            transfer.completion,
            Some(GraphicsExtractionCompletion {
                title: "GFX Extraction Complete!",
                message: format!(
                    "All GFX files have been extracted to:\n{}",
                    root.path().join("Graphics").display()
                ),
            })
        );
    }

    #[test]
    fn quick_extraction_suppresses_only_success_presentation() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.document_path = Some(root.path().join("game.smc"));
        let mut transfer = ToolbarGraphicsTransfer::default();
        transfer
            .start(&app, QuickGraphicsExtraction::Standard, false, false)
            .unwrap();
        let result = loop {
            if let Some(result) = transfer.worker.poll() {
                break result;
            }
            std::thread::yield_now();
        };
        transfer.complete(result);
        assert_eq!(transfer.completion, None);
        assert_eq!(transfer.error, None);
    }

    #[test]
    fn ordinary_exgraphics_completion_uses_count_and_fixed_directory() {
        let mut transfer = ToolbarGraphicsTransfer {
            pending_presentation: Some(GraphicsExtractionPresentation {
                action: QuickGraphicsExtraction::ExGraphics,
                target: PathBuf::from("/project/ExGraphics"),
            }),
            ..Default::default()
        };
        transfer.complete(Ok(Some(17)));
        assert_eq!(
            transfer.completion,
            Some(GraphicsExtractionCompletion {
                title: "ExGFX Extraction Complete!",
                message: "17 ExGFX files have been extracted to:\n/project/ExGraphics".into(),
            })
        );
    }

    #[test]
    fn cancelled_ordinary_extraction_has_no_completion_presentation() {
        let mut transfer = ToolbarGraphicsTransfer {
            pending_presentation: Some(GraphicsExtractionPresentation {
                action: QuickGraphicsExtraction::Standard,
                target: PathBuf::from("/project/Graphics"),
            }),
            ..Default::default()
        };
        transfer.complete(Ok(None));
        assert_eq!(transfer.pending_presentation, None);
        assert_eq!(transfer.completion, None);
        assert_eq!(transfer.error, None);
    }
}
