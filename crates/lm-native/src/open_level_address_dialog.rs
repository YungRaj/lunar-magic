use eframe::egui;
use lm_app::{LocalizationCatalog, UiTextKey};

const ORIGINAL_DIALOG_ID: u16 = 1001;

#[derive(Default)]
pub(crate) struct OpenLevelAddressDialog {
    open: bool,
    address: String,
    error: Option<String>,
}

impl OpenLevelAddressDialog {
    pub(crate) fn open(&mut self) {
        self.address.clear();
        self.error = None;
        self.open = true;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut selected = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new(dialog_text(
            catalog,
            u32::MAX,
            "Open Level From Address (in hex)",
        ))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(context, |ui| {
            ui.label(dialog_text(
                catalog,
                0x80,
                "PC address to open level (in hex)",
            ));
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.address)
                    .desired_width(116.0)
                    .char_limit(8),
            );
            let submit = ui.button(dialog_text(catalog, 1, "OK")).clicked()
                || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            if submit {
                match parse_pc_address(&self.address) {
                    Ok(address) => {
                        selected = Some(address);
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

fn parse_pc_address(value: &str) -> Result<usize, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("enter a hexadecimal PC address".into());
    }
    usize::from_str_radix(value, 16).map_err(|_| "PC address is not valid hexadecimal".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::OriginalDialogTextKey;

    #[test]
    fn parser_accepts_unprefixed_hex_and_rejects_noncanonical_text() {
        for (text, expected) in [("0", 0), ("30263", 0x30263), ("abcdef", 0xabcdef)] {
            assert_eq!(parse_pc_address(text).unwrap(), expected);
        }
        for invalid in ["", " ", "0x30263", "-1", "GG", "30263 trailing"] {
            assert!(parse_pc_address(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn opening_clears_the_previous_draft_and_error() {
        let mut dialog = OpenLevelAddressDialog {
            address: "BAD".into(),
            error: Some("old".into()),
            ..OpenLevelAddressDialog::default()
        };
        dialog.open();
        assert!(dialog.is_open());
        assert!(dialog.address.is_empty());
        assert!(dialog.error.is_none());
    }

    #[test]
    fn original_resource_1001_localizes_the_title_label_and_buttons() {
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
                "Ouvrir depuis une adresse".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 3,
                    control_id: 0x80,
                },
                "Adresse PC".into(),
            ),
        ])
        .unwrap();
        assert_eq!(
            dialog_text(Some(&catalog), u32::MAX, "fallback"),
            "Ouvrir depuis une adresse"
        );
        assert_eq!(dialog_text(Some(&catalog), 0x80, "fallback"), "Adresse PC");
        assert_eq!(dialog_text(Some(&catalog), 1, "OK"), "OK");
        assert_eq!(dialog_text(Some(&catalog), 2, "Cancel"), "Cancel");
    }
}
