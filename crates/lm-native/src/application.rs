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
    custom_collection_append::CustomCollectionAppendDialog,
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
    legacy_graphics_bypass_transfer::LegacyGraphicsBypassTransfer,
    level_access_restriction_dialog::LevelAccessRestrictionDialog,
    level_deletion_dialog::LevelDeletionDialog,
    multiple_level_deletion_dialog::MultipleLevelDeletionDialog,
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
    rom_legacy_graphics_bypass_editor::RomLegacyGraphicsBypassEditor,
    rom_level_assets_editor::RomLevelAssetsEditor,
    rom_lunar_magic_metadata_editor::RomLunarMagicMetadataEditor,
    rom_map16_editor::RomMap16Editor,
    rom_mwl_batch_export_dialog::RomMwlBatchExportDialog,
    rom_mwl_batch_import_dialog::RomMwlBatchImportDialog,
    rom_mwl_import_dialog::RomMwlImportDialog,
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
    rom_user_area_scan_dialog::RomUserAreaScanDialog,
    shortcut_editor::ShortcutEditor,
    ssc_sidecar_editor::SscSidecarEditor,
    toolbar_editor::{ToolbarEditor, ToolbarEditorResult},
    toolbar_graphics_transfer::ToolbarGraphicsTransfer,
    user_toolbar_images::{MainToolbarImageSet, UserToolbarImageSet},
    vanilla_graphics_editor::VanillaGraphicsEditor,
    vanilla_level_editor::VanillaLevelEditor,
    vram_patch_options_dialog::VramPatchOptionsDialog,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IntegratedEmulatorOptions {
    use_f4: bool,
    draw_selected_tiles: bool,
    pause_translucent: bool,
    stop_on_level_change: bool,
}

impl Default for IntegratedEmulatorOptions {
    fn default() -> Self {
        Self {
            use_f4: false,
            draw_selected_tiles: true,
            pause_translucent: false,
            stop_on_level_change: false,
        }
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct IpsSiblingSaveIntent {
    command: Command,
    confirmation: Option<Confirmation>,
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
    legacy_graphics_bypass_transfer: LegacyGraphicsBypassTransfer,
    undo_history_settings: undo_history_settings::UndoHistorySettings,
    open_level_number_dialog: crate::open_level_number_dialog::OpenLevelNumberDialog,
    open_level_address_dialog: crate::open_level_address_dialog::OpenLevelAddressDialog,
    animation_rate_dialog: crate::animation_rate::AnimationRateDialog,
    animation_rate: crate::animation_rate::AnimationRate,
    external_tool_config_editor: crate::external_tool_config_editor::ExternalToolConfigEditor,
    user_toolbar: Option<UserToolbar>,
    user_toolbar_images: UserToolbarImageSet,
    main_toolbar_images: MainToolbarImageSet,
    user_toolbar_observed_document: Option<std::path::PathBuf>,
    user_toolbar_observed_level: Option<u16>,
    user_toolbar_pending_save_notifications: u8,
    user_toolbar_pending_deleted_levels: Vec<u16>,
    user_toolbar_recent_menu_position: Option<egui::Pos2>,
    user_toolbar_recent_clear_confirmation: bool,
    level_text: String,
    special_world_passed: bool,
    joined_graphics_files: bool,
    auto_set_screens: Option<bool>,
    allow_fragmentation: Option<bool>,
    maintain_checksum: Option<bool>,
    silently_add_copier_header: Option<bool>,
    save_prompt: Option<bool>,
    mouse_gestures: Option<bool>,
    save_mouse_gestures: Option<bool>,
    warn_ips_sibling_on_save: Option<bool>,
    convert_berry_gfx_tile: Option<bool>,
    ips_sibling_save_warning: Option<IpsSiblingSaveIntent>,
    ips_sibling_save_authorized: bool,
    two_bpp_view_confirmation: bool,
    truncate_level_confirmation: bool,
    gfx_display_override: crate::vanilla_map16_preview::GfxDisplayOverride,
    gfx_display_override_form: Option<(String, String)>,
    menu_color_fix: Option<bool>,
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
    multiple_level_deletion_dialog: MultipleLevelDeletionDialog,
    level_usage_dialog: LevelUsageDialog,
    rom_user_area_scan_dialog: RomUserAreaScanDialog,
    live_emulator: crate::live_emulator::LiveEmulator,
    integrated_emulator_options: IntegratedEmulatorOptions,
    auto_deselect_on_editor_select: bool,
    show_add_editor_ids: Option<bool>,
    background_cursor_highlight: Option<bool>,
    background_editor_owned: Option<bool>,
    remember_window_size: Option<bool>,
    scan_exits_on_save: Option<bool>,
    count_sprites_on_save: Option<bool>,
    check_object_placement_on_save: Option<bool>,
    correct_fatal_errors: Option<bool>,
    prioritize_allocations_past_2mb: Option<bool>,
    warn_vertical_fireball_buoyancy: Option<bool>,
    gfx_bypass_list_dialogs: Option<bool>,
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
    custom_collection_append_dialog: CustomCollectionAppendDialog,
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
    rom_legacy_fg_bg_bypass_editor: RomLegacyGraphicsBypassEditor,
    rom_legacy_sprite_bypass_editor: RomLegacyGraphicsBypassEditor,
    rom_level_assets_editor: RomLevelAssetsEditor,
    rom_lunar_magic_metadata_editor: RomLunarMagicMetadataEditor,
    rom_mwl_batch_export_dialog: RomMwlBatchExportDialog,
    rom_mwl_batch_import_dialog: RomMwlBatchImportDialog,
    rom_mwl_import_dialog: RomMwlImportDialog,
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
    vram_patch_options_dialog: VramPatchOptionsDialog,
    pending_vram_patch_selection: Option<crate::vram_patch_options_dialog::VramPatchSelection>,
    vram_patch_selection_initialized: bool,
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
    const ALLOW_FRAGMENTATION_STORAGE_KEY: &'static str = "lunar_magic_rust.allow_fragmentation.v1";
    const MAINTAIN_CHECKSUM_STORAGE_KEY: &'static str = "lunar_magic_rust.maintain_checksum.v1";
    const SILENTLY_ADD_HEADER_STORAGE_KEY: &'static str = "lunar_magic_rust.silently_add_header.v1";
    const SAVE_PROMPT_STORAGE_KEY: &'static str = "lunar_magic_rust.save_prompt.v1";
    const MOUSE_GESTURES_STORAGE_KEY: &'static str = "lunar_magic_rust.mouse_gestures.v1";
    const SAVE_MOUSE_GESTURES_STORAGE_KEY: &'static str = "lunar_magic_rust.save_mouse_gestures.v1";
    const EXTERNAL_TOOLS_STORAGE_KEY: &'static str = "lunar_magic_rust.external_tools.v1";
    const ANIMATION_RATE_STORAGE_KEY: &'static str = "lunar_magic_rust.animation_rate.v1";
    const INTEGRATED_EMULATOR_STORAGE_KEY: &'static str = "lunar_magic_rust.integrated_emulator.v1";
    const AUTO_DESELECT_STORAGE_KEY: &'static str = "lunar_magic_rust.auto_deselect.v1";
    const SHOW_ADD_EDITOR_IDS_STORAGE_KEY: &'static str = "lunar_magic_rust.show_add_editor_ids.v1";
    const BACKGROUND_CURSOR_STORAGE_KEY: &'static str =
        "lunar_magic_rust.background_cursor_highlight.v1";
    const BACKGROUND_EDITOR_OWNED_STORAGE_KEY: &'static str =
        "lunar_magic_rust.background_editor_owned.v1";
    const REMEMBER_WINDOW_SIZE_STORAGE_KEY: &'static str =
        "lunar_magic_rust.remember_window_size.v1";
    const SCAN_EXITS_ON_SAVE_STORAGE_KEY: &'static str = "lunar_magic_rust.scan_exits_on_save.v1";
    const COUNT_SPRITES_ON_SAVE_STORAGE_KEY: &'static str =
        "lunar_magic_rust.count_sprites_on_save.v1";
    const CHECK_OBJECT_PLACEMENT_STORAGE_KEY: &'static str =
        "lunar_magic_rust.check_object_placement_on_save.v1";
    const CORRECT_FATAL_ERRORS_STORAGE_KEY: &'static str =
        "lunar_magic_rust.correct_fatal_errors.v1";
    const PRIORITIZE_ALLOCATIONS_PAST_2MB_STORAGE_KEY: &'static str =
        "lunar_magic_rust.prioritize_allocations_past_2mb.v1";
    const WARN_VERTICAL_FIREBALL_STORAGE_KEY: &'static str =
        "lunar_magic_rust.warn_vertical_fireball_buoyancy.v1";
    const WARN_IPS_SIBLING_STORAGE_KEY: &'static str =
        "lunar_magic_rust.warn_ips_sibling_on_save.v1";
    const CONVERT_BERRY_GFX_STORAGE_KEY: &'static str =
        "lunar_magic_rust.convert_berry_gfx_tile.v1";
    const GFX_BYPASS_LIST_DIALOGS_STORAGE_KEY: &'static str =
        "lunar_magic_rust.gfx_bypass_list_dialogs.v1";

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
        if let Some(encoded) = storage.get_string(Self::ANIMATION_RATE_STORAGE_KEY) {
            match crate::animation_rate::decode_preference(&encoded) {
                Ok(rate) => self.animation_rate = rate,
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load animation-rate preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::INTEGRATED_EMULATOR_STORAGE_KEY) {
            match decode_integrated_emulator_options(&encoded) {
                Ok(options) => self.integrated_emulator_options = options,
                Err(error) => {
                    self.effects.error = Some(format!(
                        "cannot load integrated-emulator preferences: {error}"
                    ));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::AUTO_DESELECT_STORAGE_KEY) {
            match decode_auto_deselect_preference(&encoded) {
                Ok(enabled) => self.auto_deselect_on_editor_select = enabled,
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load auto-deselect preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::SHOW_ADD_EDITOR_IDS_STORAGE_KEY) {
            match decode_show_add_editor_ids_preference(&encoded) {
                Ok(enabled) => self.show_add_editor_ids = Some(enabled),
                Err(error) => {
                    self.effects.error = Some(format!(
                        "cannot load Add Object/Sprite ID preference: {error}"
                    ));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::BACKGROUND_CURSOR_STORAGE_KEY) {
            match decode_background_cursor_preference(&encoded) {
                Ok(enabled) => self.background_cursor_highlight = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load background-cursor preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::BACKGROUND_EDITOR_OWNED_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "background-editor ownership") {
                Ok(enabled) => self.background_editor_owned = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::REMEMBER_WINDOW_SIZE_STORAGE_KEY) {
            match decode_remember_window_size_preference(&encoded) {
                Ok(enabled) => self.remember_window_size = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load window-size preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::SCAN_EXITS_ON_SAVE_STORAGE_KEY) {
            match decode_scan_exits_on_save_preference(&encoded) {
                Ok(enabled) => self.scan_exits_on_save = Some(enabled),
                Err(error) => {
                    self.effects.error = Some(format!("cannot load exit-scan preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::COUNT_SPRITES_ON_SAVE_STORAGE_KEY) {
            match decode_count_sprites_on_save_preference(&encoded) {
                Ok(enabled) => self.count_sprites_on_save = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load sprite-count preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::CHECK_OBJECT_PLACEMENT_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "object-placement save warning") {
                Ok(enabled) => self.check_object_placement_on_save = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::CORRECT_FATAL_ERRORS_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "fatal level-layout correction") {
                Ok(enabled) => self.correct_fatal_errors = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::PRIORITIZE_ALLOCATIONS_PAST_2MB_STORAGE_KEY)
        {
            match decode_enabled_preference(&encoded, "allocation above 2 MiB preference") {
                Ok(enabled) => self.prioritize_allocations_past_2mb = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::WARN_VERTICAL_FIREBALL_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "vertical-fireball buoyancy warning") {
                Ok(enabled) => self.warn_vertical_fireball_buoyancy = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::WARN_IPS_SIBLING_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "same-name IPS save warning") {
                Ok(enabled) => self.warn_ips_sibling_on_save = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::CONVERT_BERRY_GFX_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "berry GFX tile conversion") {
                Ok(enabled) => self.convert_berry_gfx_tile = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::GFX_BYPASS_LIST_DIALOGS_STORAGE_KEY) {
            match decode_gfx_bypass_list_dialogs_preference(&encoded) {
                Ok(enabled) => self.gfx_bypass_list_dialogs = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load GFX-bypass dialog preference: {error}"));
                }
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
        if let Some(encoded) = storage.get_string(Self::ALLOW_FRAGMENTATION_STORAGE_KEY) {
            match decode_allow_fragmentation_preference(&encoded) {
                Ok(enabled) => self.allow_fragmentation = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load fragmentation preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::MAINTAIN_CHECKSUM_STORAGE_KEY) {
            match decode_maintain_checksum_preference(&encoded) {
                Ok(enabled) => self.maintain_checksum = Some(enabled),
                Err(error) => {
                    self.effects.error = Some(format!("cannot load checksum preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::SILENTLY_ADD_HEADER_STORAGE_KEY) {
            match decode_silently_add_header_preference(&encoded) {
                Ok(enabled) => self.silently_add_copier_header = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load ROM-header preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::SAVE_PROMPT_STORAGE_KEY) {
            match decode_save_prompt_preference(&encoded) {
                Ok(enabled) => self.save_prompt = Some(enabled),
                Err(error) => {
                    self.effects.error =
                        Some(format!("cannot load save-prompt preference: {error}"));
                }
            }
        }
        if let Some(encoded) = storage.get_string(Self::MOUSE_GESTURES_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "mouse-gesture") {
                Ok(enabled) => self.mouse_gestures = Some(enabled),
                Err(error) => self.effects.error = Some(error),
            }
        }
        if let Some(encoded) = storage.get_string(Self::SAVE_MOUSE_GESTURES_STORAGE_KEY) {
            match decode_enabled_preference(&encoded, "mouse-gesture auto-save") {
                Ok(enabled) => self.save_mouse_gestures = Some(enabled),
                Err(error) => self.effects.error = Some(error),
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

    fn should_warn_for_same_name_ips(&self) -> bool {
        self.warn_ips_sibling_on_save.unwrap_or(true)
            && self
                .app
                .document_path
                .as_deref()
                .is_some_and(same_name_ips_sibling_exists)
    }

    fn queue_same_name_ips_warning(
        &mut self,
        command: Command,
        confirmation: Option<Confirmation>,
    ) {
        self.ips_sibling_save_warning = Some(IpsSiblingSaveIntent {
            command,
            confirmation,
        });
        self.app.status = "Waiting for same-name IPS save choice".into();
    }

    /// Dispatches one command and reports whether application state accepted it.
    ///
    /// ROM editor windows use this acknowledgement before discarding their staged controller.
    fn try_dispatch(&mut self, context: &egui::Context, command: Command) -> bool {
        if matches!(command, Command::Save) {
            if self.ips_sibling_save_authorized {
                self.ips_sibling_save_authorized = false;
            } else if self.should_warn_for_same_name_ips() {
                self.queue_same_name_ips_warning(command, None);
                return true;
            }
        }
        let mouse_gesture = self
            .vanilla_level_editor
            .take_mouse_gesture_command(&command);
        let auto_save_mouse_gesture = mouse_gesture && self.save_mouse_gestures.unwrap_or(false);
        if (self.save_prompt.unwrap_or(true) || auto_save_mouse_gesture)
            && command_leaves_staged_level(&self.app, &command)
            && self
                .vanilla_level_editor
                .request_save_prompt_transition(command.clone())
        {
            if auto_save_mouse_gesture {
                self.vanilla_level_editor
                    .auto_confirm_pending_transition_save();
                self.app.status = "Saving staged level before mouse gesture".into();
            } else {
                self.app.status = "Waiting for staged level save choice".into();
            }
            return true;
        }
        if self.save_prompt.unwrap_or(true)
            && command_leaves_staged_overworld(&self.app, &command)
            && self
                .rom_overworld_editor
                .request_save_prompt_transition(command.clone())
        {
            self.app.status = "Waiting for staged overworld save choice".into();
            return true;
        }
        let clears_deferred_vram = matches!(
            &command,
            Command::Open | Command::Reload | Command::Close | Command::Quit
        );
        match self.app.dispatch(command) {
            Ok(effects) => {
                if clears_deferred_vram {
                    self.pending_vram_patch_selection = None;
                    self.vram_patch_selection_initialized = false;
                }
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
                    if self.should_warn_for_same_name_ips() {
                        self.queue_same_name_ips_warning(Command::Save, Some(confirmation));
                    } else {
                        self.effects.save_before_confirmation_action(
                            &mut self.app,
                            context,
                            confirmation,
                        );
                    }
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
                        Confirmation::DiscardAndReload => self.app.discard_and_request_reload(),
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

    fn show_same_name_ips_warning(&mut self, context: &egui::Context) {
        let Some(intent) = self.ips_sibling_save_warning.clone() else {
            return;
        };
        let file_name = self
            .app
            .document_path
            .as_deref()
            .map(same_name_ips_sibling_path)
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "ROMFileName.ips".into());
        let mut save_anyway = false;
        let mut cancel = false;
        egui::Window::new("Check if ROMFileName.ips Exists")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(format!(
                    "A same-name IPS file ({file_name}) exists beside the ROM. Some emulators automatically apply it, which can hide saved editor changes or cause other problems."
                ));
                ui.label("Rename or move the IPS file to avoid automatic patching.");
                ui.label("Save the ROM anyway?");
                ui.horizontal(|ui| {
                    if ui.button("Save Anyway").clicked() {
                        save_anyway = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if save_anyway {
            self.ips_sibling_save_warning = None;
            if let Some(confirmation) = intent.confirmation {
                self.effects
                    .save_before_confirmation_action(&mut self.app, context, confirmation);
            } else {
                self.ips_sibling_save_authorized = true;
                let _accepted = self.try_dispatch(context, intent.command);
            }
        } else if cancel {
            self.ips_sibling_save_warning = None;
            if matches!(intent.confirmation, Some(Confirmation::DiscardAndOpen)) {
                self.effects.cancel_requested_rom_path();
            }
            self.app.status = "ROM save cancelled because a same-name IPS file exists".into();
        }
    }

    fn show_two_bpp_view_confirmation(&mut self, context: &egui::Context) {
        if !self.two_bpp_view_confirmation {
            return;
        }
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new("Lunar Magic Rust")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Switch 2bpp viewing mode?");
                ui.horizontal(|ui| {
                    accept = ui.button("Yes").clicked();
                    cancel = ui.button("No").clicked();
                });
            });
        if accept {
            self.two_bpp_view_confirmation = false;
            self.app.status = self.vanilla_level_editor.toolbar_cycle_two_bpp_view_mode();
            self.renderer.invalidate();
        } else if cancel {
            self.two_bpp_view_confirmation = false;
        }
    }

    fn show_truncate_level_confirmation(&mut self, context: &egui::Context) {
        if !self.truncate_level_confirmation {
            return;
        }
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new("Remove data beyond max screens?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(
                    "This will delete all objects and sprites beyond the current max screen limit for this level mode.  Proceed?",
                );
                ui.horizontal(|ui| {
                    accept = ui.button("Yes").clicked();
                    cancel = ui.button("No").clicked();
                });
            });
        if accept {
            self.truncate_level_confirmation = false;
            let removed = if crate::vanilla_level_editor::VanillaLevelEditor::handles(&self.app) {
                self.vanilla_level_editor
                    .toolbar_truncate_beyond_mode_limit()
                    .map_err(|error| error.to_owned())
            } else {
                self.rom_level_assets_editor
                    .toolbar_truncate_beyond_mode_limit()
                    .ok_or_else(|| "no editable level workspace is open".to_owned())
            };
            match removed {
                Ok((layer1, layer2, sprites)) => {
                    self.app.status = format!(
                        "Removed {layer1} Layer 1 object(s), {layer2} Layer 2 object(s), and {sprites} sprite(s) beyond the level-mode screen limit"
                    );
                    self.renderer.invalidate();
                }
                Err(error) => self.effects.error = Some(error),
            }
        } else if cancel {
            self.truncate_level_confirmation = false;
        }
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
                let synchronization = if should_stop_integrated_emulator_on_level_change(
                    self.integrated_emulator_options,
                    source,
                    target,
                ) {
                    self.live_emulator.stop();
                    self.app.status = "Internal emulator stopped on level change.".into();
                    Ok(())
                } else if source.1 != target.1 {
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
        storage.set_string(
            Self::ALLOW_FRAGMENTATION_STORAGE_KEY,
            encode_allow_fragmentation_preference(self.allow_fragmentation.unwrap_or(true)),
        );
        storage.set_string(
            Self::MAINTAIN_CHECKSUM_STORAGE_KEY,
            encode_maintain_checksum_preference(self.maintain_checksum.unwrap_or(true)),
        );
        storage.set_string(
            Self::SILENTLY_ADD_HEADER_STORAGE_KEY,
            encode_silently_add_header_preference(self.silently_add_copier_header.unwrap_or(true)),
        );
        storage.set_string(
            Self::SAVE_PROMPT_STORAGE_KEY,
            encode_save_prompt_preference(self.save_prompt.unwrap_or(true)),
        );
        storage.set_string(
            Self::MOUSE_GESTURES_STORAGE_KEY,
            encode_enabled_preference(self.mouse_gestures.unwrap_or(true)),
        );
        storage.set_string(
            Self::SAVE_MOUSE_GESTURES_STORAGE_KEY,
            encode_enabled_preference(self.save_mouse_gestures.unwrap_or(false)),
        );
        storage.set_string(
            Self::ANIMATION_RATE_STORAGE_KEY,
            crate::animation_rate::encode_preference(self.animation_rate),
        );
        storage.set_string(
            Self::INTEGRATED_EMULATOR_STORAGE_KEY,
            encode_integrated_emulator_options(self.integrated_emulator_options),
        );
        storage.set_string(
            Self::AUTO_DESELECT_STORAGE_KEY,
            encode_auto_deselect_preference(self.auto_deselect_on_editor_select),
        );
        storage.set_string(
            Self::SHOW_ADD_EDITOR_IDS_STORAGE_KEY,
            encode_show_add_editor_ids_preference(self.show_add_editor_ids.unwrap_or(true)),
        );
        storage.set_string(
            Self::BACKGROUND_CURSOR_STORAGE_KEY,
            encode_background_cursor_preference(self.background_cursor_highlight.unwrap_or(true)),
        );
        storage.set_string(
            Self::BACKGROUND_EDITOR_OWNED_STORAGE_KEY,
            encode_enabled_preference(self.background_editor_owned.unwrap_or(false)),
        );
        storage.set_string(
            Self::REMEMBER_WINDOW_SIZE_STORAGE_KEY,
            encode_remember_window_size_preference(self.remember_window_size.unwrap_or(true)),
        );
        storage.set_string(
            Self::SCAN_EXITS_ON_SAVE_STORAGE_KEY,
            encode_scan_exits_on_save_preference(self.scan_exits_on_save.unwrap_or(true)),
        );
        storage.set_string(
            Self::COUNT_SPRITES_ON_SAVE_STORAGE_KEY,
            encode_count_sprites_on_save_preference(self.count_sprites_on_save.unwrap_or(true)),
        );
        storage.set_string(
            Self::CHECK_OBJECT_PLACEMENT_STORAGE_KEY,
            encode_enabled_preference(self.check_object_placement_on_save.unwrap_or(true)),
        );
        storage.set_string(
            Self::CORRECT_FATAL_ERRORS_STORAGE_KEY,
            encode_enabled_preference(self.correct_fatal_errors.unwrap_or(true)),
        );
        storage.set_string(
            Self::PRIORITIZE_ALLOCATIONS_PAST_2MB_STORAGE_KEY,
            encode_enabled_preference(self.prioritize_allocations_past_2mb.unwrap_or(true)),
        );
        storage.set_string(
            Self::WARN_VERTICAL_FIREBALL_STORAGE_KEY,
            encode_enabled_preference(self.warn_vertical_fireball_buoyancy.unwrap_or(true)),
        );
        storage.set_string(
            Self::WARN_IPS_SIBLING_STORAGE_KEY,
            encode_enabled_preference(self.warn_ips_sibling_on_save.unwrap_or(true)),
        );
        storage.set_string(
            Self::CONVERT_BERRY_GFX_STORAGE_KEY,
            encode_enabled_preference(self.convert_berry_gfx_tile.unwrap_or(true)),
        );
        storage.set_string(
            Self::GFX_BYPASS_LIST_DIALOGS_STORAGE_KEY,
            encode_gfx_bypass_list_dialogs_preference(self.gfx_bypass_list_dialogs.unwrap_or(true)),
        );
        if !self.remember_window_size.unwrap_or(true) {
            // eframe stores native window geometry under this key immediately before App::save.
            // An invalid value makes its next-start decoder fall back to NativeOptions' default.
            storage.set_string("window", String::new());
        }
        match encode_external_tools_preference(self.app.external_tools()) {
            Ok(encoded) => storage.set_string(Self::EXTERNAL_TOOLS_STORAGE_KEY, encoded),
            Err(error) => {
                self.effects.error =
                    Some(format!("cannot save external-tool preferences: {error}"));
            }
        }
    }

    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.app
            .set_maintain_checksum(self.maintain_checksum.unwrap_or(true));
        self.app.set_prioritize_allocations_past_2mb(
            self.prioritize_allocations_past_2mb.unwrap_or(true),
        );
        self.app
            .set_silently_add_copier_header(self.silently_add_copier_header.unwrap_or(true));
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
            let live_frame = self
                .live_emulator
                .canvas_frame(self.integrated_emulator_options.pause_translucent);
            self.vanilla_level_editor
                .initialize_draw_selection_over_live(
                    self.integrated_emulator_options.draw_selected_tiles,
                );
            self.vanilla_level_editor
                .set_auto_deselect_on_editor_select(self.auto_deselect_on_editor_select);
            self.vanilla_level_editor
                .set_show_add_editor_ids(self.show_add_editor_ids.unwrap_or(true));
            self.vanilla_level_editor
                .set_background_cursor_highlight(self.background_cursor_highlight.unwrap_or(true));
            self.vanilla_level_editor
                .set_scan_exits_on_save(self.scan_exits_on_save.unwrap_or(true));
            self.vanilla_level_editor
                .set_count_sprites_on_save(self.count_sprites_on_save.unwrap_or(true));
            self.vanilla_level_editor
                .set_check_object_placement_on_save(
                    self.check_object_placement_on_save.unwrap_or(true),
                );
            self.vanilla_level_editor
                .set_correct_fatal_errors(self.correct_fatal_errors.unwrap_or(true));
            self.vanilla_level_editor
                .set_warn_vertical_fireball_buoyancy(
                    self.warn_vertical_fireball_buoyancy.unwrap_or(true),
                );
            self.vanilla_level_editor
                .set_auto_set_screens(self.auto_set_screens.unwrap_or(true));
            self.vanilla_level_editor
                .set_allow_fragmentation(self.allow_fragmentation.unwrap_or(true));
            self.vanilla_level_editor
                .set_mouse_gestures(self.mouse_gestures.unwrap_or(true));
            self.vanilla_level_editor
                .set_convert_berry_gfx_tile(self.convert_berry_gfx_tile.unwrap_or(true));
            if !self.vram_patch_selection_initialized && self.app.project().is_some() {
                self.pending_vram_patch_selection =
                    crate::vram_patch_options_dialog::effective_selection(&self.app);
                self.vram_patch_selection_initialized = true;
            }
            let effective_vram_patch_selection = self.pending_vram_patch_selection;
            self.vanilla_level_editor
                .set_deferred_rom_option_save(effective_vram_patch_selection.is_some());
            self.rom_legacy_fg_bg_bypass_editor
                .set_use_list_dialog(self.gfx_bypass_list_dialogs.unwrap_or(true));
            self.rom_legacy_sprite_bypass_editor
                .set_use_list_dialog(self.gfx_bypass_list_dialogs.unwrap_or(true));
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
                    self.dsc_sidecar_editor.resolved(),
                    self.native_map16_sidecar_editor.value(),
                    live_frame,
                    &self.main_toolbar_images,
                    self.animation_rate,
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
                let command = if level_commit {
                    if let Some(selection) = effective_vram_patch_selection {
                        let snapshot = match self.app.controller_snapshot() {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                self.effects.error = Some(error.to_string());
                                return;
                            }
                        };
                        match crate::vram_patch_options_dialog::prepare_level_save_command(
                            &snapshot, selection, command,
                        ) {
                            Ok(command) => command,
                            Err(error) => {
                                self.effects.error = Some(error);
                                return;
                            }
                        }
                    } else {
                        command
                    }
                } else {
                    command
                };
                let command = if level_commit {
                    let snapshot = match self.app.controller_snapshot() {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            self.effects.error = Some(error.to_string());
                            return;
                        }
                    };
                    match crate::menu_color_fix::prepare_level_save_command(
                        &snapshot,
                        self.menu_color_fix.unwrap_or(true),
                        command,
                    ) {
                        Ok(command) => command,
                        Err(error) => {
                            self.effects.error = Some(error);
                            return;
                        }
                    }
                } else {
                    command
                };
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
                    self.convert_berry_gfx_tile.unwrap_or(true),
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
        self.integrated_emulator_options.draw_selected_tiles =
            self.vanilla_level_editor.draw_selection_over_live();
        self.show_user_toolbar_recent_menu(context);
        self.show_confirmation(context);
        self.show_same_name_ips_warning(context);
        self.show_two_bpp_view_confirmation(context);
        self.show_truncate_level_confirmation(context);
        if let Some(status) = self
            .custom_collection_append_dialog
            .show(context, self.app.document_path.as_deref())
        {
            self.app.status = status;
        }
        if let Some((level, command)) = self.level_deletion_dialog.show(context, &self.app)
            && self.try_dispatch(context, command)
        {
            self.renderer.invalidate();
            self.vanilla_level_editor.invalidate_graphics_preview();
            self.mark_user_toolbar_level_deleted(level);
            self.dispatch(context, Command::Save);
        }
        if let Some(request) = self
            .multiple_level_deletion_dialog
            .show(context, &self.app)
            && self.try_dispatch(context, request.command)
        {
            self.renderer.invalidate();
            self.vanilla_level_editor.invalidate_graphics_preview();
            for level in request.levels {
                self.mark_user_toolbar_level_deleted(level);
            }
            self.dispatch(context, Command::Save);
        }
        self.about_dialog.show(context, self.app.localization());
        self.diagnostics_dialog
            .show(context, self.app.localization());
        self.help_dialog.show(context, self.app.localization());
        if let Some(level) = self
            .open_level_number_dialog
            .show(context, self.app.localization())
        {
            self.dispatch(context, Command::SelectLevel(level));
        }
        if let Some(address) = self
            .open_level_address_dialog
            .show(context, self.app.localization())
        {
            match self.vanilla_level_editor.open_layer1_from_pc_address(
                &self.app,
                address,
                self.ssc_sidecar_editor.resolved(),
            ) {
                Ok(()) => {
                    self.app.status = format!("Opened Layer 1 from PC address ${address:X}");
                }
                Err(error) => self.effects.error = Some(error),
            }
        }
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
        if let Some(rate) = self.animation_rate_dialog.show(context) {
            self.animation_rate = rate;
            self.renderer.invalidate();
            self.vanilla_level_editor.invalidate_graphics_preview();
            self.app.status = format!(
                "Animation rate set to {} fps",
                1.0 / rate.interval_seconds()
            );
        }
        self.show_editor_windows(context);
        self.show_global_effects(context);
        let palette_recovery_revision = self
            .rom_palette_editor
            .staged_recovery_generation(&self.app);
        let map16_recovery_revision = self.rom_map16_editor.staged_recovery_generation(&self.app);
        let graphics_recovery_revision = self
            .rom_graphics_editor
            .staged_recovery_generation(&self.app);
        let exanimation_recovery_revision = self
            .rom_exanimation_editor
            .staged_recovery_generation(&self.app);
        let title_tilemap_recovery_revision = self
            .rom_title_tilemap_editor
            .staged_recovery_generation(&self.app);
        let credits_tilemap_recovery_revision = self
            .rom_credits_tilemap_editor
            .staged_recovery_generation(&self.app);
        let expanded_settings_recovery_revision = self
            .rom_expanded_settings_editor
            .staged_recovery_generation(&self.app);
        let legacy_fg_bg_recovery_revision = self
            .rom_legacy_fg_bg_bypass_editor
            .staged_recovery_generation(&self.app);
        let legacy_sprite_recovery_revision = self
            .rom_legacy_sprite_bypass_editor
            .staged_recovery_generation(&self.app);
        let title_recording_recovery_revision = self
            .rom_title_recording_editor
            .staged_recovery_generation(&self.app);
        let shared_palette_recovery_revision = self
            .rom_shared_palette_editor
            .staged_recovery_generation(&self.app);
        let overworld_message_recovery_revision = self
            .rom_overworld_message_editor
            .staged_recovery_generation(&self.app);
        let boss_sequence_recovery_revision = self
            .rom_boss_sequence_editor
            .staged_recovery_generation(&self.app);
        let secondary_exit_recovery_revision = self
            .rom_secondary_exit_editor
            .staged_recovery_generation(&self.app);
        let level_name_recovery_revision = self
            .rom_overworld_level_name_editor
            .staged_recovery_generation(&self.app);
        let player_start_recovery_revision = self
            .rom_overworld_player_start_editor
            .staged_recovery_generation(&self.app);
        let overworld_settings_recovery_revision = self
            .rom_overworld_settings_editor
            .staged_recovery_generation(&self.app);
        let special_event_recovery_revision = self
            .rom_overworld_special_event_editor
            .staged_recovery_generation(&self.app);
        let event_number_recovery_revision = self
            .rom_overworld_event_number_editor
            .staged_recovery_generation(&self.app);
        let event_reveal_recovery_revision = self
            .rom_overworld_event_reveal_editor
            .staged_recovery_generation(&self.app);
        let event_tilemap_recovery_revision = self
            .rom_overworld_event_tilemap_editor
            .staged_recovery_generation(&self.app);
        let metadata_recovery_revision = self
            .rom_lunar_magic_metadata_editor
            .staged_recovery_generation(&self.app);
        let path_link_recovery_revision = self
            .rom_overworld_path_link_editor
            .staged_recovery_generation(&self.app);
        let warp_link_recovery_revision = self
            .rom_overworld_warp_link_editor
            .staged_recovery_generation(&self.app);
        let level_assets_recovery_revision = self
            .rom_level_assets_editor
            .staged_recovery_generation(&self.app);
        let overworld_recovery_revision = self
            .rom_overworld_editor
            .staged_recovery_generation(&self.app);
        let level_recovery_revision = self.vanilla_level_editor.recovery_generation(&self.app);
        let recovery_revision = [
            level_recovery_revision,
            palette_recovery_revision,
            map16_recovery_revision,
            graphics_recovery_revision,
            exanimation_recovery_revision,
            title_tilemap_recovery_revision,
            credits_tilemap_recovery_revision,
            expanded_settings_recovery_revision,
            legacy_fg_bg_recovery_revision,
            legacy_sprite_recovery_revision,
            title_recording_recovery_revision,
            shared_palette_recovery_revision,
            overworld_message_recovery_revision,
            boss_sequence_recovery_revision,
            secondary_exit_recovery_revision,
            level_name_recovery_revision,
            player_start_recovery_revision,
            overworld_settings_recovery_revision,
            special_event_recovery_revision,
            event_number_recovery_revision,
            event_reveal_recovery_revision,
            event_tilemap_recovery_revision,
            metadata_recovery_revision,
            path_link_recovery_revision,
            warp_link_recovery_revision,
            level_assets_recovery_revision,
            overworld_recovery_revision,
        ]
        .into_iter()
        .flatten()
        .reduce(|combined, revision| combined.rotate_left(11) ^ revision);
        self.recovery_store
            .synchronize_project(recovery_revision, || {
                let staged_editors =
                    usize::from(self.vanilla_level_editor.has_staged_recovery_edits())
                        + usize::from(palette_recovery_revision.is_some())
                        + usize::from(map16_recovery_revision.is_some())
                        + usize::from(graphics_recovery_revision.is_some())
                        + usize::from(exanimation_recovery_revision.is_some())
                        + usize::from(title_tilemap_recovery_revision.is_some())
                        + usize::from(credits_tilemap_recovery_revision.is_some())
                        + usize::from(expanded_settings_recovery_revision.is_some())
                        + usize::from(legacy_fg_bg_recovery_revision.is_some())
                        + usize::from(legacy_sprite_recovery_revision.is_some())
                        + usize::from(title_recording_recovery_revision.is_some())
                        + usize::from(shared_palette_recovery_revision.is_some())
                        + usize::from(overworld_message_recovery_revision.is_some())
                        + usize::from(boss_sequence_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(secondary_exit_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(level_name_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(player_start_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(overworld_settings_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(special_event_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(event_number_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(event_reveal_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(event_tilemap_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(metadata_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(path_link_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(warp_link_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(level_assets_recovery_revision.is_some());
                let staged_editors =
                    staged_editors + usize::from(overworld_recovery_revision.is_some());
                if staged_editors == 2
                    && path_link_recovery_revision.is_some()
                    && warp_link_recovery_revision.is_some()
                {
                    let paths = self
                        .rom_overworld_path_link_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged path-link recovery table disappeared")?;
                    let warps = self
                        .rom_overworld_warp_link_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged warp-link recovery table disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_overworld_navigation_links(
                            paths,
                            warps,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 4
                    && event_number_recovery_revision.is_some()
                    && event_reveal_recovery_revision.is_some()
                    && special_event_recovery_revision.is_some()
                    && event_tilemap_recovery_revision.is_some()
                {
                    let numbers = self
                        .rom_overworld_event_number_editor
                        .staged_recovery_map(&self.app)?
                        .ok_or("staged event-number recovery map disappeared")?;
                    let reveals = self
                        .rom_overworld_event_reveal_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged event-reveal recovery table disappeared")?;
                    let special = self
                        .rom_overworld_special_event_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged special-event recovery table disappeared")?;
                    let tilemaps = self
                        .rom_overworld_event_tilemap_editor
                        .staged_recovery_buffers(&self.app)?
                        .ok_or("staged event-tilemap recovery buffers disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_overworld_event_family(
                            numbers,
                            reveals,
                            special,
                            tilemaps,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 3
                    && level_name_recovery_revision.is_some()
                    && player_start_recovery_revision.is_some()
                    && overworld_settings_recovery_revision.is_some()
                {
                    let names = self
                        .rom_overworld_level_name_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged level-name recovery table disappeared")?;
                    let starts = self
                        .rom_overworld_player_start_editor
                        .staged_recovery_starts(&self.app)?
                        .ok_or("staged player-start recovery table disappeared")?;
                    let settings = self
                        .rom_overworld_settings_editor
                        .staged_recovery_settings(&self.app)?
                        .ok_or("staged overworld-settings recovery table disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_overworld_configuration(
                            names,
                            starts,
                            settings,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 2
                    && overworld_message_recovery_revision.is_some()
                    && boss_sequence_recovery_revision.is_some()
                {
                    let messages = self
                        .rom_overworld_message_editor
                        .staged_recovery_messages(&self.app)?
                        .ok_or("staged overworld-message recovery table disappeared")?;
                    let boss_sequence = self
                        .rom_boss_sequence_editor
                        .staged_recovery_table(&self.app)?
                        .ok_or("staged boss-sequence recovery table disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_overworld_message_family(
                            messages,
                            boss_sequence,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 2
                    && title_tilemap_recovery_revision.is_some()
                    && credits_tilemap_recovery_revision.is_some()
                {
                    let title = self
                        .rom_title_tilemap_editor
                        .staged_recovery_tilemap(&self.app)?
                        .ok_or("staged title tilemap disappeared")?;
                    let credits = self
                        .rom_credits_tilemap_editor
                        .staged_recovery_tilemap(&self.app)?
                        .ok_or("staged credits tilemap disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_global_tilemaps(
                            title,
                            credits,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 2
                    && palette_recovery_revision.is_some()
                    && shared_palette_recovery_revision.is_some()
                {
                    let mutation = self
                        .rom_palette_editor
                        .staged_recovery_mutation(&self.app)?
                        .ok_or("staged installed-palette mutation disappeared")?;
                    let shared = self
                        .rom_shared_palette_editor
                        .staged_recovery_palette(&self.app)?
                        .ok_or("staged shared-palette table disappeared")?;
                    return self
                        .app
                        .recovery_snapshot_with_palette_family(
                            &mutation,
                            shared,
                            self.app.current_level(),
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors == 2
                    && graphics_recovery_revision.is_some()
                    && exanimation_recovery_revision.is_some()
                {
                    let (graphics, graphics_level) = self
                        .rom_graphics_editor
                        .staged_recovery_mutation(&self.app)?
                        .ok_or("staged graphics mutation disappeared")?;
                    let (exanimation, exanimation_level) = self
                        .rom_exanimation_editor
                        .staged_recovery_mutation(&self.app)?
                        .ok_or("staged ExAnimation mutation disappeared")?;
                    let level = graphics_level.or(Some(exanimation_level));
                    return self
                        .app
                        .recovery_snapshot_with_graphics_family(
                            &graphics,
                            &exanimation,
                            level,
                        )
                        .map_err(|error| error.to_string());
                }
                if staged_editors > 1 {
                    return Err(
                        "cannot compose simultaneous staged level, level-assets, expanded settings, legacy graphics bypass, title recording, shared palette, overworld/boss messages, secondary exits, overworld level names/player starts/settings/special events/event numbers/event reveals/event tilemaps/metadata/path/warp links, graphics, ExAnimation, title/credits tilemap, palette, Map16, or overworld recovery yet".into(),
                    );
                }
                if palette_recovery_revision.is_some() {
                    self.rom_palette_editor.staged_recovery_snapshot(&self.app)
                } else if map16_recovery_revision.is_some() {
                    self.rom_map16_editor.staged_recovery_snapshot(&self.app)
                } else if graphics_recovery_revision.is_some() {
                    self.rom_graphics_editor
                        .staged_recovery_snapshot(&self.app)
                } else if exanimation_recovery_revision.is_some() {
                    self.rom_exanimation_editor
                        .staged_recovery_snapshot(&self.app)
                } else if title_tilemap_recovery_revision.is_some() {
                    self.rom_title_tilemap_editor
                        .staged_recovery_snapshot(&self.app)
                } else if credits_tilemap_recovery_revision.is_some() {
                    self.rom_credits_tilemap_editor
                        .staged_recovery_snapshot(&self.app)
                } else if expanded_settings_recovery_revision.is_some() {
                    self.rom_expanded_settings_editor
                        .staged_recovery_snapshot(&self.app)
                } else if legacy_fg_bg_recovery_revision.is_some() {
                    self.rom_legacy_fg_bg_bypass_editor
                        .staged_recovery_snapshot(&self.app)
                } else if legacy_sprite_recovery_revision.is_some() {
                    self.rom_legacy_sprite_bypass_editor
                        .staged_recovery_snapshot(&self.app)
                } else if title_recording_recovery_revision.is_some() {
                    self.rom_title_recording_editor
                        .staged_recovery_snapshot(&self.app)
                } else if shared_palette_recovery_revision.is_some() {
                    self.rom_shared_palette_editor
                        .staged_recovery_snapshot(&self.app)
                } else if overworld_message_recovery_revision.is_some() {
                    self.rom_overworld_message_editor
                        .staged_recovery_snapshot(&self.app)
                } else if boss_sequence_recovery_revision.is_some() {
                    self.rom_boss_sequence_editor
                        .staged_recovery_snapshot(&self.app)
                } else if secondary_exit_recovery_revision.is_some() {
                    self.rom_secondary_exit_editor
                        .staged_recovery_snapshot(&self.app)
                } else if level_name_recovery_revision.is_some() {
                    self.rom_overworld_level_name_editor
                        .staged_recovery_snapshot(&self.app)
                } else if player_start_recovery_revision.is_some() {
                    self.rom_overworld_player_start_editor
                        .staged_recovery_snapshot(&self.app)
                } else if overworld_settings_recovery_revision.is_some() {
                    self.rom_overworld_settings_editor
                        .staged_recovery_snapshot(&self.app)
                } else if special_event_recovery_revision.is_some() {
                    self.rom_overworld_special_event_editor
                        .staged_recovery_snapshot(&self.app)
                } else if event_number_recovery_revision.is_some() {
                    self.rom_overworld_event_number_editor
                        .staged_recovery_snapshot(&self.app)
                } else if event_reveal_recovery_revision.is_some() {
                    self.rom_overworld_event_reveal_editor
                        .staged_recovery_snapshot(&self.app)
                } else if event_tilemap_recovery_revision.is_some() {
                    self.rom_overworld_event_tilemap_editor
                        .staged_recovery_snapshot(&self.app)
                } else if metadata_recovery_revision.is_some() {
                    self.rom_lunar_magic_metadata_editor
                        .staged_recovery_snapshot(&self.app)
                } else if path_link_recovery_revision.is_some() {
                    self.rom_overworld_path_link_editor
                        .staged_recovery_snapshot(&self.app)
                } else if warp_link_recovery_revision.is_some() {
                    self.rom_overworld_warp_link_editor
                        .staged_recovery_snapshot(&self.app)
                } else if level_assets_recovery_revision.is_some() {
                    self.rom_level_assets_editor
                        .staged_recovery_snapshot(&self.app)
                } else if overworld_recovery_revision.is_some() {
                    self.rom_overworld_editor
                        .staged_recovery_snapshot(&self.app)
                } else {
                    self.vanilla_level_editor
                        .staged_recovery_snapshot(&self.app)
                }
            });
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

fn encode_allow_fragmentation_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_allow_fragmentation_preference(value: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err("unknown fragmentation preference version".into()),
    }
}

fn encode_maintain_checksum_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_maintain_checksum_preference(value: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err("unknown checksum preference version".into()),
    }
}

fn encode_silently_add_header_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_silently_add_header_preference(value: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err("unknown ROM-header preference version".into()),
    }
}

fn encode_save_prompt_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_save_prompt_preference(value: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err("unknown save-prompt preference version".into()),
    }
}

fn encode_enabled_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_enabled_preference(value: &str, name: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err(format!("cannot load {name} preference: unknown version")),
    }
}

fn same_name_ips_sibling_path(rom_path: &std::path::Path) -> std::path::PathBuf {
    rom_path.with_extension("ips")
}

fn same_name_ips_sibling_exists(rom_path: &std::path::Path) -> bool {
    std::fs::metadata(same_name_ips_sibling_path(rom_path)).is_ok_and(|metadata| !metadata.is_dir())
}

fn command_leaves_staged_level(app: &lm_app::AppState, command: &Command) -> bool {
    let lm_app::EditorMode::Level(current) = app.mode else {
        return false;
    };
    match command {
        Command::SelectLevel(level) => *level != current,
        Command::NavigateLevel(_)
        | Command::Open
        | Command::Reload
        | Command::Close
        | Command::Quit
        | Command::ShowOverworld
        | Command::ShowMap16
        | Command::ShowGraphics(_)
        | Command::ShowPalette(_)
        | Command::ShowExAnimation(_)
        | Command::ShowLayer3(_) => true,
        _ => false,
    }
}

fn command_leaves_staged_overworld(app: &lm_app::AppState, command: &Command) -> bool {
    if app.mode != lm_app::EditorMode::Overworld {
        return false;
    }
    matches!(
        command,
        Command::Open
            | Command::Reload
            | Command::Close
            | Command::Quit
            | Command::SelectLevel(_)
            | Command::ShowMap16
            | Command::ShowGraphics(_)
            | Command::ShowPalette(_)
            | Command::ShowExAnimation(_)
            | Command::ShowLayer3(_)
    )
}

fn encode_auto_deselect_preference(enabled: bool) -> String {
    if enabled { "enabled" } else { "disabled" }.to_owned()
}

fn decode_auto_deselect_preference(value: &str) -> Result<bool, String> {
    match value {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        _ => Err("unknown auto-deselect preference version".into()),
    }
}

fn encode_show_add_editor_ids_preference(enabled: bool) -> String {
    if enabled { "shown" } else { "hidden" }.to_owned()
}

fn decode_show_add_editor_ids_preference(value: &str) -> Result<bool, String> {
    match value {
        "shown" => Ok(true),
        "hidden" => Ok(false),
        _ => Err("unknown Add Object/Sprite ID preference version".into()),
    }
}

fn encode_background_cursor_preference(enabled: bool) -> String {
    if enabled { "highlighted" } else { "plain" }.to_owned()
}

fn decode_background_cursor_preference(value: &str) -> Result<bool, String> {
    match value {
        "highlighted" => Ok(true),
        "plain" => Ok(false),
        _ => Err("unknown background-cursor preference version".into()),
    }
}

fn encode_remember_window_size_preference(enabled: bool) -> String {
    if enabled { "remember" } else { "default" }.to_owned()
}

fn decode_remember_window_size_preference(value: &str) -> Result<bool, String> {
    match value {
        "remember" => Ok(true),
        "default" => Ok(false),
        _ => Err("unknown window-size preference version".into()),
    }
}

fn encode_scan_exits_on_save_preference(enabled: bool) -> String {
    if enabled { "scan" } else { "skip" }.to_owned()
}

fn decode_scan_exits_on_save_preference(value: &str) -> Result<bool, String> {
    match value {
        "scan" => Ok(true),
        "skip" => Ok(false),
        _ => Err("unknown exit-scan preference version".into()),
    }
}

fn encode_count_sprites_on_save_preference(enabled: bool) -> String {
    if enabled { "count" } else { "skip" }.to_owned()
}

fn decode_count_sprites_on_save_preference(value: &str) -> Result<bool, String> {
    match value {
        "count" => Ok(true),
        "skip" => Ok(false),
        _ => Err("unknown sprite-count preference version".into()),
    }
}

fn encode_gfx_bypass_list_dialogs_preference(enabled: bool) -> String {
    if enabled { "lists" } else { "fields" }.to_owned()
}

fn decode_gfx_bypass_list_dialogs_preference(value: &str) -> Result<bool, String> {
    match value {
        "lists" => Ok(true),
        "fields" => Ok(false),
        _ => Err("unknown GFX-bypass dialog preference version".into()),
    }
}

fn encode_integrated_emulator_options(options: IntegratedEmulatorOptions) -> String {
    let bits = u8::from(options.use_f4)
        | (u8::from(options.draw_selected_tiles) << 1)
        | (u8::from(options.pause_translucent) << 2)
        | (u8::from(options.stop_on_level_change) << 3);
    format!("v1:{bits:02x}")
}

fn should_stop_integrated_emulator_on_level_change(
    options: IntegratedEmulatorOptions,
    source: (u16, u64),
    target: (u16, u64),
) -> bool {
    options.stop_on_level_change && source.0 != target.0
}

fn decode_integrated_emulator_options(value: &str) -> Result<IntegratedEmulatorOptions, String> {
    let encoded = value
        .strip_prefix("v1:")
        .ok_or_else(|| "unknown integrated-emulator preference version".to_owned())?;
    if encoded.len() != 2 {
        return Err("invalid integrated-emulator preference length".into());
    }
    let bits = u8::from_str_radix(encoded, 16)
        .map_err(|_| "invalid integrated-emulator preference bits".to_owned())?;
    if bits & !0x0f != 0 {
        return Err("unknown integrated-emulator preference bits".into());
    }
    Ok(IntegratedEmulatorOptions {
        use_f4: bits & 1 != 0,
        draw_selected_tiles: bits & 2 != 0,
        pause_translucent: bits & 4 != 0,
        stop_on_level_change: bits & 8 != 0,
    })
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
    fn native_save_and_reopen_persist_the_original_main_animation_rate() {
        for rate in crate::animation_rate::AnimationRate::ALL {
            let mut source = NativeApplication {
                animation_rate: rate,
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.animation_rate, rate);
        }
    }

    #[test]
    fn native_save_and_reopen_persist_integrated_emulator_options() {
        let expected = IntegratedEmulatorOptions {
            use_f4: true,
            draw_selected_tiles: false,
            pause_translucent: true,
            stop_on_level_change: true,
        };
        let mut source = NativeApplication {
            integrated_emulator_options: expected,
            ..NativeApplication::default()
        };
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut source, &mut storage);

        let mut reopened = NativeApplication::default();
        reopened.load_persistent_preferences(Some(&storage));
        assert_eq!(reopened.integrated_emulator_options, expected);
    }

    #[test]
    fn native_save_and_reopen_persist_auto_deselect_on_editor_select() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                auto_deselect_on_editor_select: expected,
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.auto_deselect_on_editor_select, expected);
        }
        assert!(decode_auto_deselect_preference("true").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_add_editor_id_visibility() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                show_add_editor_ids: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.show_add_editor_ids, Some(expected));
        }
        assert!(decode_show_add_editor_ids_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_background_cursor_highlight() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                background_cursor_highlight: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.background_cursor_highlight, Some(expected));
        }
        assert!(decode_background_cursor_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_background_editor_ownership() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                background_editor_owned: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.background_editor_owned, Some(expected));
        }
    }

    #[test]
    fn remember_window_size_preserves_or_invalidates_eframes_geometry_record() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                remember_window_size: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            storage.set_string("window", "captured-geometry".into());
            eframe::App::save(&mut source, &mut storage);

            assert_eq!(
                storage.get_string("window").as_deref(),
                Some(if expected { "captured-geometry" } else { "" })
            );
            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.remember_window_size, Some(expected));
        }
        assert!(decode_remember_window_size_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_scan_exits_on_save() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                scan_exits_on_save: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.scan_exits_on_save, Some(expected));
        }
        assert!(decode_scan_exits_on_save_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_count_sprites_on_save() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                count_sprites_on_save: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.count_sprites_on_save, Some(expected));
        }
        assert!(decode_count_sprites_on_save_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_vertical_fireball_warning() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                warn_vertical_fireball_buoyancy: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.warn_vertical_fireball_buoyancy, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_object_placement_warning() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                check_object_placement_on_save: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.check_object_placement_on_save, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_fatal_error_correction() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                correct_fatal_errors: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.correct_fatal_errors, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_past_2mb_allocation_preference() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                prioritize_allocations_past_2mb: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.prioritize_allocations_past_2mb, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_same_name_ips_warning() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                warn_ips_sibling_on_save: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.warn_ips_sibling_on_save, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_berry_gfx_conversion() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                convert_berry_gfx_tile: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.convert_berry_gfx_tile, Some(expected));
        }
    }

    #[test]
    fn same_name_ips_warning_accepts_files_rejects_directories_and_precedes_save_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let rom_path = directory.path().join("game.smc");
        let ips_path = directory.path().join("game.ips");
        std::fs::write(&rom_path, crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        assert_eq!(same_name_ips_sibling_path(&rom_path), ips_path);
        assert!(!same_name_ips_sibling_exists(&rom_path));
        std::fs::create_dir(&ips_path).unwrap();
        assert!(!same_name_ips_sibling_exists(&rom_path));
        std::fs::remove_dir(&ips_path).unwrap();
        std::fs::write(&ips_path, b"PATCH").unwrap();
        assert!(same_name_ips_sibling_exists(&rom_path));

        let mut native = NativeApplication::default();
        native
            .app
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        native.app.document_path = Some(rom_path);
        assert!(native.try_dispatch(&egui::Context::default(), Command::Save));
        assert_eq!(
            native.ips_sibling_save_warning,
            Some(IpsSiblingSaveIntent {
                command: Command::Save,
                confirmation: None,
            })
        );
        assert_eq!(native.app.pending_save_request_id(), None);
        native.set_warn_ips_sibling_on_save(false);
        assert!(native.ips_sibling_save_warning.is_none());
    }

    #[test]
    fn native_save_and_reopen_persist_gfx_bypass_dialog_style() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                gfx_bypass_list_dialogs: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.gfx_bypass_list_dialogs, Some(expected));
        }
        assert!(decode_gfx_bypass_list_dialogs_preference("enabled").is_err());
    }

    #[test]
    fn native_save_and_reopen_persist_allow_fragmentation() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                allow_fragmentation: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.allow_fragmentation, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_maintain_checksum() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                maintain_checksum: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.maintain_checksum, Some(expected));
        }
    }

    #[test]
    fn native_save_and_reopen_persist_silently_add_header() {
        for expected in [false, true] {
            let mut source = NativeApplication {
                silently_add_copier_header: Some(expected),
                ..NativeApplication::default()
            };
            let mut storage = MemoryStorage::default();
            eframe::App::save(&mut source, &mut storage);

            let mut reopened = NativeApplication::default();
            reopened.load_persistent_preferences(Some(&storage));
            assert_eq!(reopened.silently_add_copier_header, Some(expected));
        }
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
    fn allow_fragmentation_preference_round_trips_both_original_modes() {
        assert_eq!(encode_allow_fragmentation_preference(true), "enabled");
        assert_eq!(encode_allow_fragmentation_preference(false), "disabled");
        assert!(decode_allow_fragmentation_preference("enabled").unwrap());
        assert!(!decode_allow_fragmentation_preference("disabled").unwrap());
        assert!(decode_allow_fragmentation_preference("true").is_err());
    }

    #[test]
    fn maintain_checksum_preference_round_trips_both_original_modes() {
        assert_eq!(encode_maintain_checksum_preference(true), "enabled");
        assert_eq!(encode_maintain_checksum_preference(false), "disabled");
        assert!(decode_maintain_checksum_preference("enabled").unwrap());
        assert!(!decode_maintain_checksum_preference("disabled").unwrap());
        assert!(decode_maintain_checksum_preference("true").is_err());
    }

    #[test]
    fn silently_add_header_preference_round_trips_both_original_modes() {
        assert_eq!(encode_silently_add_header_preference(true), "enabled");
        assert_eq!(encode_silently_add_header_preference(false), "disabled");
        assert!(decode_silently_add_header_preference("enabled").unwrap());
        assert!(!decode_silently_add_header_preference("disabled").unwrap());
        assert!(decode_silently_add_header_preference("true").is_err());
    }

    #[test]
    fn save_prompt_preference_round_trips_both_original_modes() {
        assert_eq!(encode_save_prompt_preference(true), "enabled");
        assert_eq!(encode_save_prompt_preference(false), "disabled");
        assert!(decode_save_prompt_preference("enabled").unwrap());
        assert!(!decode_save_prompt_preference("disabled").unwrap());
        assert!(decode_save_prompt_preference("true").is_err());
    }

    #[test]
    fn mouse_gesture_preferences_round_trip_both_original_modes() {
        assert_eq!(encode_enabled_preference(true), "enabled");
        assert_eq!(encode_enabled_preference(false), "disabled");
        assert!(decode_enabled_preference("enabled", "mouse-gesture").unwrap());
        assert!(!decode_enabled_preference("disabled", "mouse-gesture auto-save").unwrap());
        assert!(decode_enabled_preference("true", "mouse-gesture").is_err());
    }

    #[test]
    fn integrated_emulator_preferences_round_trip_all_original_toggles() {
        let options = IntegratedEmulatorOptions {
            use_f4: true,
            draw_selected_tiles: false,
            pause_translucent: true,
            stop_on_level_change: true,
        };
        assert_eq!(encode_integrated_emulator_options(options), "v1:0d");
        assert_eq!(
            decode_integrated_emulator_options("v1:0d").unwrap(),
            options
        );
        assert!(decode_integrated_emulator_options("v2:0d").is_err());
        assert!(decode_integrated_emulator_options("v1:10").is_err());
    }

    #[test]
    fn integrated_emulator_stop_option_includes_simultaneous_revision_changes() {
        let options = IntegratedEmulatorOptions {
            stop_on_level_change: true,
            ..IntegratedEmulatorOptions::default()
        };
        assert!(should_stop_integrated_emulator_on_level_change(
            options,
            (0x105, 7),
            (0x106, 7)
        ));
        assert!(!should_stop_integrated_emulator_on_level_change(
            options,
            (0x105, 7),
            (0x105, 7)
        ));
        assert!(should_stop_integrated_emulator_on_level_change(
            options,
            (0x105, 7),
            (0x106, 8)
        ));
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

    #[test]
    fn closing_a_rom_discards_its_deferred_vram_choice() {
        let context = egui::Context::default();
        let mut application = NativeApplication::default();
        application
            .app
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        application.pending_vram_patch_selection =
            Some(crate::vram_patch_options_dialog::VramPatchSelection::Normal);

        assert!(application.try_dispatch(&context, Command::Close));
        assert!(application.pending_vram_patch_selection.is_none());
        assert!(application.app.project().is_none());
    }
}
