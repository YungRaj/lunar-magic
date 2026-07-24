use crate::{Layer3Placement, MaterializedLayer3Plane};
use lm_oracle::Observation;

/// Produces a canonical provider-output snapshot preserving painter order and source binding.
#[must_use]
pub fn observe_materialized_layer3_plane(plane: &MaterializedLayer3Plane) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "layer3-plane/source-sha256",
        hex(&plane.source_digest),
    );
    put(
        &mut result,
        "layer3-plane/placement",
        match plane.placement {
            Layer3Placement::BehindLayer2 => "behind-layer2",
            Layer3Placement::BetweenLayer2AndLayer1 => "between-layer2-and-layer1",
            Layer3Placement::AboveLayer1 => "above-layer1",
            Layer3Placement::AboveEntities => "above-entities",
        },
    );
    put(
        &mut result,
        "layer3-plane/instances/count",
        plane.instances.len(),
    );
    for (index, instance) in plane.instances.iter().enumerate() {
        let base = format!("layer3-plane/instances/{index:04x}");
        put(&mut result, &format!("{base}/tile"), instance.tile_index);
        put(
            &mut result,
            &format!("{base}/palette"),
            instance.palette_index,
        );
        put(&mut result, &format!("{base}/x"), instance.x);
        put(&mut result, &format!("{base}/y"), instance.y);
        put(&mut result, &format!("{base}/x-flip"), instance.x_flip);
        put(&mut result, &format!("{base}/y-flip"), instance.y_flip);
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}
impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileInstance;
    #[test]
    fn observes_binding_placement_and_signed_painter_order() {
        let plane = MaterializedLayer3Plane {
            source_digest: [0x5a; 32],
            placement: Layer3Placement::AboveEntities,
            instances: vec![TileInstance {
                tile_index: 3,
                palette_index: 2,
                x: -8,
                y: 16,
                x_flip: true,
                y_flip: false,
            }],
        };
        let observed = observe_materialized_layer3_plane(&plane);
        assert_eq!(
            observed.get("layer3-plane/placement"),
            Some("above-entities")
        );
        assert_eq!(observed.get("layer3-plane/instances/0000/x"), Some("-8"));
        assert_eq!(
            observed.get("layer3-plane/instances/0000/x-flip"),
            Some("true")
        );
        assert_eq!(
            observed.get("layer3-plane/source-sha256").unwrap().len(),
            64
        );
    }
}
