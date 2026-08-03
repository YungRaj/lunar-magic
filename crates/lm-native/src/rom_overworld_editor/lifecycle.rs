use super::{
    AppState, PendingClose, PendingLoad, PendingOpen, RomOverworldEditor, Workspace, egui,
};
use crate::{
    document_loader::{BoundedRead, LoadedDocument},
    level_editor_forms,
};
use lm_app::{PaletteOwnershipFile, RevisionProfileControllers};
use lm_graphics::{GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile};
use lm_level::Map16SetFile;
use lm_project::Project;
use lm_rom::RomImage;

const OVERWORLD_GRAPHICS_FILES: [usize; 4] = [0x1c, 0x1d, 0x1e, 0x1f];
const TILES_PER_NATIVE_GRAPHICS_SLOT: usize = 0x80;

impl RomOverworldEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match app.profiled_controller_snapshot() {
            Ok(profiled) if profiled.snapshot.mode == lm_app::EditorMode::Overworld => {
                self.pending_open = Some(PendingOpen {
                    profiled,
                    slot: "0".into(),
                });
            }
            Ok(_) => self.error = Some("switch to overworld mode before opening the editor".into()),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.transfer_loader.is_running()
            || self.transfer_persistence.is_running()
        {
            self.error = Some("wait for overworld file work to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.controller.is_modified() {
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

    pub(super) fn open_dialog(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Open native overworld")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Profile overworld slot (hex)");
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.slot);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let slot = match parse_slot(&pending.slot) {
            Ok(slot) => slot,
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_palette_ownership() else {
            self.pending_open = Some(pending);
            return;
        };
        let request = BoundedRead::new(
            path,
            u64::try_from(PaletteOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "palette ownership evidence",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad {
                    open: pending,
                    slot,
                });
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    pub(super) fn finish_ownership_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        current_revision: u64,
    ) {
        let Some(pending) = self.pending_load.take() else {
            self.error = Some("overworld ownership load lost its open request".into());
            return;
        };
        match result.and_then(|loaded| decode_loaded(&pending, loaded, current_revision)) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.search_start.clear();
                self.search_end.clear();
                self.x = 0;
                self.y = 0;
                self.paint_anchor = None;
                self.map16_page = 0;
                self.invalidate();
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending.open);
            }
        }
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged overworld changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Changes across the nine payloads have not been committed.");
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

    pub(super) fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM overworld error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_open = None;
        self.pending_load = None;
        self.pending_close = None;
        self.paint_anchor = None;
        self.texture = None;
        self.map16_texture = None;
        self.invalidate();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn parse_slot(value: &str) -> Result<u16, String> {
    level_editor_forms::parse_hex_u16(value, "overworld slot")
}

fn decode_loaded(
    pending: &PendingLoad,
    loaded: LoadedDocument,
    current_revision: u64,
) -> Result<Workspace, String> {
    crate::rom_load::ensure_current_revision(
        pending.open.profiled.snapshot.revision,
        current_revision,
        "overworld ownership evidence",
    )?;
    let [(_, bytes)] = loaded.into_exact::<1>("overworld ownership")?;
    let ownership = PaletteOwnershipFile::decode(&bytes)
        .map_err(|error| error.to_string())?
        .ownership;
    let profiled = pending.open.profiled.clone();
    let controller = profiled
        .profile
        .decode_overworld(
            &profiled.snapshot,
            usize::from(pending.slot),
            ownership.clone(),
        )
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let assets = decode_overworld_assets(&profiled)?;
    Ok(Workspace {
        controller,
        profiled,
        slot: pending.slot,
        image,
        ownership,
        assets,
    })
}

fn decode_overworld_assets(
    profiled: &lm_app::ProfiledControllerSnapshot,
) -> Result<crate::overworld_editor_render::OverworldAssets, String> {
    let map16 = profiled
        .profile
        .decode_map16(&profiled.snapshot)
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let project = Project::new(image);
    let mut tiles =
        Vec::with_capacity(OVERWORLD_GRAPHICS_FILES.len() * TILES_PER_NATIVE_GRAPHICS_SLOT);
    for file_number in OVERWORLD_GRAPHICS_FILES {
        let slot = project
            .load_graphics_file(file_number, profiled.profile.graphics)
            .map_err(|error| format!("could not load overworld GFX{file_number:02X}: {error}"))?
            .tiles;
        append_overworld_graphics_slot(&mut tiles, file_number, slot)?;
    }
    Ok(crate::overworld_editor_render::OverworldAssets {
        map16: Map16SetFile {
            set: map16.set().clone(),
        },
        graphics: GraphicsInterchangeFile {
            source_slot: u16::try_from(OVERWORLD_GRAPHICS_FILES[0]).unwrap_or_default(),
            graphics: GraphicsFile4bpp { tiles },
        },
    })
}

fn append_overworld_graphics_slot(
    destination: &mut Vec<IndexedTile>,
    file_number: usize,
    mut slot: Vec<IndexedTile>,
) -> Result<(), String> {
    if slot.len() > TILES_PER_NATIVE_GRAPHICS_SLOT {
        return Err(format!(
            "overworld GFX{file_number:02X} has {} tiles; expected at most {TILES_PER_NATIVE_GRAPHICS_SLOT}",
            slot.len()
        ));
    }
    slot.resize_with(TILES_PER_NATIVE_GRAPHICS_SLOT, || {
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
    });
    destination.extend(slot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TILES_PER_NATIVE_GRAPHICS_SLOT, append_overworld_graphics_slot, parse_slot};
    use lm_graphics::IndexedTile;

    #[test]
    fn configured_overworld_slot_is_bound_as_hexadecimal() {
        assert_eq!(parse_slot("0").unwrap(), 0);
        assert_eq!(parse_slot("01ff").unwrap(), 0x01ff);
        assert!(parse_slot("").is_err());
        assert!(parse_slot("10000").is_err());
        assert!(parse_slot("not-a-slot").is_err());
    }

    #[test]
    fn native_overworld_graphics_slots_keep_vram_boundaries() {
        let marked = IndexedTile::new([7; IndexedTile::PIXEL_COUNT]);
        let mut tiles = Vec::new();
        append_overworld_graphics_slot(&mut tiles, 0x1c, vec![marked.clone()]).unwrap();
        append_overworld_graphics_slot(&mut tiles, 0x1d, vec![marked.clone(); 2]).unwrap();
        assert_eq!(tiles.len(), TILES_PER_NATIVE_GRAPHICS_SLOT * 2);
        assert_eq!(tiles[0], marked);
        assert_eq!(
            tiles[TILES_PER_NATIVE_GRAPHICS_SLOT],
            IndexedTile::new([7; IndexedTile::PIXEL_COUNT])
        );
        assert!(tiles[1].pixels().iter().all(|pixel| *pixel == 0));
        assert!(
            append_overworld_graphics_slot(
                &mut tiles,
                0x1e,
                vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
                    TILES_PER_NATIVE_GRAPHICS_SLOT + 1
                ],
            )
            .is_err()
        );
    }
}
