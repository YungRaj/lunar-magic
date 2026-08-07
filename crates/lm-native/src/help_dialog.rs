use eframe::egui;
use std::sync::OnceLock;

const ORIGINAL_TOPIC_INDEX: &str = include_str!("lunar_magic_363_help_topics.tsv");

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
    selected_original: Option<usize>,
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
        let matching_original = matching_original_topic_indexes(&self.query);
        if self
            .selected_original
            .is_some_and(|index| !matching_original.contains(&index))
        {
            self.selected_original = None;
        }
        if self.selected_original.is_none() && !matching.contains(&self.selected) {
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
                        ui.strong("Rust workflow guides");
                        for index in matching_topic_indexes(&self.query) {
                            if ui
                                .selectable_label(
                                    self.selected_original.is_none() && self.selected == index,
                                    HELP_TOPICS[index].title,
                                )
                                .clicked()
                            {
                                self.selected = index;
                                self.selected_original = None;
                            }
                        }
                        if !matching_original.is_empty() {
                            ui.separator();
                            ui.strong("Lunar Magic 3.63 command index");
                            for index in matching_original_topic_indexes(&self.query) {
                                let topic = original_topics()[index];
                                let indent = "  ".repeat(topic.depth.saturating_sub(1));
                                let label = if topic.route.is_empty() {
                                    format!("{indent}▸ {}", topic.title)
                                } else {
                                    format!("{indent}{}", topic.title)
                                };
                                if ui
                                    .selectable_label(
                                        self.selected_original == Some(index),
                                        label,
                                    )
                                    .clicked()
                                {
                                    self.selected_original = Some(index);
                                }
                            }
                        }
                    });
                    egui::ScrollArea::vertical().show(&mut columns[1], |ui| {
                        if let Some(topic) = self
                            .selected_original
                            .and_then(|index| original_topics().get(index).copied())
                        {
                            ui.heading(topic.title);
                            ui.add_space(8.0);
                            if topic.route.is_empty() {
                                ui.label("Original Lunar Magic 3.63 help section");
                            } else {
                                ui.label("Original Lunar Magic 3.63 help route");
                                ui.monospace(topic.route);
                            }
                            ui.add_space(8.0);
                            ui.label("This retained index identifies the original workflow without redistributing the proprietary help text. Search the Rust workflow guides for native usage and Compatibility diagnostics for ROM-specific state.");
                        } else if let Some(topic) = HELP_TOPICS.get(self.selected) {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OriginalTopic<'a> {
    depth: usize,
    route: &'a str,
    title: &'a str,
}

fn original_topics() -> &'static [OriginalTopic<'static>] {
    static TOPICS: OnceLock<Vec<OriginalTopic<'static>>> = OnceLock::new();
    TOPICS
        .get_or_init(|| {
            ORIGINAL_TOPIC_INDEX
                .lines()
                .filter_map(|line| {
                    let mut fields = line.splitn(3, '\t');
                    let depth = fields.next()?.parse().ok()?;
                    let route = fields.next()?;
                    let title = fields.next()?;
                    Some(OriginalTopic {
                        depth,
                        route,
                        title,
                    })
                })
                .collect()
        })
        .as_slice()
}

fn matching_original_topic_indexes(query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    original_topics()
        .iter()
        .enumerate()
        .filter_map(|(index, topic)| {
            (query.is_empty()
                || topic.title.to_ascii_lowercase().contains(&query)
                || topic.route.to_ascii_lowercase().contains(&query))
            .then_some(index)
        })
        .collect()
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

    #[test]
    fn retained_original_index_covers_every_routed_363_topic() {
        let topics = original_topics();
        assert_eq!(topics.len(), 314);
        assert_eq!(
            topics
                .iter()
                .filter(|topic| !topic.route.is_empty())
                .count(),
            281
        );
        assert_eq!(
            topics
                .iter()
                .filter_map(|topic| (!topic.route.is_empty()).then_some(topic.route))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            275
        );
        assert!(topics.iter().all(|topic| {
            (topic.route.is_empty()
                || (topic.route.starts_with("html/") && topic.route.ends_with(".htm")))
                && (1..=4).contains(&topic.depth)
                && !topic.title.trim().is_empty()
        }));
        for route in [
            "html/file_open_rom.htm",
            "html/editor_16x16.htm",
            "html/editor_8x8.htm",
            "html/editor_palette.htm",
            "html/editor_ov.htm",
            "html/help_contents.htm",
        ] {
            assert!(
                topics.iter().any(|topic| topic.route == route),
                "missing {route}"
            );
        }
    }

    #[test]
    fn original_topic_search_matches_titles_and_routes() {
        let map16 = matching_original_topic_indexes("16x16");
        assert!(!map16.is_empty());
        assert!(
            map16
                .iter()
                .any(|&index| original_topics()[index].route == "html/editor_16x16.htm")
        );
        assert!(!matching_original_topic_indexes("OVERWORLD").is_empty());
        assert!(matching_original_topic_indexes("not a real original topic").is_empty());
        assert_eq!(matching_original_topic_indexes(" ").len(), 314);
    }
}
