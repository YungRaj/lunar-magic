use crate::rom_allocation;
use eframe::egui;
use lm_app::{AppState, Command, LocalizationCatalog, UiTextKey};
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
        let catalog = app.localization();
        if self.open {
            egui::Window::new(dialog_title(catalog))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(dialog_control_text(
                        catalog,
                        0x69,
                        "Recompress and repoint every standard, special, ExAnimation, ExGFX, and \
                         installed overworld-event stream. Installed SMW graphics switch the \
                         matching in-game decoder in the same undoable transaction.",
                    ));
                    egui::ComboBox::from_label(dialog_control_text(catalog, 0x65, "Target codec"))
                        .selected_text(codec_name(catalog, self.target))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.target,
                                GraphicsCompression::Lz2,
                                codec_name(catalog, GraphicsCompression::Lz2),
                            );
                            ui.selectable_value(
                                &mut self.target,
                                GraphicsCompression::Lz3,
                                codec_name(catalog, GraphicsCompression::Lz3),
                            );
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
                        if ui
                            .button(dialog_control_text(
                                catalog,
                                2,
                                UiTextKey::CommonCancel.english(),
                            ))
                            .clicked()
                        {
                            self.open = false;
                        }
                        if ui
                            .button(dialog_control_text(catalog, 1, "Migrate transactionally"))
                            .clicked()
                        {
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

const ORIGINAL_DIALOG_ID: u16 = 0x0416;

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("Migrate Graphics Compression")
        .to_owned()
}

fn dialog_control_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
        .to_owned()
}

fn codec_name(catalog: Option<&LocalizationCatalog>, codec: GraphicsCompression) -> String {
    match codec {
        GraphicsCompression::Lz2 => dialog_control_text(catalog, 0x294, "LZ2"),
        GraphicsCompression::Lz3 => dialog_control_text(catalog, 0x296, "LZ3"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::OriginalDialogTextKey;

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

    #[test]
    fn original_compression_inventory_localizes_every_equivalent_native_control() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Modifier les options de compression".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 0,
                    control_id: 0x65,
                },
                "Type de compression LZ".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x294,
                },
                "LC_LZ2 — code original".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 2,
                    control_id: 0x296,
                },
                "LC_LZ3 — meilleure compression".into(),
            ),
        ])
        .unwrap();

        assert_eq!(
            dialog_title(Some(&catalog)),
            "Modifier les options de compression"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x65, "Target codec"),
            "Type de compression LZ"
        );
        assert_eq!(
            codec_name(Some(&catalog), GraphicsCompression::Lz2),
            "LC_LZ2 — code original"
        );
        assert_eq!(
            codec_name(Some(&catalog), GraphicsCompression::Lz3),
            "LC_LZ3 — meilleure compression"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 2, UiTextKey::CommonCancel.english()),
            UiTextKey::CommonCancel.english()
        );
    }
}
