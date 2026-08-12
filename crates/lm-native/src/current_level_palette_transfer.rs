use crate::{
    dialogs,
    document_loader::DocumentLoader,
    persistence_worker::PersistenceWorker,
    rom_palette_editor::transfer::{
        PendingTransfer, decode_import, encode_raw_export, encode_rgb_export, encode_tpl_export,
        palette_import_requests,
    },
};
use eframe::egui;
use lm_app::{AppState, Command, ProfiledControllerSnapshot, UiTextKey};
use lm_graphics::{Palette, RgbChannelExpansion};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CurrentLevelPaletteAction {
    Export,
    Import,
}

struct PendingImport {
    profiled: ProfiledControllerSnapshot,
    base: Palette,
    kind: PendingTransfer,
}

#[derive(Default)]
pub(crate) struct CurrentLevelPaletteTransfer {
    choice: Option<CurrentLevelPaletteAction>,
    loader: DocumentLoader,
    persistence: PersistenceWorker,
    pending_import: Option<PendingImport>,
    rgb_expansion: Option<RgbChannelExpansion>,
    error: Option<String>,
}

impl CurrentLevelPaletteTransfer {
    pub(crate) fn start(&mut self, app: &AppState, action: CurrentLevelPaletteAction) {
        if self.loader.is_running() || self.persistence.is_running() || self.choice.is_some() {
            self.error = Some("a current-level palette transfer is already active".into());
            return;
        }
        match app.profiled_controller_snapshot() {
            Ok(profiled) if matches!(profiled.snapshot.mode, lm_app::EditorMode::Level(_)) => {
                self.choice = Some(action);
            }
            Ok(_) => self.error = Some("select a level before transferring its palette".into()),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        if let Some(completion) = self.persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
        let command = self.loader.show(context).and_then(|result| {
            let pending = self.pending_import.take().ok_or_else(|| {
                "current-level palette loader lost its revision snapshot".to_owned()
            });
            match result.and_then(|loaded| {
                pending.and_then(|pending| {
                    if pending.profiled.snapshot.revision != app.project_revision() {
                        return Err("the ROM changed while the level palette was loading".into());
                    }
                    let decoded = decode_import(Some(pending.kind), loaded)?;
                    let (palette, expansion) = imported_working_palette(pending.base, decoded)?;
                    if let Some(expansion) = expansion {
                        self.rgb_expansion = Some(expansion);
                    }
                    lm_app::prepare_current_level_palette_import(
                        &pending.profiled.snapshot,
                        &pending.profiled.profile,
                        &palette,
                    )
                    .map(lm_app::PreparedRomCommit::into_command)
                    .map_err(|error| error.to_string())
                })
            }) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        });
        self.show_choice(context, app);
        self.show_error(context, app);
        command
    }

    fn show_choice(&mut self, context: &egui::Context, app: &AppState) {
        let Some(action) = self.choice else { return };
        let catalog = app.localization();
        egui::Window::new(crate::frontend_ui::localized_text(
            catalog,
            match action {
                CurrentLevelPaletteAction::Export => UiTextKey::PaletteTransferExportTitle,
                CurrentLevelPaletteAction::Import => UiTextKey::PaletteTransferImportTitle,
            },
        ))
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label(crate::frontend_ui::localized_text(
                catalog,
                UiTextKey::PaletteTransferChooseFormat,
            ));
            ui.horizontal(|ui| {
                for (key, kind) in [
                    (UiTextKey::PaletteTransferRawFormat, PendingTransfer::Raw),
                    (UiTextKey::PaletteTransferTplFormat, PendingTransfer::Tpl),
                    (UiTextKey::PaletteTransferRgbFormat, PendingTransfer::Rgb),
                ] {
                    if ui
                        .button(crate::frontend_ui::localized_text(catalog, key))
                        .clicked()
                    {
                        self.choice = None;
                        if let Err(error) = self.begin(app, action, kind) {
                            self.error = Some(error);
                        }
                    }
                }
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonCancel,
                    ))
                    .clicked()
                {
                    self.choice = None;
                }
            });
            ui.small(crate::frontend_ui::localized_text(
                catalog,
                UiTextKey::PaletteTransferMaskNotice,
            ));
        });
    }

    fn begin(
        &mut self,
        app: &AppState,
        action: CurrentLevelPaletteAction,
        kind: PendingTransfer,
    ) -> Result<(), String> {
        let profiled = app
            .profiled_controller_snapshot()
            .map_err(|error| error.to_string())?;
        let base = lm_app::load_current_level_native_palette(&profiled.snapshot, &profiled.profile)
            .map_err(|error| error.to_string())?;
        match action {
            CurrentLevelPaletteAction::Export => {
                let bytes = match kind {
                    PendingTransfer::Raw => encode_raw_export(&base),
                    PendingTransfer::Tpl => encode_tpl_export(&supported_palette(&base)?),
                    PendingTransfer::Rgb => encode_rgb_export(
                        &supported_palette(&base)?,
                        self.rgb_expansion.unwrap_or(RgbChannelExpansion::HighBits),
                    ),
                }?;
                let Some(path) = choose_export(kind) else {
                    return Ok(());
                };
                let mask = path.with_extension("palmask");
                self.persistence.start_create_removing(
                    profiled.snapshot.revision,
                    path,
                    bytes,
                    mask,
                )
            }
            CurrentLevelPaletteAction::Import => {
                let Some(path) = choose_import(kind) else {
                    return Ok(());
                };
                let (maximum, description) = match kind {
                    PendingTransfer::Raw => (
                        lm_graphics::RawSnesPaletteFile::FILE_LEN as u64,
                        "raw 257-color palette",
                    ),
                    PendingTransfer::Tpl => (
                        lm_graphics::TplPaletteFile::FILE_LEN as u64,
                        "TPL v2 palette",
                    ),
                    PendingTransfer::Rgb => (
                        lm_graphics::RgbPaletteFile::FILE_LEN as u64,
                        "RGB24 palette",
                    ),
                };
                self.loader
                    .start(palette_import_requests(path, maximum, description))?;
                self.pending_import = Some(PendingImport {
                    profiled,
                    base,
                    kind,
                });
                Ok(())
            }
        }
    }

    fn show_error(&mut self, context: &egui::Context, app: &AppState) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(crate::frontend_ui::localized_text(
                app.localization(),
                UiTextKey::PaletteTransferErrorTitle,
            ))
            .collapsible(false)
            .show(context, |ui| {
                ui.label(error);
                if ui
                    .button(crate::frontend_ui::localized_text(
                        app.localization(),
                        UiTextKey::CommonOk,
                    ))
                    .clicked()
                {
                    self.error = None;
                }
            });
        }
    }
}

fn supported_palette(installed: &Palette) -> Result<Palette, String> {
    if installed.colors.len() != lm_graphics::RawSnesPaletteFile::COLOR_COUNT {
        return Err(format!(
            "expected 257 native colors, got {}",
            installed.colors.len()
        ));
    }
    let backdrop = installed.colors[256];
    let colors = installed.colors[..256]
        .iter()
        .copied()
        .enumerate()
        .map(|(index, color)| if index % 16 == 0 { backdrop } else { color })
        .collect();
    Ok(Palette { colors })
}

fn imported_working_palette(
    mut installed: Palette,
    decoded: crate::rom_palette_editor::transfer::DecodedImport,
) -> Result<(Palette, Option<RgbChannelExpansion>), String> {
    use crate::rom_palette_editor::transfer::DecodedImport;
    match decoded {
        DecodedImport::Raw(source, mask) => {
            lm_graphics::apply_raw_palette_import(&mut installed, &source, &mask)
                .map_err(|error| error.to_string())?;
            Ok((installed, None))
        }
        DecodedImport::Supported {
            palette,
            mask,
            rgb_expansion,
        } => {
            if installed.colors.len() != 257 || palette.colors.len() != 256 {
                return Err(format!(
                    "supported palette requires 257 working and 256 source colors, got {} and {}",
                    installed.colors.len(),
                    palette.colors.len()
                ));
            }
            let mut staged = installed.clone();
            for (index, color) in palette.colors.iter().copied().enumerate() {
                if mask.is_selected(index).unwrap_or(false) && index % 16 != 0 {
                    staged.colors[index] = color;
                }
            }
            installed = staged;
            Ok((installed, rgb_expansion))
        }
    }
}

fn choose_import(kind: PendingTransfer) -> Option<std::path::PathBuf> {
    match kind {
        PendingTransfer::Raw => dialogs::choose_raw_palette_document(),
        PendingTransfer::Tpl => dialogs::choose_tpl_palette_document(),
        PendingTransfer::Rgb => dialogs::choose_rgb_palette_document(),
    }
}

fn choose_export(kind: PendingTransfer) -> Option<std::path::PathBuf> {
    match kind {
        PendingTransfer::Raw => dialogs::choose_raw_palette_save_path(),
        PendingTransfer::Tpl => dialogs::choose_tpl_palette_save_path(),
        PendingTransfer::Rgb => dialogs::choose_rgb_palette_save_path(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, PaletteMaskFile, TplPaletteFile};

    #[test]
    fn palette_transfer_surface_has_no_literal_widget_text() {
        let source = include_str!("current_level_palette_transfer.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
            "ui.small(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "palette transfer bypasses typed localization with {literal_widget}"
            );
        }
        for key in [
            UiTextKey::PaletteTransferExportTitle,
            UiTextKey::PaletteTransferImportTitle,
            UiTextKey::PaletteTransferChooseFormat,
            UiTextKey::PaletteTransferRawFormat,
            UiTextKey::PaletteTransferTplFormat,
            UiTextKey::PaletteTransferRgbFormat,
            UiTextKey::PaletteTransferMaskNotice,
            UiTextKey::PaletteTransferErrorTitle,
        ] {
            assert!(source.contains(&format!("UiTextKey::{key:?}")));
        }
    }

    #[test]
    fn supported_export_uses_word_256_for_every_unsupported_row_zero() {
        let working = Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        };
        let supported = supported_palette(&working).unwrap();
        for index in 0..256 {
            assert_eq!(
                supported.colors[index],
                if index % 16 == 0 {
                    Bgr555(256)
                } else {
                    Bgr555(index as u16)
                }
            );
        }
    }

    #[test]
    fn supported_import_mask_changes_direct_working_index_and_preserves_row_zero() {
        let working = Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        };
        let source = Palette {
            colors: vec![Bgr555(0x1234); 256],
        };
        let mut entries = vec![0; PaletteMaskFile::FILE_LEN];
        entries[0] = 1;
        entries[1] = 1;
        entries[2] = 0;
        let mask = PaletteMaskFile::decode(&entries).unwrap();
        let (actual, expansion) = imported_working_palette(
            working,
            crate::rom_palette_editor::transfer::DecodedImport::Supported {
                palette: source,
                mask,
                rgb_expansion: None,
            },
        )
        .unwrap();
        assert_eq!(actual.colors[0], Bgr555(0));
        assert_eq!(actual.colors[1], Bgr555(0x1234));
        assert_eq!(actual.colors[2], Bgr555(2));
        assert_eq!(actual.colors[256], Bgr555(256));
        assert_eq!(expansion, None);
    }

    #[test]
    fn tpl_encoder_receives_exact_256_color_shape() {
        let working = Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        };
        let bytes = encode_tpl_export(&supported_palette(&working).unwrap()).unwrap();
        assert_eq!(bytes.len(), TplPaletteFile::FILE_LEN);
        assert_eq!(&bytes[..4], b"TPL\x02");
    }
}
