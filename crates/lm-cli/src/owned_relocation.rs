//! Bounded input boundary for explicit native-payload ownership evidence.

use crate::oracle_input::read_bounded;
use lm_project::{RatsOwnershipManifest, RatsOwnershipManifestFile};
use std::path::Path;

pub fn read_optional(
    path: Option<&Path>,
) -> Result<Option<RatsOwnershipManifest>, Box<dyn std::error::Error>> {
    path.map(|path| {
        Ok(RatsOwnershipManifestFile::decode(&read_bounded(
            path,
            RatsOwnershipManifestFile::MAX_FILE_LEN,
        )?)?
        .0)
    })
    .transpose()
}
