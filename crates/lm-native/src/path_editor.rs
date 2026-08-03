use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    path_editor_forms::{EdgeForm, NodeForm},
};
use eframe::egui;
use lm_app::OverworldPathController;
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
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
        self.show_open_configuration(context);
        if self.controller.is_some() {
            self.load_forms();
            egui::Window::new("Portable Overworld Path Editor")
                .default_size([700.0, 560.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn show_open_configuration(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Path validation policy")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.checkbox(
                        &mut pending.require_reciprocal,
                        "Require reciprocal edges unless explicitly one-way",
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Nodes, "Nodes");
            ui.selectable_value(&mut self.panel, Panel::Edges, "Edges");
        });
        ui.separator();
        let edit = match self.panel {
            Panel::Nodes => self.show_nodes(ui),
            Panel::Edges => self.show_edges(ui),
        };
        if let Some(edit) = edit {
            match edit {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
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
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            if ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked()
            {
                save_requested = true;
            }
            ui.label(if modified { "Modified" } else { "Saved" });
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

    fn show_nodes(&mut self, ui: &mut egui::Ui) -> Option<Result<Vec<PathGraphEdit>, String>> {
        let controller = self.controller.as_ref()?;
        let nodes = &controller.graph().nodes;
        if !nodes.is_empty() {
            self.node_index = self.node_index.min(nodes.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.node_index, 0..=nodes.len().saturating_sub(1)).text("Node"),
        );
        if self.node_key != Some((controller.revision(), self.node_index)) {
            self.node = nodes
                .get(self.node_index)
                .copied()
                .map_or_else(NodeForm::default, NodeForm::load);
            self.node_key = Some((controller.revision(), self.node_index));
        }
        node_fields(ui, &mut self.node);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui.button("Upsert node").clicked();
            remove = ui
                .add_enabled(!nodes.is_empty(), egui::Button::new("Remove selected"))
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

    fn show_edges(&mut self, ui: &mut egui::Ui) -> Option<Result<Vec<PathGraphEdit>, String>> {
        let controller = self.controller.as_ref()?;
        let edges = &controller.graph().edges;
        if !edges.is_empty() {
            self.edge_index = self.edge_index.min(edges.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.edge_index, 0..=edges.len().saturating_sub(1)).text("Edge"),
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
        edge_fields(ui, &mut self.edge);
        let mut upsert = false;
        let mut remove = false;
        ui.horizontal(|ui| {
            upsert = ui.button("Upsert edge").clicked();
            remove = ui
                .add_enabled(!edges.is_empty(), egui::Button::new("Remove selected"))
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

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved overworld paths")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved path changes?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Path editor error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
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
