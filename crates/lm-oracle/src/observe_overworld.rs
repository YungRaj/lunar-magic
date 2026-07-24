use crate::{Observation, observe_exanimation, observe_palette, sha256_hex};
use lm_overworld::Overworld;

mod metadata;
mod paths;
mod tables;

pub use metadata::observe_overworld_metadata;
pub use paths::observe_overworld_paths;
pub use tables::{observe_overworld_messages, observe_overworld_sprites};

/// Produces a canonical snapshot of all currently modeled overworld domains.
#[must_use]
pub fn observe_overworld(overworld: &Overworld) -> Observation {
    let mut result = Observation::new();
    observe_layer(&mut result, "overworld/layer1", &overworld.layer1);
    observe_layer(&mut result, "overworld/layer2", &overworld.layer2);
    put(
        &mut result,
        "overworld/events/count",
        overworld.events.len(),
    );
    for (index, event) in overworld.events.iter().enumerate() {
        let base = format!("overworld/events/{index:04x}");
        put(&mut result, &format!("{base}/event"), event.event.0);
        put(&mut result, &format!("{base}/x"), event.x);
        put(&mut result, &format!("{base}/y"), event.y);
        put(&mut result, &format!("{base}/before"), event.before);
        put(&mut result, &format!("{base}/after"), event.after);
        put(&mut result, &format!("{base}/raw-flags"), event.raw_flags);
    }
    put(
        &mut result,
        "overworld/endpoints/count",
        overworld.endpoints.len(),
    );
    for (index, endpoint) in overworld.endpoints.iter().enumerate() {
        let base = format!("overworld/endpoints/{index:04x}");
        put(&mut result, &format!("{base}/x"), endpoint.x);
        put(&mut result, &format!("{base}/y"), endpoint.y);
        put(&mut result, &format!("{base}/submap"), endpoint.submap);
    }
    merge(
        &mut result,
        "overworld",
        &observe_overworld_paths(&overworld.paths),
        "overworld",
    );
    merge(
        &mut result,
        "overworld",
        &observe_overworld_metadata(&overworld.metadata),
        "overworld",
    );
    merge(
        &mut result,
        "overworld",
        &observe_overworld_sprites(&overworld.sprites),
        "overworld",
    );
    merge(
        &mut result,
        "overworld",
        &observe_overworld_messages(&overworld.messages),
        "overworld",
    );
    put(
        &mut result,
        "overworld/palettes/count",
        overworld.palettes.len(),
    );
    for (index, palette) in overworld.palettes.iter().enumerate() {
        merge(
            &mut result,
            &format!("overworld/palettes/{index:04x}"),
            &observe_palette(palette),
            "palette",
        );
    }
    merge(
        &mut result,
        "overworld/animations",
        &observe_exanimation(&overworld.animations),
        "exanimation",
    );
    put(
        &mut result,
        "overworld/unknown-extensions/count",
        overworld.unknown_extensions.len(),
    );
    for (index, extension) in overworld.unknown_extensions.iter().enumerate() {
        put(
            &mut result,
            &format!("overworld/unknown-extensions/{index:04x}/sha256"),
            sha256_hex(extension),
        );
    }
    result
}

fn observe_layer(result: &mut Observation, base: &str, layer: &lm_overworld::OverworldLayer) {
    put(result, &format!("{base}/width"), layer.width);
    put(result, &format!("{base}/height"), layer.height);
    put(result, &format!("{base}/tile-count"), layer.tiles.len());
    for (index, tile) in layer.tiles.iter().enumerate() {
        put(result, &format!("{base}/tiles/{index:06x}"), tile);
    }
}

fn merge(result: &mut Observation, prefix: &str, source: &Observation, source_root: &str) {
    for (path, value) in source.entries() {
        let suffix = path.strip_prefix(source_root).unwrap_or(path);
        put(result, &format!("{prefix}{suffix}"), value);
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};
    use lm_overworld::{
        EventId, EventTileChange, MetadataEdit, OverworldLayer, OverworldLevelName,
        OverworldMetadata, OverworldPathGraph, PathDirection, PathEdge, PathNode, PlayerStart,
        Submap, SubmapSettings,
    };

    #[test]
    fn overworld_snapshot_is_deterministic_and_domain_addressable() {
        let mut overworld = Overworld {
            layer1: OverworldLayer::new(2, 1, vec![1, 2]).unwrap(),
            layer2: OverworldLayer::new(0, 0, vec![]).unwrap(),
            events: vec![EventTileChange {
                event: EventId(3),
                x: 1,
                y: 0,
                before: 2,
                after: 4,
                raw_flags: 0x80,
            }],
            palettes: vec![Palette {
                colors: vec![Bgr555(5)],
            }],
            ..Overworld::default()
        };
        let before = observe_overworld(&overworld);
        assert_eq!(Observation::from_text(&before.to_text()).unwrap(), before);
        overworld.events[0].after = 9;
        let differences = before.differences(&observe_overworld(&overworld));
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].path, "overworld/events/0000/after");
    }

    #[test]
    fn nested_palette_paths_do_not_leak_the_generic_root() {
        let overworld = Overworld {
            palettes: vec![Palette {
                colors: vec![Bgr555(5)],
            }],
            ..Overworld::default()
        };
        let observation = observe_overworld(&overworld);
        assert_eq!(
            observation.get("overworld/palettes/0000/colors/0000/bgr555"),
            Some("5")
        );
    }

    #[test]
    fn path_snapshot_exposes_node_ids_and_preserves_flags() {
        let mut paths = OverworldPathGraph {
            nodes: vec![PathNode {
                id: 0x42,
                x: 3,
                y: 4,
                submap: Submap::StarWorld,
                level: Some(0x105),
                raw_flags: 0x80,
            }],
            edges: Vec::new(),
        };
        let before = observe_overworld_paths(&paths);
        assert_eq!(
            before.get("overworld/paths/nodes/0000/raw-flags"),
            Some("128")
        );
        paths.nodes.push(PathNode {
            id: 0x43,
            x: 5,
            y: 6,
            submap: Submap::Main,
            level: None,
            raw_flags: 0,
        });
        paths.edges.push(PathEdge {
            from: 0x42,
            to: 0x43,
            direction: PathDirection::Right,
            exit_index: None,
            raw_flags: PathEdge::ONE_WAY_FLAG,
        });
        let after = observe_overworld_paths(&paths);
        assert_eq!(
            after.get("overworld/paths/edges/0000/direction"),
            Some("Right")
        );
        assert!(!before.differences(&after).is_empty());
    }

    #[test]
    fn metadata_snapshot_exposes_semantics_and_hashes_unowned_bytes() {
        let mut metadata = OverworldMetadata {
            level_names: vec![OverworldLevelName {
                level: 0x105,
                tiles: [7; OverworldLevelName::TILE_COUNT],
                raw_flags: 0x80,
            }],
            player_starts: vec![PlayerStart {
                player: 1,
                x: 2,
                y: 3,
                submap: Submap::YoshiIsland,
                raw_flags: 4,
            }],
            submap_settings: vec![SubmapSettings {
                submap: Submap::YoshiIsland,
                music: 5,
                palette: 6,
                layer1_scroll: 7,
                layer2_scroll: 8,
                raw_flags: 9,
                unknown: [10; 5],
            }],
        };
        let observation = observe_overworld_metadata(&metadata);
        assert_eq!(
            observation.get("overworld/metadata/level-names/0000/level"),
            Some("261")
        );
        assert_eq!(
            observation.get("overworld/metadata/player-starts/0000/submap"),
            Some("1")
        );
        assert!(
            observation
                .get("overworld/metadata/submap-settings/0000/unknown-sha256")
                .is_some()
        );
        let unknown_hash = observation
            .get("overworld/metadata/submap-settings/0000/unknown-sha256")
            .unwrap()
            .to_owned();
        metadata
            .apply_edits(&[MetadataEdit::UpsertSubmapSettings(SubmapSettings {
                music: 12,
                ..metadata.submap_settings[0]
            })])
            .unwrap();
        let edited = observe_overworld_metadata(&metadata);
        let differences = observation.differences(&edited);
        assert_eq!(differences.len(), 1);
        assert_eq!(
            differences[0].path,
            "overworld/metadata/submap-settings/0000/music"
        );
        assert_eq!(
            edited.get("overworld/metadata/submap-settings/0000/unknown-sha256"),
            Some(unknown_hash.as_str())
        );
    }
}
