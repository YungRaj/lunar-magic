use eframe::egui;

pub(crate) const PRODUCT_NAME: &str = "Lunar Magic Rust";
pub(crate) const COMPATIBILITY_TARGET: &str = "Lunar Magic 3.63 workflow compatibility";
pub(crate) const LICENSE: &str = "MIT OR Apache-2.0";
pub(crate) const SOURCE_URL: &str = "https://github.com/YungRaj/lunar-magic";

#[derive(Default)]
pub(crate) struct AboutDialog {
    open: bool,
}

impl AboutDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if !self.open {
            return;
        }
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
            });
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
        dialog.open();
        dialog.open();
        assert!(dialog.open);
    }
}
