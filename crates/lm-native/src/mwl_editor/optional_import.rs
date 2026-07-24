use super::{BoundedRead, MwlEditor, OptionalAssetsInterpretation, PendingLoad, dialogs};
use crate::{animation_modes, document_loader::LoadedDocument};
use eframe::egui;
use lm_app::MwlDocumentController;
use lm_level::MwlFile;
use lm_project::MwlOptionalLevelAssets;

impl MwlEditor {
    pub(super) fn optional_assets_import_controls(&mut self, ui: &mut egui::Ui) {
        ui.label("Typed palette and ExAnimation import");
        ui.horizontal(|ui| {
            ui.label("Maximum ExAnimation records:");
            ui.text_edit_singleline(&mut self.optional_maximum_records);
            let enabled = !self.loader.is_running() && !self.persistence.is_running();
            if ui
                .add_enabled(enabled, egui::Button::new("Import from MWL…"))
                .clicked()
            {
                self.begin_optional_assets_import();
            }
            if ui
                .add_enabled(enabled, egui::Button::new("Interpret current sections…"))
                .clicked()
            {
                self.begin_optional_assets_interpretation();
            }
        });
        ui.label(
            "Select a source MWL and its exact 256-byte size-mode table. Other target sections are preserved.",
        );
    }

    fn begin_optional_assets_interpretation(&mut self) {
        let maximum_records = match parse_maximum_records(&self.optional_maximum_records) {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(size_modes) = dialogs::choose_exanimation_size_modes() else {
            return;
        };
        match self.loader.start(vec![BoundedRead::new(
            size_modes,
            256,
            "ExAnimation size-mode table",
        )]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad::OptionalInterpretation { maximum_records });
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_optional_assets_import(&mut self) {
        let maximum_records = match parse_maximum_records(&self.optional_maximum_records) {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(source) = dialogs::choose_mwl_document() else {
            return;
        };
        let Some(size_modes) = dialogs::choose_exanimation_size_modes() else {
            return;
        };
        match self.loader.start(vec![
            BoundedRead::new(
                source,
                u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
                "source MWL optional assets",
            ),
            BoundedRead::new(size_modes, 256, "ExAnimation size-mode table"),
        ]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad::OptionalAssets { maximum_records });
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn poll_load(&mut self, context: &egui::Context) {
        let Some(result) = self.loader.show(context) else {
            return;
        };
        let Some(pending) = self.pending_load.take() else {
            self.error = Some("MWL loader completed without a pending operation".into());
            return;
        };
        match (pending, result) {
            (_, Err(error)) => self.error = Some(error),
            (PendingLoad::Open, Ok(loaded)) => match decode_open(loaded) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.invalidate();
                }
                Err(error) => self.error = Some(error),
            },
            (PendingLoad::OptionalInterpretation { maximum_records }, Ok(loaded)) => {
                let result = self
                    .controller
                    .as_ref()
                    .ok_or_else(|| "no MWL document is open".to_string())
                    .and_then(|controller| {
                        decode_interpretation(controller, loaded, maximum_records)
                    });
                match result {
                    Ok(interpretation) => {
                        self.optional_interpretation = Some(interpretation);
                        self.optional_panel.invalidate();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            (PendingLoad::OptionalAssets { maximum_records }, Ok(loaded)) => {
                let result = self
                    .controller
                    .as_mut()
                    .ok_or_else(|| "no MWL document is open".to_string())
                    .and_then(|controller| {
                        apply_optional_assets(controller, loaded, maximum_records)
                    });
                match result {
                    Ok(interpretation) => {
                        self.optional_interpretation = Some(interpretation);
                        self.optional_panel.invalidate();
                        self.invalidate();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
}

fn parse_maximum_records(text: &str) -> Result<usize, String> {
    let value = text
        .trim()
        .parse::<usize>()
        .map_err(|error| format!("invalid maximum ExAnimation record count: {error}"))?;
    if value == 0 {
        return Err("maximum ExAnimation record count must be nonzero".into());
    }
    Ok(value)
}

fn decode_open(loaded: LoadedDocument) -> Result<MwlDocumentController, String> {
    let [(path, bytes)] = loaded.into_exact::<1>("MWL")?;
    MwlDocumentController::decode(path, &bytes).map_err(|error| error.to_string())
}

fn apply_optional_assets(
    controller: &mut MwlDocumentController,
    loaded: LoadedDocument,
    maximum_records: usize,
) -> Result<OptionalAssetsInterpretation, String> {
    let [(_, source_bytes), (_, mode_bytes)] =
        loaded.into_exact::<2>("MWL optional-assets import")?;
    let source = MwlFile::decode(&source_bytes).map_err(|error| error.to_string())?;
    let modes = animation_modes::decode(&mode_bytes)?;
    controller
        .import_optional_assets(controller.revision(), &source, maximum_records, &modes)
        .map_err(|error| error.to_string())?;
    Ok(OptionalAssetsInterpretation {
        maximum_records,
        modes,
    })
}

fn decode_interpretation(
    controller: &MwlDocumentController,
    loaded: LoadedDocument,
    maximum_records: usize,
) -> Result<OptionalAssetsInterpretation, String> {
    let [(_, mode_bytes)] = loaded.into_exact::<1>("MWL optional-assets interpretation")?;
    let modes = animation_modes::decode(&mode_bytes)?;
    MwlOptionalLevelAssets::decode(controller.value(), maximum_records, &modes)
        .map_err(|error| error.to_string())?;
    Ok(OptionalAssetsInterpretation {
        maximum_records,
        modes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette};
    use lm_level::{MwlSection, MwlSectionKind};
    use lm_project::MwlOptionalLevelAssets;
    use std::path::PathBuf;

    fn target() -> MwlDocumentController {
        let mut file = MwlFile::default();
        file.sections[MwlSectionKind::Layer1 as usize] = MwlSection {
            bytes: vec![1, 2, 3],
        };
        MwlDocumentController::decode("target.mwl".into(), &file.encode().unwrap()).unwrap()
    }

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [7, 0x10_8031],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [0, 0x10_97e9],
            exanimation: Some(CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn loaded_group_imports_both_sections_as_one_undoable_revision() {
        let expected = assets();
        let mut source = MwlFile::default();
        expected.install_into(&mut source, &[false; 256]).unwrap();
        let mut controller = target();

        apply_optional_assets(
            &mut controller,
            LoadedDocument {
                files: vec![
                    (PathBuf::from("source.mwl"), source.encode().unwrap()),
                    (PathBuf::from("modes.bin"), vec![0; 256]),
                ],
            },
            32,
        )
        .unwrap();

        assert_eq!(controller.revision(), 1);
        assert_eq!(
            controller.value().section(MwlSectionKind::Layer1),
            &[1, 2, 3]
        );
        assert_eq!(
            MwlOptionalLevelAssets::decode(controller.value(), 32, &[false; 256]).unwrap(),
            expected
        );
        assert!(controller.undo(1).unwrap());
    }

    #[test]
    fn malformed_group_and_interpretation_leave_controller_unchanged() {
        let mut controller = target();
        let original = controller.value().clone();
        assert!(
            apply_optional_assets(
                &mut controller,
                LoadedDocument {
                    files: vec![
                        (PathBuf::from("source.mwl"), b"bad".to_vec()),
                        (PathBuf::from("modes.bin"), vec![0; 256]),
                    ],
                },
                32,
            )
            .is_err()
        );
        assert_eq!(controller.value(), &original);
        assert_eq!(controller.revision(), 0);
        assert!(parse_maximum_records("0").is_err());
        assert!(parse_maximum_records("bad").is_err());
    }
}
