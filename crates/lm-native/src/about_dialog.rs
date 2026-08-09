use eframe::egui;
use lm_app::{
    AppState, EditorMode, LocalizationCatalog, ProfileStatus, ProjectStatus, SaveStatus, UiTextKey,
};

use crate::frontend_ui::localized_text;

pub(crate) const PRODUCT_NAME: &str = "Lunar Magic Rust";
pub(crate) const COMPATIBILITY_TARGET: &str = "Lunar Magic 3.63 workflow compatibility";
pub(crate) const LICENSE: &str = "MIT OR Apache-2.0";
pub(crate) const SOURCE_URL: &str = "https://github.com/YungRaj/lunar-magic";
const ORIGINAL_ABOUT_DIALOG_UNITS: (f32, f32) = (248.0, 160.0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AboutAuxiliary {
    ThirdParty,
    Legal,
}

#[derive(Default)]
pub(crate) struct AboutDialog {
    open: bool,
    copied_source: bool,
    auxiliary: Option<AboutAuxiliary>,
}

impl AboutDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.copied_source = false;
        self.auxiliary = None;
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if !self.open {
            return;
        }
        let mut copied_source = self.copied_source;
        let product = localized_text(catalog, UiTextKey::AppTitle);
        let title = localized_text(catalog, UiTextKey::AboutWindowTitleFormat)
            .replace("{product}", &product);
        let mut close_requested = false;
        let mut auxiliary = self.auxiliary;
        egui::Window::new(title)
            .open(&mut self.open)
            .collapsible(false)
            .resizable(false)
            .default_size([
                ORIGINAL_ABOUT_DIALOG_UNITS.0 * 2.0,
                ORIGINAL_ABOUT_DIALOG_UNITS.1 * 2.0,
            ])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.set_min_width(480.0);
                ui.heading(&product);
                ui.label(
                    localized_text(catalog, UiTextKey::AboutVersionFormat)
                        .replace("{version}", env!("CARGO_PKG_VERSION")),
                );
                ui.label(
                    localized_text(catalog, UiTextKey::AboutBuildFormat)
                        .replace(
                            "{build}",
                            if cfg!(debug_assertions) {
                                "Debug"
                            } else {
                                "Release"
                            },
                        )
                        .replace("{os}", std::env::consts::OS)
                        .replace("{arch}", std::env::consts::ARCH),
                );
                ui.label(localized_text(catalog, UiTextKey::AboutCleanRoomIdentity));
                ui.label(localized_text(catalog, UiTextKey::AboutCompatibilityTarget));
                ui.label(
                    localized_text(catalog, UiTextKey::AboutLicenseFormat)
                        .replace("{license}", LICENSE),
                );
                ui.hyperlink_to(
                    localized_text(catalog, UiTextKey::AboutSourceRepository),
                    SOURCE_URL,
                );
                if ui
                    .button(localized_text(catalog, UiTextKey::AboutCopySourceUrl))
                    .clicked()
                {
                    ui.ctx().copy_text(SOURCE_URL.into());
                    copied_source = true;
                }
                if copied_source {
                    ui.label(localized_text(catalog, UiTextKey::AboutSourceCopied));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(localized_text(
                            catalog,
                            UiTextKey::AboutThirdPartyEnhancements,
                        ))
                        .clicked()
                    {
                        auxiliary = Some(AboutAuxiliary::ThirdParty);
                    }
                    if ui
                        .button(localized_text(catalog, UiTextKey::AboutLegalNotice))
                        .clicked()
                    {
                        auxiliary = Some(AboutAuxiliary::Legal);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(localized_text(catalog, UiTextKey::AboutOk))
                            .clicked()
                        {
                            close_requested = true;
                        }
                    });
                });
            });
        if close_requested {
            self.open = false;
            auxiliary = None;
        }
        if !self.open {
            auxiliary = None;
        }
        self.auxiliary = auxiliary;
        self.copied_source = copied_source;
        self.show_auxiliary(context, catalog);
    }

    fn show_auxiliary(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(auxiliary) = self.auxiliary else {
            return;
        };
        let (title, body) = match auxiliary {
            AboutAuxiliary::ThirdParty => (
                UiTextKey::AboutThirdPartyTitle,
                UiTextKey::AboutThirdPartyBody,
            ),
            AboutAuxiliary::Legal => (UiTextKey::AboutLegalTitle, UiTextKey::AboutLegalBody),
        };
        let mut open = true;
        let mut close_requested = false;
        egui::Window::new(localized_text(catalog, title))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.set_max_width(480.0);
                ui.label(localized_text(catalog, body));
                ui.add_space(8.0);
                if ui
                    .button(localized_text(catalog, UiTextKey::AboutOk))
                    .clicked()
                {
                    close_requested = true;
                }
            });
        if !open || close_requested {
            self.auxiliary = None;
        }
    }
}

#[derive(Default)]
pub(crate) struct DiagnosticsDialog {
    open: bool,
    copied: bool,
    report: String,
}

impl DiagnosticsDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        self.open = true;
        self.copied = false;
        let compatibility = app.rom_compatibility_report();
        self.report = format!("{}\n\n{}", diagnostic_report(app), compatibility.text);
    }

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if !self.open {
            return;
        }
        let report = self.report.clone();
        let mut copied = self.copied;
        egui::Window::new(localized_text(catalog, UiTextKey::DiagnosticsWindowTitle))
            .open(&mut self.open)
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(localized_text(catalog, UiTextKey::DiagnosticsIntroduction));
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .show(ui, |ui| {
                        ui.monospace(&report);
                    });
                if ui
                    .button(localized_text(catalog, UiTextKey::DiagnosticsCopy))
                    .clicked()
                {
                    ui.ctx().copy_text(report.clone());
                    copied = true;
                }
                if copied {
                    ui.label(localized_text(catalog, UiTextKey::DiagnosticsCopied));
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
        dialog.auxiliary = Some(AboutAuxiliary::Legal);
        dialog.open();
        dialog.open();
        assert!(dialog.open);
        assert!(!dialog.copied_source);
        assert_eq!(dialog.auxiliary, None);
    }

    #[test]
    fn retained_lunar_magic_about_oracle_matches_the_compatibility_target() {
        let controls =
            include_str!("../../../docs/oracle-work/lm363/help-about/about-controls.tsv");
        for expected in [
            "008A\tButton\tLunar Magic : Super Mario World Level Editor",
            "0001\tButton\tOK",
            "008B\tStatic\thttp://fusoya.eludevisibility.org",
            "008E\tStatic\tPublic Build x86 --- Dec 25 2025",
            "008D\tStatic\tVersion 3.63",
            "0066\tButton\tThird Party Enhancements",
            "0067\tButton\tLegal Notice",
        ] {
            assert!(
                controls.lines().any(|line| line == expected),
                "missing {expected}"
            );
        }
        assert!(COMPATIBILITY_TARGET.contains("3.63"));
        assert_eq!(controls.lines().count(), 8);

        let layout = include_str!("../../../docs/oracle-work/lm363/help-about/about-layout.tsv");
        let rows = layout
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 11);
        assert_eq!(
            rows[0],
            [
                "dialog",
                "Dialog",
                "0",
                "0",
                "248",
                "160",
                "About Lunar Magic"
            ]
        );
        for (id, role) in [
            ("008A", "group"),
            ("008B", "website"),
            ("008D", "version"),
            ("008E", "build"),
            ("0066", "third-party"),
            ("0067", "legal"),
            ("0001", "ok"),
        ] {
            assert!(rows.iter().any(|row| row[0] == id && row[6] == role));
        }
        assert_eq!(ORIGINAL_ABOUT_DIALOG_UNITS, (248.0, 160.0));
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
        let mut app = AppState::default();
        let mut dialog = DiagnosticsDialog {
            copied: true,
            ..DiagnosticsDialog::default()
        };
        dialog.open(&app);
        assert!(dialog.open);
        assert!(!dialog.copied);
        assert!(dialog.report.contains("ROM compatibility: no project open"));
        let captured = dialog.report.clone();
        app.mode = EditorMode::Level(0x105);
        assert_eq!(dialog.report, captured);
    }
}
