use eframe::egui;
use lm_app::{AppState, LocalizationCatalog, RomUserAreaReport};
use lm_rats::RatsConflict;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

const ORIGINAL_DIALOG_ID: u16 = 0x0427;
const FALLBACK_METRIC_LABELS: [&str; 12] = [
    "RAT Protected Space:",
    "Unprotected Map16:",
    "Unprotected Used Space:",
    "Unusable Space:",
    "Free Space:",
    "Total User Space:",
    "Conflicting RATs:",
    "Conflicted Space:",
    "RAT Structures:",
    "Largest Free 32KB Bank:",
    "Largest Free Area:",
    "Last version of Lunar Magic used:",
];

#[derive(Default)]
pub(crate) struct RomUserAreaScanDialog {
    report: Option<RomUserAreaReport>,
}

impl RomUserAreaScanDialog {
    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        self.report.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) -> Result<(), String> {
        let report = app
            .rom_user_area_report()
            .ok_or_else(|| "open a supported SMW ROM before scanning its user area".to_owned())?;
        if !report.scan.conflicts.is_empty() {
            let rom_path = app.document_path.as_deref().ok_or_else(|| {
                "save the ROM before scanning conflicting RAT structures so RATS.log can be written"
                    .to_owned()
            })?;
            append_conflict_log(rom_path, &report)?;
        }
        self.report = Some(report);
        Ok(())
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new(user_area_dialog_title(catalog))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                metric_grid(ui, report, catalog);
                if report.scan.conflicting_rats != 0 {
                    ui.separator();
                    ui.colored_label(ui.visuals().warn_fg_color, "See RATS.log for more info.");
                }
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    dismiss = ui.button(user_area_dialog_text(catalog, 1, "OK")).clicked();
                });
            });
        if !open || dismiss {
            self.report = None;
        }
    }
}

fn user_area_dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("ROM User Area Scan Results")
        .to_owned()
}

fn user_area_dialog_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
        .to_owned()
}

fn user_area_metric_labels(catalog: Option<&LocalizationCatalog>) -> [String; 12] {
    let mut labels = FALLBACK_METRIC_LABELS.map(str::to_owned);
    for (control_id, range) in [(0x68, 0..6), (0x65, 6..8), (0x66, 8..11)] {
        let Some(text) = catalog.and_then(|catalog| {
            catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id)
        }) else {
            continue;
        };
        let lines = text.lines().collect::<Vec<_>>();
        if lines.len() != range.len() || lines.iter().any(|line| line.is_empty()) {
            continue;
        }
        for (index, line) in range.zip(lines) {
            labels[index] = line.to_owned();
        }
    }
    labels[11] = user_area_dialog_text(catalog, 0x67, FALLBACK_METRIC_LABELS[11]);
    labels
}

fn append_conflict_log(rom_path: &Path, report: &RomUserAreaReport) -> Result<(), String> {
    let path = conflict_log_path(rom_path)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(format!(
                "refusing to append ROM scan results to non-regular file {}",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("could not inspect {}: {error}", path.display()));
        }
    }
    let timestamp = chrono::Local::now().format("%m/%d/%Y  %I:%M:%S %p");
    let entry = format_conflict_log_entries(
        &timestamp.to_string(),
        report.physical_offset_base,
        &report.scan.conflicts,
    );
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("could not open {} for append: {error}", path.display()))?;
    file.write_all(entry.as_bytes())
        .map_err(|error| format!("could not append {}: {error}", path.display()))
}

fn conflict_log_path(rom_path: &Path) -> Result<PathBuf, String> {
    let parent = rom_path.parent().ok_or_else(|| {
        format!(
            "ROM path {} has no directory for RATS.log",
            rom_path.display()
        )
    })?;
    Ok(parent.join("RATS.log"))
}

fn format_conflict_log_entries(
    timestamp: &str,
    physical_offset_base: usize,
    conflicts: &[RatsConflict],
) -> String {
    let mut text = String::new();
    for conflict in conflicts {
        use std::fmt::Write as _;
        let _ = write!(
            text,
            "{timestamp}  Lunar Magic Rust {}\tNested RAT discovered in scan!  First RAT address: {:06X}, Size: {:06X}.  Nested RAT address: {:06X}, Size: {:06X}.  Overlapped Size: {:06X}.\r\n",
            env!("CARGO_PKG_VERSION"),
            conflict.first_range.start + physical_offset_base,
            conflict.first_range.len(),
            conflict.nested_range.start + physical_offset_base,
            conflict.nested_range.len(),
            conflict.overlapped_space,
        );
    }
    text
}

fn metric_grid(
    ui: &mut egui::Ui,
    report: &RomUserAreaReport,
    catalog: Option<&LocalizationCatalog>,
) {
    let scan = &report.scan;
    let labels = user_area_metric_labels(catalog);
    let values = [
        scan.rat_protected_space,
        scan.unprotected_map16,
        scan.unprotected_used_space,
        scan.unusable_space,
        scan.free_space,
        scan.total_user_space,
        scan.conflicting_rats,
        scan.conflicting_space,
        scan.rat_structures,
        scan.largest_free_32kb_bank,
        scan.largest_free_area,
    ];
    egui::Grid::new("rom-user-area-scan-metrics")
        .num_columns(2)
        .spacing([24.0, 2.0])
        .show(ui, |ui| {
            for (label, value) in labels.iter().zip(values) {
                ui.label(label);
                ui.monospace(format!("{value:X}"));
                ui.end_row();
            }
            ui.label(&labels[11]);
            ui.monospace(report.last_lunar_magic_version.as_deref().unwrap_or("N/A"));
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::{OriginalDialogTextKey, UiTextKey};
    use lm_rats::RomUserAreaScan;

    #[test]
    fn original_dialog_metrics_are_all_present_in_display_order() {
        let report = RomUserAreaReport {
            scan: RomUserAreaScan {
                rat_protected_space: 1,
                unprotected_map16: 2,
                unprotected_used_space: 3,
                unusable_space: 4,
                free_space: 5,
                total_user_space: 6,
                conflicting_rats: 7,
                conflicting_space: 8,
                rat_structures: 9,
                largest_free_32kb_bank: 10,
                largest_free_area: 11,
                conflicting_offsets: vec![0x1234],
                conflicts: Vec::new(),
            },
            last_lunar_magic_version: Some("3.63".into()),
            physical_offset_base: 0x200,
        };
        assert_eq!(report.scan.total_user_space, 6);
        assert_eq!(report.last_lunar_magic_version.as_deref(), Some("3.63"));
    }

    #[test]
    fn conflict_log_uses_physical_offsets_and_original_message_shape() {
        let text = format_conflict_log_entries(
            "08/10/2026  04:24:52 PM",
            0x200,
            &[RatsConflict {
                first_range: 0x100000..0x100030,
                nested_range: 0x100010..0x100020,
                overlapped_space: 0x10,
            }],
        );
        assert_eq!(
            text,
            format!(
                "08/10/2026  04:24:52 PM  Lunar Magic Rust {}\tNested RAT discovered in scan!  First RAT address: 100200, Size: 000030.  Nested RAT address: 100210, Size: 000010.  Overlapped Size: 000010.\r\n",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn conflict_log_rejects_an_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("RATS.log")).unwrap();
        let report = RomUserAreaReport {
            scan: RomUserAreaScan {
                conflicts: vec![RatsConflict {
                    first_range: 0..16,
                    nested_range: 8..16,
                    overlapped_space: 8,
                }],
                ..RomUserAreaScan::default()
            },
            last_lunar_magic_version: None,
            physical_offset_base: 0,
        };
        let error = append_conflict_log(&root.path().join("game.sfc"), &report).unwrap_err();
        assert!(error.contains("non-regular file"));
    }

    #[test]
    fn original_scan_template_splits_grouped_metric_labels_and_round_trips() {
        let catalog = LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Résultats de l’analyse ROM".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x68,
                },
                "Protégé:\r\nMap16 libre:\r\nUtilisé:\r\nInutilisable:\r\nLibre:\r\nTotal:".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 2,
                    control_id: 0x65,
                },
                "RAT en conflit:\r\nEspace en conflit:".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 3,
                    control_id: 0x66,
                },
                "Structures RAT:\r\nBanque libre:\r\nZone libre:".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 4,
                    control_id: 0x67,
                },
                "Dernière version de Lunar Magic:".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 5,
                    control_id: 1,
                },
                "Fermer".into(),
            ),
        ])
        .unwrap();

        let labels = user_area_metric_labels(Some(&catalog));
        assert_eq!(labels[0], "Protégé:");
        assert_eq!(labels[5], "Total:");
        assert_eq!(labels[6], "RAT en conflit:");
        assert_eq!(labels[10], "Zone libre:");
        assert_eq!(labels[11], "Dernière version de Lunar Magic:");
        assert_eq!(
            user_area_dialog_title(Some(&catalog)),
            "Résultats de l’analyse ROM"
        );
        assert_eq!(user_area_dialog_text(Some(&catalog), 1, "OK"), "Fermer");

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(user_area_metric_labels(Some(&reopened)), labels);
    }

    #[test]
    fn malformed_grouped_template_falls_back_without_shifting_metric_meanings() {
        let catalog = LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_original_dialog_texts([(
            OriginalDialogTextKey {
                dialog_id: ORIGINAL_DIALOG_ID,
                item_index: 1,
                control_id: 0x68,
            },
            "Seulement une ligne".into(),
        )])
        .unwrap();

        assert_eq!(
            user_area_metric_labels(Some(&catalog)),
            FALLBACK_METRIC_LABELS.map(str::to_owned)
        );
        assert_eq!(user_area_dialog_title(None), "ROM User Area Scan Results");
    }
}
