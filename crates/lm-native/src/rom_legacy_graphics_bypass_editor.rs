use eframe::egui;
use lm_app::{AppState, Command, LegacyGraphicsBypassWorkspace};
use lm_level::{LegacyGraphicsAssignment, LegacyGraphicsBypassSelectors};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyGraphicsBypassDomain {
    ForegroundBackground,
    Sprites,
}

impl LegacyGraphicsBypassDomain {
    fn title(self) -> &'static str {
        match self {
            Self::ForegroundBackground => "Standard FG/BG GFX Bypass",
            Self::Sprites => "Standard Sprite GFX Bypass",
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
            error: None,
            pending_close: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
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
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(self.domain.title())
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let stale = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.revision() != project_revision);
        ui.label("Recovered Lunar Magic standard-GFX list: 255 selectable rows.");
        ui.checkbox(&mut self.enabled, "Enable bypass for this level");

        let previous_row = self.row;
        ui.horizontal(|ui| {
            ui.label("List row");
            ui.add(
                egui::DragValue::new(&mut self.row)
                    .hexadecimal(2, false, true)
                    .range(0..=0xfe),
            );
        });
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
        ui.small("A zero-filled selected row falls back to the level's normal tileset assignment.");
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this editor opened. Close and reopen before committing.",
            );
        }

        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Stage row and level selection"))
                .clicked()
            {
                self.stage();
            }
            let modified = self
                .workspace
                .as_ref()
                .is_some_and(LegacyGraphicsBypassWorkspace::is_modified);
            if ui
                .add_enabled(modified && !stale, egui::Button::new("Commit to ROM"))
                .clicked()
            {
                command = self.prepare_commit();
            }
            ui.label(if modified { "Staged" } else { "Unchanged" });
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
        match workspace.prepare_commit(format!("Edit {}", self.domain.title())) {
            Ok(prepared) => Some(prepared.into_command()),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged GFX bypass changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("These list or level-selection changes have not been committed.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Standard GFX bypass error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::Command;

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
