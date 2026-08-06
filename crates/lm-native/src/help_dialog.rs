use eframe::egui;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpTopic {
    pub title: &'static str,
    pub body: &'static str,
}

pub(crate) const HELP_TOPICS: &[HelpTopic] = &[
    HelpTopic {
        title: "Getting started",
        body: "Open a clean Super Mario World ROM with File > Open. Select a level from the level field, then use the Editors menu to open level, Map16, graphics, palette, ExAnimation, Layer 3, and overworld tools. Save writes the checked in-memory ROM transaction; Save As publishes a new file.",
    },
    HelpTopic {
        title: "Level editing",
        body: "Use the level canvas to select, place, drag, resize, duplicate, and remove objects and sprites. The canvas fits one 256 by 224 SNES screen into the available pane and recomputes its scale when the window changes size. View toggles control Layer 1, Layer 2, Layer 3, and sprites without deleting their data.",
    },
    HelpTopic {
        title: "Entrances and exits",
        body: "Primary, midway, secondary entrances, and screen exits are edited through their typed forms. Screen and coordinate fields are bounded to their native packed widths. Changes participate in the same undo, redo, checksum, save, and reopen transaction as level objects and sprites.",
    },
    HelpTopic {
        title: "Map16 and graphics",
        body: "The Map16 editor changes visual quadrants, palette, priority, flips, and acts-like behavior. Graphics and ExGFX tools import, export, decode, and edit the active slots. Super GFX Bypass selects per-level foreground, background, and sprite files; animation options update the live preview.",
    },
    HelpTopic {
        title: "Palettes, backgrounds, and Layer 3",
        body: "Palette editors provide shared and per-level colors with protected ownership checks. Background and Layer 3 editors expose tilemaps, offsets, graphics selection, priority, and composition. Preview and image export use the staged palette and animation phase currently shown in the editor.",
    },
    HelpTopic {
        title: "Overworld editing",
        body: "Overworld tools edit Layer 1 paths and events, Layer 2 appearance, level tiles, names, messages, warps, player starts, and special-event state. Each editor stages a checked revision and can be undone before or after saving.",
    },
    HelpTopic {
        title: "Import, export, and recovery",
        body: "Level workflows support one-level MWL transfer, directory batch import, all-level export, and PNG or BMP image export. Restore points preserve ROM and associated files. Crash recovery records unsaved ROM revisions and offers them on the next launch without overwriting the last saved file.",
    },
    HelpTopic {
        title: "Compatibility diagnostics",
        body: "Help > Compatibility diagnostics creates a path-free report describing the build, ROM identity, mapper, checksum, revision profile, runtime generations, and current editor state. Copy that report when a ROM or feature behaves differently from Lunar Magic 3.63.",
    },
];

#[derive(Default)]
pub(crate) struct HelpDialog {
    open: bool,
    query: String,
    selected: usize,
}

impl HelpDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if !self.open {
            return;
        }
        let matching = matching_topic_indexes(&self.query);
        if !matching.contains(&self.selected) {
            self.selected = matching.first().copied().unwrap_or(0);
        }
        egui::Window::new("Lunar Magic Rust Help")
            .open(&mut self.open)
            .default_size([760.0, 520.0])
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.query);
                });
                ui.separator();
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                        for index in matching_topic_indexes(&self.query) {
                            if ui
                                .selectable_label(self.selected == index, HELP_TOPICS[index].title)
                                .clicked()
                            {
                                self.selected = index;
                            }
                        }
                    });
                    egui::ScrollArea::vertical().show(&mut columns[1], |ui| {
                        if let Some(topic) = HELP_TOPICS.get(self.selected) {
                            ui.heading(topic.title);
                            ui.add_space(8.0);
                            ui.label(topic.body);
                        } else {
                            ui.label("No help topics match this search.");
                        }
                    });
                });
            });
    }
}

fn matching_topic_indexes(query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    HELP_TOPICS
        .iter()
        .enumerate()
        .filter_map(|(index, topic)| {
            (query.is_empty()
                || topic.title.to_ascii_lowercase().contains(&query)
                || topic.body.to_ascii_lowercase().contains(&query))
            .then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_topics_cover_every_primary_editor_family() {
        let corpus = HELP_TOPICS
            .iter()
            .map(|topic| format!("{} {}", topic.title, topic.body))
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        for term in [
            "level",
            "sprite",
            "map16",
            "graphics",
            "palette",
            "layer 3",
            "overworld",
            "entrance",
            "export",
            "recovery",
        ] {
            assert!(corpus.contains(term), "missing help coverage for {term}");
        }
    }

    #[test]
    fn topic_search_matches_titles_and_bodies_case_insensitively() {
        let overworld = matching_topic_indexes("OVERWORLD");
        assert!(overworld.contains(&5));
        assert!(matching_topic_indexes("checksum").len() >= 2);
        assert!(matching_topic_indexes("not a real topic").is_empty());
        assert_eq!(matching_topic_indexes(" ").len(), HELP_TOPICS.len());
    }

    #[test]
    fn opening_help_preserves_the_current_topic_and_query() {
        let mut dialog = HelpDialog {
            query: "map16".into(),
            selected: 3,
            ..HelpDialog::default()
        };
        dialog.open();
        dialog.open();
        assert!(dialog.open);
        assert_eq!(dialog.query, "map16");
        assert_eq!(dialog.selected, 3);
    }
}
