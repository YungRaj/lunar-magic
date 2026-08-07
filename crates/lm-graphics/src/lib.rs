//! SNES graphics, palette, and animation models.

mod bitmap_palette;
mod color_map;
mod container;
mod editing;
mod exanimation;
mod exanimation_features;
mod exanimation_file;
mod exanimation_frames;
mod exanimation_preview;
mod exanimation_slot_options;
mod external_sprite_assets;
mod file;
mod graphics_editing;
mod materialized_frame;
mod palette;
mod palette_editing;
mod palette_file;
mod palette_import;
mod planar;
mod quantizer;
mod raw_palette_file;
mod remap_stream;
mod rgb_palette_file;
mod smw_palette_file;
mod tile;
mod tile_import;
mod tpl_palette_file;

pub use bitmap_palette::{
    BITMAP_PALETTE_COLORS, BITMAP_PALETTE_ROWS, BitmapPaletteColorOptions, BitmapPaletteEntryState,
    BitmapPaletteReduction, BitmapPaletteReductionError, MultiRowBitmapPalette,
    ReducedBitmapPalette, allocate_bitmap_palette_rows, reduce_bitmap_palette,
    reduce_bitmap_palette_with_palette,
};
pub use color_map::{GraphicsColorMapError, GraphicsColorMapFilters};
pub use container::{GraphicsFile4bpp, GraphicsFileError, JoinedGraphics};
pub use editing::ExAnimationEditError;
pub use exanimation::{CompactExAnimation, ExAnimationError, ExAnimationRecord, ExAnimationSet};
pub use exanimation_features::{ExAnimationFeature, ExAnimationFeatureOptions};
pub use exanimation_file::{CompactExAnimationFile, CompactExAnimationFileError};
pub use exanimation_frames::{
    ExAnimationFrame, ExAnimationFrameEdit, ExAnimationFrameEditError, edit_exanimation_frames,
    exanimation_frames,
};
pub use exanimation_preview::{
    ExAnimationPreviewState, ExAnimationTriggerPreviewState, SelectedExAnimationFrame,
};
pub use exanimation_slot_options::{
    EXANIMATION_LEVEL_SLOT_COUNT, ExAnimationSlotOptionError, ExAnimationSlotOptionTable,
    ExAnimationSlotOptions,
};
pub use external_sprite_assets::{
    EXTERNAL_SPRITE_GRAPHICS_BASE_TILE, EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES,
    EXTERNAL_SPRITE_GRAPHICS_SLOTS, EXTERNAL_SPRITE_GRAPHICS_TILES_PER_SLOT,
    EXTERNAL_SPRITE_PALETTE_COLORS, EXTERNAL_SPRITE_PALETTE_RGB_MAX_BYTES,
    EXTERNAL_SPRITE_PALETTE_ROWS, EXTERNAL_SPRITE_PALETTE_SNES_MAX_BYTES, ExternalSpriteAssets,
    ExternalSpriteAssetsError,
};
pub use file::{GraphicsInterchangeError, GraphicsInterchangeFile};
pub use graphics_editing::{
    EquivalentTile, GraphicsEditError, GraphicsOwnership, GraphicsTileChange, GraphicsTileOwner,
};
pub use materialized_frame::{
    MaterializedAnimationFrame, MaterializedFrameError, MaterializedPaletteOverride,
    MaterializedTileOverride,
};
pub use palette::{Bgr555, Palette, PaletteEditError, PaletteEncodingError, Rgb8};
pub use palette_editing::{
    PaletteBatchEditError, PaletteChange, PaletteEntryOwner, PaletteOwnership,
};
pub use palette_file::{PaletteInterchangeError, PaletteInterchangeFile};
pub use palette_import::{
    OpaquePaletteRowImport, PaletteImportError, Rgba8, TransparentPaletteRowImport,
};
pub use planar::{
    PlanarGraphicsError, decode_4bpp_tile, decode_planar_tile, decode_planar_tiles,
    encode_4bpp_tile, encode_planar_tile, encode_planar_tiles,
};
pub use quantizer::{QuantizedImage, QuantizerError, WuQuantizer};
pub use raw_palette_file::{
    PaletteMaskFile, RawPaletteFileError, RawSnesPaletteFile, apply_raw_palette_import,
};
pub use remap_stream::{
    DecodedGraphicsRemapCommandStream, GRAPHICS_REMAP_MAX_PREFIX_LEN, GRAPHICS_REMAP_STREAM_LIMIT,
    GRAPHICS_REMAP_WORDS, GraphicsRemapCommand, GraphicsRemapCommandStream, GraphicsRemapEnd,
    GraphicsRemapError, GraphicsRemapPayload, GraphicsRemapStride,
};
pub use rgb_palette_file::{RgbChannelExpansion, RgbPaletteFile, RgbPaletteFileError};
pub use smw_palette_file::{SmwPaletteBackend, SmwPaletteFile, SmwPaletteFileError};
pub use tile::{IndexedTile, TileEditError, TileShift};
pub use tile_import::{
    BitmapImportError, ImportedTilePlacement, IndexedBitmapImport, IndexedBitmapImportOptions,
};
pub use tpl_palette_file::{TplPaletteFile, TplPaletteFileError};
