use crate::{EditorMode, RevisionProfile};
use lm_rom::RomIdentity;
use std::path::PathBuf;

/// Immutable input for a background editor controller or renderer.
///
/// The included revision must be returned with any proposed mutation. ROM bytes retain an
/// optional copier header exactly as opened so existing project decoders observe the same image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerSnapshot {
    pub revision: u64,
    pub mode: EditorMode,
    pub identity: RomIdentity,
    pub document_path: Option<PathBuf>,
    pub rom_bytes: Vec<u8>,
}

/// One atomic background-work input containing both ROM bytes and their validated revision
/// metadata. Its shared revision token becomes stale when either ROM or profile state changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfiledControllerSnapshot {
    pub snapshot: ControllerSnapshot,
    pub profile: RevisionProfile,
}
