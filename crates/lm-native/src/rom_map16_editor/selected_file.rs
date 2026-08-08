use super::{Map16ControllerEdit, RomMap16Editor};
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_level::{Lm16Map16File, Map16Address, Map16Page, Map16Set, Map16Tile};

const PROTECTED_FOREGROUND_TILES: usize = 0x200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedFileShortcut {
    Import,
    Export,
}

pub(super) struct PendingSelectedImport {
    revision: u64,
    destination: Option<usize>,
}

impl RomMap16Editor {
    pub(super) fn poll_selected_file_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.selected_loader.show(context) {
            let pending = self.pending_selected_import.take();
            let result = result.and_then(|loaded| {
                let pending = pending.ok_or("selected Map16 import request is missing")?;
                let [(_, bytes)] = loaded.into_exact::<1>("selected Map16")?;
                let file = Lm16Map16File::decode(&bytes).map_err(|error| error.to_string())?;
                let replacements = {
                    let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                    validate_import_revision(workspace.controller.revision(), pending.revision)?;
                    import_replacements(&file, pending.destination, workspace.controller.set())?
                };
                let resolution_limit = self
                    .workspace
                    .as_ref()
                    .ok_or("Map16 workspace is closed")?
                    .controller
                    .set()
                    .pages
                    .len()
                    * Map16Page::TILE_COUNT;
                self.apply_staged_edits(&[Map16ControllerEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                }])
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.selected_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn selected_file_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        project_revision: u64,
        pasted: Option<&str>,
    ) {
        let busy = self.complete_loader.is_running()
            || self.complete_persistence.is_running()
            || self.selected_loader.is_running()
            || self.selected_persistence.is_running()
            || self.legacy_page_loader.is_running()
            || self.legacy_page_persistence.is_running()
            || self.bitmap_loader.is_running()
            || self.bitmap_clipboard_loader.is_running()
            || self.bitmap_session.is_some();
        let shortcut = take_selected_file_shortcut(ui);
        ui.horizontal(|ui| {
            ui.label("Selected range width");
            ui.text_edit_singleline(&mut self.selected_width);
            ui.label("height");
            ui.text_edit_singleline(&mut self.selected_height);
            ui.checkbox(&mut self.selected_use_file_origin, "Import at file origin");
        });
        ui.horizontal(|ui| {
            let import_clicked = ui
                .add_enabled(
                    !blocked && !busy,
                    egui::Button::new("Import selected .map16…").shortcut_text("F3"),
                )
                .clicked();
            if !blocked
                && !busy
                && (import_clicked || shortcut == Some(SelectedFileShortcut::Import))
                && let Some(path) = dialogs::choose_selected_map16_document()
            {
                let destination = (!self.selected_use_file_origin)
                    .then_some(self.page * Map16Page::TILE_COUNT + self.tile);
                match self.selected_loader.start(vec![BoundedRead::new(
                    path,
                    Lm16Map16File::MAX_FILE_LEN as u64,
                    "selected Map16 file",
                )]) {
                    Ok(()) => {
                        self.pending_selected_import = Some(PendingSelectedImport {
                            revision: project_revision,
                            destination,
                        });
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            let export_clicked = ui
                .add_enabled(
                    !blocked && !busy,
                    egui::Button::new("Export selected .map16…").shortcut_text("F2"),
                )
                .clicked();
            if !blocked
                && !busy
                && (export_clicked || shortcut == Some(SelectedFileShortcut::Export))
                && let Some(path) = dialogs::choose_selected_map16_save_path()
            {
                let result = parse_dimensions(&self.selected_width, &self.selected_height)
                    .and_then(|(width, height)| {
                        let workspace =
                            self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                        export_file(
                            workspace.controller.set(),
                            self.page * Map16Page::TILE_COUNT + self.tile,
                            width,
                            height,
                        )
                    })
                    .and_then(|file| {
                        self.selected_persistence.start(
                            project_revision,
                            PersistenceTarget::Create(path),
                            file.encode(),
                        )
                    });
                if let Err(error) = result {
                    self.error = Some(error);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!blocked && !busy, egui::Button::new("Copy rectangle"))
                .clicked()
            {
                let result = parse_dimensions(&self.selected_width, &self.selected_height)
                    .and_then(|(width, height)| {
                        let origin = self.page * Map16Page::TILE_COUNT + self.tile;
                        let workspace =
                            self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                        let tiles =
                            rectangle_tiles(workspace.controller.set(), origin, width, height)?;
                        lm_app::NativeMap16Clipboard::from_rectangle(
                            u32::try_from(origin)
                                .map_err(|_| "Map16 clipboard origin overflow".to_owned())?,
                            u32::try_from(width)
                                .map_err(|_| "Map16 clipboard width overflow".to_owned())?,
                            u32::try_from(height)
                                .map_err(|_| "Map16 clipboard height overflow".to_owned())?,
                            tiles,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .and_then(|rectangle| {
                        crate::native_clipboard::encode_native_map16_rectangle(&rectangle)
                    });
                match result {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(!blocked && !busy, egui::Button::new("Paste rectangle"))
                .clicked()
            {
                self.clipboard_paste_target = None;
                self.rectangle_clipboard_paste_target = self.workspace.as_ref().map(|workspace| {
                    (
                        workspace.controller.revision(),
                        self.staged_revision,
                        self.page * Map16Page::TILE_COUNT + self.tile,
                    )
                });
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(text) = pasted
            && let Some((revision, staged_revision, destination)) =
                self.rectangle_clipboard_paste_target.take()
        {
            if blocked || busy {
                self.error = Some(
                    "the ROM or Map16 editor changed while waiting for rectangle clipboard data"
                        .into(),
                );
            } else if let Err(error) =
                self.paste_rectangle_at(text, revision, staged_revision, destination)
            {
                self.error = Some(error);
            }
        }
        ui.small("Selected .map16 ranges use Lunar Magic's compact LM16 width, height, origin, band flags, definitions, and Acts Like sections. Width and height are hexadecimal; disable file-origin import to place at the selected tile.");
    }

    pub(super) fn paste_rectangle_at(
        &mut self,
        text: &str,
        revision: u64,
        staged_revision: u64,
        destination: usize,
    ) -> Result<(), String> {
        let rectangle = crate::native_clipboard::decode_native_map16_rectangle(text)?;
        let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
        if workspace.controller.revision() != revision || self.staged_revision != staged_revision {
            return Err(
                "the ROM Map16 state changed while waiting for rectangle clipboard data".into(),
            );
        }
        let replacements = rectangle_replacements(
            &rectangle.tiles,
            destination,
            usize::try_from(rectangle.width)
                .map_err(|_| "Map16 clipboard width overflow".to_owned())?,
            usize::try_from(rectangle.height)
                .map_err(|_| "Map16 clipboard height overflow".to_owned())?,
            workspace.controller.set(),
        )?;
        let resolution_limit = workspace.controller.set().pages.len() * Map16Page::TILE_COUNT;
        self.apply_staged_edits(&[Map16ControllerEdit::ReplaceTiles {
            replacements,
            resolution_limit,
        }])
    }
}

fn take_selected_file_shortcut(ui: &mut egui::Ui) -> Option<SelectedFileShortcut> {
    ui.input_mut(|input| {
        if input.modifiers == egui::Modifiers::NONE
            && input.consume_key(egui::Modifiers::NONE, egui::Key::F2)
        {
            Some(SelectedFileShortcut::Export)
        } else {
            let modifiers = input.modifiers;
            input
                .consume_key(modifiers, egui::Key::F3)
                .then_some(SelectedFileShortcut::Import)
        }
    })
}

fn parse_dimensions(width: &str, height: &str) -> Result<(usize, usize), String> {
    let width = usize::from_str_radix(width.trim(), 16)
        .map_err(|_| "selected Map16 width must be hexadecimal".to_owned())?;
    let height = usize::from_str_radix(height.trim(), 16)
        .map_err(|_| "selected Map16 height must be hexadecimal".to_owned())?;
    if width == 0 || width > 0x10 || height == 0 {
        return Err("selected Map16 width must be 1–10 and height must be nonzero".into());
    }
    Ok((width, height))
}

fn validate_import_revision(current: u64, requested: u64) -> Result<(), String> {
    if current != requested {
        return Err("the ROM changed while selected Map16 was loading".into());
    }
    Ok(())
}

pub(super) fn export_file(
    set: &Map16Set,
    origin: usize,
    width: usize,
    height: usize,
) -> Result<Lm16Map16File, String> {
    let tiles = rectangle_tiles(set, origin, width, height)?;
    Lm16Map16File::from_selected_tiles(origin, width, height, &tiles)
        .map_err(|error| error.to_string())
}

pub(super) fn import_replacements(
    file: &Lm16Map16File,
    destination: Option<usize>,
    current: &Map16Set,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    let (file_origin, imported) = file.selected_tiles().map_err(|error| error.to_string())?;
    let origin = destination.unwrap_or(file_origin);
    rectangle_replacements(
        &imported,
        origin,
        file.selection_width,
        file.selection_height,
        current,
    )
}

fn rectangle_replacements(
    imported: &[Map16Tile],
    origin: usize,
    width: usize,
    height: usize,
    current: &Map16Set,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    let targets = rectangle_addresses(current, origin, width, height)?;
    if targets.len() != imported.len() {
        return Err("selected Map16 semantic section count changed unexpectedly".into());
    }
    Ok(targets
        .into_iter()
        .zip(imported.iter().copied())
        .map(|(address, mut tile)| {
            let global = address.page * Map16Page::TILE_COUNT + address.tile;
            if global < PROTECTED_FOREGROUND_TILES {
                let existing = current.pages[address.page].tiles[address.tile];
                tile.top_left = existing.top_left;
                tile.top_right = existing.top_right;
                tile.bottom_left = existing.bottom_left;
                tile.bottom_right = existing.bottom_right;
            }
            if address.page >= lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                tile.acts_like = 0;
            }
            (address, tile)
        })
        .collect())
}

fn rectangle_tiles(
    set: &Map16Set,
    origin: usize,
    width: usize,
    height: usize,
) -> Result<Vec<Map16Tile>, String> {
    rectangle_addresses(set, origin, width, height)?
        .into_iter()
        .map(|address| {
            set.pages
                .get(address.page)
                .and_then(|page| page.tiles.get(address.tile))
                .copied()
                .ok_or_else(|| {
                    format!(
                        "Map16 tile {:04X} is unavailable",
                        address.page * 0x100 + address.tile
                    )
                })
        })
        .collect()
}

fn rectangle_addresses(
    set: &Map16Set,
    origin: usize,
    width: usize,
    height: usize,
) -> Result<Vec<Map16Address>, String> {
    let total = set
        .pages
        .len()
        .checked_mul(Map16Page::TILE_COUNT)
        .ok_or("Map16 namespace size overflow")?;
    let column = origin % 0x10;
    let row = origin / 0x10;
    if width == 0
        || width > 0x10
        || height == 0
        || origin >= total
        || column + width > 0x10
        || row
            .checked_add(height)
            .and_then(|end| end.checked_mul(0x10))
            .is_none_or(|end| end > total)
    {
        return Err(format!(
            "selected Map16 rectangle {origin:04X} {width:X}×{height:X} is outside the workspace"
        ));
    }
    let mut addresses = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let global = origin + y * 0x10 + x;
            addresses.push(Map16Address {
                page: global / Map16Page::TILE_COUNT,
                tile: global % Map16Page::TILE_COUNT,
            });
        }
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::Subtile;

    fn observed_shortcut(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> (Option<SelectedFileShortcut>, Option<SelectedFileShortcut>) {
        let context = egui::Context::default();
        let mut first = None;
        let mut second = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    first = take_selected_file_shortcut(ui);
                    second = take_selected_file_shortcut(ui);
                });
            },
        );
        (first, second)
    }

    fn complete_set() -> Map16Set {
        Map16Set {
            pages: (0..lm_app::SMW_COMPLETE_MAP16_PAGES)
                .map(|page| Map16Page {
                    tiles: (0..Map16Page::TILE_COUNT)
                        .map(|tile| {
                            let global = page * Map16Page::TILE_COUNT + tile;
                            Map16Tile {
                                top_left: Subtile(u16::try_from(global).unwrap()),
                                top_right: Subtile(2),
                                bottom_left: Subtile(3),
                                bottom_right: Subtile(4),
                                acts_like: u16::try_from(global).unwrap_or(0),
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn selected_helpers_round_trip_a_rectangle_across_page_rows() {
        let set = complete_set();
        let file = export_file(&set, 0x23fe, 2, 2).unwrap();
        assert_eq!(file.selection_width, 2);
        assert_eq!(file.selection_height, 2);
        let replacements = import_replacements(&file, None, &set).unwrap();
        assert_eq!(
            replacements
                .iter()
                .map(|(address, _)| *address)
                .collect::<Vec<_>>(),
            [0x23fe, 0x23ff, 0x240e, 0x240f].map(|global| Map16Address {
                page: global / 0x100,
                tile: global % 0x100,
            })
        );
        assert_eq!(replacements[0].1, set.pages[0x23].tiles[0xfe]);
        assert_eq!(replacements[3].1, set.pages[0x24].tiles[0x0f]);
    }

    #[test]
    fn selected_import_can_retarget_and_preserves_protected_or_background_semantics() {
        let set = complete_set();
        let source = export_file(&set, 0x220, 1, 1).unwrap();
        let protected = import_replacements(&source, Some(0x100), &set).unwrap();
        assert_eq!(protected[0].1.top_left, set.pages[1].tiles[0].top_left);
        assert_eq!(
            protected[0].1.acts_like,
            set.pages[0x02].tiles[0x20].acts_like
        );

        let background = import_replacements(&source, Some(0x8000), &set).unwrap();
        assert_eq!(
            background[0].1.top_left,
            set.pages[0x02].tiles[0x20].top_left
        );
        assert_eq!(background[0].1.acts_like, 0);
    }

    #[test]
    fn selected_rectangles_reject_row_wrap_and_workspace_overflow() {
        let set = complete_set();
        assert!(export_file(&set, 0x00ff, 2, 1).is_err());
        assert!(export_file(&set, 0xffff, 1, 2).is_err());
        assert!(parse_dimensions("11", "1").is_err());
    }

    #[test]
    fn selected_import_is_bound_to_the_revision_that_started_loading() {
        assert!(validate_import_revision(9, 9).is_ok());
        assert_eq!(
            validate_import_revision(10, 9).unwrap_err(),
            "the ROM changed while selected Map16 was loading"
        );
    }

    #[test]
    fn original_selected_file_shortcuts_match_f2_export_and_f3_import_modifiers() {
        assert_eq!(
            observed_shortcut(egui::Key::F2, egui::Modifiers::NONE),
            (Some(SelectedFileShortcut::Export), None)
        );
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::COMMAND,
        ] {
            assert_eq!(observed_shortcut(egui::Key::F2, modifiers), (None, None));
            assert_eq!(
                observed_shortcut(egui::Key::F3, modifiers),
                (Some(SelectedFileShortcut::Import), None)
            );
        }
        assert_eq!(
            observed_shortcut(egui::Key::F3, egui::Modifiers::NONE),
            (Some(SelectedFileShortcut::Import), None)
        );
    }
}
