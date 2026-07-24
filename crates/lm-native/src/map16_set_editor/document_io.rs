use super::Document;
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::Map16DocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::Map16SetFile;
use std::path::PathBuf;

pub(super) fn requests(set: PathBuf, graphics: PathBuf, palette: PathBuf) -> Vec<BoundedRead> {
    vec![
        BoundedRead::new(set, Map16SetFile::MAX_FILE_LEN as u64, "complete Map16 set"),
        BoundedRead::new(
            graphics,
            GraphicsInterchangeFile::MAX_FILE_LEN as u64,
            "Map16 graphics",
        ),
        BoundedRead::new(
            palette,
            PaletteInterchangeFile::MAX_FILE_LEN as u64,
            "Map16 palette",
        ),
    ]
}

pub(super) fn decode_document(loaded: LoadedDocument) -> Result<Document, String> {
    let [(set, set_bytes), (_, graphics_bytes), (_, palette_bytes)] =
        loaded.into_exact::<3>("Map16-set")?;
    Ok(Document {
        controller: Map16DocumentController::decode(set, &set_bytes)
            .map_err(|error| error.to_string())?,
        graphics: GraphicsInterchangeFile::decode(&graphics_bytes)
            .map_err(|error| error.to_string())?,
        palette: PaletteInterchangeFile::decode(&palette_bytes)
            .map_err(|error| error.to_string())?,
    })
}
