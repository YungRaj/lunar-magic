use super::GraphicsDocument;
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::GraphicsDocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use std::path::PathBuf;

pub(super) fn requests(graphics_path: PathBuf, palette_path: PathBuf) -> Vec<BoundedRead> {
    vec![
        BoundedRead::new(
            graphics_path,
            u64::try_from(GraphicsInterchangeFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "graphics document",
        ),
        BoundedRead::new(
            palette_path,
            u64::try_from(PaletteInterchangeFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "graphics palette",
        ),
    ]
}

pub(super) fn decode_documents(loaded: LoadedDocument) -> Result<GraphicsDocument, String> {
    let [(graphics_path, graphics_bytes), (_, palette_bytes)] =
        loaded.into_exact::<2>("graphics")?;
    let controller = GraphicsDocumentController::decode(graphics_path, &graphics_bytes)
        .map_err(|error| error.to_string())?;
    let palette =
        PaletteInterchangeFile::decode(&palette_bytes).map_err(|error| error.to_string())?;
    if palette.palette.colors.len() < 16 || palette.palette.colors.len() % 16 != 0 {
        return Err("graphics palette must contain complete 16-color rows".into());
    }
    Ok(GraphicsDocument {
        controller,
        palette,
    })
}

pub(super) fn begin_save(
    controller: &mut GraphicsDocumentController,
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
    controller: &mut GraphicsDocumentController,
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
