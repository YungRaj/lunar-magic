use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_app::CustomSpriteLibraryController;
use lm_level::{MAX_CUSTOM_SPRITE_SIDECAR_LEN, SpriteLengthTable};
use std::path::PathBuf;

pub(super) fn requests(
    data_path: PathBuf,
    descriptions_path: PathBuf,
    lengths_path: PathBuf,
) -> Vec<BoundedRead> {
    let maximum = u64::try_from(MAX_CUSTOM_SPRITE_SIDECAR_LEN).unwrap_or(u64::MAX);
    vec![
        BoundedRead::new(data_path, maximum, "custom-sprite binary sidecar"),
        BoundedRead::new(
            descriptions_path,
            maximum,
            "custom-sprite description sidecar",
        ),
        BoundedRead::new(
            lengths_path,
            u64::try_from(SpriteLengthTable::ENCODED_LEN).unwrap_or(u64::MAX),
            "sprite length table",
        ),
    ]
}

pub(super) fn decode(loaded: LoadedDocument) -> Result<CustomSpriteLibraryController, String> {
    let [
        (data_path, data),
        (descriptions_path, descriptions),
        (_, lengths),
    ] = loaded.into_exact::<3>("custom-sprite")?;
    let lengths = SpriteLengthTable::decode(&lengths)
        .map_err(|actual| format!("sprite length table requires 1024 bytes, got {actual}"))?;
    CustomSpriteLibraryController::decode(
        data_path,
        descriptions_path,
        &data,
        &descriptions,
        lengths,
    )
    .map_err(|error| error.to_string())
}
