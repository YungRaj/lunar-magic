//! Deterministic software reference renderer.

mod canvas;
mod editor_overlay;
mod editor_overlay_file;
mod indexed;
mod layer3_plane;
mod level;
mod map16;
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
mod viewport;
mod viewport_raster;
mod viewport_scene;

pub use canvas::{Canvas, CanvasError, Rgba};
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
pub use map16::draw_map16_tile;
pub use observe_editor_overlays::observe_editor_overlays;
pub use observe_layer3_plane::observe_materialized_layer3_plane;
pub use overworld::{
    OverworldRenderError, SpriteAppearance, apply_event_changes, apply_event_reveals,
    build_overworld_scene, resolve_sprite_appearances,
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
pub use portable_overworld::{PortableOverworldRenderError, render_portable_overworld};
pub use portable_palette::{PortablePaletteRenderError, render_portable_palette};
pub use scene::{Scene, TileInstance, draw_scene};
pub use viewport::{Point, Viewport, ViewportError, WorldRect};
pub use viewport_raster::{ViewportRasterError, rasterize_canvas_viewport};
pub use viewport_scene::draw_scene_viewport;
