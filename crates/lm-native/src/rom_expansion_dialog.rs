use crate::level_editor_forms;
use eframe::egui;
use lm_app::{AppState, Command, RomExpansionCommand};
use lm_rom::RomImage;

const LUNAR_MAGIC_LOROM_TARGETS: [usize; 3] = [0x20_0000, 0x30_0000, 0x40_0000];

#[derive(Default)]
pub(crate) struct RomExpansionDialog {
    open: bool,
    current_logical_len: usize,
    target: String,
    fill: String,
    error: Option<String>,
}

impl RomExpansionDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        match app.controller_snapshot() {
            Ok(snapshot) => match RomImage::from_bytes(snapshot.rom_bytes) {
                Ok(image) => {
                    self.current_logical_len = image.logical_len();
                    self.target = format!("{:X}", suggested_target(image.logical_len()));
                    // ExpandRomBackingStore at $004A7390 grows the original backing store through
                    // ZeroRomRange. Keep the advanced field visible, but default the installed
                    // workflow to Lunar Magic's exact fill rather than allocator-friendly $FF.
                    self.fill = "00".into();
                    self.open = true;
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let mut command = None;
        if self.open {
            egui::Window::new("Expand ROM")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("Target logical ROM size in hexadecimal bytes.");
                    ui.label("The target must be larger, 32 KiB aligned, and mapper-addressable.");
                    ui.horizontal(|ui| {
                        ui.label("Lunar Magic target:");
                        for (target, label) in LUNAR_MAGIC_LOROM_TARGETS
                            .into_iter()
                            .zip(["2 MiB", "3 MiB", "4 MiB"])
                        {
                            if ui
                                .add_enabled(
                                    target > self.current_logical_len,
                                    egui::Button::new(label),
                                )
                                .clicked()
                            {
                                self.target = format!("{target:X}");
                                self.fill = "00".into();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Target");
                        ui.text_edit_singleline(&mut self.target);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Fill byte");
                        ui.text_edit_singleline(&mut self.fill);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                        if ui.button("Expand transactionally").clicked() {
                            match build_command(app, &self.target, &self.fill) {
                                Ok(value) => {
                                    command = Some(value);
                                }
                                Err(error) => self.error = Some(error),
                            }
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM expansion error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
        command
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.open = false;
    }
}

fn build_command(app: &AppState, target: &str, fill: &str) -> Result<Command, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let target_logical_len = level_editor_forms::parse_hex_u32(target, "target size")? as usize;
    let fill = level_editor_forms::parse_hex_u8(fill, "expansion fill byte")?;
    Ok(Command::ExpandRom(RomExpansionCommand {
        expected_revision: snapshot.revision,
        mapper: snapshot.identity.mapper,
        target_logical_len,
        fill,
        checksum_field: snapshot.identity.internal_header_offset + 0x1c,
    }))
}

fn suggested_target(current: usize) -> usize {
    LUNAR_MAGIC_LOROM_TARGETS
        .into_iter()
        .find(|&target| target > current)
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_target_follows_the_original_fixed_lorom_commands() {
        assert_eq!(suggested_target(0x80_000), 0x20_0000);
        assert_eq!(suggested_target(0x20_0000), 0x30_0000);
        assert_eq!(suggested_target(0x30_0000), 0x40_0000);
        assert_eq!(suggested_target(0x40_0000), 0x40_0000);
    }

    #[test]
    fn prepared_dialog_remains_open_until_commit_acknowledgement() {
        let mut dialog = RomExpansionDialog {
            open: true,
            ..RomExpansionDialog::default()
        };
        assert!(dialog.open);
        dialog.commit_succeeded();
        assert!(!dialog.open);
    }

    #[test]
    fn pristine_dialog_defaults_to_lunar_magics_two_mib_zero_fill_command() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut dialog = RomExpansionDialog::default();
        dialog.open(&app);
        assert!(dialog.open);
        assert_eq!(dialog.current_logical_len, 0x80_000);
        assert_eq!(dialog.target, "200000");
        assert_eq!(dialog.fill, "00");
    }
}
