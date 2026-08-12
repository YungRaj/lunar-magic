use eframe::egui;
use lm_app::{
    AppState, Command, ExtendedUiTextKey, LegacyGraphicsBypassWorkspace, LocalizationCatalog,
};
use lm_level::{LegacyGraphicsAssignment, LegacyGraphicsBypassSelectors};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyGraphicsBypassDomain {
    ForegroundBackground,
    Sprites,
}

impl LegacyGraphicsBypassDomain {
    fn technical_title(self) -> &'static str {
        match self {
            Self::ForegroundBackground => "Standard FG/BG GFX Bypass",
            Self::Sprites => "Standard Sprite GFX Bypass",
        }
    }

    fn title_key(self) -> ExtendedUiTextKey {
        match self {
            Self::ForegroundBackground => ExtendedUiTextKey::LegacyBypassFgBgTitle,
            Self::Sprites => ExtendedUiTextKey::LegacyBypassSpriteTitle,
        }
    }

    fn labels(self) -> [&'static str; 4] {
        match self {
            Self::ForegroundBackground => ["FG1", "FG2", "BG1", "FG3"],
            Self::Sprites => ["SP1", "SP2", "SP3", "SP4"],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

pub(crate) struct RomLegacyGraphicsBypassEditor {
    domain: LegacyGraphicsBypassDomain,
    workspace: Option<LegacyGraphicsBypassWorkspace>,
    enabled: bool,
    row: u8,
    files: [u8; 4],
    use_list_dialog: bool,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl Default for RomLegacyGraphicsBypassEditor {
    fn default() -> Self {
        Self::new(LegacyGraphicsBypassDomain::ForegroundBackground)
    }
}

impl RomLegacyGraphicsBypassEditor {
    pub(crate) fn new(domain: LegacyGraphicsBypassDomain) -> Self {
        Self {
            domain,
            workspace: None,
            enabled: false,
            row: 0,
            files: [0; 4],
            use_list_dialog: true,
            error: None,
            pending_close: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        workspace.is_modified().then(|| {
            let selectors = workspace.selectors();
            let content_revision = workspace
                .table()
                .encode()
                .iter()
                .fold(0x4c45_4741_4359_4746_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(*byte)
                })
                ^ u64::from(selectors.foreground_background.unwrap_or(0xff)).rotate_left(17)
                ^ u64::from(selectors.sprites.unwrap_or(0xff)).rotate_left(41);
            app.project_revision().wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ workspace.revision().rotate_left(27)
                ^ content_revision
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("legacy graphics-bypass workspace is closed")?;
        if !workspace.is_modified() {
            return Ok(app.recovery_snapshot());
        }
        let prepared = workspace
            .prepare_commit(format!("Recover staged {}", self.domain.technical_title()))
            .map_err(|error| error.to_string())?;
        if prepared.expected_revision != app.project_revision() {
            return Err(
                "legacy graphics-bypass recovery was prepared from a stale revision".into(),
            );
        }
        app.recovery_snapshot_with_mutation(&prepared.mutation, Some(workspace.level()))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn staged_recovery_snapshot_merged_with(
        &self,
        other: &Self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let left = self
            .workspace
            .as_ref()
            .ok_or("legacy graphics-bypass workspace is closed")?;
        let right = other
            .workspace
            .as_ref()
            .ok_or("legacy graphics-bypass workspace is closed")?;
        let merged = left
            .merge_staged(right)
            .map_err(|error| error.to_string())?;
        let prepared = merged
            .prepare_commit("Recover staged standard FG/BG and sprite GFX bypass")
            .map_err(|error| error.to_string())?;
        if prepared.expected_revision != app.project_revision() {
            return Err(
                "legacy graphics-bypass recovery was prepared from a stale revision".into(),
            );
        }
        app.recovery_snapshot_with_mutation(&prepared.mutation, Some(merged.level()))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn set_use_list_dialog(&mut self, enabled: bool) {
        self.use_list_dialog = enabled;
    }

    pub(crate) fn open(&mut self, app: &AppState) -> Result<(), String> {
        if self.is_open() {
            return Ok(());
        }
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let workspace =
            LegacyGraphicsBypassWorkspace::load(&snapshot).map_err(|error| error.to_string())?;
        let selected = match self.domain {
            LegacyGraphicsBypassDomain::ForegroundBackground => {
                workspace.selectors().foreground_background
            }
            LegacyGraphicsBypassDomain::Sprites => workspace.selectors().sprites,
        };
        self.enabled = selected.is_some();
        self.row = selected.unwrap_or(0);
        self.files = workspace
            .table()
            .entry(usize::from(self.row))
            .map_err(|error| error.to_string())?
            .0;
        self.workspace = Some(workspace);
        self.error = None;
        Ok(())
    }

    pub(crate) fn open_domain(
        &mut self,
        app: &AppState,
        domain: LegacyGraphicsBypassDomain,
    ) -> Result<(), String> {
        if self.is_open() && self.domain != domain {
            return Err(
                "close the staged standard-GFX bypass editor before switching domains".into(),
            );
        }
        self.domain = domain;
        self.open(app)
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(text(catalog, self.domain.title_key()))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    command = self.contents(ui, project_revision, catalog)
                });
        }
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let stale = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.revision() != project_revision);
        ui.label(text(catalog, ExtendedUiTextKey::LegacyBypassDescription));
        ui.checkbox(
            &mut self.enabled,
            text(catalog, ExtendedUiTextKey::LegacyBypassEnable),
        );

        let previous_row = self.row;
        if self.use_list_dialog {
            egui::ComboBox::from_label(text(catalog, ExtendedUiTextKey::LegacyBypassListRow))
                .selected_text(self.row_label(self.row))
                .show_ui(ui, |ui| {
                    for row in 0_u8..=0xfe {
                        let label = self.row_label(row);
                        ui.selectable_value(&mut self.row, row, label);
                    }
                });
        } else {
            ui.horizontal(|ui| {
                ui.label(text(catalog, ExtendedUiTextKey::LegacyBypassRegularRow));
                ui.add(
                    egui::DragValue::new(&mut self.row)
                        .hexadecimal(2, false, true)
                        .range(0..=0xfe),
                );
            });
            ui.small(text(catalog, ExtendedUiTextKey::LegacyBypassRegularNotice));
        }
        if self.row != previous_row {
            self.load_row();
        }

        egui::Grid::new(match self.domain {
            LegacyGraphicsBypassDomain::ForegroundBackground => "legacy-fg-bg-gfx-files",
            LegacyGraphicsBypassDomain::Sprites => "legacy-sprite-gfx-files",
        })
        .num_columns(2)
        .show(ui, |ui| {
            for (slot, label) in self.domain.labels().into_iter().enumerate() {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut self.files[slot])
                        .hexadecimal(2, false, true)
                        .range(0..=0xff),
                );
                ui.end_row();
            }
        });
        ui.small(text(catalog, ExtendedUiTextKey::LegacyBypassZeroFallback));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::LegacyBypassStaleNotice),
            );
        }

        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::LegacyBypassStage)),
                )
                .clicked()
            {
                self.stage();
            }
            let modified = self
                .workspace
                .as_ref()
                .is_some_and(LegacyGraphicsBypassWorkspace::is_modified);
            if ui
                .add_enabled(
                    modified && !stale,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::LegacyBypassCommit)),
                )
                .clicked()
            {
                command = self.prepare_commit();
            }
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::LegacyBypassStaged
                } else {
                    ExtendedUiTextKey::LegacyBypassUnchanged
                },
            ));
        });
        command
    }

    fn load_row(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        match workspace.table().entry(usize::from(self.row)) {
            Ok(assignment) => self.files = assignment.0,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn row_label(&self, row: u8) -> String {
        let assignment = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.table().entry(usize::from(row)).ok())
            .map_or([0; 4], |assignment| assignment.0);
        format!(
            "{row:02X}: {:02X}, {:02X}, {:02X}, {:02X}",
            assignment[0], assignment[1], assignment[2], assignment[3]
        )
    }

    fn stage(&mut self) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if let Err(error) = workspace
            .table_mut()
            .set_entry(usize::from(self.row), LegacyGraphicsAssignment(self.files))
        {
            self.error = Some(error.to_string());
            return;
        }
        let mut selectors = workspace.selectors();
        let selected = self.enabled.then_some(self.row);
        match self.domain {
            LegacyGraphicsBypassDomain::ForegroundBackground => {
                selectors.foreground_background = selected;
            }
            LegacyGraphicsBypassDomain::Sprites => selectors.sprites = selected,
        }
        workspace.set_selectors(LegacyGraphicsBypassSelectors { ..selectors });
    }

    fn prepare_commit(&mut self) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        match workspace.prepare_commit(format!("Edit {}", self.domain.technical_title())) {
            Ok(prepared) => Some(prepared.into_command()),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::LegacyBypassDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::LegacyBypassUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::LegacyBypassCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::LegacyBypassDiscard))
                        .clicked()
                    {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::LegacyBypassErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::LegacyBypassOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::Command;

    #[test]
    fn complete_legacy_bypass_form_uses_every_typed_key() {
        let source = include_str!("rom_legacy_graphics_bypass_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LegacyBypass"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for bypass in [
            "ui.checkbox(&mut self.enabled, \"Enable bypass for this level\")",
            "Button::new(\"Stage row and level selection\")",
            "Window::new(\"Discard staged GFX bypass changes?\")",
        ] {
            assert!(!source.contains(bypass));
        }
    }

    #[test]
    fn staged_legacy_graphics_bypass_is_recovered_without_committing_live_project() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        installer
            .dispatch(Command::InstallSettings { rev: 0 })
            .unwrap();
        let installed = installer.project().unwrap().save_snapshot();

        let mut app = AppState::default();
        app.load_rom(installed).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let mut editor =
            RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::ForegroundBackground);
        editor.open(&app).unwrap();
        editor.enabled = true;
        editor.row = 9;
        editor.files = [0x11, 0x22, 0x33, 0x44];
        editor.stage();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let live =
            LegacyGraphicsBypassWorkspace::load(&app.controller_snapshot().unwrap()).unwrap();
        assert_eq!(live.selectors().foreground_background, None);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let recovered =
            LegacyGraphicsBypassWorkspace::load(&reopened.controller_snapshot().unwrap()).unwrap();
        assert_eq!(recovered.selectors().foreground_background, Some(9));
        assert_eq!(
            recovered.table().entry(9).unwrap().0,
            [0x11, 0x22, 0x33, 0x44]
        );
    }

    #[test]
    fn historical_dialog_style_switch_preserves_the_complete_row_model() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();

        let mut editor =
            RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::ForegroundBackground);
        assert!(editor.use_list_dialog);
        editor.open(&app).unwrap();
        assert!(editor.row_label(0).starts_with("00:"));
        assert!(editor.row_label(0xfe).starts_with("FE:"));
        editor.set_use_list_dialog(false);
        assert!(!editor.use_list_dialog);
        assert_eq!(
            editor
                .workspace
                .as_ref()
                .unwrap()
                .table()
                .entry(0)
                .unwrap()
                .0,
            editor.files
        );
    }

    #[test]
    fn simultaneous_dialog_recovery_reopens_both_selectors_and_rows_without_live_mutation() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        installer
            .dispatch(Command::InstallSettings { rev: 0 })
            .unwrap();
        let installed = installer.project().unwrap().save_snapshot();
        let mut app = AppState::default();
        app.load_rom(installed).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let live_bytes = app.project().unwrap().save_snapshot();
        let live_undo = app.project().unwrap().history.undo_len();

        let mut foreground =
            RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::ForegroundBackground);
        let mut sprites = RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::Sprites);
        foreground.open(&app).unwrap();
        sprites.open(&app).unwrap();
        foreground.enabled = true;
        foreground.row = 5;
        foreground.files = [1, 2, 4, 3];
        foreground.stage();
        sprites.enabled = true;
        sprites.row = 7;
        sprites.files = [0x12, 0x13, 0x14, 0x15];
        sprites.stage();

        let recovery = foreground
            .staged_recovery_snapshot_merged_with(&sprites, &app)
            .unwrap()
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), live_bytes);
        assert_eq!(app.project().unwrap().history.undo_len(), live_undo);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let recovered =
            LegacyGraphicsBypassWorkspace::load(&reopened.controller_snapshot().unwrap()).unwrap();
        assert_eq!(recovered.selectors().foreground_background, Some(5));
        assert_eq!(recovered.selectors().sprites, Some(7));
        assert_eq!(recovered.table().entry(5).unwrap().0, [1, 2, 4, 3]);
        assert_eq!(
            recovered.table().entry(7).unwrap().0,
            [0x12, 0x13, 0x14, 0x15]
        );
        let image =
            lm_rom::RomImage::from_bytes(reopened.project().unwrap().save_snapshot()).unwrap();
        let checksum_field = reopened
            .controller_snapshot()
            .unwrap()
            .identity
            .internal_header_offset
            + 0x1c;
        assert_eq!(
            lm_rom::SnesChecksum::decode(image.logical_bytes(), checksum_field).unwrap(),
            lm_rom::compute_snes_checksum(image.logical_bytes(), checksum_field).unwrap()
        );
    }

    #[test]
    fn both_original_dialog_domains_stage_independently_and_commit_one_undo() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::InstallSettings { rev: 0 }).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let baseline_undo = app.project().unwrap().history.undo_len();

        let mut foreground =
            RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::ForegroundBackground);
        foreground.open(&app).unwrap();
        foreground.enabled = true;
        foreground.row = 5;
        foreground.files = [1, 2, 4, 3];
        foreground.stage();
        app.dispatch(foreground.prepare_commit().unwrap()).unwrap();

        let reopened =
            LegacyGraphicsBypassWorkspace::load(&app.controller_snapshot().unwrap()).unwrap();
        assert_eq!(reopened.selectors().foreground_background, Some(5));
        assert_eq!(reopened.table().entry(5).unwrap().0, [1, 2, 4, 3]);
        assert_eq!(app.project().unwrap().history.undo_len(), baseline_undo + 1);

        let mut sprites = RomLegacyGraphicsBypassEditor::new(LegacyGraphicsBypassDomain::Sprites);
        sprites.open(&app).unwrap();
        assert_eq!(sprites.row, 0);
        sprites.enabled = true;
        sprites.row = 7;
        sprites.files = [0x12, 0x13, 0x14, 0x15];
        sprites.stage();
        app.dispatch(sprites.prepare_commit().unwrap()).unwrap();
        let reopened =
            LegacyGraphicsBypassWorkspace::load(&app.controller_snapshot().unwrap()).unwrap();
        assert_eq!(reopened.selectors().foreground_background, Some(5));
        assert_eq!(reopened.selectors().sprites, Some(7));
        assert_eq!(
            reopened.table().entry(7).unwrap().0,
            [0x12, 0x13, 0x14, 0x15]
        );
    }
}
