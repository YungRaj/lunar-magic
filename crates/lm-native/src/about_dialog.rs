use eframe::egui;
use lm_app::{AppState, EditorMode, ProfileStatus, ProjectStatus, SaveStatus};

pub(crate) const PRODUCT_NAME: &str = "Lunar Magic Rust";
pub(crate) const COMPATIBILITY_TARGET: &str = "Lunar Magic 3.63 workflow compatibility";
pub(crate) const LICENSE: &str = "MIT OR Apache-2.0";
pub(crate) const SOURCE_URL: &str = "https://github.com/YungRaj/lunar-magic";

#[derive(Default)]
pub(crate) struct AboutDialog {
    open: bool,
    copied_source: bool,
}

impl AboutDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.copied_source = false;
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if !self.open {
            return;
        }
        let mut copied_source = self.copied_source;
        egui::Window::new(format!("About {PRODUCT_NAME}"))
            .open(&mut self.open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.heading(PRODUCT_NAME);
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Clean-room Rust reimplementation");
                ui.label(COMPATIBILITY_TARGET);
                ui.label(format!("License: {LICENSE}"));
                ui.hyperlink_to("Source repository", SOURCE_URL);
                if ui.button("Copy source URL").clicked() {
                    ui.ctx().copy_text(SOURCE_URL.into());
                    copied_source = true;
                }
                if copied_source {
                    ui.label("Source URL copied.");
                }
            });
        self.copied_source = copied_source;
    }
}

#[derive(Default)]
pub(crate) struct DiagnosticsDialog {
    open: bool,
    copied: bool,
}

impl DiagnosticsDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.copied = false;
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) {
        if !self.open {
            return;
        }
        let report = diagnostic_report(app);
        let mut copied = self.copied;
        egui::Window::new("Build diagnostics")
            .open(&mut self.open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Non-sensitive build information for compatibility reports:");
                ui.monospace(&report);
                if ui.button("Copy diagnostics").clicked() {
                    ui.ctx().copy_text(report.clone());
                    copied = true;
                }
                if copied {
                    ui.label("Diagnostics copied.");
                }
            });
        self.copied = copied;
    }
}

pub(crate) fn diagnostic_report(app: &AppState) -> String {
    let capabilities = app.capabilities();
    format!(
        "Product: {PRODUCT_NAME}\nVersion: {}\nCompatibility: {COMPATIBILITY_TARGET}\nTarget OS: {}\nTarget architecture: {}\nBuild: {}\nLicense: {LICENSE}\nProject: {}\nEditor: {}\nProfile: {}\nSave: {}\nUndo available: {}\nRedo available: {}\nCurrent level: {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        project_status(capabilities.project),
        editor_mode(app.mode),
        profile_status(capabilities.profile),
        save_status(capabilities.save),
        capabilities.history.undo,
        capabilities.history.redo,
        app.current_level()
            .map_or_else(|| "none".into(), |level| format!("{level:03X}"))
    )
}

const fn project_status(status: ProjectStatus) -> &'static str {
    match status {
        ProjectStatus::Closed => "closed",
        ProjectStatus::OpenClean => "open-clean",
        ProjectStatus::OpenModified => "open-modified",
    }
}

const fn profile_status(status: ProfileStatus) -> &'static str {
    match status {
        ProfileStatus::Missing => "missing",
        ProfileStatus::Loaded => "loaded",
    }
}

const fn save_status(status: SaveStatus) -> &'static str {
    match status {
        SaveStatus::Idle => "idle",
        SaveStatus::Pending => "pending",
    }
}

fn editor_mode(mode: EditorMode) -> String {
    match mode {
        EditorMode::NoProject => "none".into(),
        EditorMode::Level(level) => format!("level-{level:03X}"),
        EditorMode::Overworld => "overworld".into(),
        EditorMode::Map16 => "map16".into(),
        EditorMode::Graphics(slot) => format!("graphics-{slot:03X}"),
        EditorMode::Palette(slot) => format!("palette-{slot:03X}"),
        EditorMode::ExAnimation(slot) => format!("exanimation-{slot:03X}"),
        EditorMode::Layer3(slot) => format!("layer3-{slot:03X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_identity_is_explicit_and_build_bound() {
        assert_eq!(PRODUCT_NAME, "Lunar Magic Rust");
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
        assert!(COMPATIBILITY_TARGET.contains("3.63"));
        assert_eq!(LICENSE, "MIT OR Apache-2.0");
        assert!(SOURCE_URL.starts_with("https://github.com/"));
    }

    #[test]
    fn about_dialog_open_is_idempotent() {
        let mut dialog = AboutDialog::default();
        assert!(!dialog.open);
        dialog.copied_source = true;
        dialog.open();
        dialog.open();
        assert!(dialog.open);
        assert!(!dialog.copied_source);
    }

    #[test]
    fn diagnostics_are_bounded_to_non_sensitive_build_identity() {
        let report = diagnostic_report(&AppState::default());
        for field in [
            "Product: Lunar Magic Rust",
            "Version: ",
            "Compatibility: Lunar Magic 3.63",
            "Target OS: ",
            "Target architecture: ",
            "Build: ",
            "License: MIT OR Apache-2.0",
            "Project: closed",
            "Editor: none",
            "Profile: missing",
            "Save: idle",
            "Undo available: false",
            "Redo available: false",
            "Current level: none",
        ] {
            assert!(report.contains(field), "missing diagnostic field {field:?}");
        }
        assert_eq!(report.lines().count(), 14);
        assert!(!report.contains("/Users/"));
    }

    #[test]
    fn editor_modes_are_stable_and_do_not_include_paths() {
        for (mode, expected) in [
            (EditorMode::NoProject, "none"),
            (EditorMode::Level(0x105), "level-105"),
            (EditorMode::Overworld, "overworld"),
            (EditorMode::Map16, "map16"),
            (EditorMode::Graphics(0x80), "graphics-080"),
            (EditorMode::Palette(2), "palette-002"),
            (EditorMode::ExAnimation(3), "exanimation-003"),
            (EditorMode::Layer3(4), "layer3-004"),
        ] {
            assert_eq!(editor_mode(mode), expected);
        }
    }

    #[test]
    fn runtime_status_labels_cover_every_public_state() {
        assert_eq!(project_status(ProjectStatus::Closed), "closed");
        assert_eq!(project_status(ProjectStatus::OpenClean), "open-clean");
        assert_eq!(project_status(ProjectStatus::OpenModified), "open-modified");
        assert_eq!(profile_status(ProfileStatus::Missing), "missing");
        assert_eq!(profile_status(ProfileStatus::Loaded), "loaded");
        assert_eq!(save_status(SaveStatus::Idle), "idle");
        assert_eq!(save_status(SaveStatus::Pending), "pending");
    }

    #[test]
    fn diagnostics_dialog_open_resets_copy_confirmation() {
        let mut dialog = DiagnosticsDialog {
            copied: true,
            ..DiagnosticsDialog::default()
        };
        dialog.open();
        assert!(dialog.open);
        assert!(!dialog.copied);
    }
}
