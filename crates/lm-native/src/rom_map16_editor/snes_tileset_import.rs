use super::RomMap16Editor;
use crate::{dialogs, document_loader::BoundedRead};
use eframe::egui;
use lm_app::{
    MaterializedSnesMap16Tileset, SNES_TILESET_GRAPHICS_LEN, SNES_TILESET_MAP_LEN,
    SNES_TILESET_PALETTE_ROW_LEN, SnesMap16DefinitionPlacement, SnesMap16TilesetImport,
};
use lm_level::Map16Page;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingSnesTileset {
    revision: u64,
    page: usize,
    placement: SnesMap16DefinitionPlacement,
    includes_palette: bool,
}

pub(super) struct SnesTilesetPreview {
    pending: PendingSnesTileset,
    materialized: MaterializedSnesMap16Tileset,
    candidate_page: Map16Page,
    assignments: Vec<u16>,
    written_definitions: usize,
    has_palette: bool,
}

impl RomMap16Editor {
    pub(super) fn poll_snes_tileset_io(&mut self, context: &egui::Context) {
        let Some(result) = self.snes_tileset_loader.show(context) else {
            return;
        };
        let pending = self.pending_snes_tileset.take();
        let result = result.and_then(|loaded| {
            let pending = pending.ok_or("SNES tileset request is missing")?;
            let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
            if workspace.controller.revision() != pending.revision {
                return Err("the ROM changed while the SNES tileset was loading".into());
            }
            let target = workspace
                .controller
                .set()
                .pages
                .get(pending.page)
                .ok_or_else(|| format!("Map16 page {:02X} is unavailable", pending.page))?;
            let files = loaded.files;
            let expected = if pending.includes_palette { 3 } else { 2 };
            if files.len() != expected {
                return Err(format!(
                    "SNES tileset loader returned {} files, expected {expected}",
                    files.len()
                ));
            }
            let palette = pending.includes_palette.then(|| files[2].1.as_slice());
            self.snes_tileset_preview = Some(prepare_preview(
                pending,
                &files[0].1,
                &files[1].1,
                palette,
                target,
            )?);
            Ok(())
        });
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn snes_tileset_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        project_revision: u64,
    ) {
        ui.separator();
        ui.label("SNES graphics set + screen tile map");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.snes_tileset_include_palette, "Import palette row");
            ui.checkbox(
                &mut self.snes_tileset_deduplicate,
                "Optimize Map16 definitions",
            );
            if ui
                .add_enabled(
                    !blocked && self.page < 0x100,
                    egui::Button::new("Load SNES tileset…"),
                )
                .clicked()
            {
                self.start_snes_tileset_load(project_revision);
            }
        });
        ui.small("Loads the original .set/.bin plus 32×32 .map workflow with an optional 16-color .col/.pal row. Preview is revision-bound and blocks conflicting Map16 work.");
    }

    fn start_snes_tileset_load(&mut self, project_revision: u64) {
        let Some(graphics) = dialogs::choose_snes_graphics_set() else {
            return;
        };
        let Some(tilemap) = dialogs::choose_snes_screen_tile_map() else {
            return;
        };
        let palette = if self.snes_tileset_include_palette {
            let Some(path) = dialogs::choose_snes_palette_row() else {
                return;
            };
            Some(path)
        } else {
            None
        };
        let mut reads = vec![
            BoundedRead::new(graphics, SNES_TILESET_GRAPHICS_LEN as u64, "SNES GFX set"),
            BoundedRead::new(tilemap, SNES_TILESET_MAP_LEN as u64, "SNES screen tile map"),
        ];
        if let Some(path) = palette {
            reads.push(BoundedRead::new(
                path,
                SNES_TILESET_PALETTE_ROW_LEN as u64,
                "SNES palette row",
            ));
        }
        match self.snes_tileset_loader.start(reads) {
            Ok(()) => {
                self.pending_snes_tileset = Some(PendingSnesTileset {
                    revision: project_revision,
                    page: self.page,
                    placement: if self.snes_tileset_deduplicate {
                        SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions
                    } else {
                        SnesMap16DefinitionPlacement::Direct
                    },
                    includes_palette: self.snes_tileset_include_palette,
                });
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn snes_tileset_preview_window(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) {
        let Some(preview) = self.snes_tileset_preview.as_ref() else {
            return;
        };
        let stale = preview.pending.revision != project_revision;
        let page = preview.pending.page;
        let placement = preview.pending.placement;
        let written = preview.written_definitions;
        let has_palette = preview.has_palette;
        let assignment_span = preview
            .assignments
            .first()
            .zip(preview.assignments.last())
            .map(|(first, last)| format!("${first:04X}–${last:04X}"))
            .unwrap_or_else(|| "none".into());
        // Keep the complete staged products alive until discard or the atomic ROM route consumes
        // them. Reading these shapes here also guards accidental partial preview construction.
        let graphics_tiles = preview.materialized.graphics.tiles.len();
        let candidate_tiles = preview.candidate_page.tiles.len();
        let mut discard = false;
        egui::Window::new("SNES tileset import preview")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Target Map16 page: ${page:02X}"));
                ui.label(format!("Placement: {placement:?}"));
                ui.label(format!("Graphics tiles: {graphics_tiles}"));
                ui.label(format!("Candidate definitions: {candidate_tiles}"));
                ui.label(format!("Definitions written: {written}"));
                ui.label(format!("Index-grid span: {assignment_span}"));
                ui.label(format!("Palette row loaded: {}", if has_palette { "yes" } else { "no" }));
                if stale {
                    ui.colored_label(egui::Color32::YELLOW, "The ROM changed; discard this preview.");
                }
                ui.small("The decoded graphics, optional palette, candidate page, and background index grid are retained together for the atomic ROM-application milestone.");
                if ui.button("Discard preview").clicked() {
                    discard = true;
                }
            });
        if discard {
            self.snes_tileset_preview = None;
        }
    }
}

fn prepare_preview(
    pending: PendingSnesTileset,
    graphics: &[u8],
    tilemap: &[u8],
    palette: Option<&[u8]>,
    target: &Map16Page,
) -> Result<SnesTilesetPreview, String> {
    let decoded = SnesMap16TilesetImport::decode(graphics, tilemap, palette)
        .map_err(|error| error.to_string())?;
    // The installed editor's current 1,024-tile workspace is canonical, so no additional external
    // graphics-remap stream is active here.
    let remap = std::array::from_fn(|index| u16::try_from(index).unwrap());
    let materialized = decoded
        .materialize(&remap)
        .map_err(|error| error.to_string())?;
    let mut candidate_page = target.clone();
    let applied = materialized
        .apply_to_page(
            &mut candidate_page,
            u8::try_from(pending.page).map_err(|_| "SNES tileset target page exceeds FF")?,
            pending.placement,
        )
        .map_err(|error| error.to_string())?;
    Ok(SnesTilesetPreview {
        pending,
        has_palette: decoded.palette_row.is_some(),
        materialized,
        candidate_page,
        assignments: applied.assignments,
        written_definitions: applied.written_definitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::LUNAR_MAGIC_BLANK_MAP16_WORD;
    use lm_level::{Map16Tile, Subtile};

    fn blank_page() -> Map16Page {
        Map16Page::new(vec![
            Map16Tile {
                top_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                top_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                bottom_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                bottom_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                acts_like: 0x130,
            };
            Map16Page::TILE_COUNT
        ])
        .unwrap()
    }

    fn pending(placement: SnesMap16DefinitionPlacement) -> PendingSnesTileset {
        PendingSnesTileset {
            revision: 7,
            page: 0x82,
            placement,
            includes_palette: false,
        }
    }

    #[test]
    fn native_preview_retains_all_atomic_products_and_target_behavior() {
        let preview = prepare_preview(
            pending(SnesMap16DefinitionPlacement::Direct),
            &[0; 32],
            &[0; SNES_TILESET_MAP_LEN],
            None,
            &blank_page(),
        )
        .unwrap();
        assert_eq!(preview.materialized.graphics.tiles.len(), 1024);
        assert_eq!(preview.candidate_page.tiles.len(), 256);
        assert_eq!(preview.candidate_page.tiles[0].acts_like, 0x130);
        assert_eq!(preview.assignments[0], 0x8200);
        assert_eq!(preview.assignments[255], 0x82ff);
        assert_eq!(preview.written_definitions, 256);
        assert!(!preview.has_palette);
    }

    #[test]
    fn native_preview_rejects_staged_shape_or_optimized_space_before_mutation() {
        assert!(
            prepare_preview(
                pending(SnesMap16DefinitionPlacement::Direct),
                &[],
                &[0; SNES_TILESET_MAP_LEN - 1],
                None,
                &blank_page(),
            )
            .is_err()
        );

        let mut occupied = blank_page();
        for tile in &mut occupied.tiles {
            tile.top_left = Subtile(0x2222);
        }
        let before = occupied.clone();
        assert!(
            prepare_preview(
                pending(SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions),
                &[],
                &[0; SNES_TILESET_MAP_LEN],
                None,
                &occupied,
            )
            .is_err()
        );
        assert_eq!(occupied, before);
    }
}
