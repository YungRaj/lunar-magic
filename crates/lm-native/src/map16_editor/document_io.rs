use super::{
    GraphicsInterchangeFile, Map16Document, Map16PageDocumentController, PaletteInterchangeFile,
};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_level::Map16PageFile;
use std::path::PathBuf;

pub(super) fn requests(page: PathBuf, graphics: PathBuf, palette: PathBuf) -> Vec<BoundedRead> {
    vec![
        BoundedRead::new(page, Map16PageFile::ENCODED_LEN as u64, "Map16 page"),
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

pub(super) fn decode_document(loaded: LoadedDocument) -> Result<Map16Document, String> {
    let [(page, page_bytes), (_, graphics_bytes), (_, palette_bytes)] =
        loaded.into_exact::<3>("Map16")?;
    Ok(Map16Document {
        controller: Map16PageDocumentController::decode(page, &page_bytes)
            .map_err(|error| error.to_string())?,
        graphics: GraphicsInterchangeFile::decode(&graphics_bytes)
            .map_err(|error| error.to_string())?,
        palette: PaletteInterchangeFile::decode(&palette_bytes)
            .map_err(|error| error.to_string())?,
    })
}

pub(super) fn begin_save(
    controller: &mut Map16PageDocumentController,
    worker: &mut crate::persistence_worker::PersistenceWorker,
    error_slot: &mut Option<String>,
) {
    match controller.begin_save() {
        Ok(snapshot) => {
            if let Err(error) = worker.start(
                snapshot.request_id,
                crate::persistence_worker::PersistenceTarget::Replace(snapshot.path),
                snapshot.bytes,
            ) {
                let _cancel_result = controller.cancel_save(snapshot.request_id);
                *error_slot = Some(error);
            }
        }
        Err(error) => *error_slot = Some(error.to_string()),
    }
}

pub(super) fn complete_save(
    controller: &mut Map16PageDocumentController,
    completion: crate::persistence_worker::PersistenceCompletion,
    error_slot: &mut Option<String>,
) {
    let result = match completion.result {
        Ok(()) => controller.acknowledge_save(completion.request_id),
        Err(error) => {
            let cancellation = controller.cancel_save(completion.request_id);
            *error_slot = Some(error);
            cancellation
        }
    };
    if let Err(error) = result {
        *error_slot = Some(error.to_string());
    }
}
