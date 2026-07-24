use crate::{EditorOverlay, EditorOverlayFile};
use lm_oracle::Observation;

/// Produces a canonical semantic snapshot preserving overlay painter order.
#[must_use]
pub fn observe_editor_overlays(file: &EditorOverlayFile) -> Observation {
    let mut observation = Observation::new();
    put(
        &mut observation,
        "editor-overlays/count",
        file.overlays.len(),
    );
    for (index, overlay) in file.overlays.iter().enumerate() {
        let base = format!("editor-overlays/{index:04x}");
        match overlay {
            EditorOverlay::Grid(grid) => {
                put(&mut observation, &format!("{base}/kind"), "grid");
                put(&mut observation, &format!("{base}/origin-x"), grid.origin_x);
                put(&mut observation, &format!("{base}/origin-y"), grid.origin_y);
                put(
                    &mut observation,
                    &format!("{base}/cell-width"),
                    grid.cell_width,
                );
                put(
                    &mut observation,
                    &format!("{base}/cell-height"),
                    grid.cell_height,
                );
                color(&mut observation, &base, "color", grid.color);
            }
            EditorOverlay::Selection(selection) => {
                put(&mut observation, &format!("{base}/kind"), "selection");
                put(
                    &mut observation,
                    &format!("{base}/left"),
                    selection.bounds.left,
                );
                put(
                    &mut observation,
                    &format!("{base}/top"),
                    selection.bounds.top,
                );
                put(
                    &mut observation,
                    &format!("{base}/right"),
                    selection.bounds.right,
                );
                put(
                    &mut observation,
                    &format!("{base}/bottom"),
                    selection.bounds.bottom,
                );
                color(&mut observation, &base, "light", selection.light);
                color(&mut observation, &base, "dark", selection.dark);
                put(
                    &mut observation,
                    &format!("{base}/dash-length"),
                    selection.dash_length,
                );
                put(&mut observation, &format!("{base}/phase"), selection.phase);
            }
        }
    }
    observation
}

fn color(observation: &mut Observation, base: &str, name: &str, color: crate::Rgba) {
    put(
        observation,
        &format!("{base}/{name}"),
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            color.red, color.green, color.blue, color.alpha
        ),
    );
}

fn put(observation: &mut Observation, path: &str, value: impl ObservationValue) {
    observation
        .insert(path, value.into_value())
        .expect("overlay observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GridOverlay, Rgba};

    #[test]
    fn observation_retains_painter_index_signed_geometry_and_color() {
        let file = EditorOverlayFile {
            overlays: vec![EditorOverlay::Grid(GridOverlay {
                origin_x: -8,
                origin_y: 3,
                cell_width: 16,
                cell_height: 32,
                color: Rgba {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 128,
                },
            })],
        };
        let observation = observe_editor_overlays(&file);
        assert_eq!(observation.get("editor-overlays/0000/kind"), Some("grid"));
        assert_eq!(observation.get("editor-overlays/0000/origin-x"), Some("-8"));
        assert_eq!(
            observation.get("editor-overlays/0000/color"),
            Some("01020380")
        );
    }
}
