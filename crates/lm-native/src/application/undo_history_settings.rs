use eframe::egui;
use lm_app::AppState;

#[derive(Default)]
pub(super) struct UndoHistorySettings {
    open: bool,
    draft: usize,
}

impl UndoHistorySettings {
    pub(super) fn open(&mut self, current: usize) {
        self.draft = current;
        self.open = true;
    }

    pub(super) fn show(&mut self, context: &egui::Context) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut accepted = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new("Undo History")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Snapshots retained for the level and overworld editors");
                ui.add(
                    egui::DragValue::new(&mut self.draft)
                        .range(0..=AppState::MAX_UNDO_SNAPSHOT_LIMIT),
                );
                ui.small(
                    "0 or 1 disables Undo. Lunar Magic 3.63 defaults to 33 and allows at most 51.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        accepted = Some(self.draft);
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        accepted
    }
}

pub(super) fn encode_preference(limit: usize) -> String {
    format!("v1:{limit}")
}

pub(super) fn decode_preference(encoded: &str) -> Result<usize, String> {
    let value = encoded
        .strip_prefix("v1:")
        .ok_or_else(|| "unknown undo-history preference version".to_owned())?
        .parse::<usize>()
        .map_err(|_| "undo-history preference is not an unsigned integer".to_owned())?;
    if value > AppState::MAX_UNDO_SNAPSHOT_LIMIT {
        return Err(format!(
            "undo-history preference exceeds {}",
            AppState::MAX_UNDO_SNAPSHOT_LIMIT
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_round_trips_every_original_boundary_and_rejects_bad_values() {
        for value in [0, 1, 2, 33, 51] {
            assert_eq!(decode_preference(&encode_preference(value)).unwrap(), value);
        }
        assert!(decode_preference("33").is_err());
        assert!(decode_preference("v2:33").is_err());
        assert!(decode_preference("v1:-1").is_err());
        assert!(decode_preference("v1:52").is_err());
        assert!(decode_preference("v1:33:extra").is_err());
    }
}
