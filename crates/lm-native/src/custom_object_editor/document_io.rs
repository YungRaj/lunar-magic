use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::CustomObjectLibraryController;
use lm_level::MAX_CUSTOM_OBJECT_SIDECAR_LEN;
use std::path::PathBuf;

pub(super) fn requests(data_path: PathBuf, descriptions_path: PathBuf) -> Vec<BoundedRead> {
    let maximum = u64::try_from(MAX_CUSTOM_OBJECT_SIDECAR_LEN).unwrap_or(u64::MAX);
    vec![
        BoundedRead::new(data_path, maximum, "custom-object binary sidecar"),
        BoundedRead::new(
            descriptions_path,
            maximum,
            "custom-object description sidecar",
        ),
    ]
}

pub(super) fn decode(loaded: LoadedDocument) -> Result<CustomObjectLibraryController, String> {
    let [(data_path, data), (descriptions_path, descriptions)] =
        loaded.into_exact::<2>("custom-object")?;
    CustomObjectLibraryController::decode(data_path, descriptions_path, &data, &descriptions)
        .map_err(|error| error.to_string())
}
