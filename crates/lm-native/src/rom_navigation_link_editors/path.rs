use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey as Key, LocalizationCatalog};
use lm_overworld::{
    OverworldEndpoint, OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget,
};
use lm_profile::smw_us_v1_overworld_path_patch_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: OverworldPathLinkTable,
    current: OverworldPathLinkTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldPathLinkEditor {
    workspace: Option<Workspace>,
    form: PathLinkForm,
    count: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

#[derive(Default)]
struct PathLinkForm {
    index: String,
    source_x: String,
    source_y: String,
    source_submap: String,
    destination_x: String,
    destination_y: String,
    destination_submap: String,
    target_x: String,
    target_y: String,
    loaded: Option<usize>,
}

impl RomOverworldPathLinkEditor {
    pub(crate) fn staged_recovery_table<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a OverworldPathLinkTable>, String> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(None);
        };
        if workspace.revision != app.project_revision() {
            return Err("stale path-link workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace.current.links.iter().fold(
            0x4f57_5041_5448_4c4e_u64 ^ workspace.current.links.len() as u64,
            |revision, link| {
                revision.rotate_left(7)
                    ^ u64::from(link.source.x)
                    ^ u64::from(link.source.y).rotate_left(11)
                    ^ u64::from(link.source.submap).rotate_left(23)
                    ^ u64::from(link.destination.x).rotate_left(31)
                    ^ u64::from(link.destination.y).rotate_left(43)
                    ^ u64::from(link.destination.submap).rotate_left(53)
                    ^ u64::from(link.target.x_tile).rotate_left(3)
                    ^ u64::from(link.target.y_tile).rotate_left(17)
            },
        );
        Some(
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ workspace.revision.rotate_left(31)
                ^ content_revision,
        )
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale path-link workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        app.recovery_snapshot_with_overworld_path_links(
            None,
            &workspace.current,
            app.current_level(),
        )
        .map_err(|error| error.to_string())
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let loaded = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.count = format!("{:02X}", loaded.table.links.len());
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.table.clone(),
                    current: loaded.table,
                });
                self.form.index = "00".into();
                self.load_selected().ok();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if workspace.current == workspace.original {
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
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(text(catalog, Key::NavigationPathTitle))
                .default_size([600.0, 470.0])
                .show(context, |ui| command = self.contents(ui, revision, catalog));
        }
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != revision;
        let dirty = workspace.current != workspace.original;
        ui.label(text(catalog, Key::NavigationPathNotice));
        ui.label(
            text(catalog, Key::NavigationPathCountFormat)
                .replace("{count}", &workspace.current.links.len().to_string()),
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, Key::NavigationStaleNotice),
            );
        }
        self.form_ui(ui, catalog);
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::NavigationPathTableCount));
            ui.text_edit_singleline(&mut self.count);
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(text(catalog, Key::NavigationResizeTable)),
                )
                .clicked()
                && let Err(error) = self.resize()
            {
                self.error = Some(error);
            }
        });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button(text(catalog, Key::NavigationLoadLink)).clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(text(catalog, Key::NavigationApplyLink)),
                )
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    dirty && !stale,
                    egui::Button::new(text(catalog, Key::NavigationCommitLinks)),
                )
                .clicked()
            {
                match self.prepare_commit(revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(text(
                catalog,
                if dirty {
                    Key::NavigationStaged
                } else {
                    Key::NavigationUnchanged
                },
            ));
        });
        command
    }

    fn form_ui(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        egui::Grid::new("rom-overworld-path-link-form")
            .striped(true)
            .show(ui, |ui| {
                form_row(
                    ui,
                    &text(catalog, Key::NavigationIndex),
                    &mut self.form.index,
                    &mut self.form.loaded,
                    true,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationSourceX),
                    &mut self.form.source_x,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationSourceY),
                    &mut self.form.source_y,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationSourceSubmap),
                    &mut self.form.source_submap,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationDestinationX),
                    &mut self.form.destination_x,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationDestinationY),
                    &mut self.form.destination_y,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationDestinationSubmap),
                    &mut self.form.destination_submap,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationTargetXTile),
                    &mut self.form.target_x,
                    &mut None::<usize>,
                    false,
                );
                form_row(
                    ui,
                    &text(catalog, Key::NavigationTargetYTile),
                    &mut self.form.target_y,
                    &mut None::<usize>,
                    false,
                );
            });
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u8(&self.form.index, "link index")?);
        let len = self
            .workspace
            .as_ref()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?
            .current
            .links
            .len();
        if index >= len {
            return Err(format!("link index must be below {len:02X}"));
        }
        Ok(index)
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        let link = self.workspace.as_ref().unwrap().current.links[index];
        self.form.set(index, link);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.form.loaded != Some(index) {
            return Err("load the selected link before applying it".into());
        }
        let link = self.form.parse()?;
        let workspace = self.workspace.as_mut().unwrap();
        let mut staged = workspace.current.clone();
        staged.links[index] = link;
        staged.encode_planes().map_err(|error| error.to_string())?;
        workspace.current = staged;
        Ok(())
    }

    fn resize(&mut self) -> Result<(), String> {
        let count = usize::from(parse_hex_u8(&self.count, "path-link count")?);
        if count > OverworldPathLinkTable::MAX_LINKS {
            return Err("path-link count must be at most 80".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?;
        workspace.current.links.resize(count, blank_path_link());
        workspace
            .current
            .encode_planes()
            .map_err(|error| error.to_string())?;
        self.form.loaded = None;
        Ok(())
    }

    fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?;
        if workspace.revision != revision {
            return Err("stale path-link workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        workspace
            .current
            .encode_planes()
            .map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldPathLinks {
            rev: workspace.revision,
            table: Box::new(workspace.current.clone()),
        }))
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
        egui::Window::new(text(catalog, Key::NavigationPathDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(text(catalog, Key::NavigationPathDiscardNotice));
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::NavigationCancel)).clicked() {
                        self.pending_close = None;
                    }
                    if ui.button(text(catalog, Key::NavigationDiscard)).clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::NavigationPathErrorTitle)).show(context, |ui| {
                ui.label(error);
                if ui.button(text(catalog, Key::NavigationOk)).clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.form.loaded = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

impl PathLinkForm {
    fn set(&mut self, index: usize, link: OverworldPathLink) {
        self.source_x = format!("{:04X}", link.source.x);
        self.source_y = format!("{:04X}", link.source.y);
        self.source_submap = format!("{:02X}", link.source.submap);
        self.destination_x = format!("{:04X}", link.destination.x);
        self.destination_y = format!("{:04X}", link.destination.y);
        self.destination_submap = format!("{:02X}", link.destination.submap);
        self.target_x = format!("{:02X}", link.target.x_tile);
        self.target_y = format!("{:02X}", link.target.y_tile);
        self.loaded = Some(index);
    }

    fn parse(&self) -> Result<OverworldPathLink, String> {
        Ok(OverworldPathLink {
            source: OverworldEndpoint {
                x: parse_hex_u16(&self.source_x, "source X")?,
                y: parse_hex_u16(&self.source_y, "source Y")?,
                submap: parse_hex_u8(&self.source_submap, "source submap")?,
            },
            destination: OverworldEndpoint {
                x: parse_hex_u16(&self.destination_x, "destination X")?,
                y: parse_hex_u16(&self.destination_y, "destination Y")?,
                submap: parse_hex_u8(&self.destination_submap, "destination submap")?,
            },
            target: OverworldPathTarget {
                x_tile: parse_hex_u8(&self.target_x, "target X")?,
                y_tile: parse_hex_u8(&self.target_y, "target Y")?,
            },
        })
    }
}

fn form_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    loaded: &mut Option<usize>,
    is_index: bool,
) {
    ui.label(label);
    if ui.text_edit_singleline(value).changed() && is_index {
        *loaded = None;
    }
    ui.end_row();
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

fn blank_path_link() -> OverworldPathLink {
    OverworldPathLink {
        source: OverworldEndpoint {
            x: 0xffff,
            y: 0xffff,
            submap: 0xff,
        },
        destination: OverworldEndpoint {
            x: 0xffff,
            y: 0xffff,
            submap: 0xff,
        },
        target: OverworldPathTarget {
            x_tile: 0xff,
            y_tile: 0xff,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn path_link_editor_has_no_literal_widget_text() {
        let source = include_str!("path.rs");
        for literal in [
            "ui.button(\"",
            "ui.label(\"",
            "Button::new(\"",
            "Window::new(\"",
        ] {
            assert!(
                !source.contains(literal),
                "path-link editor bypasses localization with {literal}"
            );
        }
    }

    #[test]
    fn pristine_table_grows_installs_and_reopens_exact_link() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldPathLinkEditor::default();
        editor.open(&app);
        assert_eq!(editor.workspace.as_ref().unwrap().current.links.len(), 14);
        editor.count = "0F".into();
        editor.resize().unwrap();
        editor.form.index = "0E".into();
        editor.load_selected().unwrap();
        editor.form.target_x = "2A".into();
        editor.apply_selected().unwrap();
        app.dispatch(
            editor
                .prepare_commit(app.project_revision())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap();
        assert_eq!(reopened.table.links.len(), 15);
        assert_eq!(reopened.table.links[14].target.x_tile, 0x2a);
    }

    #[test]
    fn selection_identity_and_stale_revision_are_enforced() {
        let table = OverworldPathLinkTable {
            links: vec![blank_path_link()],
        };
        let mut editor = RomOverworldPathLinkEditor {
            workspace: Some(Workspace {
                revision: 3,
                original: table.clone(),
                current: table,
            }),
            ..Default::default()
        };
        editor.form.index = "00".into();
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        assert!(editor.prepare_commit(4).is_err());
        editor.form.target_x = "7A".into();
        editor.apply_selected().unwrap();
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_pristine_path_growth_recovers_complete_installed_table() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomOverworldPathLinkEditor::default();
        editor.open(&app);
        editor.count = "0F".into();
        editor.resize().unwrap();
        editor.form.index = "0E".into();
        editor.load_selected().unwrap();
        editor.form.source_x = "1234".into();
        editor.form.destination_y = "5678".into();
        editor.form.target_x = "2A".into();
        editor.form.target_y = "3B".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        assert_eq!(table.links.len(), 15);
        assert_eq!(table.links[14].source.x, 0x1234);
        assert_eq!(table.links[14].destination.y, 0x5678);
        assert_eq!(table.links[14].target.x_tile, 0x2a);
        assert_eq!(table.links[14].target.y_tile, 0x3b);
    }

    #[test]
    fn staged_installed_path_update_preserves_prior_tail_link() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut first = RomOverworldPathLinkEditor::default();
        first.open(&installer);
        first.count = "0F".into();
        first.resize().unwrap();
        first.form.index = "0E".into();
        first.load_selected().unwrap();
        first.form.target_x = "44".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldPathLinkEditor::default();
        editor.open(&app);
        editor.form.index = "00".into();
        editor.load_selected().unwrap();
        editor.form.target_y = "55".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        assert_eq!(table.links.len(), 15);
        assert_eq!(table.links[0].target.y_tile, 0x55);
        assert_eq!(table.links[14].target.x_tile, 0x44);
    }
}
