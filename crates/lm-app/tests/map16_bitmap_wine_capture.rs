use lm_app::{
    Map16BitmapImportOptions, Map16BitmapImportPlan, Map16BitmapImportRequest,
    decode_map16_bitmap_image,
};
use lm_graphics::{
    Bgr555, BitmapPaletteColorOptions, BitmapPaletteEntryState, BitmapPaletteReduction,
    GraphicsFile4bpp, GraphicsOwnership, Palette, PaletteOwnership, Rgb8,
};
use std::{collections::BTreeMap, fs, path::Path};

const NATIVE_GRAPHICS_TILES: usize = 0x300;
const NATIVE_GRAPHICS_BYTES: usize = NATIVE_GRAPHICS_TILES * GraphicsFile4bpp::BYTES_PER_TILE;

#[test]
#[ignore = "requires a disposable Lunar Magic/Wine bitmap-import capture"]
fn lunar_magic_bitmap_capture_matches_rust_palette_and_graphics() {
    let capture_dir = std::env::var_os("LM_BITMAP_CAPTURE_DIR")
        .expect("set LM_BITMAP_CAPTURE_DIR to an audit output directory");
    let source = std::env::var_os("LM_BITMAP_SOURCE")
        .expect("set LM_BITMAP_SOURCE to the exact bitmap used by the audit");
    let capture_dir = Path::new(&capture_dir);
    let manifest = parse_manifest(&fs::read_to_string(capture_dir.join("manifest.tsv")).unwrap());

    let palette_before = fs::read(capture_dir.join("palette-before.rgb32")).unwrap();
    let palette_after = fs::read(capture_dir.join("palette-after.rgb32")).unwrap();
    let graphics_before = fs::read(capture_dir.join("graphics-before.bin")).unwrap();
    let graphics_after = fs::read(capture_dir.join("graphics-after.bin")).unwrap();
    assert_eq!(palette_before.len(), 0x400);
    assert_eq!(palette_after.len(), 0x400);
    assert!(graphics_before.len() >= NATIVE_GRAPHICS_BYTES);
    assert!(graphics_after.len() >= NATIVE_GRAPHICS_BYTES);

    let palette = decode_rgb32_palette(&palette_before);
    let graphics = GraphicsFile4bpp::decode(&graphics_before[..NATIVE_GRAPHICS_BYTES]).unwrap();
    let occupied = graphics
        .tiles
        .iter()
        .map(|tile| tile.pixels().iter().any(|pixel| *pixel != 0))
        .collect::<Vec<_>>();
    let bitmap = decode_map16_bitmap_image(&fs::read(source).unwrap()).unwrap();
    let options = options_from_manifest(&manifest);
    let plan = Map16BitmapImportPlan::prepare_with_options(
        Map16BitmapImportRequest {
            pixels: &bitmap.pixels,
            width: bitmap.width,
            height: bitmap.height,
            palette_row: 4,
            acts_like: 0,
            palette: &palette,
            palette_ownership: &PaletteOwnership::editable(palette.colors.len()),
            graphics: &graphics,
            graphics_ownership: &GraphicsOwnership::editable(graphics.tiles.len()),
            occupied: &occupied,
        },
        options.clone(),
    )
    .unwrap();

    let color_options = options.color.clone().unwrap();
    let free_indices = color_options
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, state)| (*state == BitmapPaletteEntryState::Free).then_some(index))
        .collect::<Vec<_>>();
    let rust_palette = free_indices
        .iter()
        .map(|index| plan.palette.colors[*index].to_rgb8())
        .collect::<Vec<_>>();
    let original_palette = free_indices
        .iter()
        .map(|index| rgb32_color(&palette_after, *index))
        .collect::<Vec<_>>();
    assert_eq!(
        rust_palette, original_palette,
        "writable palette entries differ"
    );

    let rust_graphics = plan.graphics.encode().unwrap();
    let expected_graphics = &graphics_after[..NATIVE_GRAPHICS_BYTES];
    let graphics_differences = rust_graphics
        .iter()
        .zip(expected_graphics)
        .enumerate()
        .filter_map(|(offset, (rust, original))| (rust != original).then_some(offset))
        .collect::<Vec<_>>();
    assert!(
        graphics_differences.is_empty(),
        "the native $000-$2FF graphics workspace has {} differences; first offsets: {:X?}",
        graphics_differences.len(),
        &graphics_differences[..graphics_differences.len().min(16)],
    );
}

fn parse_manifest(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .skip(1)
        .map(|line| line.split_once('\t').expect("malformed manifest row"))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn manifest<'a>(values: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    values
        .get(key)
        .unwrap_or_else(|| panic!("missing {key}"))
        .as_str()
}

fn flag(values: &BTreeMap<String, String>, key: &str) -> bool {
    match manifest(values, key) {
        "0" => false,
        "1" => true,
        value => panic!("invalid {key} flag {value}"),
    }
}

fn decimal(values: &BTreeMap<String, String>, key: &str) -> usize {
    manifest(values, key).parse().unwrap()
}

fn hexadecimal(values: &BTreeMap<String, String>, key: &str) -> usize {
    usize::from_str_radix(manifest(values, key), 16).unwrap()
}

fn options_from_manifest(values: &BTreeMap<String, String>) -> Map16BitmapImportOptions {
    let mut options = lm_app::native_map16_bitmap_import_options();
    options.graphics.allocation_start = hexadecimal(values, "requested_first_8x8_tile_hex");
    options.graphics.optimize_new_tiles = flag(values, "optimize_8x8");
    options.graphics.reuse_existing_tiles = flag(values, "reuse_existing_8x8");
    options.graphics.blank_tile =
        flag(values, "use_blank_8x8").then(|| hexadecimal(values, "requested_blank_8x8_tile_hex"));
    options.deduplicate_map16 = flag(values, "optimize_16x16");
    options.layer_priority = flag(values, "requested_layer_priority");
    options.use_reserved_map16_for_blank = flag(values, "use_blank_16x16");
    options.reserved_map16_tile = hexadecimal(values, "requested_blank_map16_tile_hex");
    options.map16_allocation_start = hexadecimal(values, "requested_first_map16_tile_hex");

    let mut color = BitmapPaletteColorOptions::lunar_magic_initial();
    color.reduction = match manifest(values, "reduction") {
        "median-cut" => BitmapPaletteReduction::MedianCut,
        "popularity" => BitmapPaletteReduction::Popularity,
        value => panic!("invalid reduction {value}"),
    };
    color.priority_level = u8::try_from(decimal(values, "priority")).unwrap();
    color.maximum_colors = decimal(values, "maximum_colors");
    color.prioritize_unique_colors = flag(values, "unique_colors");
    color.maintain_detail = flag(values, "maintain_detail");
    color.popularity_reduction_method_1 = flag(values, "reduction_method_1");
    color.popularity_reduction_method_2 = flag(values, "reduction_method_2");
    color.allow_modifying_unmarked_colors = flag(values, "allow_unmarked");
    color.prioritize_exact_palette_matches = flag(values, "exact_matches");
    options.color = Some(color);
    options
}

fn decode_rgb32_palette(bytes: &[u8]) -> Palette {
    Palette {
        colors: bytes
            .chunks_exact(4)
            .map(|entry| {
                Bgr555::from_rgb8(Rgb8 {
                    red: entry[2],
                    green: entry[1],
                    blue: entry[0],
                })
            })
            .collect(),
    }
}

fn rgb32_color(bytes: &[u8], index: usize) -> Rgb8 {
    let offset = index * 4;
    Rgb8 {
        red: bytes[offset + 2],
        green: bytes[offset + 1],
        blue: bytes[offset],
    }
}
