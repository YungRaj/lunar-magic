use lm_project::{GraphicsIoError, Project, TransactionError};
use lm_rom::{IdentityError, RomError, RomImage};
use std::path::PathBuf;

use crate::document::PendingOpen;
use crate::level_navigation::LevelNavigationHistory;
#[cfg(test)]
use crate::{
    AppCapabilities, FrontendConfig, ProfileStatus, ProjectStatus, SaveStatus, ShortcutGesture,
    ToolEvent, ToolbarAction, ToolbarActivation,
};
use crate::{
    ClipboardError, ClipboardKind, ClipboardPayload, Command, EditorMode, EditorSelection,
    ExternalTool, ExternalToolError, FrontendConfigError, FrontendEffect, LocalizationCatalog,
    LocalizationError, RecentDocuments, RevisionProfile, RevisionProfileAuditError,
    RevisionProfileError, ShortcutConfig, ShortcutError, ToolbarConfig, ToolbarError,
};

#[derive(Debug, Default)]
pub struct AppState {
    pub(crate) project: Option<Project>,
    pub document_path: Option<PathBuf>,
    pub mode: EditorMode,
    pub status: String,
    pub selection: Option<EditorSelection>,
    pub(crate) revision_profile: Option<RevisionProfile>,
    pub(crate) external_tools: Vec<ExternalTool>,
    pub(crate) localization: Option<LocalizationCatalog>,
    pub(crate) toolbar: Option<ToolbarConfig>,
    pub(crate) shortcuts: Option<ShortcutConfig>,
    pub(crate) recent_documents: RecentDocuments,
    pub(crate) pending_save: Option<PendingSave>,
    pub(crate) pending_open: Option<PendingOpen>,
    pub(crate) next_open_request: u64,
    pub(crate) next_save_request: u64,
    pub(crate) project_revision: u64,
    pub(crate) level_navigation: LevelNavigationHistory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingSave {
    pub request_id: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum AppError {
    NoProject,
    NoLevelView,
    NoRevisionProfile,
    ProjectAlreadyOpen,
    EmptyEditDescription,
    StaleProjectRevision {
        expected: u64,
        actual: u64,
    },
    ProjectRevisionOverflow,
    NoPendingSave,
    SaveAlreadyPending,
    SaveInProgress,
    NoPendingOpen,
    OpenAlreadyPending,
    OpenInProgress,
    OpenRequestOverflow,
    SaveRequestOverflow,
    OpenContextChanged,
    StaleOpenRequest {
        expected: u64,
        actual: u64,
    },
    StaleSaveAcknowledgement,
    StaleSaveRequest {
        expected: u64,
        actual: u64,
    },
    Rom(RomError),
    Identity(IdentityError),
    Ips(lm_rom::IpsError),
    IpsIdentityMismatch,
    Clipboard(ClipboardError),
    ExternalTool(ExternalToolError),
    Localization(LocalizationError),
    Toolbar(ToolbarError),
    Shortcut(ShortcutError),
    FrontendConfig(FrontendConfigError),
    RevisionProfile(RevisionProfileError),
    RevisionProfileAudit(RevisionProfileAuditError),
    Transaction(TransactionError),
    GraphicsMigration(GraphicsIoError),
    LevelAccessRestriction(lm_project::LevelAccessRestrictionError),
    GraphicsMigrationProfileMismatch,
    RevisionPatchPlan(lm_profile::RevisionPatchPlanError),
    ExpandedSettingsPlan(lm_profile::ExpandedSettingsInstallPlanError),
    Layer3Plan(lm_profile::CompleteLayer3BuildError),
    Lfix3Plan(lm_profile::Lfix3RuntimeLengthError),
    Lfix3Detect(lm_profile::SmwUsV1Lfix3DetectError),
    Map16RuntimePlan(lm_profile::SmwUsV1Map16RuntimeInstallBuildError),
    Map16RuntimeDetect(lm_profile::SmwUsV1Map16RuntimeDetectError),
    Sprite19FixPlan(lm_profile::SmwUsV1Sprite19FixInstallError),
    SecondaryExitInstallPlan(lm_profile::SecondaryExitInstallBuildError),
    SecondaryExitLfix3AuthenticationMissing,
    RevisionPatch(lm_project::RelocatablePatchError),
    RevisionPatchGroup(lm_project::RelocatablePatchGroupError),
    RatsReclamation(lm_project::RatsReclamationError),
    ExpandedSettingsIdentityMismatch,
    Layer3IdentityMismatch,
    Lfix3IdentityMismatch,
    Lfix3AlreadyInstalled,
    Map16RuntimeIdentityMismatch,
    Map16RuntimeAlreadyInstalled,
    Sprite19FixIdentityMismatch,
    Sprite19FixAlreadyInstalled,
    NativeOverworldPathIdentityMismatch,
    NativeOverworldPathReopenMismatch,
    NativeOverworldPath(lm_project::OverworldPathLinkIoError),
    NativeOverworldPathPatch(lm_project::OverworldPathPatchError),
    NativeOverworldPathPatchSave(lm_project::OverworldPathPatchSaveError),
    NativeOverworldPathPatchBuild(lm_profile::OverworldPathPatchBuildError),
    NativeOverworldMessageIdentityMismatch,
    NativeOverworldMessageHookMismatch,
    NativeOverworldMessageReopenMismatch,
    NativeOverworldMessagePatch(lm_project::OverworldMessagePatchError),
    NativeOverworldMessagePatchSave(lm_project::OverworldMessagePatchSaveError),
    NativeOverworldMessagePatchBuild(lm_profile::OverworldMessagePatchBuildError),
    NativeOverworldEventIdentityMismatch,
    NativeOverworldEventPatch(lm_project::OverworldEventRevealPatchError),
    NativeOverworldEventSave(lm_project::OverworldEventRevealSaveError),
    NativeOverworldEventMapIdentityMismatch,
    NativeOverworldEventMap(lm_project::OverworldEventNumberMapError),
    NativeSpecialEventIdentityMismatch,
    NativeSpecialEventPatch(lm_project::SpecialEventRevealPatchError),
    NativeSpecialEventSave(lm_project::SpecialEventRevealSaveError),
    NativeSpecialEventBuild(lm_profile::SpecialEventRevealPatchBuildError),
    NativeEventTilemapIdentityMismatch,
    NativeEventTilemapReopenMismatch,
    NativeEventTilemap(lm_project::EventTilemapPatchError),
    NativeEventTilemapLoad(lm_profile::SmwUsV1EventTilemapLoadError),
    NativeBossSequenceIdentityMismatch,
    NativeBossSequence(lm_project::BossSequencePatchError),
    CreditsTilemapIdentityMismatch,
    CreditsTilemap(lm_project::CreditsTilemapIoError),
    CreditsTilemapPatch(lm_project::CreditsTilemapPatchError),
    TitleTilemapIdentityMismatch,
    TitleTilemapPatch(lm_project::TitleTilemapPatchError),
    TitleRecordingIdentityMismatch,
    TitleRecordingPatch(lm_project::TitleRecordingPatchError),
    LunarMagicMetadataIdentityMismatch,
    LunarMagicMetadata(lm_project::LunarMagicRomMetadataIoError),
    SecondaryExitIdentityMismatch,
    SecondaryExitPatch(lm_project::SecondaryExitPatchError),
    NativeOverworldWarpIdentityMismatch,
    NativeOverworldWarpReopenMismatch,
    NativeOverworldWarp(lm_project::OverworldWarpLinkIoError),
    NativeOverworldWarpPatch(lm_project::OverworldWarpPatchError),
    NativeOverworldWarpPatchSave(lm_project::OverworldWarpPatchSaveError),
    NativeOverworldWarpPatchMigration(lm_project::OverworldWarpPatchMigrationError),
    NativeOverworldWarpPatchBuild(lm_profile::OverworldWarpPatchBuildError),
    NativeOverworldLevelNameIdentityMismatch,
    NativeOverworldLevelNameReopenMismatch,
    NativeOverworldLevelNameTable(lm_overworld::NativeOverworldLevelNameError),
    NativeOverworldLevelNameIo(lm_project::OverworldLevelNameIoError),
    NativeOverworldLevelNameSave(lm_project::OverworldLevelNamePatchSaveError),
    NativeOverworldLevelNameBuild(lm_profile::OverworldLevelNamePatchBuildError),
    NativeOverworldSettingsIdentityMismatch,
    NativeOverworldSettingsReopenMismatch,
    NativeOverworldSettingsStorage(String),
    NativeOverworldSettingsIo(lm_project::ExpandedOverworldSettingsIoError),
    NativeSharedPaletteIdentityMismatch,
    NativeSharedPaletteReopenMismatch,
    NativeSharedPaletteIo(lm_project::SharedPaletteIoError),
    NativeSharedPaletteFile(lm_graphics::SmwPaletteFileError),
    NativeSharedPaletteInstallPlan(lm_profile::SharedPaletteInstallPlanError),
    NativeOverworldPlayerStartIdentityMismatch,
    NativeOverworldPlayerStartReopenMismatch,
    NativeOverworldPlayerStartTable(lm_overworld::NativeOverworldPlayerStartError),
    NativeOverworldPlayerStartIo(lm_project::OverworldPlayerStartIoError),
    SelectionWrongMode {
        mode: EditorMode,
        kind: ClipboardKind,
    },
    ClipboardSelectionMismatch {
        selected: usize,
        records: usize,
    },
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "application command failed: {self:?}")
    }
}

impl std::error::Error for AppError {}

impl From<RomError> for AppError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<IdentityError> for AppError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}

impl From<lm_rom::IpsError> for AppError {
    fn from(value: lm_rom::IpsError) -> Self {
        Self::Ips(value)
    }
}

impl From<ClipboardError> for AppError {
    fn from(value: ClipboardError) -> Self {
        Self::Clipboard(value)
    }
}

impl From<ExternalToolError> for AppError {
    fn from(value: ExternalToolError) -> Self {
        Self::ExternalTool(value)
    }
}

impl From<LocalizationError> for AppError {
    fn from(value: LocalizationError) -> Self {
        Self::Localization(value)
    }
}

impl From<ToolbarError> for AppError {
    fn from(value: ToolbarError) -> Self {
        Self::Toolbar(value)
    }
}

impl From<ShortcutError> for AppError {
    fn from(value: ShortcutError) -> Self {
        Self::Shortcut(value)
    }
}

impl From<FrontendConfigError> for AppError {
    fn from(value: FrontendConfigError) -> Self {
        Self::FrontendConfig(value)
    }
}

impl From<RevisionProfileError> for AppError {
    fn from(value: RevisionProfileError) -> Self {
        Self::RevisionProfile(value)
    }
}

impl From<RevisionProfileAuditError> for AppError {
    fn from(value: RevisionProfileAuditError) -> Self {
        Self::RevisionProfileAudit(value)
    }
}

impl From<TransactionError> for AppError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<GraphicsIoError> for AppError {
    fn from(value: GraphicsIoError) -> Self {
        Self::GraphicsMigration(value)
    }
}

impl From<lm_project::LevelAccessRestrictionError> for AppError {
    fn from(value: lm_project::LevelAccessRestrictionError) -> Self {
        Self::LevelAccessRestriction(value)
    }
}

impl From<lm_profile::RevisionPatchPlanError> for AppError {
    fn from(value: lm_profile::RevisionPatchPlanError) -> Self {
        Self::RevisionPatchPlan(value)
    }
}

impl From<lm_profile::ExpandedSettingsInstallPlanError> for AppError {
    fn from(value: lm_profile::ExpandedSettingsInstallPlanError) -> Self {
        Self::ExpandedSettingsPlan(value)
    }
}

impl From<lm_profile::CompleteLayer3BuildError> for AppError {
    fn from(value: lm_profile::CompleteLayer3BuildError) -> Self {
        Self::Layer3Plan(value)
    }
}

impl From<lm_profile::Lfix3RuntimeLengthError> for AppError {
    fn from(value: lm_profile::Lfix3RuntimeLengthError) -> Self {
        Self::Lfix3Plan(value)
    }
}

impl From<lm_profile::SmwUsV1Lfix3DetectError> for AppError {
    fn from(value: lm_profile::SmwUsV1Lfix3DetectError) -> Self {
        Self::Lfix3Detect(value)
    }
}

impl From<lm_profile::SmwUsV1Map16RuntimeInstallBuildError> for AppError {
    fn from(value: lm_profile::SmwUsV1Map16RuntimeInstallBuildError) -> Self {
        Self::Map16RuntimePlan(value)
    }
}

impl From<lm_profile::SmwUsV1Map16RuntimeDetectError> for AppError {
    fn from(value: lm_profile::SmwUsV1Map16RuntimeDetectError) -> Self {
        Self::Map16RuntimeDetect(value)
    }
}

impl From<lm_profile::SmwUsV1Sprite19FixInstallError> for AppError {
    fn from(value: lm_profile::SmwUsV1Sprite19FixInstallError) -> Self {
        Self::Sprite19FixPlan(value)
    }
}

impl From<lm_profile::SmwUsV1Sprite19FixDetectError> for AppError {
    fn from(value: lm_profile::SmwUsV1Sprite19FixDetectError) -> Self {
        Self::Sprite19FixPlan(lm_profile::SmwUsV1Sprite19FixInstallError::Detect(value))
    }
}

impl From<lm_project::RatsReclamationError> for AppError {
    fn from(value: lm_project::RatsReclamationError) -> Self {
        Self::RatsReclamation(value)
    }
}

impl From<lm_profile::SecondaryExitInstallBuildError> for AppError {
    fn from(value: lm_profile::SecondaryExitInstallBuildError) -> Self {
        Self::SecondaryExitInstallPlan(value)
    }
}

impl From<lm_project::RelocatablePatchError> for AppError {
    fn from(value: lm_project::RelocatablePatchError) -> Self {
        Self::RevisionPatch(value)
    }
}

impl From<lm_profile::SharedPaletteInstallPlanError> for AppError {
    fn from(value: lm_profile::SharedPaletteInstallPlanError) -> Self {
        Self::NativeSharedPaletteInstallPlan(value)
    }
}

impl From<lm_project::RelocatablePatchGroupError> for AppError {
    fn from(value: lm_project::RelocatablePatchGroupError) -> Self {
        Self::RevisionPatchGroup(value)
    }
}

impl From<lm_project::OverworldPathLinkIoError> for AppError {
    fn from(value: lm_project::OverworldPathLinkIoError) -> Self {
        Self::NativeOverworldPath(value)
    }
}

impl From<lm_project::OverworldPathPatchError> for AppError {
    fn from(value: lm_project::OverworldPathPatchError) -> Self {
        Self::NativeOverworldPathPatch(value)
    }
}

impl From<lm_project::OverworldPathPatchSaveError> for AppError {
    fn from(value: lm_project::OverworldPathPatchSaveError) -> Self {
        Self::NativeOverworldPathPatchSave(value)
    }
}

impl From<lm_profile::OverworldPathPatchBuildError> for AppError {
    fn from(value: lm_profile::OverworldPathPatchBuildError) -> Self {
        Self::NativeOverworldPathPatchBuild(value)
    }
}

impl From<lm_project::OverworldMessagePatchError> for AppError {
    fn from(value: lm_project::OverworldMessagePatchError) -> Self {
        Self::NativeOverworldMessagePatch(value)
    }
}

impl From<lm_project::OverworldMessagePatchSaveError> for AppError {
    fn from(value: lm_project::OverworldMessagePatchSaveError) -> Self {
        Self::NativeOverworldMessagePatchSave(value)
    }
}

impl From<lm_profile::OverworldMessagePatchBuildError> for AppError {
    fn from(value: lm_profile::OverworldMessagePatchBuildError) -> Self {
        Self::NativeOverworldMessagePatchBuild(value)
    }
}

impl From<lm_project::OverworldEventRevealPatchError> for AppError {
    fn from(value: lm_project::OverworldEventRevealPatchError) -> Self {
        Self::NativeOverworldEventPatch(value)
    }
}

impl From<lm_project::OverworldEventRevealSaveError> for AppError {
    fn from(value: lm_project::OverworldEventRevealSaveError) -> Self {
        Self::NativeOverworldEventSave(value)
    }
}

impl From<lm_project::OverworldEventNumberMapError> for AppError {
    fn from(value: lm_project::OverworldEventNumberMapError) -> Self {
        Self::NativeOverworldEventMap(value)
    }
}

impl From<lm_project::SpecialEventRevealPatchError> for AppError {
    fn from(value: lm_project::SpecialEventRevealPatchError) -> Self {
        Self::NativeSpecialEventPatch(value)
    }
}

impl From<lm_project::SpecialEventRevealSaveError> for AppError {
    fn from(value: lm_project::SpecialEventRevealSaveError) -> Self {
        Self::NativeSpecialEventSave(value)
    }
}

impl From<lm_profile::SpecialEventRevealPatchBuildError> for AppError {
    fn from(value: lm_profile::SpecialEventRevealPatchBuildError) -> Self {
        Self::NativeSpecialEventBuild(value)
    }
}

impl From<lm_project::EventTilemapPatchError> for AppError {
    fn from(value: lm_project::EventTilemapPatchError) -> Self {
        Self::NativeEventTilemap(value)
    }
}

impl From<lm_profile::SmwUsV1EventTilemapLoadError> for AppError {
    fn from(value: lm_profile::SmwUsV1EventTilemapLoadError) -> Self {
        Self::NativeEventTilemapLoad(value)
    }
}

impl From<lm_project::BossSequencePatchError> for AppError {
    fn from(value: lm_project::BossSequencePatchError) -> Self {
        Self::NativeBossSequence(value)
    }
}

impl From<lm_project::CreditsTilemapIoError> for AppError {
    fn from(value: lm_project::CreditsTilemapIoError) -> Self {
        Self::CreditsTilemap(value)
    }
}

impl From<lm_project::CreditsTilemapPatchError> for AppError {
    fn from(value: lm_project::CreditsTilemapPatchError) -> Self {
        Self::CreditsTilemapPatch(value)
    }
}

impl From<lm_project::TitleTilemapPatchError> for AppError {
    fn from(value: lm_project::TitleTilemapPatchError) -> Self {
        Self::TitleTilemapPatch(value)
    }
}

impl From<lm_project::TitleRecordingPatchError> for AppError {
    fn from(value: lm_project::TitleRecordingPatchError) -> Self {
        Self::TitleRecordingPatch(value)
    }
}

impl From<lm_project::LunarMagicRomMetadataIoError> for AppError {
    fn from(value: lm_project::LunarMagicRomMetadataIoError) -> Self {
        Self::LunarMagicMetadata(value)
    }
}

impl From<lm_project::SecondaryExitPatchError> for AppError {
    fn from(value: lm_project::SecondaryExitPatchError) -> Self {
        Self::SecondaryExitPatch(value)
    }
}

impl From<lm_project::OverworldWarpLinkIoError> for AppError {
    fn from(value: lm_project::OverworldWarpLinkIoError) -> Self {
        Self::NativeOverworldWarp(value)
    }
}

impl From<lm_project::OverworldWarpPatchError> for AppError {
    fn from(value: lm_project::OverworldWarpPatchError) -> Self {
        Self::NativeOverworldWarpPatch(value)
    }
}

impl From<lm_project::OverworldWarpPatchSaveError> for AppError {
    fn from(value: lm_project::OverworldWarpPatchSaveError) -> Self {
        Self::NativeOverworldWarpPatchSave(value)
    }
}

impl From<lm_project::OverworldWarpPatchMigrationError> for AppError {
    fn from(value: lm_project::OverworldWarpPatchMigrationError) -> Self {
        Self::NativeOverworldWarpPatchMigration(value)
    }
}

impl From<lm_profile::OverworldWarpPatchBuildError> for AppError {
    fn from(value: lm_profile::OverworldWarpPatchBuildError) -> Self {
        Self::NativeOverworldWarpPatchBuild(value)
    }
}

impl From<lm_overworld::NativeOverworldLevelNameError> for AppError {
    fn from(value: lm_overworld::NativeOverworldLevelNameError) -> Self {
        Self::NativeOverworldLevelNameTable(value)
    }
}

impl From<lm_project::OverworldLevelNameIoError> for AppError {
    fn from(value: lm_project::OverworldLevelNameIoError) -> Self {
        Self::NativeOverworldLevelNameIo(value)
    }
}

impl From<lm_project::OverworldLevelNamePatchSaveError> for AppError {
    fn from(value: lm_project::OverworldLevelNamePatchSaveError) -> Self {
        Self::NativeOverworldLevelNameSave(value)
    }
}

impl From<lm_profile::OverworldLevelNamePatchBuildError> for AppError {
    fn from(value: lm_profile::OverworldLevelNamePatchBuildError) -> Self {
        Self::NativeOverworldLevelNameBuild(value)
    }
}

impl From<lm_project::ExpandedOverworldSettingsIoError> for AppError {
    fn from(value: lm_project::ExpandedOverworldSettingsIoError) -> Self {
        Self::NativeOverworldSettingsIo(value)
    }
}

impl From<lm_project::SharedPaletteIoError> for AppError {
    fn from(value: lm_project::SharedPaletteIoError) -> Self {
        Self::NativeSharedPaletteIo(value)
    }
}

impl From<lm_graphics::SmwPaletteFileError> for AppError {
    fn from(value: lm_graphics::SmwPaletteFileError) -> Self {
        Self::NativeSharedPaletteFile(value)
    }
}

impl From<lm_project::OverworldPlayerStartIoError> for AppError {
    fn from(value: lm_project::OverworldPlayerStartIoError) -> Self {
        Self::NativeOverworldPlayerStartIo(value)
    }
}

impl From<lm_overworld::NativeOverworldPlayerStartError> for AppError {
    fn from(value: lm_overworld::NativeOverworldPlayerStartError) -> Self {
        Self::NativeOverworldPlayerStartTable(value)
    }
}

impl AppState {
    /// Returns the open project for read-only synchronous inspection.
    ///
    /// Background work should use [`Self::controller_snapshot`] instead so its input carries a
    /// revision token. All mutation remains behind [`Command::CommitRomWrites`].
    #[must_use]
    pub const fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    /// Installs ROM bytes during application startup when no document is open.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rom`] if the data is not a valid-sized ROM image.
    pub fn load_rom(&mut self, bytes: Vec<u8>) -> Result<(), AppError> {
        self.load_rom_at(bytes, None)
    }

    /// Installs startup ROM bytes and their platform document path when no document is open.
    ///
    /// Interactive frontends must use [`Command::Open`] followed by [`Self::complete_open`]; this
    /// startup entry point cannot replace an existing document or bypass dirty-state confirmation.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the data is not a supported ROM. Existing state is preserved on
    /// failure.
    pub fn load_rom_at(&mut self, bytes: Vec<u8>, path: Option<PathBuf>) -> Result<(), AppError> {
        if self.project.is_some() {
            return Err(AppError::ProjectAlreadyOpen);
        }
        if self.pending_open.is_some() {
            return Err(AppError::OpenAlreadyPending);
        }
        let project = Project::open_supported(RomImage::from_bytes(bytes)?)?;
        self.install_project(project, path);
        Ok(())
    }

    /// Executes a toolkit-independent command and returns work for the native frontend.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NoProject`] when an editor command needs an open ROM, or a ROM
    /// error if undo/redo cannot restore its recorded byte range.
    #[allow(clippy::too_many_lines)] // Exhaustive routing; each native operation is implemented in a focused module.
    pub fn dispatch(&mut self, command: Command) -> Result<Vec<FrontendEffect>, AppError> {
        Ok(match command {
            Command::Open => {
                self.require_no_pending_save()?;
                if self.project.as_ref().is_some_and(Project::is_modified) {
                    vec![FrontendEffect::ConfirmDiscardAndOpen]
                } else {
                    self.begin_open()?
                }
            }
            Command::Save => {
                let (request_id, bytes) = self.begin_save()?;
                vec![match self.document_path.clone() {
                    Some(path) => FrontendEffect::PersistRomAt {
                        request_id,
                        path,
                        bytes,
                    },
                    None => FrontendEffect::ChooseSaveDestination { request_id, bytes },
                }]
            }
            Command::SaveAs => {
                let (request_id, bytes) = self.begin_save()?;
                vec![FrontendEffect::ChooseSaveDestination { request_id, bytes }]
            }
            Command::Close => self.request_close(false)?,
            Command::Quit => self.request_close(true)?,
            Command::Undo => self.change_history(true)?,
            Command::Redo => self.change_history(false)?,
            Command::SelectLevel(level) => self.change_view(EditorMode::Level(level))?,
            Command::NavigateLevel(direction) => self.navigate_level(direction)?,
            Command::SetLevelViewport(viewport) => self.set_level_viewport(viewport)?,
            Command::ShowOverworld => self.change_view(EditorMode::Overworld)?,
            Command::ShowMap16 => self.change_view(EditorMode::Map16)?,
            Command::ShowGraphics(file) => self.change_view(EditorMode::Graphics(file))?,
            Command::ShowPalette(palette) => self.change_view(EditorMode::Palette(palette))?,
            Command::ShowExAnimation(slot) => self.change_view(EditorMode::ExAnimation(slot))?,
            Command::ShowLayer3(level) => self.change_view(EditorMode::Layer3(level))?,
            Command::SetSelection(selection) => {
                self.require_kind_for_mode(selection.kind)?;
                self.selection = Some(selection);
                Vec::new()
            }
            Command::ClearSelection => self.clear_selection(),
            Command::Copy(payload) => {
                self.validate_selection_payload(&payload)?;
                vec![FrontendEffect::WriteClipboard(payload.encode()?)]
            }
            Command::Cut(payload) => {
                self.validate_selection_payload(&payload)?;
                let selection =
                    self.selection
                        .clone()
                        .ok_or(AppError::ClipboardSelectionMismatch {
                            selected: 0,
                            records: payload.records().len(),
                        })?;
                vec![FrontendEffect::CutSelection {
                    selection,
                    clipboard: payload.encode()?,
                }]
            }
            Command::Paste(bytes) => {
                self.project.as_ref().ok_or(AppError::NoProject)?;
                let payload = ClipboardPayload::decode(&bytes)?;
                self.require_kind_for_mode(payload.kind)?;
                vec![FrontendEffect::ApplyClipboard(payload)]
            }
            Command::InstallRevisionProfile(profile) => self.install_revision_profile(*profile)?,
            Command::ClearRevisionProfile => self.clear_revision_profile()?,
            Command::CommitRomWrites {
                expected_revision,
                description,
                writes,
            } => self.commit_rom_writes(expected_revision, description, &writes)?,
            Command::CommitRomMutation {
                expected_revision,
                description,
                mutation,
            } => self.commit_rom_mutation(expected_revision, description, &mutation)?,
            Command::MigrateGraphicsCompression {
                expected_revision,
                source,
                target,
                options,
            } => self.migrate_graphics_compression(expected_revision, source, target, &options)?,
            Command::InstallRevisionPatch {
                expected_revision,
                template,
                search,
                fill,
            } => self.install_revision_patch(expected_revision, &template, search, fill)?,
            Command::InstallSettings { rev } => self.install(rev, false)?,
            Command::InstallLayer3 { rev } => self.install(rev, true)?,
            Command::InstallLfix3 { rev } => self.install_lfix3(rev)?,
            Command::InstallMap16Runtime { rev } => self.install_map16_runtime(rev)?,
            Command::InstallSprite19Fix { rev } => self.install_sprite19_fix(rev)?,
            Command::InstallExpandedSharedPalettes { rev } => {
                self.install_native_expanded_shared_palettes(rev)?
            }
            Command::ReclaimOwnedRats {
                rev,
                manifest,
                fill,
            } => self.reclaim_owned_rats(rev, &manifest, fill)?,
            Command::ApplyIpsPatch { rev, patch } => self.apply_ips_patch(rev, &patch)?,
            Command::SetCopierHeader { rev, target, fill } => {
                self.set_copier_header(rev, target, fill)?
            }
            Command::ReplaceNativeOverworldPathLinks { rev, table } => {
                self.replace_native_path_links(rev, &table)?
            }
            Command::ReplaceNativeOverworldMessages { rev, messages } => {
                self.replace_native_overworld_messages(rev, &messages)?
            }
            Command::ReplaceNativeOverworldEventReveals { rev, table } => {
                self.replace_native_overworld_event_reveals(rev, &table)?
            }
            Command::ReplaceNativeOverworldEventNumberMap { rev, map } => {
                self.replace_native_overworld_event_number_map(rev, &map)?
            }
            Command::ReplaceNativeSpecialEventReveals { rev, table } => {
                self.replace_native_special_event_reveals(rev, &table)?
            }
            Command::ReplaceNativeOverworldEventTilemaps { rev, buffers } => {
                self.replace_native_event_tilemaps(rev, &buffers)?
            }
            Command::ReplaceNativeOverworldBossSequence { rev, table } => {
                self.replace_native_boss_sequence_messages(rev, &table)?
            }
            Command::ReplaceNativeCreditsTilemap { rev, tilemap } => {
                self.replace_credits_tilemap(rev, &tilemap)?
            }
            Command::ReplaceNativeTitleTilemap { rev, tilemap } => {
                self.replace_title_tilemap(rev, &tilemap)?
            }
            Command::ReplaceNativeTitleRecording { rev, recording } => {
                self.replace_title_recording(rev, &recording)?
            }
            Command::ReplaceLunarMagicRomMetadata { rev, metadata } => {
                self.replace_lunar_magic_rom_metadata(rev, &metadata)?
            }
            Command::ReplaceNativeSecondaryExits { rev, table } => {
                self.replace_native_secondary_exits(rev, &table)?
            }
            Command::ReplaceNativeOverworldWarpLinks { rev, table } => {
                self.replace_native_warp_links(rev, &table)?
            }
            Command::ReplaceNativeOverworldLevelNames { rev, table } => {
                self.replace_native_level_names(rev, &table)?
            }
            Command::ReplaceNativeOverworldSettings { rev, settings } => {
                self.replace_native_overworld_settings(rev, &settings)?
            }
            Command::ReplaceNativeOverworldLayer3Settings { rev, settings } => {
                self.replace_native_overworld_layer3_settings(rev, &settings)?
            }
            Command::ReplaceNativeSharedPalette { rev, palette } => {
                self.replace_native_shared_palette(rev, &palette)?
            }
            Command::ReplaceNativeOverworldPlayerStarts { rev, starts } => {
                self.replace_native_overworld_player_starts(rev, &starts)?
            }
            Command::ExpandRom(request) => self.expand_rom(&request)?,
            Command::RestrictLevelAccess { rev, title, keys } => {
                self.restrict_level_access(rev, &title, keys)?
            }
            Command::RunExternalTool(id) => self.run_external_tool(&id)?,
        })
    }

    fn clear_selection(&mut self) -> Vec<FrontendEffect> {
        self.selection = None;
        Vec::new()
    }

    /// Completes a user-confirmed destructive close request.
    #[must_use]
    pub fn discard_and_close(&mut self, quit_after: bool) -> Vec<FrontendEffect> {
        self.project = None;
        self.revision_profile = None;
        self.document_path = None;
        self.pending_save = None;
        self.pending_open = None;
        self.selection = None;
        self.mode = EditorMode::NoProject;
        self.project_revision = 0;
        self.level_navigation.reset(None);
        self.status = "Project closed".into();
        let mut effects = vec![FrontendEffect::ProjectClosed];
        if quit_after {
            effects.push(FrontendEffect::QuitApplication);
        }
        effects
    }

    /// Completes confirmation for replacing a modified project and requests a ROM from the
    /// frontend.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if another chooser is active or no new request identifier can be
    /// allocated. The current project is preserved on failure.
    pub fn discard_and_request_open(&mut self) -> Result<Vec<FrontendEffect>, AppError> {
        if self.pending_open.is_some() {
            return Err(AppError::OpenAlreadyPending);
        }
        if self.next_open_request == u64::MAX {
            return Err(AppError::OpenRequestOverflow);
        }
        let mut effects = self.discard_and_close(false);
        effects.extend(self.begin_open()?);
        Ok(effects)
    }

    fn request_close(&mut self, quit_after: bool) -> Result<Vec<FrontendEffect>, AppError> {
        self.require_no_pending_save()?;
        self.require_no_pending_open()?;
        let Some(project) = self.project.as_ref() else {
            return Ok(if quit_after {
                vec![FrontendEffect::QuitApplication]
            } else {
                Vec::new()
            });
        };
        if project.is_modified() {
            Ok(vec![FrontendEffect::ConfirmDiscardChanges { quit_after }])
        } else {
            Ok(self.discard_and_close(quit_after))
        }
    }

    pub(crate) fn require_no_pending_save(&self) -> Result<(), AppError> {
        if self.pending_save.is_some() {
            Err(AppError::SaveInProgress)
        } else {
            Ok(())
        }
    }

    fn require_no_pending_open(&self) -> Result<(), AppError> {
        if self.pending_open.is_some() {
            Err(AppError::OpenInProgress)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
