use super::{Map16ControllerEdit, RomMap16Editor};
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_level::{
    Lm16Map16File, Lm16Map16SectionKind, Map16Address, Map16Page, Map16Set, Map16Tile, Subtile,
};

const PROTECTED_FOREGROUND_TILES: usize = 0x200;

impl RomMap16Editor {
    pub(super) fn poll_complete_file_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.complete_loader.show(context) {
            let pending_revision = self.pending_complete_revision.take();
            let result = result.and_then(|loaded| {
                let requested_revision = pending_revision
                    .ok_or("complete Map16 import request is missing its ROM revision")?;
                let [(_, bytes)] = loaded.into_exact::<1>("complete Map16")?;
                let file = Lm16Map16File::decode(&bytes).map_err(|error| error.to_string())?;
                let replacements = {
                    let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                    validate_import_revision(workspace.controller.revision(), requested_revision)?;
                    import_replacements(&file, workspace.controller.set())?
                };
                let resolution_limit = lm_app::SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT;
                self.apply_staged_edits(&[Map16ControllerEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                }])?;
                self.complete_template = Some(file);
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.complete_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn complete_file_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
    ) {
        let busy = self.complete_loader.is_running() || self.complete_persistence.is_running();
        let supported = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.supports_complete_lm_file());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    supported && !stale && !busy,
                    egui::Button::new("Import complete .map16…"),
                )
                .clicked()
                && let Some(path) = dialogs::choose_complete_map16_document()
            {
                match self.complete_loader.start(vec![BoundedRead::new(
                    path,
                    Lm16Map16File::MAX_FILE_LEN as u64,
                    "complete Map16 file",
                )]) {
                    Ok(()) => self.pending_complete_revision = Some(project_revision),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    supported && !stale && !busy,
                    egui::Button::new("Export complete .map16…"),
                )
                .clicked()
                && let Some(path) = dialogs::choose_complete_map16_save_path()
            {
                let result = self
                    .workspace
                    .as_ref()
                    .ok_or_else(|| "Map16 workspace is closed".to_owned())
                    .and_then(|workspace| {
                        export_file(workspace.controller.set(), self.complete_template.as_ref())
                    })
                    .and_then(|file| {
                        self.complete_persistence.start(
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
        if self.complete_template.is_some() {
            ui.small(
                "Export preserves auxiliary and editor-state sections from the imported file.",
            );
        }
        if !supported {
            ui.small(
                "Complete Lunar Magic .map16 transfer requires the native 256-page SMW workspace.",
            );
        }
    }
}

fn validate_import_revision(current: u64, requested: u64) -> Result<(), String> {
    if current != requested {
        return Err("the ROM changed while the complete Map16 file was loading".into());
    }
    Ok(())
}

pub(super) fn export_file(
    set: &Map16Set,
    template: Option<&Lm16Map16File>,
) -> Result<Lm16Map16File, String> {
    if set.pages.len() != lm_app::SMW_COMPLETE_MAP16_PAGES {
        return Err(format!(
            "complete SMW Map16 export requires {} pages, got {}",
            lm_app::SMW_COMPLETE_MAP16_PAGES,
            set.pages.len()
        ));
    }
    let mut combined = Vec::with_capacity(Lm16Map16File::COMBINED_TILES_LEN);
    let mut acts_like = Vec::with_capacity(Lm16Map16File::ACTS_LIKE_LEN);
    for (page, value) in set.pages.iter().enumerate() {
        if value.tiles.len() != Map16Page::TILE_COUNT {
            return Err(format!(
                "complete SMW Map16 page {page:02X} has {} tiles",
                value.tiles.len()
            ));
        }
        for (tile, value) in value.tiles.iter().enumerate() {
            let global_tile = page * Map16Page::TILE_COUNT + tile;
            let words = if global_tile < PROTECTED_FOREGROUND_TILES {
                [0; 4]
            } else {
                tile_words(*value)
            };
            for word in words {
                combined.extend_from_slice(&word.to_le_bytes());
            }
            if page < lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                acts_like.extend_from_slice(&value.acts_like.to_le_bytes());
            }
        }
    }
    let mut file = if let Some(template) = template {
        template.clone()
    } else {
        return Lm16Map16File::from_complete_core(&combined, &acts_like)
            .map_err(|error| error.to_string());
    };
    file.replace_complete_core(&combined, &acts_like)
        .map_err(|error| error.to_string())?;
    Ok(file)
}

pub(super) fn import_replacements(
    file: &Lm16Map16File,
    current: &Map16Set,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    if current.pages.len() != lm_app::SMW_COMPLETE_MAP16_PAGES {
        return Err("complete Map16 import requires the 256-page SMW workspace".into());
    }
    for (page, value) in current.pages.iter().enumerate() {
        if value.tiles.len() != Map16Page::TILE_COUNT {
            return Err(format!(
                "complete SMW Map16 page {page:02X} has {} tiles",
                value.tiles.len()
            ));
        }
    }
    let combined = file.section(Lm16Map16SectionKind::CombinedTiles);
    let acts_like = file.section(Lm16Map16SectionKind::ActsLike);
    if combined.len() != Lm16Map16File::COMBINED_TILES_LEN
        || acts_like.len() != Lm16Map16File::ACTS_LIKE_LEN
    {
        return Err("selected .map16 file does not contain a complete semantic core".into());
    }
    let mut replacements = Vec::with_capacity(Lm16Map16File::TILE_COUNT);
    for global_tile in 0..Lm16Map16File::TILE_COUNT {
        let page = global_tile / Map16Page::TILE_COUNT;
        let tile = global_tile % Map16Page::TILE_COUNT;
        let words = if global_tile < PROTECTED_FOREGROUND_TILES {
            tile_words(current.pages[page].tiles[tile])
        } else {
            let offset = global_tile * Lm16Map16File::TILE_BYTES;
            decode_tile_words(&combined[offset..offset + Lm16Map16File::TILE_BYTES])
        };
        let acts_like = if page < lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
            let offset = global_tile * 2;
            u16::from_le_bytes([acts_like[offset], acts_like[offset + 1]])
        } else {
            0
        };
        replacements.push((
            Map16Address { page, tile },
            Map16Tile {
                top_left: Subtile(words[0]),
                top_right: Subtile(words[1]),
                bottom_left: Subtile(words[2]),
                bottom_right: Subtile(words[3]),
                acts_like,
            },
        ));
    }
    Ok(replacements)
}

fn tile_words(tile: Map16Tile) -> [u16; 4] {
    [
        tile.top_left.0,
        tile.top_right.0,
        tile.bottom_left.0,
        tile.bottom_right.0,
    ]
}

fn decode_tile_words(bytes: &[u8]) -> [u16; 4] {
    std::array::from_fn(|word| {
        let offset = word * 2;
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    fn complete_set() -> Map16Set {
        Map16Set {
            pages: (0..lm_app::SMW_COMPLETE_MAP16_PAGES)
                .map(|page| Map16Page {
                    tiles: (0..Map16Page::TILE_COUNT)
                        .map(|tile| Map16Tile {
                            top_left: Subtile(u16::try_from(page).unwrap()),
                            top_right: Subtile(u16::try_from(tile).unwrap()),
                            bottom_left: Subtile(2),
                            bottom_right: Subtile(3),
                            acts_like: u16::try_from(page * Map16Page::TILE_COUNT + tile)
                                .unwrap_or(0),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn complete_file_round_trip_preserves_domains_and_protected_definitions() {
        let set = complete_set();
        let file = export_file(&set, None).unwrap();
        assert_eq!(
            file.section(Lm16Map16SectionKind::CombinedTiles)[..0x1000]
                .iter()
                .filter(|byte| **byte != 0)
                .count(),
            0
        );
        let replacements = import_replacements(&file, &set).unwrap();
        assert_eq!(replacements.len(), Lm16Map16File::TILE_COUNT);
        assert_eq!(replacements[0].1, set.pages[0].tiles[0]);
        let foreground = 0x2345;
        assert_eq!(
            replacements[foreground].1,
            set.pages[foreground / 0x100].tiles[foreground % 0x100]
        );
        let background = 0x9234;
        let expected = set.pages[background / 0x100].tiles[background % 0x100];
        assert_eq!(tile_words(replacements[background].1), tile_words(expected));
        assert_eq!(replacements[background].1.acts_like, 0);
    }

    #[test]
    fn template_export_preserves_every_unrelated_section_byte() {
        let set = complete_set();
        let canonical = export_file(&set, None).unwrap();
        let templated = export_file(&set, Some(&canonical)).unwrap();
        assert_eq!(templated.encode(), canonical.encode());
    }

    #[test]
    fn malformed_current_page_shape_is_rejected_before_indexing() {
        let mut set = complete_set();
        set.pages[0].tiles.pop();
        let file = export_file(&complete_set(), None).unwrap();
        assert_eq!(
            import_replacements(&file, &set).unwrap_err(),
            "complete SMW Map16 page 00 has 255 tiles"
        );
    }

    #[test]
    fn genuine_lunar_magic_export_round_trips_byte_exactly_through_gui_helpers() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/map16/all.map16");
        let bytes = fs::read(path).unwrap();
        let source = Lm16Map16File::decode(&bytes).unwrap();
        let mut set = complete_set();
        for (address, tile) in import_replacements(&source, &set).unwrap() {
            set.pages[address.page].tiles[address.tile] = tile;
        }
        let exported = export_file(&set, Some(&source)).unwrap();
        assert_eq!(exported.encode(), bytes);
    }

    #[test]
    fn complete_import_is_bound_to_the_revision_that_started_loading() {
        assert!(validate_import_revision(41, 41).is_ok());
        assert_eq!(
            validate_import_revision(42, 41).unwrap_err(),
            "the ROM changed while the complete Map16 file was loading"
        );
    }
}
