use crate::level_editor_forms;
use eframe::egui;
use lm_app::{AppState, Command, RomExpansionCommand};
use lm_rom::RomImage;

#[derive(Default)]
pub(crate) struct RomExpansionDialog {
    open: bool,
    target: String,
    fill: String,
    error: Option<String>,
}

impl RomExpansionDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        match app.controller_snapshot() {
            Ok(snapshot) => match RomImage::from_bytes(snapshot.rom_bytes) {
                Ok(image) => {
                    self.target = format!("{:X}", suggested_target(image.logical_len()));
                    self.fill = "FF".into();
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
    current
        .checked_div(0x8000)
        .and_then(|banks| banks.checked_add(1))
        .and_then(|banks| banks.checked_mul(0x8000))
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_target_advances_to_another_complete_bank() {
        assert_eq!(suggested_target(0x8000), 0x10000);
        assert_eq!(suggested_target(0x8001), 0x10000);
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
}
