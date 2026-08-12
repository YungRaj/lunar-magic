use eframe::egui;
use lm_app::LocalizationCatalog;

const ORIGINAL_DIALOG_ID: u16 = 0x0410;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AnimationRate {
    Fps7_5,
    #[default]
    Fps15,
    Fps30,
    Fps60,
}

impl AnimationRate {
    pub(crate) const ALL: [Self; 4] = [Self::Fps7_5, Self::Fps15, Self::Fps30, Self::Fps60];

    pub(crate) const fn interval_seconds(self) -> f64 {
        match self {
            Self::Fps7_5 => 0.120,
            Self::Fps15 => 0.060,
            Self::Fps30 => 0.030,
            Self::Fps60 => 0.015,
        }
    }

    pub(crate) const fn interval(self) -> std::time::Duration {
        std::time::Duration::from_millis(match self {
            Self::Fps7_5 => 120,
            Self::Fps15 => 60,
            Self::Fps30 => 30,
            Self::Fps60 => 15,
        })
    }

    pub(crate) fn quantize_seconds(self, seconds: f64) -> f64 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0.0;
        }
        let interval = self.interval_seconds();
        (seconds / interval).floor() * interval
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Fps7_5 => "Low (7.5 fps)",
            Self::Fps15 => "Normal (15 fps)",
            Self::Fps30 => "Medium (30 fps)",
            Self::Fps60 => "High (60 fps)",
        }
    }

    const fn preference_value(self) -> &'static str {
        match self {
            Self::Fps7_5 => "7.5",
            Self::Fps15 => "15",
            Self::Fps30 => "30",
            Self::Fps60 => "60",
        }
    }
}

#[derive(Default)]
pub(crate) struct AnimationRateDialog {
    open: bool,
    draft: AnimationRate,
}

impl AnimationRateDialog {
    pub(crate) fn open(&mut self, current: AnimationRate) {
        self.draft = current;
        self.open = true;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<AnimationRate> {
        if !self.open {
            return None;
        }
        let mut accepted = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new(dialog_title(catalog))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(dialog_text(
                    catalog,
                    0x66,
                    "Each faster setting requires more computing power.",
                ));
                for rate in AnimationRate::ALL {
                    ui.radio_value(&mut self.draft, rate, rate_label(catalog, rate));
                }
                ui.horizontal(|ui| {
                    if ui.button(dialog_text(catalog, 1, "OK")).clicked() {
                        accepted = Some(self.draft);
                        close = true;
                    }
                    if ui.button(dialog_text(catalog, 2, "Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        accepted
    }

    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }
}

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("Change Animation Rate")
        .to_owned()
}

fn dialog_text(catalog: Option<&LocalizationCatalog>, control_id: u32, fallback: &str) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
        .to_owned()
}

fn rate_label(catalog: Option<&LocalizationCatalog>, rate: AnimationRate) -> String {
    let control_id = match rate {
        AnimationRate::Fps7_5 => 0x68,
        AnimationRate::Fps15 => 0x69,
        AnimationRate::Fps30 => 0x6a,
        AnimationRate::Fps60 => 0x67,
    };
    dialog_text(catalog, control_id, rate.label())
}

pub(crate) fn encode_preference(rate: AnimationRate) -> String {
    format!("v1:{}", rate.preference_value())
}

pub(crate) fn decode_preference(encoded: &str) -> Result<AnimationRate, String> {
    match encoded.strip_prefix("v1:") {
        Some("7.5") => Ok(AnimationRate::Fps7_5),
        Some("15") => Ok(AnimationRate::Fps15),
        Some("30") => Ok(AnimationRate::Fps30),
        Some("60") => Ok(AnimationRate::Fps60),
        Some(_) => Err("animation-rate preference is not one of 7.5, 15, 30, or 60 fps".into()),
        None => Err("unknown animation-rate preference version".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::{OriginalDialogTextKey, UiTextKey};

    fn localized_catalog() -> LocalizationCatalog {
        LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Vitesse d’animation".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x68,
                },
                "Basse (7,5 ips)".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 2,
                    control_id: 1,
                },
                "Valider".into(),
            ),
        ])
        .unwrap()
    }

    #[test]
    fn original_rates_have_exact_cadence_default_and_persistence() {
        assert_eq!(AnimationRate::default(), AnimationRate::Fps15);
        let expected = [(120, "v1:7.5"), (60, "v1:15"), (30, "v1:30"), (15, "v1:60")];
        for (rate, (milliseconds, encoded)) in AnimationRate::ALL.into_iter().zip(expected) {
            assert_eq!(
                rate.interval(),
                std::time::Duration::from_millis(milliseconds)
            );
            assert_eq!(encode_preference(rate), encoded);
            assert_eq!(decode_preference(encoded).unwrap(), rate);
            assert_eq!(rate.quantize_seconds(rate.interval_seconds() * 0.99), 0.0);
            assert_eq!(
                rate.quantize_seconds(rate.interval_seconds()),
                rate.interval_seconds()
            );
        }
        for malformed in ["15", "v2:15", "v1:7", "v1:0", "v1:15:extra"] {
            assert!(
                decode_preference(malformed).is_err(),
                "accepted {malformed}"
            );
        }
    }

    #[test]
    fn original_dialog_template_localizes_title_controls_and_rate_labels_with_fallbacks() {
        let catalog = localized_catalog();
        assert_eq!(dialog_title(Some(&catalog)), "Vitesse d’animation");
        assert_eq!(
            rate_label(Some(&catalog), AnimationRate::Fps7_5),
            "Basse (7,5 ips)"
        );
        assert_eq!(dialog_text(Some(&catalog), 1, "OK"), "Valider");
        assert_eq!(
            rate_label(Some(&catalog), AnimationRate::Fps60),
            AnimationRate::Fps60.label()
        );
        assert_eq!(dialog_title(None), "Change Animation Rate");

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(dialog_title(Some(&reopened)), "Vitesse d’animation");
        assert_eq!(dialog_text(Some(&reopened), 1, "OK"), "Valider");
    }
}
