use crate::{
    about_dialog::{AboutDialog, DiagnosticsDialog},
    appearance_editor::AppearanceEditor,
    built_in_runtime_installer::BuiltInRuntimeInstaller,
    configuration_loader::{
        ConfigurationLoader, InstalledLocalization, InstalledOriginalLocalization,
        LoadedConfiguration,
    },
    copier_header_dialog::CopierHeaderDialog,
    custom_object_editor::CustomObjectEditor,
    custom_sprite_editor::CustomSpriteEditor,
    current_level_palette_transfer::CurrentLevelPaletteTransfer,
    dsc_sidecar_editor::DscSidecarEditor,
    editor_view,
    effects::Confirmation,
    effects::EffectState,
    exanimation_editor::ExAnimationEditor,
    expanded_settings_editor::ExpandedSettingsEditor,
    graphics_editor::GraphicsEditor,
    graphics_migration_dialog::GraphicsMigrationDialog,
    help_dialog::HelpDialog,
    ips_create_dialog::IpsCreateDialog,
    ips_patch_dialog::IpsPatchDialog,
    layer3_editor::Layer3Editor,
    level_access_restriction_dialog::LevelAccessRestrictionDialog,
    level_deletion_dialog::LevelDeletionDialog,
    level_editor::LevelEditor,
    level_usage_dialog::LevelUsageDialog,
    map16_editor::Map16Editor,
    map16_set_editor::Map16SetEditor,
    metadata_editor::MetadataEditor,
    mwl_editor::MwlEditor,
    native_level_assets_editor::NativeLevelAssetsEditor,
    native_level_document_editor::NativeLevelDocumentEditor,
    native_map16_sidecar_editor::NativeMap16SidecarEditor,
    native_render::NativeRenderState,
    osc_sidecar_editor::OscSidecarEditor,
    overworld_appearance_editor::OverworldAppearanceEditor,
    overworld_editor::OverworldEditor,
    palette_editor::PaletteEditor,
    path_editor::PathEditor,
    rats_reclamation_dialog::RatsReclamationDialog,
    recovery_store::RecoveryStore,
    restore_point_dialog::RestorePointDialog,
    revision_patch_installer::RevisionPatchInstaller,
    rom_boss_sequence_editor::RomBossSequenceEditor,
    rom_event_editors::{RomOverworldEventRevealEditor, RomOverworldEventTilemapEditor},
    rom_exanimation_editor::RomExAnimationEditor,
    rom_expanded_settings_editor::RomExpandedSettingsEditor,
    rom_expansion_dialog::RomExpansionDialog,
    rom_graphics_editor::RomGraphicsEditor,
    rom_level_assets_editor::RomLevelAssetsEditor,
    rom_lunar_magic_metadata_editor::RomLunarMagicMetadataEditor,
    rom_map16_editor::RomMap16Editor,
    rom_mwl_batch_export_dialog::RomMwlBatchExportDialog,
    rom_mwl_batch_import_dialog::RomMwlBatchImportDialog,
    rom_navigation_link_editors::{RomOverworldPathLinkEditor, RomOverworldWarpLinkEditor},
    rom_overworld_editor::RomOverworldEditor,
    rom_overworld_event_number_editor::RomOverworldEventNumberEditor,
    rom_overworld_level_name_editor::RomOverworldLevelNameEditor,
    rom_overworld_message_editor::RomOverworldMessageEditor,
    rom_overworld_player_start_editor::RomOverworldPlayerStartEditor,
    rom_overworld_settings_editor::RomOverworldSettingsEditor,
    rom_overworld_special_event_editor::RomOverworldSpecialEventEditor,
    rom_palette_editor::RomPaletteEditor,
    rom_secondary_exit_editor::RomSecondaryExitEditor,
    rom_shared_palette_editor::RomSharedPaletteEditor,
    rom_tilemap_editor::{RomCreditsTilemapEditor, RomTitleTilemapEditor},
    rom_title_recording_editor::RomTitleRecordingEditor,
    shortcut_editor::ShortcutEditor,
    ssc_sidecar_editor::SscSidecarEditor,
    toolbar_editor::{ToolbarEditor, ToolbarEditorResult},
    toolbar_graphics_transfer::ToolbarGraphicsTransfer,
    user_toolbar_images::{MainToolbarImageSet, UserToolbarImageSet},
    vanilla_graphics_editor::VanillaGraphicsEditor,
    vanilla_level_editor::VanillaLevelEditor,
};
use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, ExternalTool, LocalizationCatalog, ShortcutConfig, ToolConfig,
    ToolbarConfig, UiTextKey, UserToolbar,
};

mod document_menus;
mod menus;
mod rom_menus;
mod rom_windows;
mod shutdown;
mod toolbar;
mod undo_history_settings;
mod windows;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LevelScreenOverlay {
    None,
    ScreenExits,
    ScreenGrid,
    BoundaryGuide,
}

impl Default for LevelScreenOverlay {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LevelViewVisibility {
    pub layer1: bool,
    pub layer2: bool,
    pub layer3: bool,
    pub sprites: bool,
    pub tile_grid: bool,
    pub surface_outline: bool,
    pub line_guide_outline: bool,
    pub screen_overlay: LevelScreenOverlay,
}

impl Default for LevelViewVisibility {
    fn default() -> Self {
        Self {
            layer1: true,
            layer2: true,
            layer3: true,
            sprites: true,
            tile_grid: false,
            surface_outline: false,
            line_guide_outline: false,
            screen_overlay: LevelScreenOverlay::None,
        }
    }
}

#[derive(Default)]
pub(crate) struct NativeApplication {
    app: AppState,
    effects: EffectState,
    about_dialog: AboutDialog,
    diagnostics_dialog: DiagnosticsDialog,
    help_dialog: HelpDialog,
    shortcut_editor: ShortcutEditor,
    toolbar_editor: ToolbarEditor,
    toolbar_graphics_transfer: ToolbarGraphicsTransfer,
    undo_history_settings: undo_history_settings::UndoHistorySettings,
    external_tool_config_editor: crate::external_tool_config_editor::ExternalToolConfigEditor,
    user_toolbar: Option<UserToolbar>,
    user_toolbar_images: UserToolbarImageSet,
    main_toolbar_images: MainToolbarImageSet,
    user_toolbar_observed_document: Option<std::path::PathBuf>,
    user_toolbar_observed_level: Option<u16>,
    user_toolbar_pending_save_notifications: u8,
    user_toolbar_pending_deleted_levels: Vec<u16>,
    level_text: String,
    special_world_passed: bool,
    joined_graphics_files: bool,
    level_view_visibility: LevelViewVisibility,
    renderer: NativeRenderState,
    vanilla_graphics_editor: VanillaGraphicsEditor,
    vanilla_level_editor: VanillaLevelEditor,
    palette_editor: PaletteEditor,
    graphics_editor: GraphicsEditor,
    map16_editor: Map16Editor,
    exanimation_editor: ExAnimationEditor,
    level_editor: LevelEditor,
    level_access_restriction_dialog: LevelAccessRestrictionDialog,
    level_deletion_dialog: LevelDeletionDialog,
    level_usage_dialog: LevelUsageDialog,
    live_emulator: crate::live_emulator::LiveEmulator,
    overworld_editor: OverworldEditor,
    path_editor: PathEditor,
    metadata_editor: MetadataEditor,
    appearance_editor: AppearanceEditor,
    built_in_runtime_installer: BuiltInRuntimeInstaller,
    overworld_appearance_editor: OverworldAppearanceEditor,
    layer3_editor: Layer3Editor,
    mwl_editor: MwlEditor,
    expanded_settings_editor: ExpandedSettingsEditor,
    custom_object_editor: CustomObjectEditor,
    custom_sprite_editor: CustomSpriteEditor,
    current_level_palette_transfer: CurrentLevelPaletteTransfer,
    native_map16_sidecar_editor: NativeMap16SidecarEditor,
    dsc_sidecar_editor: DscSidecarEditor,
    ssc_sidecar_editor: SscSidecarEditor,
    osc_sidecar_editor: OscSidecarEditor,
    map16_set_editor: Map16SetEditor,
    native_level_document_editor: NativeLevelDocumentEditor,
    native_level_assets_editor: NativeLevelAssetsEditor,
    rom_expanded_settings_editor: RomExpandedSettingsEditor,
    rom_overworld_event_reveal_editor: RomOverworldEventRevealEditor,
    rom_overworld_event_tilemap_editor: RomOverworldEventTilemapEditor,
    rom_boss_sequence_editor: RomBossSequenceEditor,
    rom_exanimation_editor: RomExAnimationEditor,
    rom_graphics_editor: RomGraphicsEditor,
    rom_level_assets_editor: RomLevelAssetsEditor,
    rom_lunar_magic_metadata_editor: RomLunarMagicMetadataEditor,
    rom_mwl_batch_export_dialog: RomMwlBatchExportDialog,
    rom_mwl_batch_import_dialog: RomMwlBatchImportDialog,
    rom_map16_editor: RomMap16Editor,
    rom_overworld_path_link_editor: RomOverworldPathLinkEditor,
    rom_overworld_warp_link_editor: RomOverworldWarpLinkEditor,
    rom_overworld_editor: RomOverworldEditor,
    rom_overworld_event_number_editor: RomOverworldEventNumberEditor,
    rom_overworld_level_name_editor: RomOverworldLevelNameEditor,
    rom_overworld_message_editor: RomOverworldMessageEditor,
    rom_overworld_player_start_editor: RomOverworldPlayerStartEditor,
    rom_overworld_settings_editor: RomOverworldSettingsEditor,
    rom_overworld_special_event_editor: RomOverworldSpecialEventEditor,
    rom_palette_editor: RomPaletteEditor,
    rom_secondary_exit_editor: RomSecondaryExitEditor,
    rom_shared_palette_editor: RomSharedPaletteEditor,
    rom_title_recording_editor: RomTitleRecordingEditor,
    rom_title_tilemap_editor: RomTitleTilemapEditor,
    rom_credits_tilemap_editor: RomCreditsTilemapEditor,
    rom_expansion_dialog: RomExpansionDialog,
    graphics_migration_dialog: GraphicsMigrationDialog,
    ips_create_dialog: IpsCreateDialog,
    ips_patch_dialog: IpsPatchDialog,
    copier_header_dialog: CopierHeaderDialog,
    revision_patch_installer: RevisionPatchInstaller,
    rats_reclamation_dialog: RatsReclamationDialog,
    restore_point_dialog: RestorePointDialog,
    recovery_store: RecoveryStore,
    recent_state: Option<lm_app::recent_state_file::RecentStateFile>,
    configuration_loader: ConfigurationLoader,
    installed_localizations: Vec<InstalledLocalization>,
    installed_original_localizations: Vec<InstalledOriginalLocalization>,
    auto_detect_localization: bool,
    profile_loader: crate::profile_loader::ProfileLoader,
    #[cfg(feature = "visual-smoke")]
    visual_smoke_frames: u8,
    #[cfg(feature = "visual-smoke")]
    visual_smoke_requested: bool,
}

impl NativeApplication {
    const RESTORE_POLICY_STORAGE_KEY: &'static str = "lunar_magic_rust.restore_policy.v1";
    const SHORTCUT_STORAGE_KEY: &'static str = "lunar_magic_rust.shortcuts.v1";
    const TOOLBAR_STORAGE_KEY: &'static str = "lunar_magic_rust.toolbar.v1";
    const LOCALIZATION_STORAGE_KEY: &'static str = "lunar_magic_rust.localization.v1";
    const UNDO_HISTORY_STORAGE_KEY: &'static str = "lunar_magic_rust.undo_history.v1";
    const JOINED_GRAPHICS_STORAGE_KEY: &'static str = "lunar_magic_rust.joined_graphics.v1";
    const EXTERNAL_TOOLS_STORAGE_KEY: &'static str = "lunar_magic_rust.external_tools.v1";

    pub(crate) fn from_startup(
        initialized: Result<crate::startup::InitializedNative, String>,
    ) -> Self {
        let mut application = match initialized {
            Ok(initialized) => Self {
                app: initialized.app,
                recent_state: initialized.recent_state,
                ..Self::default()
            },
            Err(error) => Self {
                effects: EffectState {
                    error: Some(error),
                    ..EffectState::default()
                },
                ..Self::default()
            },
        };
        application.load_main_toolbar_images();
        application.load_user_toolbar();
        application.load_installed_localizations();
        application
    }

    fn load_installed_localizations(&mut self) {
        let result = std::env::current_exe()
            .map_err(|error| format!("cannot locate application executable: {error}"))
            .and_then(|path| {
                let directory = path
                    .parent()
                    .ok_or_else(|| "application executable has no parent directory".to_owned())?;
                let catalogs = ConfigurationLoader::discover_installed_localizations(directory)?;
                let original_modules =
                    ConfigurationLoader::discover_installed_original_localizations(directory)?;
                Ok((catalogs, original_modules))
            });
        match result {
            Ok((catalogs, original_modules)) => {
                self.installed_localizations = catalogs;
                self.installed_original_localizations = original_modules;
            }
            Err(error) => self.effects.error = Some(error),
        }
    }

    fn load_main_toolbar_images(&mut self) {
        let result = std::env::current_exe()
            .map_err(|error| format!("cannot locate application executable: {error}"))
            .and_then(|path| {
                let directory = path
                    .parent()
                    .ok_or_else(|| "application executable has no parent directory".to_owned())?;
                MainToolbarImageSet::load(directory)
            });
        match result {
            Ok(images) => self.main_toolbar_images = images,
            Err(error) => self.effects.error = Some(error),
        }
    }

    fn load_user_toolbar(&mut self) {
        let result = std::env::current_exe()
            .map_err(|error| format!("cannot locate application executable: {error}"))
            .and_then(|path| {
                let path = path
                    .parent()
                    .ok_or_else(|| "application executable has no parent directory".to_owned())?
                    .join("usertoolbar.txt");
                match std::fs::read_to_string(&path) {
                    Ok(text) => match UserToolbar::parse(&text) {
                        Ok(toolbar) => Ok(Some((path, toolbar))),
                        Err(error) => Err(format!("cannot parse {}: {error}", path.display())),
                    },
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(format!("cannot read {}: {error}", path.display())),
                }
            });
        match result {
            Ok(Some((path, toolbar))) => {
                let directory = path.parent().expect("joined toolbar path has a parent");
                match UserToolbarImageSet::load(directory, &toolbar) {
                    Ok(images) => self.user_toolbar_images = images,
                    Err(error) => self.effects.error = Some(error),
                }
                self.user_toolbar = Some(toolbar);
            }
            Ok(None) => self.user_toolbar = None,
            Err(error) => self.effects.error = Some(error),
        }
    }

    pub(crate) fn enable_crash_recovery(&mut self) {
        self.recovery_store.enable();
        if let Some(error) = self.recovery_store.error.take() {
            self.effects.error = Some(error);
        }
    }

    fn show_crash_recovery(&mut self, context: &egui::Context) {
        let Some(snapshot) = self.recovery_store.pending_snapshot() else {
            return;
        };
        let revision = snapshot.revision;
        let level = snapshot.level;
        let pending_count = self.recovery_store.pending_count();
        let project_open = self.app.controller_snapshot().is_ok();
        let recovery_available = self.localized(
            UiTextKey::RecoveryAvailable,
            UiTextKey::RecoveryAvailable.english(),
        );
        let recovery_requires_save_as = self.localized(
            UiTextKey::RecoveryRequiresSaveAs,
            UiTextKey::RecoveryRequiresSaveAs.english(),
        );
        let recovery_close_current = self.localized(
            UiTextKey::RecoveryCloseCurrent,
            UiTextKey::RecoveryCloseCurrent.english(),
        );
        let recovery_action = self.localized(
            UiTextKey::RecoveryAction,
            UiTextKey::RecoveryAction.english(),
        );
        let recovery_discard = self.localized(
            UiTextKey::RecoveryDiscard,
            UiTextKey::RecoveryDiscard.english(),
        );
        egui::Window::new(self.localized(
            UiTextKey::RecoveryWindowTitle,
            UiTextKey::RecoveryWindowTitle.english(),
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.label(&recovery_available);
            if pending_count > 1 {
                ui.label(
                    self.localized(
                        UiTextKey::RecoveryCountFormat,
                        UiTextKey::RecoveryCountFormat.english(),
                    )
                    .replace("{count}", &pending_count.to_string()),
                );
            }
            ui.label(
                self.localized(
                    UiTextKey::RecoveryRevisionFormat,
                    UiTextKey::RecoveryRevisionFormat.english(),
                )
                .replace("{revision}", &revision.to_string()),
            );
            if let Some(level) = level {
                ui.label(
                    self.localized(
                        UiTextKey::RecoveryLevelFormat,
                        UiTextKey::RecoveryLevelFormat.english(),
                    )
                    .replace("{level}", &format!("{level:03X}")),
                );
            }
            ui.label(&recovery_requires_save_as);
            if project_open {
                ui.label(&recovery_close_current);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!project_open, egui::Button::new(&recovery_action))
                    .clicked()
                {
                    let snapshot = self
                        .recovery_store
                        .pending_snapshot()
                        .cloned()
                        .expect("the recovery prompt owns a pending record");
                    match self.app.load_recovery(snapshot) {
                        Ok(()) => {
                            self.recovery_store.complete_pending_recovery();
                            self.renderer.invalidate();
                        }
                        Err(error) => self.effects.error = Some(error.to_string()),
                    }
                }
                if ui.button(&recovery_discard).clicked() {
                    self.recovery_store.discard_pending();
                }
            });
        });
    }

    pub(crate) fn load_persistent_preferences(&mut self, storage: Option<&dyn eframe::Storage>) {
        let Some(storage) = storage else {
            self.import_original_external_tools_if_unconfigured();
            self.auto_detect_localization = true;
            if let Err(error) = self.start_auto_detected_localization() {
                self.effects.error = Some(error);
            }
            return;
        };
        if let Some(encoded) = storage.get_string(Self::RESTORE_POLICY_STORAGE_KEY)
            && let Err(error) = self
                .restore_point_dialog
                .load_automatic_preferences(&encoded)
        {
            self.effects.error = Some(format!("cannot load restore-point preferences: {error}"));
        }
        if let Some(encoded) = storage.get_string(Self::SHORTCUT_STORAGE_KEY) {
            match decode_shortcut_preference(&encoded).and_then(|config| {
                self.app
                    .set_shortcuts(config)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {}
                Err(error) => {
                    self.effects.error = Some(format!("cannot load keyboard shortcuts: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::TOOLBAR_STORAGE_KEY) {
            let result = if encoded == "default" {
                self.app.clear_toolbar();
                Ok(())
            } else {
                encoded
                    .strip_prefix("hex:")
                    .ok_or_else(|| "unknown toolbar preference version".to_owned())
                    .and_then(decode_toolbar_preference)
                    .and_then(|toolbar| {
                        self.app
                            .set_toolbar(toolbar)
                            .map_err(|error| error.to_string())
                    })
            };
            if let Err(error) = result {
                self.effects.error = Some(format!("cannot load toolbar layout: {error}"));
            }
        }
        if let Some(encoded) = storage.get_string(Self::LOCALIZATION_STORAGE_KEY) {
            let result = if encoded == "auto-detect" {
                self.auto_detect_localization = true;
                self.start_auto_detected_localization()
            } else if encoded == "builtin-en" {
                self.auto_detect_localization = false;
                self.app.clear_localization();
                Ok(())
            } else {
                self.auto_detect_localization = false;
                encoded
                    .strip_prefix("hex:")
                    .ok_or_else(|| "unknown localization preference version".to_owned())
                    .and_then(decode_localization_preference)
                    .and_then(|catalog| {
                        self.app
                            .set_localization(catalog)
                            .map_err(|error| error.to_string())
                    })
            };
            if let Err(error) = result {
                self.effects.error = Some(format!("cannot load language catalog: {error}"));
            }
        } else {
            self.auto_detect_localization = true;
            if let Err(error) = self.start_auto_detected_localization() {
                self.effects.error = Some(error);
            }
        }
        if let Some(encoded) = storage.get_string(Self::UNDO_HISTORY_STORAGE_KEY) {
            let result = undo_history_settings::decode_preference(&encoded).and_then(|limit| {
                self.app
                    .set_undo_snapshot_limit(limit)
                    .map_err(|error| error.to_string())
            });
            if let Err(error) = result {
                self.effects.error = Some(format!("cannot load undo-history preference: {error}"));
            }
        }
        if let Some(encoded) = storage.get_string(Self::JOINED_GRAPHICS_STORAGE_KEY) {
            match decode_joined_graphics_preference(&encoded) {
                Ok(joined) => self.joined_graphics_files = joined,
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load joined-GFX preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::EXTERNAL_TOOLS_STORAGE_KEY) {
            match decode_external_tools_preference(&encoded).and_then(|config| {
                self.app
                    .set_external_tools(config.tools)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {}
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load external-tool preferences: {error}"));
                }
            }
        } else {
            self.import_original_external_tools_if_unconfigured();
        }
    }

    fn import_original_external_tools_if_unconfigured(&mut self) {
        if !self.app.external_tools().is_empty() {
            return;
        }
        match load_original_external_tool_settings()
            .and_then(|settings| settings.map(original_external_tools).transpose())
        {
            Ok(Some(tools)) if !tools.is_empty() => {
                if let Err(error) = self.app.set_external_tools(tools) {
                    self.effects.error = Some(format!(
                        "cannot migrate Lunar Magic external-tool settings: {error}"
                    ));
                }
            }
            Ok(_) => {}
            Err(error) => {
                self.effects.error = Some(format!(
                    "cannot read Lunar Magic external-tool settings: {error}"
                ));
            }
        }
    }

    fn dispatch(&mut self, context: &egui::Context, command: Command) {
        let _accepted = self.try_dispatch(context, command);
    }

    /// Dispatches one command and reports whether application state accepted it.
    ///
    /// ROM editor windows use this acknowledgement before discarding their staged controller.
    fn try_dispatch(&mut self, context: &egui::Context, command: Command) -> bool {
        match self.app.dispatch(command) {
            Ok(effects) => {
                self.effects.handle(&mut self.app, context, effects);
                true
            }
            Err(error) => {
                self.effects.error = Some(error.to_string());
                false
            }
        }
    }

    fn show_confirmation(&mut self, context: &egui::Context) {
        let Some(confirmation) = self.effects.confirmation else {
            return;
        };
        let question = self.localized(
            UiTextKey::UnsavedChangesQuestion,
            UiTextKey::UnsavedChangesQuestion.english(),
        );
        let cancel = self.localized(UiTextKey::CommonCancel, UiTextKey::CommonCancel.english());
        let discard = self.localized(
            UiTextKey::UnsavedDiscard,
            UiTextKey::UnsavedDiscard.english(),
        );
        let save = self.localized(UiTextKey::FileSave, UiTextKey::FileSave.english());
        egui::Window::new(self.localized(
            UiTextKey::UnsavedChangesTitle,
            UiTextKey::UnsavedChangesTitle.english(),
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.label(&question);
            ui.horizontal(|ui| {
                if ui.button(&save).clicked() {
                    self.effects.confirmation = None;
                    self.effects.save_before_confirmation_action(
                        &mut self.app,
                        context,
                        confirmation,
                    );
                }
                if ui.button(&cancel).clicked() {
                    self.effects.confirmation = None;
                    if matches!(confirmation, Confirmation::DiscardAndOpen) {
                        self.effects.cancel_requested_rom_path();
                    }
                }
                if ui.button(&discard).clicked() {
                    self.effects.confirmation = None;
                    let effects = match confirmation {
                        Confirmation::DiscardAndOpen => self.app.discard_and_request_open(),
                        Confirmation::DiscardAndClose { quit_after } => {
                            Ok(self.app.discard_and_close(quit_after))
                        }
                    };
                    match effects {
                        Ok(effects) => self.effects.handle(&mut self.app, context, effects),
                        Err(error) => self.effects.error = Some(error.to_string()),
                    }
                }
            });
        });
    }

    fn synchronize_level_text(&mut self) {
        if let EditorMode::Level(level) = self.app.mode
            && !self
                .level_text
                .eq_ignore_ascii_case(&format!("{level:03X}"))
        {
            self.level_text = format!("{level:03X}");
        }
    }

    fn persist_recent_state(&mut self) {
        let Some(state) = self.recent_state.as_mut() else {
            return;
        };
        if let Err(error) = state.persist_if_changed(&self.app) {
            self.effects.error = Some(error.to_string());
        }
    }

    fn localized(&self, key: UiTextKey, fallback: &str) -> String {
        self.app.localization().map_or_else(
            || fallback.to_owned(),
            |catalog| catalog.text(key).to_owned(),
        )
    }

    fn synchronize_localized_chrome(&self, context: &egui::Context) {
        context.send_viewport_cmd(egui::ViewportCommand::Title(
            self.localized(UiTextKey::AppTitle, "Lunar Magic Rust"),
        ));
    }

    fn prepare_frame(&mut self, context: &egui::Context) {
        self.show_configuration_loader(context);
        self.show_profile_loader(context);
        self.handle_user_toolbar_document_change(context);
        self.handle_user_toolbar_level_change();
        if !self.shortcut_editor.is_open() && !self.toolbar_editor.is_open() {
            self.handle_shortcuts(context);
        }
        self.synchronize_localized_chrome(context);
        self.synchronize_level_text();
    }

    fn show_profile_loader(&mut self, context: &egui::Context) {
        let Some(result) = self.profile_loader.show(context) else {
            return;
        };
        match result {
            Ok(profile) => {
                if self.try_dispatch(context, Command::InstallRevisionProfile(Box::new(profile))) {
                    self.renderer.invalidate();
                }
            }
            Err(error) => self.effects.error = Some(error),
        }
    }

    fn show_configuration_loader(&mut self, context: &egui::Context) {
        let Some(result) = self.configuration_loader.show(context) else {
            return;
        };
        let result = result.and_then(|configuration| match configuration {
            LoadedConfiguration::Frontend(config) => {
                self.app
                    .set_frontend_config(config)
                    .map_err(|error| error.to_string())?;
                self.app.status = "Installed frontend configuration".into();
                Ok(())
            }
            LoadedConfiguration::ExternalTools(config) => {
                self.app
                    .set_external_tools(config.tools)
                    .map_err(|error| error.to_string())?;
                self.app.status = "Installed external-tool configuration".into();
                Ok(())
            }
            LoadedConfiguration::Localization(catalog) => {
                let locale = catalog.locale().to_owned();
                self.app
                    .set_localization(catalog)
                    .map_err(|error| error.to_string())?;
                self.app.status = format!("Installed {locale} language catalog");
                Ok(())
            }
        });
        if let Err(error) = result {
            self.effects.error = Some(error);
        }
    }

    fn start_auto_detected_localization(&mut self) -> Result<(), String> {
        self.app.clear_localization();
        let Some(selection) = select_preferred_installed_localization_with_original(
            &self.installed_localizations,
            &self.installed_original_localizations,
            system_locale_preferences(),
        ) else {
            return Ok(());
        };
        match selection {
            PreferredInstalledLocalization::CatalogPath(path) => {
                self.configuration_loader.start_localization_path(path)
            }
            PreferredInstalledLocalization::Original(catalog) => self
                .app
                .set_localization(catalog)
                .map_err(|error| error.to_string()),
        }
    }

    fn show_global_effects(&mut self, context: &egui::Context) {
        self.effects.show_rom_loader(context, &mut self.app);
        self.effects.show_persistence(context, &mut self.app);
        if std::mem::take(&mut self.effects.completed_rom_save) {
            self.publish_user_toolbar_save_notifications();
            self.publish_user_toolbar_level_deleted_notifications();
        } else if matches!(
            self.app.capabilities().project,
            lm_app::ProjectStatus::Closed | lm_app::ProjectStatus::OpenClean
        ) {
            self.user_toolbar_pending_save_notifications = 0;
            self.user_toolbar_pending_deleted_levels.clear();
        }
        self.effects.show_external_tools(context, &mut self.app);
        let live_context = match self.app.mode {
            EditorMode::Level(level) if self.app.project().is_some() => {
                Some((level, self.app.project_revision()))
            }
            _ => None,
        };
        if self.live_emulator.retain_for_open_project(live_context) {
            self.live_emulator
                .set_editor_animation_playing(self.vanilla_level_editor.animation_playing());
            if let (Some(source), Some(target)) =
                (self.live_emulator.source_context(), live_context)
            {
                let synchronization = if source.1 != target.1 {
                    self.app
                        .controller_snapshot()
                        .map_err(|error| error.to_string())
                        .and_then(|snapshot| {
                            self.live_emulator.reload_snapshot(
                                snapshot.revision,
                                target.0,
                                snapshot.rom_bytes,
                            )
                        })
                } else if source.0 != target.0 {
                    self.live_emulator.switch_level(target.0, target.1)
                } else {
                    Ok(())
                };
                if let Err(error) = synchronization {
                    self.effects.error = Some(error);
                }
            }
        }
        let localization = self.app.localization().cloned();
        if let Some(status) = self.live_emulator.show(context, |key| {
            localization
                .as_ref()
                .map_or_else(|| key.english().into(), |catalog| catalog.text(key).into())
        }) {
            self.app.status = status;
        }
        if let Some(error) = self.effects.error.clone() {
            let ok = self.localized(UiTextKey::CommonOk, UiTextKey::CommonOk.english());
            egui::Window::new(self.localized(
                UiTextKey::ErrorWindowTitle,
                UiTextKey::ErrorWindowTitle.english(),
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(error);
                if ui.button(&ok).clicked() {
                    self.effects.error = None;
                }
            });
        }
        if self.effects.quit_requested {
            self.stop_user_toolbar_tools_on_close();
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for NativeApplication {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            Self::RESTORE_POLICY_STORAGE_KEY,
            self.restore_point_dialog.automatic_preferences(),
        );
        if let Some(shortcuts) = self.app.shortcuts() {
            storage.set_string(
                Self::SHORTCUT_STORAGE_KEY,
                encode_shortcut_preference(shortcuts),
            );
        }
        let toolbar = self.app.toolbar().map_or_else(
            || "default".to_owned(),
            |toolbar| format!("hex:{}", encode_toolbar_preference(toolbar)),
        );
        storage.set_string(Self::TOOLBAR_STORAGE_KEY, toolbar);
        let localization = encode_localization_storage_preference(
            self.auto_detect_localization,
            self.app.localization(),
        );
        storage.set_string(Self::LOCALIZATION_STORAGE_KEY, localization);
        storage.set_string(
            Self::UNDO_HISTORY_STORAGE_KEY,
            undo_history_settings::encode_preference(self.app.undo_snapshot_limit()),
        );
        storage.set_string(
            Self::JOINED_GRAPHICS_STORAGE_KEY,
            encode_joined_graphics_preference(self.joined_graphics_files),
        );
        match encode_external_tools_preference(self.app.external_tools()) {
            Ok(encoded) => storage.set_string(Self::EXTERNAL_TOOLS_STORAGE_KEY, encoded),
            Err(error) => {
                self.effects.error =
                    Some(format!("cannot save external-tool preferences: {error}"));
            }
        }
    }

    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if context.input(|input| input.viewport().close_requested()) && !self.effects.quit_requested
        {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_quit(context);
        }
        self.prepare_frame(context);
        self.show_crash_recovery(context);
        egui::TopBottomPanel::top("menu").show(context, |ui| self.menu_bar(context, ui));
        egui::TopBottomPanel::top("toolbar").show(context, |ui| self.toolbar(context, ui));
        egui::TopBottomPanel::bottom("status").show(context, |ui| {
            ui.label(if self.app.status.is_empty() {
                self.app
                    .localization()
                    .map_or("Ready", |catalog| catalog.text(UiTextKey::StatusReady))
            } else {
                &self.app.status
            });
        });
        egui::CentralPanel::default().show(context, |ui| {
            let vanilla_level = VanillaLevelEditor::handles(&self.app);
            let vanilla_graphics = VanillaGraphicsEditor::handles(&self.app);
            let live_frame = self.live_emulator.canvas_frame();
            if vanilla_level
                && let Some(command) = self.vanilla_level_editor.show(
                    ui,
                    &self.app,
                    self.special_world_passed,
                    self.level_view_visibility,
                    self.ssc_sidecar_editor.resolved(),
                    self.ssc_sidecar_editor.external_assets(),
                    self.ssc_sidecar_editor.asset_revision(),
                    self.osc_sidecar_editor.resolved(),
                    self.native_map16_sidecar_editor.value(),
                    live_frame,
                    &self.main_toolbar_images,
                )
            {
                let sprite_only_commit = matches!(
                    command,
                    Command::CommitRomWrites { .. } | Command::CommitRomMutation { .. }
                ) && self.vanilla_level_editor.has_sprite_only_changes();
                let sprite_payload =
                    sprite_only_commit.then(|| self.vanilla_level_editor.lmsw_sprite_payload());
                let level_commit = matches!(
                    command,
                    Command::CommitRomWrites { .. } | Command::CommitRomMutation { .. }
                );
                if self.try_dispatch(context, command) {
                    if level_commit {
                        self.mark_user_toolbar_save_notification(
                            lm_app::LunarMagicNotificationKind::SaveLevel,
                        );
                    }
                    if let Some(payload) = sprite_payload {
                        let reload = payload.and_then(|sprites| {
                            let snapshot = self
                                .app
                                .controller_snapshot()
                                .map_err(|error| error.to_string())?;
                            let EditorMode::Level(level) = snapshot.mode else {
                                return Err("sprite commit left level editing mode".into());
                            };
                            if self
                                .live_emulator
                                .source_context()
                                .is_none_or(|(source_level, _)| source_level != level)
                            {
                                return Ok(());
                            }
                            self.live_emulator.reload_sprite_snapshot(
                                snapshot.revision,
                                level,
                                snapshot.rom_bytes,
                                sprites,
                            )
                        });
                        if let Err(error) = reload {
                            self.effects.error = Some(error);
                        }
                    }
                }
            } else if vanilla_graphics
                && let Some(command) = self.vanilla_graphics_editor.show(
                    ui,
                    &self.app,
                    self.special_world_passed,
                    &mut self.joined_graphics_files,
                )
            {
                self.dispatch(context, command);
            } else if !vanilla_level
                && !vanilla_graphics
                && !self.renderer.show(context, ui, &self.app)
            {
                editor_view::show(ui, self.app.mode);
            }
        });
        self.show_confirmation(context);
        if let Some((level, command)) = self.level_deletion_dialog.show(context, &self.app)
            && self.try_dispatch(context, command)
        {
            self.renderer.invalidate();
            self.vanilla_level_editor.invalidate_graphics_preview();
            self.mark_user_toolbar_level_deleted(level);
            self.dispatch(context, Command::Save);
        }
        self.about_dialog.show(context, self.app.localization());
        self.diagnostics_dialog
            .show(context, self.app.localization());
        self.help_dialog.show(context, self.app.localization());
        if let Some(shortcuts) = self.shortcut_editor.show(context) {
            match self.app.set_shortcuts(shortcuts) {
                Ok(()) => self.app.status = "Updated keyboard shortcuts".into(),
                Err(error) => self.effects.error = Some(error.to_string()),
            }
        }
        if let Some(result) = self.toolbar_editor.show(context) {
            match result {
                ToolbarEditorResult::Apply(toolbar) => match self.app.set_toolbar(toolbar) {
                    Ok(()) => self.app.status = "Updated toolbar layout".into(),
                    Err(error) => self.effects.error = Some(error.to_string()),
                },
                ToolbarEditorResult::UseDefault => {
                    self.app.clear_toolbar();
                    self.app.status = "Restored built-in toolbar".into();
                }
            }
        }
        if let Some(limit) = self
            .undo_history_settings
            .show(context, self.app.localization())
        {
            match self.app.set_undo_snapshot_limit(limit) {
                Ok(()) => {
                    self.app.status = format!(
                        "Undo history retains {limit} snapshots ({} undo operations)",
                        limit.saturating_sub(1)
                    );
                }
                Err(error) => self.effects.error = Some(error.to_string()),
            }
        }
        self.show_editor_windows(context);
        self.show_global_effects(context);
        let recovery_revision = matches!(
            self.app.capabilities().project,
            lm_app::ProjectStatus::OpenModified
        )
        .then(|| self.app.project_revision());
        self.recovery_store
            .synchronize_project(recovery_revision, || self.app.recovery_snapshot());
        if let Some(error) = self.recovery_store.error.take() {
            self.effects.error = Some(error);
        }
        #[cfg(feature = "visual-smoke")]
        self.capture_visual_smoke(context);
    }
}

fn encode_shortcut_preference(config: &ShortcutConfig) -> String {
    let bytes = config
        .encode()
        .expect("active shortcut configuration is already validated");
    encode_hex(&bytes)
}

fn decode_shortcut_preference(value: &str) -> Result<ShortcutConfig, String> {
    ShortcutConfig::decode(&decode_hex(value, ShortcutConfig::MAX_ENCODED_LEN)?)
        .map_err(|error| error.to_string())
}

fn encode_joined_graphics_preference(joined: bool) -> String {
    if joined { "joined" } else { "separate" }.to_owned()
}

fn decode_joined_graphics_preference(value: &str) -> Result<bool, String> {
    match value {
        "joined" => Ok(true),
        "separate" => Ok(false),
        _ => Err("unknown joined-GFX preference version".into()),
    }
}

fn encode_toolbar_preference(config: &ToolbarConfig) -> String {
    let bytes = config
        .encode()
        .expect("active toolbar configuration is already validated");
    encode_hex(&bytes)
}

fn decode_toolbar_preference(value: &str) -> Result<ToolbarConfig, String> {
    ToolbarConfig::decode(&decode_hex(value, ToolbarConfig::MAX_ENCODED_LEN)?)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn system_locale_preferences() -> Vec<String> {
    let preferences = lm_windows::preferred_ui_languages();
    if preferences.is_empty() {
        environment_locale_preferences()
    } else {
        preferences
    }
}

#[cfg(not(windows))]
fn system_locale_preferences() -> Vec<String> {
    environment_locale_preferences()
}

const MAX_PREFERRED_UI_LANGUAGES: usize = 64;

fn environment_locale_preferences() -> Vec<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

fn normalize_locale(value: &str) -> Option<String> {
    let value = value
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .trim()
        .replace('_', "-");
    (!value.is_empty() && !value.eq_ignore_ascii_case("C") && !value.eq_ignore_ascii_case("POSIX"))
        .then(|| value.to_ascii_lowercase())
}

fn select_preferred_installed_localization(
    installed: &[InstalledLocalization],
    preferences: impl IntoIterator<Item = String>,
) -> Option<std::path::PathBuf> {
    match select_preferred_installed_localization_with_original(installed, &[], preferences)? {
        PreferredInstalledLocalization::CatalogPath(path) => Some(path),
        PreferredInstalledLocalization::Original(_) => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PreferredInstalledLocalization {
    CatalogPath(std::path::PathBuf),
    Original(LocalizationCatalog),
}

fn select_preferred_installed_localization_with_original(
    installed: &[InstalledLocalization],
    originals: &[InstalledOriginalLocalization],
    preferences: impl IntoIterator<Item = String>,
) -> Option<PreferredInstalledLocalization> {
    let preferences = preferences
        .into_iter()
        .filter_map(|locale| normalize_locale(&locale))
        .take(MAX_PREFERRED_UI_LANGUAGES)
        .collect::<Vec<_>>();
    for preference in &preferences {
        if let Some(catalog) = installed
            .iter()
            .find(|catalog| normalize_locale(&catalog.locale).as_ref() == Some(preference))
        {
            return Some(PreferredInstalledLocalization::CatalogPath(
                catalog.path.clone(),
            ));
        }
        if let Some(module) = originals
            .iter()
            .find(|module| normalize_locale(&module.metadata.locale).as_ref() == Some(preference))
        {
            return Some(PreferredInstalledLocalization::Original(
                module.catalog.clone(),
            ));
        }
    }
    for preference in preferences {
        let language = preference.split('-').next().unwrap_or_default();
        if let Some(catalog) = installed.iter().find(|catalog| {
            normalize_locale(&catalog.locale)
                .is_some_and(|locale| locale.split('-').next() == Some(language))
        }) {
            return Some(PreferredInstalledLocalization::CatalogPath(
                catalog.path.clone(),
            ));
        }
        if let Some(module) = originals.iter().find(|module| {
            normalize_locale(&module.metadata.locale)
                .is_some_and(|locale| locale.split('-').next() == Some(language))
        }) {
            return Some(PreferredInstalledLocalization::Original(
                module.catalog.clone(),
            ));
        }
    }
    None
}

fn encode_localization_preference(catalog: &LocalizationCatalog) -> String {
    let bytes = catalog
        .encode()
        .expect("active localization catalog is already validated");
    encode_hex(&bytes)
}

fn encode_localization_storage_preference(
    auto_detect: bool,
    catalog: Option<&LocalizationCatalog>,
) -> String {
    if auto_detect {
        "auto-detect".to_owned()
    } else {
        catalog.map_or_else(
            || "builtin-en".to_owned(),
            |catalog| format!("hex:{}", encode_localization_preference(catalog)),
        )
    }
}

fn decode_localization_preference(value: &str) -> Result<LocalizationCatalog, String> {
    LocalizationCatalog::decode(&decode_hex(value, LocalizationCatalog::MAX_ENCODED_LEN)?)
        .map_err(|error| error.to_string())
}

fn encode_external_tools_preference(tools: &[ExternalTool]) -> Result<String, String> {
    let bytes = ToolConfig {
        tools: tools.to_vec(),
    }
    .encode()
    .map_err(|error| error.to_string())?;
    Ok(format!("hex:{}", encode_hex(&bytes)))
}

fn decode_external_tools_preference(value: &str) -> Result<ToolConfig, String> {
    let value = value
        .strip_prefix("hex:")
        .ok_or_else(|| "unknown external-tool preference version".to_owned())?;
    ToolConfig::decode(&decode_hex(value, ToolConfig::MAX_ENCODED_LEN)?)
        .map_err(|error| error.to_string())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OriginalExternalToolSettings {
    emulator: Option<String>,
    emulator_arguments: Option<String>,
    gba_emulator: Option<String>,
    gba_emulator_arguments: Option<String>,
    tile_editor: Option<String>,
    tile_editor_arguments: Option<String>,
    options: u32,
    options2: u32,
}

#[cfg(windows)]
fn load_original_external_tool_settings() -> Result<Option<OriginalExternalToolSettings>, String> {
    lm_windows::lunar_magic_external_tool_registry()
        .map(|settings| {
            settings.map(|settings| OriginalExternalToolSettings {
                emulator: settings.emulator,
                emulator_arguments: settings.emulator_arguments,
                gba_emulator: settings.gba_emulator,
                gba_emulator_arguments: settings.gba_emulator_arguments,
                tile_editor: settings.tile_editor,
                tile_editor_arguments: settings.tile_editor_arguments,
                options: settings.options,
                options2: settings.options2,
            })
        })
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn load_original_external_tool_settings() -> Result<Option<OriginalExternalToolSettings>, String> {
    Ok(None)
}

fn original_external_tools(
    settings: OriginalExternalToolSettings,
) -> Result<Vec<ExternalTool>, String> {
    const EMULATOR_CUSTOM_ARGUMENTS: u32 = 1 << 29;
    const EMULATOR_SHORT_PATH: u32 = 1 << 16;
    const GBA_EMULATOR_SHORT_PATH: u32 = 1 << 17;
    const GBA_EMULATOR_CUSTOM_ARGUMENTS: u32 = 1 << 18;
    const TILE_EDITOR_CUSTOM_ARGUMENTS: u32 = 1 << 24;

    let mut tools = Vec::new();
    if let Some(executable) = nonempty_original_path(settings.emulator) {
        let placeholder = if settings.options2 & EMULATOR_SHORT_PATH != 0 {
            "rom_8dot3"
        } else {
            "rom"
        };
        tools.push(original_profile_tool(
            "lunar-magic-snes-emulator",
            "SNES Emulator",
            executable,
            settings.emulator_arguments,
            settings.options & EMULATOR_CUSTOM_ARGUMENTS != 0,
            placeholder,
        )?);
    }
    if let Some(executable) = nonempty_original_path(settings.gba_emulator) {
        let placeholder = if settings.options2 & GBA_EMULATOR_SHORT_PATH != 0 {
            "rom_8dot3"
        } else {
            "rom"
        };
        tools.push(original_profile_tool(
            "lunar-magic-gba-emulator",
            "GBA Emulator",
            executable,
            settings.gba_emulator_arguments,
            settings.options2 & GBA_EMULATOR_CUSTOM_ARGUMENTS != 0,
            placeholder,
        )?);
    }
    if let Some(executable) = nonempty_original_path(settings.tile_editor) {
        tools.push(original_profile_tool(
            "lunar-magic-tile-editor",
            "Tile Editor",
            executable,
            settings.tile_editor_arguments,
            settings.options2 & TILE_EDITOR_CUSTOM_ARGUMENTS != 0,
            "graphics",
        )?);
    }
    Ok(tools)
}

fn nonempty_original_path(path: Option<String>) -> Option<std::path::PathBuf> {
    path.filter(|path| !path.trim().is_empty())
        .map(std::path::PathBuf::from)
}

fn original_profile_tool(
    id: &str,
    name: &str,
    executable: std::path::PathBuf,
    arguments: Option<String>,
    custom_arguments: bool,
    placeholder: &str,
) -> Result<ExternalTool, String> {
    let arguments = if custom_arguments {
        let command_line = arguments.as_deref().unwrap_or(r#""%1""#);
        parse_windows_argument_tail(command_line)
            .into_iter()
            .map(|argument| translate_original_placeholder(&argument, placeholder))
            .collect()
    } else {
        vec![format!("{{{placeholder}}}")]
    };
    let tool = ExternalTool {
        id: id.into(),
        name: name.into(),
        executable,
        arguments,
        working_directory: None,
        subscriptions: Vec::new(),
    };
    ToolConfig {
        tools: vec![tool.clone()],
    }
    .encode()
    .map_err(|error| error.to_string())?;
    Ok(tool)
}

/// Splits an original registry argument tail with the quote/backslash rules used by the Microsoft
/// C runtime. The registry wrapper bounds the input, so this parser cannot allocate without bound.
fn parse_windows_argument_tail(value: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut arguments = Vec::new();
    let mut offset = 0;
    while offset < chars.len() {
        while offset < chars.len() && chars[offset].is_whitespace() {
            offset += 1;
        }
        if offset == chars.len() {
            break;
        }
        let mut argument = String::new();
        let mut quoted = false;
        loop {
            let mut backslashes = 0;
            while offset < chars.len() && chars[offset] == '\\' {
                backslashes += 1;
                offset += 1;
            }
            if offset < chars.len() && chars[offset] == '"' {
                argument.extend(std::iter::repeat_n('\\', backslashes / 2));
                if backslashes % 2 == 0 {
                    if quoted && chars.get(offset + 1) == Some(&'"') {
                        argument.push('"');
                        offset += 1;
                    } else {
                        quoted = !quoted;
                    }
                } else {
                    argument.push('"');
                }
                offset += 1;
            } else {
                argument.extend(std::iter::repeat_n('\\', backslashes));
                if offset == chars.len() || (!quoted && chars[offset].is_whitespace()) {
                    break;
                }
                argument.push(chars[offset]);
                offset += 1;
            }
        }
        arguments.push(argument);
    }
    arguments
}

fn translate_original_placeholder(value: &str, placeholder: &str) -> String {
    value
        .split("%1")
        .map(|literal| literal.replace('{', "{{").replace('}', "}}"))
        .collect::<Vec<_>>()
        .join(&format!("{{{placeholder}}}"))
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn decode_hex(value: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("encoded preference has an odd length".into());
    }
    if value.len() / 2 > max_bytes {
        return Err("encoded preference exceeds its bounded format limit".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .map_err(|_| "encoded preference is not UTF-8".to_owned())
                .and_then(|pair| {
                    u8::from_str_radix(pair, 16)
                        .map_err(|_| "encoded preference is not hexadecimal".to_owned())
                })
        })
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod preference_tests {
    use super::*;
    use eframe::Storage as _;
    use lm_app::{ShortcutBinding, ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolbarAction};
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }

        fn flush(&mut self) {}
    }

    fn configured_tool() -> ExternalTool {
        ExternalTool {
            id: "emu".into(),
            name: "Unicode Emulator 日本語".into(),
            executable: "/tools/my emulator".into(),
            arguments: vec!["--rom={rom}".into(), "two words".into()],
            working_directory: Some("{project_dir}".into()),
            subscriptions: vec![lm_app::ToolEvent::ProjectSaved],
        }
    }

    #[test]
    fn external_tool_preference_round_trips_canonical_configuration() {
        let tools = vec![configured_tool()];
        let encoded = encode_external_tools_preference(&tools).unwrap();
        assert!(encoded.starts_with("hex:"));
        assert_eq!(
            decode_external_tools_preference(&encoded).unwrap().tools,
            tools
        );
        assert!(decode_external_tools_preference("LMTOOLS1").is_err());
        assert!(decode_external_tools_preference("hex:0").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_external_tools_without_registry_fallback() {
        let tools = vec![configured_tool()];
        let mut source = NativeApplication::default();
        source.app.set_external_tools(tools.clone()).unwrap();
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut source, &mut storage);

        let mut reopened = NativeApplication::default();
        reopened.load_persistent_preferences(Some(&storage));
        assert_eq!(reopened.app.external_tools(), tools);
    }

    #[test]
    fn malformed_native_tool_preference_is_authoritative_and_failure_atomic() {
        let original = vec![configured_tool()];
        let mut application = NativeApplication::default();
        application
            .app
            .set_external_tools(original.clone())
            .unwrap();
        let mut storage = MemoryStorage::default();
        storage.set_string(
            NativeApplication::EXTERNAL_TOOLS_STORAGE_KEY,
            "hex:00".into(),
        );
        application.load_persistent_preferences(Some(&storage));
        assert_eq!(application.app.external_tools(), original);
        assert!(
            application
                .effects
                .error
                .as_deref()
                .is_some_and(|error| error.contains("external-tool preferences"))
        );
    }

    #[test]
    fn original_registry_profiles_map_every_recovered_option_and_placeholder() {
        let settings = OriginalExternalToolSettings {
            emulator: Some(r"C:\Emulators\snes9x.exe".into()),
            emulator_arguments: Some(r#"--fullscreen --rom="%1" "literal {brace}" """#.into()),
            gba_emulator: Some(r"C:\Emulators\mgba.exe".into()),
            gba_emulator_arguments: Some(r#"--ignored "%1""#.into()),
            tile_editor: Some(r"C:\Tools\yy-chr.exe".into()),
            tile_editor_arguments: Some(r#"--palette=keep "%1""#.into()),
            options: 1 << 29,
            options2: (1 << 16) | (1 << 17) | (1 << 24),
        };
        let tools = original_external_tools(settings).unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].id, "lunar-magic-snes-emulator");
        assert_eq!(
            tools[0].arguments,
            ["--fullscreen", "--rom={rom_8dot3}", "literal {{brace}}", ""]
        );
        assert_eq!(
            tools[1].arguments,
            ["{rom_8dot3}"],
            "disabled GBA custom arguments ignore the stored tail"
        );
        assert_eq!(tools[2].id, "lunar-magic-tile-editor");
        assert_eq!(tools[2].arguments, ["--palette=keep", "{graphics}"]);
        ToolConfig { tools }.encode().unwrap();
    }

    #[test]
    fn original_registry_migration_skips_empty_paths_and_defaults_missing_custom_tail() {
        let tools = original_external_tools(OriginalExternalToolSettings {
            emulator: Some("  ".into()),
            tile_editor: Some("tile.exe".into()),
            options2: 1 << 24,
            ..OriginalExternalToolSettings::default()
        })
        .unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].arguments, ["{graphics}"]);
    }

    #[test]
    fn windows_argument_tail_matches_quote_and_backslash_boundaries() {
        assert_eq!(
            parse_windows_argument_tail(r#"plain "two words" "" a\\\"b tail\\"#),
            ["plain", "two words", "", "a\\\"b", "tail\\\\"]
        );
        assert_eq!(parse_windows_argument_tail(r#""a""b" """"#), ["a\"b", "\""]);
    }

    #[test]
    fn joined_graphics_preference_round_trips_both_original_modes() {
        assert_eq!(encode_joined_graphics_preference(false), "separate");
        assert_eq!(encode_joined_graphics_preference(true), "joined");
        assert!(!decode_joined_graphics_preference("separate").unwrap());
        assert!(decode_joined_graphics_preference("joined").unwrap());
        assert!(decode_joined_graphics_preference("true").is_err());
    }

    #[test]
    fn shortcut_preference_round_trips_canonical_configuration() {
        let config = ShortcutConfig {
            bindings: vec![ShortcutBinding {
                gesture: ShortcutGesture {
                    modifiers: ShortcutModifiers::PRIMARY,
                    key: ShortcutKey::Character('s'),
                },
                action: ToolbarAction::Save,
            }],
        };
        let encoded = encode_shortcut_preference(&config);
        assert_eq!(decode_shortcut_preference(&encoded).unwrap(), config);
    }

    #[test]
    fn shortcut_preference_rejects_malformed_text_and_payloads() {
        assert!(decode_shortcut_preference("0").is_err());
        assert!(decode_shortcut_preference("zz").is_err());
        assert!(decode_shortcut_preference("00").is_err());
    }

    #[test]
    fn toolbar_preference_round_trips_canonical_configuration() {
        let config = ToolbarConfig {
            items: vec![lm_app::ToolbarItem::Action {
                id: "save".into(),
                action: ToolbarAction::Save,
                label: UiTextKey::FileSave,
            }],
        };
        let encoded = encode_toolbar_preference(&config);
        assert_eq!(decode_toolbar_preference(&encoded).unwrap(), config);
    }

    #[test]
    fn toolbar_preference_rejects_malformed_payloads() {
        assert!(decode_toolbar_preference("0").is_err());
        assert!(decode_toolbar_preference("zz").is_err());
        assert!(decode_toolbar_preference("00").is_err());
    }

    #[test]
    fn localization_preference_round_trips_unicode_catalog() {
        let catalog = LocalizationCatalog::new(
            "ja-JP",
            UiTextKey::ALL.map(|key| (key, format!("日本語-{key:?}"))),
        )
        .unwrap();
        let encoded = encode_localization_preference(&catalog);
        assert_eq!(decode_localization_preference(&encoded).unwrap(), catalog);
    }

    #[test]
    fn localization_preference_rejects_malformed_payloads() {
        assert!(decode_localization_preference("0").is_err());
        assert!(decode_localization_preference("zz").is_err());
        assert!(decode_localization_preference("00").is_err());
        assert!(
            decode_localization_preference(&"00".repeat(LocalizationCatalog::MAX_ENCODED_LEN + 1))
                .is_err()
        );
    }

    #[test]
    fn localization_storage_keeps_auto_detect_distinct_from_builtin_and_explicit() {
        let catalog = LocalizationCatalog::new(
            "ja-JP",
            UiTextKey::ALL.map(|key| (key, format!("日本語-{key:?}"))),
        )
        .unwrap();
        assert_eq!(
            encode_localization_storage_preference(true, Some(&catalog)),
            "auto-detect"
        );
        assert_eq!(
            encode_localization_storage_preference(false, None),
            "builtin-en"
        );
        let explicit = encode_localization_storage_preference(false, Some(&catalog));
        let encoded = explicit.strip_prefix("hex:").unwrap();
        assert_eq!(decode_localization_preference(encoded).unwrap(), catalog);
    }

    #[test]
    fn installed_language_autodetection_prefers_exact_then_primary_language() {
        let installed = [
            InstalledLocalization {
                locale: "de-DE".into(),
                path: "de.lmlang".into(),
            },
            InstalledLocalization {
                locale: "fr-CA".into(),
                path: "fr.lmlang".into(),
            },
            InstalledLocalization {
                locale: "zh-Hant".into(),
                path: "zh.lmlang".into(),
            },
        ];
        assert_eq!(
            select_preferred_installed_localization(
                &installed,
                ["fr_FR.UTF-8".to_owned(), "de-DE".to_owned()]
            ),
            Some("de.lmlang".into())
        );
        assert_eq!(
            select_preferred_installed_localization(&installed, ["fr_FR.UTF-8".to_owned()]),
            Some("fr.lmlang".into())
        );
        assert_eq!(
            select_preferred_installed_localization(
                &installed,
                ["C".to_owned(), "POSIX".to_owned()]
            ),
            None
        );
    }

    #[test]
    fn installed_language_autodetection_obeys_original_sixty_four_preference_bound() {
        let installed = [InstalledLocalization {
            locale: "de-DE".into(),
            path: "de.lmlang".into(),
        }];
        let within_bound = (0..63)
            .map(|index| format!("zz-{index}"))
            .chain(["de-DE".to_owned()]);
        assert_eq!(
            select_preferred_installed_localization(&installed, within_bound),
            Some("de.lmlang".into())
        );

        let beyond_bound = (0..64)
            .map(|index| format!("zz-{index}"))
            .chain(["de-DE".to_owned()]);
        assert_eq!(
            select_preferred_installed_localization(&installed, beyond_bound),
            None
        );
    }

    #[test]
    fn installed_language_autodetection_includes_converted_original_modules() {
        let installed = [InstalledLocalization {
            locale: "fr-CA".into(),
            path: "fr.lmlang".into(),
        }];
        let original_catalog = LocalizationCatalog::new(
            "de-DE",
            UiTextKey::ALL.map(|key| (key, format!("de-{key:?}"))),
        )
        .unwrap();
        let originals = [InstalledOriginalLocalization {
            metadata: lm_app::OriginalLanguageModuleMetadata {
                display_name: "Deutsch".into(),
                version: "3.63".into(),
                locale: "de-DE".into(),
                code_page: "1252".into(),
            },
            catalog: original_catalog.clone(),
            path: "de.dll".into(),
        }];

        assert_eq!(
            select_preferred_installed_localization_with_original(
                &installed,
                &originals,
                ["fr-FR".to_owned(), "de-DE".to_owned()]
            ),
            Some(PreferredInstalledLocalization::Original(original_catalog))
        );
    }
}

#[cfg(feature = "visual-smoke")]
impl NativeApplication {
    fn capture_visual_smoke(&mut self, context: &egui::Context) {
        let Ok(path) = std::env::var("LM_NATIVE_SCREENSHOT_TO") else {
            return;
        };
        let screenshot = context.input(|input| {
            input.events.iter().find_map(|event| {
                let egui::Event::Screenshot {
                    user_data, image, ..
                } = event
                else {
                    return None;
                };
                user_data
                    .data
                    .as_deref()
                    .and_then(|data| data.downcast_ref::<String>())
                    .filter(|marker| marker.as_str() == "lm-native-visual-smoke")
                    .map(|_| image.clone())
            })
        });
        if let Some(image) = screenshot {
            if let Err(error) = save_visual_smoke_image(&image, &path) {
                eprintln!("native visual-smoke capture failed: {error}");
                #[allow(clippy::exit)]
                std::process::exit(1);
            }
            #[allow(clippy::exit)]
            std::process::exit(0);
        }
        self.visual_smoke_frames = self.visual_smoke_frames.saturating_add(1);
        if self.visual_smoke_frames >= 8 && !self.visual_smoke_requested {
            self.visual_smoke_requested = true;
            context.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                "lm-native-visual-smoke".to_owned(),
            )));
        }
        context.request_repaint();
    }
}

#[cfg(feature = "visual-smoke")]
fn save_visual_smoke_image(image: &egui::ColorImage, path: &str) -> Result<(), String> {
    if !std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err("LM_NATIVE_SCREENSHOT_TO must name a .png file".into());
    }
    let pixels = image
        .pixels
        .iter()
        .map(|color| {
            let [red, green, blue, alpha] = color.to_srgba_unmultiplied();
            lm_render::Rgba {
                red,
                green,
                blue,
                alpha,
            }
        })
        .collect();
    let canvas = lm_render::Canvas::from_pixels(image.size[0], image.size[1], pixels)
        .map_err(|error| error.to_string())?;
    let png = lm_render::encode_png(&canvas).map_err(|error| error.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {path}: {error}"))?;
    std::io::Write::write_all(&mut output, &png)
        .map_err(|error| format!("cannot write {path}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_acknowledgement_distinguishes_acceptance_from_rejection() {
        let context = egui::Context::default();
        let mut application = NativeApplication::default();

        assert!(application.try_dispatch(&context, Command::ClearSelection));
        assert!(application.effects.error.is_none());

        assert!(!application.try_dispatch(&context, Command::Save));
        assert!(application.effects.error.is_some());
        assert!(application.app.project().is_none());
    }
}
