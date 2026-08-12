use crate::level_editor_forms;
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, RomExpansionCommand};
use lm_project::{SA1_6_MIB_LEN, SA1_8_MIB_LEN};
use lm_rom::{Mapper, RomImage};

const LUNAR_MAGIC_LOROM_TARGETS: [usize; 3] = [0x20_0000, 0x30_0000, 0x40_0000];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RomExpansionPreset {
    LoRom2MiB,
    LoRom3MiB,
    LoRom4MiB,
    ExLoRom8MiB,
    Sa1_6MiB,
    Sa1_8MiB,
}

#[derive(Default)]
pub(crate) struct RomExpansionDialog {
    open: bool,
    current_logical_len: usize,
    target: String,
    fill: String,
    source_mapper: Option<Mapper>,
    confirm_exlorom: bool,
    confirm_sa1_target: Option<usize>,
    error: Option<String>,
}

impl RomExpansionDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        match app.controller_snapshot() {
            Ok(snapshot) => match RomImage::from_bytes(snapshot.rom_bytes) {
                Ok(image) => {
                    self.current_logical_len = image.logical_len();
                    self.source_mapper = Some(snapshot.identity.mapper);
                    self.target = format!("{:X}", suggested_target(image.logical_len()));
                    // ExpandRomBackingStore at $004A7390 grows the original backing store through
                    // ZeroRomRange. Keep the advanced field visible, but default the installed
                    // workflow to Lunar Magic's exact fill rather than allocator-friendly $FF.
                    self.fill = "00".into();
                    self.open = true;
                    self.confirm_exlorom = false;
                    self.confirm_sa1_target = None;
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn open_preset(&mut self, app: &AppState, preset: RomExpansionPreset) {
        self.open(app);
        if !self.open {
            return;
        }
        match preset {
            RomExpansionPreset::LoRom2MiB => self.select_lorom_target(0x20_0000),
            RomExpansionPreset::LoRom3MiB => self.select_lorom_target(0x30_0000),
            RomExpansionPreset::LoRom4MiB => self.select_lorom_target(0x40_0000),
            RomExpansionPreset::ExLoRom8MiB => {
                if exlorom_eligible(self.source_mapper, self.current_logical_len) {
                    self.confirm_exlorom = true;
                } else {
                    self.error =
                        Some("64-Mbit ExLoROM conversion requires a 512 KiB–4 MiB LoROM".into());
                }
            }
            RomExpansionPreset::Sa1_6MiB => self.select_sa1_target(SA1_6_MIB_LEN),
            RomExpansionPreset::Sa1_8MiB => self.select_sa1_target(SA1_8_MIB_LEN),
        }
    }

    fn select_lorom_target(&mut self, target: usize) {
        if ordinary_expansion_eligible(self.source_mapper, self.current_logical_len, target) {
            self.target = format!("{target:X}");
            self.fill = "00".into();
        } else {
            self.error = Some(format!(
                "The {} MiB ordinary expansion target is not available for this ROM",
                target / 0x10_0000
            ));
        }
    }

    fn select_sa1_target(&mut self, target: usize) {
        if self.source_mapper == Some(Mapper::Sa1) && target > self.current_logical_len {
            self.confirm_sa1_target = Some(target);
        } else {
            self.error = Some(format!(
                "The {} MiB expansion target requires a smaller SA-1 ROM",
                target / 0x10_0000
            ));
        }
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let mut command = None;
        if self.open {
            egui::Window::new(text(catalog, ExtendedUiTextKey::RomExpansionTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(text(catalog, ExtendedUiTextKey::RomExpansionTargetNotice));
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::RomExpansionAlignmentNotice,
                    ));
                    ui.horizontal(|ui| {
                        ui.label(text(catalog, ExtendedUiTextKey::RomExpansionLmTarget));
                        for (target, label) in LUNAR_MAGIC_LOROM_TARGETS.into_iter().zip([
                            ExtendedUiTextKey::RomExpansion2MiB,
                            ExtendedUiTextKey::RomExpansion3MiB,
                            ExtendedUiTextKey::RomExpansion4MiB,
                        ]) {
                            if ui
                                .add_enabled(
                                    ordinary_expansion_eligible(
                                        self.source_mapper,
                                        self.current_logical_len,
                                        target,
                                    ),
                                    egui::Button::new(text(catalog, label)),
                                )
                                .clicked()
                            {
                                self.target = format!("{target:X}");
                                self.fill = "00".into();
                            }
                        }
                    });
                    ui.separator();
                    ui.heading(text(catalog, ExtendedUiTextKey::RomExpansionExLoRomHeading));
                    ui.label(text(catalog, ExtendedUiTextKey::RomExpansionExLoRomNotice));
                    let exlorom_eligible =
                        exlorom_eligible(self.source_mapper, self.current_logical_len);
                    if ui
                        .add_enabled(
                            exlorom_eligible,
                            egui::Button::new(text(
                                catalog,
                                ExtendedUiTextKey::RomExpansionExLoRomConvert,
                            )),
                        )
                        .clicked()
                    {
                        self.confirm_exlorom = true;
                    }
                    if !exlorom_eligible {
                        ui.weak(text(
                            catalog,
                            ExtendedUiTextKey::RomExpansionExLoRomRequires,
                        ));
                    }
                    ui.separator();
                    ui.heading(text(catalog, ExtendedUiTextKey::RomExpansionSa1Heading));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(text(catalog, ExtendedUiTextKey::RomExpansionLmTarget));
                        for (target, label) in [
                            (SA1_6_MIB_LEN, ExtendedUiTextKey::RomExpansion6MiB),
                            (SA1_8_MIB_LEN, ExtendedUiTextKey::RomExpansion8MiB),
                        ] {
                            if ui
                                .add_enabled(
                                    self.source_mapper == Some(Mapper::Sa1)
                                        && target > self.current_logical_len,
                                    egui::Button::new(text(catalog, label)),
                                )
                                .clicked()
                            {
                                self.confirm_sa1_target = Some(target);
                            }
                        }
                    });
                    if self.source_mapper != Some(Mapper::Sa1) {
                        ui.weak(text(catalog, ExtendedUiTextKey::RomExpansionSa1Requires));
                    }
                    let ordinary_route = self.source_mapper != Some(Mapper::Sa1);
                    ui.add_enabled_ui(ordinary_route, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(text(catalog, ExtendedUiTextKey::RomExpansionTarget));
                            ui.text_edit_singleline(&mut self.target);
                        });
                        ui.horizontal(|ui| {
                            ui.label(text(catalog, ExtendedUiTextKey::RomExpansionFillByte));
                            ui.text_edit_singleline(&mut self.fill);
                        });
                    });
                    if !ordinary_route {
                        ui.weak(text(catalog, ExtendedUiTextKey::RomExpansionSa1FixedNotice));
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::RomExpansionCancel))
                            .clicked()
                        {
                            self.open = false;
                        }
                        if ui
                            .add_enabled(
                                ordinary_route,
                                egui::Button::new(text(
                                    catalog,
                                    ExtendedUiTextKey::RomExpansionApply,
                                )),
                            )
                            .clicked()
                        {
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
        if self.confirm_exlorom {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::RomExpansionExLoRomWarningTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    text(catalog, ExtendedUiTextKey::RomExpansionMapperWarning),
                );
                ui.label(text(
                    catalog,
                    ExtendedUiTextKey::RomExpansionCompatibilityWarning,
                ));
                ui.label(text(catalog, ExtendedUiTextKey::RomExpansionUndoableNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpansionCancel))
                        .clicked()
                    {
                        self.confirm_exlorom = false;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpansionConvertRom))
                        .clicked()
                    {
                        command = Some(Command::ConvertRomTo64MbitExLoRom {
                            expected_revision: app.project_revision(),
                        });
                    }
                });
            });
        }
        if let Some(target) = self.confirm_sa1_target {
            let mib = target / 0x10_0000;
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::RomExpansionSa1ConfirmTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    text(catalog, ExtendedUiTextKey::RomExpansionSa1ConfirmNotice)
                        .replace("{mib}", &mib.to_string()),
                );
                if target == SA1_6_MIB_LEN {
                    ui.label(text(catalog, ExtendedUiTextKey::RomExpansionSnes9xNotice));
                } else {
                    ui.label(text(catalog, ExtendedUiTextKey::RomExpansionZsnesNotice));
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpansionCancel))
                        .clicked()
                    {
                        self.confirm_sa1_target = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpansionExpandRom))
                        .clicked()
                    {
                        command = Some(Command::ExpandSa1Rom {
                            expected_revision: app.project_revision(),
                            target_logical_len: target,
                        });
                    }
                });
            });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::RomExpansionErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpansionOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
        }
        command
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.open = false;
        self.confirm_exlorom = false;
        self.confirm_sa1_target = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn build_command(app: &AppState, target: &str, fill: &str) -> Result<Command, String> {
    let snapshot = app
        .controller_snapshot()
        .map_err(|error| error.to_string())?;
    if snapshot.identity.mapper == Mapper::Sa1 {
        return Err("SA-1 ROMs must use the fixed 6 MiB or 8 MiB expansion action".to_owned());
    }
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

fn exlorom_eligible(mapper: Option<Mapper>, logical_len: usize) -> bool {
    mapper == Some(Mapper::LoRom) && (0x80_000..=0x40_0000).contains(&logical_len)
}

fn ordinary_expansion_eligible(
    mapper: Option<Mapper>,
    current_logical_len: usize,
    target_logical_len: usize,
) -> bool {
    mapper != Some(Mapper::Sa1) && target_logical_len > current_logical_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_expansion_surface_uses_every_typed_key() {
        let source = include_str!("rom_expansion_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("RomExpansion"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for bypass in [
            "egui::Window::new(\"Expand ROM\")",
            "ui.heading(\"64-Mbit ExLoROM\")",
            "egui::Button::new(\"Convert to 64-Mbit ExLoROM…\")",
            "egui::Window::new(\"Expand SA-1 ROM?\")",
            "egui::Window::new(\"ROM expansion error\")",
        ] {
            assert!(!source.contains(bypass));
        }
    }

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
        assert!(exlorom_eligible(
            dialog.source_mapper,
            dialog.current_logical_len
        ));
    }

    #[test]
    fn exlorom_action_rejects_wrong_mapper_and_out_of_range_sources() {
        assert!(exlorom_eligible(Some(Mapper::LoRom), 0x80_000));
        assert!(exlorom_eligible(Some(Mapper::LoRom), 0x40_0000));
        assert!(!exlorom_eligible(Some(Mapper::LoRom), 0x40_8000));
        assert!(!exlorom_eligible(Some(Mapper::ExLoRom), 0x40_0000));
        assert!(!exlorom_eligible(None, 0x80_000));
    }

    #[test]
    fn ordinary_actions_cannot_bypass_fixed_sa1_expansion() {
        assert!(ordinary_expansion_eligible(
            Some(Mapper::LoRom),
            0x80_000,
            0x20_0000
        ));
        assert!(!ordinary_expansion_eligible(
            Some(Mapper::Sa1),
            0x80_000,
            0x20_0000
        ));
        assert!(!ordinary_expansion_eligible(
            Some(Mapper::LoRom),
            0x20_0000,
            0x20_0000
        ));
    }

    #[test]
    fn authenticated_lorom_presets_select_exact_zero_filled_targets() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        for (preset, target) in [
            (RomExpansionPreset::LoRom2MiB, "200000"),
            (RomExpansionPreset::LoRom3MiB, "300000"),
            (RomExpansionPreset::LoRom4MiB, "400000"),
        ] {
            let mut dialog = RomExpansionDialog::default();
            dialog.open_preset(&app, preset);
            assert!(dialog.open);
            assert_eq!(dialog.target, target);
            assert_eq!(dialog.fill, "00");
            assert_eq!(dialog.error, None);
        }
    }

    #[test]
    fn authenticated_exlorom_preset_enters_the_warning_confirmation() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut dialog = RomExpansionDialog::default();
        dialog.open_preset(&app, RomExpansionPreset::ExLoRom8MiB);
        assert!(dialog.open);
        assert!(dialog.confirm_exlorom);
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn authenticated_sa1_presets_are_mapper_gated_and_enter_confirmation() {
        for target in [SA1_6_MIB_LEN, SA1_8_MIB_LEN] {
            let mut eligible = RomExpansionDialog {
                open: true,
                source_mapper: Some(Mapper::Sa1),
                current_logical_len: 0x40_0000,
                ..RomExpansionDialog::default()
            };
            eligible.select_sa1_target(target);
            assert_eq!(eligible.confirm_sa1_target, Some(target));
            assert_eq!(eligible.error, None);

            let mut wrong_mapper = RomExpansionDialog {
                open: true,
                source_mapper: Some(Mapper::LoRom),
                current_logical_len: 0x40_0000,
                ..RomExpansionDialog::default()
            };
            wrong_mapper.select_sa1_target(target);
            assert_eq!(wrong_mapper.confirm_sa1_target, None);
            assert!(wrong_mapper.error.is_some());
        }
    }
}
