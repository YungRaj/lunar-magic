use super::{Observation, put, sha256_hex};
use lm_overworld::OverworldMetadata;

#[must_use]
pub fn observe_overworld_metadata(metadata: &OverworldMetadata) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "overworld/metadata/level-names/count",
        metadata.level_names.len(),
    );
    for (index, name) in metadata.level_names.iter().enumerate() {
        let base = format!("overworld/metadata/level-names/{index:04x}");
        put(&mut result, &format!("{base}/level"), name.level);
        put(
            &mut result,
            &format!("{base}/tiles-sha256"),
            sha256_hex(&name.tiles),
        );
        put(&mut result, &format!("{base}/raw-flags"), name.raw_flags);
    }
    put(
        &mut result,
        "overworld/metadata/player-starts/count",
        metadata.player_starts.len(),
    );
    for (index, start) in metadata.player_starts.iter().enumerate() {
        let base = format!("overworld/metadata/player-starts/{index:04x}");
        put(&mut result, &format!("{base}/player"), start.player);
        put(&mut result, &format!("{base}/x"), start.x);
        put(&mut result, &format!("{base}/y"), start.y);
        put(
            &mut result,
            &format!("{base}/submap"),
            start.submap.encoded(),
        );
        put(&mut result, &format!("{base}/raw-flags"), start.raw_flags);
    }
    put(
        &mut result,
        "overworld/metadata/submap-settings/count",
        metadata.submap_settings.len(),
    );
    for (index, settings) in metadata.submap_settings.iter().enumerate() {
        let base = format!("overworld/metadata/submap-settings/{index:04x}");
        put(
            &mut result,
            &format!("{base}/submap"),
            settings.submap.encoded(),
        );
        put(&mut result, &format!("{base}/music"), settings.music);
        put(&mut result, &format!("{base}/palette"), settings.palette);
        put(
            &mut result,
            &format!("{base}/layer1-scroll"),
            settings.layer1_scroll,
        );
        put(
            &mut result,
            &format!("{base}/layer2-scroll"),
            settings.layer2_scroll,
        );
        put(
            &mut result,
            &format!("{base}/raw-flags"),
            settings.raw_flags,
        );
        put(
            &mut result,
            &format!("{base}/unknown-sha256"),
            sha256_hex(&settings.unknown),
        );
    }
    result
}
