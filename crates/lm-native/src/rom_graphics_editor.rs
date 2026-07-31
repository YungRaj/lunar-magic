use crate::{
    document_loader::DocumentLoader,
    graphics_painter::{paint_tile, palette_color, tile_button, tile_coordinate},
    native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, GraphicsController, GraphicsControllerEdit, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_graphics::{GraphicsTileChange, IndexedTile, PaletteInterchangeFile};

mod commit;
mod external_edit;
mod graphics_batch;
mod graphics_import;
mod lifecycle;
mod ownership;

const STANDARD_GFX_LIMIT: usize = 0x34;
const EXGFX_FIRST: usize = 0x80;
const EXGFX_LIMIT: usize = 0x1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}
struct Workspace {
    controller: GraphicsController,
    profile: RevisionProfile,
    palette: PaletteInterchangeFile,
    slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
}

enum PendingLoad {
    Ownership {
        profiled: ProfiledControllerSnapshot,
    },
    RawImport {
        expected_revision: u64,
    },
}

#[derive(Default)]
pub(crate) struct RomGraphicsEditor {
    workspace: Option<Workspace>,
    selected_tile: usize,
    selected_color: u8,
    palette_row: usize,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    persistence: crate::persistence_worker::PersistenceWorker,
    next_persistence_request: u64,
    io_status: Option<String>,
    graphics_batch: graphics_batch::GraphicsBatchWorker,
    graphics_import: graphics_import::GraphicsImportWorker,
    external_editor: external_edit::ExternalGraphicsEditor,
}

impl RomGraphicsEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.loader.show(context) {
            self.finish_load(result, revision);
        }
        if let Some(completion) = self.persistence.show(context) {
            self.io_status = Some(match completion.result {
                Ok(()) => "Raw graphics file extracted successfully.".into(),
                Err(error) => format!("Raw graphics extraction failed: {error}"),
            });
        }
        if let Some(result) = self.graphics_batch.show(context) {
            self.io_status = Some(match result {
                Ok(Some(count)) => format!("Extracted {count} GFX files successfully."),
                Ok(None) => "GFX extraction cancelled.".into(),
                Err(error) => format!("GFX extraction failed: {error}"),
            });
        }
        if let Some(result) = self.external_editor.show(context) {
            match result.and_then(|completion| {
                ensure_external_edit_revision(completion.expected_revision, revision)?;
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("graphics workspace is closed")?;
                workspace
                    .controller
                    .import_raw(&completion.bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {
                    self.io_status = Some("Externally edited graphics staged successfully.".into());
                }
                Err(error) => self.error = Some(error),
            }
        }
        let import_command = match self.graphics_import.show(context) {
            Some(Ok(Some(commit))) => {
                self.io_status = Some("GFX directory prepared successfully.".into());
                Some(commit.into_command())
            }
            Some(Ok(None)) => {
                self.io_status = Some("GFX insertion cancelled.".into());
                None
            }
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        let mut command =
            import_command.or_else(|| match self.manifest_loader.show(context, revision) {
                Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                    Ok(command) => Some(command),
                    Err(error) => {
                        self.error = Some(error);
                        None
                    }
                },
                Some(Err(error)) => {
                    self.error = Some(error);
                    None
                }
                None => None,
            });
        if self.workspace.is_some() {
            egui::Window::new("ROM Graphics Editor")
                .default_size([780.0, 680.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision)
                        && command.is_none()
                    {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        let file_work_running = self.loader.is_running()
            || self.persistence.is_running()
            || self.graphics_batch.is_running()
            || self.graphics_import.is_running()
            || self.external_editor.is_running();
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let rows = workspace.palette.palette.colors.len() / 16;
        let special_graphics_available = pristine_special_graphics(&workspace.profile);
        let exgraphics_available = supports_exgraphics(&workspace.profile);
        egui::ComboBox::from_label("Palette row")
            .selected_text(format!("{:X}", self.palette_row))
            .show_ui(ui, |ui| {
                for row in 0..rows {
                    ui.selectable_value(&mut self.palette_row, row, format!("{row:X}"));
                }
            });
        let palette = workspace.palette.clone();
        ui.horizontal_wrapped(|ui| {
            for color in 0_u8..16 {
                let fill = palette_color(&palette, self.palette_row, color);
                if ui
                    .add_sized(
                        [26.0, 26.0],
                        egui::Button::new(if color == self.selected_color {
                            "•"
                        } else {
                            ""
                        })
                        .fill(fill),
                    )
                    .clicked()
                {
                    self.selected_color = color;
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Edit current GFX externally…"),
                )
                .on_hover_text("Stages this slot, launches one executable, and reloads after exit")
                .clicked()
            {
                self.begin_external_edit(revision);
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Insert raw GFX/ExGFX…"),
                )
                .clicked()
            {
                self.begin_raw_import(revision);
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract raw GFX/ExGFX…"),
                )
                .clicked()
            {
                self.begin_raw_export();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract all standard GFX…"),
                )
                .clicked()
            {
                self.begin_graphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && special_graphics_available,
                    egui::Button::new("Extract GFX32/GFX33…"),
                )
                .on_hover_text("Uses the authenticated pristine SMW special-pointer operands")
                .clicked()
            {
                self.begin_special_graphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && exgraphics_available,
                    egui::Button::new("Extract installed ExGFX…"),
                )
                .on_hover_text("Exports every nonempty ExGFX pointer from the installed table")
                .clicked()
            {
                self.begin_exgraphics_batch();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running,
                    egui::Button::new("Extract AllGFX.bin…"),
                )
                .clicked()
            {
                self.begin_all_gfx_export();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && !modified_controller(self.workspace.as_ref()),
                    egui::Button::new("Insert all standard GFX…"),
                )
                .on_hover_text("Commit or discard staged tile edits before inserting a directory")
                .clicked()
            {
                self.begin_graphics_import();
            }
            if ui
                .add_enabled(
                    !stale
                        && !file_work_running
                        && !modified_controller(self.workspace.as_ref())
                        && special_graphics_available,
                    egui::Button::new("Insert GFX32/GFX33…"),
                )
                .on_hover_text("Uses the authenticated pristine SMW special-pointer operands")
                .clicked()
            {
                self.begin_special_graphics_import();
            }
            if ui
                .add_enabled(
                    !stale
                        && !file_work_running
                        && !modified_controller(self.workspace.as_ref())
                        && exgraphics_available,
                    egui::Button::new("Insert ExGFX…"),
                )
                .on_hover_text("Atomically inserts the canonical ExGFX files found in a directory")
                .clicked()
            {
                self.begin_exgraphics_import();
            }
            if ui
                .add_enabled(
                    !stale && !file_work_running && !modified_controller(self.workspace.as_ref()),
                    egui::Button::new("Insert AllGFX.bin…"),
                )
                .on_hover_text("Commit or discard staged tile edits before inserting AllGFX.bin")
                .clicked()
            {
                self.begin_all_gfx_import();
            }
        });
        if let Some(status) = &self.io_status {
            ui.label(status);
        }
        ui.separator();
        ui.columns(2, |columns| {
            self.tile_list(&mut columns[0], &palette);
            self.pixel_editor(
                &mut columns[1],
                &palette,
                stale || file_work_running,
                pasted.as_deref(),
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !file_work_running && !self.manifest_loader.is_running(),
                egui::Button::new("Commit graphics to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !file_work_running && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged graphics changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn tile_list(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let tiles = &workspace.controller.graphics().tiles;
        self.selected_tile = self.selected_tile.min(tiles.len().saturating_sub(1));
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                egui::Grid::new("rom-graphics-tiles").show(ui, |ui| {
                    for (index, tile) in tiles.iter().enumerate() {
                        if tile_button(
                            ui,
                            tile,
                            palette,
                            self.palette_row,
                            index == self.selected_tile,
                        )
                        .clicked()
                        {
                            self.selected_tile = index;
                        }
                        if index % 8 == 7 {
                            ui.end_row();
                        }
                    }
                });
            });
    }
    fn pixel_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        stale: bool,
        pasted: Option<&str>,
    ) {
        let tile = self
            .workspace
            .as_ref()
            .and_then(|w| w.controller.graphics().tiles.get(self.selected_tile))
            .cloned();
        let Some(tile) = tile else {
            ui.label("No graphics tiles");
            return;
        };
        ui.label(format!("Tile {:03X}", self.selected_tile));
        let owner = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.controller.ownership().owner(self.selected_tile));
        let editable = ownership::show(ui, owner);
        ui.horizontal(|ui| {
            if ui.button("Copy tile").clicked() {
                match native_clipboard::encode_graphics_tile(&tile) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(!stale && editable, egui::Button::new("Paste tile"))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if !stale
            && editable
            && let Some(text) = pasted
        {
            match native_clipboard::decode_graphics_tile(text) {
                Ok(tile) => self.apply_tile(tile),
                Err(error) => self.error = Some(error),
            }
        }
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(320.0), egui::Sense::click_and_drag());
        paint_tile(ui.painter(), rect, &tile, palette, self.palette_row);
        if !stale
            && editable
            && (response.clicked() || response.dragged())
            && let Some(position) = response.interact_pointer_pos()
            && let Some((x, y)) = tile_coordinate(rect, position)
        {
            self.apply_pixel(x, y, tile);
        }
    }
    fn apply_pixel(&mut self, x: usize, y: usize, mut tile: IndexedTile) {
        if let Err(error) = tile.set_pixel(x, y, self.selected_color) {
            self.error = Some(error.to_string());
            return;
        }
        self.apply_tile(tile);
    }
    fn apply_tile(&mut self, tile: IndexedTile) {
        let edit = GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
            index: self.selected_tile,
            tile,
        }]);
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("graphics workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
            self.error = Some(error.to_string());
        }
    }
}

impl RomGraphicsEditor {
    fn begin_external_edit(&mut self, revision: u64) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let bytes = match workspace.controller.export_raw() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let Some(executable) = crate::dialogs::choose_external_graphics_editor() else {
            return;
        };
        let file_name = crate::dialogs::raw_graphics_file_name(workspace.slot);
        match self
            .external_editor
            .stage(executable, &file_name, &bytes, revision)
        {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_raw_import(&mut self, revision: u64) {
        let maximum = match self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.controller.export_raw().ok())
            .and_then(|bytes| u64::try_from(bytes.len()).ok())
        {
            Some(maximum) => maximum,
            None => {
                self.error = Some("could not determine the current raw graphics size".into());
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_raw_graphics() else {
            return;
        };
        let request = crate::document_loader::BoundedRead::new(path, maximum, "raw GFX/ExGFX file");
        match self.loader.start(vec![request]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad::RawImport {
                    expected_revision: revision,
                });
                self.io_status = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_raw_export(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(path) = crate::dialogs::choose_raw_graphics_save_path(workspace.slot) else {
            return;
        };
        let bytes = match workspace.controller.export_raw() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        self.next_persistence_request = self.next_persistence_request.wrapping_add(1);
        if let Err(error) = self.persistence.start(
            self.next_persistence_request,
            crate::persistence_worker::PersistenceTarget::Create(path),
            bytes,
        ) {
            self.error = Some(error);
        } else {
            self.io_status = None;
        }
    }

    fn begin_graphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(directory) = crate::dialogs::choose_graphics_directory() else {
            return;
        };
        let slots = standard_graphics_slots(workspace.profile.graphics);
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "standard",
        };
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_graphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_graphics_import_directory() else {
            return;
        };
        let slots = standard_graphics_slots(workspace.profile.graphics);
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: slots.clone(),
            file_numbers: slots,
            family: "standard",
            description: "Insert all standard GFX files",
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_all_gfx_export(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(path) = crate::dialogs::choose_all_gfx_save_path() else {
            return;
        };
        let slots = standard_graphics_slots(workspace.profile.graphics);
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "standard",
        };
        match self.graphics_batch.start_joined(source, path) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_all_gfx_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_all_gfx_file() else {
            return;
        };
        let slots = standard_graphics_slots(workspace.profile.graphics);
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: slots.clone(),
            file_numbers: slots,
            family: "standard",
            description: "Insert AllGFX.bin",
        };
        match self.graphics_import.start_joined(source, path) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_special_graphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(directory) = crate::dialogs::choose_graphics_directory() else {
            return;
        };
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
            slots: vec![0, 1],
            file_numbers: vec![0x33, 0x32],
            family: "special",
        };
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_special_graphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_graphics_import_directory() else {
            return;
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: vec![0, 1],
            file_numbers: vec![0x33, 0x32],
            family: "special",
            description: "Insert GFX32/GFX33 files",
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_exgraphics_batch(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let slots = match installed_exgraphics_slots(&workspace.image, workspace.profile.graphics) {
            Ok(slots) if !slots.is_empty() => slots,
            Ok(_) => {
                self.error = Some("the installed graphics table contains no ExGFX files".into());
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_exgraphics_directory() else {
            return;
        };
        let source = graphics_batch::GraphicsBatchSource {
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            slots: slots.clone(),
            file_numbers: slots,
            family: "extended",
        };
        match self.graphics_batch.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }

    fn begin_exgraphics_import(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let options = match self.save_options(workspace) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(directory) = crate::dialogs::choose_exgraphics_import_directory() else {
            return;
        };
        let slots = match graphics_import::enumerate_exgraphics_files(
            &directory,
            workspace.profile.graphics.pointers.entries,
        ) {
            Ok(slots) => slots,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let source = graphics_import::GraphicsImportSource {
            expected_revision: workspace.controller.revision(),
            image: workspace.image.clone(),
            layout: workspace.profile.graphics,
            checksum_field: workspace.internal_header + 0x1c,
            options,
            slots: slots.clone(),
            file_numbers: slots,
            family: "extended",
            description: "Insert ExGFX files",
        };
        match self.graphics_import.start(source, directory) {
            Ok(()) => self.io_status = None,
            Err(error) => self.error = Some(error),
        }
    }
}

fn modified_controller(workspace: Option<&Workspace>) -> bool {
    workspace.is_some_and(|workspace| workspace.controller.is_modified())
}

fn ensure_external_edit_revision(expected: u64, current: u64) -> Result<(), String> {
    crate::rom_load::ensure_current_revision(expected, current, "external graphics reload")
}

fn pristine_special_graphics(profile: &RevisionProfile) -> bool {
    profile.game == lm_rom::SupportedGame::SuperMarioWorld
        && profile.region == lm_rom::Region::NorthAmerica
        && profile.revision == 0
        && profile.mapper == lm_rom::Mapper::LoRom
        && profile.graphics == lm_profile::smw_us_v1_vanilla_graphics_layout()
}

fn supports_exgraphics(profile: &RevisionProfile) -> bool {
    (EXGFX_FIRST + 1..=EXGFX_LIMIT).contains(&profile.graphics.pointers.entries)
}

fn standard_graphics_slots(layout: lm_project::GraphicsRomLayout) -> Vec<usize> {
    (0..layout.pointers.entries.min(STANDARD_GFX_LIMIT)).collect()
}

fn installed_exgraphics_slots(
    image: &lm_rom::RomImage,
    layout: lm_project::GraphicsRomLayout,
) -> Result<Vec<usize>, String> {
    if !(EXGFX_FIRST + 1..=EXGFX_LIMIT).contains(&layout.pointers.entries) {
        return Err(format!(
            "profile graphics table has {} entries; ExGFX requires 129 through 4096",
            layout.pointers.entries
        ));
    }
    let project = lm_project::Project::new(image.clone());
    (EXGFX_FIRST..layout.pointers.entries)
        .filter_map(|slot| match layout.read_pointer(&project, slot) {
            Ok(pointer) if pointer.get() == 0 => None,
            Ok(_) => Some(Ok(slot)),
            Err(error) => Some(Err(format!("ExGFX{slot:02X}: {error}"))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_external_edit_revision, installed_exgraphics_slots, pristine_special_graphics,
        supports_exgraphics,
    };
    use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable};
    use lm_rom::{Mapper, RomImage};

    #[test]
    fn special_pair_actions_require_the_exact_recovered_split_layout() {
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        assert!(pristine_special_graphics(&profile));
        profile.graphics.split_pointer_planes = None;
        assert!(!pristine_special_graphics(&profile));
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile
            .graphics
            .split_pointer_planes
            .as_mut()
            .unwrap()
            .bank_offset += 1;
        assert!(!pristine_special_graphics(&profile));
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile.region = lm_rom::Region::Japan;
        assert!(!pristine_special_graphics(&profile));
    }

    #[test]
    fn installed_exgraphics_enumeration_uses_nonzero_pointer_entries_only() {
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x100,
                entries: 0x83,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        };
        let mut bytes = vec![0; 0x8000];
        for slot in [0x80, 0x82] {
            let offset = layout.pointers.offset + slot * 3;
            bytes[offset..offset + 3].copy_from_slice(&[0x00, 0x81, 0x80]);
        }
        let image = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            installed_exgraphics_slots(&image, layout).unwrap(),
            [0x80, 0x82]
        );

        let mut profile = lm_profile::test_support::profile();
        profile.graphics = layout;
        assert!(supports_exgraphics(&profile));
        profile.graphics.pointers.entries = 0x80;
        assert!(!supports_exgraphics(&profile));
    }

    #[test]
    fn external_reload_is_bound_to_the_revision_that_was_staged() {
        ensure_external_edit_revision(12, 12).unwrap();
        let error = ensure_external_edit_revision(12, 13).unwrap_err();
        assert!(error.contains("external graphics reload"), "{error}");
    }
}
