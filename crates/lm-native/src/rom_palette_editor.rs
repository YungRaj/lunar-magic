use crate::{document_loader::DocumentLoader, native_clipboard};
use eframe::egui;
use lm_app::{
    AppState, Command, PaletteController, PaletteControllerEdit, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_graphics::{Bgr555, PaletteChange, PaletteEntryOwner, Rgb8};

mod commit;
mod lifecycle;
mod ownership;
pub(crate) mod transfer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PalettePasteTarget {
    Color,
    Row,
}

struct Workspace {
    controller: PaletteController,
    profile: RevisionProfile,
    slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
}

struct PendingLoad {
    profiled: ProfiledControllerSnapshot,
}

#[derive(Default)]
pub(crate) struct RomPaletteEditor {
    workspace: Option<Workspace>,
    selected: usize,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    transfer_loader: DocumentLoader,
    transfer_persistence: crate::persistence_worker::PersistenceWorker,
    pending_transfer: Option<transfer::PendingTransfer>,
    pending_row_start: Option<usize>,
    rgb_expansion: Option<lm_graphics::RgbChannelExpansion>,
    palette_mask: Vec<u8>,
    palette_mask_edit: bool,
    palette_paste_target: Option<PalettePasteTarget>,
}

impl RomPaletteEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, revision);
        }
        self.poll_transfer_file_io(context, revision);
        let mut command = match self.manifest_loader.show(context, revision) {
            Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if self.workspace.is_some() {
            egui::Window::new("ROM Palette Editor")
                .default_size([600.0, 560.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        let transfer_busy =
            self.transfer_loader.is_running() || self.transfer_persistence.is_running();
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        self.raw_palette_file_controls(ui, stale, revision);
        self.palette_mask_controls(ui);
        let palette_locked = stale || self.transfer_loader.is_running();
        ui.add_enabled_ui(!palette_locked, |ui| {
            self.palette_surface(ui, palette_locked, pasted);
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit palette to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged palette changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn palette_surface(&mut self, ui: &mut egui::Ui, stale: bool, pasted: Option<String>) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("palette workspace is closed".into());
            return;
        };
        let colors = workspace.controller.palette().colors.clone();
        if self.palette_mask.len() != colors.len() {
            self.palette_mask = vec![1; colors.len()];
        }
        self.selected = self.selected.min(colors.len().saturating_sub(1));
        if let Some(result) = self.palette_grid(ui, &colors, stale) {
            match result {
                Ok(changes) => self.apply(PaletteControllerEdit::ApplyChanges(changes)),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(text) = pasted {
            if stale {
                if self.palette_paste_target.take().is_some() {
                    self.error = Some("palette paste arrived while editing was unavailable".into());
                }
            } else if let Some(target) = self.palette_paste_target.take() {
                match palette_paste_changes(&text, self.selected, colors.len(), target) {
                    Ok(changes) => self.apply(PaletteControllerEdit::ApplyChanges(changes)),
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if let Some(color) = colors.get(self.selected).copied() {
            self.selected_color_controls(ui, stale, color);
        }
    }

    fn palette_grid(
        &mut self,
        ui: &mut egui::Ui,
        colors: &[Bgr555],
        stale: bool,
    ) -> Option<Result<Vec<PaletteChange>, String>> {
        let mut native_paste = None;
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                egui::Grid::new("rom-palette-grid")
                    .spacing([3.0, 3.0])
                    .show(ui, |ui| {
                        for (index, color) in colors.iter().copied().enumerate() {
                            let rgb = color.to_rgb8();
                            let label = if self.palette_mask[index] == 0 {
                                "X"
                            } else if index == self.selected {
                                "•"
                            } else {
                                ""
                            };
                            let response = ui.add_sized(
                                [25.0, 25.0],
                                egui::Button::new(label)
                                    .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)),
                            );
                            if response.clicked() {
                                if self.palette_mask_edit {
                                    toggle_palette_mask(
                                        &mut self.palette_mask,
                                        index,
                                        ui.input(|input| input.modifiers.alt),
                                    );
                                } else {
                                    self.selected = index;
                                    let modifiers = ui.input(|input| input.modifiers);
                                    if modifiers.ctrl {
                                        let encoded = if modifiers.alt {
                                            palette_row(colors, index).and_then(|row| {
                                                native_clipboard::copy_palette_row_to_system(
                                                    ui.ctx(),
                                                    row,
                                                )
                                            })
                                        } else {
                                            native_clipboard::copy_palette_color_to_system(
                                                ui.ctx(),
                                                color,
                                            )
                                        };
                                        match encoded {
                                            Ok(()) => {}
                                            Err(error) => self.error = Some(error),
                                        }
                                    }
                                }
                            }
                            if !stale && !self.palette_mask_edit && response.secondary_clicked() {
                                let modifiers = ui.input(|input| input.modifiers);
                                if modifiers.ctrl {
                                    self.selected = index;
                                    let target = if modifiers.alt {
                                        PalettePasteTarget::Row
                                    } else {
                                        PalettePasteTarget::Color
                                    };
                                    let request = match target {
                                        PalettePasteTarget::Color => {
                                            native_clipboard::request_palette_color_paste(ui.ctx())
                                                .map(|value| {
                                                    value.map(|color| {
                                                        vec![PaletteChange { index, color }]
                                                    })
                                                })
                                        }
                                        PalettePasteTarget::Row => {
                                            native_clipboard::request_palette_row_paste(ui.ctx())
                                                .map(|value| {
                                                    value.map(|row| {
                                                        let start = index / 16 * 16;
                                                        row.into_iter()
                                                            .enumerate()
                                                            .map(|(offset, color)| PaletteChange {
                                                                index: start + offset,
                                                                color,
                                                            })
                                                            .collect()
                                                    })
                                                })
                                        }
                                    };
                                    match request {
                                        Ok(Some(changes)) => native_paste = Some(Ok(changes)),
                                        Ok(None) => self.palette_paste_target = Some(target),
                                        Err(error) => native_paste = Some(Err(error)),
                                    }
                                }
                            }
                            if index % 16 == 15 {
                                ui.end_row();
                            }
                        }
                    });
            });
        native_paste
    }

    fn selected_color_controls(&mut self, ui: &mut egui::Ui, stale: bool, color: Bgr555) {
        let rgb = color.to_rgb8();
        let mut value = [rgb.red, rgb.green, rgb.blue];
        ui.label(format!(
            "Color {:03X} — raw BGR555 {:04X}",
            self.selected, color.0
        ));
        let owner = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.controller.ownership().owner(self.selected));
        let editable = ownership::show(ui, owner);
        let row_colors: Option<[Bgr555; 16]> = palette_row(
            &self
                .workspace
                .as_ref()
                .expect("palette controls require an open workspace")
                .controller
                .palette()
                .colors,
            self.selected,
        )
        .ok()
        .and_then(|row| row.try_into().ok());
        let row_editable = self.workspace.as_ref().is_some_and(|workspace| {
            let start = self.selected / 16 * 16;
            row_colors.is_some()
                && (start..start + 16).all(|index| {
                    workspace.controller.ownership().owner(index)
                        == Some(PaletteEntryOwner::Editable)
                })
        });
        ui.horizontal(|ui| {
            if ui.button("Copy color").clicked() {
                if let Err(error) = native_clipboard::copy_palette_color_to_system(ui.ctx(), color)
                {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale && editable, egui::Button::new("Paste color"))
                .clicked()
            {
                match native_clipboard::request_palette_color_paste(ui.ctx()) {
                    Ok(Some(color)) => {
                        self.apply(PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                            index: self.selected,
                            color,
                        }]))
                    }
                    Ok(None) => self.palette_paste_target = Some(PalettePasteTarget::Color),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(row_colors.is_some(), egui::Button::new("Copy row"))
                .clicked()
            {
                if let Err(error) = native_clipboard::copy_palette_row_to_system(
                    ui.ctx(),
                    row_colors.as_ref().expect("enabled row is complete"),
                ) {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale && row_editable, egui::Button::new("Paste row"))
                .clicked()
            {
                match native_clipboard::request_palette_row_paste(ui.ctx()) {
                    Ok(Some(colors)) => {
                        let start = self.selected / 16 * 16;
                        self.apply(PaletteControllerEdit::ApplyChanges(
                            colors
                                .into_iter()
                                .enumerate()
                                .map(|(offset, color)| PaletteChange {
                                    index: start + offset,
                                    color,
                                })
                                .collect(),
                        ));
                    }
                    Ok(None) => self.palette_paste_target = Some(PalettePasteTarget::Row),
                    Err(error) => self.error = Some(error),
                }
            }
        });
        ui.small(
            "Ctrl+left/right copies or pastes a color; add Alt for its complete 16-color row.",
        );
        if ui
            .add_enabled_ui(!stale && editable, |ui| {
                ui.color_edit_button_srgb(&mut value)
            })
            .inner
            .changed()
        {
            self.apply(PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                index: self.selected,
                color: Bgr555::from_rgb8(Rgb8 {
                    red: value[0],
                    green: value[1],
                    blue: value[2],
                }),
            }]));
        }
    }

    fn palette_mask_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.palette_mask_edit, "Palette mask edit mode");
            if ui
                .add_enabled(self.palette_mask_edit, egui::Button::new("Enable all"))
                .clicked()
            {
                self.palette_mask.fill(1);
            }
            if ui
                .add_enabled(self.palette_mask_edit, egui::Button::new("Disable all"))
                .clicked()
            {
                self.palette_mask.fill(0);
            }
        });
        if self.palette_mask_edit {
            ui.small("Click a color to enable/disable it for .palmask export; hold Alt to change its entire row.");
        }
    }
    fn apply(&mut self, edit: PaletteControllerEdit) {
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("palette workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
            self.error = Some(error.to_string());
        }
    }
}

fn toggle_palette_mask(mask: &mut [u8], index: usize, whole_row: bool) {
    let Some(entry) = mask.get(index) else {
        return;
    };
    let value = u8::from(*entry == 0);
    if whole_row {
        let start = index / 16 * 16;
        let end = (start + 16).min(mask.len());
        mask[start..end].fill(value);
    } else {
        mask[index] = value;
    }
}

fn palette_row(colors: &[Bgr555], index: usize) -> Result<&[Bgr555], String> {
    let start = index / 16 * 16;
    let end = start
        .checked_add(16)
        .ok_or_else(|| "palette-row range overflow".to_string())?;
    colors
        .get(start..end)
        .ok_or_else(|| "selected color does not belong to a complete 16-color row".to_string())
}

fn palette_paste_changes(
    text: &str,
    selected: usize,
    color_count: usize,
    target: PalettePasteTarget,
) -> Result<Vec<PaletteChange>, String> {
    match target {
        PalettePasteTarget::Color => {
            if selected >= color_count {
                return Err("selected palette color is out of range".into());
            }
            Ok(vec![PaletteChange {
                index: selected,
                color: native_clipboard::decode_palette_color(text)?,
            }])
        }
        PalettePasteTarget::Row => {
            let start = selected / 16 * 16;
            let end = start
                .checked_add(16)
                .ok_or_else(|| "palette-row range overflow".to_string())?;
            if end > color_count {
                return Err("selected color does not belong to a complete 16-color row".into());
            }
            Ok(native_clipboard::decode_palette_row(text)?
                .into_iter()
                .enumerate()
                .map(|(offset, color)| PaletteChange {
                    index: start + offset,
                    color,
                })
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PalettePasteTarget, palette_paste_changes, toggle_palette_mask};
    use crate::native_clipboard;
    use lm_graphics::Bgr555;

    #[test]
    fn palette_mask_click_and_alt_row_preserve_unrelated_and_partial_rows() {
        let mut mask = vec![0x80; 257];
        toggle_palette_mask(&mut mask, 17, false);
        assert_eq!(mask[17], 0);
        assert_eq!(mask[16], 0x80);

        toggle_palette_mask(&mut mask, 17, true);
        assert!(mask[16..32].iter().all(|entry| *entry == 1));
        assert_eq!(mask[15], 0x80);
        assert_eq!(mask[32], 0x80);

        toggle_palette_mask(&mut mask, 256, true);
        assert_eq!(mask[256], 0);
        assert_eq!(mask[255], 0x80);
    }

    #[test]
    fn palette_row_paste_targets_the_complete_selected_row_and_rejects_partial_tail() {
        let colors: [Bgr555; 16] = std::array::from_fn(|index| {
            Bgr555(u16::try_from(index + 1).expect("sixteen palette entries fit u16"))
        });
        let text = native_clipboard::encode_palette_row(&colors).unwrap();
        let changes = palette_paste_changes(&text, 19, 257, PalettePasteTarget::Row).unwrap();
        assert_eq!(changes.len(), 16);
        assert_eq!((changes[0].index, changes[15].index), (16, 31));
        assert_eq!(
            (changes[0].color, changes[15].color),
            (colors[0], colors[15])
        );
        assert!(palette_paste_changes(&text, 256, 257, PalettePasteTarget::Row).is_err());
    }
}
