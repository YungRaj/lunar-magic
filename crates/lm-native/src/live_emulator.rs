//! Native control surface for the isolated `LMEMU001` libretro backend.

use eframe::egui;
use lm_app::{
    EMULATOR_JOYPAD_A, EMULATOR_JOYPAD_B, EMULATOR_JOYPAD_DOWN, EMULATOR_JOYPAD_LEFT,
    EMULATOR_JOYPAD_RIGHT, EMULATOR_JOYPAD_SELECT, EMULATOR_JOYPAD_START, EMULATOR_JOYPAD_UP,
    EMULATOR_JOYPAD_X, EMULATOR_JOYPAD_Y, EmulatorBackendCommand, EmulatorBackendEvent,
    EmulatorPauseMode, EmulatorPauseReason, EmulatorSessionAction, EmulatorSessionState, UiTextKey,
};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

const MAX_PROTOCOL_RECORD: usize = 40 * 1024 * 1024;

enum WorkerCommand {
    Protocol(EmulatorBackendCommand),
    Stop,
}

struct RunningSession {
    commands: Sender<WorkerCommand>,
    events: Receiver<Result<EmulatorBackendEvent, String>>,
    model: EmulatorSessionState,
    pause: EmulatorPauseMode,
    capabilities: Option<u32>,
    joypad: u16,
    input_pause_until: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct LiveEmulator {
    running: Option<RunningSession>,
    source_level: Option<u16>,
    source_revision: Option<u64>,
    texture: Option<egui::TextureHandle>,
    frame_size: Option<[usize; 2]>,
    status: String,
}

impl LiveEmulator {
    pub(crate) fn start(
        &mut self,
        core: PathBuf,
        revision: u64,
        level: u16,
        rom: Vec<u8>,
    ) -> Result<(), String> {
        self.stop();
        let backend = backend_executable()?;
        let mut child = Command::new(&backend)
            .arg(&core)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                format!(
                    "could not start live emulator backend {}: {error}",
                    backend.display()
                )
            })?;
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-live-emulator".into())
            .spawn(move || {
                let result = run_worker(
                    &mut child,
                    &command_receiver,
                    &event_sender,
                    EmulatorBackendCommand::Initialize {
                        revision,
                        level,
                        flags: 0,
                        rom,
                        sprites: Vec::new(),
                    },
                );
                if let Err(error) = result {
                    let _ = event_sender.send(Err(error));
                }
                terminate_child(&mut child);
            })
            .map_err(|error| format!("could not create live emulator worker: {error}"))?;
        let mut model = EmulatorSessionState::default();
        let _ = model.start();
        self.running = Some(RunningSession {
            commands: command_sender,
            events: event_receiver,
            model,
            pause: EmulatorPauseMode::Running,
            capabilities: None,
            joypad: 0,
            input_pause_until: None,
        });
        self.source_level = Some(level);
        self.source_revision = Some(revision);
        self.texture = None;
        self.frame_size = None;
        self.status = format!("Starting live emulator for level {level:03X}");
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            let _ = running.commands.send(WorkerCommand::Stop);
        }
        self.texture = None;
        self.frame_size = None;
        self.source_level = None;
        self.source_revision = None;
    }

    pub(crate) fn source_context(&self) -> Option<(u16, u64)> {
        self.running
            .as_ref()
            .and(self.source_level.zip(self.source_revision))
    }

    /// Stops a live session only when there is no longer an open level/project context.
    pub(crate) fn retain_for_open_project(&mut self, context: Option<(u16, u64)>) -> bool {
        if self.running.is_some() && context.is_none() {
            self.stop();
            return false;
        }
        self.running.is_some()
    }

    pub(crate) fn switch_level(&mut self, level: u16, revision: u64) -> Result<(), String> {
        let running = self
            .running
            .as_ref()
            .ok_or_else(|| "live emulator is not running".to_string())?;
        running
            .commands
            .send(WorkerCommand::Protocol(EmulatorBackendCommand::LoadLevel(
                level,
            )))
            .map_err(|_| "live emulator worker disconnected".to_string())?;
        self.source_level = Some(level);
        self.source_revision = Some(revision);
        self.status = format!("Switching live emulator to level {level:03X}");
        Ok(())
    }

    pub(crate) fn reload_snapshot(
        &mut self,
        revision: u64,
        level: u16,
        rom: Vec<u8>,
    ) -> Result<(), String> {
        let running = self
            .running
            .as_ref()
            .ok_or_else(|| "live emulator is not running".to_string())?;
        running
            .commands
            .send(WorkerCommand::Protocol(EmulatorBackendCommand::ReloadRom {
                revision,
                rom,
            }))
            .map_err(|_| "live emulator worker disconnected".to_string())?;
        running
            .commands
            .send(WorkerCommand::Protocol(EmulatorBackendCommand::LoadLevel(
                level,
            )))
            .map_err(|_| "live emulator worker disconnected".to_string())?;
        self.source_level = Some(level);
        self.source_revision = Some(revision);
        self.texture = None;
        self.frame_size = None;
        self.status = format!("Reloading revision {revision} into level {level:03X}");
        Ok(())
    }

    pub(crate) fn set_editor_animation_playing(&mut self, playing: bool) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some(action) = running
            .model
            .set_hard_pause_reason(EmulatorPauseReason::EditorMode, !playing)
        else {
            return;
        };
        if let EmulatorSessionAction::SetPauseMode(mode) = action {
            running.pause = mode;
        }
        send_session_action(&running.commands, action);
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        text: impl Fn(UiTextKey) -> String,
    ) -> Option<String> {
        self.poll(context);
        let running = self.running.as_mut()?;
        let (focused, minimized) = context.input(viewport_pause_state);
        let now = Instant::now();
        let (deadline, input_paused) = updated_input_pause(
            running.input_pause_until,
            context.memory(|memory| memory.any_popup_open()),
            now,
        );
        running.input_pause_until = deadline;
        for pause_action in [
            running.model.set_focus_soft_paused(!focused),
            running
                .model
                .set_hard_pause_reason(EmulatorPauseReason::MainWindow, minimized),
            running
                .model
                .set_hard_pause_reason(EmulatorPauseReason::Input, input_paused),
        ]
        .into_iter()
        .flatten()
        {
            if let EmulatorSessionAction::SetPauseMode(mode) = pause_action {
                running.pause = mode;
            }
            send_session_action(&running.commands, pause_action);
        }
        let mut stop = false;
        let mut action = None;
        let window_response = egui::Window::new(text(UiTextKey::LiveEmulatorWindowTitle))
            .default_width(560.0)
            .resizable(true)
            .show(context, |ui| {
                ui.label(&self.status);
                if let (Some(texture), Some([width, height])) = (&self.texture, self.frame_size) {
                    let available = ui.available_width().max(1.0);
                    let scale = (available / width as f32).min(3.0);
                    ui.image((
                        texture.id(),
                        egui::vec2(width as f32, height as f32) * scale,
                    ));
                } else {
                    ui.spinner();
                }
                ui.horizontal(|ui| {
                    let paused = running.pause != EmulatorPauseMode::Running;
                    if ui
                        .button(text(if paused {
                            UiTextKey::LiveEmulatorResume
                        } else {
                            UiTextKey::LiveEmulatorPause
                        }))
                        .clicked()
                    {
                        action = running.model.toggle_manual_pause();
                    }
                    if ui
                        .add_enabled(paused, egui::Button::new(text(UiTextKey::LiveEmulatorStep)))
                        .clicked()
                    {
                        for session_action in running.model.step_frame() {
                            send_session_action(&running.commands, session_action);
                        }
                    }
                    stop = ui.button(text(UiTextKey::LiveEmulatorStop)).clicked();
                });
            });
        let viewport_paused = window_response
            .as_ref()
            .is_some_and(|response| response.inner.is_none());
        if let Some(pause_action) = running
            .model
            .set_hard_pause_reason(EmulatorPauseReason::Viewport, viewport_paused)
        {
            if let EmulatorSessionAction::SetPauseMode(mode) = pause_action {
                running.pause = mode;
            }
            send_session_action(&running.commands, pause_action);
        }
        if let Some(session_action) = action {
            if let EmulatorSessionAction::SetPauseMode(mode) = session_action {
                running.pause = mode;
            }
            send_session_action(&running.commands, session_action);
        }
        let joypad = context.input(joypad_from_input);
        if joypad != running.joypad {
            running.joypad = joypad;
            let _ =
                running
                    .commands
                    .send(WorkerCommand::Protocol(EmulatorBackendCommand::SetJoypad(
                        joypad,
                    )));
        }
        if stop {
            self.stop();
            return Some("Stopped live emulator".into());
        }
        context.request_repaint_after(Duration::from_millis(16));
        None
    }

    fn poll(&mut self, context: &egui::Context) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        loop {
            match running.events.try_recv() {
                Ok(Ok(event)) => match event {
                    EmulatorBackendEvent::Ready { capabilities } => {
                        running.capabilities = Some(capabilities);
                        self.status = format!("Backend ready (capabilities ${capabilities:08X})");
                    }
                    EmulatorBackendEvent::Active(true) => {
                        self.status = "ROM loaded; running live frames".into();
                    }
                    EmulatorBackendEvent::Active(false) => {
                        self.status = "Live emulator stopped".into();
                    }
                    EmulatorBackendEvent::Frame {
                        width,
                        height,
                        rgba,
                    } => {
                        install_frame(
                            context,
                            &mut self.texture,
                            &mut self.frame_size,
                            width,
                            height,
                            &rgba,
                        );
                    }
                    EmulatorBackendEvent::RuntimeFrame {
                        width,
                        height,
                        rgba,
                        state,
                    } => {
                        install_frame(
                            context,
                            &mut self.texture,
                            &mut self.frame_size,
                            width,
                            height,
                            &rgba,
                        );
                        self.status = format!(
                            "Live mode ${:02X}, sublevel {:03X}, translevel {:02X}, camera ({}, {})",
                            state.game_mode,
                            state.sublevel,
                            state.translevel,
                            state.camera_x,
                            state.camera_y
                        );
                    }
                    EmulatorBackendEvent::Error(error) => {
                        self.status = format!("Live emulator error: {error}");
                    }
                    EmulatorBackendEvent::Acknowledged | EmulatorBackendEvent::Viewport(_) => {}
                },
                Ok(Err(error)) => {
                    self.status = error;
                    self.running = None;
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.running = None;
                    return;
                }
            }
        }
    }
}

fn install_frame(
    context: &egui::Context,
    texture: &mut Option<egui::TextureHandle>,
    frame_size: &mut Option<[usize; 2]>,
    width: u32,
    height: u32,
    rgba: &[u8],
) {
    let size = [width as usize, height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba);
    if let Some(texture) = texture.as_mut() {
        texture.set(image, egui::TextureOptions::NEAREST);
    } else {
        *texture =
            Some(context.load_texture("live-libretro-frame", image, egui::TextureOptions::NEAREST));
    }
    *frame_size = Some(size);
}

fn joypad_from_input(input: &egui::InputState) -> u16 {
    let mut buttons = 0;
    for (key, mask) in [
        (egui::Key::Z, EMULATOR_JOYPAD_B),
        (egui::Key::A, EMULATOR_JOYPAD_Y),
        (egui::Key::Backspace, EMULATOR_JOYPAD_SELECT),
        (egui::Key::Enter, EMULATOR_JOYPAD_START),
        (egui::Key::ArrowUp, EMULATOR_JOYPAD_UP),
        (egui::Key::ArrowDown, EMULATOR_JOYPAD_DOWN),
        (egui::Key::ArrowLeft, EMULATOR_JOYPAD_LEFT),
        (egui::Key::ArrowRight, EMULATOR_JOYPAD_RIGHT),
        (egui::Key::X, EMULATOR_JOYPAD_A),
        (egui::Key::S, EMULATOR_JOYPAD_X),
    ] {
        if input.key_down(key) {
            buttons |= mask;
        }
    }
    buttons
}

fn viewport_pause_state(input: &egui::InputState) -> (bool, bool) {
    (
        input.viewport().focused.unwrap_or(true),
        input.viewport().minimized.unwrap_or(false),
    )
}

fn updated_input_pause(
    current_deadline: Option<Instant>,
    popup_open: bool,
    now: Instant,
) -> (Option<Instant>, bool) {
    let deadline = if popup_open {
        now.checked_add(Duration::from_millis(100))
    } else {
        current_deadline.filter(|deadline| *deadline > now)
    };
    (deadline, deadline.is_some())
}

impl Drop for LiveEmulator {
    fn drop(&mut self) {
        self.stop();
    }
}

fn send_session_action(commands: &Sender<WorkerCommand>, action: EmulatorSessionAction) {
    let command = match action {
        EmulatorSessionAction::SetPauseMode(mode) => EmulatorBackendCommand::SetPauseMode(mode),
        EmulatorSessionAction::StepFrame => EmulatorBackendCommand::StepFrame,
    };
    let _ = commands.send(WorkerCommand::Protocol(command));
}

fn backend_executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("LM_LIBRETRO_BACKEND") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe()
        .map_err(|error| format!("could not resolve native executable: {error}"))?;
    let name = if cfg!(windows) {
        "lm-libretro.exe"
    } else {
        "lm-libretro"
    };
    Ok(current.with_file_name(name))
}

fn run_worker(
    child: &mut Child,
    commands: &Receiver<WorkerCommand>,
    events: &Sender<Result<EmulatorBackendEvent, String>>,
    initialize: EmulatorBackendCommand,
) -> Result<(), String> {
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| "live emulator backend stdin was not piped".to_string())?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| "live emulator backend stdout was not piped".to_string())?;
    let ready = read_event(&mut output)?;
    if !matches!(ready, EmulatorBackendEvent::Ready { .. }) {
        return Err(format!(
            "live emulator backend did not begin with Ready: {ready:?}"
        ));
    }
    events.send(Ok(ready)).map_err(|error| error.to_string())?;
    send_command(&mut input, &initialize)?;
    events
        .send(Ok(read_event(&mut output)?))
        .map_err(|error| error.to_string())?;
    let mut emulating = true;
    loop {
        let next = if emulating {
            commands.recv_timeout(Duration::from_millis(16))
        } else {
            commands
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match next {
            Ok(WorkerCommand::Protocol(command)) => {
                let pause = match command {
                    EmulatorBackendCommand::SetPauseMode(mode) => Some(mode),
                    _ => None,
                };
                send_command(&mut input, &command)?;
                events
                    .send(Ok(read_event(&mut output)?))
                    .map_err(|error| error.to_string())?;
                if let Some(mode) = pause {
                    emulating = mode == EmulatorPauseMode::Running;
                }
            }
            Ok(WorkerCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if emulating {
                    send_command(&mut input, &EmulatorBackendCommand::StepFrame)?;
                    events
                        .send(Ok(read_event(&mut output)?))
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }
    let _ = send_command(&mut input, &EmulatorBackendCommand::Stop);
    Ok(())
}

fn send_command(writer: &mut impl Write, command: &EmulatorBackendCommand) -> Result<(), String> {
    let bytes = command.encode().map_err(|error| error.to_string())?;
    writer
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn read_event(reader: &mut impl Read) -> Result<EmulatorBackendEvent, String> {
    let mut header = [0_u8; 12];
    reader
        .read_exact(&mut header)
        .map_err(|error| format!("could not read live emulator event header: {error}"))?;
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if length > MAX_PROTOCOL_RECORD {
        return Err("live emulator event exceeds its bounded record size".into());
    }
    let mut record = Vec::with_capacity(12 + length);
    record.extend_from_slice(&header);
    record.resize(12 + length, 0);
    reader
        .read_exact(&mut record[12..])
        .map_err(|error| format!("could not read live emulator event payload: {error}"))?;
    EmulatorBackendEvent::decode(&record).map_err(|error| error.to_string())
}

fn terminate_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) fn choose_core() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose Snes9x Libretro Core")
        .add_filter("Libretro core", &["dylib", "so", "dll"])
        .pick_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_and_event_stream_helpers_share_exact_protocol_records() {
        let command = EmulatorBackendCommand::SetPauseMode(EmulatorPauseMode::HardPaused);
        let mut encoded = Vec::new();
        send_command(&mut encoded, &command).unwrap();
        assert_eq!(EmulatorBackendCommand::decode(&encoded).unwrap(), command);

        let event = EmulatorBackendEvent::Active(true);
        let bytes = event.encode().unwrap();
        assert_eq!(read_event(&mut bytes.as_slice()).unwrap(), event);
    }

    #[test]
    fn event_reader_rejects_truncation_and_unbounded_lengths() {
        assert!(read_event(&mut [0_u8; 11].as_slice()).is_err());
        let mut header = *b"LMEMU001\0\0\0\0";
        header[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_event(&mut header.as_slice()).is_err());
    }

    #[test]
    fn sibling_backend_name_is_platform_specific() {
        let path = backend_executable().unwrap();
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            if cfg!(windows) {
                "lm-libretro.exe"
            } else {
                "lm-libretro"
            }
        );
    }

    #[test]
    fn live_session_survives_context_changes_and_stops_when_project_closes() {
        let (commands, _command_receiver) = mpsc::channel();
        let (_event_sender, events) = mpsc::channel();
        let mut emulator = LiveEmulator::default();
        emulator.running = Some(RunningSession {
            commands,
            events,
            model: EmulatorSessionState::default(),
            pause: EmulatorPauseMode::Running,
            capabilities: None,
            joypad: 0,
            input_pause_until: None,
        });
        emulator.source_level = Some(0x105);
        emulator.source_revision = Some(7);
        assert_eq!(emulator.source_context(), Some((0x105, 7)));
        assert!(emulator.retain_for_open_project(Some((0x105, 8))));
        assert!(!emulator.retain_for_open_project(None));
        assert!(emulator.running.is_none());
        assert_eq!(emulator.source_level, None);
        assert_eq!(emulator.source_revision, None);
    }

    #[test]
    fn level_switch_and_revision_reload_queue_exact_backend_commands() {
        let (commands, command_receiver) = mpsc::channel();
        let (_event_sender, events) = mpsc::channel();
        let mut emulator = LiveEmulator::default();
        let mut model = EmulatorSessionState::default();
        let _ = model.start();
        emulator.running = Some(RunningSession {
            commands,
            events,
            model,
            pause: EmulatorPauseMode::Running,
            capabilities: None,
            joypad: 0,
            input_pause_until: None,
        });
        emulator.source_level = Some(0x105);
        emulator.source_revision = Some(7);

        emulator.switch_level(0x106, 7).unwrap();
        assert!(matches!(
            command_receiver.recv().unwrap(),
            WorkerCommand::Protocol(EmulatorBackendCommand::LoadLevel(0x106))
        ));
        assert_eq!(emulator.source_context(), Some((0x106, 7)));

        emulator.reload_snapshot(8, 0x107, vec![1, 2, 3]).unwrap();
        assert!(matches!(
            command_receiver.recv().unwrap(),
            WorkerCommand::Protocol(EmulatorBackendCommand::ReloadRom {
                revision: 8,
                rom
            }) if rom == vec![1, 2, 3]
        ));
        assert!(matches!(
            command_receiver.recv().unwrap(),
            WorkerCommand::Protocol(EmulatorBackendCommand::LoadLevel(0x107))
        ));
        assert_eq!(emulator.source_context(), Some((0x107, 8)));

        emulator.set_editor_animation_playing(false);
        assert!(matches!(
            command_receiver.recv().unwrap(),
            WorkerCommand::Protocol(EmulatorBackendCommand::SetPauseMode(
                EmulatorPauseMode::HardPaused
            ))
        ));
        emulator.set_editor_animation_playing(true);
        assert!(matches!(
            command_receiver.recv().unwrap(),
            WorkerCommand::Protocol(EmulatorBackendCommand::SetPauseMode(
                EmulatorPauseMode::Running
            ))
        ));
    }

    #[test]
    fn keyboard_mapping_uses_standard_snes_layout_without_unknown_bits() {
        let mut input = egui::RawInput::default();
        for key in [egui::Key::Z, egui::Key::X, egui::Key::ArrowRight] {
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let context = egui::Context::default();
        context.begin_pass(input);
        let buttons = context.input(joypad_from_input);
        let _ = context.end_pass();
        assert_eq!(
            buttons,
            EMULATOR_JOYPAD_B | EMULATOR_JOYPAD_A | EMULATOR_JOYPAD_RIGHT
        );
    }

    #[test]
    fn viewport_focus_and_minimize_map_to_original_pause_inputs() {
        let mut input = egui::RawInput::default();
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.focused = Some(false);
        viewport.minimized = Some(true);
        let context = egui::Context::default();
        context.begin_pass(input);
        assert_eq!(context.input(viewport_pause_state), (false, true));
        let _ = context.end_pass();
    }

    #[test]
    fn menu_input_pause_retains_the_recovered_hundred_millisecond_grace() {
        let now = Instant::now();
        let (deadline, paused) = updated_input_pause(None, true, now);
        assert!(paused);
        let deadline = deadline.unwrap();
        assert_eq!(deadline.duration_since(now), Duration::from_millis(100));
        assert!(updated_input_pause(Some(deadline), false, now + Duration::from_millis(99)).1);
        assert_eq!(
            updated_input_pause(Some(deadline), false, now + Duration::from_millis(100)),
            (None, false)
        );
    }
}
