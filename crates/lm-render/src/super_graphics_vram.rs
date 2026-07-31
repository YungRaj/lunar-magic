use lm_graphics::IndexedTile;
use lm_project::LoadedSuperGraphicsBypass;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedSuperGraphicsVram {
    /// Six consecutive 0x1000-byte native slots: FG1, FG2, FG3, BG1, BG2, BG3.
    pub foreground_background: Vec<IndexedTile>,
    /// Four consecutive 0x1000-byte native slots: SP1, SP2, SP3, SP4.
    pub sprites: Vec<IndexedTile>,
}

/// Materializes Lunar Magic's decoded Super GFX files into native 128-tile VRAM slot order.
#[must_use]
pub fn materialize_super_graphics_vram(
    loaded: &LoadedSuperGraphicsBypass,
) -> MaterializedSuperGraphicsVram {
    MaterializedSuperGraphicsVram {
        foreground_background: loaded
            .foreground_background
            .iter()
            .flat_map(|slot| slot.tiles.iter().cloned())
            .collect(),
        sprites: loaded
            .sprites
            .iter()
            .flat_map(|slot| slot.tiles.iter().cloned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NativeLevelRasterRequest, NativeMap16Placement, Rgba, render_native_level_framebuffer,
    };
    use lm_graphics::{Bgr555, Palette};
    use lm_level::{Map16Tile, Subtile, SuperGraphicsBypass};
    use lm_project::{LoadedSuperGraphicsBypass, LoadedSuperGraphicsSlot};

    fn slot(file_number: u16, color: u8) -> LoadedSuperGraphicsSlot {
        LoadedSuperGraphicsSlot {
            file_number,
            bits_per_pixel: 4,
            tiles: vec![IndexedTile::new([color; IndexedTile::PIXEL_COUNT]); 128],
        }
    }

    #[test]
    fn bypass_slots_materialize_in_native_order_and_drive_framebuffer_pixels() {
        let loaded = LoadedSuperGraphicsBypass {
            selection: SuperGraphicsBypass {
                enabled: true,
                foreground_background: [0, 1, 2, 3, 4, 5],
                sprites: [6, 7, 8, 9],
            },
            foreground_background: (0..6)
                .map(|slot_index| slot(slot_index, u8::try_from(slot_index + 1).unwrap()))
                .collect(),
            sprites: (6..10).map(|slot_index| slot(slot_index, 1)).collect(),
        };
        let vram = materialize_super_graphics_vram(&loaded);
        assert_eq!(vram.foreground_background.len(), 6 * 128);
        assert_eq!(vram.sprites.len(), 4 * 128);
        assert_eq!(vram.foreground_background[0].pixels()[0], 1);
        assert_eq!(vram.foreground_background[128].pixels()[0], 2);

        let definition = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0),
            bottom_left: Subtile(0),
            bottom_right: Subtile(0),
            acts_like: 0,
        };
        let placement = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
        }];
        let layers: [&[NativeMap16Placement]; 1] = [&placement];
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 16,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop: Rgba::default(),
            layers: &layers,
            definitions: &[definition],
            tiles: &vram.foreground_background,
            palette: &Palette { colors },
        })
        .unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
    }
}
