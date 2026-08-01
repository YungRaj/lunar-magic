use lm_oracle::CodecObservationKind;
use lm_rom::Mapper;
use std::path::PathBuf;

mod bitmap_imports;
mod oracle;
mod transfers;

pub use bitmap_imports::{PngMap16ImportCommand, RgbMap16ImportCommand, RgbaMap16ImportCommand};
pub use oracle::{OracleCaptureCommand, OracleOwnership};
pub use transfers::{
    ExAnimationTransferCommand, ExpandedSettingsTransferCommand, GraphicsMigrationCommand,
    GraphicsTransferCommand, LevelTransferCommand, Map16TransferCommand, OverworldTransferCommand,
    PaletteTransferCommand,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    DscSidecar {
        input: PathBuf,
        lossless_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    NativeMap16Sidecar {
        kind: NativeMap16SidecarKind,
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Lm16Map16File {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
    },
    Inspect(PathBuf),
    Profile {
        profile: PathBuf,
        rom: Option<PathBuf>,
    },
    ProfileExport {
        kind: ProfileExportKind,
        rom: PathBuf,
        profile: PathBuf,
        slot: usize,
        output: PathBuf,
    },
    ProfileImport {
        kind: ProfileImportKind,
        input_rom: PathBuf,
        output_rom: PathBuf,
        profile: PathBuf,
        slot: usize,
        asset: PathBuf,
        search_start: usize,
        search_end: usize,
    },
    RevisionPatchInstall {
        input_rom: PathBuf,
        output_rom: PathBuf,
        profile: PathBuf,
        template: PathBuf,
        search_start: usize,
        search_end: usize,
        fill: u8,
    },
    ExpandedSettingsInstall {
        input_rom: PathBuf,
        output_rom: PathBuf,
    },
    Map16RuntimeInstall {
        input_rom: PathBuf,
        output_rom: PathBuf,
    },
    Sprite19FixInstall {
        input_rom: PathBuf,
        output_rom: PathBuf,
    },
    SmwMap16CompleteExport {
        rom: PathBuf,
        template: Option<PathBuf>,
        output: PathBuf,
    },
    SmwMap16CompleteImport {
        input_rom: PathBuf,
        map16: PathBuf,
        output_rom: PathBuf,
    },
    Layer3Install {
        input_rom: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldPathExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldPathImport {
        input_rom: PathBuf,
        links: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldMessageExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldMessageInstall {
        input_rom: PathBuf,
        messages: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldEventExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldEventImport {
        input_rom: PathBuf,
        events: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldEventMapExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldEventMapImport {
        input_rom: PathBuf,
        event_map: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldTransferObserve {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldTransferFullObserve {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwTransferredMap16Observe {
        rom: PathBuf,
        output: PathBuf,
    },
    ExAnimationSlotOptionsObserve {
        rom: PathBuf,
        mapper: Mapper,
        pointer: usize,
        output: PathBuf,
    },
    SmwInstalledMap16RemapsObserve {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldSpecialEventExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldSpecialEventImport {
        input_rom: PathBuf,
        events: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldBossSequenceExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldBossSequenceImport {
        input_rom: PathBuf,
        messages: PathBuf,
        output_rom: PathBuf,
    },
    SmwCreditsTilemapExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwCreditsTilemapImport {
        input_rom: PathBuf,
        tilemap: PathBuf,
        output_rom: PathBuf,
    },
    SmwTitleTilemapExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwTitleTilemapImport {
        input_rom: PathBuf,
        tilemap: PathBuf,
        output_rom: PathBuf,
    },
    SmwTitleRecordingExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwTitleRecordingImport {
        input_rom: PathBuf,
        recording: PathBuf,
        output_rom: PathBuf,
    },
    SmwTitleRecordingZsnesExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwTitleRecordingZsnesImport {
        input_rom: PathBuf,
        state: PathBuf,
        output_rom: PathBuf,
    },
    SmwTitleRecordingSnes9xImport {
        input_rom: PathBuf,
        state: PathBuf,
        output_rom: PathBuf,
    },
    SmwLunarMagicMetadataExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwLunarMagicMetadataImport {
        input_rom: PathBuf,
        metadata: PathBuf,
        output_rom: PathBuf,
    },
    SmwSecondaryExitExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwSecondaryExitImport {
        input_rom: PathBuf,
        table: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldEventTilemapExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldEventTilemapImport {
        input_rom: PathBuf,
        tilemaps: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldWarpExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldWarpImport {
        input_rom: PathBuf,
        links: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldNameExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldNameImport {
        input_rom: PathBuf,
        names: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldSettingsExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldSettingsImport {
        input_rom: PathBuf,
        settings: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldLayer3SettingsObserve {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwSharedPaletteExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwSharedPaletteImport {
        input_rom: PathBuf,
        palette: PathBuf,
        output_rom: PathBuf,
    },
    SmwOverworldStartExport {
        rom: PathBuf,
        output: PathBuf,
    },
    SmwOverworldStartImport {
        input_rom: PathBuf,
        starts: PathBuf,
        output_rom: PathBuf,
    },
    RevisionPatchFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Rats(PathBuf),
    RatsObserve {
        rom: PathBuf,
        output: PathBuf,
    },
    RatsManifest {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    RatsPlan {
        rom: PathBuf,
        manifest: PathBuf,
        fill: u8,
    },
    RatsReclaim {
        input: PathBuf,
        output: PathBuf,
        manifest: PathBuf,
        fill: u8,
    },
    Mwl(PathBuf),
    MwlNormalize {
        input: PathBuf,
        output: PathBuf,
    },
    MwlObserve {
        input: PathBuf,
        output: PathBuf,
    },
    MwlObserveOptionalAssets {
        input: PathBuf,
        size_modes: PathBuf,
        maximum_records: usize,
        output: PathBuf,
    },
    MwlPaletteTpl {
        input: PathBuf,
        output: PathBuf,
    },
    MwlTransferOptionalAssets {
        source: PathBuf,
        target: PathBuf,
        size_modes: PathBuf,
        maximum_records: usize,
        output: PathBuf,
    },
    MwlEditOptionalAssets {
        input: PathBuf,
        size_modes: PathBuf,
        maximum_records: usize,
        edits: PathBuf,
        output: PathBuf,
    },
    MwlEditLayer3Settings {
        input: PathBuf,
        enabled: bool,
        file: u16,
        length_selector: u8,
        offset_selector: u8,
        output: PathBuf,
    },
    MwlObserveLayer3Settings {
        input: PathBuf,
        output: PathBuf,
    },
    MwlCorpus {
        root: PathBuf,
    },
    Level {
        rom: PathBuf,
        mapper: Mapper,
        number: usize,
        layer1_table: usize,
        sprite_table: usize,
        expanded_sprites: bool,
    },
    LevelSplitBank {
        rom: PathBuf,
        mapper: Mapper,
        number: usize,
        layer1_table: usize,
        sprite_low_table: usize,
        sprite_bank_table: usize,
        expanded_sprites: bool,
    },
    LevelLayer2 {
        rom: PathBuf,
        mapper: Mapper,
        number: usize,
        layer1_table: usize,
        layer2_table: usize,
        output: PathBuf,
    },
    CompleteLevel {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    EditCompleteLevel {
        input: PathBuf,
        script: PathBuf,
        output: PathBuf,
    },
    Layer3File {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Layer3WorkspaceApply {
        packed_descriptor: u16,
        workspace: PathBuf,
        decoded_graphics: PathBuf,
        output: PathBuf,
        observation: Option<PathBuf>,
    },
    GraphicsRemapFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    GraphicsRemapApply {
        stream: PathBuf,
        scratch: PathBuf,
        output: PathBuf,
        observation: Option<PathBuf>,
    },
    ExpandedSettingsFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    ExpandedSettingsLayer3 {
        input: PathBuf,
        enabled: bool,
        file: u16,
        length_selector: u8,
        offset_selector: u8,
        output: PathBuf,
    },
    NativeAssetsFile {
        input: PathBuf,
        profile: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Layer3PlaneFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    LayerTilemapFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    CreditsTilemapFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    OverworldEventFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    EditorOverlayFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    AnimationFrameFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    AppearanceFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    NativeLevelFile {
        input: PathBuf,
        sprite_lengths: Option<PathBuf>,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    OverworldAppearanceFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    CustomObjectLibrary {
        data: PathBuf,
        descriptions: PathBuf,
        normalized_outputs: Option<(PathBuf, PathBuf)>,
        observation: Option<PathBuf>,
    },
    CustomSpriteLibrary {
        data: PathBuf,
        descriptions: PathBuf,
        sprite_lengths: PathBuf,
        normalized_outputs: Option<(PathBuf, PathBuf)>,
        observation: Option<PathBuf>,
    },
    CompleteMap16 {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Map16PageFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    GraphicsFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    GraphicsOwnershipFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    PaletteFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    PaletteOwnershipFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    SmwPaletteFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    TplPaletteFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    RawPaletteFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    PaletteMaskFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    RgbPaletteFile {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    ExAnimationFile {
        input: PathBuf,
        size_modes: PathBuf,
        maximum_records: usize,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    RenderMap16Page {
        graphics: PathBuf,
        palette: PathBuf,
        page: PathBuf,
        output: PathBuf,
    },
    RenderMap16Dsc {
        graphics: PathBuf,
        palette: PathBuf,
        map16: PathBuf,
        dsc: PathBuf,
        page: usize,
        first_feature: bool,
        first_suppressed: bool,
        second_feature: bool,
        output: PathBuf,
    },
    RenderGraphics {
        graphics: PathBuf,
        palette: PathBuf,
        palette_row: usize,
        columns: usize,
        output: PathBuf,
    },
    RenderPalette {
        palette: PathBuf,
        columns: usize,
        cell_size: usize,
        output: PathBuf,
    },
    RenderLevel {
        level: PathBuf,
        map16: PathBuf,
        graphics: PathBuf,
        palette: PathBuf,
        appearances: Option<PathBuf>,
        layer3_plane: Option<PathBuf>,
        layer1_width: usize,
        layer1_height: usize,
        layer2_width: usize,
        layer2_height: usize,
        output: PathBuf,
    },
    RenderLevelDsc {
        level: PathBuf,
        map16: PathBuf,
        graphics: PathBuf,
        palette: PathBuf,
        appearances: Option<PathBuf>,
        layer3_plane: Option<PathBuf>,
        dsc: PathBuf,
        custom_display: bool,
        special_markers: bool,
        first_feature: bool,
        first_suppressed: bool,
        second_feature: bool,
        level_mode: u8,
        layer1_width: usize,
        layer1_height: usize,
        layer2_width: usize,
        layer2_height: usize,
        output: PathBuf,
    },
    RenderOverworld {
        overworld: PathBuf,
        size_modes: PathBuf,
        maximum_animation_records: usize,
        map16: PathBuf,
        graphics: PathBuf,
        appearances: Option<PathBuf>,
        animation_frame: Option<PathBuf>,
        completed_reveals: usize,
        output: PathBuf,
    },
    Map16 {
        rom: PathBuf,
        mapper: Mapper,
        page: usize,
        graphics_table: usize,
        acts_like_table: usize,
        observation: Option<PathBuf>,
    },
    Map16Transfer(Map16TransferCommand),
    GraphicsTransfer(GraphicsTransferCommand),
    GraphicsMigration(GraphicsMigrationCommand),
    LevelTransfer(LevelTransferCommand),
    PaletteTransfer(PaletteTransferCommand),
    ExAnimationTransfer(ExAnimationTransferCommand),
    OverworldTransfer(OverworldTransferCommand),
    ExpandedSettingsTransfer(ExpandedSettingsTransferCommand),
    OverworldPath {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    OverworldMetadata {
        input: PathBuf,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    OverworldFile {
        input: PathBuf,
        size_modes: PathBuf,
        maximum_animation_records: usize,
        normalized_output: Option<PathBuf>,
        observation: Option<PathBuf>,
    },
    Asset(AssetCommand),
    Address {
        mapper: Mapper,
        direction: Direction,
        value: u32,
    },
    Codec {
        operation: CodecOperation,
        input: PathBuf,
        output: PathBuf,
    },
    CodecSizedRleDecode {
        input: PathBuf,
        output: PathBuf,
        expected_len: usize,
    },
    CodecObserve {
        kind: CodecObservationKind,
        input: PathBuf,
        output_bound: usize,
        observation: PathBuf,
    },
    Planar {
        operation: PlanarOperation,
        bits_per_pixel: u8,
        input: PathBuf,
        output: PathBuf,
    },
    QuantizeRgb24 {
        input: PathBuf,
        maximum_colors: usize,
        palette_output: PathBuf,
        indices_output: PathBuf,
    },
    ImportIndexedMap16 {
        indices: PathBuf,
        graphics: PathBuf,
        occupancy: PathBuf,
        palette_row: u8,
        acts_like: u16,
        source_page: u16,
        graphics_output: PathBuf,
        occupancy_output: PathBuf,
        page_output: PathBuf,
    },
    ImportRgbMap16(RgbMap16ImportCommand),
    ImportRgbaMap16(RgbaMap16ImportCommand),
    ImportPngMap16(PngMap16ImportCommand),
    EditExAnimationFrames {
        input: PathBuf,
        size_modes: PathBuf,
        maximum_records: usize,
        record: usize,
        edits: PathBuf,
        output: PathBuf,
    },
    Diff {
        left: PathBuf,
        right: PathBuf,
    },
    OracleVerify {
        manifest: PathBuf,
        before: PathBuf,
        after: PathBuf,
        observations: Option<(PathBuf, PathBuf)>,
    },
    OracleVerifySuite {
        root: PathBuf,
    },
    OracleCoverage {
        root: PathBuf,
        requirements: Vec<String>,
    },
    OracleReleaseGate {
        root: PathBuf,
        requirements: Vec<String>,
    },
    OracleCapture(OracleCaptureCommand),
    Checksum {
        input: PathBuf,
        output: PathBuf,
        field_offset: usize,
    },
    ChecksumAuto {
        input: PathBuf,
        output: PathBuf,
    },
    RomExpand {
        input: PathBuf,
        output: PathBuf,
        mapper: Mapper,
        target_logical_len: usize,
        fill: u8,
    },
    CopierHeaderAdd {
        input: PathBuf,
        output: PathBuf,
        fill: u8,
    },
    CopierHeaderRemove {
        input: PathBuf,
        output: PathBuf,
    },
    Patch {
        input: PathBuf,
        output: PathBuf,
        offset: usize,
        bytes: Vec<u8>,
    },
    IpsApply {
        source: PathBuf,
        patch: PathBuf,
        output: PathBuf,
    },
    IpsCreate {
        before: PathBuf,
        after: PathBuf,
        output: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMap16SidecarKind {
    M16,
    S16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanarOperation {
    Decode,
    Encode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileExportKind {
    NativeAssets,
    Level,
    Layer2,
    Map16,
    Graphics,
    Palette,
    ExAnimation,
    ExpandedSettings,
    Overworld,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileImportKind {
    NativeAssets,
    Level,
    Map16,
    Graphics,
    Palette,
    ExAnimation,
    ExpandedSettings,
    Overworld,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetCommand {
    Graphics {
        rom: PathBuf,
        mapper: Mapper,
        file: usize,
        pointer_table: usize,
        maximum_compressed_len: usize,
        maximum_decompressed_len: usize,
        compression: lm_project::GraphicsCompression,
        observation: Option<PathBuf>,
    },
    Palette {
        rom: PathBuf,
        mapper: Mapper,
        index: usize,
        pointer_table: usize,
        colors: usize,
        observation: Option<PathBuf>,
    },
    ExAnimation {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        maximum_records: usize,
        maximum_encoded_len: usize,
        size_modes: PathBuf,
        observation: Option<PathBuf>,
    },
    OverworldMessages {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        count: usize,
        observation: Option<PathBuf>,
    },
    OverworldSprites {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        count: usize,
        record_len: usize,
        observation: Option<PathBuf>,
    },
    NativeCustomOverworldSprites {
        rom: PathBuf,
        mapper: Mapper,
        pointer: usize,
        record_sizes: PathBuf,
        observation: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    SnesToPc,
    PcToSnes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecOperation {
    Lz2Decode,
    Lz2Encode,
    Lz3Decode,
    Lz3Encode,
    RleDecode,
    RleEncode,
    RleSizedEncode,
}
