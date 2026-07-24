use super::{LevelAssets, PendingOpen, suggested_dimensions};
use crate::{
    dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use lm_app::CompleteLevelDocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{CompleteLevelFile, Map16SetFile};

pub(super) fn choose_requests() -> Option<Vec<BoundedRead>> {
    let path = dialogs::choose_complete_level_document()?;
    let map16_path = dialogs::choose_map16_set_document()?;
    let graphics_path = dialogs::choose_graphics_document()?;
    let palette_path = dialogs::choose_palette_document()?;
    Some(vec![
        BoundedRead::new(
            path,
            CompleteLevelFile::MAX_FILE_LEN as u64,
            "complete level",
        ),
        BoundedRead::new(map16_path, Map16SetFile::MAX_FILE_LEN as u64, "Map16 set"),
        BoundedRead::new(
            graphics_path,
            GraphicsInterchangeFile::MAX_FILE_LEN as u64,
            "level graphics",
        ),
        BoundedRead::new(
            palette_path,
            PaletteInterchangeFile::MAX_FILE_LEN as u64,
            "level palette",
        ),
    ])
}

pub(super) fn decode_loaded(loaded: LoadedDocument) -> Result<PendingOpen, String> {
    let [
        (path, level_bytes),
        (_, map16_bytes),
        (_, graphics_bytes),
        (_, palette_bytes),
    ] = loaded.into_exact::<4>("complete-level")?;
    let controller = CompleteLevelDocumentController::decode(path, &level_bytes)
        .map_err(|error| error.to_string())?;
    let dimensions = suggested_dimensions(controller.value());
    Ok(PendingOpen {
        controller,
        assets: LevelAssets {
            map16: Map16SetFile::decode(&map16_bytes).map_err(|error| error.to_string())?,
            graphics: GraphicsInterchangeFile::decode(&graphics_bytes)
                .map_err(|error| error.to_string())?,
            palette: PaletteInterchangeFile::decode(&palette_bytes)
                .map_err(|error| error.to_string())?,
        },
        dimensions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_level_load_requires_all_four_ordered_inputs() {
        assert!(decode_loaded(LoadedDocument { files: Vec::new() }).is_err());
        assert!(
            decode_loaded(LoadedDocument {
                files: vec![(std::path::PathBuf::from("level.lmlevel"), Vec::new())],
            })
            .is_err()
        );
    }
}
