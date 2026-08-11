use eframe::egui;
use lm_app::{AppState, RomUserAreaReport};
use lm_rats::RatsConflict;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

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

    pub(crate) fn show(&mut self, context: &egui::Context) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new("ROM User Area Scan Results")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                metric_grid(ui, report);
                if report.scan.conflicting_rats != 0 {
                    ui.separator();
                    ui.colored_label(ui.visuals().warn_fg_color, "See RATS.log for more info.");
                }
                ui.add_space(4.0);
                ui.vertical_centered(|ui| dismiss = ui.button("OK").clicked());
            });
        if !open || dismiss {
            self.report = None;
        }
    }
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

fn metric_grid(ui: &mut egui::Ui, report: &RomUserAreaReport) {
    let scan = &report.scan;
    let metrics = [
        ("RAT Protected Space:", scan.rat_protected_space),
        ("Unprotected Map16:", scan.unprotected_map16),
        ("Unprotected Used Space:", scan.unprotected_used_space),
        ("Unusable Space:", scan.unusable_space),
        ("Free Space:", scan.free_space),
        ("Total User Space:", scan.total_user_space),
        ("Conflicting RATs:", scan.conflicting_rats),
        ("Conflicted Space:", scan.conflicting_space),
        ("RAT Structures:", scan.rat_structures),
        ("Largest Free 32KB Bank:", scan.largest_free_32kb_bank),
        ("Largest Free Area:", scan.largest_free_area),
    ];
    egui::Grid::new("rom-user-area-scan-metrics")
        .num_columns(2)
        .spacing([24.0, 2.0])
        .show(ui, |ui| {
            for (label, value) in metrics {
                ui.label(label);
                ui.monospace(format!("{value:X}"));
                ui.end_row();
            }
            ui.label("Last version of Lunar Magic used:");
            ui.monospace(report.last_lunar_magic_version.as_deref().unwrap_or("N/A"));
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
