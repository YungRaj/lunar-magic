use lm_app::{
    Map16BitmapAllocationMode, Map16BitmapAllocationOptions, Map16BitmapImportOptions,
    Map16BitmapImportPlan, Map16BitmapImportRequest, allocate_bitmap_map16_tiles_sequential_grid,
    allocate_bitmap_map16_tiles_with_reserved_sources, decode_map16_bitmap_image,
};
use lm_graphics::{
    Bgr555, BitmapPaletteColorOptions, BitmapPaletteEntryState, BitmapPaletteReduction,
    GraphicsFile4bpp, GraphicsOwnership, Palette, PaletteOwnership, Rgb8,
};
use lm_level::{Map16Tile, Subtile};
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
    let effective_palette = fs::read(capture_dir.join("palette-effective.rgb32"))
        .unwrap_or_else(|_| palette_before.clone());
    let palette_after = fs::read(capture_dir.join("palette-after.rgb32")).unwrap();
    let graphics_before = fs::read(capture_dir.join("graphics-before.bin")).unwrap();
    let graphics_after = fs::read(capture_dir.join("graphics-after.bin")).unwrap();
    let map16_before = fs::read(capture_dir.join("map16-definitions-before.bin")).unwrap();
    let map16_after = fs::read(capture_dir.join("map16-definitions-after.bin")).unwrap();
    assert_eq!(palette_before.len(), 0x400);
    assert_eq!(effective_palette.len(), 0x400);
    assert_eq!(palette_after.len(), 0x400);
    assert!(graphics_before.len() >= NATIVE_GRAPHICS_BYTES);
    assert!(graphics_after.len() >= NATIVE_GRAPHICS_BYTES);
    assert_eq!(map16_before.len(), 0x8_0000);
    assert_eq!(map16_after.len(), 0x8_0000);

    let palette = decode_rgb32_palette(&effective_palette);
    let graphics = GraphicsFile4bpp::decode(&graphics_before[..NATIVE_GRAPHICS_BYTES]).unwrap();
    let occupied = graphics
        .tiles
        .iter()
        .map(|tile| tile.pixels().iter().any(|pixel| *pixel != 0))
        .collect::<Vec<_>>();
    let bitmap = decode_map16_bitmap_image(&fs::read(source).unwrap()).unwrap();
    let options = options_from_manifest(&manifest, capture_dir);
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

    let rust_palette = plan.palette.colors[..32].to_vec();
    let original_palette = decode_rgb32_palette(&palette_after).colors[..32].to_vec();
    assert_eq!(rust_palette, original_palette, "native palette differs");

    let rust_graphics = plan.graphics.encode().unwrap();
    let expected_graphics = &graphics_after[..NATIVE_GRAPHICS_BYTES];
    let graphics_differences = rust_graphics
        .iter()
        .zip(expected_graphics)
        .enumerate()
        .filter_map(|(offset, (rust, original))| {
            (rust != original).then_some((offset, *rust, *original))
        })
        .collect::<Vec<_>>();
    assert!(
        graphics_differences.is_empty(),
        "the native $000-$2FF graphics workspace has {} differences; first (offset, Rust, original) triples: {:X?}",
        graphics_differences.len(),
        &graphics_differences[..graphics_differences.len().min(16)],
    );

    let mut expected_definitions = decode_map16_definitions(&map16_before);
    let reserved_sources = options
        .use_reserved_map16_for_blank
        .then(|| {
            plan.map16_tiles
                .iter()
                .map(|tile| {
                    [
                        tile.top_left,
                        tile.top_right,
                        tile.bottom_left,
                        tile.bottom_right,
                    ]
                    .iter()
                    .all(|subtile| {
                        plan.graphics
                            .tiles
                            .get(usize::from(subtile.tile_number()))
                            .is_some_and(|tile| tile.pixels().iter().all(|pixel| *pixel == 0))
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let definition_count = expected_definitions.len();
    let allocation_end = if options.map16_allocation_start < 0x8000 {
        definition_count.min(0x8000)
    } else {
        definition_count.min((options.map16_allocation_start & !0x0fff).saturating_add(0x1000))
    };
    let allocation_options = Map16BitmapAllocationOptions {
        start: options.map16_allocation_start,
        end: allocation_end,
        reserved: options.reserved_map16_tile,
        mode: if options.deduplicate_map16 {
            Map16BitmapAllocationMode::Deduplicated
        } else {
            Map16BitmapAllocationMode::Sequential
        },
    };
    let allocation = if options.deduplicate_map16 {
        allocate_bitmap_map16_tiles_with_reserved_sources(
            &mut expected_definitions,
            &plan.map16_tiles,
            &reserved_sources,
            allocation_options,
        )
    } else {
        allocate_bitmap_map16_tiles_sequential_grid(
            &mut expected_definitions,
            &plan.map16_tiles,
            plan.width_in_map16_tiles,
            allocation_options,
        )
    }
    .unwrap();
    assert!(!allocation.exhausted);
    let expected_map16 = encode_map16_definitions(&expected_definitions);
    let map16_differences = expected_map16
        .iter()
        .zip(&map16_after)
        .enumerate()
        .filter_map(|(offset, (expected, actual))| {
            (expected != actual).then_some((offset, *expected, *actual))
        })
        .collect::<Vec<_>>();
    assert!(
        map16_differences.is_empty(),
        "native complete Map16 definition workspace differs at {} bytes; first differences: {:?}",
        map16_differences.len(),
        &map16_differences[..map16_differences.len().min(64)]
    );
}

fn decode_map16_definitions(bytes: &[u8]) -> Vec<Map16Tile> {
    bytes
        .chunks_exact(8)
        .map(|tile| Map16Tile {
            top_left: Subtile(u16::from_le_bytes([tile[0], tile[1]])),
            // Lunar Magic's live definition workspace is column-major even
            // though its ROM and file codecs are row-major.
            bottom_left: Subtile(u16::from_le_bytes([tile[2], tile[3]])),
            top_right: Subtile(u16::from_le_bytes([tile[4], tile[5]])),
            bottom_right: Subtile(u16::from_le_bytes([tile[6], tile[7]])),
            acts_like: 0,
        })
        .collect()
}

fn encode_map16_definitions(definitions: &[Map16Tile]) -> Vec<u8> {
    definitions
        .iter()
        .flat_map(|tile| {
            [
                tile.top_left.0.to_le_bytes(),
                tile.bottom_left.0.to_le_bytes(),
                tile.top_right.0.to_le_bytes(),
                tile.bottom_right.0.to_le_bytes(),
            ]
            .into_iter()
            .flatten()
        })
        .collect()
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

fn options_from_manifest(
    values: &BTreeMap<String, String>,
    capture_dir: &Path,
) -> Map16BitmapImportOptions {
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
    color.entries = fs::read(capture_dir.join("palette-entry-states.bin"))
        .expect("capture the effective Lunar Magic palette-entry map")
        .into_iter()
        .map(|state| match state {
            0 => BitmapPaletteEntryState::Free,
            2 => BitmapPaletteEntryState::Reserved,
            4 => BitmapPaletteEntryState::Reusable,
            value => panic!("unsupported captured palette-entry state {value:#04x}"),
        })
        .collect();
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
