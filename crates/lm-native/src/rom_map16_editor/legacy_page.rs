use super::{Map16ControllerEdit, RomMap16Editor};
use crate::{dialogs, document_loader::BoundedRead};
use eframe::egui;
use lm_level::{Map16Address, Map16Page, Map16Tile};
use std::path::{Path, PathBuf};

const GRAPHICS_LEN: usize = Map16Page::TILE_COUNT * Map16Tile::GRAPHICS_LEN;
const ACTS_LIKE_LEN: usize = Map16Page::TILE_COUNT * 2;
const FIRST_EDITABLE_PAGE: usize = 2;
const FOREGROUND_PAGE_LIMIT: usize = lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES;

impl RomMap16Editor {
    pub(super) fn poll_legacy_page_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.legacy_page_loader.show(context) {
            let pending = self.pending_legacy_page.take();
            let result = result.and_then(|loaded| {
                let (revision, page) = pending.ok_or("legacy Map16 page request is missing")?;
                let [(_, definitions), (_, acts_like)] =
                    loaded.into_exact::<2>("legacy Map16 page pair")?;
                let imported = decode_legacy_page(&definitions, &acts_like)?;
                let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the Map16 page was loading".into());
                }
                let replacements = page_replacements(page, imported)?;
                let resolution_limit =
                    workspace.controller.set().pages.len() * Map16Page::TILE_COUNT;
                self.apply_staged_edits(&[Map16ControllerEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                }])?;
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.legacy_page_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn legacy_page_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
    ) {
        let busy = self.complete_loader.is_running()
            || self.complete_persistence.is_running()
            || self.selected_loader.is_running()
            || self.selected_persistence.is_running()
            || self.legacy_page_loader.is_running()
            || self.legacy_page_persistence.is_running();
        let supported = (FIRST_EDITABLE_PAGE..FOREGROUND_PAGE_LIMIT).contains(&self.page);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    supported && !stale && !busy,
                    egui::Button::new("Import legacy page pair…"),
                )
                .clicked()
                && let Some(definitions_path) = dialogs::choose_legacy_map16_page_document()
            {
                let acts_like_path = g_sibling(&definitions_path);
                let requests = vec![
                    BoundedRead::new(
                        definitions_path,
                        GRAPHICS_LEN as u64,
                        "Map16Page.bin definitions",
                    ),
                    BoundedRead::new(
                        acts_like_path,
                        ACTS_LIKE_LEN as u64,
                        "Map16PageG.bin Acts Like",
                    ),
                ];
                match self.legacy_page_loader.start(requests) {
                    Ok(()) => self.pending_legacy_page = Some((project_revision, self.page)),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    supported && !stale && !busy,
                    egui::Button::new("Export legacy page pair…"),
                )
                .clicked()
                && let Some(definitions_path) = dialogs::choose_legacy_map16_page_save_path()
            {
                let result =
                    self.workspace
                        .as_ref()
                        .ok_or_else(|| "Map16 workspace is closed".to_owned())
                        .and_then(|workspace| {
                            let page = workspace.controller.set().pages.get(self.page).ok_or_else(
                                || format!("Map16 page {:02X} is unavailable", self.page),
                            )?;
                            encode_legacy_page(page)
                        });
                match result {
                    Ok((definitions, acts_like)) => {
                        let acts_like_path = g_sibling(&definitions_path);
                        if let Err(error) = self.legacy_page_persistence.start_create_pair(
                            project_revision,
                            definitions_path,
                            definitions,
                            acts_like_path,
                            acts_like,
                        ) {
                            self.error = Some(error);
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        });
        if supported {
            ui.small("Legacy transfer atomically reads or creates Map16Page.bin (definitions) and Map16PageG.bin (Acts Like) for the selected foreground page.");
        } else {
            ui.small("Legacy page pairs apply only to editable foreground pages 02–7F; built-in pages 00–01 and background pages use other Lunar Magic boundaries.");
        }
    }
}

fn g_sibling(definitions_path: &Path) -> PathBuf {
    let mut name = definitions_path.file_stem().map_or_else(
        || std::ffi::OsString::from("Map16Page"),
        std::ffi::OsString::from,
    );
    name.push("G.bin");
    let mut path = definitions_path.to_path_buf();
    path.set_file_name(name);
    path
}

fn decode_legacy_page(definitions: &[u8], acts_like: &[u8]) -> Result<Map16Page, String> {
    if definitions.len() != GRAPHICS_LEN || acts_like.len() != ACTS_LIKE_LEN {
        return Err(format!(
            "legacy Map16 page requires {GRAPHICS_LEN:#x} definition and {ACTS_LIKE_LEN:#x} Acts-Like bytes, got {:#x} and {:#x}",
            definitions.len(),
            acts_like.len()
        ));
    }
    Map16Page::decode(definitions, acts_like).map_err(|error| error.to_string())
}

fn encode_legacy_page(page: &Map16Page) -> Result<(Vec<u8>, Vec<u8>), String> {
    page.encode().map_err(|error| error.to_string())
}

fn page_replacements(
    page: usize,
    imported: Map16Page,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    if !(FIRST_EDITABLE_PAGE..FOREGROUND_PAGE_LIMIT).contains(&page) {
        return Err(format!(
            "legacy Map16 page target must be an editable foreground page 02–7F, got {page:02X}"
        ));
    }
    if imported.tiles.len() != Map16Page::TILE_COUNT {
        return Err(format!(
            "legacy Map16 page requires {} tiles, got {}",
            Map16Page::TILE_COUNT,
            imported.tiles.len()
        ));
    }
    Ok(imported
        .tiles
        .into_iter()
        .enumerate()
        .map(|(tile, value)| (Map16Address { page, tile }, value))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Map16Set, Subtile};

    fn page() -> Map16Page {
        Map16Page::new(
            (0..Map16Page::TILE_COUNT)
                .map(|tile| Map16Tile {
                    top_left: Subtile(u16::try_from(tile).unwrap()),
                    top_right: Subtile(0x1234),
                    bottom_left: Subtile(0x5678),
                    bottom_right: Subtile(0x9abc),
                    acts_like: u16::try_from(tile + 0x200).unwrap(),
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn legacy_pair_round_trips_exact_plane_shapes_and_order() {
        let expected = page();
        let (definitions, acts_like) = encode_legacy_page(&expected).unwrap();
        assert_eq!(acts_like.len(), ACTS_LIKE_LEN);
        assert_eq!(definitions.len(), GRAPHICS_LEN);
        assert_eq!(
            decode_legacy_page(&definitions, &acts_like).unwrap(),
            expected
        );
        assert_eq!(&definitions[..2], &0_u16.to_le_bytes());
        assert_eq!(&acts_like[..2], &0x0200_u16.to_le_bytes());
    }

    #[test]
    fn page_import_is_complete_targeted_and_rejects_protected_or_background_pages() {
        let replacements = page_replacements(2, page()).unwrap();
        assert_eq!(replacements.len(), Map16Page::TILE_COUNT);
        assert_eq!(replacements[0].0, Map16Address { page: 2, tile: 0 });
        assert_eq!(replacements[255].0, Map16Address { page: 2, tile: 255 });
        assert!(page_replacements(1, page()).is_err());
        assert!(page_replacements(FOREGROUND_PAGE_LIMIT, page()).is_err());

        let blank = Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap();
        let mut set = Map16Set {
            pages: vec![blank.clone(); FOREGROUND_PAGE_LIMIT],
        };
        set.replace_tiles(&replacements, FOREGROUND_PAGE_LIMIT * Map16Page::TILE_COUNT)
            .unwrap();
        assert_eq!(set.pages[1], blank);
        assert_eq!(set.pages[2], page());
        assert_eq!(set.pages[3], blank);
    }

    #[test]
    fn companion_path_matches_lunar_magic_g_suffix() {
        assert_eq!(
            g_sibling(Path::new("somewhere/Map16Page.bin")),
            Path::new("somewhere/Map16PageG.bin")
        );
        assert_eq!(
            g_sibling(Path::new("somewhere/custom.bin")),
            Path::new("somewhere/customG.bin")
        );
    }
}
