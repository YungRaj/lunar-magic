use super::{put, put_hex};
use crate::{Observation, sha256_hex};
use lm_level::Layer3Data;

/// Produces a canonical semantic observation of standalone Layer 3 state.
#[must_use]
pub fn observe_layer3(layer3: &Layer3Data) -> Observation {
    let mut result = Observation::new();
    observe_layer3_at(&mut result, "layer3", layer3);
    result
}

pub(super) fn observe_optional_layer3(result: &mut Observation, layer3: Option<&Layer3Data>) {
    let Some(layer3) = layer3 else {
        put(result, "level/layer3/present", false);
        return;
    };
    put(result, "level/layer3/present", true);
    observe_layer3_at(result, "level/layer3", layer3);
}

fn observe_layer3_at(result: &mut Observation, base: &str, layer3: &Layer3Data) {
    put(
        result,
        &format!("{base}/start-position"),
        layer3.settings.start_position,
    );
    put(
        result,
        &format!("{base}/tilemap-size"),
        layer3.settings.tilemap_size,
    );
    put(
        result,
        &format!("{base}/liquid-type"),
        layer3.settings.liquid_type,
    );
    put(result, &format!("{base}/flags"), layer3.settings.flags);
    for (slot, file) in layer3.settings.graphics_files.iter().enumerate() {
        put(result, &format!("{base}/graphics/{slot}"), file);
    }
    put_hex(
        result,
        &format!("{base}/settings-reserved"),
        &layer3.settings.reserved,
    );
    put(
        result,
        &format!("{base}/tilemap-length"),
        layer3.tilemap.len(),
    );
    put(
        result,
        &format!("{base}/tilemap-sha256"),
        sha256_hex(&layer3.tilemap),
    );
    put(
        result,
        &format!("{base}/remap-length"),
        layer3.remap_commands.len(),
    );
    put_hex(
        result,
        &format!("{base}/remap-commands"),
        &layer3.remap_commands,
    );
}
