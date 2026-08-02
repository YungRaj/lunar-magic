use crate::{
    ClipboardPayload, EditorSelection, EmulatorTestRequest, ExternalToolError, LevelViewport,
    ToolInvocation,
};
use std::path::PathBuf;

/// The toolkit-independent editor surface currently selected by a frontend.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorMode {
    #[default]
    NoProject,
    Level(u16),
    Overworld,
    Map16,
    Graphics(u16),
    Palette(u16),
    ExAnimation(u16),
    Layer3(u16),
}

/// Work that the pure application state delegates to its platform frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrontendEffect {
    ChooseRom {
        request_id: u64,
    },
    ChooseSaveDestination {
        request_id: u64,
        bytes: Vec<u8>,
    },
    PersistRomAt {
        request_id: u64,
        path: PathBuf,
        bytes: Vec<u8>,
    },
    ViewChanged(EditorMode),
    LevelViewportChanged(LevelViewport),
    ConfirmDiscardChanges {
        quit_after: bool,
    },
    ConfirmDiscardAndOpen,
    ProjectClosed,
    QuitApplication,
    WriteClipboard(Vec<u8>),
    ApplyClipboard(ClipboardPayload),
    ProjectChanged {
        description: String,
        mode: EditorMode,
        revision: u64,
    },
    RevisionProfileChanged {
        name: Option<String>,
        revision: u64,
    },
    CutSelection {
        selection: EditorSelection,
        clipboard: Vec<u8>,
    },
    LaunchExternalTool(ToolInvocation),
    StageEmulatorTest(EmulatorTestRequest),
    ExternalToolFailed {
        tool_id: String,
        error: ExternalToolError,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectStatus {
    #[default]
    Closed,
    OpenClean,
    OpenModified,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SaveStatus {
    #[default]
    Idle,
    Pending,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HistoryCapabilities {
    pub undo: bool,
    pub redo: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NavigationCapabilities {
    pub level_back: bool,
    pub level_forward: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppCapabilities {
    pub project: ProjectStatus,
    pub profile: ProfileStatus,
    pub history: HistoryCapabilities,
    pub navigation: NavigationCapabilities,
    pub save: SaveStatus,
    pub selection: SelectionCapabilities,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProfileStatus {
    #[default]
    Missing,
    Loaded,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SelectionCapabilities {
    pub copy: bool,
    pub cut: bool,
}

impl AppCapabilities {
    #[must_use]
    pub const fn can_save(self) -> bool {
        matches!(self.project, ProjectStatus::OpenModified) && matches!(self.save, SaveStatus::Idle)
    }
}
