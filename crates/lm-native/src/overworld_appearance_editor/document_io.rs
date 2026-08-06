use super::OverworldAppearanceEditor;
use crate::{
    dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use lm_app::OverworldAppearanceDocumentController;
use lm_level::S16OvSidecar;
use lm_overworld::SpriteAppearanceFile;

impl OverworldAppearanceEditor {
    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_overworld_appearance_document() else {
            return;
        };
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sscov"))
        {
            self.start_native_pair_load(path);
            return;
        }
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(SpriteAppearanceFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "overworld appearance document",
        )]) {
            self.error = Some(error);
        } else {
            self.pending_load = Some(super::PendingAppearanceLoad::PortableOpen);
        }
    }

    pub(super) fn import_native_pair(&mut self) {
        if self
            .controller
            .as_ref()
            .is_some_and(OverworldAppearanceDocumentController::is_modified)
        {
            self.error = Some(
                "save or discard portable appearance changes before opening a native pair".into(),
            );
            return;
        }
        let Some(path) = dialogs::choose_native_overworld_sprite_sidecar() else {
            return;
        };
        self.start_native_pair_load(path);
    }

    fn start_native_pair_load(&mut self, path: std::path::PathBuf) {
        let map16_path = path.with_extension("s16ov");
        if let Err(error) = self.loader.start(vec![
            BoundedRead::new(
                path,
                lm_overworld::SSCOV_MAX_BYTES as u64,
                "native .sscov appearance definitions",
            ),
            BoundedRead::new(
                map16_path,
                S16OvSidecar::CAPACITY as u64,
                "native .s16ov Sprite Map16 definitions",
            ),
        ]) {
            self.error = Some(error);
        } else {
            self.pending_load = Some(super::PendingAppearanceLoad::NativeImport);
        }
    }

    pub(super) fn export_native_pair(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let Some(path) = dialogs::choose_native_overworld_sprite_sidecar_save_path() else {
            return;
        };
        let native = match lm_render::export_native_overworld_appearances(controller.value()) {
            Ok(native) => native,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let definitions = match native.definitions.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if let Err(error) = self.native_persistence.start_create_pair(
            controller.revision(),
            path.clone(),
            definitions,
            path.with_extension("s16ov"),
            native.sprite_map16.encode(),
        ) {
            self.error = Some(error);
        }
    }
}

pub(super) fn decode(
    mut loaded: LoadedDocument,
) -> Result<OverworldAppearanceDocumentController, String> {
    let (path, bytes) = loaded
        .files
        .pop()
        .ok_or_else(|| "overworld-appearance loader returned no file".to_string())?;
    OverworldAppearanceDocumentController::decode(path, &bytes).map_err(|error| error.to_string())
}

pub(super) fn decode_native_pair(
    loaded: LoadedDocument,
) -> Result<lm_app::NativeOverworldAppearanceController, String> {
    let [(definitions_path, definitions), (map16_path, map16)] =
        loaded.into_exact::<2>("native overworld sprite sidecar")?;
    lm_app::NativeOverworldAppearanceController::decode(
        definitions_path,
        map16_path,
        &definitions,
        &map16,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};
    use std::path::PathBuf;

    fn portable() -> SpriteAppearanceFile {
        SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x101,
                parts: [(1, 0, 0), (2, 8, 0), (3, 0, 8), (4, 8, 8)]
                    .into_iter()
                    .map(|(tile_index, x_offset, y_offset)| SpriteAppearancePart {
                        tile_index,
                        palette_index: 2,
                        x_offset,
                        y_offset,
                        x_flip: false,
                        y_flip: false,
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn generated_native_pair_loads_back_to_the_exact_portable_value() {
        let portable = portable();
        let native = lm_render::export_native_overworld_appearances(&portable).unwrap();
        let loaded = LoadedDocument {
            files: vec![
                (
                    PathBuf::from("sprites.sscov"),
                    native.definitions.encode().unwrap(),
                ),
                (PathBuf::from("sprites.s16ov"), native.sprite_map16.encode()),
            ],
        };
        let controller = decode_native_pair(loaded).unwrap();
        let converted = lm_render::import_native_overworld_appearances(
            &controller.value().definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &controller.value().sprite_map16,
        )
        .unwrap();
        assert_eq!(converted, portable);
    }

    #[test]
    fn original_pair_can_resolve_lunar_magics_builtin_sprite_map16_page() {
        let loaded = LoadedDocument {
            files: vec![
                (PathBuf::from("original.sscov"), b"01\t2\t0,0,1\n".to_vec()),
                (PathBuf::from("original.s16ov"), Vec::new()),
            ],
        };
        let native = decode_native_pair(loaded).unwrap();
        let imported = lm_render::import_native_overworld_appearances(
            &native.value().definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &native.value().sprite_map16,
        )
        .unwrap();
        let parts = &imported.definition(1).unwrap().parts;
        assert_eq!(parts.len(), 4);
        assert_eq!(
            parts.iter().map(|part| part.tile_index).collect::<Vec<_>>(),
            [0x26, 0x36, 0x27, 0x37]
        );
        assert_eq!(
            parts
                .iter()
                .map(|part| (part.x_offset, part.y_offset, part.palette_index))
                .collect::<Vec<_>>(),
            [(0, 0, 1), (8, 0, 1), (0, 8, 1), (8, 8, 1)]
        );
    }

    #[test]
    fn native_pair_requires_both_files_in_request_order() {
        let error = decode_native_pair(LoadedDocument {
            files: vec![(PathBuf::from("sprites.sscov"), Vec::new())],
        })
        .unwrap_err();
        assert!(error.contains("expected 2, got 1"));
    }
}
