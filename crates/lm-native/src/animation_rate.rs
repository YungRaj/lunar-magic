use eframe::egui;

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

    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<AnimationRate> {
        if !self.open {
            return None;
        }
        let mut accepted = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new("Change Animation Rate")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Animation rate");
                for rate in AnimationRate::ALL {
                    ui.radio_value(&mut self.draft, rate, rate.label());
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        accepted = Some(self.draft);
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
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
}
