use lm_app::{ClipboardPayload, NativeMap16Clipboard};
use lm_graphics::{Bgr555, ExAnimationFrame, ExAnimationRecord, IndexedTile};
use lm_level::{Map16Tile, ObjectRecord, SpriteRecord};
use lm_overworld::{OverworldMessage, OverworldSprite, SpriteAppearancePart};

const PREFIX: &str = "LMCLIP1:";
const NATIVE_MAP16_PREFIX: &str = "LM16TILES1:";

pub(crate) fn encode(payload: &ClipboardPayload) -> Result<String, String> {
    let bytes = payload.encode().map_err(|error| error.to_string())?;
    encode_bytes(&bytes)
}

pub(crate) fn encode_bytes(bytes: &[u8]) -> Result<String, String> {
    ClipboardPayload::decode(bytes).map_err(|error| error.to_string())?;
    let hex_len = bytes
        .len()
        .checked_mul(2)
        .and_then(|length| length.checked_add(PREFIX.len()))
        .ok_or_else(|| "clipboard text length overflow".to_string())?;
    let mut text = String::with_capacity(hex_len);
    text.push_str(PREFIX);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02X}").expect("writing to a String cannot fail");
    }
    Ok(text)
}

pub(crate) fn decode(text: &str) -> Result<ClipboardPayload, String> {
    let hex = text
        .strip_prefix(PREFIX)
        .ok_or_else(|| "clipboard does not contain a typed Lunar Magic payload".to_string())?;
    let maximum_hex = ClipboardPayload::MAX_ENCODED_LEN
        .checked_mul(2)
        .ok_or_else(|| "clipboard length bound overflow".to_string())?;
    if hex.len() > maximum_hex {
        return Err("clipboard payload exceeds its encoded length bound".into());
    }
    if hex.len() % 2 != 0 {
        return Err("clipboard payload contains a partial hexadecimal byte".into());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let text = std::str::from_utf8(pair)
            .map_err(|_| "clipboard payload contains non-ASCII hexadecimal data".to_string())?;
        bytes.push(
            u8::from_str_radix(text, 16)
                .map_err(|_| "clipboard payload contains invalid hexadecimal data".to_string())?,
        );
    }
    ClipboardPayload::decode(&bytes).map_err(|error| error.to_string())
}

pub(crate) fn encode_palette_color(color: Bgr555) -> Result<String, String> {
    encode(&ClipboardPayload::from_palette_colors(&[color]))
}

pub(crate) fn encode_palette_row(colors: &[Bgr555]) -> Result<String, String> {
    if colors.len() != 16 {
        return Err("palette-row copy requires exactly 16 colors".into());
    }
    encode(&ClipboardPayload::from_palette_colors(colors))
}

pub(crate) fn decode_palette_color(text: &str) -> Result<Bgr555, String> {
    let colors = decode(text)?
        .to_palette_colors()
        .map_err(|error| error.to_string())?;
    let [color] = colors.as_slice() else {
        return Err("palette paste requires exactly one color".into());
    };
    Ok(*color)
}

pub(crate) fn decode_palette_row(text: &str) -> Result<[Bgr555; 16], String> {
    let colors = decode(text)?
        .to_palette_colors()
        .map_err(|error| error.to_string())?;
    colors
        .try_into()
        .map_err(|_| "palette-row paste requires exactly 16 colors".to_string())
}

pub(crate) fn copy_palette_color_to_system(
    context: &eframe::egui::Context,
    color: Bgr555,
) -> Result<(), String> {
    let fallback = encode_palette_color(color)?;
    #[cfg(windows)]
    {
        lm_windows::write_palette_color_clipboard(
            &encode_lunar_magic_palette_color(color),
            &fallback,
        )
    }
    #[cfg(not(windows))]
    {
        context.copy_text(fallback);
        Ok(())
    }
}

pub(crate) fn copy_palette_row_to_system(
    context: &eframe::egui::Context,
    colors: &[Bgr555],
) -> Result<(), String> {
    let fallback = encode_palette_row(colors)?;
    #[cfg(windows)]
    {
        let colors = <[Bgr555; 16]>::try_from(colors)
            .map_err(|_| "palette-row copy requires exactly 16 colors".to_string())?;
        lm_windows::write_palette_row_clipboard(&encode_lunar_magic_palette_row(&colors), &fallback)
    }
    #[cfg(not(windows))]
    {
        context.copy_text(fallback);
        Ok(())
    }
}

pub(crate) fn request_palette_color_paste(
    context: &eframe::egui::Context,
) -> Result<Option<Bgr555>, String> {
    #[cfg(windows)]
    if let Some(bytes) = lm_windows::read_palette_color_clipboard()? {
        return decode_lunar_magic_palette_color(&bytes).map(Some);
    }
    context.send_viewport_cmd(eframe::egui::ViewportCommand::RequestPaste);
    Ok(None)
}

pub(crate) fn request_palette_row_paste(
    context: &eframe::egui::Context,
) -> Result<Option<[Bgr555; 16]>, String> {
    #[cfg(windows)]
    if let Some(bytes) = lm_windows::read_palette_row_clipboard()? {
        return decode_lunar_magic_palette_row(&bytes).map(Some);
    }
    context.send_viewport_cmd(eframe::egui::ViewportCommand::RequestPaste);
    Ok(None)
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn encode_lunar_magic_palette_color(color: Bgr555) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[4..8].copy_from_slice(&lunar_magic_rgb(color).to_le_bytes());
    bytes[8..12].copy_from_slice(&u32::from(color.0 & 0x7fff).to_le_bytes());
    bytes
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn decode_lunar_magic_palette_color(bytes: &[u8]) -> Result<Bgr555, String> {
    let record = bytes
        .get(..12)
        .ok_or_else(|| "Lunar Magic Color V2 data is shorter than 12 bytes".to_string())?;
    let flags = u32::from_le_bytes(record[..4].try_into().expect("four-byte flag"));
    if flags & 1 == 0 {
        return decode_snes_color_dword(&record[8..12]);
    }
    Ok(rgb_dword_to_bgr555(u32::from_le_bytes(
        record[4..8].try_into().expect("four-byte RGB value"),
    )))
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn encode_lunar_magic_palette_row(colors: &[Bgr555; 16]) -> [u8; 132] {
    let mut bytes = [0; 132];
    for (index, color) in colors.iter().copied().enumerate() {
        let rgb_start = 4 + index * 4;
        bytes[rgb_start..rgb_start + 4].copy_from_slice(&lunar_magic_rgb(color).to_le_bytes());
        let snes_start = 68 + index * 4;
        bytes[snes_start..snes_start + 4]
            .copy_from_slice(&u32::from(color.0 & 0x7fff).to_le_bytes());
    }
    bytes
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn decode_lunar_magic_palette_row(bytes: &[u8]) -> Result<[Bgr555; 16], String> {
    let record = bytes
        .get(..132)
        .ok_or_else(|| "Lunar Magic Color Row V2 data is shorter than 132 bytes".to_string())?;
    let flags = u32::from_le_bytes(record[..4].try_into().expect("four-byte flag"));
    let mut colors = [Bgr555(0); 16];
    for (index, color) in colors.iter_mut().enumerate() {
        let start = if flags & 1 == 0 {
            68 + index * 4
        } else {
            4 + index * 4
        };
        *color = if flags & 1 == 0 {
            decode_snes_color_dword(&record[start..start + 4])
        } else {
            Ok(rgb_dword_to_bgr555(u32::from_le_bytes(
                record[start..start + 4]
                    .try_into()
                    .expect("four-byte RGB value"),
            )))
        }?;
    }
    Ok(colors)
}

fn decode_snes_color_dword(bytes: &[u8]) -> Result<Bgr555, String> {
    let value = u32::from_le_bytes(bytes.try_into().expect("four-byte SNES value"));
    Ok(Bgr555((value & 0x7fff) as u16))
}

fn lunar_magic_rgb(color: Bgr555) -> u32 {
    let rgb = color.to_rgb8();
    u32::from(rgb.blue) | (u32::from(rgb.green) << 8) | (u32::from(rgb.red) << 16)
}

fn rgb_dword_to_bgr555(value: u32) -> Bgr555 {
    let five_bit = |channel: u8| {
        (0_u16..=0x1f)
            .min_by_key(|candidate| {
                let expanded = ((*candidate << 3) | (*candidate >> 2)) as i16;
                (
                    (i16::from(channel) - expanded).unsigned_abs(),
                    0x1f - *candidate,
                )
            })
            .expect("the five-bit color range is nonempty")
    };
    let red = five_bit((value >> 16) as u8);
    let green = five_bit((value >> 8) as u8);
    let blue = five_bit(value as u8);
    Bgr555(red | green << 5 | blue << 10)
}

pub(crate) fn encode_graphics_tile(tile: &IndexedTile) -> Result<String, String> {
    encode(&ClipboardPayload::from_graphics_tiles(
        std::slice::from_ref(tile),
    ))
}

pub(crate) fn decode_graphics_tile(text: &str) -> Result<IndexedTile, String> {
    let tiles = decode(text)?
        .to_graphics_tiles()
        .map_err(|error| error.to_string())?;
    let [tile] = tiles.as_slice() else {
        return Err("graphics paste requires exactly one tile".into());
    };
    Ok(tile.clone())
}

pub(crate) fn copy_graphics_tile_to_system(
    context: &eframe::egui::Context,
    tile: &IndexedTile,
) -> Result<(), String> {
    let fallback = encode_graphics_tile(tile)?;
    #[cfg(windows)]
    {
        let native = encode_lunar_magic_graphics_tile(tile);
        lm_windows::write_graphics_tile_clipboard(&native, &fallback)
    }
    #[cfg(not(windows))]
    {
        context.copy_text(fallback);
        Ok(())
    }
}

pub(crate) fn request_graphics_tile_paste(
    context: &eframe::egui::Context,
) -> Result<Option<IndexedTile>, String> {
    #[cfg(windows)]
    if let Some(bytes) = lm_windows::read_graphics_tile_clipboard()? {
        return decode_lunar_magic_graphics_tile(&bytes).map(Some);
    }
    context.send_viewport_cmd(eframe::egui::ViewportCommand::RequestPaste);
    Ok(None)
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn encode_lunar_magic_graphics_tile(tile: &IndexedTile) -> [u8; 64] {
    *tile.pixels()
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn decode_lunar_magic_graphics_tile(bytes: &[u8]) -> Result<IndexedTile, String> {
    let pixels = bytes
        .get(..IndexedTile::PIXEL_COUNT)
        .ok_or_else(|| "Lunar Magic graphics clipboard data is shorter than 64 bytes".to_owned())?;
    if let Some(color) = pixels.iter().copied().find(|color| *color > 0x0f) {
        return Err(format!(
            "Lunar Magic graphics clipboard contains invalid color {color:02X}"
        ));
    }
    let pixels = <[u8; IndexedTile::PIXEL_COUNT]>::try_from(pixels)
        .expect("the exact 64-byte slice was checked above");
    Ok(IndexedTile::new(pixels))
}

pub(crate) fn encode_map16_tile(tile: Map16Tile) -> Result<String, String> {
    encode(&ClipboardPayload::from_map16_tiles(&[tile]))
}

pub(crate) fn decode_map16_tile(text: &str) -> Result<Map16Tile, String> {
    let tiles = decode(text)?
        .to_map16_tiles()
        .map_err(|error| error.to_string())?;
    let [tile] = tiles.as_slice() else {
        return Err("Map16 paste requires exactly one tile".into());
    };
    Ok(*tile)
}

pub(crate) fn copy_map16_tile_to_system(
    context: &eframe::egui::Context,
    tile: Map16Tile,
) -> Result<(), String> {
    let fallback = encode_map16_tile(tile)?;
    #[cfg(windows)]
    {
        let native = encode_lunar_magic_map16_tile(tile);
        lm_windows::write_map16_tile_clipboard(&native, &fallback)
    }
    #[cfg(not(windows))]
    {
        context.copy_text(fallback);
        Ok(())
    }
}

pub(crate) fn request_map16_tile_paste(
    context: &eframe::egui::Context,
) -> Result<Option<Map16Tile>, String> {
    #[cfg(windows)]
    if let Some(bytes) = lm_windows::read_map16_tile_clipboard()? {
        return decode_lunar_magic_map16_tile(&bytes).map(Some);
    }
    context.send_viewport_cmd(eframe::egui::ViewportCommand::RequestPaste);
    Ok(None)
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn encode_lunar_magic_map16_tile(tile: Map16Tile) -> [u8; 10] {
    let mut bytes = [0; 10];
    bytes[..Map16Tile::GRAPHICS_LEN].copy_from_slice(&tile.encode_graphics());
    bytes[Map16Tile::GRAPHICS_LEN..].copy_from_slice(&tile.acts_like.to_le_bytes());
    bytes
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn decode_lunar_magic_map16_tile(bytes: &[u8]) -> Result<Map16Tile, String> {
    let record = bytes
        .get(..10)
        .ok_or_else(|| "Lunar Magic Map16 clipboard data is shorter than 10 bytes".to_owned())?;
    let acts_like = u16::from_le_bytes([record[8], record[9]]);
    Map16Tile::decode(&record[..8], acts_like).map_err(|error| error.to_string())
}

pub(crate) fn encode_native_map16_rectangle(
    rectangle: &NativeMap16Clipboard,
) -> Result<String, String> {
    let bytes = rectangle.encode().map_err(|error| error.to_string())?;
    encode_hex(NATIVE_MAP16_PREFIX, &bytes)
}

pub(crate) fn decode_native_map16_rectangle(text: &str) -> Result<NativeMap16Clipboard, String> {
    let bytes = decode_hex(NATIVE_MAP16_PREFIX, text, ClipboardPayload::MAX_ENCODED_LEN)?;
    NativeMap16Clipboard::decode(&bytes).map_err(|error| error.to_string())
}

fn encode_hex(prefix: &str, bytes: &[u8]) -> Result<String, String> {
    let hex_len = bytes
        .len()
        .checked_mul(2)
        .and_then(|length| length.checked_add(prefix.len()))
        .ok_or_else(|| "clipboard text length overflow".to_string())?;
    let mut text = String::with_capacity(hex_len);
    text.push_str(prefix);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02X}").expect("writing to a String cannot fail");
    }
    Ok(text)
}

fn decode_hex(prefix: &str, text: &str, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let hex = text.strip_prefix(prefix).ok_or_else(|| {
        "clipboard does not contain the requested Lunar Magic payload".to_string()
    })?;
    let maximum_hex = maximum_bytes
        .checked_mul(2)
        .ok_or_else(|| "clipboard length bound overflow".to_string())?;
    if hex.len() > maximum_hex {
        return Err("clipboard payload exceeds its encoded length bound".into());
    }
    if hex.len() % 2 != 0 {
        return Err("clipboard payload contains a partial hexadecimal byte".into());
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)
                .map_err(|_| "clipboard payload contains non-ASCII hexadecimal data".to_string())?;
            u8::from_str_radix(text, 16)
                .map_err(|_| "clipboard payload contains invalid hexadecimal data".to_string())
        })
        .collect()
}

pub(crate) fn encode_level_object(object: &ObjectRecord) -> Result<String, String> {
    encode(&ClipboardPayload::from_level_objects(std::slice::from_ref(
        object,
    )))
}

pub(crate) fn decode_level_object(text: &str) -> Result<ObjectRecord, String> {
    let objects = decode(text)?
        .to_level_objects()
        .map_err(|error| error.to_string())?;
    let [object] = objects.as_slice() else {
        return Err("level-object paste requires exactly one object".into());
    };
    Ok(object.clone())
}

pub(crate) fn encode_level_sprite(sprite: &SpriteRecord) -> Result<String, String> {
    encode_level_sprites(std::slice::from_ref(sprite))
}

pub(crate) fn encode_level_sprites(sprites: &[SpriteRecord]) -> Result<String, String> {
    encode(&ClipboardPayload::from_level_sprites(sprites))
}

pub(crate) fn decode_level_sprite(text: &str) -> Result<SpriteRecord, String> {
    let sprites = decode_level_sprites(text)?;
    let [sprite] = sprites.as_slice() else {
        return Err("level-sprite paste requires exactly one sprite".into());
    };
    Ok(sprite.clone())
}

pub(crate) fn decode_level_sprites(text: &str) -> Result<Vec<SpriteRecord>, String> {
    decode(text)?
        .to_level_sprites()
        .map_err(|error| error.to_string())
}

pub(crate) fn encode_layer2_tilemap_selection(
    width: u8,
    height: u8,
    words: &[u16],
) -> Result<String, String> {
    let payload = ClipboardPayload::from_layer2_tilemap_selection(width, height, words)
        .map_err(|error| error.to_string())?;
    encode(&payload)
}

pub(crate) fn decode_layer2_tilemap_selection(text: &str) -> Result<(u8, u8, Vec<u16>), String> {
    decode(text)?
        .to_layer2_tilemap_selection()
        .map_err(|error| error.to_string())
}

pub(crate) fn encode_layer3_tilemap(bytes: &[u8]) -> Result<String, String> {
    encode(&ClipboardPayload::from_layer3_tilemap_bytes(bytes))
}

pub(crate) fn decode_layer3_tilemap(text: &str) -> Result<Vec<u8>, String> {
    decode(text)?
        .to_layer3_tilemap_bytes()
        .map_err(|error| error.to_string())
}

pub(crate) fn encode_layer3_remap(bytes: &[u8]) -> Result<String, String> {
    encode(&ClipboardPayload::from_layer3_remap_bytes(bytes))
}

pub(crate) fn decode_layer3_remap(text: &str) -> Result<Vec<u8>, String> {
    decode(text)?
        .to_layer3_remap_bytes()
        .map_err(|error| error.to_string())
}

pub(crate) fn encode_exanimation_record(record: &ExAnimationRecord) -> Result<String, String> {
    encode(&ClipboardPayload::from_exanimation_records(
        std::slice::from_ref(record),
    ))
}

pub(crate) fn decode_exanimation_record(text: &str) -> Result<ExAnimationRecord, String> {
    let records = decode(text)?
        .to_exanimation_records()
        .map_err(|error| error.to_string())?;
    let [record] = records.as_slice() else {
        return Err("ExAnimation paste requires exactly one record".into());
    };
    Ok(record.clone())
}

pub(crate) fn encode_exanimation_frame(frame: &ExAnimationFrame) -> Result<String, String> {
    let payload = ClipboardPayload::from_exanimation_frames(std::slice::from_ref(frame))
        .map_err(|error| error.to_string())?;
    encode(&payload)
}

pub(crate) fn decode_exanimation_frame(text: &str) -> Result<ExAnimationFrame, String> {
    let frames = decode(text)?
        .to_exanimation_frames()
        .map_err(|error| error.to_string())?;
    let [frame] = frames.as_slice() else {
        return Err("ExAnimation frame paste requires exactly one frame".into());
    };
    Ok(frame.clone())
}

pub(crate) fn encode_overworld_sprite(sprite: &OverworldSprite) -> Result<String, String> {
    let payload = ClipboardPayload::from_overworld_sprites(std::slice::from_ref(sprite))
        .map_err(|error| error.to_string())?;
    encode(&payload)
}

pub(crate) fn decode_overworld_sprite(text: &str) -> Result<OverworldSprite, String> {
    let sprites = decode(text)?
        .to_overworld_sprites()
        .map_err(|error| error.to_string())?;
    let [sprite] = sprites.as_slice() else {
        return Err("overworld paste requires exactly one sprite".into());
    };
    Ok(sprite.clone())
}

pub(crate) fn encode_overworld_message(message: &OverworldMessage) -> Result<String, String> {
    encode(&ClipboardPayload::from_overworld_messages(
        std::slice::from_ref(message),
    ))
}

pub(crate) fn decode_overworld_message(text: &str) -> Result<OverworldMessage, String> {
    let messages = decode(text)?
        .to_overworld_messages()
        .map_err(|error| error.to_string())?;
    let [message] = messages.as_slice() else {
        return Err("overworld paste requires exactly one message".into());
    };
    Ok(message.clone())
}

pub(crate) fn encode_overworld_appearance_part(
    part: SpriteAppearancePart,
) -> Result<String, String> {
    encode_overworld_appearance_parts(&[part])
}

pub(crate) fn encode_overworld_appearance_parts(
    parts: &[SpriteAppearancePart],
) -> Result<String, String> {
    if parts.is_empty() {
        return Err("overworld appearance copy requires at least one part".into());
    }
    let payload = ClipboardPayload::from_overworld_appearance_parts(parts)
        .map_err(|error| error.to_string())?;
    encode(&payload)
}

pub(crate) fn decode_overworld_appearance_part(text: &str) -> Result<SpriteAppearancePart, String> {
    let parts = decode_overworld_appearance_parts(text)?;
    let [part] = parts.as_slice() else {
        return Err("overworld appearance paste requires exactly one part".into());
    };
    Ok(*part)
}

pub(crate) fn decode_overworld_appearance_parts(
    text: &str,
) -> Result<Vec<SpriteAppearancePart>, String> {
    let parts = decode(text)?
        .to_overworld_appearance_parts()
        .map_err(|error| error.to_string())?;
    if parts.is_empty() {
        return Err("overworld appearance paste requires at least one part".into());
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::ClipboardKind;

    #[test]
    fn text_envelope_round_trips_the_exact_typed_payload() {
        let payload = ClipboardPayload::new(
            ClipboardKind::PaletteColors,
            vec![vec![0x34, 0x12], vec![0xcd, 0xab]],
        )
        .unwrap();
        assert_eq!(decode(&encode(&payload).unwrap()).unwrap(), payload);
    }

    #[test]
    fn encoded_payload_bytes_can_be_forwarded_to_the_native_clipboard() {
        let payload =
            ClipboardPayload::new(ClipboardKind::PaletteColors, vec![vec![0x34, 0x12]]).unwrap();
        let bytes = payload.encode().unwrap();
        let text = encode_bytes(&bytes).unwrap();

        assert_eq!(decode(&text).unwrap(), payload);
    }

    #[test]
    fn arbitrary_bytes_are_not_exposed_as_a_typed_clipboard_envelope() {
        assert!(encode_bytes(b"not a clipboard payload").is_err());
    }

    #[test]
    fn malformed_or_untyped_text_is_rejected() {
        assert!(decode("ordinary text").is_err());
        assert!(decode("LMCLIP1:0").is_err());
        assert!(decode("LMCLIP1:GG").is_err());
    }

    #[test]
    fn lunar_magic_native_graphics_tile_is_the_exact_first_64_pixels() {
        let tile = IndexedTile::new(std::array::from_fn(|index| {
            u8::try_from(index & 0x0f).unwrap()
        }));
        let encoded = encode_lunar_magic_graphics_tile(&tile);
        assert_eq!(encoded.as_slice(), tile.pixels());
        assert_eq!(decode_lunar_magic_graphics_tile(&encoded).unwrap(), tile);

        let mut oversized = encoded.to_vec();
        oversized.extend_from_slice(&[0xaa; 32]);
        assert_eq!(decode_lunar_magic_graphics_tile(&oversized).unwrap(), tile);
        assert!(decode_lunar_magic_graphics_tile(&encoded[..63]).is_err());

        let mut invalid = encoded;
        invalid[17] = 0x10;
        assert!(decode_lunar_magic_graphics_tile(&invalid).is_err());
    }

    #[test]
    fn retained_lunar_magic_graphics_clipboard_oracle_binds_copy_and_roundtrip() {
        let oracle = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/graphics-single-tile-clipboard/oracle.tsv"
        );
        let fields = oracle
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once('\t'))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(fields.get("format_name"), Some(&"Lunar Magic 8x8 Tile"));
        assert_eq!(fields.get("copied_size"), Some(&"64"));
        assert_eq!(
            fields.get("copied_bytes"),
            Some(
                &"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
            )
        );
        assert_eq!(fields.get("roundtrip_size"), Some(&"64"));
        assert_eq!(
            fields.get("roundtrip_bytes"),
            Some(
                &"000102030405060708090A0B0C0D0E0F000102030405060708090A0B0C0D0E0F000102030405060708090A0B0C0D0E0F000102030405060708090A0B0C0D0E0F"
            )
        );

        let expected = IndexedTile::new(std::array::from_fn(|index| (index & 0x0f) as u8));
        let encoded = encode_lunar_magic_graphics_tile(&expected);
        assert_eq!(
            decode_lunar_magic_graphics_tile(&encoded).unwrap(),
            expected
        );
    }

    #[test]
    fn lunar_magic_native_map16_tile_is_four_words_then_acts_like() {
        let tile = Map16Tile {
            top_left: lm_level::Subtile(0x0123),
            top_right: lm_level::Subtile(0x4567),
            bottom_left: lm_level::Subtile(0x89ab),
            bottom_right: lm_level::Subtile(0xcdef),
            acts_like: 0x1357,
        };
        let encoded = encode_lunar_magic_map16_tile(tile);
        assert_eq!(
            encoded,
            [0x23, 0x01, 0x67, 0x45, 0xab, 0x89, 0xef, 0xcd, 0x57, 0x13]
        );
        assert_eq!(decode_lunar_magic_map16_tile(&encoded).unwrap(), tile);

        let mut oversized = encoded.to_vec();
        oversized.extend_from_slice(&[0xaa; 16]);
        assert_eq!(decode_lunar_magic_map16_tile(&oversized).unwrap(), tile);
        assert!(decode_lunar_magic_map16_tile(&encoded[..9]).is_err());
    }

    #[test]
    fn retained_lunar_magic_map16_clipboard_oracle_binds_copy_and_roundtrip() {
        let oracle = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/map16-single-tile-clipboard/oracle.tsv"
        );
        let fields = oracle
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once('\t'))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(fields.get("format_name"), Some(&"Lunar Magic 16x16 Tile"));
        assert_eq!(fields.get("copied_size"), Some(&"10"));
        assert_eq!(fields.get("copied_bytes"), Some(&"701C721C711C731C0000"));
        assert_eq!(fields.get("roundtrip_size"), Some(&"10"));
        assert_eq!(fields.get("roundtrip_bytes"), Some(&"23016745AB89EFCD5713"));

        let roundtrip = [0x23, 0x01, 0x67, 0x45, 0xab, 0x89, 0xef, 0xcd, 0x57, 0x13];
        let decoded = decode_lunar_magic_map16_tile(&roundtrip).unwrap();
        assert_eq!(encode_lunar_magic_map16_tile(decoded), roundtrip);
    }

    #[test]
    fn palette_adapter_requires_exactly_one_color() {
        assert_eq!(
            decode_palette_color(&encode_palette_color(Bgr555(0x1234)).unwrap()).unwrap(),
            Bgr555(0x1234)
        );
        let two = ClipboardPayload::from_palette_colors(&[Bgr555(1), Bgr555(2)]);
        assert!(decode_palette_color(&encode(&two).unwrap()).is_err());
    }

    #[test]
    fn palette_row_requires_and_round_trips_exactly_sixteen_colors() {
        let colors = std::array::from_fn(|index| {
            Bgr555(u16::try_from(index).expect("sixteen palette entries fit u16"))
        });
        let text = encode_palette_row(&colors).unwrap();
        assert_eq!(decode_palette_row(&text).unwrap(), colors);
        assert!(encode_palette_row(&colors[..15]).is_err());
        assert!(decode_palette_row(&encode_palette_color(Bgr555(1)).unwrap()).is_err());
    }

    #[test]
    fn lunar_magic_color_v2_matches_the_recovered_twelve_byte_record() {
        let color = Bgr555(0x7fdd);
        let encoded = encode_lunar_magic_palette_color(color);
        assert_eq!(encoded, [0, 0, 0, 0, 0xff, 0xf7, 0xef, 0, 0xdd, 0x7f, 0, 0]);
        assert_eq!(decode_lunar_magic_palette_color(&encoded).unwrap(), color);

        let mut rgb_record = encoded;
        rgb_record[..4].copy_from_slice(&1_u32.to_le_bytes());
        rgb_record[4..8].copy_from_slice(&0x00_ef_f7_ff_u32.to_le_bytes());
        rgb_record[8..12].fill(0xff);
        assert_eq!(
            decode_lunar_magic_palette_color(&rgb_record).unwrap(),
            color
        );
        rgb_record[4..8].copy_from_slice(&0x00_77_77_77_u32.to_le_bytes());
        assert_eq!(
            decode_lunar_magic_palette_color(&rgb_record).unwrap(),
            Bgr555(0x3def)
        );
        assert!(decode_lunar_magic_palette_color(&encoded[..11]).is_err());
    }

    #[test]
    fn lunar_magic_color_row_v2_has_rgb_then_snes_planes_and_prefers_snes() {
        let colors = std::array::from_fn(|index| Bgr555((index as u16 * 0x421) & 0x7fff));
        let encoded = encode_lunar_magic_palette_row(&colors);
        assert_eq!(encoded.len(), 132);
        assert_eq!(&encoded[..4], &[0; 4]);
        for (index, color) in colors.iter().copied().enumerate() {
            let rgb = 4 + index * 4;
            assert_eq!(
                u32::from_le_bytes(encoded[rgb..rgb + 4].try_into().unwrap()),
                lunar_magic_rgb(color)
            );
            let snes = 68 + index * 4;
            assert_eq!(
                u32::from_le_bytes(encoded[snes..snes + 4].try_into().unwrap()),
                u32::from(color.0)
            );
        }
        assert_eq!(decode_lunar_magic_palette_row(&encoded).unwrap(), colors);

        let mut invalid = encoded;
        invalid[68..72].copy_from_slice(&0x8000_u32.to_le_bytes());
        let masked = decode_lunar_magic_palette_row(&invalid).unwrap();
        assert_eq!(masked[0], Bgr555(0));
        assert!(decode_lunar_magic_palette_row(&encoded[..131]).is_err());
    }

    #[test]
    fn retained_lunar_magic_palette_clipboard_oracle_matches_both_v2_records() {
        let oracle = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/palette-clipboard/oracle.tsv"
        );
        let fields = oracle
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once('\t'))
            .collect::<std::collections::BTreeMap<_, _>>();
        let oracle_bytes = |field: &str| {
            fields[field]
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(fields["color_v2_format"], "Lunar Magic Color V2");
        assert_eq!(fields["color_v2_size"], "12");
        assert_eq!(
            oracle_bytes("color_v2_bytes"),
            encode_lunar_magic_palette_color(Bgr555(0x7fdd))
        );
        assert_eq!(fields["row_v2_format"], "Lunar Magic Color Row V2");
        assert_eq!(fields["row_v2_size"], "132");
        let oracle_row = oracle_bytes("row_v2_bytes");
        let colors = decode_lunar_magic_palette_row(&oracle_row).unwrap();
        let encoded = encode_lunar_magic_palette_row(&colors);
        assert_eq!(&encoded[68..], &oracle_row[68..]);
        assert_eq!(colors[7], Bgr555(0x3def));
    }

    #[test]
    fn graphics_adapter_validates_and_requires_one_tile() {
        let tile = IndexedTile::new([7; IndexedTile::PIXEL_COUNT]);
        assert_eq!(
            decode_graphics_tile(&encode_graphics_tile(&tile).unwrap()).unwrap(),
            tile
        );
        let two = ClipboardPayload::from_graphics_tiles(&[tile.clone(), tile]);
        assert!(decode_graphics_tile(&encode(&two).unwrap()).is_err());
    }

    #[test]
    fn map16_adapter_retains_graphics_and_acts_like() {
        let tile = Map16Tile {
            acts_like: 0x1234,
            ..Map16Tile::default()
        };
        assert_eq!(
            decode_map16_tile(&encode_map16_tile(tile).unwrap()).unwrap(),
            tile
        );
    }

    #[test]
    fn native_map16_rectangle_adapter_retains_exact_shape_origin_and_sections() {
        let tiles = vec![
            Map16Tile {
                acts_like: 0x1234,
                ..Map16Tile::default()
            },
            Map16Tile {
                acts_like: 0xabcd,
                ..Map16Tile::default()
            },
        ];
        let rectangle = NativeMap16Clipboard::from_rectangle(0x2e, 2, 1, tiles).unwrap();
        let text = encode_native_map16_rectangle(&rectangle).unwrap();
        assert!(text.starts_with(NATIVE_MAP16_PREFIX));
        assert_eq!(decode_native_map16_rectangle(&text).unwrap(), rectangle);
        assert!(decode_map16_tile(&text).is_err());
        assert!(decode_native_map16_rectangle("LM16TILES1:0").is_err());
        assert!(decode_native_map16_rectangle("LM16TILES1:GG").is_err());
    }

    #[test]
    fn level_object_adapter_retains_the_lossless_record() {
        let object = ObjectRecord::new(vec![0x21, 0x43, 0x65, 0x87]).unwrap();
        assert_eq!(
            decode_level_object(&encode_level_object(&object).unwrap()).unwrap(),
            object
        );
        let two = ClipboardPayload::from_level_objects(&[object.clone(), object]);
        assert!(decode_level_object(&encode(&two).unwrap()).is_err());
    }

    #[test]
    fn level_sprite_adapter_retains_revision_sized_bytes() {
        let sprite = SpriteRecord {
            encoded: vec![0x10, 0x20, 0x30, 0xaa, 0xbb],
        };
        assert_eq!(
            decode_level_sprite(&encode_level_sprite(&sprite).unwrap()).unwrap(),
            sprite
        );
        let two = ClipboardPayload::from_level_sprites(&[sprite.clone(), sprite]);
        assert!(decode_level_sprite(&encode(&two).unwrap()).is_err());
    }

    #[test]
    fn layer3_adapters_keep_tilemap_and_remap_domains_distinct() {
        let tilemap = vec![0x10, 0x20, 0x30];
        let remap = vec![0xaa, 0xbb, 0xcc, 0xdd];
        let tilemap_text = encode_layer3_tilemap(&tilemap).unwrap();
        let remap_text = encode_layer3_remap(&remap).unwrap();
        assert_eq!(decode_layer3_tilemap(&tilemap_text).unwrap(), tilemap);
        assert_eq!(decode_layer3_remap(&remap_text).unwrap(), remap);
        assert!(decode_layer3_remap(&tilemap_text).is_err());
        assert!(decode_layer3_tilemap(&remap_text).is_err());
    }

    #[test]
    fn layer2_rectangle_adapter_retains_shape_and_word_order() {
        let words = [0x1234, 0xabcd, 0x5678, 0xbeef];
        let text = encode_layer2_tilemap_selection(2, 2, &words).unwrap();
        assert_eq!(
            decode_layer2_tilemap_selection(&text).unwrap(),
            (2, 2, words.to_vec())
        );
        assert!(decode_layer3_tilemap(&text).is_err());
    }

    #[test]
    fn exanimation_adapter_retains_the_complete_fixed_record() {
        let record = ExAnimationRecord::new(1, 0, 0, 0x1234, true, &[7, 8], false).unwrap();
        assert_eq!(
            decode_exanimation_record(&encode_exanimation_record(&record).unwrap()).unwrap(),
            record
        );
    }

    #[test]
    fn exanimation_frame_adapter_retains_one_or_two_source_words() {
        for frame in [
            ExAnimationFrame {
                source_words: vec![0x1234],
            },
            ExAnimationFrame {
                source_words: vec![0x1234, 0xabcd],
            },
        ] {
            assert_eq!(
                decode_exanimation_frame(&encode_exanimation_frame(&frame).unwrap()).unwrap(),
                frame
            );
        }
        let frames = [
            ExAnimationFrame {
                source_words: vec![1],
            },
            ExAnimationFrame {
                source_words: vec![2],
            },
        ];
        let payload = ClipboardPayload::from_exanimation_frames(&frames).unwrap();
        assert!(decode_exanimation_frame(&encode(&payload).unwrap()).is_err());
    }

    #[test]
    fn overworld_sprite_adapter_retains_variable_extension_bytes() {
        let sprite = OverworldSprite {
            id: 0x1234,
            x: 0x2345,
            y: 0x3456,
            submap: lm_overworld::Submap::StarWorld,
            extra: vec![0xaa, 0xbb],
        };
        assert_eq!(
            decode_overworld_sprite(&encode_overworld_sprite(&sprite).unwrap()).unwrap(),
            sprite
        );
    }

    #[test]
    fn overworld_message_adapter_retains_all_tiles() {
        let mut bytes = [0; OverworldMessage::ENCODED_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap();
        }
        let message = OverworldMessage(bytes);
        assert_eq!(
            decode_overworld_message(&encode_overworld_message(&message).unwrap()).unwrap(),
            message
        );
    }

    #[test]
    fn overworld_appearance_adapter_retains_one_complete_part() {
        let part = SpriteAppearancePart {
            tile_index: 0xabcd,
            palette_index: 7,
            x_offset: -1234,
            y_offset: 2345,
            x_flip: true,
            y_flip: false,
        };
        let text = encode_overworld_appearance_part(part).unwrap();
        assert_eq!(decode_overworld_appearance_part(&text).unwrap(), part);
        assert!(decode_overworld_sprite(&text).is_err());

        let payload = ClipboardPayload::from_overworld_appearance_parts(&[part, part]).unwrap();
        assert!(decode_overworld_appearance_part(&encode(&payload).unwrap()).is_err());
    }

    #[test]
    fn overworld_appearance_composition_adapter_retains_painter_order() {
        let parts = [
            SpriteAppearancePart {
                tile_index: 1,
                palette_index: 2,
                x_offset: -8,
                y_offset: 0,
                x_flip: false,
                y_flip: true,
            },
            SpriteAppearancePart {
                tile_index: 3,
                palette_index: 4,
                x_offset: 16,
                y_offset: -24,
                x_flip: true,
                y_flip: false,
            },
        ];
        let text = encode_overworld_appearance_parts(&parts).unwrap();
        assert_eq!(decode_overworld_appearance_parts(&text).unwrap(), parts);
        assert!(decode_overworld_appearance_part(&text).is_err());
        assert!(encode_overworld_appearance_parts(&[]).is_err());
        let empty = ClipboardPayload::from_overworld_appearance_parts(&[]).unwrap();
        assert!(decode_overworld_appearance_parts(&encode(&empty).unwrap()).is_err());
        assert!(
            decode_overworld_appearance_parts(&encode_palette_color(Bgr555(1)).unwrap()).is_err()
        );
    }
}
