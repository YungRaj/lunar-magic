use super::ExAnimationDocument;
use lm_app::ExAnimationDocumentController;
use std::path::PathBuf;

pub(super) fn decode_document(
    path: PathBuf,
    bytes: &[u8],
    modes: [bool; 256],
    maximum_records: usize,
) -> Result<ExAnimationDocument, String> {
    if maximum_records == 0 || maximum_records > 255 {
        return Err("maximum record count must be between 1 and 255".into());
    }
    let controller = ExAnimationDocumentController::decode(path, bytes, maximum_records, &modes)
        .map_err(|error| error.to_string())?;
    Ok(ExAnimationDocument { controller, modes })
}
