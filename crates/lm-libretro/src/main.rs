//! Isolated libretro backend for Lunar Magic Rust's `LMEMU001` live-session protocol.

use libloading::Library;
use lm_app::{
    EmulatorBackendCommand, EmulatorBackendEvent, EmulatorPauseMode, EmulatorViewport,
    MAX_EMULATOR_FRAME_HEIGHT, MAX_EMULATOR_FRAME_WIDTH,
};
use std::ffi::{c_char, c_void};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const RETRO_API_VERSION: u32 = 1;
const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: u32 = 10;
const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: u32 = 11;
const RETRO_ENVIRONMENT_SET_VARIABLES: u32 = 16;
const RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: u32 = 18;
const RETRO_ENVIRONMENT_SET_MEMORY_MAPS: u32 = 36;
const RETRO_ENVIRONMENT_SET_GEOMETRY: u32 = 37;
const RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE: u32 = 47;
const CAP_ROM_LOAD: u32 = 1 << 0;
const CAP_FRAME: u32 = 1 << 1;
const CAP_PAUSE: u32 = 1 << 2;
const CAP_STEP: u32 = 1 << 3;
const CAP_VIEWPORT: u32 = 1 << 4;
const CAPABILITIES: u32 = CAP_ROM_LOAD | CAP_FRAME | CAP_PAUSE | CAP_STEP | CAP_VIEWPORT;
const MAX_PROTOCOL_RECORD: usize = 40 * 1024 * 1024;

#[repr(C)]
struct RetroGameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

#[repr(C)]
struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

type EnvironmentFn = unsafe extern "C" fn(u32, *mut c_void) -> bool;
type VideoFn = unsafe extern "C" fn(*const c_void, u32, u32, usize);
type AudioFn = unsafe extern "C" fn(i16, i16);
type AudioBatchFn = unsafe extern "C" fn(*const i16, usize) -> usize;
type InputPollFn = unsafe extern "C" fn();
type InputStateFn = unsafe extern "C" fn(u32, u32, u32, u32) -> i16;

struct CoreApi {
    _library: Library,
    set_environment: unsafe extern "C" fn(EnvironmentFn),
    set_video_refresh: unsafe extern "C" fn(VideoFn),
    set_audio_sample: unsafe extern "C" fn(AudioFn),
    set_audio_sample_batch: unsafe extern "C" fn(AudioBatchFn),
    set_input_poll: unsafe extern "C" fn(InputPollFn),
    set_input_state: unsafe extern "C" fn(InputStateFn),
    init: unsafe extern "C" fn(),
    api_version: unsafe extern "C" fn() -> u32,
    get_system_info: unsafe extern "C" fn(*mut RetroSystemInfo),
    load_game: unsafe extern "C" fn(*const RetroGameInfo) -> bool,
    run: unsafe extern "C" fn(),
    unload_game: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
}

impl CoreApi {
    fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: The library remains owned by `CoreApi`; every copied symbol has the exact
        // libretro v1 C ABI and is used only while that owner is alive.
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("could not load libretro core {}: {error}", path.display()))?;
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: Symbol names and signatures are fixed by the libretro v1 API.
                *unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) }
                    .map_err(|error| format!("libretro core is missing {}: {error}", $name))?
            }};
        }
        let api = Self {
            set_environment: symbol!("retro_set_environment", unsafe extern "C" fn(EnvironmentFn)),
            set_video_refresh: symbol!("retro_set_video_refresh", unsafe extern "C" fn(VideoFn)),
            set_audio_sample: symbol!("retro_set_audio_sample", unsafe extern "C" fn(AudioFn)),
            set_audio_sample_batch: symbol!(
                "retro_set_audio_sample_batch",
                unsafe extern "C" fn(AudioBatchFn)
            ),
            set_input_poll: symbol!("retro_set_input_poll", unsafe extern "C" fn(InputPollFn)),
            set_input_state: symbol!("retro_set_input_state", unsafe extern "C" fn(InputStateFn)),
            init: symbol!("retro_init", unsafe extern "C" fn()),
            api_version: symbol!("retro_api_version", unsafe extern "C" fn() -> u32),
            get_system_info: symbol!(
                "retro_get_system_info",
                unsafe extern "C" fn(*mut RetroSystemInfo)
            ),
            load_game: symbol!(
                "retro_load_game",
                unsafe extern "C" fn(*const RetroGameInfo) -> bool
            ),
            run: symbol!("retro_run", unsafe extern "C" fn()),
            unload_game: symbol!("retro_unload_game", unsafe extern "C" fn()),
            deinit: symbol!("retro_deinit", unsafe extern "C" fn()),
            _library: library,
        };
        // SAFETY: Pure version query with no arguments.
        if unsafe { (api.api_version)() } != RETRO_API_VERSION {
            return Err("libretro core does not implement API version 1".into());
        }
        Ok(api)
    }
}

#[derive(Default)]
struct VideoState {
    pixel_format: u32,
    width: u32,
    height: u32,
    pitch: usize,
    bytes: Vec<u8>,
}

static VIDEO: OnceLock<Mutex<VideoState>> = OnceLock::new();

unsafe extern "C" fn environment(command: u32, data: *mut c_void) -> bool {
    match command {
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
            if data.is_null() {
                return false;
            }
            // SAFETY: The command contract supplies one readable `u32` for the duration of call.
            let format = unsafe { *(data.cast::<u32>()) };
            if format > 2 {
                return false;
            }
            if let Ok(mut video) = VIDEO.get_or_init(Default::default).lock() {
                video.pixel_format = format;
                true
            } else {
                false
            }
        }
        RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE => {
            if data.is_null() {
                return false;
            }
            // SAFETY: The command contract supplies one writable `i32` for the duration of call.
            unsafe { *data.cast::<i32>() = 3 };
            true
        }
        RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS
        | RETRO_ENVIRONMENT_SET_VARIABLES
        | RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME
        | RETRO_ENVIRONMENT_SET_MEMORY_MAPS
        | RETRO_ENVIRONMENT_SET_GEOMETRY => true,
        _ => false,
    }
}

unsafe extern "C" fn video_refresh(data: *const c_void, width: u32, height: u32, pitch: usize) {
    if data.is_null()
        || width == 0
        || height == 0
        || width > MAX_EMULATOR_FRAME_WIDTH
        || height > MAX_EMULATOR_FRAME_HEIGHT
        || pitch == 0
        || pitch > 4096
    {
        return;
    }
    let Some(length) = usize::try_from(height)
        .ok()
        .and_then(|height| height.checked_mul(pitch))
        .filter(|length| *length <= 16 * 1024 * 1024)
    else {
        return;
    };
    // SAFETY: libretro guarantees `data` contains `height * pitch` readable bytes during callback.
    let source = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) };
    if let Ok(mut video) = VIDEO.get_or_init(Default::default).lock() {
        video.width = width;
        video.height = height;
        video.pitch = pitch;
        video.bytes.clear();
        video.bytes.extend_from_slice(source);
    }
}

unsafe extern "C" fn audio_sample(_left: i16, _right: i16) {}
unsafe extern "C" fn audio_batch(_data: *const i16, frames: usize) -> usize {
    frames
}
unsafe extern "C" fn input_poll() {}
unsafe extern "C" fn input_state(_port: u32, _device: u32, _index: u32, _id: u32) -> i16 {
    0
}

struct Backend {
    api: CoreApi,
    initialized: bool,
    loaded: bool,
    rom: Vec<u8>,
    pause: EmulatorPauseMode,
    viewport: Option<EmulatorViewport>,
}

impl Backend {
    fn new(core: &Path) -> Result<Self, String> {
        let api = CoreApi::load(core)?;
        // SAFETY: All callback addresses are static functions and remain valid for process life.
        unsafe {
            (api.set_environment)(environment);
            (api.set_video_refresh)(video_refresh);
            (api.set_audio_sample)(audio_sample);
            (api.set_audio_sample_batch)(audio_batch);
            (api.set_input_poll)(input_poll);
            (api.set_input_state)(input_state);
            (api.init)();
        }
        let mut info = RetroSystemInfo {
            library_name: std::ptr::null(),
            library_version: std::ptr::null(),
            valid_extensions: std::ptr::null(),
            need_fullpath: false,
            block_extract: false,
        };
        // SAFETY: `info` is writable and has the exact libretro v1 layout.
        unsafe { (api.get_system_info)(&raw mut info) };
        if info.need_fullpath {
            // SAFETY: Core was initialized above and must be deinitialized before returning.
            unsafe { (api.deinit)() };
            return Err(
                "libretro core requires full-path ROM loading; in-memory loading is required"
                    .into(),
            );
        }
        Ok(Self {
            api,
            initialized: true,
            loaded: false,
            rom: Vec::new(),
            pause: EmulatorPauseMode::Running,
            viewport: None,
        })
    }

    fn load_rom(&mut self, rom: Vec<u8>) -> Result<(), String> {
        if self.loaded {
            // SAFETY: `loaded` records one successful matching `retro_load_game` call.
            unsafe { (self.api.unload_game)() };
            self.loaded = false;
        }
        self.rom = rom;
        let game = RetroGameInfo {
            path: std::ptr::null(),
            data: self.rom.as_ptr().cast(),
            size: self.rom.len(),
            meta: std::ptr::null(),
        };
        // SAFETY: `game` and the owned ROM backing remain valid throughout the loaded session.
        if !unsafe { (self.api.load_game)(&raw const game) } {
            self.rom.clear();
            return Err("libretro core rejected the ROM image".into());
        }
        self.loaded = true;
        Ok(())
    }

    fn run_frame(&self) -> Result<EmulatorBackendEvent, String> {
        if !self.loaded {
            return Err("no ROM is loaded".into());
        }
        // SAFETY: A game is loaded and all callbacks are installed.
        unsafe { (self.api.run)() };
        frame_event(self.viewport)
    }

    fn command(&mut self, command: EmulatorBackendCommand) -> EmulatorBackendEvent {
        let result = match command {
            EmulatorBackendCommand::Initialize { rom, .. }
            | EmulatorBackendCommand::ReloadRom { rom, .. } => self
                .load_rom(rom)
                .map(|()| EmulatorBackendEvent::Active(true)),
            EmulatorBackendCommand::SetPauseMode(mode) => {
                self.pause = mode;
                Ok(EmulatorBackendEvent::Acknowledged)
            }
            EmulatorBackendCommand::StepFrame => self.run_frame(),
            EmulatorBackendCommand::SetViewport(viewport) => {
                self.viewport = Some(viewport);
                Ok(EmulatorBackendEvent::Viewport(viewport))
            }
            EmulatorBackendCommand::Stop => {
                if self.loaded {
                    // SAFETY: `loaded` records one successful matching load.
                    unsafe { (self.api.unload_game)() };
                    self.loaded = false;
                }
                self.rom.clear();
                Ok(EmulatorBackendEvent::Active(false))
            }
            EmulatorBackendCommand::LoadLevel(_)
            | EmulatorBackendCommand::ReloadSprites(_)
            | EmulatorBackendCommand::SetFlags(_) => {
                Err("backend does not advertise this capability".into())
            }
        };
        result.unwrap_or_else(EmulatorBackendEvent::Error)
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // SAFETY: The booleans retain the balanced libretro lifecycle calls.
        unsafe {
            if self.loaded {
                (self.api.unload_game)();
            }
            if self.initialized {
                (self.api.deinit)();
            }
        }
    }
}

fn frame_event(viewport: Option<EmulatorViewport>) -> Result<EmulatorBackendEvent, String> {
    let video = VIDEO
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "video callback state is poisoned".to_string())?;
    if video.bytes.is_empty() {
        return Err("libretro core did not publish a video frame".into());
    }
    let bounds = viewport.unwrap_or(EmulatorViewport {
        x: 0,
        y: 0,
        width: video.width,
        height: video.height,
    });
    let width = bounds.width.min(video.width);
    let height = bounds.height.min(video.height);
    let origin_x = u32::try_from(bounds.x.max(0)).unwrap_or(0).min(video.width);
    let origin_y = u32::try_from(bounds.y.max(0))
        .unwrap_or(0)
        .min(video.height);
    let width = width.min(video.width - origin_x);
    let height = height.min(video.height - origin_y);
    if width == 0 || height == 0 {
        return Err("viewport does not intersect the libretro frame".into());
    }
    let pixel_bytes = if video.pixel_format == 1 { 4 } else { 2 };
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in origin_y..origin_y + height {
        for x in origin_x..origin_x + width {
            let offset = y as usize * video.pitch + x as usize * pixel_bytes;
            if video.pixel_format == 1 {
                let value = u32::from_ne_bytes(
                    video.bytes[offset..offset + 4]
                        .try_into()
                        .map_err(|_| "invalid XRGB8888 frame pitch".to_string())?,
                );
                rgba.extend_from_slice(&[
                    u8::try_from((value >> 16) & 0xff).expect("masked channel fits u8"),
                    u8::try_from((value >> 8) & 0xff).expect("masked channel fits u8"),
                    u8::try_from(value & 0xff).expect("masked channel fits u8"),
                    0xff,
                ]);
            } else {
                let value = u16::from_ne_bytes(
                    video.bytes[offset..offset + 2]
                        .try_into()
                        .map_err(|_| "invalid 16-bit frame pitch".to_string())?,
                );
                let (red, green, blue) = if video.pixel_format == 2 {
                    ((value >> 11) & 31, (value >> 5) & 63, value & 31)
                } else {
                    ((value >> 10) & 31, (value >> 5) & 31, value & 31)
                };
                rgba.extend_from_slice(&[
                    u8::try_from(red * 255 / 31).expect("scaled red channel fits u8"),
                    u8::try_from(green * 255 / if video.pixel_format == 2 { 63 } else { 31 })
                        .expect("scaled green channel fits u8"),
                    u8::try_from(blue * 255 / 31).expect("scaled blue channel fits u8"),
                    0xff,
                ]);
            }
        }
    }
    Ok(EmulatorBackendEvent::Frame {
        width,
        height,
        rgba,
    })
}

fn read_record(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0_u8; 12];
    let mut read = 0;
    while read < header.len() {
        match reader.read(&mut header[read..])? {
            0 if read == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated protocol header",
                ));
            }
            count => read += count,
        }
    }
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if length > MAX_PROTOCOL_RECORD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol record exceeds limit",
        ));
    }
    let mut record = Vec::with_capacity(12 + length);
    record.extend_from_slice(&header);
    record.resize(12 + length, 0);
    reader.read_exact(&mut record[12..])?;
    Ok(Some(record))
}

fn write_event(writer: &mut impl Write, event: &EmulatorBackendEvent) -> Result<(), String> {
    let bytes = event.encode().map_err(|error| error.to_string())?;
    writer
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn run(core: &Path) -> Result<(), String> {
    let mut backend = Backend::new(core)?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    write_event(
        &mut writer,
        &EmulatorBackendEvent::Ready {
            capabilities: CAPABILITIES,
        },
    )?;
    while let Some(record) = read_record(&mut reader).map_err(|error| error.to_string())? {
        let event = match EmulatorBackendCommand::decode(&record) {
            Ok(command) => backend.command(command),
            Err(error) => EmulatorBackendEvent::Error(error.to_string()),
        };
        write_event(&mut writer, &event)?;
    }
    Ok(())
}

fn main() {
    let mut arguments = std::env::args_os();
    let _program = arguments.next();
    let Some(core) = arguments.next() else {
        eprintln!("usage: lm-libretro LIBRETRO_CORE");
        std::process::exit(2);
    };
    if arguments.next().is_some() {
        eprintln!("usage: lm-libretro LIBRETRO_CORE");
        std::process::exit(2);
    }
    if let Err(error) = run(Path::new(&core)) {
        eprintln!("lm-libretro: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_reader_accepts_one_frame_and_rejects_truncation_and_excess() {
        let record = EmulatorBackendCommand::StepFrame.encode().unwrap();
        assert_eq!(
            read_record(&mut record.as_slice()).unwrap(),
            Some(record.clone())
        );
        assert!(read_record(&mut record[..11].as_ref()).is_err());
        let mut excessive = *b"LMEMU001\0\0\0\0";
        excessive[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_record(&mut excessive.as_ref()).is_err());
    }

    #[test]
    fn event_writer_uses_the_shared_exact_framing() {
        let event = EmulatorBackendEvent::Ready {
            capabilities: CAPABILITIES,
        };
        let mut output = Vec::new();
        write_event(&mut output, &event).unwrap();
        assert_eq!(EmulatorBackendEvent::decode(&output).unwrap(), event);
    }

    #[test]
    fn missing_core_is_reported_without_starting_a_session() {
        let error = match Backend::new(Path::new("definitely-missing-libretro-core")) {
            Ok(_) => panic!("missing core unexpectedly loaded"),
            Err(error) => error,
        };
        assert!(error.contains("could not load libretro core"));
    }

    #[test]
    fn protocol_errors_are_safe_backend_diagnostics() {
        let error = lm_app::EmulatorProtocolError::BadMagic.to_string();
        let encoded = EmulatorBackendEvent::Error(error.clone()).encode().unwrap();
        assert_eq!(
            EmulatorBackendEvent::decode(&encoded).unwrap(),
            EmulatorBackendEvent::Error(error)
        );
    }
}
