use lm_project::GraphicsCompression;
use lm_rom::Mapper;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        table_offset: usize,
        entries: usize,
        stride: usize,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        table_offset: usize,
        entries: usize,
        stride: usize,
        record: PathBuf,
        checksum_field: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        layout: PathBuf,
        size_modes: PathBuf,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        layout: PathBuf,
        size_modes: PathBuf,
        overworld_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        maximum_records: usize,
        maximum_encoded_len: usize,
        size_modes: PathBuf,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        maximum_records: usize,
        maximum_encoded_len: usize,
        size_modes: PathBuf,
        animation_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        palette: usize,
        pointer_table: usize,
        colors: usize,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        palette: usize,
        pointer_table: usize,
        colors: usize,
        palette_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        level: usize,
        layer1_table: usize,
        sprite_table: usize,
        expanded_sprites: bool,
        sprite_lengths: Option<PathBuf>,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        level: usize,
        layer1_table: usize,
        sprite_table: usize,
        expanded_sprites: bool,
        sprite_lengths: Option<PathBuf>,
        level_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16TransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        page: usize,
        graphics_table: usize,
        acts_like_table: usize,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        page: usize,
        graphics_table: usize,
        acts_like_table: usize,
        page_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsTransferCommand {
    Export {
        rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        maximum_compressed_len: usize,
        maximum_decompressed_len: usize,
        compression: GraphicsCompression,
        output: PathBuf,
    },
    Import {
        input_rom: PathBuf,
        output_rom: PathBuf,
        mapper: Mapper,
        slot: usize,
        pointer_table: usize,
        maximum_compressed_len: usize,
        maximum_decompressed_len: usize,
        compression: GraphicsCompression,
        graphics_file: PathBuf,
        checksum_field: usize,
        search_start: usize,
        search_end: usize,
        ownership_manifest: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsMigrationCommand {
    pub input_rom: PathBuf,
    pub output_rom: PathBuf,
    pub mapper: Mapper,
    pub pointer_table: usize,
    pub entries: usize,
    pub maximum_compressed_len: usize,
    pub maximum_decompressed_len: usize,
    pub source_compression: GraphicsCompression,
    pub target_compression: GraphicsCompression,
    pub checksum_field: usize,
    pub search_start: usize,
    pub search_end: usize,
}
