use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog};
use lm_project::LevelAccessRestrictionKeys;
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGINAL_DIALOG_ID: u16 = 0x03ff;

#[derive(Default)]
pub(crate) struct LevelAccessRestrictionDialog {
    open: bool,
    title: String,
    acknowledged: bool,
    error: Option<String>,
    stage: RestrictionStage,
    restore_point_pending: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RestrictionStage {
    #[default]
    Configure,
    CreateRestorePoint,
    PersistBeforeIps,
    OfferIps,
    WaitForIps,
    Complete,
    Persisting,
}

pub(crate) enum LevelAccessRestrictionAction {
    Restrict(Command),
    CreateRestorePoint,
    PersistRestrictedRom,
    CreateIps,
    SaveAndClose,
}

impl LevelAccessRestrictionDialog {
    #[cfg(test)]
    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

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
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<LevelAccessRestrictionAction> {
        let mut action = None;
        if self.stage == RestrictionStage::WaitForIps && !ips_workflow_active {
            self.stage = RestrictionStage::Complete;
        }
        if self.observe_persistence(
            app.pending_save_request_id().is_some(),
            app.project().map(lm_project::Project::is_modified),
        ) {
            action = Some(LevelAccessRestrictionAction::CreateRestorePoint);
        }
        if self.open && self.stage == RestrictionStage::Configure {
            egui::Window::new(restriction_dialog_title(catalog))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(restriction_dialog_text(
                        catalog,
                        0x68,
                        "This is a relatively weak form of protection. It only prevents casual \
                         examination of your levels.",
                    ));
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        restriction_dialog_text(
                            catalog,
                            0x67,
                            "This operation is permanent in Lunar Magic. Keep an unmodified \
                             backup.",
                        ),
                    );
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionEditingWarning,
                    ));
                    ui.separator();
                    ui.label(restriction_dialog_text(
                        catalog,
                        0x66,
                        "ROM Title (21 char Max, ASCII)",
                    ));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.title)
                            .char_limit(21)
                            .desired_width(260.0),
                    );
                    ui.checkbox(
                        &mut self.acknowledged,
                        text(catalog, ExtendedUiTextKey::LevelRestrictionAcknowledge),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .button(restriction_dialog_text(catalog, 2, "Cancel"))
                            .clicked()
                        {
                            self.open = false;
                        }
                        if ui
                            .add_enabled(
                                self.acknowledged,
                                egui::Button::new(restriction_dialog_text(catalog, 1, "OK")),
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
        if self.stage == RestrictionStage::CreateRestorePoint {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::LevelRestrictionRestoreTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(text(
                    catalog,
                    ExtendedUiTextKey::LevelRestrictionRestoreNotice,
                ));
                if ui
                    .button(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionRetryRestore,
                    ))
                    .clicked()
                {
                    action = Some(LevelAccessRestrictionAction::CreateRestorePoint);
                }
            });
        }
        if self.stage == RestrictionStage::OfferIps {
            egui::Window::new(text(catalog, ExtendedUiTextKey::LevelRestrictionIpsTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionIpsQuestion,
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::LevelRestrictionYes))
                            .clicked()
                        {
                            action = Some(LevelAccessRestrictionAction::CreateIps);
                        }
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::LevelRestrictionNo))
                            .clicked()
                        {
                            self.stage = RestrictionStage::Complete;
                        }
                    });
                });
        }
        if self.stage == RestrictionStage::PersistBeforeIps {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::LevelRestrictionSavingTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                if app.pending_save_request_id().is_some() {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionSavingForIps,
                    ));
                } else {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionSaveRequired,
                    ));
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::LevelRestrictionRetrySave))
                        .clicked()
                    {
                        action = Some(LevelAccessRestrictionAction::PersistRestrictedRom);
                    }
                }
            });
        }
        if self.stage == RestrictionStage::Complete {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::LevelRestrictionCompleteTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(text(
                    catalog,
                    ExtendedUiTextKey::LevelRestrictionCompleteNotice,
                ));
                if ui
                    .button(text(catalog, ExtendedUiTextKey::LevelRestrictionOk))
                    .clicked()
                {
                    self.stage = RestrictionStage::Persisting;
                    action = Some(LevelAccessRestrictionAction::SaveAndClose);
                }
            });
        }
        if self.stage == RestrictionStage::Persisting && app.project().is_some() {
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::LevelRestrictionSavingTitle,
            ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                if app.pending_save_request_id().is_some() {
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::LevelRestrictionSavingForClose,
                    ));
                } else {
                    ui.label(text(catalog, ExtendedUiTextKey::LevelRestrictionStillOpen));
                    if ui
                        .button(text(
                            catalog,
                            ExtendedUiTextKey::LevelRestrictionRetrySaveClose,
                        ))
                        .clicked()
                    {
                        action = Some(LevelAccessRestrictionAction::SaveAndClose);
                    }
                }
            });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::LevelRestrictionErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::LevelRestrictionOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
        action
    }

    pub(crate) fn commit_succeeded(&mut self, create_restore_point: bool) {
        self.open = false;
        self.restore_point_pending = create_restore_point;
        self.stage = RestrictionStage::PersistBeforeIps;
    }

    pub(crate) fn restore_point_completed(&mut self) {
        self.restore_point_pending = false;
        self.stage = RestrictionStage::OfferIps;
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

    fn observe_persistence(&mut self, save_pending: bool, project_modified: Option<bool>) -> bool {
        if self.stage == RestrictionStage::PersistBeforeIps
            && !save_pending
            && project_modified == Some(false)
        {
            self.stage = if self.restore_point_pending {
                RestrictionStage::CreateRestorePoint
            } else {
                RestrictionStage::OfferIps
            };
            return self.restore_point_pending;
        }
        if self.stage == RestrictionStage::Persisting && project_modified.is_none() {
            self.stage = RestrictionStage::Configure;
        }
        false
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn restriction_dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("Restrict Level Access by Lunar Magic (Version 1.1)")
        .to_owned()
}

fn restriction_dialog_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
        .to_owned()
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
    use lm_app::{OriginalDialogTextKey, UiTextKey};

    #[test]
    fn complete_restriction_surface_uses_every_typed_extension_key() {
        let source = include_str!("level_access_restriction_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LevelRestriction"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for bypass in [
            "egui::Window::new(\"Create Full Restore Point\")",
            "egui::Window::new(\"Create an IPS patch?\")",
            "egui::Window::new(\"Level Access Restriction Complete\")",
            "egui::Window::new(\"Level access restriction error\")",
        ] {
            assert!(!source.contains(bypass));
        }
    }

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
        dialog.commit_succeeded(false);
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        assert!(!dialog.observe_persistence(true, Some(true)));
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        assert!(!dialog.observe_persistence(false, Some(true)));
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        assert!(!dialog.observe_persistence(false, Some(false)));
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
        let _action = dialog.show(&context, &app, false, None);
        assert_eq!(dialog.stage, RestrictionStage::Configure);
    }

    #[test]
    fn cancelled_ips_chooser_advances_to_completion_notice() {
        let mut dialog = LevelAccessRestrictionDialog::default();
        dialog.commit_succeeded(false);
        assert!(!dialog.observe_persistence(false, Some(false)));
        dialog.ips_choice_completed(false);
        assert_eq!(dialog.stage, RestrictionStage::Complete);
    }

    #[test]
    fn enabled_restore_policy_blocks_persistence_until_checkpoint_succeeds() {
        let mut dialog = LevelAccessRestrictionDialog::default();
        dialog.commit_succeeded(true);
        assert_eq!(dialog.stage, RestrictionStage::PersistBeforeIps);
        assert!(dialog.observe_persistence(false, Some(false)));
        assert_eq!(dialog.stage, RestrictionStage::CreateRestorePoint);
        dialog.restore_point_completed();
        assert_eq!(dialog.stage, RestrictionStage::OfferIps);
    }

    #[test]
    fn original_restriction_template_localizes_every_matching_native_caption() {
        let catalog = LocalizationCatalog::new(
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
                "Restreindre l’accès aux niveaux".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x66,
                },
                "Titre ROM (21 caractères ASCII)".into(),
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
        .unwrap();

        assert_eq!(
            restriction_dialog_title(Some(&catalog)),
            "Restreindre l’accès aux niveaux"
        );
        assert_eq!(
            restriction_dialog_text(Some(&catalog), 0x66, "fallback"),
            "Titre ROM (21 caractères ASCII)"
        );
        assert_eq!(restriction_dialog_text(Some(&catalog), 1, "OK"), "Valider");
        assert_eq!(
            restriction_dialog_text(Some(&catalog), 2, "Cancel"),
            "Cancel"
        );
        assert_eq!(
            restriction_dialog_title(None),
            "Restrict Level Access by Lunar Magic (Version 1.1)"
        );

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(
            restriction_dialog_title(Some(&reopened)),
            "Restreindre l’accès aux niveaux"
        );
    }
}
