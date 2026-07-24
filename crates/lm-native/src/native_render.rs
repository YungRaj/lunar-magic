use eframe::egui;
use lm_app::{AppState, EditorMode, RevisionProfileControllers};
use lm_graphics::{
    Bgr555, GraphicsInterchangeFile, Palette, PaletteInterchangeFile, PaletteOwnership,
};
use lm_level::Map16PageFile;
use lm_render::{
    Canvas, render_portable_graphics, render_portable_map16_page, render_portable_palette,
};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderKey {
    revision: u64,
    mode: EditorMode,
    profile_name: String,
    generation: u64,
}

struct RunningRender {
    key: RenderKey,
    result: Receiver<Result<Canvas, String>>,
}

#[derive(Default)]
pub(crate) struct NativeRenderState {
    key: Option<RenderKey>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    running: Option<RunningRender>,
    generation: u64,
}

impl NativeRenderState {
    pub(crate) fn invalidate(&mut self) {
        self.key = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        ui: &mut egui::Ui,
        app: &AppState,
    ) -> bool {
        let profile_name = if let Some(profile) = app.revision_profile() {
            profile.name.clone()
        } else if is_builtin_vanilla_graphics(app) && matches!(app.mode, EditorMode::Graphics(_)) {
            "Built-in SMW US revision 0".into()
        } else {
            return false;
        };
        if app.revision_profile().is_some()
            && !matches!(
                app.mode,
                EditorMode::Graphics(_) | EditorMode::Palette(_) | EditorMode::Map16
            )
        {
            return false;
        }
        let key = RenderKey {
            revision: app.project_revision(),
            mode: app.mode,
            profile_name,
            generation: self.generation,
        };
        self.poll(context, &key);
        if self.key.as_ref() != Some(&key) {
            self.start(app, key.clone());
        }
        if self.running.is_some() {
            ui.centered_and_justified(|ui| {
                ui.label("Preparing native preview…");
            });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        } else if self.key.as_ref() == Some(&key)
            && let Some(texture) = &self.texture
        {
            egui::ScrollArea::both().show(ui, |ui| {
                ui.image(texture);
            });
        } else if self.key.as_ref() == Some(&key)
            && let Some(error) = &self.error
        {
            ui.centered_and_justified(|ui| {
                ui.label(format!("Preview unavailable: {error}"));
            });
        }
        true
    }

    fn start(&mut self, app: &AppState, key: RenderKey) {
        if self.running.is_some() {
            return;
        }
        self.texture = None;
        self.error = None;
        self.key = None;
        let (sender, result) = mpsc::channel();
        let input = app.profiled_controller_snapshot().map_or_else(
            |_| app.controller_snapshot().map(RenderInput::Builtin),
            |profiled| Ok(RenderInput::Profiled(Box::new(profiled))),
        );
        let input = match input {
            Ok(input) => input,
            Err(error) => {
                self.error = Some(error.to_string());
                self.key = Some(key);
                return;
            }
        };
        match std::thread::Builder::new()
            .name("lm-native-render".into())
            .spawn(move || {
                let rendered = match input {
                    RenderInput::Profiled(profiled) => build_canvas(&profiled),
                    RenderInput::Builtin(snapshot) => build_builtin_canvas(&snapshot),
                };
                let _send_result = sender.send(rendered);
            }) {
            Ok(_worker) => self.running = Some(RunningRender { key, result }),
            Err(error) => {
                self.error = Some(format!("could not create native-render worker: {error}"));
                self.key = Some(key);
            }
        }
    }

    fn poll(&mut self, context: &egui::Context, requested: &RenderKey) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let completion = match running.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(
                "native-render worker stopped without reporting a result".into(),
            )),
        };
        let Some(completion) = completion else {
            return;
        };
        let Some(running) = self.running.take() else {
            return;
        };
        if &running.key != requested {
            return;
        }
        match completion {
            Ok(canvas) => {
                self.texture = Some(context.load_texture(
                    "native-editor-preview",
                    color_image(&canvas),
                    egui::TextureOptions::NEAREST,
                ));
                self.error = None;
            }
            Err(error) => {
                self.texture = None;
                self.error = Some(error);
            }
        }
        self.key = Some(running.key);
    }
}

enum RenderInput {
    Profiled(Box<lm_app::ProfiledControllerSnapshot>),
    Builtin(lm_app::ControllerSnapshot),
}

fn is_builtin_vanilla_graphics(app: &AppState) -> bool {
    app.controller_snapshot().is_ok_and(|snapshot| {
        snapshot.identity.game == SupportedGame::SuperMarioWorld
            && snapshot.identity.region == Region::NorthAmerica
            && snapshot.identity.revision == 0
            && snapshot.identity.mapper == Mapper::LoRom
    })
}

fn build_builtin_canvas(snapshot: &lm_app::ControllerSnapshot) -> Result<Canvas, String> {
    let EditorMode::Graphics(slot) = snapshot.mode else {
        return Err("built-in vanilla rendering currently supports graphics mode".into());
    };
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let project = lm_project::Project::new(image);
    let graphics = project
        .load_graphics_file(
            usize::from(slot),
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .map_err(|error| error.to_string())?;
    let palette = PaletteInterchangeFile {
        source_palette: 0,
        palette: Palette {
            colors: (0_u16..16)
                .map(|component| Bgr555(component | (component << 5) | (component << 10)))
                .collect(),
        },
    };
    render_portable_graphics(
        &GraphicsInterchangeFile {
            source_slot: slot,
            graphics,
        },
        &palette,
        0,
        16,
    )
    .map_err(|error| error.to_string())
}

fn build_canvas(profiled: &lm_app::ProfiledControllerSnapshot) -> Result<Canvas, String> {
    match profiled.snapshot.mode {
        EditorMode::Graphics(slot) => {
            let graphics = profiled
                .profile
                .decode_graphics_editable(&profiled.snapshot)
                .map_err(|error| error.to_string())?;
            let palette = decode_palette_zero(profiled)?;
            render_portable_graphics(
                &GraphicsInterchangeFile {
                    source_slot: slot,
                    graphics: graphics.graphics().clone(),
                },
                &palette,
                0,
                16,
            )
            .map_err(|error| error.to_string())
        }
        EditorMode::Palette(slot) => {
            let palette = profiled
                .profile
                .decode_palette(
                    &profiled.snapshot,
                    PaletteOwnership::editable(profiled.profile.palette.colors_per_palette),
                )
                .map_err(|error| error.to_string())?;
            render_portable_palette(
                &PaletteInterchangeFile {
                    source_palette: slot,
                    palette: palette.palette().clone(),
                },
                16,
                16,
            )
            .map_err(|error| error.to_string())
        }
        EditorMode::Map16 => {
            let map16 = profiled
                .profile
                .decode_map16(&profiled.snapshot)
                .map_err(|error| error.to_string())?;
            let mut graphics_snapshot = profiled.snapshot.clone();
            graphics_snapshot.mode = EditorMode::Graphics(0);
            let graphics = profiled
                .profile
                .decode_graphics_editable(&graphics_snapshot)
                .map_err(|error| error.to_string())?;
            let palette = decode_palette_zero(profiled)?;
            let page = map16
                .set()
                .pages
                .first()
                .cloned()
                .ok_or_else(|| "native Map16 workspace has no pages".to_owned())?;
            render_portable_map16_page(
                &GraphicsInterchangeFile {
                    source_slot: 0,
                    graphics: graphics.graphics().clone(),
                },
                &palette,
                &Map16PageFile {
                    source_page: 0,
                    page,
                },
            )
            .map_err(|error| error.to_string())
        }
        _ => Err("editor mode has no native raster adapter".into()),
    }
}

fn decode_palette_zero(
    profiled: &lm_app::ProfiledControllerSnapshot,
) -> Result<PaletteInterchangeFile, String> {
    let mut snapshot = profiled.snapshot.clone();
    snapshot.mode = EditorMode::Palette(0);
    let palette = profiled
        .profile
        .decode_palette(
            &snapshot,
            PaletteOwnership::editable(profiled.profile.palette.colors_per_palette),
        )
        .map_err(|error| error.to_string())?;
    Ok(PaletteInterchangeFile {
        source_palette: 0,
        palette: palette.palette().clone(),
    })
}

fn color_image(canvas: &Canvas) -> egui::ColorImage {
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_render::Rgba;

    fn key(revision: u64) -> RenderKey {
        RenderKey {
            revision,
            mode: EditorMode::Map16,
            profile_name: "fixture".into(),
            generation: 0,
        }
    }

    #[test]
    fn canvas_conversion_preserves_dimensions_and_rgba() {
        let canvas = Canvas::from_pixels(
            2,
            1,
            vec![
                Rgba {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                },
                Rgba {
                    red: 5,
                    green: 6,
                    blue: 7,
                    alpha: 8,
                },
            ],
        )
        .unwrap();
        let image = color_image(&canvas);
        assert_eq!(image.size, [2, 1]);
        assert_eq!(
            image.pixels[0],
            egui::Color32::from_rgba_unmultiplied(1, 2, 3, 4)
        );
        assert_eq!(
            image.pixels[1],
            egui::Color32::from_rgba_unmultiplied(5, 6, 7, 8)
        );
    }

    #[test]
    fn stale_worker_completion_is_discarded_before_texture_upload() {
        let (sender, result) = mpsc::channel();
        sender.send(Ok(Canvas::try_new(1, 1).unwrap())).unwrap();
        let mut state = NativeRenderState {
            running: Some(RunningRender {
                key: key(1),
                result,
            }),
            ..NativeRenderState::default()
        };

        state.poll(&egui::Context::default(), &key(2));

        assert!(state.running.is_none());
        assert!(state.key.is_none());
        assert!(state.texture.is_none());
    }

    #[test]
    fn disconnected_current_worker_becomes_a_keyed_preview_error() {
        let (sender, result) = mpsc::channel::<Result<Canvas, String>>();
        drop(sender);
        let requested = key(3);
        let mut state = NativeRenderState {
            running: Some(RunningRender {
                key: requested.clone(),
                result,
            }),
            ..NativeRenderState::default()
        };

        state.poll(&egui::Context::default(), &requested);

        assert_eq!(state.key, Some(requested));
        assert!(state.error.as_deref().unwrap().contains("stopped"));
    }

    #[test]
    fn explicit_invalidation_rejects_same_revision_worker() {
        let (sender, result) = mpsc::channel();
        sender.send(Ok(Canvas::try_new(1, 1).unwrap())).unwrap();
        let old_key = key(4);
        let mut state = NativeRenderState {
            running: Some(RunningRender {
                key: old_key.clone(),
                result,
            }),
            ..NativeRenderState::default()
        };

        state.invalidate();
        let requested = RenderKey {
            generation: state.generation,
            ..old_key
        };
        state.poll(&egui::Context::default(), &requested);

        assert!(state.running.is_none());
        assert!(state.key.is_none());
        assert!(state.texture.is_none());
    }
}
