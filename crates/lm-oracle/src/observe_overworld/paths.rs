use super::{Observation, put};
use lm_overworld::OverworldPathGraph;

#[must_use]
pub fn observe_overworld_paths(paths: &OverworldPathGraph) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "overworld/paths/nodes/count",
        paths.nodes.len(),
    );
    for (index, node) in paths.nodes.iter().enumerate() {
        let base = format!("overworld/paths/nodes/{index:04x}");
        put(&mut result, &format!("{base}/id"), node.id);
        put(&mut result, &format!("{base}/x"), node.x);
        put(&mut result, &format!("{base}/y"), node.y);
        put(
            &mut result,
            &format!("{base}/submap"),
            node.submap.encoded(),
        );
        put(
            &mut result,
            &format!("{base}/level"),
            node.level
                .map_or_else(|| "none".into(), |value| value.to_string()),
        );
        put(&mut result, &format!("{base}/raw-flags"), node.raw_flags);
    }
    put(
        &mut result,
        "overworld/paths/edges/count",
        paths.edges.len(),
    );
    for (index, edge) in paths.edges.iter().enumerate() {
        let base = format!("overworld/paths/edges/{index:04x}");
        put(&mut result, &format!("{base}/from"), edge.from);
        put(&mut result, &format!("{base}/to"), edge.to);
        put(
            &mut result,
            &format!("{base}/direction"),
            format!("{:?}", edge.direction),
        );
        put(
            &mut result,
            &format!("{base}/exit-index"),
            edge.exit_index
                .map_or_else(|| "none".into(), |value| value.to_string()),
        );
        put(&mut result, &format!("{base}/raw-flags"), edge.raw_flags);
    }
    result
}
