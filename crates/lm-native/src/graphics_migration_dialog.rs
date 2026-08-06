use crate::rom_allocation;
use eframe::egui;
use lm_app::{AppState, Command};
use lm_project::{GraphicsCompression, GraphicsMigrationOptions};
use lm_rom::RomImage;

pub(crate) struct GraphicsMigrationDialog {
    open: bool,
    target: GraphicsCompression,
    allocation_start: String,
    allocation_end: String,
    error: Option<String>,
}

impl Default for GraphicsMigrationDialog {
    fn default() -> Self {
        Self {
            open: false,
            target: GraphicsCompression::Lz3,
            allocation_start: "080000".into(),
            allocation_end: "400000".into(),
            error: None,
        }
    }
}

impl GraphicsMigrationDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        let Some(profile) = app.revision_profile() else {
            self.error =
                Some("install a matching revision profile before migrating graphics".into());
            return;
        };
        self.target = match profile.graphics.compression {
            GraphicsCompression::Lz2 => GraphicsCompression::Lz3,
            GraphicsCompression::Lz3 => GraphicsCompression::Lz2,
        };
        self.open = true;
        self.error = None;
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let mut command = None;
        if self.open {
            egui::Window::new("Migrate Graphics Compression")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        "Recompress and repoint every profile-declared graphics slot. Installed \
                         SMW graphics require the matching in-game runtime migration and are \
                         rejected until that transaction is available.",
                    );
                    egui::ComboBox::from_label("Target codec")
                        .selected_text(codec_name(self.target))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.target, GraphicsCompression::Lz2, "LZ2");
                            ui.selectable_value(&mut self.target, GraphicsCompression::Lz3, "LZ3");
                        });
                    ui.label("End-exclusive logical-PC allocation range (hexadecimal).");
                    ui.horizontal(|ui| {
                        ui.label("Start");
                        ui.text_edit_singleline(&mut self.allocation_start);
                    });
                    ui.horizontal(|ui| {
                        ui.label("End");
                        ui.text_edit_singleline(&mut self.allocation_end);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                        if ui.button("Migrate transactionally").clicked() {
                            match build_command(
                                app,
                                self.target,
                                &self.allocation_start,
                                &self.allocation_end,
                            ) {
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
            egui::Window::new("Graphics migration error").show(context, |ui| {
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

fn build_command(
    app: &AppState,
    target: GraphicsCompression,
    start: &str,
    end: &str,
) -> Result<Command, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    let profile = app.revision_profile().ok_or_else(|| {
        "install a matching revision profile before migrating graphics".to_string()
    })?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let allocation = profile
        .allocation_policy_for_rom(
            rom_allocation::parse_search_range(start, end)?,
            &image,
            snapshot.identity.internal_header_offset,
        )
        .map_err(|error| error.to_string())?;
    Ok(Command::MigrateGraphicsCompression {
        expected_revision: snapshot.revision,
        source: profile.graphics,
        target,
        options: GraphicsMigrationOptions {
            allocation,
            reuse_identical: true,
            erase_fill: 0xff,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
        },
    })
}

const fn codec_name(codec: GraphicsCompression) -> &'static str {
    match codec {
        GraphicsCompression::Lz2 => "LZ2",
        GraphicsCompression::Lz3 => "LZ3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_dialog_remains_open_until_commit_acknowledgement() {
        let mut dialog = GraphicsMigrationDialog {
            open: true,
            ..GraphicsMigrationDialog::default()
        };
        assert!(dialog.open);
        dialog.commit_succeeded();
        assert!(!dialog.open);
    }
}
