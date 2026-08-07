use eframe::egui;
use lm_app::{LocalizationCatalog, UiTextKey};
use std::sync::OnceLock;

use crate::frontend_ui::localized_text;

const ORIGINAL_TOPIC_INDEX: &str = include_str!("lunar_magic_363_help_topics.tsv");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpTopic {
    pub title: UiTextKey,
    pub body: UiTextKey,
}

pub(crate) const HELP_TOPICS: &[HelpTopic] = &[
    HelpTopic {
        title: UiTextKey::HelpGettingStartedTitle,
        body: UiTextKey::HelpGettingStartedBody,
    },
    HelpTopic {
        title: UiTextKey::HelpLevelEditingTitle,
        body: UiTextKey::HelpLevelEditingBody,
    },
    HelpTopic {
        title: UiTextKey::HelpEntrancesExitsTitle,
        body: UiTextKey::HelpEntrancesExitsBody,
    },
    HelpTopic {
        title: UiTextKey::HelpMap16GraphicsTitle,
        body: UiTextKey::HelpMap16GraphicsBody,
    },
    HelpTopic {
        title: UiTextKey::HelpPalettesBackgroundsLayer3Title,
        body: UiTextKey::HelpPalettesBackgroundsLayer3Body,
    },
    HelpTopic {
        title: UiTextKey::HelpOverworldEditingTitle,
        body: UiTextKey::HelpOverworldEditingBody,
    },
    HelpTopic {
        title: UiTextKey::HelpImportExportRecoveryTitle,
        body: UiTextKey::HelpImportExportRecoveryBody,
    },
    HelpTopic {
        title: UiTextKey::HelpCompatibilityDiagnosticsTitle,
        body: UiTextKey::HelpCompatibilityDiagnosticsBody,
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

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if !self.open {
            return;
        }
        let matching = matching_topic_indexes(&self.query, catalog);
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
        egui::Window::new(localized_text(catalog, UiTextKey::HelpWindowTitle))
            .open(&mut self.open)
            .default_size([760.0, 520.0])
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(localized_text(catalog, UiTextKey::HelpSearchLabel));
                    ui.text_edit_singleline(&mut self.query);
                });
                ui.separator();
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                        ui.strong(localized_text(catalog, UiTextKey::HelpRustWorkflowGuides));
                        for index in matching_topic_indexes(&self.query, catalog) {
                            if ui
                                .selectable_label(
                                    self.selected_original.is_none() && self.selected == index,
                                    localized_text(catalog, HELP_TOPICS[index].title),
                                )
                                .clicked()
                            {
                                self.selected = index;
                                self.selected_original = None;
                            }
                        }
                        if !matching_original.is_empty() {
                            ui.separator();
                            ui.strong(localized_text(catalog, UiTextKey::HelpOriginalCommandIndex));
                            for index in matching_original_topic_indexes(&self.query) {
                                let topic = original_topics()[index];
                                let indent = "  ".repeat(topic.depth.saturating_sub(1));
                                let label = if topic.route.is_empty() {
                                    format!("{indent}▸ {}", topic.title)
                                } else {
                                    format!("{indent}{}", topic.title)
                                };
                                if ui
                                    .selectable_label(self.selected_original == Some(index), label)
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
                                ui.label(localized_text(catalog, UiTextKey::HelpOriginalSection));
                            } else {
                                ui.label(localized_text(catalog, UiTextKey::HelpOriginalRoute));
                                ui.monospace(topic.route);
                            }
                            ui.add_space(8.0);
                            ui.label(localized_text(catalog, UiTextKey::HelpOriginalNotice));
                        } else if let Some(topic) = HELP_TOPICS.get(self.selected) {
                            ui.heading(localized_text(catalog, topic.title));
                            ui.add_space(8.0);
                            ui.label(localized_text(catalog, topic.body));
                        } else {
                            ui.label(localized_text(catalog, UiTextKey::HelpNoMatches));
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

fn matching_topic_indexes(query: &str, catalog: Option<&LocalizationCatalog>) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    HELP_TOPICS
        .iter()
        .enumerate()
        .filter_map(|(index, topic)| {
            (query.is_empty()
                || localized_text(catalog, topic.title)
                    .to_ascii_lowercase()
                    .contains(&query)
                || localized_text(catalog, topic.body)
                    .to_ascii_lowercase()
                    .contains(&query))
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
            .map(|topic| format!("{} {}", topic.title.english(), topic.body.english()))
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
        let overworld = matching_topic_indexes("OVERWORLD", None);
        assert!(overworld.contains(&5));
        assert!(matching_topic_indexes("checksum", None).len() >= 2);
        assert!(matching_topic_indexes("not a real topic", None).is_empty());
        assert_eq!(matching_topic_indexes(" ", None).len(), HELP_TOPICS.len());
    }

    #[test]
    fn topic_search_uses_the_installed_translation() {
        let catalog = LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| {
                let text = if key == UiTextKey::HelpLevelEditingTitle {
                    "Édition du niveau".into()
                } else {
                    format!("traduit-{key:?}")
                };
                (key, text)
            }),
        )
        .unwrap();
        assert_eq!(matching_topic_indexes("NIVEAU", Some(&catalog)), vec![1]);
        assert!(matching_topic_indexes("Level editing", Some(&catalog)).is_empty());
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
