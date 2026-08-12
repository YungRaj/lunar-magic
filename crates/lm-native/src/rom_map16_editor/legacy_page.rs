use super::{Map16ControllerEdit, RomMap16Editor, text};
use crate::{
    dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};
use lm_level::{Map16Address, Map16Page, Map16Tile};
use std::path::{Path, PathBuf};

const GRAPHICS_LEN: usize = Map16Page::TILE_COUNT * Map16Tile::GRAPHICS_LEN;
const ACTS_LIKE_LEN: usize = Map16Page::TILE_COUNT * 2;
const FIRST_EDITABLE_PAGE: usize = 2;
const FOREGROUND_PAGE_LIMIT: usize = lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES;
const COMPLETE_PAGE_LIMIT: usize = lm_app::SMW_COMPLETE_MAP16_PAGES;
const FOREGROUND_DEFINITIONS_LEN: usize = FOREGROUND_PAGE_LIMIT * GRAPHICS_LEN;
const FOREGROUND_ACTS_LIKE_LEN: usize = FOREGROUND_PAGE_LIMIT * ACTS_LIKE_LEN;
const BACKGROUND_DEFINITIONS_LEN: usize =
    (COMPLETE_PAGE_LIMIT - FOREGROUND_PAGE_LIMIT) * GRAPHICS_LEN;

pub(super) enum PendingLegacyImport {
    Page { revision: u64, page: usize },
    Foreground { revision: u64 },
    Background { revision: u64 },
}

impl RomMap16Editor {
    pub(super) fn poll_legacy_page_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.legacy_page_loader.show(context) {
            let pending = self.pending_legacy_import.take();
            let result = result.and_then(|loaded| {
                let pending = pending.ok_or("legacy Map16 import request is missing")?;
                let (revision, replacements) =
                    match pending {
                        PendingLegacyImport::Page { revision, page } => {
                            let (definitions, acts_like) =
                                loaded_legacy_pair(loaded, "legacy Map16 page pair")?;
                            let workspace =
                                self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                            let current =
                                workspace.controller.set().pages.get(page).ok_or_else(|| {
                                    format!("Map16 page {page:02X} is unavailable")
                                })?;
                            let imported = overlay_legacy_page(&definitions, &acts_like, current)?;
                            (revision, page_replacements(page, imported)?)
                        }
                        PendingLegacyImport::Foreground { revision } => {
                            let (definitions, acts_like) =
                                loaded_legacy_pair(loaded, "legacy foreground Map16 pair")?;
                            let workspace =
                                self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                            (
                                revision,
                                decode_legacy_foreground(
                                    &definitions,
                                    &acts_like,
                                    workspace.controller.set(),
                                )?,
                            )
                        }
                        PendingLegacyImport::Background { revision } => {
                            let [(_, definitions)] =
                                loaded.into_exact::<1>("legacy background Map16 file")?;
                            let workspace =
                                self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                            (
                                revision,
                                decode_legacy_background(&definitions, workspace.controller.set())?,
                            )
                        }
                    };
                let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while legacy Map16 data was loading".into());
                }
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
        catalog: Option<&LocalizationCatalog>,
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
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16TransferImportPage)),
                )
                .clicked()
                && let Some(definitions_path) = dialogs::choose_legacy_map16_page_document()
            {
                let acts_like_path = g_sibling(&definitions_path);
                let requests = vec![
                    BoundedRead::prefix(
                        definitions_path,
                        GRAPHICS_LEN as u64,
                        "Map16Page.bin definitions",
                    ),
                    BoundedRead::optional_prefix(
                        acts_like_path,
                        ACTS_LIKE_LEN as u64,
                        "Map16PageG.bin Acts Like",
                    ),
                ];
                match self.legacy_page_loader.start(requests) {
                    Ok(()) => {
                        self.pending_legacy_import = Some(PendingLegacyImport::Page {
                            revision: project_revision,
                            page: self.page,
                        })
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    supported && !stale && !busy,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16TransferExportPage)),
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
            ui.small(text(catalog, ExtendedUiTextKey::RomMap16TransferPageNotice));
        } else {
            ui.small(text(
                catalog,
                ExtendedUiTextKey::RomMap16TransferPageUnsupportedNotice,
            ));
        }
        ui.horizontal_wrapped(|ui| {
            let enabled = !stale && !busy && self.workspace.is_some();
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::RomMap16TransferImportForeground,
                    )),
                )
                .clicked()
                && let Some(definitions_path) = dialogs::choose_legacy_map16_foreground_document()
            {
                let acts_like_path = g_sibling(&definitions_path);
                let requests = vec![
                    BoundedRead::prefix(
                        definitions_path,
                        FOREGROUND_DEFINITIONS_LEN as u64,
                        "Map16FG.bin definitions",
                    ),
                    BoundedRead::optional_prefix(
                        acts_like_path,
                        FOREGROUND_ACTS_LIKE_LEN as u64,
                        "Map16FGG.bin Acts Like",
                    ),
                ];
                match self.legacy_page_loader.start(requests) {
                    Ok(()) => {
                        self.pending_legacy_import = Some(PendingLegacyImport::Foreground {
                            revision: project_revision,
                        })
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::RomMap16TransferExportForeground,
                    )),
                )
                .clicked()
                && let Some(definitions_path) = dialogs::choose_legacy_map16_foreground_save_path()
            {
                let result = self
                    .workspace
                    .as_ref()
                    .ok_or_else(|| "Map16 workspace is closed".to_owned())
                    .and_then(|workspace| encode_legacy_foreground(workspace.controller.set()));
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
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::RomMap16TransferImportBackground,
                    )),
                )
                .clicked()
                && let Some(path) = dialogs::choose_legacy_map16_background_document()
            {
                match self.legacy_page_loader.start(vec![BoundedRead::prefix(
                    path,
                    BACKGROUND_DEFINITIONS_LEN as u64,
                    "Map16BG.bin definitions",
                )]) {
                    Ok(()) => {
                        self.pending_legacy_import = Some(PendingLegacyImport::Background {
                            revision: project_revision,
                        })
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(text(
                        catalog,
                        ExtendedUiTextKey::RomMap16TransferExportBackground,
                    )),
                )
                .clicked()
                && let Some(path) = dialogs::choose_legacy_map16_background_save_path()
            {
                let result = self
                    .workspace
                    .as_ref()
                    .ok_or_else(|| "Map16 workspace is closed".to_owned())
                    .and_then(|workspace| encode_legacy_background(workspace.controller.set()))
                    .and_then(|definitions| {
                        self.legacy_page_persistence.start(
                            project_revision,
                            crate::persistence_worker::PersistenceTarget::Create(path),
                            definitions,
                        )
                    });
                if let Err(error) = result {
                    self.error = Some(error);
                }
            }
        });
        ui.small(text(
            catalog,
            ExtendedUiTextKey::RomMap16TransferLegacyCompleteNotice,
        ));
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

fn loaded_legacy_pair(
    loaded: LoadedDocument,
    description: &str,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut files = loaded.files.into_iter();
    let (_, definitions) = files
        .next()
        .ok_or_else(|| format!("{description} is missing its definition file"))?;
    let acts_like = files.next().map(|(_, bytes)| bytes).unwrap_or_default();
    if files.next().is_some() {
        return Err(format!("{description} loader returned too many files"));
    }
    Ok((definitions, acts_like))
}

fn overlay_legacy_page(
    definitions: &[u8],
    acts_like: &[u8],
    current: &Map16Page,
) -> Result<Map16Page, String> {
    if definitions.len() > GRAPHICS_LEN || acts_like.len() > ACTS_LIKE_LEN {
        return Err(format!(
            "legacy Map16 page prefixes exceed {GRAPHICS_LEN:#x} definition or {ACTS_LIKE_LEN:#x} Acts-Like bytes"
        ));
    }
    let (mut merged_definitions, mut merged_acts_like) = encode_legacy_page(current)?;
    merged_definitions[..definitions.len()].copy_from_slice(definitions);
    merged_acts_like[..acts_like.len()].copy_from_slice(acts_like);
    decode_legacy_page(&merged_definitions, &merged_acts_like)
}

fn encode_legacy_foreground(set: &lm_level::Map16Set) -> Result<(Vec<u8>, Vec<u8>), String> {
    validate_complete_set(set)?;
    let mut definitions = Vec::with_capacity(FOREGROUND_DEFINITIONS_LEN);
    let mut acts_like = Vec::with_capacity(FOREGROUND_ACTS_LIKE_LEN);
    for page in &set.pages[..FOREGROUND_PAGE_LIMIT] {
        let (page_definitions, page_acts_like) = encode_legacy_page(page)?;
        definitions.extend_from_slice(&page_definitions);
        acts_like.extend_from_slice(&page_acts_like);
    }
    Ok((definitions, acts_like))
}

fn decode_legacy_foreground(
    definitions: &[u8],
    acts_like: &[u8],
    current: &lm_level::Map16Set,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    validate_complete_set(current)?;
    if definitions.len() > FOREGROUND_DEFINITIONS_LEN || acts_like.len() > FOREGROUND_ACTS_LIKE_LEN
    {
        return Err(format!(
            "legacy foreground Map16 prefixes exceed {FOREGROUND_DEFINITIONS_LEN:#x} definition or {FOREGROUND_ACTS_LIKE_LEN:#x} Acts-Like bytes, got {:#x} and {:#x}",
            definitions.len(),
            acts_like.len()
        ));
    }
    let (mut merged_definitions, mut merged_acts_like) = encode_legacy_foreground(current)?;
    merged_definitions[..definitions.len()].copy_from_slice(definitions);
    merged_acts_like[..acts_like.len()].copy_from_slice(acts_like);
    let mut replacements = Vec::with_capacity(FOREGROUND_PAGE_LIMIT * Map16Page::TILE_COUNT);
    for page in 0..FOREGROUND_PAGE_LIMIT {
        let definition_offset = page * GRAPHICS_LEN;
        let acts_like_offset = page * ACTS_LIKE_LEN;
        let imported = decode_legacy_page(
            &merged_definitions[definition_offset..definition_offset + GRAPHICS_LEN],
            &merged_acts_like[acts_like_offset..acts_like_offset + ACTS_LIKE_LEN],
        )?;
        for (tile, mut value) in imported.tiles.into_iter().enumerate() {
            if page < FIRST_EDITABLE_PAGE {
                let retained = current.pages[page].tiles[tile];
                value.top_left = retained.top_left;
                value.top_right = retained.top_right;
                value.bottom_left = retained.bottom_left;
                value.bottom_right = retained.bottom_right;
            }
            replacements.push((Map16Address { page, tile }, value));
        }
    }
    Ok(replacements)
}

fn encode_legacy_background(set: &lm_level::Map16Set) -> Result<Vec<u8>, String> {
    validate_complete_set(set)?;
    let mut definitions = Vec::with_capacity(BACKGROUND_DEFINITIONS_LEN);
    for page in &set.pages[FOREGROUND_PAGE_LIMIT..COMPLETE_PAGE_LIMIT] {
        let (page_definitions, _) = encode_legacy_page(page)?;
        definitions.extend_from_slice(&page_definitions);
    }
    Ok(definitions)
}

fn decode_legacy_background(
    definitions: &[u8],
    current: &lm_level::Map16Set,
) -> Result<Vec<(Map16Address, Map16Tile)>, String> {
    if definitions.len() > BACKGROUND_DEFINITIONS_LEN {
        return Err(format!(
            "legacy background Map16 prefix exceeds {BACKGROUND_DEFINITIONS_LEN:#x} definition bytes, got {:#x}",
            definitions.len()
        ));
    }
    let mut merged_definitions = encode_legacy_background(current)?;
    merged_definitions[..definitions.len()].copy_from_slice(definitions);
    let blank_acts_like = vec![0; ACTS_LIKE_LEN];
    let mut replacements =
        Vec::with_capacity((COMPLETE_PAGE_LIMIT - FOREGROUND_PAGE_LIMIT) * Map16Page::TILE_COUNT);
    for relative_page in 0..(COMPLETE_PAGE_LIMIT - FOREGROUND_PAGE_LIMIT) {
        let definition_offset = relative_page * GRAPHICS_LEN;
        let imported = decode_legacy_page(
            &merged_definitions[definition_offset..definition_offset + GRAPHICS_LEN],
            &blank_acts_like,
        )?;
        let page = FOREGROUND_PAGE_LIMIT + relative_page;
        replacements.extend(
            imported
                .tiles
                .into_iter()
                .enumerate()
                .map(|(tile, value)| (Map16Address { page, tile }, value)),
        );
    }
    Ok(replacements)
}

fn validate_complete_set(set: &lm_level::Map16Set) -> Result<(), String> {
    if set.pages.len() != COMPLETE_PAGE_LIMIT {
        return Err(format!(
            "legacy complete Map16 transfer requires {COMPLETE_PAGE_LIMIT} pages, got {}",
            set.pages.len()
        ));
    }
    for (page, value) in set.pages.iter().enumerate() {
        if value.tiles.len() != Map16Page::TILE_COUNT {
            return Err(format!(
                "legacy complete Map16 page {page:02X} has {} tiles",
                value.tiles.len()
            ));
        }
    }
    Ok(())
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

    fn complete_set() -> Map16Set {
        Map16Set {
            pages: (0..COMPLETE_PAGE_LIMIT)
                .map(|page_index| {
                    let mut value = page();
                    value.tiles[0].top_left = Subtile(u16::try_from(page_index).unwrap());
                    value.tiles[0].acts_like = u16::try_from(0x100 + page_index).unwrap();
                    value
                })
                .collect(),
        }
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

    #[test]
    fn legacy_complete_planes_round_trip_exact_original_namespaces_and_lengths() {
        let set = complete_set();
        let (foreground_definitions, foreground_acts_like) =
            encode_legacy_foreground(&set).unwrap();
        let background_definitions = encode_legacy_background(&set).unwrap();
        assert_eq!(foreground_definitions.len(), 0x40000);
        assert_eq!(foreground_acts_like.len(), 0x10000);
        assert_eq!(background_definitions.len(), 0x40000);

        let foreground =
            decode_legacy_foreground(&foreground_definitions, &foreground_acts_like, &set).unwrap();
        assert_eq!(
            foreground.len(),
            FOREGROUND_PAGE_LIMIT * Map16Page::TILE_COUNT
        );
        assert_eq!(foreground[0].0, Map16Address { page: 0, tile: 0 });
        assert_eq!(
            foreground.last().unwrap().0,
            Map16Address {
                page: 0x7f,
                tile: 0xff
            }
        );
        assert_eq!(foreground[0].1.top_left, set.pages[0].tiles[0].top_left);
        assert_eq!(foreground[0].1.acts_like, set.pages[0].tiles[0].acts_like);

        let background = decode_legacy_background(&background_definitions, &set).unwrap();
        assert_eq!(
            background.len(),
            (COMPLETE_PAGE_LIMIT - FOREGROUND_PAGE_LIMIT) * Map16Page::TILE_COUNT
        );
        assert_eq!(
            background[0].0,
            Map16Address {
                page: 0x80,
                tile: 0
            }
        );
        assert_eq!(background[0].1.top_left, Subtile(0x80));
        assert_eq!(background[0].1.acts_like, 0);
        assert_eq!(
            background.last().unwrap().0,
            Map16Address {
                page: 0xff,
                tile: 0xff
            }
        );
    }

    #[test]
    fn foreground_import_preserves_protected_definition_words_but_imports_acts_like() {
        let current = complete_set();
        let mut imported = complete_set();
        imported.pages[0].tiles[0].top_left = Subtile(0x7777);
        imported.pages[0].tiles[0].acts_like = 0x3456;
        imported.pages[2].tiles[0].top_left = Subtile(0x2222);
        let (definitions, acts_like) = encode_legacy_foreground(&imported).unwrap();
        let replacements = decode_legacy_foreground(&definitions, &acts_like, &current).unwrap();
        assert_eq!(
            replacements[0].1.top_left,
            current.pages[0].tiles[0].top_left
        );
        assert_eq!(replacements[0].1.acts_like, 0x3456);
        assert_eq!(
            replacements[2 * Map16Page::TILE_COUNT].1.top_left,
            Subtile(0x2222)
        );
        assert!(
            decode_legacy_foreground(
                &vec![0; FOREGROUND_DEFINITIONS_LEN + 1],
                &acts_like,
                &current,
            )
            .is_err()
        );
        assert!(
            decode_legacy_background(&vec![0; BACKGROUND_DEFINITIONS_LEN + 1], &current,).is_err()
        );
    }

    #[test]
    fn legacy_prefix_reads_overlay_short_planes_and_missing_g_companions() {
        let current = complete_set();
        let mut page_definitions = 0x7777_u16.to_le_bytes().to_vec();
        page_definitions.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        let overlaid = overlay_legacy_page(&page_definitions, &[], &current.pages[2]).unwrap();
        assert_eq!(overlaid.tiles[0].top_left, Subtile(0x7777));
        assert_eq!(
            overlaid.tiles[0].acts_like,
            current.pages[2].tiles[0].acts_like
        );
        assert_eq!(overlaid.tiles[1], current.pages[2].tiles[1]);

        let mut foreground_prefix = vec![0; 2 * GRAPHICS_LEN];
        foreground_prefix.extend_from_slice(&0x2222_u16.to_le_bytes());
        let foreground = decode_legacy_foreground(&foreground_prefix, &[], &current).unwrap();
        assert_eq!(
            foreground[2 * Map16Page::TILE_COUNT].1.top_left,
            Subtile(0x2222)
        );
        assert_eq!(
            foreground[2 * Map16Page::TILE_COUNT].1.acts_like,
            current.pages[2].tiles[0].acts_like
        );

        let background = decode_legacy_background(&0x8888_u16.to_le_bytes(), &current).unwrap();
        assert_eq!(background[0].1.top_left, Subtile(0x8888));
        assert_eq!(
            background[1].1,
            Map16Tile {
                acts_like: 0,
                ..current.pages[0x80].tiles[1]
            }
        );

        let loaded = LoadedDocument {
            files: vec![(PathBuf::from("Map16Page.bin"), vec![1, 2])],
        };
        assert_eq!(
            loaded_legacy_pair(loaded, "page").unwrap(),
            (vec![1, 2], Vec::new())
        );
    }
}
