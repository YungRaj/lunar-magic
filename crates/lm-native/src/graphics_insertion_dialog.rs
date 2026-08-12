use eframe::egui;
use lm_app::LocalizationCatalog;

const STANDARD_DEFAULT_LOGICAL_PC: usize = 0x40000;
const EXGRAPHICS_DEFAULT_LOGICAL_PC: usize = 0x100000;
const STANDARD_EXPANSION_TARGET: usize = 0x100000;
const EXGRAPHICS_EXPANSION_TARGET: usize = 0x200000;
const MAX_PC_ADDRESS: usize = 0x800000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphicsInsertionFamily {
    Standard,
    ExGraphics,
}

impl GraphicsInsertionFamily {
    const fn original_dialog_id(self) -> u16 {
        match self {
            Self::Standard => 0x03ec,
            Self::ExGraphics => 0x03fe,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Standard => "Insert GFX to ROM (in hex)",
            Self::ExGraphics => "Insert ExGFX to ROM (in hex)",
        }
    }

    const fn default_logical_pc(self) -> usize {
        match self {
            Self::Standard => STANDARD_DEFAULT_LOGICAL_PC,
            Self::ExGraphics => EXGRAPHICS_DEFAULT_LOGICAL_PC,
        }
    }

    pub(crate) const fn expansion_target(self) -> usize {
        match self {
            Self::Standard => STANDARD_EXPANSION_TARGET,
            Self::ExGraphics => EXGRAPHICS_EXPANSION_TARGET,
        }
    }

    const fn expansion_label(self) -> &'static str {
        match self {
            Self::Standard => {
                "Expand the ROM if it's currently smaller than 1 Meg (recommended if using 4bpp)"
            }
            Self::ExGraphics => {
                "Expand the ROM if it's currently smaller than 2 Megs (recommended if using 4bpp)"
            }
        }
    }

    const fn reciprocal_note(self) -> &'static str {
        match self {
            Self::Standard => {
                "If GFX are inserted as 4bpp, existing ExGFX must also be reinserted as 4bpp."
            }
            Self::ExGraphics => {
                "Regular GFX must be inserted as 4bpp before ExGFX can be inserted as 4bpp."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GraphicsInsertionRequest {
    pub(crate) family: GraphicsInsertionFamily,
    /// Logical headerless PC address corresponding to Lunar Magic's physical-file edit field.
    pub(crate) logical_pc_address: usize,
    pub(crate) expand_rom: bool,
    pub(crate) use_4bpp: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GraphicsInsertionDialog {
    family: GraphicsInsertionFamily,
    physical_pc_address: String,
    copier_prefix_len: usize,
    expand_rom: bool,
    use_4bpp: bool,
    error: Option<String>,
}

impl GraphicsInsertionDialog {
    pub(crate) fn new(
        family: GraphicsInsertionFamily,
        copier_prefix_len: usize,
        logical_rom_len: usize,
        use_4bpp: bool,
    ) -> Self {
        let physical = family
            .default_logical_pc()
            .saturating_add(copier_prefix_len);
        Self {
            family,
            physical_pc_address: format!("{physical:X}"),
            copier_prefix_len,
            expand_rom: logical_rom_len < family.expansion_target(),
            use_4bpp,
            error: None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn uses_4bpp(&self) -> bool {
        self.use_4bpp
    }

    fn request(&self) -> Result<GraphicsInsertionRequest, String> {
        let text = self
            .physical_pc_address
            .trim()
            .strip_prefix("0x")
            .or_else(|| self.physical_pc_address.trim().strip_prefix("0X"))
            .or_else(|| self.physical_pc_address.trim().strip_prefix('$'))
            .unwrap_or(self.physical_pc_address.trim());
        if text.is_empty() {
            return Err("enter a hexadecimal PC insertion address".into());
        }
        let physical = usize::from_str_radix(text, 16)
            .map_err(|_| "PC insertion address must be hexadecimal".to_owned())?;
        if physical < self.copier_prefix_len {
            return Err("PC insertion address points inside the copier prefix".into());
        }
        let logical_pc_address = physical - self.copier_prefix_len;
        if logical_pc_address >= MAX_PC_ADDRESS {
            return Err(format!(
                "PC insertion address must be below ${:X}",
                MAX_PC_ADDRESS + self.copier_prefix_len
            ));
        }
        Ok(GraphicsInsertionRequest {
            family: self.family,
            logical_pc_address,
            expand_rom: self.expand_rom,
            use_4bpp: self.use_4bpp,
        })
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Option<GraphicsInsertionRequest>> {
        let mut accept = false;
        let mut cancel = false;
        let dialog_id = self.family.original_dialog_id();
        egui::Window::new(dialog_title(catalog, self.family))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.set_max_width(620.0);
                ui.checkbox(
                    &mut self.expand_rom,
                    dialog_control_text(
                        catalog,
                        dialog_id,
                        0xdd,
                        self.family.expansion_label(),
                    ),
                );
                ui.checkbox(
                    &mut self.use_4bpp,
                    dialog_control_text(
                        catalog,
                        dialog_id,
                        0x1bb,
                        "Modify the ROM with ASM to use 4bpp tiles instead of 3bpp tiles, if it doesn't already",
                    ),
                );
                ui.label(dialog_control_text(
                    catalog,
                    dialog_id,
                    0x65,
                    self.family.reciprocal_note(),
                ));
                ui.horizontal(|ui| {
                    ui.label(dialog_control_text(
                        catalog,
                        dialog_id,
                        0xdb,
                        "PC address to insert (in hex)",
                    ));
                    ui.text_edit_singleline(&mut self.physical_pc_address);
                });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                ui.horizontal(|ui| {
                    accept = ui
                        .button(dialog_control_text(catalog, dialog_id, 1, "OK"))
                        .clicked();
                    cancel = ui
                        .button(dialog_control_text(catalog, dialog_id, 2, "Cancel"))
                        .clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape));
                });
            });
        if cancel {
            return Some(None);
        }
        if accept {
            match self.request() {
                Ok(request) => return Some(Some(request)),
                Err(error) => self.error = Some(error),
            }
        }
        None
    }
}

fn dialog_title(catalog: Option<&LocalizationCatalog>, family: GraphicsInsertionFamily) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(family.original_dialog_id()))
        .unwrap_or_else(|| family.title())
        .to_owned()
}

fn dialog_control_text(
    catalog: Option<&LocalizationCatalog>,
    dialog_id: u16,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(dialog_id, control_id))
        .unwrap_or(fallback)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::{OriginalDialogTextKey, UiTextKey};

    fn localized_graphics_dialog_catalog() -> LocalizationCatalog {
        let mut dialog_texts = Vec::new();
        for (family, title, prefix) in [
            (GraphicsInsertionFamily::Standard, "Insérer GFX", "GFX"),
            (
                GraphicsInsertionFamily::ExGraphics,
                "Insérer ExGFX",
                "ExGFX",
            ),
        ] {
            let dialog_id = family.original_dialog_id();
            dialog_texts.push((
                OriginalDialogTextKey {
                    dialog_id,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                title.into(),
            ));
            for (item_index, control_id, suffix) in [
                (0, 1, "Valider"),
                (1, 2, "Annuler"),
                (2, 0xdd, "Agrandir"),
                (3, 0x1bb, "Utiliser 4bpp"),
                (4, 0x65, "Note réciproque"),
                (5, 0xdb, "Adresse PC"),
            ] {
                dialog_texts.push((
                    OriginalDialogTextKey {
                        dialog_id,
                        item_index,
                        control_id,
                    },
                    format!("{prefix} {suffix}"),
                ));
            }
        }
        LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, format!("traduit-{key:?}"))),
        )
        .unwrap()
        .with_original_dialog_texts(dialog_texts)
        .unwrap()
    }

    #[test]
    fn original_defaults_track_physical_copier_prefix_and_expansion_thresholds() {
        let standard =
            GraphicsInsertionDialog::new(GraphicsInsertionFamily::Standard, 0x200, 0x80000, false);
        assert_eq!(standard.physical_pc_address, "40200");
        assert!(standard.expand_rom);
        assert_eq!(
            standard.request().unwrap(),
            GraphicsInsertionRequest {
                family: GraphicsInsertionFamily::Standard,
                logical_pc_address: 0x40000,
                expand_rom: true,
                use_4bpp: false,
            }
        );

        let exgraphics = GraphicsInsertionDialog::new(
            GraphicsInsertionFamily::ExGraphics,
            0x200,
            0x100000,
            true,
        );
        assert_eq!(exgraphics.physical_pc_address, "100200");
        assert!(exgraphics.expand_rom);
        assert_eq!(exgraphics.request().unwrap().logical_pc_address, 0x100000);
    }

    #[test]
    fn headerless_defaults_preserve_the_same_logical_insertion_addresses() {
        let standard =
            GraphicsInsertionDialog::new(GraphicsInsertionFamily::Standard, 0, 0x100000, true);
        assert_eq!(standard.physical_pc_address, "40000");
        assert!(!standard.expand_rom);
        assert_eq!(standard.request().unwrap().logical_pc_address, 0x40000);

        let exgraphics =
            GraphicsInsertionDialog::new(GraphicsInsertionFamily::ExGraphics, 0, 0x200000, true);
        assert_eq!(exgraphics.physical_pc_address, "100000");
        assert!(!exgraphics.expand_rom);
        assert_eq!(exgraphics.request().unwrap().logical_pc_address, 0x100000);
    }

    #[test]
    fn address_parser_accepts_original_hex_forms_and_rejects_prefix_or_rom_overflow() {
        let mut dialog =
            GraphicsInsertionDialog::new(GraphicsInsertionFamily::Standard, 0x200, 0x80000, true);
        dialog.physical_pc_address = "$40200".into();
        assert_eq!(dialog.request().unwrap().logical_pc_address, 0x40000);
        dialog.physical_pc_address = "0x40200".into();
        assert_eq!(dialog.request().unwrap().logical_pc_address, 0x40000);
        dialog.physical_pc_address = "1FF".into();
        assert!(dialog.request().unwrap_err().contains("copier prefix"));
        dialog.physical_pc_address = "800200".into();
        assert!(dialog.request().unwrap_err().contains("must be below"));
    }

    #[test]
    fn decoded_original_templates_drive_both_graphics_insertion_dialog_families() {
        let catalog = localized_graphics_dialog_catalog();
        for (family, title, prefix) in [
            (GraphicsInsertionFamily::Standard, "Insérer GFX", "GFX"),
            (
                GraphicsInsertionFamily::ExGraphics,
                "Insérer ExGFX",
                "ExGFX",
            ),
        ] {
            let dialog_id = family.original_dialog_id();
            assert_eq!(dialog_title(Some(&catalog), family), title);
            for (control_id, suffix) in [
                (1, "Valider"),
                (2, "Annuler"),
                (0xdd, "Agrandir"),
                (0x1bb, "Utiliser 4bpp"),
                (0x65, "Note réciproque"),
                (0xdb, "Adresse PC"),
            ] {
                assert_eq!(
                    dialog_control_text(Some(&catalog), dialog_id, control_id, "fallback"),
                    format!("{prefix} {suffix}")
                );
            }
        }

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(
            dialog_title(Some(&reopened), GraphicsInsertionFamily::ExGraphics),
            "Insérer ExGFX"
        );
        assert_eq!(
            dialog_control_text(Some(&reopened), 0x03fe, 0x1bb, "fallback"),
            "ExGFX Utiliser 4bpp"
        );
    }

    #[test]
    fn graphics_insertion_template_fallbacks_remain_family_specific() {
        for family in [
            GraphicsInsertionFamily::Standard,
            GraphicsInsertionFamily::ExGraphics,
        ] {
            assert_eq!(dialog_title(None, family), family.title());
            assert_eq!(
                dialog_control_text(
                    None,
                    family.original_dialog_id(),
                    0xdd,
                    family.expansion_label()
                ),
                family.expansion_label()
            );
        }
    }
}
