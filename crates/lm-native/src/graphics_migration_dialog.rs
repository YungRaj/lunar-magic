use crate::rom_allocation;
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey as Key, LocalizationCatalog, UiTextKey};
use lm_project::{GraphicsCompression, GraphicsMigrationOptions};
use lm_rom::RomImage;

pub(crate) struct GraphicsMigrationDialog {
    open: bool,
    target: GraphicsMigrationTarget,
    allocation_start: String,
    allocation_end: String,
    error: Option<String>,
}

impl Default for GraphicsMigrationDialog {
    fn default() -> Self {
        Self {
            open: false,
            target: GraphicsMigrationTarget::Lz3,
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
        self.target = match current_installed_mode(app) {
            Some(lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original) => {
                GraphicsMigrationTarget::Lz2Speed
            }
            Some(lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Speed)
            | Some(lm_profile::SmwUsV1GraphicsCompressionMode::Lz3) => GraphicsMigrationTarget::Lz3,
            None => match profile.graphics.compression {
                GraphicsCompression::Lz2 => GraphicsMigrationTarget::Lz3,
                GraphicsCompression::Lz3 => GraphicsMigrationTarget::Lz2Original,
            },
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
                        .selected_text(target_name(catalog, self.target))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.target,
                                GraphicsMigrationTarget::Lz2Original,
                                target_name(catalog, GraphicsMigrationTarget::Lz2Original),
                            );
                            ui.selectable_value(
                                &mut self.target,
                                GraphicsMigrationTarget::Lz2Speed,
                                target_name(catalog, GraphicsMigrationTarget::Lz2Speed),
                            );
                            ui.selectable_value(
                                &mut self.target,
                                GraphicsMigrationTarget::Lz3,
                                target_name(catalog, GraphicsMigrationTarget::Lz3),
                            );
                        });
                    ui.label(text(catalog, Key::GraphicsMigrationAllocationNotice));
                    ui.horizontal(|ui| {
                        ui.label(text(catalog, Key::GraphicsMigrationStart));
                        ui.text_edit_singleline(&mut self.allocation_start);
                    });
                    ui.horizontal(|ui| {
                        ui.label(text(catalog, Key::GraphicsMigrationEnd));
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
            egui::Window::new(text(catalog, Key::GraphicsMigrationErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::GraphicsMigrationOk)).clicked() {
                        self.error = None;
                    }
                },
            );
        }
        command
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.open = false;
    }
}

fn build_command(
    app: &AppState,
    target: GraphicsMigrationTarget,
    start: &str,
    end: &str,
) -> Result<Command, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    if target == GraphicsMigrationTarget::Lz2Speed {
        return Ok(Command::InstallLz2SpeedRuntime {
            rev: snapshot.revision,
        });
    }
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
        target: target.codec(),
        options: GraphicsMigrationOptions {
            allocation,
            reuse_identical: true,
            erase_fill: 0xff,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
        },
    })
}

const ORIGINAL_DIALOG_ID: u16 = 0x0416;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsMigrationTarget {
    Lz2Original,
    Lz2Speed,
    Lz3,
}

impl GraphicsMigrationTarget {
    const fn codec(self) -> GraphicsCompression {
        match self {
            Self::Lz2Original | Self::Lz2Speed => GraphicsCompression::Lz2,
            Self::Lz3 => GraphicsCompression::Lz3,
        }
    }
}

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("Migrate Graphics Compression")
        .to_owned()
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
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

fn target_name(catalog: Option<&LocalizationCatalog>, target: GraphicsMigrationTarget) -> String {
    match target {
        GraphicsMigrationTarget::Lz2Original => {
            dialog_control_text(catalog, 0x294, "LC_LZ2 — original game code")
        }
        GraphicsMigrationTarget::Lz2Speed => {
            dialog_control_text(catalog, 0x295, "LC_LZ2 — optimized for speed")
        }
        GraphicsMigrationTarget::Lz3 => {
            dialog_control_text(catalog, 0x296, "LC_LZ3 — better compression")
        }
    }
}

fn current_installed_mode(app: &AppState) -> Option<lm_profile::SmwUsV1GraphicsCompressionMode> {
    let image = RomImage::from_bytes(app.controller_snapshot().ok()?.rom_bytes).ok()?;
    lm_profile::detect_smw_us_v1_graphics_compression_mode(&image).ok()
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
    fn complete_graphics_migration_surface_has_no_literal_native_widget_text() {
        let source = include_str!("graphics_migration_dialog.rs");
        for literal in ["egui::Window::new(\"", "ui.button(\"", "ui.label(\""] {
            assert!(
                !source.contains(literal),
                "literal migration widget text: {literal}"
            );
        }
        assert!(source.contains("original_dialog_control_text"));
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
            target_name(Some(&catalog), GraphicsMigrationTarget::Lz2Original),
            "LC_LZ2 — code original"
        );
        assert_eq!(
            target_name(Some(&catalog), GraphicsMigrationTarget::Lz3),
            "LC_LZ3 — meilleure compression"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 2, UiTextKey::CommonCancel.english()),
            UiTextKey::CommonCancel.english()
        );
    }

    #[test]
    fn optimized_lz2_target_routes_to_the_authenticated_runtime_command() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        assert!(matches!(
            build_command(
                &app,
                GraphicsMigrationTarget::Lz2Speed,
                "not consulted",
                "not consulted",
            )
            .unwrap(),
            Command::InstallLz2SpeedRuntime { rev: 0 }
        ));
        assert_eq!(
            target_name(None, GraphicsMigrationTarget::Lz2Speed),
            "LC_LZ2 — optimized for speed"
        );
    }
}
