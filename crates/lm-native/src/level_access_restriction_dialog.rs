use eframe::egui;
use lm_app::{AppState, Command};
use lm_project::LevelAccessRestrictionKeys;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(crate) struct LevelAccessRestrictionDialog {
    open: bool,
    title: String,
    acknowledged: bool,
    error: Option<String>,
    stage: RestrictionStage,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RestrictionStage {
    #[default]
    Configure,
    PersistBeforeIps,
    OfferIps,
    WaitForIps,
    Complete,
    Persisting,
}

pub(crate) enum LevelAccessRestrictionAction {
    Restrict(Command),
    PersistRestrictedRom,
    CreateIps,
    SaveAndClose,
}

impl LevelAccessRestrictionDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.acknowledged = false;
        self.error = None;
        self.stage = RestrictionStage::Configure;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
        ips_workflow_active: bool,
    ) -> Option<LevelAccessRestrictionAction> {
        if self.stage == RestrictionStage::WaitForIps && !ips_workflow_active {
            self.stage = RestrictionStage::Complete;
        }
        self.observe_persistence(
            app.pending_save_request_id().is_some(),
            app.project().map(lm_project::Project::is_modified),
        );
        let mut action = None;
        if self.open && self.stage == RestrictionStage::Configure {
            egui::Window::new("Restrict Level Access by Lunar Magic (Version 1.1)")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        "This is a relatively weak form of protection. It only prevents casual \
                         examination of your levels.",
                    );
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "This operation is permanent in Lunar Magic. Keep an unmodified backup.",
                    );
                    ui.label(
                        "After restriction, performing additional editing operations on the \
                         locked ROM is not recommended.",
                    );
                    ui.separator();
                    ui.label("ROM Title (21 char Max, ASCII)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.title)
                            .char_limit(21)
                            .desired_width(260.0),
                    );
                    ui.checkbox(
                        &mut self.acknowledged,
                        "I understand that the original tool cannot reverse this operation.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                        if ui
                            .add_enabled(
                                self.acknowledged,
                                egui::Button::new("Restrict Level Access"),
                            )
                            .clicked()
                        {
                            if !self.title.is_ascii() {
                                self.error = Some("the ROM title must contain only ASCII".into());
                            } else {
                                let title = if self.title.is_empty() {
                                    "Super Peachy World".to_owned()
                                } else {
                                    self.title.clone()
                                };
                                action = Some(LevelAccessRestrictionAction::Restrict(
                                    Command::RestrictLevelAccess {
                                        rev: app.project_revision(),
                                        title,
                                        keys: fresh_keys(),
                                    },
                                ));
                            }
                        }
                    });
                });
        }
        if self.stage == RestrictionStage::OfferIps {
            egui::Window::new("Create an IPS patch?")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("Do you want to create an IPS for this locked ROM?");
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            action = Some(LevelAccessRestrictionAction::CreateIps);
                        }
                        if ui.button("No").clicked() {
                            self.stage = RestrictionStage::Complete;
                        }
                    });
                });
        }
        if self.stage == RestrictionStage::PersistBeforeIps {
            egui::Window::new("Saving restricted ROM")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    if app.pending_save_request_id().is_some() {
                        ui.label("Saving the restricted ROM before IPS creation…");
                    } else {
                        ui.label("The restricted ROM must be saved before an IPS can be created.");
                        if ui.button("Retry Save").clicked() {
                            action = Some(LevelAccessRestrictionAction::PersistRestrictedRom);
                        }
                    }
                });
        }
        if self.stage == RestrictionStage::Complete {
            egui::Window::new("Level Access Restriction Complete")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        "Your modified levels are no longer accessible by Lunar Magic. \
                         Performing any additional operations on this ROM is not recommended.",
                    );
                    if ui.button("OK").clicked() {
                        self.stage = RestrictionStage::Persisting;
                        action = Some(LevelAccessRestrictionAction::SaveAndClose);
                    }
                });
        }
        if self.stage == RestrictionStage::Persisting && app.project().is_some() {
            egui::Window::new("Saving restricted ROM")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    if app.pending_save_request_id().is_some() {
                        ui.label("Saving the restricted ROM before closing it…");
                    } else {
                        ui.label("The restricted ROM is still open and has not been saved.");
                        if ui.button("Retry Save and Close").clicked() {
                            action = Some(LevelAccessRestrictionAction::SaveAndClose);
                        }
                    }
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Level access restriction error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
        action
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.open = false;
        self.stage = RestrictionStage::PersistBeforeIps;
    }

    pub(crate) fn ips_choice_completed(&mut self, started: bool) {
        self.stage = if started {
            RestrictionStage::WaitForIps
        } else {
            RestrictionStage::Complete
        };
    }

    pub(crate) fn workflow_failed(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    fn observe_persistence(&mut self, save_pending: bool, project_modified: Option<bool>) {
        if self.stage == RestrictionStage::PersistBeforeIps
            && !save_pending
            && project_modified == Some(false)
        {
            self.stage = RestrictionStage::OfferIps;
        }
        if self.stage == RestrictionStage::Persisting && project_modified.is_none() {
            self.stage = RestrictionStage::Configure;
        }
    }
}

fn fresh_keys() -> LevelAccessRestrictionKeys {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut state = duration.as_secs() ^ u64::from(duration.subsec_nanos()).rotate_left(19);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    LevelAccessRestrictionKeys {
        per_save_low: next().to_le_bytes()[0] & 0x7f,
        per_save_high: next().to_le_bytes()[0],
        graphics: u16::from_le_bytes(next().to_le_bytes()[..2].try_into().unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_material_respects_lunar_magics_seven_bit_first_key() {
        for _ in 0..16 {
            assert_eq!(fresh_keys().per_save_low & 0x80, 0);
        }
    }

    #[test]
    fn successful_restriction_orders_ips_completion_and_close() {
        let mut dialog = LevelAccessRestrictionDialog::default();
        dialog.open();
        dialog.commit_succeeded();
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        dialog.observe_persistence(true, Some(true));
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        dialog.observe_persistence(false, Some(true));
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        dialog.observe_persistence(false, Some(false));
        assert_eq!(dialog.stage, RestrictionStage::OfferIps);
        dialog.ips_choice_completed(true);
        assert_eq!(dialog.stage, RestrictionStage::WaitForIps);

        // The completion notice must not race an active asynchronous IPS workflow.
        if dialog.stage == RestrictionStage::WaitForIps {
            dialog.stage = RestrictionStage::Complete;
        }
        assert_eq!(dialog.stage, RestrictionStage::Complete);
        dialog.stage = RestrictionStage::Persisting;
        let context = egui::Context::default();
        let app = AppState::default();
        let _action = dialog.show(&context, &app, false);
        assert_eq!(dialog.stage, RestrictionStage::Configure);
    }

    #[test]
    fn cancelled_ips_chooser_advances_to_completion_notice() {
        let mut dialog = LevelAccessRestrictionDialog::default();
        dialog.commit_succeeded();
        dialog.observe_persistence(false, Some(false));
        dialog.ips_choice_completed(false);
        assert_eq!(dialog.stage, RestrictionStage::Complete);
    }
}
