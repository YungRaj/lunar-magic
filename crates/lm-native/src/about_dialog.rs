use eframe::egui;

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

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if !self.open {
            return;
        }
        let report = diagnostic_report();
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

pub(crate) fn diagnostic_report() -> String {
    format!(
        "Product: {PRODUCT_NAME}\nVersion: {}\nCompatibility: {COMPATIBILITY_TARGET}\nTarget OS: {}\nTarget architecture: {}\nBuild: {}\nLicense: {LICENSE}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    )
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
        let report = diagnostic_report();
        for field in [
            "Product: Lunar Magic Rust",
            "Version: ",
            "Compatibility: Lunar Magic 3.63",
            "Target OS: ",
            "Target architecture: ",
            "Build: ",
            "License: MIT OR Apache-2.0",
        ] {
            assert!(report.contains(field), "missing diagnostic field {field:?}");
        }
        assert_eq!(report.lines().count(), 7);
        assert!(!report.contains("/Users/"));
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
