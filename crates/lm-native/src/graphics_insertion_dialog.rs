use eframe::egui;

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
    ) -> Option<Option<GraphicsInsertionRequest>> {
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new(self.family.title())
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.set_max_width(620.0);
                ui.checkbox(&mut self.expand_rom, self.family.expansion_label());
                ui.checkbox(
                    &mut self.use_4bpp,
                    "Modify the ROM with ASM to use 4bpp tiles instead of 3bpp tiles, if it doesn't already",
                );
                ui.label(self.family.reciprocal_note());
                ui.horizontal(|ui| {
                    ui.label("PC address to insert (in hex)");
                    ui.text_edit_singleline(&mut self.physical_pc_address);
                });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                ui.horizontal(|ui| {
                    accept = ui.button("OK").clicked();
                    cancel = ui.button("Cancel").clicked()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
