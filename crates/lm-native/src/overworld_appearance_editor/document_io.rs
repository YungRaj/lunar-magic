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
        let Some(path) = dialogs::choose_native_overworld_sprite_sidecar() else {
            return;
        };
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

    pub(super) fn replace_with_import(&mut self, value: SpriteAppearanceFile) {
        let Some(controller) = self.controller.as_mut() else {
            self.error =
                Some("open a portable appearance document before importing native sidecars".into());
            return;
        };
        let edits = replacement_edits(controller.value(), &value);
        match controller.apply_edits(controller.revision(), &edits) {
            Ok(()) => {
                self.clipboard_paste_target = None;
                self.invalidate();
            }
            Err(error) => self.error = Some(error.to_string()),
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

pub(super) fn decode_native_pair(loaded: LoadedDocument) -> Result<SpriteAppearanceFile, String> {
    let [(_, definitions), (_, map16)] =
        loaded.into_exact::<2>("native overworld sprite sidecar")?;
    let definitions = lm_overworld::NativeOverworldSpriteSidecar::decode(&definitions)
        .map_err(|error| error.to_string())?;
    let map16 = S16OvSidecar::decode(&map16).map_err(|error| error.to_string())?;
    // Exported native pairs own all referenced definitions in `.s16ov`. Original pairs that use
    // Lunar Magic's built-in `$000..$3FF` pages require a ROM-backed editor and fail explicitly.
    lm_render::import_native_overworld_appearances(&definitions, &[], &map16)
        .map_err(|error| error.to_string())
}

fn replacement_edits(
    current: &SpriteAppearanceFile,
    replacement: &SpriteAppearanceFile,
) -> Vec<lm_app::OverworldAppearanceDocumentEdit> {
    let mut edits = current
        .definitions
        .iter()
        .map(
            |definition| lm_app::OverworldAppearanceDocumentEdit::RemoveDefinition {
                sprite_id: definition.sprite_id,
            },
        )
        .collect::<Vec<_>>();
    for (index, definition) in replacement.definitions.iter().enumerate() {
        edits.push(lm_app::OverworldAppearanceDocumentEdit::InsertDefinition {
            index,
            sprite_id: definition.sprite_id,
        });
        edits.push(lm_app::OverworldAppearanceDocumentEdit::ReplaceParts {
            sprite_id: definition.sprite_id,
            values: definition.parts.clone(),
        });
    }
    edits
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
        assert_eq!(decode_native_pair(loaded).unwrap(), portable);
    }

    #[test]
    fn replacement_batch_is_complete_ordered_and_one_revision() {
        let current = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 7,
                parts: Vec::new(),
            }],
        };
        let mut controller = OverworldAppearanceDocumentController::decode(
            PathBuf::from("appearance.lmowapp"),
            &current.encode().unwrap(),
        )
        .unwrap();
        let replacement = portable();
        controller
            .apply_edits(
                controller.revision(),
                &replacement_edits(controller.value(), &replacement),
            )
            .unwrap();
        assert_eq!(controller.value(), &replacement);
        assert_eq!(controller.revision(), 1);
        assert!(controller.is_modified());
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.value(), &current);
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
