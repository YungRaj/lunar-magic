use crate::{
    document_loader::DocumentLoader,
    level_editor_forms,
    overworld_editor_animation::OverworldAnimationPanel,
    overworld_editor_palette::OverworldPalettePanel,
    overworld_editor_records::OverworldRecordPanels,
    overworld_editor_render::{self, OverworldAssets},
};
use eframe::egui;
use lm_app::{
    AppState, Command, ExtendedUiTextKey as Key, LocalizationCatalog,
    NativeCustomOverworldSpriteController, NativeCustomOverworldSpriteEdit, OverworldController,
    OverworldControllerEdit, OverworldLayerId, ProfiledControllerSnapshot,
    SmwMainOverworldLayer2Controller,
};
use lm_graphics::{Palette, PaletteOwnership};
use lm_overworld::{
    OverworldEndpoint, OverworldPathDirection, OverworldPathLink, OverworldPathLinkTable,
    OverworldPathTarget,
};

use lm_project::{CompleteOverworldFile, CompleteOverworldShape};
use std::collections::BTreeSet;

fn ow_text(catalog: Option<&LocalizationCatalog>, key: lm_app::ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

mod commit;
mod lifecycle;
mod transfer;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Records,
    Palette,
    Animation,
    NativeSprites,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingEditorTransitionSave {
    transition: Command,
    final_revision: u64,
    intermediate_command: Option<Command>,
}

#[cfg(test)]
mod overworld_sprite_gesture_oracle_tests {
    use std::{fs, path::PathBuf};

    const FIXTURES: [(&str, &str); 3] = [
        (
            "selected-two.png",
            "03e534d27b83d07478eb4dc3e663884318d6e1476f0c265f91d91c54f53e27cc",
        ),
        (
            "alt-property-dialog.png",
            "68c267ae6b6fd3bb42182038e74b4aaa0f509d87ca5c32543f93bcb2b73838de",
        ),
        (
            "right-drag-group.png",
            "b2d40f58ded2df2cae8c2bd70f14addc5c706346a6485455e41981d11ce8b969",
        ),
    ];

    fn fixture_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/oracle-work/lm363/pristine-us/overworld-sprite-gestures")
    }

    #[test]
    fn retained_lunar_magic_overworld_sprite_gestures_are_hash_and_structure_bound() {
        for (name, expected_sha256) in FIXTURES {
            let bytes = fs::read(fixture_directory().join(name)).unwrap();
            assert_eq!(lm_oracle::sha256_hex(&bytes), expected_sha256, "{name}");
            assert_eq!(
                bytes.get(..8),
                Some(b"\x89PNG\r\n\x1a\n".as_slice()),
                "{name}"
            );
            assert_eq!(bytes.get(12..16), Some(b"IHDR".as_slice()), "{name}");
            assert_eq!(
                u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
                1202,
                "{name}"
            );
            assert_eq!(
                u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
                1252,
                "{name}"
            );
            assert_eq!(bytes[24], 8, "{name}");
            assert_eq!(bytes[25], 6, "{name}");
            assert!(bytes.windows(4).any(|chunk| chunk == b"IDAT"), "{name}");
            assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND", "{name}");
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MapPaintTool {
    #[default]
    Select,
    Brush,
    Rectangle,
    Fill,
    NativeSprite,
    RouteSource,
    RouteDestination,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct PendingOpen {
    profiled: ProfiledControllerSnapshot,
    slot: String,
    rom_path: Option<std::path::PathBuf>,
}

struct PendingLoad {
    open: PendingOpen,
    slot: u16,
}

struct Workspace {
    controller: OverworldController,
    profiled: ProfiledControllerSnapshot,
    slot: u16,
    image: lm_rom::RomImage,
    ownership: PaletteOwnership,
    assets: OverworldAssets,
    baseline_animation_options: [crate::overworld_editor_render::OverworldAnimationOptions; 7],
    native_appearances: Option<lm_render::NativeOverworldAppearancePair>,
    native_sprites: NativeCustomOverworldSpriteController,
    native_sprite_layout: lm_profile::SmwUsV1NativeCustomOverworldSpriteLayout,
}

#[derive(Clone)]
struct NativeSpriteForm {
    map: usize,
    index: usize,
    id: String,
    x: String,
    y: String,
    screen: String,
    extra: String,
}

struct NativeSpritePropertyDialog {
    form: NativeSpriteForm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeSpriteDrag {
    map: usize,
    anchor: (usize, usize),
    selected: Vec<usize>,
    kind: NativeSpriteDragKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeSpriteDragKind {
    Move,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeSpriteSecondaryAction {
    EditProperties(usize),
    DuplicateSelection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeSpriteMarquee {
    map: usize,
    anchor: (usize, usize),
    baseline: BTreeSet<usize>,
    current: (usize, usize),
}

impl Default for NativeSpriteForm {
    fn default() -> Self {
        Self {
            map: 0,
            index: 0,
            id: "00".into(),
            x: "0000".into(),
            y: "0000".into(),
            screen: "00".into(),
            extra: String::new(),
        }
    }
}

struct MainLayer2Workspace {
    controller: SmwMainOverworldLayer2Controller,
    original_paths: OverworldPathLinkTable,
    paths: OverworldPathLinkTable,
    palette: Palette,
    assets: OverworldAssets,
}

#[derive(Default)]
struct MainPathLinkForm {
    index: usize,
    source_x: String,
    source_y: String,
    source_submap: String,
    destination_x: String,
    destination_y: String,
    destination_submap: String,
    target_x: String,
    target_y: String,
    direction: OverworldPathDirection,
    one_way: bool,
    loaded: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum OverworldAnimationRate {
    Fps7_5,
    #[default]
    Fps15,
    Fps30,
    Fps60,
}

impl OverworldAnimationRate {
    const ALL: [Self; 4] = [Self::Fps7_5, Self::Fps15, Self::Fps30, Self::Fps60];

    const fn interval_seconds(self) -> f64 {
        match self {
            Self::Fps7_5 => 0.120,
            Self::Fps15 => 0.060,
            Self::Fps30 => 0.030,
            Self::Fps60 => 0.015,
        }
    }

    const fn substeps_per_tick(self) -> usize {
        match self {
            Self::Fps7_5 => 8,
            Self::Fps15 => 4,
            Self::Fps30 => 2,
            Self::Fps60 => 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Fps7_5 => "7.5 fps",
            Self::Fps15 => "15 fps",
            Self::Fps30 => "30 fps",
            Self::Fps60 => "60 fps",
        }
    }
}

#[derive(Default)]
pub(crate) struct RomOverworldEditor {
    workspace: Option<Workspace>,
    main_layer2_workspace: Option<MainLayer2Workspace>,
    pending_open: Option<PendingOpen>,
    panel: Panel,
    records: OverworldRecordPanels,
    palette: OverworldPalettePanel,
    animation: OverworldAnimationPanel,
    layer: usize,
    x: usize,
    y: usize,
    tile: String,
    loaded: Option<(u64, usize, usize, usize)>,
    paint_tool: MapPaintTool,
    paint_anchor: Option<(usize, usize)>,
    completed_reveals: usize,
    rendered_key: Option<(u64, usize, usize)>,
    texture: Option<egui::TextureHandle>,
    animation_graphics_texture: Option<egui::TextureHandle>,
    animation_preview_paused: bool,
    animation_preview_origin: Option<f64>,
    animation_preview_tick: usize,
    animation_preview_rate: OverworldAnimationRate,
    animation_preview_triggers: lm_graphics::ExAnimationTriggerPreviewState,
    animation_preview_events_passed: Vec<bool>,
    animation_preview_trigger_kind: usize,
    animation_preview_trigger_index: usize,
    animation_preview_event: usize,
    animation_option_map: usize,
    native_sprite: NativeSpriteForm,
    native_sprite_selection: BTreeSet<usize>,
    native_sprite_drag: Option<NativeSpriteDrag>,
    native_sprite_marquee: Option<NativeSpriteMarquee>,
    native_sprite_property_dialog: Option<NativeSpritePropertyDialog>,
    map16_page: usize,
    map16_rendered_key: Option<(u64, usize)>,
    map16_texture: Option<egui::TextureHandle>,
    direct_tile_palette: usize,
    direct_tile_rendered_palette: Option<usize>,
    direct_tile_texture: Option<egui::TextureHandle>,
    main_path: MainPathLinkForm,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    transfer_loader: DocumentLoader,
    transfer_persistence: crate::persistence_worker::PersistenceWorker,
    transfer_kind: Option<transfer::TransferKind>,
    editor_transition_prompt: Option<Command>,
    editor_transition_after_save: Option<PendingEditorTransitionSave>,
    authorized_editor_transition: Option<Command>,
}

impl RomOverworldEditor {
    pub(crate) fn staged_main_terrain_and_paths(
        &self,
        app: &AppState,
    ) -> Result<
        (
            Option<lm_project::RomMutation>,
            Option<&lm_overworld::OverworldPathLinkTable>,
        ),
        String,
    > {
        let workspace = self
            .main_layer2_workspace
            .as_ref()
            .ok_or("playable main-overworld workspace is closed")?;
        if workspace.controller.revision() != app.project_revision() {
            return Err("stale playable main-overworld workspace cannot be recovered".into());
        }
        let terrain = if workspace.controller.is_modified() {
            Some(overworld_mutation_from_command(
                app,
                self.prepare_main_layer2_commit()?,
            )?)
        } else {
            None
        };
        let paths = (workspace.paths != workspace.original_paths).then_some(&workspace.paths);
        Ok((terrain, paths))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        if let Some(workspace) = self.workspace.as_ref()
            && (workspace.controller.is_modified()
                || workspace.native_sprites.is_modified()
                || workspace.assets.animation_options != workspace.baseline_animation_options)
        {
            let (features, lightning) = overworld_editor_render::encode_overworld_animation_options(
                workspace.assets.animation_options,
                workspace.assets.animation_lightning_unused_low_bit,
            );
            let option_generation = features
                .into_iter()
                .fold(u64::from(lightning), |value, byte| {
                    value.rotate_left(7) ^ u64::from(byte)
                });
            return Some(
                app.project_revision().wrapping_mul(0x8ebc_6af0_9c88_c6e3)
                    ^ workspace.controller.revision().rotate_left(13)
                    ^ workspace.native_sprites.revision().rotate_left(37)
                    ^ option_generation
                    ^ 0x4f56_4552_574f_524c,
            );
        }
        let workspace = self.main_layer2_workspace.as_ref()?;
        (workspace.controller.is_modified() || workspace.paths != workspace.original_paths).then(
            || {
                app.project_revision().wrapping_mul(0x5899_65cc_7537_4cc3)
                    ^ workspace.controller.revision().rotate_left(19)
                    ^ 0x4f57_4c32_0000_0000
            },
        )
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        if let Some(workspace) = self.workspace.as_ref() {
            let command = self.prepare_commit()?;
            return recovery_snapshot_from_overworld_command(app, command, Some(workspace.slot));
        }
        let workspace = self
            .main_layer2_workspace
            .as_ref()
            .ok_or("overworld workspace is closed")?;
        let terrain_mutation = if workspace.controller.is_modified() {
            let command = self.prepare_main_layer2_commit()?;
            Some(overworld_mutation_from_command(app, command)?)
        } else {
            None
        };
        if workspace.paths != workspace.original_paths {
            return app
                .recovery_snapshot_with_overworld_path_links(
                    terrain_mutation.as_ref(),
                    &workspace.paths,
                    None,
                )
                .map_err(|error| error.to_string());
        }
        let mutation = terrain_mutation.ok_or("overworld workspace has no staged changes")?;
        app.recovery_snapshot_with_mutation(&mutation, None)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn request_save_prompt_transition(&mut self, command: Command) -> bool {
        if self.authorized_editor_transition.as_ref() == Some(&command) {
            self.authorized_editor_transition = None;
            return false;
        }
        if !self.has_staged_changes() {
            return false;
        }
        self.editor_transition_prompt = Some(command);
        true
    }

    fn has_staged_changes(&self) -> bool {
        self.workspace.as_ref().is_some_and(|workspace| {
            workspace.controller.is_modified()
                || workspace.native_sprites.is_modified()
                || workspace.assets.animation_options != workspace.baseline_animation_options
        }) || self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| {
                workspace.controller.is_modified() || workspace.paths != workspace.original_paths
            })
    }

    fn prepare_transition_commits(&self, revision: u64) -> Result<Vec<Command>, String> {
        if self.workspace.is_some() {
            return self.prepare_commit().map(|command| vec![command]);
        }
        let workspace = self
            .main_layer2_workspace
            .as_ref()
            .ok_or("overworld workspace is closed")?;
        let terrain = workspace.controller.is_modified();
        let paths = workspace.paths != workspace.original_paths;
        match (terrain, paths) {
            (true, false) => self
                .prepare_main_layer2_commit()
                .map(|command| vec![command]),
            (false, true) => Ok(vec![Command::ReplaceNativeOverworldPathLinks {
                rev: workspace.controller.revision(),
                table: Box::new(workspace.paths.clone()),
            }]),
            (true, true) => Ok(vec![
                self.prepare_main_layer2_commit()?,
                Command::ReplaceNativeOverworldPathLinks {
                    rev: revision.checked_add(1).ok_or("project revision overflow")?,
                    table: Box::new(workspace.paths.clone()),
                },
            ]),
            (false, false) => Err("the overworld has no staged changes".into()),
        }
    }

    fn take_editor_transition_after_save(&mut self, revision: u64) -> Option<Command> {
        let pending = self.editor_transition_after_save.as_mut()?;
        if let Some(command) = pending.intermediate_command.take() {
            if revision.checked_add(1) == Some(pending.final_revision) {
                return Some(command);
            }
            self.editor_transition_after_save = None;
            return None;
        }
        if revision == pending.final_revision {
            let command = pending.transition.clone();
            self.editor_transition_after_save = None;
            self.authorized_editor_transition = Some(command.clone());
            return Some(command);
        }
        if revision > pending.final_revision {
            self.editor_transition_after_save = None;
        }
        None
    }

    fn show_editor_transition_confirmation(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> Option<Command> {
        self.editor_transition_prompt.as_ref()?;
        let mut choice = None;
        egui::Window::new("Save overworld to ROM?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The overworld has staged changes. Save before continuing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(0_u8);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
        self.resolve_editor_transition_choice(choice, revision)
    }

    fn resolve_editor_transition_choice(
        &mut self,
        choice: Option<u8>,
        revision: u64,
    ) -> Option<Command> {
        let transition = self.editor_transition_prompt.clone()?;
        match choice {
            Some(0) => match self.prepare_transition_commits(revision) {
                Ok(mut commands) => {
                    let Some(expected) = revision.checked_add(commands.len() as u64) else {
                        self.error = Some("project revision overflow".into());
                        return None;
                    };
                    let command = commands.remove(0);
                    self.editor_transition_prompt = None;
                    self.editor_transition_after_save = Some(PendingEditorTransitionSave {
                        transition,
                        final_revision: expected,
                        intermediate_command: commands.pop(),
                    });
                    Some(command)
                }
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(1) => {
                self.editor_transition_prompt = None;
                self.clear();
                self.authorized_editor_transition = Some(transition.clone());
                Some(transition)
            }
            Some(2) => {
                self.editor_transition_prompt = None;
                None
            }
            _ => None,
        }
    }

    fn main_layer2_contents(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.main_layer2_workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        let shape = CompleteOverworldShape {
            width: lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
            height: lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
            event_reveals: 0,
            endpoints: 0,
            messages: 0,
            sprites: 0,
            sprite_record_len: 0,
            palette_colors: workspace.palette.colors.len(),
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                ow_text(catalog, Key::RomOverworldStaleNotice),
            );
        }
        let paths_modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.paths != workspace.original_paths);
        ui.label(ow_text(catalog, Key::RomOverworldPlayableMapNotice));
        self.layer = 1;
        self.world_canvas(ui, shape, stale || paths_modified);
        self.main_layer2_tile_controls(ui, shape, stale || paths_modified, catalog);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(ow_text(catalog, Key::RomOverworldAllocation));
            ui.text_edit_singleline(&mut self.search_start);
            ui.label(ow_text(catalog, Key::RomOverworldRangeSeparator));
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.is_modified());
        if ui
            .add_enabled(
                modified && !paths_modified && !stale,
                egui::Button::new(ow_text(catalog, Key::RomOverworldCommitPlayable)),
            )
            .clicked()
        {
            match self.prepare_main_layer2_commit() {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        ui.label(ow_text(
            catalog,
            if modified {
                Key::RomOverworldPlayableStaged
            } else {
                Key::RomOverworldPlayableUnmodified
            },
        ));
        if paths_modified {
            ui.small(ow_text(catalog, Key::RomOverworldRouteBlocksTerrain));
        }
        ui.separator();
        if let Some(path_command) = self.main_path_link_controls(ui, stale, modified, catalog) {
            return Some(path_command);
        }
        None
    }

    fn main_path_link_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        terrain_modified: bool,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let path_count = self
            .main_layer2_workspace
            .as_ref()
            .map_or(0, |workspace| workspace.paths.links.len());
        let paths_modified = self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.paths != workspace.original_paths);
        let mut command = None;
        ui.collapsing(ow_text(catalog, Key::RomOverworldRouteTitle), |ui| {
            ui.label(ow_text(catalog, Key::RomOverworldRouteNotice));
            ui.small(ow_text(catalog, Key::RomOverworldRouteCanvasNotice));
            if path_count == 0 {
                ui.label(ow_text(catalog, Key::RomOverworldRouteUnavailable));
                return;
            }
            let previous = self.main_path.index;
            ui.add(
                egui::Slider::new(&mut self.main_path.index, 0..=path_count - 1)
                    .text(ow_text(catalog, Key::RomOverworldRouteLink)),
            );
            if self.main_path.index != previous {
                self.load_main_path_link();
            }
            egui::Grid::new("playable-overworld-path-link-form")
                .striped(true)
                .show(ui, |ui| {
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteSourceX),
                        &mut self.main_path.source_x,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteSourceY),
                        &mut self.main_path.source_y,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteSourceSubmap),
                        &mut self.main_path.source_submap,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteDestinationX),
                        &mut self.main_path.destination_x,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteDestinationY),
                        &mut self.main_path.destination_y,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteDestinationSubmap),
                        &mut self.main_path.destination_submap,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteTargetX),
                        &mut self.main_path.target_x,
                    );
                    path_form_row(
                        ui,
                        &ow_text(catalog, Key::RomOverworldRouteTargetY),
                        &mut self.main_path.target_y,
                    );
                });
            let previous_direction = self.main_path.direction;
            ui.horizontal(|ui| {
                ui.label(ow_text(catalog, Key::RomOverworldRouteDirection));
                egui::ComboBox::from_id_salt("playable-overworld-path-direction")
                    .selected_text(match self.main_path.direction {
                        OverworldPathDirection::Up => ow_text(catalog, Key::PathEditorDirectionUp),
                        OverworldPathDirection::Down => {
                            ow_text(catalog, Key::PathEditorDirectionDown)
                        }
                        OverworldPathDirection::Left => {
                            ow_text(catalog, Key::PathEditorDirectionLeft)
                        }
                        OverworldPathDirection::Right => {
                            ow_text(catalog, Key::PathEditorDirectionRight)
                        }
                    })
                    .show_ui(ui, |ui| {
                        for (direction, label) in [
                            (OverworldPathDirection::Up, Key::PathEditorDirectionUp),
                            (OverworldPathDirection::Down, Key::PathEditorDirectionDown),
                            (OverworldPathDirection::Left, Key::PathEditorDirectionLeft),
                            (OverworldPathDirection::Right, Key::PathEditorDirectionRight),
                        ] {
                            ui.selectable_value(
                                &mut self.main_path.direction,
                                direction,
                                ow_text(catalog, label),
                            );
                        }
                    });
                ui.checkbox(
                    &mut self.main_path.one_way,
                    ow_text(catalog, Key::RomOverworldRouteOneWay),
                );
            });
            if self.main_path.direction != previous_direction
                && let Err(error) = self.main_path.reorient_from(previous_direction)
            {
                self.main_path.direction = previous_direction;
                self.error = Some(error);
            }
            ui.small(ow_text(catalog, Key::RomOverworldRouteOrderNotice));
            ui.horizontal(|ui| {
                if ui
                    .button(ow_text(catalog, Key::RomOverworldRouteReload))
                    .clicked()
                {
                    self.load_main_path_link();
                }
                if ui
                    .add_enabled(
                        !stale && !terrain_modified,
                        egui::Button::new(ow_text(catalog, Key::RomOverworldRouteApply)),
                    )
                    .clicked()
                    && let Err(error) = self.apply_main_path_link()
                {
                    self.error = Some(error);
                }
                if ui
                    .add_enabled(
                        paths_modified && !stale && !terrain_modified,
                        egui::Button::new(ow_text(catalog, Key::RomOverworldRouteCommit)),
                    )
                    .clicked()
                    && let Some(workspace) = self.main_layer2_workspace.as_ref()
                {
                    command = Some(Command::ReplaceNativeOverworldPathLinks {
                        rev: workspace.controller.revision(),
                        table: Box::new(workspace.paths.clone()),
                    });
                }
            });
            if terrain_modified {
                ui.small(ow_text(catalog, Key::RomOverworldTerrainBlocksRoute));
            } else if paths_modified {
                ui.small(ow_text(catalog, Key::RomOverworldRouteStaged));
            }
        });
        command
    }

    fn load_main_path_link(&mut self) {
        let Some(link) = self
            .main_layer2_workspace
            .as_ref()
            .and_then(|workspace| workspace.paths.links.get(self.main_path.index))
            .copied()
        else {
            self.main_path.loaded = None;
            return;
        };
        self.main_path.set(link);
    }

    fn apply_main_path_link(&mut self) -> Result<(), String> {
        if self.main_path.loaded != Some(self.main_path.index) {
            return Err("reload the selected route link before applying it".into());
        }
        let link = self.main_path.parse()?;
        let workspace = self
            .main_layer2_workspace
            .as_mut()
            .ok_or("playable overworld workspace is closed")?;
        let mut staged = workspace.paths.clone();
        staged.links[self.main_path.index] = link;
        staged.encode_planes().map_err(|error| error.to_string())?;
        workspace.paths = staged;
        Ok(())
    }

    fn main_layer2_tile_controls(
        &mut self,
        ui: &mut egui::Ui,
        shape: CompleteOverworldShape,
        stale: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let old_selection = (self.x, self.y);
        ui.label(ow_text(catalog, Key::RomOverworldLayer2Tilemap));
        ui.add(egui::Slider::new(&mut self.x, 0..=shape.width.saturating_sub(1)).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0..=shape.height.saturating_sub(1)).text("Y"));
        if old_selection != (self.x, self.y) {
            self.paint_anchor = None;
            self.loaded = None;
            self.load_main_layer2_tile();
        }
        ui.horizontal(|ui| {
            ui.label(ow_text(catalog, Key::RomOverworldTileWord));
            ui.text_edit_singleline(&mut self.tile);
        });
        self.direct_tile_picker(ui);
        if ui
            .add_enabled(
                !stale,
                egui::Button::new(ow_text(catalog, Key::RomOverworldApplyLayerTile)),
            )
            .clicked()
        {
            match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
                Ok(tile) => self.apply(OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: self.x,
                    y: self.y,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn direct_tile_picker(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Visual 8x8 tile picker", |ui| {
            let previous_palette = self.direct_tile_palette;
            ui.add(egui::Slider::new(&mut self.direct_tile_palette, 0..=7).text("Palette row"));
            if previous_palette != self.direct_tile_palette {
                self.direct_tile_rendered_palette = None;
            }
            if self.direct_tile_rendered_palette != Some(self.direct_tile_palette) {
                self.direct_tile_texture =
                    self.main_layer2_workspace.as_ref().and_then(|workspace| {
                        overworld_editor_render::render_layer2_graphics_texture(
                            ui.ctx(),
                            &workspace.assets.graphics,
                            &workspace.palette,
                            self.direct_tile_palette,
                        )
                        .map_err(|error| self.error = Some(error))
                        .ok()
                    });
                self.direct_tile_rendered_palette = Some(self.direct_tile_palette);
            }
            let Some(texture) = self.direct_tile_texture.clone() else {
                ui.label("The current overworld graphics cannot be previewed.");
                return;
            };
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click()));
            let columns = 16;
            let rows = self.main_layer2_workspace.as_ref().map_or(0, |workspace| {
                workspace
                    .assets
                    .graphics
                    .graphics
                    .tiles
                    .len()
                    .div_ceil(columns)
            });
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
                && let Some((x, y)) =
                    overworld_editor_render::selected_tile(response.rect, position, columns, rows)
                && let Some(index) = y.checked_mul(columns).and_then(|base| base.checked_add(x))
                && index
                    < self.main_layer2_workspace.as_ref().map_or(0, |workspace| {
                        workspace.assets.graphics.graphics.tiles.len()
                    })
                && let Ok(word) = level_editor_forms::parse_hex_u16(&self.tile, "SNES tilemap word")
                && let Ok(tile_number) = u16::try_from(index)
            {
                let word = (word & !0x1fff) | tile_number | (self.direct_tile_palette as u16) << 10;
                self.tile = format!("{word:04X}");
            }
        });
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        if let Some(command) = self.take_editor_transition_after_save(revision) {
            return (false, Some(command));
        }
        self.poll_transfer_file_io(context, revision);
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, revision);
        }
        self.open_dialog(context, catalog);
        let mut command = match self.manifest_loader.show(context, revision) {
            Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if self.workspace.is_some() {
            self.update_animation_preview_clock(context);
            self.load_tile();
            self.refresh_texture(context);
            self.refresh_map16_texture(context);
            egui::Window::new(ow_text(catalog, Key::RomOverworldCompleteTitle))
                .default_size([820.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision, catalog) {
                        command = Some(ui_command);
                    }
                });
        }
        if self.main_layer2_workspace.is_some() {
            self.load_main_layer2_tile();
            self.refresh_main_layer2_texture(context);
            self.refresh_map16_texture(context);
            egui::Window::new(ow_text(catalog, Key::RomOverworldPlayableTitle))
                .default_size([820.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.main_layer2_contents(ui, revision, catalog) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context, catalog);
        if command.is_none() {
            command = self.show_editor_transition_confirmation(context, revision);
        }
        self.show_error(context, catalog);
        (approved, command)
    }
}

fn recovery_snapshot_from_overworld_command(
    app: &AppState,
    command: Command,
    level: Option<u16>,
) -> Result<Option<lm_app::RecoverySnapshot>, String> {
    let mutation = overworld_mutation_from_command(app, command)?;
    app.recovery_snapshot_with_mutation(&mutation, level)
        .map_err(|error| error.to_string())
}

fn overworld_mutation_from_command(
    app: &AppState,
    command: Command,
) -> Result<lm_project::RomMutation, String> {
    let Command::CommitRomMutation {
        expected_revision,
        mutation,
        ..
    } = command
    else {
        return Err("overworld recovery expected one prepared ROM mutation".into());
    };
    if expected_revision != app.project_revision() {
        return Err("overworld recovery mutation was prepared from a stale revision".into());
    }
    Ok(mutation)
}

impl MainPathLinkForm {
    fn set(&mut self, link: OverworldPathLink) {
        self.source_x = format!("{:04X}", link.source.x);
        self.source_y = format!("{:04X}", link.source.y);
        self.source_submap = format!("{:02X}", link.source.submap);
        self.destination_x = format!("{:04X}", link.destination.x);
        self.destination_y = format!("{:04X}", link.destination.y);
        self.destination_submap = format!("{:02X}", link.destination.submap);
        self.target_x = format!("{:02X}", link.target.x_tile);
        self.target_y = format!("{:02X}", link.target.y_tile);
        self.one_way = link.destination
            == (OverworldEndpoint {
                x: 0xffff,
                y: 0xffff,
                submap: 0xff,
            });
        self.loaded = Some(self.index);
    }

    fn parse(&self) -> Result<OverworldPathLink, String> {
        Ok(OverworldPathLink {
            source: OverworldEndpoint {
                x: level_editor_forms::parse_hex_u16(&self.source_x, "route source X")?,
                y: level_editor_forms::parse_hex_u16(&self.source_y, "route source Y")?,
                submap: level_editor_forms::parse_hex_u8(
                    &self.source_submap,
                    "route source submap",
                )?,
            },
            destination: if self.one_way {
                OverworldEndpoint {
                    x: 0xffff,
                    y: 0xffff,
                    submap: 0xff,
                }
            } else {
                OverworldEndpoint {
                    x: level_editor_forms::parse_hex_u16(
                        &self.destination_x,
                        "route destination X",
                    )?,
                    y: level_editor_forms::parse_hex_u16(
                        &self.destination_y,
                        "route destination Y",
                    )?,
                    submap: level_editor_forms::parse_hex_u8(
                        &self.destination_submap,
                        "route destination submap",
                    )?,
                }
            },
            target: OverworldPathTarget {
                x_tile: level_editor_forms::parse_hex_u8(&self.target_x, "route target X")?,
                y_tile: level_editor_forms::parse_hex_u8(&self.target_y, "route target Y")?,
            },
        })
    }

    fn reorient_from(&mut self, previous: OverworldPathDirection) -> Result<(), String> {
        let source = OverworldEndpoint {
            x: level_editor_forms::parse_hex_u16(&self.source_x, "route source X")?,
            y: level_editor_forms::parse_hex_u16(&self.source_y, "route source Y")?,
            submap: level_editor_forms::parse_hex_u8(&self.source_submap, "route source submap")?,
        };
        let destination = if self.one_way {
            None
        } else {
            Some(OverworldEndpoint {
                x: level_editor_forms::parse_hex_u16(&self.destination_x, "route destination X")?,
                y: level_editor_forms::parse_hex_u16(&self.destination_y, "route destination Y")?,
                submap: level_editor_forms::parse_hex_u8(
                    &self.destination_submap,
                    "route destination submap",
                )?,
            })
        };
        let source = previous.reorient_directional_point(source, self.direction);
        self.source_x = format!("{:04X}", source.x);
        self.source_y = format!("{:04X}", source.y);
        self.source_submap = format!("{:02X}", source.submap);
        if let Some(destination) = destination {
            let destination = previous.reorient_directional_point(destination, self.direction);
            self.destination_x = format!("{:04X}", destination.x);
            self.destination_y = format!("{:04X}", destination.y);
            self.destination_submap = format!("{:02X}", destination.submap);
        }
        Ok(())
    }
}

fn path_form_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

fn parse_native_sprite_form(
    form: &NativeSpriteForm,
) -> Result<lm_overworld::NativeCustomOverworldSprite, String> {
    let extra = form
        .extra
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|value| !value.is_empty())
        .enumerate()
        .map(|(index, value)| {
            level_editor_forms::parse_hex_u8(value, &format!("extension byte {index}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(lm_overworld::NativeCustomOverworldSprite {
        id: level_editor_forms::parse_hex_u8(&form.id, "native sprite ID")?,
        x: level_editor_forms::parse_hex_u16(&form.x, "native sprite X")?,
        y: level_editor_forms::parse_hex_u16(&form.y, "native sprite Y")?,
        screen: level_editor_forms::parse_hex_u8(&form.screen, "native sprite screen")?,
        extra,
    })
}

impl RomOverworldEditor {
    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let (
            stale,
            shape,
            slot,
            controller_revision,
            data,
            modes,
            ownership,
            animation_ownership,
            global_animation,
            native_summary,
        ) = {
            let workspace = self.workspace.as_ref()?;
            let map = usize::from(workspace.slot).min(6);
            (
                workspace.controller.revision() != revision,
                workspace.profiled.profile.overworld_shape,
                workspace.slot,
                workspace.controller.revision(),
                workspace.controller.data().clone(),
                workspace.profiled.profile.exanimation_double_size_modes,
                workspace.ownership.clone(),
                overworld_editor_render::overworld_animation_ownership(
                    &workspace.controller.data().animation,
                    workspace.assets.global_animation.as_ref(),
                    workspace.assets.animation_options[map],
                    workspace.assets.graphics.graphics.tiles.len(),
                    workspace.controller.data().palette.colors.len(),
                ),
                workspace.assets.global_animation.clone(),
                workspace.native_appearances.as_ref().map(|pair| {
                    (
                        pair.definitions.appearances.len(),
                        pair.definitions.tooltips.len(),
                        pair.sprite_map16.loaded_len(),
                    )
                }),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let transfer_busy = self.transfer_busy();
        let mutation_blocked = stale || transfer_busy;
        let editing_blocked = mutation_blocked || self.native_sprite_property_dialog.is_some();
        if transfer_busy {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Complete-overworld file transfer is active; editing is temporarily disabled.",
            );
        }
        self.complete_file_controls(ui, stale, revision, catalog);
        if let Some((appearances, tooltips, map16_bytes)) = native_summary {
            ui.label(format!(
                "ROM-adjacent native sprite display: {appearances} appearances, {tooltips} tooltips, {map16_bytes} Sprite Map16 bytes"
            ));
        }
        self.world_canvas(ui, shape, editing_blocked);
        self.layer_tile_controls(ui, shape, editing_blocked);
        ui.separator();
        let previous_panel = self.panel;
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.panel,
                Panel::Records,
                ow_text(catalog, Key::RomOverworldTabRecords),
            );
            ui.selectable_value(
                &mut self.panel,
                Panel::Palette,
                ow_text(catalog, Key::RomOverworldTabPalette),
            );
            ui.selectable_value(
                &mut self.panel,
                Panel::Animation,
                ow_text(catalog, Key::RomOverworldTabAnimation),
            );
            ui.selectable_value(
                &mut self.panel,
                Panel::NativeSprites,
                ow_text(catalog, Key::RomOverworldTabNativeSprites),
            );
        });
        if previous_panel != self.panel && self.panel != Panel::NativeSprites {
            if self.paint_tool == MapPaintTool::NativeSprite {
                self.paint_tool = MapPaintTool::Select;
            }
        }
        let file = CompleteOverworldFile {
            source_slot: slot,
            shape,
            data,
        };
        let mut runtime_command = None;
        let edit = match self.panel {
            Panel::Records => self.records.show(ui, &file, controller_revision),
            Panel::Palette => {
                let edit = self.palette.show(
                    ui,
                    &file.data.palette,
                    &ownership,
                    &animation_ownership.palette,
                );
                if let Some(owner) = self.palette.take_navigation() {
                    self.panel = Panel::Animation;
                    self.animation.navigate(owner);
                }
                edit
            }
            Panel::Animation => {
                self.animation_preview_controls(ui, &file.data.animation, catalog);
                self.animation_file_controls(ui, stale, revision, catalog);
                runtime_command = self.animation_option_controls(ui, editing_blocked, catalog);
                self.animation_destination_controls(ui, &animation_ownership.graphics);
                self.animation.show(
                    ui,
                    &file.data.animation,
                    global_animation.as_ref(),
                    &modes,
                    controller_revision,
                    catalog,
                )
            }
            Panel::NativeSprites => {
                self.native_sprite_controls(ui, editing_blocked, catalog);
                None
            }
        };
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !editing_blocked => self.apply(edit),
                Ok(_) if transfer_busy => {
                    self.error = Some("overworld editing is disabled during file transfer".into());
                }
                Ok(_) => self.error = Some("stale overworld workspace cannot accept edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        if runtime_command.is_some() {
            return runtime_command;
        }
        self.show_native_sprite_property_dialog(ui.ctx(), mutation_blocked, catalog);
        self.commit_controls(ui, editing_blocked, revision)
    }

    fn native_sprite_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let counts = self.workspace.as_ref().map(|workspace| {
            std::array::from_fn::<_, 7, _>(|map| workspace.native_sprites.table().maps[map].len())
        });
        let Some(counts) = counts else { return };
        ui.heading(ow_text(catalog, Key::RomOverworldSpriteTitle));
        ui.small(ow_text(catalog, Key::RomOverworldSpriteNotice));
        ui.small(ow_text(catalog, Key::RomOverworldSpriteCanvasNotice));
        let previous_map = self.native_sprite.map;
        ui.add(
            egui::Slider::new(&mut self.native_sprite.map, 0..=6)
                .text(ow_text(catalog, Key::RomOverworldSpriteMap)),
        );
        if self.native_sprite.map != previous_map {
            self.native_sprite_selection.clear();
            self.native_sprite_drag = None;
            self.native_sprite_marquee = None;
        }
        let count = counts[self.native_sprite.map];
        ui.add(
            egui::Slider::new(&mut self.native_sprite.index, 0..=count)
                .text(ow_text(catalog, Key::RomOverworldSpriteIndex)),
        );
        egui::Grid::new("native-custom-overworld-sprite-form")
            .striped(true)
            .show(ui, |ui| {
                path_form_row(
                    ui,
                    &ow_text(catalog, Key::RomOverworldSpriteId),
                    &mut self.native_sprite.id,
                );
                path_form_row(
                    ui,
                    &ow_text(catalog, Key::RomOverworldSpriteX),
                    &mut self.native_sprite.x,
                );
                path_form_row(
                    ui,
                    &ow_text(catalog, Key::RomOverworldSpriteY),
                    &mut self.native_sprite.y,
                );
                path_form_row(
                    ui,
                    &ow_text(catalog, Key::RomOverworldSpriteScreen),
                    &mut self.native_sprite.screen,
                );
                path_form_row(
                    ui,
                    &ow_text(catalog, Key::RomOverworldSpriteExtension),
                    &mut self.native_sprite.extra,
                );
            });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.native_sprite.index < count,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteLoad)),
                )
                .clicked()
            {
                self.load_native_sprite_form();
            }
            if ui
                .button(ow_text(catalog, Key::RomOverworldSpriteUseCanvas))
                .clicked()
            {
                match native_sprite_canvas_position(self.native_sprite.map, self.x, self.y) {
                    Some((x, y)) => {
                        self.native_sprite.x = format!("{x:04X}");
                        self.native_sprite.y = format!("{y:04X}");
                    }
                    None => {
                        self.error =
                            Some("the selected canvas cell is outside this map's plane".into())
                    }
                }
            }
            if ui
                .add_enabled(
                    !blocked,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpritePlace)),
                )
                .clicked()
                && let Err(error) = self.place_native_sprite_at_canvas((self.x, self.y))
            {
                self.error = Some(error);
            }
        });
        if let Ok(id) = level_editor_forms::parse_hex_u8(&self.native_sprite.id, "native sprite ID")
            && let Some(required) = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.native_sprites.required_extra_len(id))
        {
            ui.horizontal(|ui| {
                ui.label(
                    ow_text(catalog, Key::RomOverworldSpriteRequiredFormat)
                        .replace("{id}", &format!("{id:02X}"))
                        .replace("{count}", &required.to_string()),
                );
                if ui
                    .button(ow_text(catalog, Key::RomOverworldSpriteFillExtension))
                    .clicked()
                {
                    self.native_sprite.extra = std::iter::repeat_n("00", required)
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            });
        }
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !blocked,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteInsert)),
                )
                .clicked()
            {
                edit = Some(
                    self.parse_native_sprite()
                        .map(|sprite| NativeCustomOverworldSpriteEdit::Insert {
                            map: self.native_sprite.map,
                            index: self.native_sprite.index,
                            sprite,
                        })
                        .map_err(|error| error.to_string()),
                );
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteReplace)),
                )
                .clicked()
            {
                edit = Some(
                    self.parse_native_sprite()
                        .map(|sprite| NativeCustomOverworldSpriteEdit::Replace {
                            map: self.native_sprite.map,
                            index: self.native_sprite.index,
                            sprite,
                        })
                        .map_err(|error| error.to_string()),
                );
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteDelete)),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::Remove {
                    map: self.native_sprite.map,
                    index: self.native_sprite.index,
                }));
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index > 0 && self.native_sprite.index < count,
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteMoveUp)),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::MoveBefore {
                    map: self.native_sprite.map,
                    from: self.native_sprite.index,
                    before: self.native_sprite.index - 1,
                }));
            }
            if ui
                .add_enabled(
                    !blocked && self.native_sprite.index < count.saturating_sub(1),
                    egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteMoveDown)),
                )
                .clicked()
            {
                edit = Some(Ok(NativeCustomOverworldSpriteEdit::MoveBefore {
                    map: self.native_sprite.map,
                    from: self.native_sprite.index,
                    before: self.native_sprite.index + 2,
                }));
            }
        });
        if let Some(edit) = edit {
            match edit.and_then(|edit| {
                self.workspace
                    .as_mut()
                    .ok_or_else(|| String::from("workspace is closed"))?
                    .native_sprites
                    .apply_edits(&[edit])
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => {
                    self.native_sprite_selection.clear();
                    self.native_sprite_drag = None;
                    self.native_sprite_marquee = None;
                    self.rendered_key = None;
                    self.texture = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.label(
            ow_text(catalog, Key::RomOverworldSpriteCountFormat)
                .replace("{map}", &self.native_sprite.map.to_string())
                .replace("{count}", &count.to_string())
                .replace(
                    "{selected}",
                    &self.native_sprite_selection.len().to_string(),
                ),
        );
    }

    fn load_native_sprite_form(&mut self) {
        let Some(sprite) = self.workspace.as_ref().and_then(|workspace| {
            workspace
                .native_sprites
                .table()
                .maps
                .get(self.native_sprite.map)?
                .get(self.native_sprite.index)
        }) else {
            return;
        };
        self.native_sprite.id = format!("{:02X}", sprite.id);
        self.native_sprite.x = format!("{:04X}", sprite.x);
        self.native_sprite.y = format!("{:04X}", sprite.y);
        self.native_sprite.screen = format!("{:02X}", sprite.screen);
        self.native_sprite.extra = sprite
            .extra
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
    }

    fn parse_native_sprite(&self) -> Result<lm_overworld::NativeCustomOverworldSprite, String> {
        parse_native_sprite_form(&self.native_sprite)
    }

    fn show_native_sprite_property_dialog(
        &mut self,
        context: &egui::Context,
        blocked: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let Some(mut dialog) = self.native_sprite_property_dialog.take() else {
            return;
        };
        let mut open = true;
        let mut accepted = false;
        let mut cancelled = false;
        egui::Window::new(ow_text(catalog, Key::RomOverworldSpritePropertiesTitle))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(
                    ow_text(catalog, Key::RomOverworldSpriteRecordFormat)
                        .replace("{map}", &dialog.form.map.to_string())
                        .replace("{record}", &dialog.form.index.to_string()),
                );
                egui::Grid::new("native-custom-overworld-sprite-property-dialog")
                    .striped(true)
                    .show(ui, |ui| {
                        path_form_row(
                            ui,
                            &ow_text(catalog, Key::RomOverworldSpriteId),
                            &mut dialog.form.id,
                        );
                        path_form_row(
                            ui,
                            &ow_text(catalog, Key::RomOverworldSpriteX),
                            &mut dialog.form.x,
                        );
                        path_form_row(
                            ui,
                            &ow_text(catalog, Key::RomOverworldSpriteY),
                            &mut dialog.form.y,
                        );
                        path_form_row(
                            ui,
                            &ow_text(catalog, Key::RomOverworldSpriteScreen),
                            &mut dialog.form.screen,
                        );
                        path_form_row(
                            ui,
                            &ow_text(catalog, Key::RomOverworldSpriteExtension),
                            &mut dialog.form.extra,
                        );
                    });
                ui.horizontal(|ui| {
                    accepted = ui
                        .add_enabled(
                            !blocked,
                            egui::Button::new(ow_text(catalog, Key::RomOverworldSpriteApply)),
                        )
                        .clicked();
                    cancelled = ui
                        .button(ow_text(catalog, Key::RomOverworldCancel))
                        .clicked();
                });
            });
        if accepted {
            let result = parse_native_sprite_form(&dialog.form).and_then(|sprite| {
                self.workspace
                    .as_mut()
                    .ok_or_else(|| String::from("workspace is closed"))?
                    .native_sprites
                    .apply_edits(&[NativeCustomOverworldSpriteEdit::Replace {
                        map: dialog.form.map,
                        index: dialog.form.index,
                        sprite,
                    }])
                    .map_err(|error| error.to_string())
            });
            match result {
                Ok(()) => {
                    self.native_sprite = dialog.form;
                    self.rendered_key = None;
                    self.texture = None;
                }
                Err(error) => {
                    self.error = Some(error);
                    self.native_sprite_property_dialog = Some(dialog);
                }
            }
        } else if open && !cancelled {
            self.native_sprite_property_dialog = Some(dialog);
        }
    }

    fn layer_tile_controls(
        &mut self,
        ui: &mut egui::Ui,
        shape: CompleteOverworldShape,
        stale: bool,
    ) {
        let old_selection = (self.layer, self.x, self.y);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.layer, 0, "Layer 1");
            ui.selectable_value(&mut self.layer, 1, "Layer 2");
        });
        ui.add(egui::Slider::new(&mut self.x, 0..=shape.width.saturating_sub(1)).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0..=shape.height.saturating_sub(1)).text("Y"));
        if old_selection != (self.layer, self.x, self.y) {
            self.paint_anchor = None;
            self.loaded = None;
            self.load_tile();
        }
        ui.horizontal(|ui| {
            ui.label("Map16 tile");
            ui.text_edit_singleline(&mut self.tile);
        });
        self.map16_picker(ui);
        if ui
            .add_enabled(!stale, egui::Button::new("Apply layer tile"))
            .clicked()
        {
            match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
                Ok(tile) => self.apply(OverworldControllerEdit::SetLayerTile {
                    layer: self.layer_id(),
                    x: self.x,
                    y: self.y,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn animation_destination_controls(
        &mut self,
        ui: &mut egui::Ui,
        owners: &[Option<overworld_editor_render::OverworldAnimationOwner>],
    ) {
        ui.collapsing("Rendered graphics destinations", |ui| {
            ui.small(
                "Ctrl+Shift+click an attributed 8x8 tile to select its last-writing local or global ExAnimation record.",
            );
            let Some(texture) = self.animation_graphics_texture.clone() else {
                ui.label("The current animated graphics cache could not be rendered.");
                return;
            };
            let columns = 16;
            let rows = owners.len().div_ceil(columns);
            let native = texture.size_vec2();
            let width = (native.x * 2.0).min(ui.available_width()).max(native.x);
            let response = ui.add(
                egui::Image::new(&texture)
                    .fit_to_exact_size(egui::vec2(width, width * native.y / native.x))
                    .sense(egui::Sense::click()),
            );
            let pointed = response
                .hover_pos()
                .and_then(|position| {
                    overworld_editor_render::selected_tile(
                        response.rect,
                        position,
                        columns,
                        rows,
                    )
                })
                .and_then(|(x, y)| y.checked_mul(columns).and_then(|base| base.checked_add(x)));
            if let Some(index) = pointed {
                response.clone().on_hover_text(match owners.get(index).copied().flatten() {
                    Some(owner) => format!(
                        "Tile {index:03X}: {:?} ExAnimation record {:02X}",
                        owner.domain, owner.record
                    ),
                    None => format!("Tile {index:03X}: no ExAnimation owner"),
                });
                if response.clicked()
                    && let Some(owner) =
                        overworld_editor_render::ctrl_shift_animation_navigation(
                            ui.input(|input| input.modifiers),
                            owners.get(index).copied().flatten(),
                        )
                {
                    self.animation.navigate(owner);
                }
            }
        });
    }

    fn map16_picker(&mut self, ui: &mut egui::Ui) {
        let page_count = self
            .assets()
            .map_or(0, |assets| assets.map16.set.pages.len());
        ui.collapsing("Visual Map16 tile picker", |ui| {
            let previous_page = self.map16_page;
            ui.add(
                egui::Slider::new(&mut self.map16_page, 0..=page_count.saturating_sub(1))
                    .text("Map16 page"),
            );
            if previous_page != self.map16_page {
                self.map16_rendered_key = None;
                self.refresh_map16_texture(ui.ctx());
            }
            let Some(texture) = self.map16_texture.clone() else {
                ui.label("This Map16 page cannot be previewed with the current overworld assets.");
                return;
            };
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click()));
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
                && let Some(index) =
                    crate::map16_editor_render::selected_tile(response.rect, position)
                && let Some(tile) = self
                    .map16_page
                    .checked_mul(lm_level::Map16Page::TILE_COUNT)
                    .and_then(|base| base.checked_add(index))
                    .and_then(|tile| u16::try_from(tile).ok())
            {
                self.tile = format!("{tile:04X}");
            }
            if let Ok(tile) = level_editor_forms::parse_hex_u16(&self.tile, "overworld tile")
                && usize::from(tile) / lm_level::Map16Page::TILE_COUNT == self.map16_page
            {
                let index = usize::from(tile) % lm_level::Map16Page::TILE_COUNT;
                let cell = response.rect.width() / 16.0;
                let column = f32::from(u8::try_from(index % 16).unwrap_or_default());
                let row = f32::from(u8::try_from(index / 16).unwrap_or_default());
                let minimum = response.rect.min + egui::vec2(column * cell, row * cell);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell)),
                    0.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
        });
    }

    fn world_canvas(&mut self, ui: &mut egui::Ui, shape: CompleteOverworldShape, stale: bool) {
        let reveal_count = self.workspace.as_ref().map_or(0, |workspace| {
            workspace.controller.data().event_reveals.entries.len()
        });
        if ui
            .add(
                egui::Slider::new(&mut self.completed_reveals, 0..=reveal_count)
                    .text("Completed event reveals"),
            )
            .changed()
        {
            self.rendered_key = None;
        }
        let Some(texture) = self.texture.clone() else {
            ui.label("Overworld preview unavailable; property editing remains available.");
            return;
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Select, "Select");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Brush, "Brush");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Rectangle, "Rectangle");
            ui.selectable_value(&mut self.paint_tool, MapPaintTool::Fill, "Fill");
            if self.workspace.is_some() && self.panel == Panel::NativeSprites {
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::NativeSprite,
                    "Place/move native sprite",
                );
            }
            if self.main_layer2_workspace.is_some() {
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::RouteSource,
                    "Set route source",
                );
                ui.selectable_value(
                    &mut self.paint_tool,
                    MapPaintTool::RouteDestination,
                    "Set route destination",
                );
            }
        });
        if self.paint_tool != MapPaintTool::NativeSprite {
            self.native_sprite_drag = None;
            self.native_sprite_marquee = None;
        }
        let mut action = None;
        let mut native_sprite_group_destination = None;
        let mut delete_native_sprite_selection = false;
        egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click_and_drag()));
            let primary_started = response.drag_started_by(egui::PointerButton::Primary);
            let primary_stopped = response.drag_stopped_by(egui::PointerButton::Primary);
            let secondary_started = response.drag_started_by(egui::PointerButton::Secondary);
            let secondary_stopped = response.drag_stopped_by(egui::PointerButton::Secondary);
            if primary_started || secondary_started {
                response.request_focus();
            }
            if (response.clicked()
                || response.secondary_clicked()
                || response.dragged()
                || primary_started
                || primary_stopped
                || secondary_started
                || secondary_stopped)
                && let Some(position) = response.interact_pointer_pos()
                && let Some((x, y)) = overworld_editor_render::selected_tile(
                    response.rect,
                    position,
                    shape.width,
                    shape.height,
                )
            {
                let canvas_pixel = overworld_editor_render::selected_tile(
                    response.rect,
                    position,
                    shape.width.saturating_mul(8),
                    shape.height.saturating_mul(8),
                )
                .unwrap_or((x.saturating_mul(8), y.saturating_mul(8)));
                self.x = x;
                self.y = y;
                self.loaded = None;
                match self.paint_tool {
                    MapPaintTool::Select => {
                        self.load_tile();
                        self.refresh_map16_texture(ui.ctx());
                    }
                    MapPaintTool::Brush if !stale => action = Some((MapPaintTool::Brush, (x, y))),
                    MapPaintTool::Fill if !stale && response.clicked() => {
                        action = Some((MapPaintTool::Fill, (x, y)));
                    }
                    MapPaintTool::NativeSprite if !stale => {
                        let modifiers = ui.input(|input| input.modifiers);
                        let toggle = modifiers.ctrl || modifiers.command;
                        if secondary_started {
                            self.native_sprite_marquee = None;
                            let secondary_action = native_sprite_secondary_action(
                                modifiers.alt,
                                self.native_sprite_hit_test(canvas_pixel),
                                !self.native_sprite_selection.is_empty(),
                            );
                            match secondary_action {
                                Some(NativeSpriteSecondaryAction::EditProperties(index)) => {
                                    self.native_sprite_drag = None;
                                    self.native_sprite.index = index;
                                    self.load_native_sprite_form();
                                    self.native_sprite_property_dialog =
                                        Some(NativeSpritePropertyDialog {
                                            form: self.native_sprite.clone(),
                                        });
                                }
                                Some(NativeSpriteSecondaryAction::DuplicateSelection) => {
                                    self.native_sprite_drag = Some(NativeSpriteDrag {
                                        map: self.native_sprite.map,
                                        anchor: canvas_pixel,
                                        selected: self
                                            .native_sprite_selection
                                            .iter()
                                            .copied()
                                            .collect(),
                                        kind: NativeSpriteDragKind::Duplicate,
                                    });
                                }
                                None => self.native_sprite_drag = None,
                            }
                        } else if primary_started {
                            let anchor = ui
                                .input(|input| input.pointer.press_origin())
                                .and_then(|origin| {
                                    overworld_editor_render::selected_tile(
                                        response.rect,
                                        origin,
                                        shape.width.saturating_mul(8),
                                        shape.height.saturating_mul(8),
                                    )
                                })
                                .unwrap_or(canvas_pixel);
                            if let Some(index) = self.native_sprite_hit_test(canvas_pixel) {
                                self.native_sprite.index = index;
                                self.load_native_sprite_form();
                                if toggle {
                                    toggle_native_sprite_selection(
                                        &mut self.native_sprite_selection,
                                        index,
                                    );
                                    self.native_sprite_drag = None;
                                } else {
                                    if !self.native_sprite_selection.contains(&index) {
                                        self.native_sprite_selection.clear();
                                        self.native_sprite_selection.insert(index);
                                    }
                                    self.native_sprite_drag = Some(NativeSpriteDrag {
                                        map: self.native_sprite.map,
                                        anchor,
                                        selected: self
                                            .native_sprite_selection
                                            .iter()
                                            .copied()
                                            .collect(),
                                        kind: NativeSpriteDragKind::Move,
                                    });
                                }
                                self.native_sprite_marquee = None;
                            } else {
                                self.native_sprite_drag = None;
                                let baseline = if toggle {
                                    self.native_sprite_selection.clone()
                                } else {
                                    BTreeSet::default()
                                };
                                if !toggle {
                                    self.native_sprite_selection.clear();
                                }
                                self.native_sprite_marquee = Some(NativeSpriteMarquee {
                                    map: self.native_sprite.map,
                                    anchor,
                                    baseline,
                                    current: canvas_pixel,
                                });
                            }
                        } else if response.dragged() {
                            if let Some(marquee) = self.native_sprite_marquee.as_mut() {
                                marquee.current = canvas_pixel;
                                let rect = inclusive_canvas_rect(marquee.anchor, marquee.current);
                                let mut selection = marquee.baseline.clone();
                                if let Some(workspace) = self.workspace.as_ref() {
                                    selection.extend(
                                        overworld_editor_render::native_custom_sprite_indices_in_rect(
                                            workspace.native_appearances.as_ref(),
                                            workspace.native_sprites.table(),
                                            marquee.map,
                                            rect,
                                        ),
                                    );
                                }
                                self.native_sprite_selection = selection;
                            }
                        } else if primary_stopped || secondary_stopped {
                            if let Some(drag) = self.native_sprite_drag.take() {
                                self.native_sprite.map = drag.map;
                                native_sprite_group_destination = Some((drag, canvas_pixel));
                            }
                            self.native_sprite_marquee = None;
                        } else if response.clicked() {
                            if let Some(index) = self.native_sprite_hit_test(canvas_pixel) {
                                self.native_sprite.index = index;
                                self.load_native_sprite_form();
                                if toggle {
                                    toggle_native_sprite_selection(
                                        &mut self.native_sprite_selection,
                                        index,
                                    );
                                } else {
                                    self.native_sprite_selection.clear();
                                    self.native_sprite_selection.insert(index);
                                }
                            } else if !toggle {
                                self.native_sprite_selection.clear();
                            }
                        }
                    }
                    MapPaintTool::Rectangle if !stale => {
                        if response.drag_started() {
                            self.paint_anchor = ui
                                .input(|input| input.pointer.press_origin())
                                .and_then(|origin| {
                                    overworld_editor_render::selected_tile(
                                        response.rect,
                                        origin,
                                        shape.width,
                                        shape.height,
                                    )
                                })
                                .or(Some((x, y)));
                        }
                        if response.drag_stopped() || response.clicked() {
                            action = Some((MapPaintTool::Rectangle, (x, y)));
                        }
                    }
                    MapPaintTool::RouteSource if !stale && response.clicked() => {
                        let submap = level_editor_forms::parse_hex_u8(
                            &self.main_path.source_submap,
                            "route source submap",
                        );
                        if let Ok(submap) = submap
                            && let Some(endpoint) = route_directional_canvas_endpoint(
                                response.rect,
                                position,
                                submap,
                                self.main_path.direction,
                            )
                        {
                            self.main_path.source_x = format!("{:04X}", endpoint.x);
                            self.main_path.source_y = format!("{:04X}", endpoint.y);
                            self.main_path.source_submap = format!("{:02X}", endpoint.submap);
                        }
                    }
                    MapPaintTool::RouteDestination if !stale && response.clicked() => {
                        let submap = level_editor_forms::parse_hex_u8(
                            &self.main_path.destination_submap,
                            "route destination submap",
                        );
                        if let Ok(submap) = submap
                            && let Some(endpoint) = route_directional_canvas_endpoint(
                                response.rect,
                                position,
                                submap,
                                self.main_path.direction,
                            )
                        {
                            self.main_path.destination_x = format!("{:04X}", endpoint.x);
                            self.main_path.destination_y = format!("{:04X}", endpoint.y);
                            self.main_path.destination_submap = format!("{:02X}", endpoint.submap);
                            self.main_path.one_way = false;
                        }
                    }
                    _ => {}
                }
            }
            if shape.width > 0 && shape.height > 0 {
                let width = f32::from(u16::try_from(shape.width).unwrap_or(1));
                let height = f32::from(u16::try_from(shape.height).unwrap_or(1));
                let selected_x = f32::from(u16::try_from(self.x).unwrap_or_default());
                let selected_y = f32::from(u16::try_from(self.y).unwrap_or_default());
                let cell_width = response.rect.width() / width;
                let cell_height = response.rect.height() / height;
                let minimum = response.rect.min
                    + egui::vec2(selected_x * cell_width, selected_y * cell_height);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(minimum, egui::vec2(cell_width, cell_height)),
                    0.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                    egui::StrokeKind::Inside,
                );
            }
            self.paint_main_route_overlay(ui, response.rect);
            self.paint_native_sprite_selection_overlay(ui, response.rect, shape);
            if self.paint_tool == MapPaintTool::NativeSprite
                && response.has_focus()
                && !stale
            {
                let select_all = ui.input(|input| {
                    (input.modifiers.ctrl || input.modifiers.command)
                        && input.key_pressed(egui::Key::A)
                });
                if select_all {
                    let count = self.workspace.as_ref().map_or(0, |workspace| {
                        workspace.native_sprites.table().maps[self.native_sprite.map].len()
                    });
                    self.native_sprite_selection = (0..count).collect();
                }
                delete_native_sprite_selection =
                    ui.input(|input| input.key_pressed(egui::Key::Delete));
            }
        });
        if let Some((tool, position)) = action {
            match tool {
                MapPaintTool::Brush => self.paint_to(position),
                MapPaintTool::Rectangle => self.paint_rectangle_to(position),
                MapPaintTool::Fill => self.fill_at(position),
                MapPaintTool::Select
                | MapPaintTool::NativeSprite
                | MapPaintTool::RouteSource
                | MapPaintTool::RouteDestination => {}
            }
        }
        if let Some((drag, destination)) = native_sprite_group_destination
            && let Err(error) = self.finish_native_sprite_drag(&drag, destination)
        {
            self.error = Some(error);
        }
        if delete_native_sprite_selection && let Err(error) = self.delete_native_sprite_selection()
        {
            self.error = Some(error);
        }
        if self.paint_tool == MapPaintTool::Brush && !ui.input(|input| input.pointer.primary_down())
        {
            self.paint_anchor = None;
        }
    }

    fn place_native_sprite_at_canvas(&mut self, position: (usize, usize)) -> Result<(), String> {
        let (x, y) = native_sprite_canvas_position(self.native_sprite.map, position.0, position.1)
            .ok_or("the clicked canvas cell is outside the selected native sprite map")?;
        self.native_sprite.x = format!("{x:04X}");
        self.native_sprite.y = format!("{y:04X}");
        let sprite = self.parse_native_sprite()?;
        let workspace = self.workspace.as_mut().ok_or("workspace is closed")?;
        let count = workspace.native_sprites.table().maps[self.native_sprite.map].len();
        let edit = native_sprite_canvas_edit(
            self.native_sprite.map,
            self.native_sprite.index,
            count,
            sprite,
            position,
        )?;
        workspace
            .native_sprites
            .apply_edits(&[edit])
            .map_err(|error| error.to_string())?;
        self.native_sprite.index = self.native_sprite.index.min(count);
        self.rendered_key = None;
        self.texture = None;
        Ok(())
    }

    fn native_sprite_hit_test(&self, point: (usize, usize)) -> Option<usize> {
        let workspace = self.workspace.as_ref()?;
        overworld_editor_render::native_custom_sprite_hit_test(
            workspace.native_appearances.as_ref(),
            workspace.native_sprites.table(),
            self.native_sprite.map,
            point,
        )
    }

    fn finish_native_sprite_drag(
        &mut self,
        drag: &NativeSpriteDrag,
        destination: (usize, usize),
    ) -> Result<(), String> {
        let workspace = self.workspace.as_mut().ok_or("workspace is closed")?;
        let records = workspace
            .native_sprites
            .table()
            .maps
            .get(drag.map)
            .ok_or("native sprite map is out of range")?;
        let edits = match drag.kind {
            NativeSpriteDragKind::Move => native_sprite_group_move_edits(
                drag.map,
                records,
                &drag.selected,
                drag.anchor,
                destination,
            )?,
            NativeSpriteDragKind::Duplicate => {
                native_sprite_group_duplicate_edits(drag.map, records, &drag.selected, destination)?
            }
        };
        if edits.is_empty() {
            return Ok(());
        }
        workspace
            .native_sprites
            .apply_edits(&edits)
            .map_err(|error| error.to_string())?;
        if drag.kind == NativeSpriteDragKind::Duplicate {
            let count = workspace.native_sprites.table().maps[drag.map].len();
            self.native_sprite_selection =
                (count.saturating_sub(drag.selected.len())..count).collect();
        }
        if let Some(index) = self.native_sprite_selection.first().copied() {
            self.native_sprite.index = index;
            self.load_native_sprite_form();
        }
        self.rendered_key = None;
        self.texture = None;
        Ok(())
    }

    fn delete_native_sprite_selection(&mut self) -> Result<(), String> {
        if self.native_sprite_selection.is_empty() {
            return Ok(());
        }
        let map = self.native_sprite.map;
        let edits = native_sprite_selection_remove_edits(map, &self.native_sprite_selection);
        let workspace = self.workspace.as_mut().ok_or("workspace is closed")?;
        workspace
            .native_sprites
            .apply_edits(&edits)
            .map_err(|error| error.to_string())?;
        self.native_sprite_selection.clear();
        self.native_sprite.index = self
            .native_sprite
            .index
            .min(workspace.native_sprites.table().maps[map].len());
        self.native_sprite_drag = None;
        self.native_sprite_marquee = None;
        self.rendered_key = None;
        self.texture = None;
        Ok(())
    }

    fn paint_native_sprite_selection_overlay(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        shape: CompleteOverworldShape,
    ) {
        if self.paint_tool != MapPaintTool::NativeSprite || shape.width == 0 || shape.height == 0 {
            return;
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let map = self.native_sprite.map;
        let Some(records) = workspace.native_sprites.table().maps.get(map) else {
            return;
        };
        let canvas_width = shape.width.saturating_mul(8);
        let canvas_height = shape.height.saturating_mul(8);
        let to_screen = |point: (usize, usize)| {
            let point_x = f32::from(u16::try_from(point.0).unwrap_or(u16::MAX));
            let point_y = f32::from(u16::try_from(point.1).unwrap_or(u16::MAX));
            let canvas_width = f32::from(u16::try_from(canvas_width).unwrap_or(u16::MAX));
            let canvas_height = f32::from(u16::try_from(canvas_height).unwrap_or(u16::MAX));
            rect.min
                + egui::vec2(
                    point_x / canvas_width * rect.width(),
                    point_y / canvas_height * rect.height(),
                )
        };
        let plane_x = if map == 0 { 0 } else { 512 };
        for index in &self.native_sprite_selection {
            let Some(sprite) = records.get(*index) else {
                continue;
            };
            let minimum = to_screen((usize::from(sprite.x) + plane_x, usize::from(sprite.y)));
            let maximum = to_screen((
                usize::from(sprite.x) + plane_x + 8,
                usize::from(sprite.y) + 8,
            ));
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(minimum, maximum),
                0.0,
                egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(marquee) = self
            .native_sprite_marquee
            .as_ref()
            .filter(|marquee| marquee.map == map)
        {
            let selection = inclusive_canvas_rect(marquee.anchor, marquee.current);
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(
                    to_screen((selection.0, selection.1)),
                    to_screen((selection.2, selection.3)),
                ),
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn paint_main_route_overlay(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self.main_layer2_workspace.is_none() {
            return;
        }
        let Ok(link) = self.main_path.parse() else {
            return;
        };
        let point = |endpoint: OverworldEndpoint| {
            route_endpoint_canvas_pixel(endpoint).map(|(x, y)| {
                rect.min
                    + egui::vec2(
                        f32::from(x) / 1024.0 * rect.width(),
                        f32::from(y) / 512.0 * rect.height(),
                    )
            })
        };
        let source = point(link.source);
        let destination = point(link.destination);
        if let (Some(source), Some(destination)) = (source, destination) {
            ui.painter().line_segment(
                [source, destination],
                egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            );
        }
        for (position, color) in [
            (source, egui::Color32::CYAN),
            (destination, egui::Color32::MAGENTA),
        ] {
            if let Some(position) = position {
                ui.painter().circle_filled(position, 5.0, color);
                ui.painter().circle_stroke(
                    position,
                    6.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::BLACK),
                );
            }
        }
    }

    fn refresh_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let key = (
            workspace.controller.revision(),
            self.completed_reveals,
            self.animation_preview_tick
                .saturating_mul(self.animation_preview_rate.substeps_per_tick()),
        );
        if self.rendered_key == Some(key) {
            return;
        }
        let file = CompleteOverworldFile {
            source_slot: workspace.slot,
            shape: workspace.profiled.profile.overworld_shape,
            data: workspace.controller.data().clone(),
        };
        let preview = overworld_editor_render::OverworldExAnimationPreview {
            tick: self.animation_preview_tick,
            substeps_per_tick: self.animation_preview_rate.substeps_per_tick(),
            triggers: self.animation_preview_triggers.clone(),
            events_passed: self.animation_preview_events_passed.clone(),
        };
        match overworld_editor_render::render_texture_with_preview(
            context,
            &file,
            &workspace.assets,
            workspace.native_appearances.as_ref(),
            Some(workspace.native_sprites.table()),
            self.completed_reveals,
            Some(&preview),
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.animation_graphics_texture =
                    overworld_editor_render::render_exanimation_graphics_texture(
                        context,
                        &file,
                        &workspace.assets,
                        &preview,
                    )
                    .ok();
                self.rendered_key = Some(key);
            }
            Err(error) => {
                self.texture = None;
                self.animation_graphics_texture = None;
                self.rendered_key = Some(key);
                self.error = Some(format!("could not render native overworld: {error}"));
            }
        }
    }

    fn refresh_main_layer2_texture(&mut self, context: &egui::Context) {
        let Some(workspace) = self.main_layer2_workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), 0, 0);
        if self.rendered_key == Some(key) {
            return;
        }
        match overworld_editor_render::render_layer_texture(
            context,
            workspace.controller.layer(),
            &workspace.palette,
            &workspace.assets,
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.rendered_key = Some(key);
            }
            Err(error) => {
                self.texture = None;
                self.rendered_key = Some(key);
                self.error = Some(format!("could not render playable main overworld: {error}"));
            }
        }
    }

    fn refresh_map16_texture(&mut self, context: &egui::Context) {
        let page_count = self
            .assets()
            .map_or(0, |assets| assets.map16.set.pages.len());
        self.map16_page = self.map16_page.min(page_count.saturating_sub(1));
        let revision = self.workspace.as_ref().map_or_else(
            || {
                self.main_layer2_workspace
                    .as_ref()
                    .map_or(0, |workspace| workspace.controller.revision())
            },
            |workspace| workspace.controller.revision(),
        );
        let key = (revision, self.map16_page);
        if self.map16_rendered_key == Some(key) {
            return;
        }
        let Some(assets) = self.assets() else {
            return;
        };
        let Some(page) = assets.map16.set.pages.get(self.map16_page) else {
            self.map16_texture = None;
            self.map16_rendered_key = Some(key);
            return;
        };
        let page = lm_level::Map16PageFile {
            source_page: u16::try_from(self.map16_page).unwrap_or_default(),
            page: page.clone(),
        };
        let palette = if let Some(workspace) = self.workspace.as_ref() {
            lm_graphics::PaletteInterchangeFile {
                source_palette: workspace.slot,
                palette: workspace.controller.data().palette.clone(),
            }
        } else if let Some(workspace) = self.main_layer2_workspace.as_ref() {
            lm_graphics::PaletteInterchangeFile {
                source_palette: 0,
                palette: workspace.palette.clone(),
            }
        } else {
            return;
        };
        match crate::map16_editor_render::render_texture(context, &page, &assets.graphics, &palette)
        {
            Ok(texture) => {
                self.map16_texture = Some(texture);
                self.map16_rendered_key = Some(key);
            }
            Err(_) => {
                self.map16_texture = None;
                self.map16_rendered_key = Some(key);
            }
        }
    }

    fn paint_to(&mut self, position: (usize, usize)) {
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let cells = grid_line(self.paint_anchor.unwrap_or(position), position);
        self.paint_anchor = Some(position);
        let layer = self.layer_id();
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn paint_rectangle_to(&mut self, position: (usize, usize)) {
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let start = self.paint_anchor.take().unwrap_or(position);
        let cells = rectangle_cells(start, position);
        let layer = self.layer_id();
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn fill_at(&mut self, position: (usize, usize)) {
        let Some(tile) = self.brush_tile() else {
            return;
        };
        let layer = self.layer_id();
        let (width, height, source) = if let Some(workspace) = self.workspace.as_ref() {
            let shape = workspace.profiled.profile.overworld_shape;
            let source = match layer {
                OverworldLayerId::Layer1 => &workspace.controller.data().layers.layer1.tiles,
                OverworldLayerId::Layer2 => &workspace.controller.data().layers.layer2.tiles,
            };
            (shape.width, shape.height, source.as_slice())
        } else if let Some(workspace) = self.main_layer2_workspace.as_ref() {
            if layer != OverworldLayerId::Layer2 {
                return;
            }
            let layer = workspace.controller.layer();
            (layer.width, layer.height, layer.tiles.as_slice())
        } else {
            return;
        };
        let cells = flood_fill_cells(width, height, source, position);
        let edits = stroke_edits(layer, &cells, tile, |x, y| self.current_tile(layer, x, y));
        self.apply_many(&edits);
    }

    fn brush_tile(&mut self) -> Option<u16> {
        match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
            Ok(tile) => Some(tile),
            Err(error) => {
                self.error = Some(error);
                self.paint_anchor = None;
                None
            }
        }
    }

    fn current_tile(&self, layer: OverworldLayerId, x: usize, y: usize) -> Option<u16> {
        if let Some(workspace) = self.workspace.as_ref() {
            let shape = workspace.profiled.profile.overworld_shape;
            let index = y.checked_mul(shape.width)?.checked_add(x)?;
            return match layer {
                OverworldLayerId::Layer1 => {
                    workspace.controller.data().layers.layer1.tiles.get(index)
                }
                OverworldLayerId::Layer2 => {
                    workspace.controller.data().layers.layer2.tiles.get(index)
                }
            }
            .copied();
        }
        let workspace = self.main_layer2_workspace.as_ref()?;
        if layer != OverworldLayerId::Layer2 {
            return None;
        }
        workspace.controller.layer().tile(x, y).ok()
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        revision: u64,
    ) -> Option<Command> {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self.workspace.as_ref().is_some_and(|value| {
            value.controller.is_modified()
                || value.native_sprites.is_modified()
                || value.assets.animation_options != value.baseline_animation_options
        });
        let payloads_modified = self
            .workspace
            .as_ref()
            .is_some_and(|value| value.controller.is_modified());
        let transfer_busy = self.transfer_busy();
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit all staged overworld changes"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                payloads_modified && !stale && !self.manifest_loader.is_running() && !transfer_busy,
                egui::Button::new("Commit and reclaim all nine"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged overworld changes"
        } else {
            "No staged changes"
        });
        None
    }

    fn animation_option_controls(
        &mut self,
        ui: &mut egui::Ui,
        editing_blocked: bool,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let Some(workspace) = self.workspace.as_mut() else {
            return None;
        };
        ui.separator();
        ui.heading(ow_text(
            catalog,
            lm_app::ExtendedUiTextKey::OverworldAnimationOptionsHeading,
        ));
        ui.add(
            egui::Slider::new(&mut self.animation_option_map, 0..=6).text(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationMapSelector,
            )),
        );
        let installed = workspace.assets.animation_options_runtime_installed;
        let layout_supported = workspace.assets.animation_options_layout_supported;
        let mut staged = workspace.controller.is_modified()
            || workspace.assets.animation_options != workspace.baseline_animation_options;
        let option = &mut workspace.assets.animation_options[self.animation_option_map];
        let before = *option;
        ui.add_enabled_ui(!editing_blocked && layout_supported, |ui| {
            ui.add_enabled_ui(installed, |ui| {
                for (label, feature) in [
                    (
                        lm_app::ExtendedUiTextKey::OverworldAnimationOriginalPalette,
                        lm_graphics::ExAnimationFeature::PaletteAnimation,
                    ),
                    (
                        lm_app::ExtendedUiTextKey::OverworldAnimationOriginalTiles,
                        lm_graphics::ExAnimationFeature::VanillaAnimation,
                    ),
                    (
                        lm_app::ExtendedUiTextKey::OverworldAnimationGlobalFeature,
                        lm_graphics::ExAnimationFeature::GlobalExAnimation,
                    ),
                    (
                        lm_app::ExtendedUiTextKey::OverworldAnimationMapFeature,
                        lm_graphics::ExAnimationFeature::LevelExAnimation,
                    ),
                ] {
                    let mut enabled = option.features.enabled(feature);
                    if ui.checkbox(&mut enabled, ow_text(catalog, label)).changed() {
                        option.features.set_enabled(feature, enabled);
                    }
                }
            });
            ui.checkbox(
                &mut option.original_lightning,
                ow_text(
                    catalog,
                    lm_app::ExtendedUiTextKey::OverworldAnimationOriginalLightning,
                ),
            );
        });
        if !layout_supported {
            ui.small(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationOptionsUnsupported,
            ));
        } else if !installed {
            ui.small(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationRuntimeRequired,
            ));
        }
        if *option != before {
            staged = true;
            self.rendered_key = None;
            self.texture = None;
        }
        if !installed
            && layout_supported
            && ui
                .add_enabled(
                    !editing_blocked && !staged,
                    egui::Button::new(ow_text(
                        catalog,
                        lm_app::ExtendedUiTextKey::OverworldAnimationInstallRuntime,
                    )),
                )
                .on_hover_text(ow_text(
                    catalog,
                    lm_app::ExtendedUiTextKey::OverworldAnimationInstallRuntimeNotice,
                ))
                .clicked()
        {
            match self.prepare_animation_runtime_install() {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        if !installed && staged {
            ui.small(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationInstallBlocked,
            ));
        }
        None
    }

    fn apply(&mut self, edit: OverworldControllerEdit) {
        self.apply_many(&[edit]);
    }

    fn apply_many(&mut self, edits: &[OverworldControllerEdit]) {
        if edits.is_empty() {
            return;
        }
        if self.transfer_busy() {
            self.error = Some("overworld editing is disabled during file transfer".into());
            return;
        }
        let result = if let Some(workspace) = self.workspace.as_mut() {
            workspace
                .controller
                .apply_edits(edits)
                .map_err(|error| error.to_string())
        } else if let Some(workspace) = self.main_layer2_workspace.as_mut() {
            workspace
                .controller
                .apply_edits(edits)
                .map_err(|error| error.to_string())
        } else {
            self.error = Some("overworld workspace is closed".into());
            return;
        };
        if let Err(error) = result {
            self.error = Some(error);
        } else {
            self.invalidate();
        }
    }

    fn transfer_busy(&self) -> bool {
        self.transfer_loader.is_running() || self.transfer_persistence.is_running()
    }

    fn load_tile(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (workspace.controller.revision(), self.layer, self.x, self.y);
        if self.loaded == Some(key) {
            return;
        }
        let shape = workspace.profiled.profile.overworld_shape;
        self.x = self.x.min(shape.width.saturating_sub(1));
        self.y = self.y.min(shape.height.saturating_sub(1));
        let tiles = if self.layer == 0 {
            &workspace.controller.data().layers.layer1.tiles
        } else {
            &workspace.controller.data().layers.layer2.tiles
        };
        if let Some(tile) = tiles.get(self.y * shape.width + self.x) {
            self.tile = format!("{tile:04X}");
            let page = usize::from(*tile) / lm_level::Map16Page::TILE_COUNT;
            if page != self.map16_page {
                self.map16_page = page;
                self.map16_rendered_key = None;
            }
        }
        self.loaded = Some((workspace.controller.revision(), self.layer, self.x, self.y));
    }

    fn load_main_layer2_tile(&mut self) {
        let Some(workspace) = self.main_layer2_workspace.as_ref() else {
            return;
        };
        let key = (workspace.controller.revision(), 1, self.x, self.y);
        if self.loaded == Some(key) {
            return;
        }
        let layer = workspace.controller.layer();
        self.x = self.x.min(layer.width.saturating_sub(1));
        self.y = self.y.min(layer.height.saturating_sub(1));
        if let Ok(tile) = layer.tile(self.x, self.y) {
            self.tile = format!("{tile:04X}");
            self.direct_tile_palette = usize::from(lm_level::Subtile(tile).palette());
            let page = usize::from(tile) / lm_level::Map16Page::TILE_COUNT;
            if page != self.map16_page {
                self.map16_page = page;
                self.map16_rendered_key = None;
            }
        }
        self.loaded = Some((workspace.controller.revision(), 1, self.x, self.y));
    }

    fn assets(&self) -> Option<&OverworldAssets> {
        self.workspace
            .as_ref()
            .map(|workspace| &workspace.assets)
            .or_else(|| {
                self.main_layer2_workspace
                    .as_ref()
                    .map(|workspace| &workspace.assets)
            })
    }

    fn layer_id(&self) -> OverworldLayerId {
        if self.layer == 0 {
            OverworldLayerId::Layer1
        } else {
            OverworldLayerId::Layer2
        }
    }

    fn invalidate(&mut self) {
        self.loaded = None;
        self.rendered_key = None;
        self.map16_rendered_key = None;
        self.records.invalidate();
        self.animation.invalidate();
        self.reset_animation_preview();
    }

    fn update_animation_preview_clock(&mut self, context: &egui::Context) {
        if self.animation_preview_paused {
            return;
        }
        let seconds = context.input(|input| input.time);
        let origin = *self.animation_preview_origin.get_or_insert(seconds);
        self.animation_preview_tick =
            overworld_animation_preview_tick(seconds - origin, self.animation_preview_rate);
        context.request_repaint_after(std::time::Duration::from_secs_f64(
            self.animation_preview_rate.interval_seconds(),
        ));
    }

    fn reset_animation_preview(&mut self) {
        self.animation_preview_origin = None;
        self.animation_preview_tick = 0;
        self.animation_preview_triggers = lm_graphics::ExAnimationTriggerPreviewState::default();
        self.animation_preview_events_passed.clear();
        self.animation_preview_events_passed.resize(256, false);
        if let Some(animation) = self
            .workspace
            .as_ref()
            .map(|workspace| &workspace.controller.data().animation)
        {
            for index in 0..16 {
                if animation.trigger_mask & (1 << index) != 0 {
                    self.animation_preview_triggers.manual_frames[index] =
                        animation.trigger_values[index];
                    self.animation_preview_triggers.custom[index] =
                        animation.trigger_values[index] != 0;
                }
            }
        }
    }

    fn animation_preview_controls(
        &mut self,
        ui: &mut egui::Ui,
        animation: &lm_graphics::CompactExAnimation,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if self.animation_preview_events_passed.len() != 256 {
            self.animation_preview_events_passed.resize(256, false);
        }
        ui.group(|ui| {
            ui.label(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationPreviewHeading,
            ));
            ui.horizontal(|ui| {
                let label = ow_text(
                    catalog,
                    if self.animation_preview_paused {
                        lm_app::ExtendedUiTextKey::OverworldAnimationPlay
                    } else {
                        lm_app::ExtendedUiTextKey::OverworldAnimationPause
                    },
                );
                if ui.button(label).clicked() {
                    self.animation_preview_paused = !self.animation_preview_paused;
                    if self.animation_preview_paused {
                        self.animation_preview_origin = None;
                    } else {
                        let now = ui.input(|input| input.time);
                        self.animation_preview_origin = Some(
                            now - self.animation_preview_tick as f64
                                * self.animation_preview_rate.interval_seconds(),
                        );
                    }
                }
                if ui
                    .button(ow_text(
                        catalog,
                        lm_app::ExtendedUiTextKey::OverworldAnimationReset,
                    ))
                    .clicked()
                {
                    self.reset_animation_preview();
                    self.rendered_key = None;
                }
                if ui
                    .add_enabled(
                        self.animation_preview_paused,
                        egui::Button::new(ow_text(
                            catalog,
                            lm_app::ExtendedUiTextKey::OverworldAnimationStepTimer,
                        )),
                    )
                    .clicked()
                {
                    self.animation_preview_tick = self.animation_preview_tick.saturating_add(1);
                    self.rendered_key = None;
                }
                ui.monospace(
                    ow_text(
                        catalog,
                        lm_app::ExtendedUiTextKey::OverworldAnimationPhaseTick,
                    )
                    .replace(
                        "{phase}",
                        &format!(
                            "{:X}",
                            self.animation_preview_tick
                                .saturating_mul(self.animation_preview_rate.substeps_per_tick())
                                & 7
                        ),
                    )
                    .replace("{tick}", &self.animation_preview_tick.to_string()),
                );
                egui::ComboBox::from_id_salt("overworld-animation-preview-rate")
                    .selected_text(self.animation_preview_rate.label())
                    .show_ui(ui, |ui| {
                        for rate in OverworldAnimationRate::ALL {
                            if ui
                                .selectable_value(
                                    &mut self.animation_preview_rate,
                                    rate,
                                    rate.label(),
                                )
                                .changed()
                            {
                                self.animation_preview_origin = None;
                                self.animation_preview_tick = 0;
                                self.rendered_key = None;
                            }
                        }
                    });
            });
            ui.small(
                ow_text(
                    catalog,
                    lm_app::ExtendedUiTextKey::OverworldAnimationTimerNotice,
                )
                .replace(
                    "{count}",
                    &self.animation_preview_rate.substeps_per_tick().to_string(),
                )
                .replace(
                    "{unit}",
                    if self.animation_preview_rate.substeps_per_tick() == 1 {
                        "substep"
                    } else {
                        "substeps"
                    },
                ),
            );
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("overworld-preview-trigger-kind")
                    .selected_text(match self.animation_preview_trigger_kind {
                        0 => ow_text(catalog, lm_app::ExtendedUiTextKey::OverworldAnimationCustom),
                        1 => ow_text(
                            catalog,
                            lm_app::ExtendedUiTextKey::OverworldAnimationOneShot,
                        ),
                        _ => ow_text(
                            catalog,
                            lm_app::ExtendedUiTextKey::OverworldAnimationManualFrame,
                        ),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.animation_preview_trigger_kind,
                            0,
                            ow_text(catalog, lm_app::ExtendedUiTextKey::OverworldAnimationCustom),
                        );
                        ui.selectable_value(
                            &mut self.animation_preview_trigger_kind,
                            1,
                            ow_text(
                                catalog,
                                lm_app::ExtendedUiTextKey::OverworldAnimationOneShot,
                            ),
                        );
                        ui.selectable_value(
                            &mut self.animation_preview_trigger_kind,
                            2,
                            ow_text(
                                catalog,
                                lm_app::ExtendedUiTextKey::OverworldAnimationManualFrame,
                            ),
                        );
                    });
                let maximum = if self.animation_preview_trigger_kind == 1 {
                    31
                } else {
                    15
                };
                self.animation_preview_trigger_index =
                    self.animation_preview_trigger_index.min(maximum);
                ui.add(
                    egui::DragValue::new(&mut self.animation_preview_trigger_index)
                        .range(0..=maximum)
                        .prefix("#"),
                );
                let changed = match self.animation_preview_trigger_kind {
                    0 => ui
                        .checkbox(
                            &mut self.animation_preview_triggers.custom
                                [self.animation_preview_trigger_index],
                            ow_text(catalog, lm_app::ExtendedUiTextKey::OverworldAnimationActive),
                        )
                        .changed(),
                    1 => ui
                        .checkbox(
                            &mut self.animation_preview_triggers.one_shot
                                [self.animation_preview_trigger_index],
                            ow_text(catalog, lm_app::ExtendedUiTextKey::OverworldAnimationActive),
                        )
                        .changed(),
                    _ => ui
                        .add(
                            egui::DragValue::new(
                                &mut self.animation_preview_triggers.manual_frames
                                    [self.animation_preview_trigger_index],
                            )
                            .range(0..=u8::MAX)
                            .prefix("frame $"),
                        )
                        .changed(),
                };
                if changed {
                    self.rendered_key = None;
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.animation_preview_event)
                        .range(0..=u8::MAX as usize)
                        .prefix(ow_text(
                            catalog,
                            lm_app::ExtendedUiTextKey::OverworldAnimationEventPrefix,
                        )),
                );
                if ui
                    .checkbox(
                        &mut self.animation_preview_events_passed[self.animation_preview_event],
                        ow_text(catalog, lm_app::ExtendedUiTextKey::OverworldAnimationPassed),
                    )
                    .changed()
                {
                    self.rendered_key = None;
                }
            });
            ui.small(ow_text(
                catalog,
                lm_app::ExtendedUiTextKey::OverworldAnimationEventManualNotice,
            ));
            if animation.records.is_empty() {
                ui.small(ow_text(
                    catalog,
                    lm_app::ExtendedUiTextKey::OverworldAnimationNoRecordsNotice,
                ));
            }
        });
    }
}

fn native_sprite_canvas_position(map: usize, x: usize, y: usize) -> Option<(u16, u16)> {
    if map >= 7 || y >= 64 {
        return None;
    }
    let local_x = if map == 0 {
        (x < 64).then_some(x)?
    } else if (64..128).contains(&x) {
        x - 64
    } else {
        return None;
    };
    Some((u16::try_from(local_x * 8).ok()?, u16::try_from(y * 8).ok()?))
}

fn native_sprite_canvas_edit(
    map: usize,
    selected: usize,
    count: usize,
    mut sprite: lm_overworld::NativeCustomOverworldSprite,
    position: (usize, usize),
) -> Result<NativeCustomOverworldSpriteEdit, String> {
    let (x, y) = native_sprite_canvas_position(map, position.0, position.1)
        .ok_or("the clicked canvas cell is outside the selected native sprite map")?;
    sprite.x = x;
    sprite.y = y;
    Ok(if selected < count {
        NativeCustomOverworldSpriteEdit::Replace {
            map,
            index: selected,
            sprite,
        }
    } else {
        NativeCustomOverworldSpriteEdit::Insert {
            map,
            index: count,
            sprite,
        }
    })
}

fn toggle_native_sprite_selection(selection: &mut BTreeSet<usize>, index: usize) {
    if !selection.remove(&index) {
        selection.insert(index);
    }
}

fn native_sprite_secondary_action(
    alt: bool,
    hit: Option<usize>,
    has_selection: bool,
) -> Option<NativeSpriteSecondaryAction> {
    if alt {
        hit.map(NativeSpriteSecondaryAction::EditProperties)
    } else {
        has_selection.then_some(NativeSpriteSecondaryAction::DuplicateSelection)
    }
}

fn inclusive_canvas_rect(
    anchor: (usize, usize),
    current: (usize, usize),
) -> (usize, usize, usize, usize) {
    (
        anchor.0.min(current.0),
        anchor.1.min(current.1),
        anchor.0.max(current.0).saturating_add(1),
        anchor.1.max(current.1).saturating_add(1),
    )
}

fn native_sprite_group_move_edits(
    map: usize,
    records: &[lm_overworld::NativeCustomOverworldSprite],
    selected: &[usize],
    anchor: (usize, usize),
    destination: (usize, usize),
) -> Result<Vec<NativeCustomOverworldSpriteEdit>, String> {
    const MAXIMUM_POSITION: i32 = 504;
    let selected = selected.iter().copied().collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let sprites = selected
        .iter()
        .map(|index| {
            records
                .get(*index)
                .map(|sprite| (*index, sprite))
                .ok_or_else(|| format!("native sprite index {index} is out of range"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let desired_x = snapped_pointer_delta(anchor.0, destination.0);
    let desired_y = snapped_pointer_delta(anchor.1, destination.1);
    let delta_x = constrain_native_sprite_axis(
        desired_x,
        sprites.iter().map(|(_, sprite)| i32::from(sprite.x)),
        MAXIMUM_POSITION,
    );
    let delta_y = constrain_native_sprite_axis(
        desired_y,
        sprites.iter().map(|(_, sprite)| i32::from(sprite.y)),
        MAXIMUM_POSITION,
    );
    if delta_x == 0 && delta_y == 0 {
        return Ok(Vec::new());
    }
    sprites
        .into_iter()
        .map(|(index, sprite)| {
            let mut sprite = sprite.clone();
            sprite.x = u16::try_from(i32::from(sprite.x) + delta_x)
                .map_err(|_| "constrained native sprite X is out of range")?;
            sprite.y = u16::try_from(i32::from(sprite.y) + delta_y)
                .map_err(|_| "constrained native sprite Y is out of range")?;
            Ok(NativeCustomOverworldSpriteEdit::Replace { map, index, sprite })
        })
        .collect()
}

fn native_sprite_group_duplicate_edits(
    map: usize,
    records: &[lm_overworld::NativeCustomOverworldSprite],
    selected: &[usize],
    destination: (usize, usize),
) -> Result<Vec<NativeCustomOverworldSpriteEdit>, String> {
    const MAXIMUM_SPRITES_PER_MAP: usize = 24;
    let selected = selected.iter().copied().collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    if records.len().saturating_add(selected.len()) > MAXIMUM_SPRITES_PER_MAP {
        return Err(format!(
            "duplicating {} selected native sprite(s) would exceed the 24-sprite map limit",
            selected.len()
        ));
    }
    let sprites = selected
        .iter()
        .map(|index| {
            records
                .get(*index)
                .map(|sprite| (*index, sprite))
                .ok_or_else(|| format!("native sprite index {index} is out of range"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let plane_x = if map == 0 { 0 } else { 512 };
    let destination_x = destination
        .0
        .checked_sub(plane_x)
        .filter(|x| *x < 512)
        .ok_or("the duplicate target is outside the selected native sprite map")?;
    if destination.1 >= 512 {
        return Err("the duplicate target is outside the selected native sprite map".into());
    }
    let Some((_, anchor)) = sprites.iter().min_by_key(|(index, sprite)| {
        (
            usize::from(sprite.x / 8) + usize::from(sprite.y / 8),
            *index,
        )
    }) else {
        return Ok(Vec::new());
    };
    let desired_x = snapped_pointer_delta(usize::from(anchor.x), destination_x);
    let desired_y = snapped_pointer_delta(usize::from(anchor.y), destination.1);
    let delta_x = constrain_native_sprite_axis(
        desired_x,
        sprites.iter().map(|(_, sprite)| i32::from(sprite.x)),
        504,
    );
    let delta_y = constrain_native_sprite_axis(
        desired_y,
        sprites.iter().map(|(_, sprite)| i32::from(sprite.y)),
        504,
    );
    sprites
        .into_iter()
        .enumerate()
        .map(|(offset, (_, sprite))| {
            let mut sprite = sprite.clone();
            sprite.x = u16::try_from(i32::from(sprite.x) + delta_x)
                .map_err(|_| "constrained duplicate native sprite X is out of range")?;
            sprite.y = u16::try_from(i32::from(sprite.y) + delta_y)
                .map_err(|_| "constrained duplicate native sprite Y is out of range")?;
            Ok(NativeCustomOverworldSpriteEdit::Insert {
                map,
                index: records.len() + offset,
                sprite,
            })
        })
        .collect()
}

fn native_sprite_selection_remove_edits(
    map: usize,
    selected: &BTreeSet<usize>,
) -> Vec<NativeCustomOverworldSpriteEdit> {
    selected
        .iter()
        .rev()
        .copied()
        .map(|index| NativeCustomOverworldSpriteEdit::Remove { map, index })
        .collect()
}

fn snapped_pointer_delta(anchor: usize, destination: usize) -> i32 {
    let anchor = i64::try_from(anchor / 8).unwrap_or(i64::MAX);
    let destination = i64::try_from(destination / 8).unwrap_or(i64::MAX);
    i32::try_from((destination - anchor).saturating_mul(8)).unwrap_or({
        if destination < anchor {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

fn constrain_native_sprite_axis(
    mut desired: i32,
    positions: impl Iterator<Item = i32>,
    maximum: i32,
) -> i32 {
    let positions = positions.collect::<Vec<_>>();
    while desired != 0
        && positions.iter().any(|position| {
            position
                .checked_add(desired)
                .is_none_or(|moved| !(0..=maximum).contains(&moved))
        })
    {
        desired += if desired < 0 { 8 } else { -8 };
    }
    desired
}

fn overworld_animation_preview_tick(seconds: f64, rate: OverworldAnimationRate) -> usize {
    if let Ok(tick) = std::env::var("LM_NATIVE_OVERWORLD_ANIMATION_TICK")
        && let Ok(tick) = tick.parse::<usize>()
    {
        return tick;
    }
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds / rate.interval_seconds()).floor() as u64;
    usize::try_from(ticks).unwrap_or(usize::MAX)
}

fn grid_line(start: (usize, usize), end: (usize, usize)) -> Vec<(usize, usize)> {
    let (mut x, mut y) = (start.0 as i64, start.1 as i64);
    let (end_x, end_y) = (end.0 as i64, end.1 as i64);
    let dx = (end_x - x).abs();
    let step_x = if x < end_x { 1 } else { -1 };
    let dy = -(end_y - y).abs();
    let step_y = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    let mut cells = Vec::new();
    loop {
        cells.push((x as usize, y as usize));
        if x == end_x && y == end_y {
            break;
        }
        let doubled = error * 2;
        if doubled >= dy {
            error += dy;
            x += step_x;
        }
        if doubled <= dx {
            error += dx;
            y += step_y;
        }
    }
    cells
}

fn route_endpoint_canvas_pixel(endpoint: OverworldEndpoint) -> Option<(u16, u16)> {
    let plane_x = match endpoint.submap {
        0 => 0,
        1..=6 => 512,
        _ => return None,
    };
    Some((plane_x + (endpoint.x & 0x01ff), endpoint.y & 0x01ff))
}

fn route_canvas_endpoint(
    rect: egui::Rect,
    position: egui::Pos2,
    selected_submap: u8,
) -> Option<OverworldEndpoint> {
    let (x, y) = overworld_editor_render::selected_tile(rect, position, 128, 64)?;
    let (x, submap) = if x < 64 {
        (x, 0)
    } else if (1..=6).contains(&selected_submap) {
        (x - 64, selected_submap)
    } else {
        return None;
    };
    Some(OverworldEndpoint {
        x: u16::try_from(x.checked_mul(8)?).ok()?,
        y: u16::try_from(y.checked_mul(8)?).ok()?,
        submap,
    })
}

fn route_directional_canvas_endpoint(
    rect: egui::Rect,
    position: egui::Pos2,
    selected_submap: u8,
    direction: OverworldPathDirection,
) -> Option<OverworldEndpoint> {
    route_canvas_endpoint(rect, position, selected_submap)
        .map(|endpoint| direction.offset_directional_point(endpoint))
}

fn stroke_edits(
    layer: OverworldLayerId,
    cells: &[(usize, usize)],
    tile: u16,
    mut current_tile: impl FnMut(usize, usize) -> Option<u16>,
) -> Vec<OverworldControllerEdit> {
    cells
        .iter()
        .copied()
        .filter_map(|(x, y)| {
            (current_tile(x, y) != Some(tile)).then_some(OverworldControllerEdit::SetLayerTile {
                layer,
                x,
                y,
                tile,
            })
        })
        .collect()
}

fn rectangle_cells(start: (usize, usize), end: (usize, usize)) -> Vec<(usize, usize)> {
    let minimum_x = start.0.min(end.0);
    let maximum_x = start.0.max(end.0);
    let minimum_y = start.1.min(end.1);
    let maximum_y = start.1.max(end.1);
    (minimum_y..=maximum_y)
        .flat_map(|y| (minimum_x..=maximum_x).map(move |x| (x, y)))
        .collect()
}

fn flood_fill_cells(
    width: usize,
    height: usize,
    tiles: &[u16],
    start: (usize, usize),
) -> Vec<(usize, usize)> {
    let Some(cell_count) = width.checked_mul(height) else {
        return Vec::new();
    };
    if width == 0
        || height == 0
        || tiles.len() != cell_count
        || start.0 >= width
        || start.1 >= height
    {
        return Vec::new();
    }
    let start_index = start.1 * width + start.0;
    let target = tiles[start_index];
    let mut visited = vec![false; cell_count];
    let mut pending = vec![start];
    let mut cells = Vec::new();
    while let Some((x, y)) = pending.pop() {
        let index = y * width + x;
        if visited[index] || tiles[index] != target {
            continue;
        }
        visited[index] = true;
        cells.push((x, y));
        if x > 0 {
            pending.push((x - 1, y));
        }
        if x + 1 < width {
            pending.push((x + 1, y));
        }
        if y > 0 {
            pending.push((x, y - 1));
        }
        if y + 1 < height {
            pending.push((x, y + 1));
        }
    }
    cells.sort_unstable_by_key(|&(x, y)| (y, x));
    cells
}

#[cfg(test)]
mod canvas_tests {
    use super::{
        MainPathLinkForm, NativeCustomOverworldSpriteEdit, NativeSpriteSecondaryAction,
        OverworldAnimationRate, OverworldControllerEdit, OverworldEndpoint, OverworldLayerId,
        OverworldPathDirection, OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget,
        RomOverworldEditor, flood_fill_cells, grid_line, inclusive_canvas_rect,
        native_sprite_canvas_edit, native_sprite_canvas_position,
        native_sprite_group_duplicate_edits, native_sprite_group_move_edits,
        native_sprite_secondary_action, native_sprite_selection_remove_edits,
        overworld_animation_preview_tick, rectangle_cells, route_canvas_endpoint,
        route_directional_canvas_endpoint, route_endpoint_canvas_pixel, stroke_edits,
        toggle_native_sprite_selection,
    };
    use crate::document_loader::BoundedRead;
    use crate::overworld_editor_render;
    use eframe::egui;

    #[test]
    fn rom_overworld_lifecycle_and_transfer_use_every_typed_key_without_literals() {
        let sources = [
            include_str!("rom_overworld_editor.rs"),
            include_str!("rom_overworld_editor/lifecycle.rs"),
            include_str!("rom_overworld_editor/transfer.rs"),
        ]
        .join("\n");
        for key in lm_app::ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("RomOverworld"))
        {
            assert!(
                sources.contains(&format!("{key:?}")),
                "missing ROM overworld label {key:?}"
            );
        }
        for child in [
            include_str!("rom_overworld_editor/lifecycle.rs"),
            include_str!("rom_overworld_editor/transfer.rs"),
        ] {
            for literal_widget in [
                "Window::new(\"",
                "ui.heading(\"",
                "ui.label(\"",
                "ui.button(\"",
                "ui.small(\"",
                "Button::new(\"",
            ] {
                assert!(
                    !child.contains(literal_widget),
                    "ROM overworld child surface regressed to fixed widget text: {literal_widget}"
                );
            }
        }
    }

    #[test]
    fn installed_animation_options_and_preview_use_every_typed_key() {
        let source = include_str!("rom_overworld_editor.rs");
        for key in [
            lm_app::ExtendedUiTextKey::OverworldAnimationOptionsHeading,
            lm_app::ExtendedUiTextKey::OverworldAnimationMapSelector,
            lm_app::ExtendedUiTextKey::OverworldAnimationOriginalPalette,
            lm_app::ExtendedUiTextKey::OverworldAnimationOriginalTiles,
            lm_app::ExtendedUiTextKey::OverworldAnimationGlobalFeature,
            lm_app::ExtendedUiTextKey::OverworldAnimationMapFeature,
            lm_app::ExtendedUiTextKey::OverworldAnimationOriginalLightning,
            lm_app::ExtendedUiTextKey::OverworldAnimationOptionsUnsupported,
            lm_app::ExtendedUiTextKey::OverworldAnimationRuntimeRequired,
            lm_app::ExtendedUiTextKey::OverworldAnimationInstallRuntime,
            lm_app::ExtendedUiTextKey::OverworldAnimationInstallRuntimeNotice,
            lm_app::ExtendedUiTextKey::OverworldAnimationInstallBlocked,
            lm_app::ExtendedUiTextKey::OverworldAnimationPreviewHeading,
            lm_app::ExtendedUiTextKey::OverworldAnimationPlay,
            lm_app::ExtendedUiTextKey::OverworldAnimationPause,
            lm_app::ExtendedUiTextKey::OverworldAnimationReset,
            lm_app::ExtendedUiTextKey::OverworldAnimationStepTimer,
            lm_app::ExtendedUiTextKey::OverworldAnimationPhaseTick,
            lm_app::ExtendedUiTextKey::OverworldAnimationTimerNotice,
            lm_app::ExtendedUiTextKey::OverworldAnimationCustom,
            lm_app::ExtendedUiTextKey::OverworldAnimationOneShot,
            lm_app::ExtendedUiTextKey::OverworldAnimationManualFrame,
            lm_app::ExtendedUiTextKey::OverworldAnimationActive,
            lm_app::ExtendedUiTextKey::OverworldAnimationEventPrefix,
            lm_app::ExtendedUiTextKey::OverworldAnimationPassed,
            lm_app::ExtendedUiTextKey::OverworldAnimationEventManualNotice,
            lm_app::ExtendedUiTextKey::OverworldAnimationNoRecordsNotice,
        ] {
            assert!(
                source.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "installed animation UI does not consume {key:?}"
            );
        }
        for bypass in [
            "ui.heading(\"Per-map animation options\")",
            "ui.button(\"Reset\")",
            "egui::Button::new(\"Step timer\")",
            "ui.small(\"No animation records are installed.\")",
        ] {
            assert!(
                !source.contains(bypass),
                "installed animation UI bypasses typed localization: {bypass}"
            );
        }
    }

    #[test]
    fn native_sprite_form_preserves_variable_extensions_and_hex_coordinates() {
        let mut editor = RomOverworldEditor::default();
        editor.native_sprite.id = "2A".into();
        editor.native_sprite.x = "01F8".into();
        editor.native_sprite.y = "0100".into();
        editor.native_sprite.screen = "F8".into();
        editor.native_sprite.extra = "01, aB 7f".into();
        assert_eq!(
            editor.parse_native_sprite().unwrap(),
            lm_overworld::NativeCustomOverworldSprite {
                id: 0x2a,
                x: 0x1f8,
                y: 0x100,
                screen: 0xf8,
                extra: vec![1, 0xab, 0x7f],
            }
        );
    }

    #[test]
    fn native_sprite_canvas_selection_maps_main_and_shared_planes_locally() {
        assert_eq!(native_sprite_canvas_position(0, 63, 63), Some((504, 504)));
        assert_eq!(native_sprite_canvas_position(1, 64, 2), Some((0, 16)));
        assert_eq!(native_sprite_canvas_position(6, 127, 63), Some((504, 504)));
        assert_eq!(native_sprite_canvas_position(0, 64, 0), None);
        assert_eq!(native_sprite_canvas_position(2, 63, 0), None);
        assert_eq!(native_sprite_canvas_position(7, 64, 0), None);
    }

    #[test]
    fn native_sprite_canvas_tool_inserts_at_end_or_replaces_selected_record() {
        let sprite = lm_overworld::NativeCustomOverworldSprite {
            id: 3,
            x: 0,
            y: 0,
            screen: 8,
            extra: vec![0xaa],
        };
        assert!(matches!(
            native_sprite_canvas_edit(1, 4, 4, sprite.clone(), (70, 3)).unwrap(),
            NativeCustomOverworldSpriteEdit::Insert { map: 1, index: 4, sprite }
                if (sprite.x, sprite.y) == (48, 24)
        ));
        assert!(matches!(
            native_sprite_canvas_edit(0, 2, 4, sprite, (10, 5)).unwrap(),
            NativeCustomOverworldSpriteEdit::Replace { map: 0, index: 2, sprite }
                if (sprite.x, sprite.y) == (80, 40)
        ));
    }

    #[test]
    fn native_sprite_selection_toggle_and_marquee_rectangle_are_deterministic() {
        let mut selected = std::collections::BTreeSet::from([1, 4]);
        toggle_native_sprite_selection(&mut selected, 1);
        toggle_native_sprite_selection(&mut selected, 3);
        assert_eq!(selected, std::collections::BTreeSet::from([3, 4]));
        assert_eq!(inclusive_canvas_rect((20, 30), (12, 9)), (12, 9, 21, 31));
        assert_eq!(inclusive_canvas_rect((7, 8), (7, 8)), (7, 8, 8, 9));
        assert_eq!(
            native_sprite_secondary_action(true, Some(5), true),
            Some(NativeSpriteSecondaryAction::EditProperties(5))
        );
        assert_eq!(native_sprite_secondary_action(true, None, true), None);
        assert_eq!(
            native_sprite_secondary_action(false, Some(5), true),
            Some(NativeSpriteSecondaryAction::DuplicateSelection)
        );
        assert_eq!(native_sprite_secondary_action(false, Some(5), false), None);
    }

    #[test]
    fn native_sprite_group_drag_is_one_ordered_constrained_edit_batch() {
        let sprite = |id, x, y| lm_overworld::NativeCustomOverworldSprite {
            id,
            x,
            y,
            screen: id.wrapping_add(0x10),
            extra: vec![id, 0xaa],
        };
        let records = vec![sprite(1, 8, 16), sprite(2, 496, 24), sprite(3, 40, 32)];
        let edits =
            native_sprite_group_move_edits(2, &records, &[2, 0, 2], (520, 16), (544, 40)).unwrap();
        assert_eq!(edits.len(), 2);
        assert!(matches!(
            &edits[0],
            NativeCustomOverworldSpriteEdit::Replace { map: 2, index: 0, sprite }
                if (sprite.x, sprite.y, sprite.screen, sprite.extra.as_slice())
                    == (32, 40, 0x11, &[1, 0xaa])
        ));
        assert!(matches!(
            &edits[1],
            NativeCustomOverworldSpriteEdit::Replace { map: 2, index: 2, sprite }
                if (sprite.x, sprite.y) == (64, 56)
        ));

        let constrained =
            native_sprite_group_move_edits(0, &records, &[0, 1], (8, 8), (80, 8)).unwrap();
        assert!(matches!(
            &constrained[0],
            NativeCustomOverworldSpriteEdit::Replace { sprite, .. } if sprite.x == 16
        ));
        assert!(matches!(
            &constrained[1],
            NativeCustomOverworldSpriteEdit::Replace { sprite, .. } if sprite.x == 504
        ));
        assert!(
            native_sprite_group_move_edits(0, &records, &[0], (8, 8), (9, 9))
                .unwrap()
                .is_empty()
        );
        assert!(native_sprite_group_move_edits(0, &records, &[9], (0, 0), (8, 0)).is_err());
    }

    #[test]
    fn native_sprite_right_drag_duplicates_the_complete_group_and_delete_is_descending() {
        let sprite = |id, x, y| lm_overworld::NativeCustomOverworldSprite {
            id,
            x,
            y,
            screen: id,
            extra: vec![id],
        };
        let records = vec![sprite(1, 16, 24), sprite(2, 40, 8), sprite(3, 80, 80)];
        let edits =
            native_sprite_group_duplicate_edits(1, &records, &[1, 0], (512 + 64, 40)).unwrap();
        assert_eq!(edits.len(), 2);
        assert!(matches!(
            &edits[0],
            NativeCustomOverworldSpriteEdit::Insert { map: 1, index: 3, sprite }
                if (sprite.id, sprite.x, sprite.y, sprite.extra.as_slice())
                    == (1, 64, 40, &[1])
        ));
        assert!(matches!(
            &edits[1],
            NativeCustomOverworldSpriteEdit::Insert { map: 1, index: 4, sprite }
                if (sprite.id, sprite.x, sprite.y) == (2, 88, 24)
        ));
        assert!(native_sprite_group_duplicate_edits(0, &records, &[0], (520, 40)).is_err());
        let full = vec![sprite(4, 0, 0); 24];
        assert!(native_sprite_group_duplicate_edits(0, &full, &[0], (0, 0)).is_err());

        let selected = std::collections::BTreeSet::from([1, 3, 7]);
        let deletes = native_sprite_selection_remove_edits(2, &selected);
        assert!(matches!(
            deletes.as_slice(),
            [
                NativeCustomOverworldSpriteEdit::Remove { map: 2, index: 7 },
                NativeCustomOverworldSpriteEdit::Remove { map: 2, index: 3 },
                NativeCustomOverworldSpriteEdit::Remove { map: 2, index: 1 }
            ]
        ));
    }

    #[test]
    fn native_sprite_canvas_hit_test_selects_topmost_record_on_the_active_plane() {
        let sprite = |id, x, y| lm_overworld::NativeCustomOverworldSprite {
            id,
            x,
            y,
            screen: 0,
            extra: Vec::new(),
        };
        let table = lm_overworld::NativeCustomOverworldSpriteTable {
            maps: [
                vec![sprite(1, 80, 40), sprite(2, 80, 40)],
                vec![sprite(3, 48, 24)],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
        };

        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 0, (80, 40)),
            Some(1)
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 1, (560, 24)),
            Some(0)
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 0, (560, 24)),
            None
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 1, (80, 40)),
            None
        );
        assert_eq!(
            overworld_editor_render::native_custom_sprite_hit_test(None, &table, 7, (512, 0)),
            None
        );
    }

    #[test]
    fn overworld_animation_clock_uses_the_authenticated_native_preview_cadence() {
        assert_eq!(
            overworld_animation_preview_tick(f64::NAN, OverworldAnimationRate::Fps15),
            0
        );
        assert_eq!(
            overworld_animation_preview_tick(-1.0, OverworldAnimationRate::Fps15),
            0
        );
        assert_eq!(
            overworld_animation_preview_tick(0.0, OverworldAnimationRate::Fps15),
            0
        );
        for rate in OverworldAnimationRate::ALL {
            let interval = rate.interval_seconds();
            assert_eq!(overworld_animation_preview_tick(interval * 0.99, rate), 0);
            assert_eq!(overworld_animation_preview_tick(interval, rate), 1);
            let callbacks = 64 / rate.substeps_per_tick();
            assert_eq!(overworld_animation_preview_tick(0.960, rate), callbacks);
            assert_eq!(
                rate.substeps_per_tick() * overworld_animation_preview_tick(0.960, rate),
                64
            );
        }
    }

    #[test]
    fn active_complete_transfer_rejects_direct_mutation_entry_point() {
        let mut editor = RomOverworldEditor::default();
        editor
            .transfer_loader
            .start(vec![BoundedRead::new(
                std::env::temp_dir().join(format!(
                    "lm-missing-overworld-transfer-{}",
                    std::process::id()
                )),
                1,
                "missing transfer fixture",
            )])
            .unwrap();
        assert!(editor.transfer_busy());
        editor.apply_many(&[OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer1,
            x: 0,
            y: 0,
            tile: 1,
        }]);
        assert_eq!(
            editor.error.as_deref(),
            Some("overworld editing is disabled during file transfer")
        );
    }

    #[test]
    fn drag_strokes_cover_skipped_grid_cells_in_both_directions() {
        assert_eq!(
            grid_line((1, 2), (5, 2)),
            vec![(1, 2), (2, 2), (3, 2), (4, 2), (5, 2)]
        );
        assert_eq!(
            grid_line((4, 4), (1, 1)),
            vec![(4, 4), (3, 3), (2, 2), (1, 1)]
        );
        assert_eq!(grid_line((3, 7), (3, 7)), vec![(3, 7)]);
    }

    #[test]
    fn stroke_batch_preserves_order_and_omits_unchanged_cells() {
        let edits = stroke_edits(
            OverworldLayerId::Layer2,
            &[(2, 4), (3, 4), (4, 4)],
            0x1234,
            |x, _| (x == 3).then_some(0x1234),
        );
        assert_eq!(
            edits,
            vec![
                OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: 2,
                    y: 4,
                    tile: 0x1234,
                },
                OverworldControllerEdit::SetLayerTile {
                    layer: OverworldLayerId::Layer2,
                    x: 4,
                    y: 4,
                    tile: 0x1234,
                },
            ]
        );
    }

    #[test]
    fn rectangle_cells_are_normalized_and_row_major() {
        assert_eq!(
            rectangle_cells((3, 2), (1, 1)),
            vec![(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2)]
        );
    }

    #[test]
    fn flood_fill_is_four_connected_bounded_and_row_major() {
        let tiles = [1, 1, 9, 1, 9, 2, 2, 2, 2];
        assert_eq!(
            flood_fill_cells(3, 3, &tiles, (0, 0)),
            vec![(0, 0), (1, 0), (0, 1)]
        );
        assert_eq!(flood_fill_cells(3, 3, &tiles, (2, 0)), vec![(2, 0)]);
        assert!(flood_fill_cells(3, 3, &tiles[..8], (0, 0)).is_empty());
        assert!(flood_fill_cells(3, 3, &tiles, (3, 0)).is_empty());
    }

    #[test]
    fn integrated_route_form_round_trips_every_native_field() {
        let link = OverworldPathLink {
            source: OverworldEndpoint {
                x: 0x1234,
                y: 0x5678,
                submap: 0x9a,
            },
            destination: OverworldEndpoint {
                x: 0xbcde,
                y: 0xf012,
                submap: 0x34,
            },
            target: OverworldPathTarget {
                x_tile: 0x56,
                y_tile: 0x78,
            },
        };
        let mut form = MainPathLinkForm {
            index: 3,
            ..Default::default()
        };
        form.set(link);
        assert_eq!(form.loaded, Some(3));
        assert_eq!(form.parse().unwrap(), link);
        form.target_x = "100".into();
        assert!(form.parse().is_err());

        form.set(OverworldPathLink {
            destination: OverworldEndpoint {
                x: 0xffff,
                y: 0xffff,
                submap: 0xff,
            },
            ..link
        });
        assert!(form.one_way);
        form.destination_x = "not parsed while one-way".into();
        assert_eq!(form.parse().unwrap().destination.x, 0xffff);
    }

    #[test]
    fn directional_route_canvas_clicks_apply_lunar_magic_edge_offsets() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 512.0));
        let position = egui::pos2(320.0, 40.0);
        let cases = [
            (OverworldPathDirection::Up, (328, 40)),
            (OverworldPathDirection::Down, (312, 40)),
            (OverworldPathDirection::Left, (320, 48)),
            (OverworldPathDirection::Right, (320, 32)),
        ];
        for (direction, (x, y)) in cases {
            assert_eq!(
                route_directional_canvas_endpoint(rect, position, 0, direction),
                Some(OverworldEndpoint { x, y, submap: 0 })
            );
        }
    }

    #[test]
    fn retained_lm363_left_to_up_one_way_route_transition_is_exact() {
        let fixture = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/overworld-path-direction/transition.tsv"
        );
        let fixture_field = |name: &str, column: usize| {
            fixture
                .lines()
                .find(|line| line.starts_with(name))
                .unwrap()
                .split('\t')
                .nth(column)
                .unwrap()
        };
        let record_hex = |column: usize| {
            fixture_field("interleaved_record_hex\t", column)
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(fixture_field("rejection_tile_type\t", 1), "00");
        assert_eq!(fixture_field("rejection_title\t", 1), "Wrong type of tile!");
        assert_eq!(
            fixture_field("rejection_body\t", 1),
            "An exit tile must be selected to use this."
        );
        assert_eq!(
            fixture_field("rejection_rom_sha256\t", 1),
            fixture_field("rejection_rom_sha256\t", 2)
        );
        let before = OverworldPathLink {
            source: OverworldEndpoint {
                x: 0x00d8,
                y: 0x00a0,
                submap: 0,
            },
            destination: OverworldEndpoint {
                x: 0x0058,
                y: 0x0150,
                submap: 2,
            },
            target: OverworldPathTarget {
                x_tile: 0x05,
                y_tile: 0x14,
            },
        };
        assert_eq!(
            &OverworldPathLinkTable {
                links: vec![before],
            }
            .encode_native_file()
            .unwrap()[12..],
            record_hex(1)
        );
        let mut form = MainPathLinkForm {
            index: 4,
            direction: OverworldPathDirection::Left,
            ..Default::default()
        };
        form.set(before);
        form.one_way = true;
        form.direction = OverworldPathDirection::Up;
        form.reorient_from(OverworldPathDirection::Left).unwrap();

        let after = form.parse().unwrap();
        assert_eq!(
            after,
            OverworldPathLink {
                source: OverworldEndpoint {
                    x: 0x00e0,
                    y: 0x0098,
                    submap: 0,
                },
                destination: OverworldEndpoint {
                    x: 0xffff,
                    y: 0xffff,
                    submap: 0xff,
                },
                target: before.target,
            }
        );
        assert_eq!(
            &OverworldPathLinkTable { links: vec![after] }
                .encode_native_file()
                .unwrap()[12..],
            record_hex(2)
        );
    }

    #[test]
    fn route_points_map_main_and_shared_submap_runtime_planes() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 512.0));
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(320.0, 40.0), 6),
            Some(OverworldEndpoint {
                x: 320,
                y: 40,
                submap: 0,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(511.0, 511.0), 6),
            Some(OverworldEndpoint {
                x: 504,
                y: 504,
                submap: 0,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(512.0, 40.0), 4),
            Some(OverworldEndpoint {
                x: 0,
                y: 40,
                submap: 4,
            })
        );
        assert_eq!(
            route_canvas_endpoint(rect, egui::pos2(512.0, 40.0), 0),
            None
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x140,
                y: 0x28,
                submap: 0,
            }),
            Some((0x140, 0x28))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x48,
                y: 0x10,
                submap: 1,
            }),
            Some((0x248, 0x10))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0x200,
                y: 0x200,
                submap: 3,
            }),
            Some((0x200, 0))
        );
        assert_eq!(
            route_endpoint_canvas_pixel(OverworldEndpoint {
                x: 0,
                y: 0,
                submap: 7,
            }),
            None
        );
    }
}
