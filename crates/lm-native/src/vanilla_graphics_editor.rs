use crate::graphics_painter::{
    TILE_GRID_COLUMNS, apply_tile_keyboard_navigation, paint_tile, show_tile_grid_status,
    tile_button, tile_coordinate,
};
use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, GraphicsController, GraphicsControllerEdit, RomExpansionCommand,
};
use lm_graphics::{Bgr555, GraphicsTileChange, IndexedTile, Palette, PaletteInterchangeFile};
use lm_project::GraphicsSaveOptions;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    slot: u16,
}

#[derive(Default)]
pub(crate) struct VanillaGraphicsEditor {
    key: Option<EditorKey>,
    controller: Option<GraphicsController>,
    selected_tile: usize,
    selected_color: u8,
    error: Option<String>,
}

impl VanillaGraphicsEditor {
    pub(crate) fn handles(app: &AppState) -> bool {
        app.revision_profile().is_none()
            && app.controller_snapshot().is_ok_and(|snapshot| {
                matches!(snapshot.mode, EditorMode::Graphics(_)) && is_supported(&snapshot)
            })
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, app: &AppState) -> Option<Command> {
        let snapshot = app.controller_snapshot().ok()?;
        let EditorMode::Graphics(slot) = snapshot.mode else {
            self.clear();
            return None;
        };
        if !is_supported(&snapshot) || app.revision_profile().is_some() {
            self.clear();
            return None;
        }
        let key = EditorKey {
            revision: snapshot.revision,
            slot,
        };
        if self.key != Some(key) {
            self.load(&snapshot, key);
        }
        ui.heading(format!("GFX{slot:02X} — built-in SMW graphics editor"));
        ui.label("Vanilla split pointer planes detected automatically.");
        ui.separator();
        let palette = grayscale_palette();
        let Some(controller) = self.controller.as_ref() else {
            ui.colored_label(
                egui::Color32::RED,
                self.error.as_deref().unwrap_or("graphics load failed"),
            );
            return None;
        };
        let tile_count = controller.graphics().tiles.len();
        self.selected_tile = self.selected_tile.min(tile_count.saturating_sub(1));
        ui.horizontal(|ui| {
            ui.label("Paint color");
            for color in 0_u8..16 {
                let fill = crate::graphics_painter::palette_color(&palette, 0, color);
                if ui
                    .add(
                        egui::Button::new(if color == self.selected_color {
                            "●"
                        } else {
                            ""
                        })
                        .min_size(egui::Vec2::splat(22.0))
                        .fill(fill),
                    )
                    .clicked()
                {
                    self.selected_color = color;
                }
            }
        });
        ui.columns(2, |columns| {
            self.tile_list(&mut columns[0], &palette);
            self.pixel_editor(&mut columns[1], &palette);
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        let expanded = snapshot.rom_bytes.len() > 0x80_000;
        if !expanded {
            ui.label("Graphics relocation needs one expanded free-space bank.");
            if ui.button("Expand ROM to 1 MiB").clicked() {
                return Some(Command::ExpandRom(RomExpansionCommand {
                    expected_revision: snapshot.revision,
                    mapper: snapshot.identity.mapper,
                    target_logical_len: 0x10_0000,
                    fill: 0xff,
                    checksum_field: snapshot.identity.internal_header_offset + 0x1c,
                }));
            }
        }
        let modified = self
            .controller
            .as_ref()
            .is_some_and(GraphicsController::is_modified);
        if ui
            .add_enabled(
                expanded && modified,
                egui::Button::new("Commit graphics changes to ROM"),
            )
            .clicked()
        {
            match prepare_commit(
                self.controller
                    .as_ref()
                    .ok_or("graphics controller is closed")
                    .map_err(str::to_owned),
                &snapshot,
            ) {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        None
    }

    fn load(&mut self, snapshot: &lm_app::ControllerSnapshot, key: EditorKey) {
        match GraphicsController::decode_editable(
            snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        ) {
            Ok(controller) => {
                self.controller = Some(controller);
                self.selected_tile = 0;
                self.selected_color = 1;
                self.error = None;
            }
            Err(error) => {
                self.controller = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn tile_list(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let Some(controller) = &self.controller else {
            return;
        };
        let mut responses = Vec::with_capacity(controller.graphics().tiles.len());
        egui::ScrollArea::vertical()
            .max_height(430.0)
            .show(ui, |ui| {
                egui::Grid::new("vanilla-graphics-tiles").show(ui, |ui| {
                    for (index, tile) in controller.graphics().tiles.iter().enumerate() {
                        let response =
                            tile_button(ui, tile, palette, 0, index == self.selected_tile);
                        if response.clicked() {
                            self.selected_tile = index;
                        }
                        responses.push(response);
                        if index % TILE_GRID_COLUMNS == TILE_GRID_COLUMNS - 1 {
                            ui.end_row();
                        }
                    }
                });
            });
        apply_tile_keyboard_navigation(ui, &mut self.selected_tile, &responses);
        show_tile_grid_status(ui, self.selected_tile, &responses);
    }

    fn pixel_editor(&mut self, ui: &mut egui::Ui, palette: &PaletteInterchangeFile) {
        let tile = self
            .controller
            .as_ref()
            .and_then(|controller| controller.graphics().tiles.get(self.selected_tile))
            .cloned();
        let Some(mut tile) = tile else {
            ui.label("No tiles in this graphics file.");
            return;
        };
        ui.label(format!("Tile {:03X}", self.selected_tile));
        let transform = ui
            .horizontal(|ui| {
                if ui.button("Flip horizontal").clicked() {
                    Some((true, false))
                } else if ui.button("Flip vertical").clicked() {
                    Some((false, true))
                } else {
                    None
                }
            })
            .inner;
        if let Some((horizontal, vertical)) = transform {
            tile = tile.flipped(horizontal, vertical);
            self.apply_tile(tile.clone());
        }
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(320.0), egui::Sense::click_and_drag());
        paint_tile(ui.painter(), rect, &tile, palette, 0);
        if (response.clicked() || response.dragged())
            && let Some(position) = response.interact_pointer_pos()
            && let Some((x, y)) = tile_coordinate(rect, position)
        {
            if let Err(error) = tile.set_pixel(x, y, self.selected_color) {
                self.error = Some(error.to_string());
                return;
            }
            self.apply_tile(tile);
        }
    }

    fn apply_tile(&mut self, tile: IndexedTile) {
        let edit = GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
            index: self.selected_tile,
            tile,
        }]);
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.apply_edits(&[edit])
        {
            self.error = Some(error.to_string());
        }
    }

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.error = None;
    }
}

fn grayscale_palette() -> PaletteInterchangeFile {
    PaletteInterchangeFile {
        source_palette: 0,
        palette: Palette {
            colors: (0_u16..16)
                .map(|component| Bgr555(component | (component << 5) | (component << 10)))
                .collect(),
        },
    }
}

fn is_supported(snapshot: &lm_app::ControllerSnapshot) -> bool {
    snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
        && matches!(
            snapshot.mode,
            EditorMode::Graphics(slot) if usize::from(slot) < lm_profile::SMW_US_V1_VANILLA_GRAPHICS_FILES
        )
}

fn prepare_commit(
    controller: Result<&GraphicsController, String>,
    snapshot: &lm_app::ControllerSnapshot,
) -> Result<Command, String> {
    let controller = controller?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let logical_len = image.logical_len();
    if logical_len <= 0x80_000 {
        return Err("expand the ROM before committing graphics changes".into());
    }
    let layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
    let planes = layout
        .split_pointer_planes
        .ok_or_else(|| "built-in graphics layout lost its pointer planes".to_owned())?;
    let plane_range =
        |offset| ProtectedRange(offset..offset + (planes.entries - 1) * planes.stride + 1);
    controller
        .prepare_commit(
            "Edit pristine SMW graphics",
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x80_000..logical_len,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        plane_range(planes.low_offset),
                        plane_range(planes.high_offset),
                        plane_range(planes.bank_offset),
                        ProtectedRange(
                            snapshot.identity.internal_header_offset
                                ..snapshot.identity.internal_header_offset + 0x40,
                        ),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .map(lm_app::PreparedRomCommit::into_command)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pristine_editor_flips_enter_the_graphics_controller_staging_path() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app.dispatch(Command::ShowGraphics(0)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let controller = GraphicsController::decode_editable(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        let mut editor = VanillaGraphicsEditor {
            controller: Some(controller),
            selected_tile: 0,
            ..VanillaGraphicsEditor::default()
        };
        let original = IndexedTile::new(std::array::from_fn(|index| index.to_le_bytes()[0] & 0x0f));
        editor.apply_tile(original.clone());
        assert_eq!(editor.error, None);
        editor.apply_tile(original.flipped(true, false));
        assert_eq!(editor.error, None);
        let controller = editor.controller.as_ref().unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            controller.graphics().tiles[0],
            original.flipped(true, false)
        );
    }
}
