//! Typed commands for toolkit-neutral portable document sessions.
//!
//! These types are separate from the textual shell grammar so graphical frontends can route the
//! same lifecycle operations without depending on command-line parsing details.

use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3DocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompleteLevelDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16DocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16PageDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldAppearanceDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityAppearanceDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    ImportOptionalAssets(PathBuf),
    EditOptionalAssets(PathBuf),
    EditLayer3Settings(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLevelDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeAssetsDocumentCommand {
    Open(PathBuf),
    Edit(PathBuf),
    Render(PathBuf),
    Undo,
    Redo,
    Status,
    Save,
    Close,
    Discard,
}
