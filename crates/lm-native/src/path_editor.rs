use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    path_editor_forms::{EdgeForm, NodeForm},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, OverworldPathController};
use lm_overworld::PathGraphEdit;
use std::path::PathBuf;

mod document_io;
mod form_fields;

use form_fields::{edge_fields, node_fields};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    path: PathBuf,
    bytes: Vec<u8>,
    require_reciprocal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Nodes,
    Edges,
}

#[derive(Default)]
pub(crate) struct PathEditor {
    controller: Option<OverworldPathController>,
    pending_open: Option<PendingOpen>,
    panel: Panel,
    node_index: usize,
    node: NodeForm,
    node_key: Option<(u64, usize)>,
    edge_index: usize,
    edge: EdgeForm,
    edge_key: Option<(u64, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl PathEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for path loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for path persistence to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(document_io::pending_open) {
                Ok(pending) => self.pending_open = Some(pending),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        self.show_open_configuration(context, catalog);
        if self.controller.is_some() {
            self.load_forms();
            egui::Window::new(text(catalog, Key::PathEditorTitle))
                .default_size([700.0, 560.0])
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn show_open_configuration(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new(text(catalog, Key::PathEditorPolicyTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.checkbox(
                        &mut pending.require_reciprocal,
                        text(catalog, Key::PathEditorReciprocalPolicy),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::PathEditorCancel)).clicked() {
                        self.pending_open = None;
                    }
                    if ui.button(text(catalog, Key::PathEditorOpen)).clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.panel,
                Panel::Nodes,
                text(catalog, Key::PathEditorNodes),
            );
            ui.selectable_value(
                &mut self.panel,
                Panel::Edges,
                text(catalog, Key::PathEditorEdges),
            );
        });
        ui.separator();
        let edit = match self.panel {
            Panel::Nodes => self.show_nodes(ui, catalog),
            Panel::Edges => self.show_edges(ui, catalog),
        };
        if let Some(edit) = edit {
            match edit {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let can_undo = controller.can_undo();
        let can_redo = controller.can_redo();
        let modified = controller.is_modified();
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, Key::PathEditorUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, Key::PathEditorRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            if ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, Key::PathEditorSave)),
                )
                .clicked()
            {
                save_requested = true;
            }
            ui.label(if modified {
                text(catalog, Key::PathEditorModified)
            } else {
                text(catalog, Key::PathEditorSaved)
            });
        });
        let mut changed = false;
        if let Some(controller) = self.controller.as_mut() {
            if let Some(undo) = history {
                let revision = controller.revision();
                let result = if undo {
                    controller.undo(revision)
                } else {
                    controller.redo(revision)
                };
                match result {
                    Ok(_) => changed = true,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn show_nodes(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<PathGraphEdit>, String>> {
        let controller = self.controller.as_ref()?;
        let nodes = &controller.graph().nodes;
        if !nodes.is_empty() {
            self.node_index = self.node_index.min(nodes.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.node_index, 0..=nodes.len().saturating_sub(1))
                .text(text(catalog, Key::PathEditorNode)),
        );
        if self.node_key != Some((controller.revision(), self.node_index)) {
            self.node = nodes
                .get(self.node_index)
                .copied()
                .map_or_else(NodeForm::default, NodeForm::load);
            self.node_key = Some((controller.revision(), self.node_index));
        }
        node_fields(ui, &mut self.node, catalog);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui
                .button(text(catalog, Key::PathEditorUpsertNode))
                .clicked();
            remove = ui
                .add_enabled(
                    !nodes.is_empty(),
                    egui::Button::new(text(catalog, Key::PathEditorRemoveSelected)),
                )
                .clicked();
        });
        if upsert {
            Some(
                self.node
                    .parse()
                    .map(|node| vec![PathGraphEdit::UpsertNode(node)]),
            )
        } else if remove {
            Some(Ok(vec![PathGraphEdit::RemoveNode(
                nodes[self.node_index].id,
            )]))
        } else {
            None
        }
    }

    fn show_edges(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<PathGraphEdit>, String>> {
        let controller = self.controller.as_ref()?;
        let edges = &controller.graph().edges;
        if !edges.is_empty() {
            self.edge_index = self.edge_index.min(edges.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.edge_index, 0..=edges.len().saturating_sub(1))
                .text(text(catalog, Key::PathEditorEdge)),
        );
        if self.edge_key != Some((controller.revision(), self.edge_index)) {
            self.edge = edges
                .get(self.edge_index)
                .copied()
                .map_or_else(EdgeForm::default, |edge| {
                    EdgeForm::load_with_edges(edge, edges)
                });
            self.edge_key = Some((controller.revision(), self.edge_index));
        }
        edge_fields(ui, &mut self.edge, catalog);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui
                .button(text(catalog, Key::PathEditorUpsertEdge))
                .clicked();
            remove = ui
                .add_enabled(
                    !edges.is_empty(),
                    egui::Button::new(text(catalog, Key::PathEditorRemoveSelected)),
                )
                .clicked();
        });
        if upsert {
            Some(
                self.edge
                    .parse_pair()
                    .map(|edges| edges.into_iter().map(PathGraphEdit::UpsertEdge).collect()),
            )
        } else if remove {
            let edge = edges[self.edge_index];
            let mut edits = vec![PathGraphEdit::RemoveEdge {
                from: edge.from,
                direction: edge.direction,
            }];
            if self.edge.reciprocal {
                edits.push(PathGraphEdit::RemoveEdge {
                    from: edge.to,
                    direction: edge.direction.opposite(),
                });
            }
            Some(Ok(edits))
        } else {
            None
        }
    }

    fn apply_edits(&mut self, edits: &[PathGraphEdit]) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) = controller.apply_edits(controller.revision(), edits) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn load_forms(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if !controller.graph().nodes.is_empty() {
            self.node_index = self.node_index.min(controller.graph().nodes.len() - 1);
        }
        if !controller.graph().edges.is_empty() {
            self.edge_index = self.edge_index.min(controller.graph().edges.len() - 1);
        }
    }

    fn invalidate(&mut self) {
        self.node_key = None;
        self.edge_key = None;
    }

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, Key::PathEditorDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, Key::PathEditorDiscardNotice));
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::PathEditorCancel)).clicked() {
                        self.pending_close = None;
                    }
                    if ui.button(text(catalog, Key::PathEditorDiscard)).clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::PathEditorErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::PathEditorOk)).clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_open = None;
        self.pending_close = None;
        self.invalidate();
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod localization_tests {
    #[test]
    fn complete_path_editor_surface_has_no_literal_widget_text() {
        let sources = [
            include_str!("path_editor.rs"),
            include_str!("path_editor/form_fields.rs"),
        ]
        .join("\n");
        for literal in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
            ".text(\"",
        ] {
            assert!(
                !sources.contains(literal),
                "literal path widget text: {literal}"
            );
        }
    }
}
