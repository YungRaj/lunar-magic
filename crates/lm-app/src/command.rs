use crate::{
    ClipboardPayload, EditorSelection, LevelNavigationDirection, LevelViewport, RevisionProfile,
};
use lm_graphics::SmwPaletteFile;
use lm_level::{ExpandedOverworldSettings, SecondaryExitTable};
use lm_overworld::{
    BossSequenceMessageTable, CreditsTilemap, EventNumberMap, EventRevealTable,
    EventTilemapBuffers, ExpandedLayerTilemap, NativeOverworldLevelNameTable,
    NativeOverworldPlayerStarts, OverworldLayer3SettingsTable, OverworldMessage,
    OverworldPathLinkTable, OverworldWarpLinkTable, SpecialEventRevealTable,
};
use lm_project::{
    GraphicsCompression, GraphicsMigrationOptions, GraphicsRomLayout, LevelAccessRestrictionKeys,
    RatsOwnershipManifest, RomMutation, RomWrite,
};
use lm_rom::{CopierHeader, LunarMagicRomMetadata, Mapper};
use lm_title::TitleScreenRecording;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Open,
    Save,
    SaveAs,
    Close,
    Quit,
    Undo,
    Redo,
    SelectLevel(u16),
    NavigateLevel(LevelNavigationDirection),
    SetLevelViewport(LevelViewport),
    ShowOverworld,
    ShowMap16,
    ShowGraphics(u16),
    ShowPalette(u16),
    ShowExAnimation(u16),
    ShowLayer3(u16),
    SetSelection(EditorSelection),
    ClearSelection,
    Copy(ClipboardPayload),
    Cut(ClipboardPayload),
    Paste(Vec<u8>),
    /// Atomically validates and installs external revision metadata for the open ROM.
    InstallRevisionProfile(Box<RevisionProfile>),
    /// Removes the active revision metadata and invalidates decoded controller state.
    ClearRevisionProfile,
    /// Commits serializer-produced ROM writes as one project history entry.
    CommitRomWrites {
        expected_revision: u64,
        description: String,
        writes: Vec<RomWrite>,
    },
    /// Commits serializer-produced writes plus an optional logical ROM tail as one history entry.
    CommitRomMutation {
        expected_revision: u64,
        description: String,
        mutation: RomMutation,
    },
    /// Atomically recompresses every native graphics slot as one project history operation.
    MigrateGraphicsCompression {
        expected_revision: u64,
        source: GraphicsRomLayout,
        target: GraphicsCompression,
        options: GraphicsMigrationOptions,
    },
    /// Installs an identity-bound, address-independent runtime template as one project revision.
    InstallRevisionPatch {
        expected_revision: u64,
        template: Box<lm_profile::RevisionPatchTemplate>,
        search: Range<usize>,
        fill: u8,
    },
    /// Installs the built-in SMW US revision-0 expanded-settings runtime and table.
    InstallSettings {
        rev: u64,
    },
    /// Installs the complete built-in SMW US revision-0 Layer 3 runtime family.
    InstallLayer3 {
        rev: u64,
    },
    /// Installs the recovered SMW US revision-0 Lfix3 core runtime and shared tables.
    InstallLfix3 {
        rev: u64,
    },
    /// Installs the recovered pristine SMW US revision-0 Map16 runtime and auxiliary table.
    InstallMap16Runtime {
        rev: u64,
    },
    /// Migrates an authenticated Lunar Magic Layer 2 format-$102 runtime to format `$103`.
    InstallLayer2Runtime {
        rev: u64,
    },
    /// Installs Lunar Magic's recovered user-requested sprite `$19` ASM fix.
    InstallSprite19Fix {
        rev: u64,
    },
    /// Installs Lunar Magic's recovered fixed-location level support patch B.
    InstallSupportPatchB {
        rev: u64,
    },
    InstallExpandedSharedPalettes {
        rev: u64,
    },
    /// Erases only manifest-owned, non-retained RATS allocations and repairs the checksum.
    ReclaimOwnedRats {
        rev: u64,
        manifest: Box<RatsOwnershipManifest>,
        fill: u8,
    },
    /// Applies one bounded IPS patch to the logical ROM as an undoable project replacement.
    ApplyIpsPatch {
        rev: u64,
        patch: Vec<u8>,
    },
    /// Adds or removes the physical 512-byte copier header without changing logical ROM bytes.
    SetCopierHeader {
        rev: u64,
        target: CopierHeader,
        fill: u8,
    },
    /// Adds or replaces the physical prefix with Lunar Magic 3.63's canonical SMW-US header.
    SetLunarMagicSmwUsCopierHeader {
        rev: u64,
    },
    /// Replaces the fixed native SMW US revision-0 path-link planes.
    ReplaceNativeOverworldPathLinks {
        rev: u64,
        table: Box<OverworldPathLinkTable>,
    },
    /// Replaces or installs Lunar Magic's expanded native overworld-message table.
    ReplaceNativeOverworldMessages {
        rev: u64,
        messages: Vec<OverworldMessage>,
    },
    /// Replaces the complete native main overworld event-reveal table.
    ReplaceNativeOverworldEventReveals {
        rev: u64,
        table: Box<EventRevealTable>,
    },
    /// Replaces or installs Lunar Magic's overworld event-number mapping.
    ReplaceNativeOverworldEventNumberMap {
        rev: u64,
        map: Box<EventNumberMap>,
    },
    /// Replaces all 24 native special-event reveal records and directions.
    ReplaceNativeSpecialEventReveals {
        rev: u64,
        table: Box<SpecialEventRevealTable>,
    },
    /// Replaces or installs both compressed native overworld event-tilemap streams.
    ReplaceNativeOverworldEventTilemaps {
        rev: u64,
        buffers: Box<EventTilemapBuffers>,
    },
    /// Replaces all seven native overworld boss-sequence messages.
    ReplaceNativeOverworldBossSequence {
        rev: u64,
        table: Box<BossSequenceMessageTable>,
    },
    /// Replaces the complete 256×32 credits editor tilemap.
    ReplaceNativeCreditsTilemap {
        rev: u64,
        tilemap: Box<CreditsTilemap>,
    },
    /// Replaces or installs the title-screen Layer 3 tilemap.
    ReplaceNativeTitleTilemap {
        rev: u64,
        tilemap: Box<ExpandedLayerTilemap>,
    },
    /// Installs or replaces Lunar Magic's title-screen playback recording.
    ReplaceNativeTitleRecording {
        rev: u64,
        recording: TitleScreenRecording,
    },
    /// Replaces Lunar Magic's fixed attribution, VRAM version, and packed feature record.
    ReplaceLunarMagicRomMetadata {
        rev: u64,
        metadata: Box<LunarMagicRomMetadata>,
    },
    /// Replaces Lunar Magic's complete native expanded secondary-exit table.
    ReplaceNativeSecondaryExits {
        rev: u64,
        table: Box<SecondaryExitTable>,
    },
    /// Replaces the fixed native SMW US revision-0 warp/exit coordinate planes.
    ReplaceNativeOverworldWarpLinks {
        rev: u64,
        table: Box<OverworldWarpLinkTable>,
    },
    /// Replaces or installs Lunar Magic-compatible native overworld level names.
    ReplaceNativeOverworldLevelNames {
        rev: u64,
        table: Box<NativeOverworldLevelNameTable>,
    },
    /// Replaces or installs the seven exact Lunar Magic expanded overworld-setting records.
    ReplaceNativeOverworldSettings {
        rev: u64,
        settings: Box<ExpandedOverworldSettings>,
    },
    /// Replaces the same seven records through their recovered Layer 3 semantic view.
    ReplaceNativeOverworldLayer3Settings {
        rev: u64,
        settings: Box<OverworldLayer3SettingsTable>,
    },
    /// Replaces the marker-selected fixed shared palette table.
    ReplaceNativeSharedPalette {
        rev: u64,
        palette: Box<SmwPaletteFile>,
    },
    /// Replaces both exact native SMW overworld player-start records.
    ReplaceNativeOverworldPlayerStarts {
        rev: u64,
        starts: Box<NativeOverworldPlayerStarts>,
    },
    /// Expands the logical ROM and repairs its checksum as one project history operation.
    ExpandRom(RomExpansionCommand),
    /// Permanently applies Lunar Magic's level-access restriction migration.
    RestrictLevelAccess {
        rev: u64,
        title: String,
        keys: LevelAccessRestrictionKeys,
    },
    RunExternalTool(String),
    /// Stages the current in-memory ROM revision and launches a configured emulator tool.
    TestRomInEmulator(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomExpansionCommand {
    pub expected_revision: u64,
    pub mapper: Mapper,
    pub target_logical_len: usize,
    pub fill: u8,
    pub checksum_field: usize,
}
