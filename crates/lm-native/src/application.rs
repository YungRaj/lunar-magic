use crate::{
    appearance_editor::AppearanceEditor,
    built_in_runtime_installer::BuiltInRuntimeInstaller,
    configuration_loader::{ConfigurationLoader, LoadedConfiguration},
    copier_header_dialog::CopierHeaderDialog,
    custom_object_editor::CustomObjectEditor,
    custom_sprite_editor::CustomSpriteEditor,
    dsc_sidecar_editor::DscSidecarEditor,
    editor_view,
    effects::Confirmation,
    effects::EffectState,
    exanimation_editor::ExAnimationEditor,
    expanded_settings_editor::ExpandedSettingsEditor,
    graphics_editor::GraphicsEditor,
    graphics_migration_dialog::GraphicsMigrationDialog,
    ips_create_dialog::IpsCreateDialog,
    ips_patch_dialog::IpsPatchDialog,
    layer3_editor::Layer3Editor,
    level_editor::LevelEditor,
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
    ssc_sidecar_editor::SscSidecarEditor,
    vanilla_graphics_editor::VanillaGraphicsEditor,
    vanilla_level_editor::VanillaLevelEditor,
};
use eframe::egui;
use lm_app::{AppState, Command, EditorMode, UiTextKey};

mod document_menus;
mod menus;
mod rom_menus;
mod rom_windows;
mod shutdown;
mod toolbar;
mod windows;

#[derive(Default)]
pub(crate) struct NativeApplication {
    app: AppState,
    effects: EffectState,
    level_text: String,
    renderer: NativeRenderState,
    vanilla_graphics_editor: VanillaGraphicsEditor,
    vanilla_level_editor: VanillaLevelEditor,
    palette_editor: PaletteEditor,
    graphics_editor: GraphicsEditor,
    map16_editor: Map16Editor,
    exanimation_editor: ExAnimationEditor,
    level_editor: LevelEditor,
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
    recent_state: Option<lm_app::recent_state_file::RecentStateFile>,
    configuration_loader: ConfigurationLoader,
    profile_loader: crate::profile_loader::ProfileLoader,
    #[cfg(feature = "visual-smoke")]
    visual_smoke_frames: u8,
    #[cfg(feature = "visual-smoke")]
    visual_smoke_requested: bool,
}

impl NativeApplication {
    pub(crate) fn from_startup(
        initialized: Result<crate::startup::InitializedNative, String>,
    ) -> Self {
        match initialized {
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
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved changes?");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.effects.confirmation = None;
                        if matches!(confirmation, Confirmation::DiscardAndOpen) {
                            self.effects.cancel_requested_rom_path();
                        }
                    }
                    if ui.button("Discard").clicked() {
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
        self.handle_shortcuts(context);
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
        });
        if let Err(error) = result {
            self.effects.error = Some(error);
        }
    }

    fn show_global_effects(&mut self, context: &egui::Context) {
        self.effects.show_rom_loader(context, &mut self.app);
        self.effects.show_persistence(context, &mut self.app);
        self.effects.show_external_tools(context, &mut self.app);
        if let Some(error) = self.effects.error.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.effects.error = None;
                    }
                });
        }
        if self.effects.quit_requested {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for NativeApplication {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if context.input(|input| input.viewport().close_requested()) && !self.effects.quit_requested
        {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_quit(context);
        }
        self.prepare_frame(context);
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
            if vanilla_level
                && let Some(command) = self.vanilla_level_editor.show(
                    ui,
                    &self.app,
                    self.ssc_sidecar_editor.resolved(),
                    self.ssc_sidecar_editor.external_assets(),
                    self.ssc_sidecar_editor.asset_revision(),
                    self.osc_sidecar_editor.resolved(),
                    self.native_map16_sidecar_editor.value(),
                )
            {
                self.dispatch(context, command);
            } else if vanilla_graphics
                && let Some(command) = self.vanilla_graphics_editor.show(ui, &self.app)
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
        self.show_editor_windows(context);
        self.show_global_effects(context);
        #[cfg(feature = "visual-smoke")]
        self.capture_visual_smoke(context);
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
