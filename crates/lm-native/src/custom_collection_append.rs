use crate::vanilla_level_editor::CustomCollectionSelection;
use eframe::egui;
use lm_level::{
    CustomObjectEntry, CustomObjectLibrary, CustomSpriteEntry, CustomSpriteLibrary,
    DescriptionFormat, SpriteLengthTable,
};
use std::path::Path;

const FAILURE_STATUS: &str = "Nothing selected or couldn't open file.";
const OBJECT_SUCCESS_STATUS: &str = "Saved selected objects to Custom Collection of Objects file.";
const SPRITE_SUCCESS_STATUS: &str = "Saved selected sprites to Custom Collection of Sprites file.";

#[derive(Clone, Debug)]
pub(crate) struct CustomCollectionAppendDialog {
    open: bool,
    selection: Option<CustomCollectionSelection>,
    description: String,
}

impl Default for CustomCollectionAppendDialog {
    fn default() -> Self {
        Self {
            open: false,
            selection: None,
            description: String::new(),
        }
    }
}

impl CustomCollectionAppendDialog {
    pub(crate) fn open(&mut self, selection: CustomCollectionSelection) {
        self.selection = Some(selection);
        self.description.clear();
        self.open = true;
    }

    fn cancel(&mut self) {
        self.open = false;
        self.selection = None;
        self.description.clear();
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        rom_path: Option<&Path>,
    ) -> Option<String> {
        if !self.open {
            return None;
        }
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Add to Custom Collection")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Description");
                let response = ui.text_edit_singleline(&mut self.description);
                response.request_focus();
                ui.horizontal(|ui| {
                    save = ui.button("Save").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.cancel();
            return None;
        }
        if !save {
            return None;
        }
        self.open = false;
        let Some(selection) = self.selection.take() else {
            return Some(FAILURE_STATUS.into());
        };
        let Some(rom_path) = rom_path else {
            return Some(FAILURE_STATUS.into());
        };
        Some(
            append_selection(rom_path, selection, &self.description)
                .unwrap_or_else(|_| FAILURE_STATUS.into()),
        )
    }
}

fn append_selection(
    rom_path: &Path,
    selection: CustomCollectionSelection,
    description: &str,
) -> Result<String, String> {
    let description = if description.is_empty() {
        "(not specified)".to_owned()
    } else {
        description.to_owned()
    };
    match selection {
        CustomCollectionSelection::Objects(objects) => {
            let data_path = rom_path.with_extension("mw0");
            let text_path = rom_path.with_extension("mw0t");
            let mut library = load_object_library(&data_path, &text_path)?;
            force_trailing_line_ending_object(&mut library)?;
            library
                .push(
                    CustomObjectEntry::new_group(objects, description)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            let (data, text) = library.encode().map_err(|e| e.to_string())?;
            persist_pair(&data_path, &data, &text_path, &text)?;
            Ok(OBJECT_SUCCESS_STATUS.into())
        }
        CustomCollectionSelection::Sprites(mut sprites, lengths) => {
            let data_path = rom_path.with_extension("mw2");
            let text_path = rom_path.with_extension("mwt");
            if let Some(first) = sprites.first_mut() {
                first.encoded[0] |= 1;
            }
            for sprite in sprites.iter_mut().skip(1) {
                sprite.encoded[0] &= !1;
            }
            let mut library = load_sprite_library(&data_path, &text_path, &lengths)?;
            force_trailing_line_ending_sprite(&mut library)?;
            library
                .push(CustomSpriteEntry::new(sprites, description).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let (data, text) = library
                .encode_checked(&lengths)
                .map_err(|e| e.to_string())?;
            persist_pair(&data_path, &data, &text_path, &text)?;
            Ok(SPRITE_SUCCESS_STATUS.into())
        }
    }
}

fn load_pair(first: &Path, second: &Path) -> Result<Option<(Vec<u8>, Vec<u8>)>, String> {
    let first_exists = first.try_exists().map_err(|e| e.to_string())?;
    let second_exists = second.try_exists().map_err(|e| e.to_string())?;
    match (first_exists, second_exists) {
        (false, false) => Ok(None),
        (true, true) => Ok(Some((
            std::fs::read(first).map_err(|e| e.to_string())?,
            std::fs::read(second).map_err(|e| e.to_string())?,
        ))),
        _ => Err("custom collection sidecar pair is incomplete".into()),
    }
}

fn load_object_library(data: &Path, text: &Path) -> Result<CustomObjectLibrary, String> {
    load_pair(data, text)?.map_or_else(
        || Ok(CustomObjectLibrary::default()),
        |(data, text)| CustomObjectLibrary::decode(&data, &text).map_err(|e| e.to_string()),
    )
}

fn load_sprite_library(
    data: &Path,
    text: &Path,
    lengths: &SpriteLengthTable,
) -> Result<CustomSpriteLibrary, String> {
    load_pair(data, text)?.map_or_else(
        || CustomSpriteLibrary::decode(&[0, 0xff], b"", lengths).map_err(|e| e.to_string()),
        |(data, text)| {
            CustomSpriteLibrary::decode(&data, &text, lengths).map_err(|e| e.to_string())
        },
    )
}

fn trailing(format: DescriptionFormat) -> DescriptionFormat {
    DescriptionFormat {
        trailing_line_ending: true,
        ..format
    }
}

fn force_trailing_line_ending_object(library: &mut CustomObjectLibrary) -> Result<(), String> {
    library
        .set_description_format(trailing(library.description_format()))
        .map_err(|e| e.to_string())
}

fn force_trailing_line_ending_sprite(library: &mut CustomSpriteLibrary) -> Result<(), String> {
    library
        .set_description_format(trailing(library.description_format()))
        .map_err(|e| e.to_string())
}

fn persist_pair(
    first: &Path,
    first_bytes: &[u8],
    second: &Path,
    second_bytes: &[u8],
) -> Result<(), String> {
    if first.exists() && second.exists() {
        lm_app::file_persistence::replace_existing_pair(
            (first, first_bytes),
            (second, second_bytes),
        )
    } else {
        lm_app::file_persistence::write_new_group(&[(first, first_bytes), (second, second_bytes)])
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{ObjectRecord, SpriteRecord};
    use std::path::PathBuf;

    fn temp_rom() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("vanilla.smc");
        std::fs::write(&rom, b"rom").unwrap();
        (directory, rom)
    }

    #[test]
    fn object_selection_creates_then_appends_synchronized_pair() {
        let (_directory, rom) = temp_rom();
        let object = || ObjectRecord::new(vec![1, 0, 3]).unwrap();
        assert_eq!(
            append_selection(&rom, CustomCollectionSelection::Objects(vec![object()]), ""),
            Ok(OBJECT_SUCCESS_STATUS.into())
        );
        append_selection(
            &rom,
            CustomCollectionSelection::Objects(vec![object()]),
            "second",
        )
        .unwrap();
        let library = CustomObjectLibrary::decode(
            &std::fs::read(rom.with_extension("mw0")).unwrap(),
            &std::fs::read(rom.with_extension("mw0t")).unwrap(),
        )
        .unwrap();
        assert_eq!(library.entries().len(), 2);
        assert_eq!(library.entries()[0].description, "(not specified)");
        assert_eq!(library.entries()[1].description, "second");
        assert!(library.description_format().trailing_line_ending);
    }

    #[test]
    fn sprite_selection_creates_a_decodable_pair() {
        let (_directory, rom) = temp_rom();
        let sprite = SpriteRecord {
            encoded: vec![1, 0, 0x15],
        };
        assert_eq!(
            append_selection(
                &rom,
                CustomCollectionSelection::Sprites(vec![sprite], SpriteLengthTable::standard()),
                "koopa"
            ),
            Ok(SPRITE_SUCCESS_STATUS.into())
        );
        append_selection(
            &rom,
            CustomCollectionSelection::Sprites(
                vec![SpriteRecord {
                    encoded: vec![0, 0x10, 0x15],
                }],
                SpriteLengthTable::standard(),
            ),
            "second",
        )
        .unwrap();
        let library = CustomSpriteLibrary::decode(
            &std::fs::read(rom.with_extension("mw2")).unwrap(),
            &std::fs::read(rom.with_extension("mwt")).unwrap(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        assert_eq!(library.entries().len(), 2);
        assert_eq!(library.entries()[0].description, "koopa");
        assert_eq!(library.entries()[1].description, "second");
        assert_ne!(library.entries()[0].sprites[0].encoded[0] & 1, 0);
        assert_ne!(library.entries()[1].sprites[0].encoded[0] & 1, 0);
    }

    #[test]
    fn incomplete_pair_fails_without_modifying_the_existing_member() {
        let (_directory, rom) = temp_rom();
        let data = rom.with_extension("mw0");
        std::fs::write(&data, b"sentinel").unwrap();
        let result = append_selection(
            &rom,
            CustomCollectionSelection::Objects(vec![ObjectRecord::new(vec![1, 0, 3]).unwrap()]),
            "x",
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(data).unwrap(), b"sentinel");
        assert!(!rom.with_extension("mw0t").exists());
    }

    #[test]
    fn malformed_existing_pair_and_cancel_publish_nothing() {
        let (_directory, rom) = temp_rom();
        let data = rom.with_extension("mw0");
        let text = rom.with_extension("mw0t");
        std::fs::write(&data, b"malformed").unwrap();
        std::fs::write(&text, b"old").unwrap();
        assert!(
            append_selection(
                &rom,
                CustomCollectionSelection::Objects(vec![ObjectRecord::new(vec![1, 0, 3]).unwrap()]),
                "new",
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&data).unwrap(), b"malformed");
        assert_eq!(std::fs::read(&text).unwrap(), b"old");

        let mut dialog = CustomCollectionAppendDialog::default();
        dialog.open(CustomCollectionSelection::Objects(vec![
            ObjectRecord::new(vec![1, 0, 3]).unwrap(),
        ]));
        dialog.description = "never written".into();
        dialog.cancel();
        assert!(!dialog.open);
        assert!(dialog.selection.is_none());
        assert!(dialog.description.is_empty());
        assert_eq!(std::fs::read(data).unwrap(), b"malformed");
        assert_eq!(std::fs::read(text).unwrap(), b"old");
    }
}
