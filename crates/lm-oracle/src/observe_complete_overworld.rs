use crate::{
    Observation, observe_compact_exanimation, observe_overworld_messages,
    observe_overworld_sprites, observe_palette,
};
use lm_project::CompleteOverworldFile;

/// Produces a canonical snapshot of all nine domains in one `LMOWFULL` artifact.
#[must_use]
pub fn observe_complete_overworld(file: &CompleteOverworldFile) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "overworld/source-slot", file.source_slot);
    for (name, value) in [
        ("width", file.shape.width),
        ("height", file.shape.height),
        ("event-reveals", file.shape.event_reveals),
        ("endpoints", file.shape.endpoints),
        ("messages", file.shape.messages),
        ("sprites", file.shape.sprites),
        ("sprite-record-len", file.shape.sprite_record_len),
        ("palette-colors", file.shape.palette_colors),
    ] {
        put(&mut result, &format!("overworld/shape/{name}"), value);
    }
    observe_layer(&mut result, "overworld/layer1", &file.data.layers.layer1);
    observe_layer(&mut result, "overworld/layer2", &file.data.layers.layer2);
    put(
        &mut result,
        "overworld/event-reveals/count",
        file.data.event_reveals.entries.len(),
    );
    for (index, reveal) in file.data.event_reveals.entries.iter().enumerate() {
        let base = format!("overworld/event-reveals/{index:04x}");
        put(&mut result, &format!("{base}/source"), reveal.source_tile);
        put(
            &mut result,
            &format!("{base}/destination"),
            reveal.destination_tile,
        );
    }
    put(
        &mut result,
        "overworld/endpoints/count",
        file.data.endpoints.len(),
    );
    for (index, endpoint) in file.data.endpoints.iter().enumerate() {
        let base = format!("overworld/endpoints/{index:04x}");
        put(&mut result, &format!("{base}/x"), endpoint.x);
        put(&mut result, &format!("{base}/y"), endpoint.y);
        put(&mut result, &format!("{base}/submap"), endpoint.submap);
    }
    merge(
        &mut result,
        &observe_overworld_messages(&file.data.messages),
    );
    merge(&mut result, &observe_overworld_sprites(&file.data.sprites));
    merge_at(
        &mut result,
        "overworld/palette",
        &observe_palette(&file.data.palette),
        "palette",
    );
    merge_at(
        &mut result,
        "overworld/animation",
        &observe_compact_exanimation(&file.data.animation),
        "exanimation",
    );
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

fn merge(result: &mut Observation, source: &Observation) {
    for (path, value) in source.entries() {
        result
            .insert(path, value)
            .expect("observation domains are disjoint");
    }
}

fn merge_at(result: &mut Observation, base: &str, source: &Observation, source_base: &str) {
    for (path, value) in source.entries() {
        let suffix = path
            .strip_prefix(source_base)
            .expect("observer uses its domain prefix");
        result
            .insert(format!("{base}{suffix}"), value)
            .expect("observation domains are disjoint");
    }
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_observation_value())
        .expect("observation paths are unique");
}

trait ObservationValue {
    fn into_observation_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_observation_value(self) -> String {
        self.to_string()
    }
}
