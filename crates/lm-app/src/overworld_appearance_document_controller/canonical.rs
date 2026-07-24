use super::OverworldAppearanceDocumentControllerError;
use lm_overworld::SpriteAppearanceFile;

pub(super) fn canonical_bytes(
    value: &SpriteAppearanceFile,
) -> Result<Vec<u8>, OverworldAppearanceDocumentControllerError> {
    let bytes = value
        .encode()
        .map_err(OverworldAppearanceDocumentControllerError::File)?;
    if SpriteAppearanceFile::decode(&bytes)
        .map_err(OverworldAppearanceDocumentControllerError::File)?
        != *value
    {
        return Err(OverworldAppearanceDocumentControllerError::NonCanonicalEncoding);
    }
    Ok(bytes)
}

pub(super) fn canonical_reopen(
    value: &SpriteAppearanceFile,
) -> Result<SpriteAppearanceFile, OverworldAppearanceDocumentControllerError> {
    SpriteAppearanceFile::decode(&canonical_bytes(value)?)
        .map_err(OverworldAppearanceDocumentControllerError::File)
}
