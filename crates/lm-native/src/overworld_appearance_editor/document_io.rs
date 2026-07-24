use super::OverworldAppearanceEditor;
use crate::{
    dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use lm_app::OverworldAppearanceDocumentController;
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
