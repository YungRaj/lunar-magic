//! Deterministic software reference renderer.

mod bmp;
mod canvas;
mod custom_object_renderer;
mod custom_sprite_renderer;
mod editor_overlay;
mod editor_overlay_file;
mod indexed;
mod layer3_plane;
mod level;
mod level_map16_cache;
mod map16;
mod native_level_raster;
mod native_overworld_sprite;
mod observe_editor_overlays;
mod observe_layer3_plane;
mod overworld;
mod png;
mod portable_graphics;
mod portable_level;
mod portable_level_dsc;
mod portable_map16;
mod portable_map16_dsc;
mod portable_overworld;
mod portable_palette;
mod scene;
mod standard_object_renderer;
mod standard_sprite_renderer;
mod super_graphics_vram;
mod viewport;
mod viewport_raster;
mod viewport_scene;

pub use bmp::{BmpError, encode_bmp};
pub use canvas::{Canvas, CanvasError, Rgba};
pub use custom_object_renderer::{
    CustomObjectPreviewTile, render_lunar_magic_custom_object,
    render_resolved_lunar_magic_custom_object,
};
pub use custom_sprite_renderer::{
    RemappedCustomSpritePreviewTile, raster_external_custom_sprite_tile,
    raster_remapped_custom_sprite_tile_with, render_atlas_lunar_magic_custom_sprite_with,
    render_lunar_magic_custom_sprite, render_remapped_lunar_magic_custom_sprite_with,
    render_resolved_lunar_magic_custom_sprite, render_resolved_lunar_magic_custom_sprite_with,
};
pub use editor_overlay::{
    EditorOverlay, EditorOverlayError, GridOverlay, SelectionOverlay, draw_editor_overlays,
    validate_editor_overlays,
};
pub use editor_overlay_file::{EditorOverlayFile, EditorOverlayFileError};
pub use indexed::{draw_indexed_tile, draw_indexed_tile_clipped};
pub use layer3_plane::{Layer3Placement, MaterializedLayer3Error, MaterializedLayer3Plane};
pub use level::{
    EntityAppearance, EntitySource, GridPlacement, LevelRenderError, LevelSceneLayout,
    build_level_scene, build_level_scene_with_layer3, resolve_entity_appearances,
};
pub use level_map16_cache::{
    LEVEL_MAP16_CACHE_CELLS, LEVEL_MAP16_CACHE_SENTINEL, NativeLevelMap16Cache,
    NativeLevelMap16CacheError, NativeLevelMap16Layout, NativeLevelMap16Write,
};
pub use map16::draw_map16_tile;
pub use native_level_raster::{
    NativeLevelRasterError, NativeLevelRasterRequest, NativeMap16Composition,
    NativeMap16DefinitionBank, NativeMap16PaletteRouting, NativeMap16Placement,
    draw_lunar_magic_editor_label, draw_lunar_magic_editor_node_text_lines,
    draw_native_level_layers_with_layer_palette_routing,
    draw_native_level_layers_with_layer_palette_routing_and_addition,
    draw_native_sprite_preview_definition, draw_native_sprite_preview_definition_pages,
    draw_native_sprite_preview_definition_pages_with_half_color, render_native_level_framebuffer,
    render_native_level_framebuffer_with_layer_palette_routing,
};
pub use native_overworld_sprite::{
    NativeOverworldAppearanceConversionError, NativeOverworldAppearancePair,
    NativeOverworldSpritePlacement, NativeOverworldSpriteResourceMap,
    ResolvedNativeOverworldSpriteElement, draw_resolved_native_overworld_sprite_elements,
    export_native_overworld_appearances, import_native_overworld_appearances,
    lunar_magic_builtin_overworld_sprite_map16, resolve_native_overworld_sprite_elements,
};
pub use observe_editor_overlays::observe_editor_overlays;
pub use observe_layer3_plane::observe_materialized_layer3_plane;
pub use overworld::{
    OverworldRenderError, SpriteAppearance, apply_event_changes, apply_event_reveals,
    build_overworld_layer_scene, build_overworld_scene, resolve_sprite_appearances,
};
pub use png::{PngError, encode_png};
pub use portable_graphics::{PortableGraphicsRenderError, render_portable_graphics};
pub use portable_level::{
    PortableLevelRenderDimensions, PortableLevelRenderError, render_portable_level,
};
pub use portable_level_dsc::{
    PortableDscLevelRenderError, PortableDscLevelRenderRequest, render_portable_level_with_dsc,
};
pub use portable_map16::{PortableMap16RenderError, render_portable_map16_page};
pub use portable_map16_dsc::{PortableDscMap16RenderError, render_portable_map16_page_with_dsc};
pub use portable_overworld::{
    PortableOverworldRenderError, render_portable_overworld, render_portable_overworld_layer,
    render_smw_overworld_layer2_tilemap,
};
pub use portable_palette::{PortablePaletteRenderError, render_portable_palette};
pub use scene::{Scene, TileInstance, draw_scene};
pub use standard_object_renderer::{
    STANDARD_OBJECT_COMMANDS, StandardObjectDefinitionSet, StandardObjectPaintedCell,
    StandardObjectPattern, StandardObjectRenderError, StandardObjectRenderReport,
    StandardObjectResizeModel, install_lunar_magic_shared_extended_objects,
    install_lunar_magic_shared_standard_objects, install_lunar_magic_tileset_extended_objects,
    lunar_magic_conditional_extended_object_tile, lunar_magic_shared_extended_object_tile,
    render_mapped_standard_object_placement, render_mapped_standard_object_stream,
    render_standard_object_stream,
};
pub use standard_sprite_renderer::{
    StandardLevelOrientation, StandardSpritePreviewMode, StandardSpritePreviewSource,
    StandardSpritePreviewTile, StandardSpriteWideContext,
    lunar_magic_standard_sprite_preview_source, render_lunar_magic_standard_sprite,
    render_lunar_magic_standard_sprite_with_mode,
};
pub use super_graphics_vram::{MaterializedSuperGraphicsVram, materialize_super_graphics_vram};
pub use viewport::{Point, Viewport, ViewportError, WorldRect};
pub use viewport_raster::{ViewportRasterError, rasterize_canvas_viewport};
pub use viewport_scene::draw_scene_viewport;
