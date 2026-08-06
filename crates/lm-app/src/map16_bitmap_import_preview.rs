//! Revision-independent staged state for the native bitmap import preview.

use crate::{
    Map16BitmapImportError, Map16BitmapImportOptions, Map16BitmapImportPlan,
    Map16BitmapImportRequest,
};
use lm_graphics::{GraphicsFile4bpp, GraphicsOwnership, Palette, PaletteOwnership, Rgba8};

/// Owned immutable source domains from which every preview revision is recomputed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16BitmapImportInputs {
    pub pixels: Vec<Rgba8>,
    pub width: usize,
    pub height: usize,
    pub palette_row: u8,
    pub acts_like: u16,
    pub palette: Palette,
    pub palette_ownership: PaletteOwnership,
    pub graphics: GraphicsFile4bpp,
    pub graphics_ownership: GraphicsOwnership,
    pub occupied: Vec<bool>,
}

impl Map16BitmapImportInputs {
    fn request(&self) -> Map16BitmapImportRequest<'_> {
        Map16BitmapImportRequest {
            pixels: &self.pixels,
            width: self.width,
            height: self.height,
            palette_row: self.palette_row,
            acts_like: self.acts_like,
            palette: &self.palette,
            palette_ownership: &self.palette_ownership,
            graphics: &self.graphics,
            graphics_ownership: &self.graphics_ownership,
            occupied: &self.occupied,
        }
    }
}

/// One synchronized original/converted preview and the exact plan awaiting acceptance.
///
/// Option changes always recompute from `inputs`, never from the preceding plan. Failed changes
/// preserve both the accepted options and pixels, preventing cumulative quantization or partial
/// graphics allocation while a dialog is being edited.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16BitmapImportPreviewState {
    inputs: Map16BitmapImportInputs,
    options: Map16BitmapImportOptions,
    plan: Map16BitmapImportPlan,
    converted_pixels: Vec<Rgba8>,
}

impl Map16BitmapImportPreviewState {
    /// Builds the initial synchronized preview.
    ///
    /// # Errors
    ///
    /// Returns the shared import planner's validation or resource-exhaustion error.
    pub fn new(
        inputs: Map16BitmapImportInputs,
        options: Map16BitmapImportOptions,
    ) -> Result<Self, Map16BitmapImportError> {
        let plan = Map16BitmapImportPlan::prepare_with_options(inputs.request(), options.clone())?;
        let converted_pixels = plan.converted_pixels();
        Ok(Self {
            inputs,
            options,
            plan,
            converted_pixels,
        })
    }

    #[must_use]
    pub const fn inputs(&self) -> &Map16BitmapImportInputs {
        &self.inputs
    }

    #[must_use]
    pub fn options(&self) -> Map16BitmapImportOptions {
        self.options.clone()
    }

    #[must_use]
    pub const fn plan(&self) -> &Map16BitmapImportPlan {
        &self.plan
    }

    #[must_use]
    pub fn original_pixels(&self) -> &[Rgba8] {
        &self.inputs.pixels
    }

    #[must_use]
    pub fn converted_pixels(&self) -> &[Rgba8] {
        &self.converted_pixels
    }

    /// Recomputes all staged domains and both preview panes from immutable source inputs.
    ///
    /// # Errors
    ///
    /// A rejected option combination leaves this state byte-for-byte unchanged.
    pub fn set_options(
        &mut self,
        options: Map16BitmapImportOptions,
    ) -> Result<(), Map16BitmapImportError> {
        if options == self.options {
            return Ok(());
        }
        let plan =
            Map16BitmapImportPlan::prepare_with_options(self.inputs.request(), options.clone())?;
        let converted_pixels = plan.converted_pixels();
        self.options = options;
        self.plan = plan;
        self.converted_pixels = converted_pixels;
        Ok(())
    }

    /// Recomputes the preview with a replacement palette-ownership map.
    ///
    /// A rejected ownership shape or color allocation leaves the accepted preview and inputs
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns the shared import planner's validation or resource error.
    pub fn set_palette_ownership(
        &mut self,
        ownership: PaletteOwnership,
    ) -> Result<(), Map16BitmapImportError> {
        if ownership == self.inputs.palette_ownership {
            return Ok(());
        }
        let mut inputs = self.inputs.clone();
        inputs.palette_ownership = ownership;
        let plan =
            Map16BitmapImportPlan::prepare_with_options(inputs.request(), self.options.clone())?;
        let converted_pixels = plan.converted_pixels();
        self.inputs = inputs;
        self.plan = plan;
        self.converted_pixels = converted_pixels;
        Ok(())
    }

    /// Consumes the dialog state and returns the exact plan that was previewed.
    #[must_use]
    pub fn accept(self) -> Map16BitmapImportPlan {
        self.plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsTileOwner, IndexedBitmapImportOptions, IndexedTile};

    fn inputs() -> Map16BitmapImportInputs {
        let mut pixels = Vec::with_capacity(crate::MAP16_BITMAP_PIXELS);
        for y in 0..crate::MAP16_BITMAP_HEIGHT {
            for x in 0..crate::MAP16_BITMAP_WIDTH {
                let red = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 0 };
                pixels.push(Rgba8 {
                    red,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                });
            }
        }
        Map16BitmapImportInputs {
            pixels,
            width: crate::MAP16_BITMAP_WIDTH,
            height: crate::MAP16_BITMAP_HEIGHT,
            palette_row: 2,
            acts_like: 0x130,
            palette: Palette {
                colors: vec![Bgr555(0); 128],
            },
            palette_ownership: PaletteOwnership::editable(128),
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; 64]); 0x400],
            },
            graphics_ownership: GraphicsOwnership::from_owners(vec![
                GraphicsTileOwner::Editable;
                0x400
            ]),
            occupied: vec![false; 0x400],
        }
    }

    fn options() -> Map16BitmapImportOptions {
        Map16BitmapImportOptions {
            graphics: IndexedBitmapImportOptions {
                allocation_start: 0,
                allocation_end: 0x400,
                reuse_existing_tiles: true,
                optimize_new_tiles: true,
                allow_flipped_matches: true,
                blank_tile: None,
            },
            color: None,
            deduplicate_map16: true,
            use_reserved_map16_for_blank: false,
            reserved_map16_tile: 0,
            map16_allocation_start: 0,
            layer_priority: false,
        }
    }

    #[test]
    fn option_changes_recompute_the_exact_accepted_preview() {
        let mut preview = Map16BitmapImportPreviewState::new(inputs(), options()).unwrap();
        assert_eq!(preview.plan().newly_occupied_tiles, 2);
        let mut changed = options();
        changed.graphics.optimize_new_tiles = false;
        preview.set_options(changed.clone()).unwrap();
        assert_eq!(preview.plan().newly_occupied_tiles, 0x400);
        changed.layer_priority = true;
        preview.set_options(changed).unwrap();
        assert!(
            preview
                .plan()
                .page
                .tiles
                .iter()
                .all(|tile| tile.top_left.0 & 0x2000 != 0)
        );
        let converted = preview.converted_pixels().to_vec();
        let accepted = preview.accept();
        assert_eq!(accepted.converted_pixels(), converted);
    }

    #[test]
    fn failed_option_update_preserves_every_preview_domain() {
        let mut preview = Map16BitmapImportPreviewState::new(inputs(), options()).unwrap();
        let before = preview.clone();
        let mut invalid = options();
        invalid.graphics.allocation_end = 0x401;
        assert!(preview.set_options(invalid).is_err());
        assert_eq!(preview, before);
    }

    #[test]
    fn fixed_palette_ownership_recomputes_without_overwriting_reserved_color() {
        let mut preview = Map16BitmapImportPreviewState::new(inputs(), options()).unwrap();
        let retained = preview.inputs().palette.colors[33];
        let mut ownership = PaletteOwnership::editable(128);
        ownership
            .set_owner(33, lm_graphics::PaletteEntryOwner::Fixed)
            .unwrap();
        preview.set_palette_ownership(ownership).unwrap();
        assert_eq!(preview.plan().palette.colors[33], retained);
        assert_eq!(
            preview.inputs().palette_ownership.owner(33),
            Some(lm_graphics::PaletteEntryOwner::Fixed)
        );
    }
}
