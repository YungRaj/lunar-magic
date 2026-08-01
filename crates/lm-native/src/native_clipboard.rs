use lm_app::ClipboardPayload;
use lm_graphics::{Bgr555, ExAnimationFrame, ExAnimationRecord, IndexedTile};
use lm_level::{Map16Tile, ObjectRecord, SpriteRecord};
use lm_overworld::{OverworldMessage, OverworldSprite};

const PREFIX: &str = "LMCLIP1:";

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
}
