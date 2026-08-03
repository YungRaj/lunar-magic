mod layer3;
mod map16;

pub use layer3::observe_layer3;
use layer3::observe_optional_layer3;
pub use map16::{observe_map16_page, observe_map16_page_file, observe_map16_set};

use crate::{Observation, sha256_hex};
use lm_level::{EntranceKind, Level, ScreenJumpEncoding};

/// Produces a canonical semantic snapshot of a complete decoded level.
#[must_use]
pub fn observe_level(level: &Level) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "level/number", level.number);
    put_hex(
        &mut result,
        "level/header/legacy",
        &level.header.legacy.encoded(),
    );
    match level.header.expanded {
        Some(header) => put_hex(&mut result, "level/header/expanded", &header.encode()),
        None => put(&mut result, "level/header/expanded", "none"),
    }
    observe_layer(&mut result, "level/layer1", &level.layer1);
    observe_layer(&mut result, "level/layer2", &level.layer2);
    observe_optional_layer3(&mut result, level.layer3.as_ref());
    observe_sprites(&mut result, level);
    observe_entrances(&mut result, level);
    observe_exits(&mut result, level);
    observe_map16_overrides(&mut result, level);
    observe_unknowns(
        &mut result,
        "level/unknown-extensions",
        &level.unknown_extensions,
    );
    result
}

fn observe_sprites(result: &mut Observation, level: &Level) {
    put(result, "level/sprites/header", level.sprites.header);
    put(result, "level/sprites/count", level.sprites.records.len());
    for (index, sprite) in level.sprites.records.iter().enumerate() {
        put_hex(
            result,
            &format!("level/sprites/{index:04x}/encoded"),
            &sprite.encoded,
        );
    }
}

fn observe_entrances(result: &mut Observation, level: &Level) {
    put(result, "level/entrances/count", level.entrances.len());
    for (index, entrance) in level.entrances.iter().enumerate() {
        let base = format!("level/entrances/{index:04x}");
        put(
            result,
            &format!("{base}/kind"),
            match entrance.kind {
                EntranceKind::Main => "main",
                EntranceKind::Midway => "midway",
                EntranceKind::Secondary => "secondary",
            },
        );
        put(result, &format!("{base}/x"), entrance.x);
        put(result, &format!("{base}/y"), entrance.y);
        put(result, &format!("{base}/screen"), entrance.screen);
        put(result, &format!("{base}/action"), entrance.action);
        put(result, &format!("{base}/raw-flags"), entrance.raw_flags);
    }
}

fn observe_exits(result: &mut Observation, level: &Level) {
    put(result, "level/screen-exits/count", level.screen_exits.len());
    for (index, exit) in level.screen_exits.iter().enumerate() {
        put(
            result,
            &format!("level/screen-exits/{index:04x}"),
            exit.encoded,
        );
    }
    put(
        result,
        "level/secondary-exits/count",
        level.secondary_exits.len(),
    );
    for (index, exit) in level.secondary_exits.iter().enumerate() {
        let base = format!("level/secondary-exits/{index:04x}");
        for (field, value) in [
            ("destination", exit.destination_level),
            ("position-method", u16::from(exit.position_and_method)),
            ("screen", u16::from(exit.screen)),
            ("x", u16::from(exit.x)),
            ("y", u16::from(exit.y)),
            ("destination-flags", u16::from(exit.destination_flags)),
            ("x-overworld-flags", u16::from(exit.x_and_overworld_flags)),
            ("additional-flags", u16::from(exit.additional_flags)),
        ] {
            put(result, &format!("{base}/{field}"), value);
        }
    }
}

fn observe_map16_overrides(result: &mut Observation, level: &Level) {
    put(
        result,
        "level/map16-overrides/count",
        level.map16_overrides.len(),
    );
    for (index, (tile_index, tile)) in level.map16_overrides.iter().enumerate() {
        let base = format!("level/map16-overrides/{index:04x}");
        put(result, &format!("{base}/index"), tile_index);
        put_hex(result, &format!("{base}/graphics"), &tile.encode_graphics());
        put(result, &format!("{base}/acts-like"), tile.acts_like);
    }
}

fn observe_layer(result: &mut Observation, base: &str, layer: &lm_level::LayerData) {
    put(
        result,
        &format!("{base}/object-count"),
        layer.objects.records.len(),
    );
    for (index, object) in layer.objects.records.iter().enumerate() {
        let object_base = format!("{base}/objects/{index:04x}");
        put_hex(result, &object_base, object.encoded());
        put(
            result,
            &format!("{object_base}/command-id"),
            object.command_id(),
        );
        put(
            result,
            &format!("{object_base}/parameter"),
            object.parameter(),
        );
        let coordinates = object.coordinate_nibbles();
        put(
            result,
            &format!("{object_base}/coordinate-first"),
            coordinates.first,
        );
        put(
            result,
            &format!("{object_base}/coordinate-second"),
            coordinates.second,
        );
        put(
            result,
            &format!("{object_base}/screen-advance"),
            object.advances_screen(),
        );
        match object.screen_jump() {
            Some(jump) => {
                put(result, &format!("{object_base}/kind"), "screen-jump");
                put(
                    result,
                    &format!("{object_base}/screen-jump-encoding"),
                    match jump.encoding {
                        ScreenJumpEncoding::FirstLow => "first-low",
                        ScreenJumpEncoding::FirstHigh => "first-high",
                    },
                );
                put(
                    result,
                    &format!("{object_base}/screen-jump-target"),
                    jump.packed_target,
                );
                put(
                    result,
                    &format!("{object_base}/screen-jump-resolved-screen"),
                    jump.resolved_screen(),
                );
            }
            None => put(result, &format!("{object_base}/kind"), "object"),
        }
    }
    let bytes: Vec<_> = layer
        .raw_tilemap
        .iter()
        .flat_map(|tile| tile.to_le_bytes())
        .collect();
    put(
        result,
        &format!("{base}/raw-tile-count"),
        layer.raw_tilemap.len(),
    );
    put(
        result,
        &format!("{base}/raw-tile-sha256"),
        sha256_hex(&bytes),
    );
}

fn observe_unknowns(result: &mut Observation, base: &str, values: &[Vec<u8>]) {
    put(result, &format!("{base}/count"), values.len());
    for (index, value) in values.iter().enumerate() {
        put_hex(result, &format!("{base}/{index:04x}"), value);
    }
}

pub(super) fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_observation_value())
        .expect("adapter paths are unique");
}

pub(super) trait ObservationValue {
    fn into_observation_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_observation_value(self) -> String {
        self.to_string()
    }
}

pub(super) fn put_hex(result: &mut Observation, path: &str, bytes: &[u8]) {
    put(result, path, hex(bytes));
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").expect("String writes cannot fail");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{
        Entrance, LayerData, Map16Page, Map16Set, Map16Tile, ObjectRecord, ObjectStream,
        ScreenExit, SecondaryExit, SpriteRecord, SpriteStream,
    };

    #[test]
    fn level_observation_is_deterministic_and_field_sensitive() {
        let mut level = Level {
            number: 0x105,
            layer1: LayerData {
                objects: ObjectStream {
                    records: vec![ObjectRecord::new(vec![1, 2, 3]).unwrap()],
                },
                raw_tilemap: vec![0x1234],
            },
            sprites: SpriteStream {
                header: 7,
                records: vec![SpriteRecord {
                    encoded: vec![4, 5, 6],
                }],
            },
            ..Level::default()
        };
        let before = observe_level(&level);
        assert_eq!(Observation::from_text(&before.to_text()).unwrap(), before);
        level.sprites.records[0].encoded[2] = 9;
        let difference = before.differences(&observe_level(&level));
        assert_eq!(difference.len(), 1);
        assert_eq!(difference[0].path, "level/sprites/0000/encoded");
    }

    #[test]
    fn level_observation_exposes_packed_and_resolved_screen_jump_values() {
        let level = Level {
            layer1: LayerData {
                objects: ObjectStream {
                    records: vec![ObjectRecord::new(vec![5, 3, 1]).unwrap()],
                },
                raw_tilemap: Vec::new(),
            },
            layer2: LayerData {
                objects: ObjectStream {
                    records: vec![ObjectRecord::new(vec![5, 3, 3]).unwrap()],
                },
                raw_tilemap: Vec::new(),
            },
            ..Level::default()
        };

        let observed = observe_level(&level);
        for base in ["level/layer1/objects/0000", "level/layer2/objects/0000"] {
            assert_eq!(
                observed.get(&format!("{base}/screen-jump-resolved-screen")),
                Some("8")
            );
        }
        assert_eq!(
            observed.get("level/layer1/objects/0000/screen-jump-target"),
            Some("773")
        );
        assert_eq!(
            observed.get("level/layer2/objects/0000/screen-jump-target"),
            Some("1283")
        );
    }

    #[test]
    fn map16_observation_identifies_individual_tile_fields() {
        let mut page = Map16Page::new(vec![lm_level::Map16Tile::default(); 256]).unwrap();
        let before = observe_map16_page(&page);
        page.tiles[3].acts_like = 7;
        let difference = before.differences(&observe_map16_page(&page));
        assert_eq!(difference.len(), 1);
        assert_eq!(difference[0].path, "map16/tiles/0003/acts-like");
    }

    #[test]
    fn level_component_observers_keep_every_addressable_path() {
        let level = Level {
            entrances: vec![Entrance {
                kind: EntranceKind::Secondary,
                x: 0x123,
                y: 0x234,
                screen: 5,
                action: 6,
                raw_flags: 0xa500,
            }],
            screen_exits: vec![ScreenExit { encoded: 0x12345 }],
            secondary_exits: vec![SecondaryExit {
                destination_level: 0x1ab,
                position_and_method: 1,
                screen: 2,
                x: 3,
                y: 4,
                destination_flags: 5,
                x_and_overworld_flags: 6,
                additional_flags: 7,
            }],
            map16_overrides: vec![(0x2345, Map16Tile::default())],
            ..Level::default()
        };

        let observed = observe_level(&level);
        assert_eq!(observed.get("level/entrances/0000/kind"), Some("secondary"));
        assert_eq!(
            observed.get("level/entrances/0000/raw-flags"),
            Some("42240")
        );
        assert_eq!(observed.get("level/screen-exits/0000"), Some("74565"));
        assert_eq!(
            observed.get("level/secondary-exits/0000/destination"),
            Some("427")
        );
        assert_eq!(
            observed.get("level/secondary-exits/0000/additional-flags"),
            Some("7")
        );
        assert_eq!(
            observed.get("level/map16-overrides/0000/index"),
            Some("9029")
        );
        assert_eq!(
            observed.get("level/map16-overrides/0000/acts-like"),
            Some("0")
        );
    }

    #[test]
    fn page_file_observation_includes_source_identity() {
        let file = lm_level::Map16PageFile {
            source_page: 0x42,
            page: Map16Page::new(vec![lm_level::Map16Tile::default(); 256]).unwrap(),
        };
        let observed = observe_map16_page_file(&file);
        assert_eq!(observed.get("map16/source-page"), Some("66"));
        assert_eq!(observed.get("map16/tile-count"), Some("256"));
    }

    #[test]
    fn complete_map16_observation_preserves_page_addressing() {
        let mut set = Map16Set {
            pages: vec![
                Map16Page::new(vec![lm_level::Map16Tile::default(); 256]).unwrap(),
                Map16Page::new(vec![lm_level::Map16Tile::default(); 256]).unwrap(),
            ],
        };
        let before = observe_map16_set(&set);
        set.pages[1].tiles[2].acts_like = 0x102;
        let differences = before.differences(&observe_map16_set(&set));
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].path, "map16/pages/0001/tiles/0002/acts-like");
    }

    #[test]
    fn standalone_layer3_observation_covers_semantics_and_unknown_bytes() {
        let layer3 = lm_level::Layer3Data {
            settings: lm_level::Layer3Settings {
                start_position: 0xfe,
                tilemap_size: 3,
                liquid_type: 0x81,
                flags: 0xa5,
                graphics_files: [0, 0x123, 0xabc, 0xfff],
                reserved: [0x5a; 16],
            },
            tilemap: vec![1, 2, 3],
            remap_commands: vec![0, 1, 0xff, 0x80],
        };
        let observed = observe_layer3(&layer3);
        assert_eq!(observed.get("layer3/start-position"), Some("254"));
        assert_eq!(observed.get("layer3/graphics/2"), Some("2748"));
        assert_eq!(
            observed.get("layer3/settings-reserved"),
            Some("5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a")
        );
        assert_eq!(observed.get("layer3/remap-commands"), Some("0001ff80"));

        let mut edited = layer3;
        edited.tilemap.push(4);
        let differences = observed.differences(&observe_layer3(&edited));
        assert_eq!(differences.len(), 2);
        assert_eq!(differences[0].path, "layer3/tilemap-length");
        assert_eq!(differences[1].path, "layer3/tilemap-sha256");
    }
}
