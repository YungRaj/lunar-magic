use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{
    DscDescriptionStyle, DscDisplayContext, DscResolvedTable, DscSidecar, MAX_DSC_SOURCE_LEN,
    Map16PageFile, Map16SetFile,
};
use lm_render::{encode_png, render_portable_map16_page, render_portable_map16_page_with_dsc};
use std::path::Path;

pub fn execute(
    graphics_path: &Path,
    palette_path: &Path,
    page_path: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if [graphics_path, palette_path, page_path].contains(&output) {
        return Err("render output must differ from every input".into());
    }
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        graphics_path,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        palette_path,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let page = Map16PageFile::decode(&crate::oracle_input::read_exact(
        page_path,
        Map16PageFile::ENCODED_LEN,
        "Map16 page",
    )?)?;
    let canvas = render_portable_map16_page(&graphics, &palette, &page)?;
    write_new(output, encode_png(&canvas)?)?;
    println!("width: {}", canvas.width());
    println!("height: {}", canvas.height());
    println!("output: {}", output.display());
    Ok(())
}

pub fn execute_dsc(
    graphics_path: &Path,
    palette_path: &Path,
    map16_path: &Path,
    dsc_path: &Path,
    page: usize,
    context: DscDisplayContext,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if [graphics_path, palette_path, map16_path, dsc_path].contains(&output) {
        return Err("DSC render output must differ from every input".into());
    }
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        graphics_path,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        palette_path,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let map16 = Map16SetFile::decode(&read_bounded(map16_path, Map16SetFile::MAX_FILE_LEN)?)?;
    let source = DscSidecar::decode(&read_bounded(dsc_path, MAX_DSC_SOURCE_LEN)?)?;
    let resolved = DscResolvedTable::from_sidecar(
        &source,
        DscDescriptionStyle {
            background: 0,
            detail: 0,
            foreground: 0,
            mode: 0,
        },
    );
    let canvas =
        render_portable_map16_page_with_dsc(&graphics, &palette, &map16, page, &resolved, context)?;
    write_new(output, encode_png(&canvas)?)?;
    println!("page: {page}");
    println!("DSC entries: {}", source.entries().len());
    println!("output: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use lm_level::{Map16Page, Map16Tile, Subtile};
    use lm_render::Rgba;

    const PALETTE_ROW_COLORS: usize = 16;
    const REQUIRED_PALETTE_ROWS: usize = 8;

    fn assets() -> (
        GraphicsInterchangeFile,
        PaletteInterchangeFile,
        Map16PageFile,
    ) {
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[0] = 1;
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new(pixels)],
            },
        };
        let mut colors = vec![Bgr555(0); PALETTE_ROW_COLORS * REQUIRED_PALETTE_ROWS];
        colors[1] = Bgr555(0x001f);
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette { colors },
        };
        let definition = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0x4000),
            bottom_left: Subtile(0x8000),
            bottom_right: Subtile(0xc000),
            acts_like: 0,
        };
        let page = Map16PageFile {
            source_page: 0,
            page: Map16Page::new(vec![definition; Map16Page::TILE_COUNT]).unwrap(),
        };
        (graphics, palette, page)
    }

    #[test]
    fn renders_exact_page_dimensions_transparency_color_and_flips() {
        let (graphics, palette, page) = assets();
        let canvas = render_portable_map16_page(&graphics, &palette, &page).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (256, 256));
        let red = Rgba {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        assert_eq!(canvas.get(0, 0), Some(red));
        assert_eq!(canvas.get(15, 0), Some(red));
        assert_eq!(canvas.get(0, 15), Some(red));
        assert_eq!(canvas.get(15, 15), Some(red));
        assert_eq!(canvas.get(1, 0), Some(Rgba::default()));
        let png = encode_png(&canvas).unwrap();
        assert_eq!(
            lm_oracle::sha256_hex(&png),
            "56da76e0b14295a8eb6869db018b99b9ef4260a6939099dbd421c5a1c3ccf74f"
        );
    }

    #[test]
    fn missing_graphics_and_short_or_misaligned_palettes_are_rejected() {
        let (mut graphics, mut palette, page) = assets();
        graphics.graphics.tiles.clear();
        assert!(render_portable_map16_page(&graphics, &palette, &page).is_err());
        palette.palette.colors.truncate(17);
        assert!(render_portable_map16_page(&assets().0, &palette, &page).is_err());
        palette.palette.colors.truncate(16);
        assert!(render_portable_map16_page(&assets().0, &palette, &page).is_err());
    }

    #[test]
    fn output_cannot_alias_any_input() {
        let path = Path::new("same.file");
        assert!(execute(path, path, path, path).is_err());
    }
}
