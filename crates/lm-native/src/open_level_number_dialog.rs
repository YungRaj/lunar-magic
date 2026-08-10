use eframe::egui;
use lm_app::{LocalizationCatalog, UiTextKey};

const ORIGINAL_DIALOG_ID: u16 = 1000;

#[derive(Default)]
pub(crate) struct OpenLevelNumberDialog {
    open: bool,
    level: String,
    error: Option<String>,
}

impl OpenLevelNumberDialog {
    pub(crate) fn open(&mut self, current: Option<u16>) {
        self.level = format!("{:03X}", current.unwrap_or(0));
        self.error = None;
        self.open = true;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<u16> {
        if !self.open {
            return None;
        }
        let mut selected = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new(dialog_text(catalog, u32::MAX, "Open Level Number (in hex)"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(dialog_text(catalog, 0x66, "Level Number (0-1FF)"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.level)
                        .desired_width(72.0)
                        .char_limit(3),
                );
                let submit = ui.button(dialog_text(catalog, 1, "OK")).clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if submit {
                    match parse_level_number(&self.level) {
                        Ok(level) => {
                            selected = Some(level);
                            close = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                if ui
                    .button(dialog_text(catalog, 2, UiTextKey::CommonCancel.english()))
                    .clicked()
                {
                    close = true;
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
            });
        self.open = open && !close;
        selected
    }

    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn draft(&self) -> &str {
        &self.level
    }
}

fn dialog_text(catalog: Option<&LocalizationCatalog>, control_id: u32, fallback: &str) -> String {
    if control_id == u32::MAX {
        catalog
            .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
            .unwrap_or(fallback)
            .to_owned()
    } else {
        catalog
            .and_then(|catalog| {
                catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id)
            })
            .unwrap_or(fallback)
            .to_owned()
    }
}

fn parse_level_number(value: &str) -> Result<u16, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("enter a hexadecimal level number from 0 through 1FF".into());
    }
    let level = u16::from_str_radix(value, 16)
        .map_err(|_| "level number is not valid hexadecimal".to_owned())?;
    if level > 0x1ff {
        return Err("level number must be from 0 through 1FF".into());
    }
    Ok(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::OriginalDialogTextKey;

    #[test]
    fn parser_accepts_every_original_boundary_and_rejects_noncanonical_inputs() {
        for (text, expected) in [("0", 0), ("000", 0), ("105", 0x105), ("1ff", 0x1ff)] {
            assert_eq!(parse_level_number(text).unwrap(), expected);
        }
        for invalid in ["", " ", "0x105", "200", "-1", "GG", "105 trailing"] {
            assert!(parse_level_number(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn opening_uses_the_current_level_and_clears_stale_errors() {
        let mut dialog = OpenLevelNumberDialog {
            error: Some("old".into()),
            ..OpenLevelNumberDialog::default()
        };
        dialog.open(Some(0x1ab));
        assert!(dialog.is_open());
        assert_eq!(dialog.draft(), "1AB");
        assert!(dialog.error.is_none());
    }

    #[test]
    fn original_resource_1000_localizes_the_title_label_and_buttons() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Ouvrir le niveau".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 3,
                    control_id: 0x66,
                },
                "Numéro du niveau".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 0,
                    control_id: 1,
                },
                "Valider".into(),
            ),
        ])
        .unwrap();
        assert_eq!(
            dialog_text(Some(&catalog), u32::MAX, "fallback"),
            "Ouvrir le niveau"
        );
        assert_eq!(
            dialog_text(Some(&catalog), 0x66, "fallback"),
            "Numéro du niveau"
        );
        assert_eq!(dialog_text(Some(&catalog), 1, "fallback"), "Valider");
        assert_eq!(dialog_text(Some(&catalog), 2, "Cancel"), "Cancel");
    }
}
