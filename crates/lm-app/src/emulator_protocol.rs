//! Bounded binary transport for a live emulator backend process.

use crate::EmulatorPauseMode;

const MAGIC: &[u8; 8] = b"LMEMU001";
pub const MAX_EMULATOR_ROM_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_EMULATOR_SPRITE_BYTES: usize = 1024 * 1024;
pub const MAX_EMULATOR_FRAME_WIDTH: u32 = 512;
pub const MAX_EMULATOR_FRAME_HEIGHT: u32 = 478;
pub const MAX_EMULATOR_AUDIO_FRAMES: usize = 8192;
pub const MIN_EMULATOR_AUDIO_RATE: u32 = 8_000;
pub const MAX_EMULATOR_AUDIO_RATE: u32 = 384_000;
pub const EMULATOR_JOYPAD_B: u16 = 1 << 0;
pub const EMULATOR_JOYPAD_Y: u16 = 1 << 1;
pub const EMULATOR_JOYPAD_SELECT: u16 = 1 << 2;
pub const EMULATOR_JOYPAD_START: u16 = 1 << 3;
pub const EMULATOR_JOYPAD_UP: u16 = 1 << 4;
pub const EMULATOR_JOYPAD_DOWN: u16 = 1 << 5;
pub const EMULATOR_JOYPAD_LEFT: u16 = 1 << 6;
pub const EMULATOR_JOYPAD_RIGHT: u16 = 1 << 7;
pub const EMULATOR_JOYPAD_A: u16 = 1 << 8;
pub const EMULATOR_JOYPAD_X: u16 = 1 << 9;
pub const EMULATOR_JOYPAD_L: u16 = 1 << 10;
pub const EMULATOR_JOYPAD_R: u16 = 1 << 11;
pub const EMULATOR_JOYPAD_ALL: u16 = (1 << 12) - 1;
const MAX_ERROR_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmulatorViewport {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmulatorRuntimeState {
    pub game_mode: u8,
    /// Lunar Magic's two-byte current sublevel slot (`$010B/$010C`). Vanilla SMW leaves it unset.
    pub sublevel: u16,
    /// Vanilla SMW's current overworld translevel slot (`$13BF`).
    pub translevel: u8,
    pub camera_x: u16,
    pub camera_y: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmulatorBackendCommand {
    Initialize {
        revision: u64,
        level: u16,
        flags: u8,
        rom: Vec<u8>,
        sprites: Vec<u8>,
    },
    ReloadRom {
        revision: u64,
        rom: Vec<u8>,
    },
    LoadLevel(u16),
    ReloadSprites(Vec<u8>),
    SetPauseMode(EmulatorPauseMode),
    StepFrame,
    SetFlags(u8),
    SetViewport(EmulatorViewport),
    SetJoypad(u16),
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmulatorBackendEvent {
    Ready {
        capabilities: u32,
    },
    Acknowledged,
    Active(bool),
    Viewport(EmulatorViewport),
    Frame {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    RuntimeFrame {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        state: EmulatorRuntimeState,
    },
    RuntimeFrameAudio {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        state: EmulatorRuntimeState,
        sample_rate: u32,
        /// Interleaved signed 16-bit stereo samples (left, right).
        audio: Vec<i16>,
    },
    Error(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmulatorProtocolError {
    BadMagic,
    Truncated,
    TrailingBytes,
    UnknownTag(u8),
    InvalidPauseMode(u8),
    RomTooLarge(usize),
    SpriteDataTooLarge(usize),
    InvalidViewport,
    InvalidFrame,
    InvalidAudio,
    InvalidJoypad(u16),
    InvalidUtf8,
    ErrorMessageTooLarge(usize),
    LengthOverflow,
}

impl std::fmt::Display for EmulatorProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "live-emulator protocol error: {self:?}")
    }
}

impl std::error::Error for EmulatorProtocolError {}

impl EmulatorBackendCommand {
    /// Encodes one self-framed command after validating every variable-sized field.
    pub fn encode(&self) -> Result<Vec<u8>, EmulatorProtocolError> {
        let mut payload = Vec::new();
        match self {
            Self::Initialize {
                revision,
                level,
                flags,
                rom,
                sprites,
            } => {
                validate_rom(rom)?;
                validate_sprites(sprites)?;
                payload.push(0);
                payload.extend_from_slice(&revision.to_le_bytes());
                payload.extend_from_slice(&level.to_le_bytes());
                payload.push(*flags);
                put_bytes(&mut payload, rom)?;
                put_bytes(&mut payload, sprites)?;
            }
            Self::ReloadRom { revision, rom } => {
                validate_rom(rom)?;
                payload.push(1);
                payload.extend_from_slice(&revision.to_le_bytes());
                put_bytes(&mut payload, rom)?;
            }
            Self::LoadLevel(level) => {
                payload.push(2);
                payload.extend_from_slice(&level.to_le_bytes());
            }
            Self::ReloadSprites(sprites) => {
                validate_sprites(sprites)?;
                payload.push(3);
                put_bytes(&mut payload, sprites)?;
            }
            Self::SetPauseMode(mode) => payload.extend_from_slice(&[4, *mode as u8]),
            Self::StepFrame => payload.push(5),
            Self::SetFlags(flags) => payload.extend_from_slice(&[6, *flags]),
            Self::SetViewport(viewport) => {
                validate_viewport(*viewport)?;
                payload.push(7);
                put_viewport(&mut payload, *viewport);
            }
            Self::Stop => payload.push(8),
            Self::SetJoypad(buttons) => {
                validate_joypad(*buttons)?;
                payload.push(9);
                payload.extend_from_slice(&buttons.to_le_bytes());
            }
        }
        frame(&payload)
    }

    /// Decodes exactly one self-framed command.
    pub fn decode(bytes: &[u8]) -> Result<Self, EmulatorProtocolError> {
        let payload = unframe(bytes)?;
        let mut input = Input::new(payload);
        let command = match input.byte()? {
            0 => Self::Initialize {
                revision: input.u64()?,
                level: input.u16()?,
                flags: input.byte()?,
                rom: input.bytes(MAX_EMULATOR_ROM_BYTES, true)?,
                sprites: input.bytes(MAX_EMULATOR_SPRITE_BYTES, false)?,
            },
            1 => Self::ReloadRom {
                revision: input.u64()?,
                rom: input.bytes(MAX_EMULATOR_ROM_BYTES, true)?,
            },
            2 => Self::LoadLevel(input.u16()?),
            3 => Self::ReloadSprites(input.bytes(MAX_EMULATOR_SPRITE_BYTES, false)?),
            4 => Self::SetPauseMode(match input.byte()? {
                0 => EmulatorPauseMode::Running,
                1 => EmulatorPauseMode::SoftPaused,
                2 => EmulatorPauseMode::HardPaused,
                value => return Err(EmulatorProtocolError::InvalidPauseMode(value)),
            }),
            5 => Self::StepFrame,
            6 => Self::SetFlags(input.byte()?),
            7 => {
                let viewport = input.viewport()?;
                validate_viewport(viewport)?;
                Self::SetViewport(viewport)
            }
            8 => Self::Stop,
            9 => {
                let buttons = input.u16()?;
                validate_joypad(buttons)?;
                Self::SetJoypad(buttons)
            }
            tag => return Err(EmulatorProtocolError::UnknownTag(tag)),
        };
        input.finish()?;
        Ok(command)
    }
}

impl EmulatorBackendEvent {
    pub fn encode(&self) -> Result<Vec<u8>, EmulatorProtocolError> {
        let mut payload = Vec::new();
        match self {
            Self::Ready { capabilities } => {
                payload.push(0x80);
                payload.extend_from_slice(&capabilities.to_le_bytes());
            }
            Self::Acknowledged => payload.push(0x81),
            Self::Active(active) => payload.extend_from_slice(&[0x82, u8::from(*active)]),
            Self::Viewport(viewport) => {
                validate_viewport(*viewport)?;
                payload.push(0x83);
                put_viewport(&mut payload, *viewport);
            }
            Self::Frame {
                width,
                height,
                rgba,
            } => {
                validate_frame(*width, *height, rgba)?;
                payload.push(0x84);
                payload.extend_from_slice(&width.to_le_bytes());
                payload.extend_from_slice(&height.to_le_bytes());
                put_bytes(&mut payload, rgba)?;
            }
            Self::RuntimeFrame {
                width,
                height,
                rgba,
                state,
            } => {
                validate_frame(*width, *height, rgba)?;
                payload.push(0x85);
                payload.extend_from_slice(&width.to_le_bytes());
                payload.extend_from_slice(&height.to_le_bytes());
                put_bytes(&mut payload, rgba)?;
                payload.push(state.game_mode);
                payload.extend_from_slice(&state.sublevel.to_le_bytes());
                payload.push(state.translevel);
                payload.extend_from_slice(&state.camera_x.to_le_bytes());
                payload.extend_from_slice(&state.camera_y.to_le_bytes());
            }
            Self::RuntimeFrameAudio {
                width,
                height,
                rgba,
                state,
                sample_rate,
                audio,
            } => {
                validate_frame(*width, *height, rgba)?;
                validate_audio(*sample_rate, audio)?;
                payload.push(0x86);
                payload.extend_from_slice(&width.to_le_bytes());
                payload.extend_from_slice(&height.to_le_bytes());
                put_bytes(&mut payload, rgba)?;
                payload.push(state.game_mode);
                payload.extend_from_slice(&state.sublevel.to_le_bytes());
                payload.push(state.translevel);
                payload.extend_from_slice(&state.camera_x.to_le_bytes());
                payload.extend_from_slice(&state.camera_y.to_le_bytes());
                payload.extend_from_slice(&sample_rate.to_le_bytes());
                put_i16s(&mut payload, audio)?;
            }
            Self::Error(message) => {
                if message.len() > MAX_ERROR_BYTES {
                    return Err(EmulatorProtocolError::ErrorMessageTooLarge(message.len()));
                }
                payload.push(0xff);
                put_bytes(&mut payload, message.as_bytes())?;
            }
        }
        frame(&payload)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, EmulatorProtocolError> {
        let payload = unframe(bytes)?;
        let mut input = Input::new(payload);
        let event = match input.byte()? {
            0x80 => Self::Ready {
                capabilities: input.u32()?,
            },
            0x81 => Self::Acknowledged,
            0x82 => Self::Active(match input.byte()? {
                0 => false,
                1 => true,
                _ => return Err(EmulatorProtocolError::InvalidFrame),
            }),
            0x83 => {
                let viewport = input.viewport()?;
                validate_viewport(viewport)?;
                Self::Viewport(viewport)
            }
            0x84 => {
                let width = input.u32()?;
                let height = input.u32()?;
                let rgba = input.bytes(max_frame_bytes()?, false)?;
                validate_frame(width, height, &rgba)?;
                Self::Frame {
                    width,
                    height,
                    rgba,
                }
            }
            0x85 => {
                let width = input.u32()?;
                let height = input.u32()?;
                let rgba = input.bytes(max_frame_bytes()?, false)?;
                validate_frame(width, height, &rgba)?;
                Self::RuntimeFrame {
                    width,
                    height,
                    rgba,
                    state: EmulatorRuntimeState {
                        game_mode: input.byte()?,
                        sublevel: input.u16()?,
                        translevel: input.byte()?,
                        camera_x: input.u16()?,
                        camera_y: input.u16()?,
                    },
                }
            }
            0x86 => {
                let width = input.u32()?;
                let height = input.u32()?;
                let rgba = input.bytes(max_frame_bytes()?, false)?;
                validate_frame(width, height, &rgba)?;
                let state = EmulatorRuntimeState {
                    game_mode: input.byte()?,
                    sublevel: input.u16()?,
                    translevel: input.byte()?,
                    camera_x: input.u16()?,
                    camera_y: input.u16()?,
                };
                let sample_rate = input.u32()?;
                let audio = input.i16s(MAX_EMULATOR_AUDIO_FRAMES * 2)?;
                validate_audio(sample_rate, &audio)?;
                Self::RuntimeFrameAudio {
                    width,
                    height,
                    rgba,
                    state,
                    sample_rate,
                    audio,
                }
            }
            0xff => {
                let message = input.bytes(MAX_ERROR_BYTES, false)?;
                Self::Error(
                    String::from_utf8(message).map_err(|_| EmulatorProtocolError::InvalidUtf8)?,
                )
            }
            tag => return Err(EmulatorProtocolError::UnknownTag(tag)),
        };
        input.finish()?;
        Ok(event)
    }
}

fn validate_rom(bytes: &[u8]) -> Result<(), EmulatorProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_EMULATOR_ROM_BYTES {
        return Err(EmulatorProtocolError::RomTooLarge(bytes.len()));
    }
    Ok(())
}

fn validate_sprites(bytes: &[u8]) -> Result<(), EmulatorProtocolError> {
    if bytes.len() > MAX_EMULATOR_SPRITE_BYTES {
        return Err(EmulatorProtocolError::SpriteDataTooLarge(bytes.len()));
    }
    Ok(())
}

fn validate_viewport(viewport: EmulatorViewport) -> Result<(), EmulatorProtocolError> {
    if viewport.width == 0
        || viewport.height == 0
        || viewport.width > MAX_EMULATOR_FRAME_WIDTH
        || viewport.height > MAX_EMULATOR_FRAME_HEIGHT
    {
        return Err(EmulatorProtocolError::InvalidViewport);
    }
    Ok(())
}

fn max_frame_bytes() -> Result<usize, EmulatorProtocolError> {
    usize::try_from(MAX_EMULATOR_FRAME_WIDTH)
        .ok()
        .and_then(|width| {
            usize::try_from(MAX_EMULATOR_FRAME_HEIGHT)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(EmulatorProtocolError::LengthOverflow)
}

fn validate_frame(width: u32, height: u32, rgba: &[u8]) -> Result<(), EmulatorProtocolError> {
    if width == 0
        || height == 0
        || width > MAX_EMULATOR_FRAME_WIDTH
        || height > MAX_EMULATOR_FRAME_HEIGHT
        || usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            != Some(rgba.len())
    {
        return Err(EmulatorProtocolError::InvalidFrame);
    }
    Ok(())
}

fn validate_audio(sample_rate: u32, samples: &[i16]) -> Result<(), EmulatorProtocolError> {
    if !(MIN_EMULATOR_AUDIO_RATE..=MAX_EMULATOR_AUDIO_RATE).contains(&sample_rate)
        || samples.len() > MAX_EMULATOR_AUDIO_FRAMES * 2
        || samples.len() % 2 != 0
    {
        return Err(EmulatorProtocolError::InvalidAudio);
    }
    Ok(())
}

fn validate_joypad(buttons: u16) -> Result<(), EmulatorProtocolError> {
    if buttons & !EMULATOR_JOYPAD_ALL != 0 {
        return Err(EmulatorProtocolError::InvalidJoypad(buttons));
    }
    Ok(())
}

fn put_viewport(output: &mut Vec<u8>, viewport: EmulatorViewport) {
    output.extend_from_slice(&viewport.x.to_le_bytes());
    output.extend_from_slice(&viewport.y.to_le_bytes());
    output.extend_from_slice(&viewport.width.to_le_bytes());
    output.extend_from_slice(&viewport.height.to_le_bytes());
}

fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EmulatorProtocolError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EmulatorProtocolError::LengthOverflow)?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_i16s(output: &mut Vec<u8>, samples: &[i16]) -> Result<(), EmulatorProtocolError> {
    let length = u32::try_from(samples.len()).map_err(|_| EmulatorProtocolError::LengthOverflow)?;
    output.extend_from_slice(&length.to_le_bytes());
    for sample in samples {
        output.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(())
}

fn frame(payload: &[u8]) -> Result<Vec<u8>, EmulatorProtocolError> {
    let length = u32::try_from(payload.len()).map_err(|_| EmulatorProtocolError::LengthOverflow)?;
    let mut output = Vec::with_capacity(MAGIC.len() + 4 + payload.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(payload);
    Ok(output)
}

fn unframe(bytes: &[u8]) -> Result<&[u8], EmulatorProtocolError> {
    if bytes.len() < MAGIC.len() + 4 {
        return Err(EmulatorProtocolError::Truncated);
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(EmulatorProtocolError::BadMagic);
    }
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let end = 12usize
        .checked_add(length)
        .ok_or(EmulatorProtocolError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(EmulatorProtocolError::Truncated);
    }
    if end != bytes.len() {
        return Err(EmulatorProtocolError::TrailingBytes);
    }
    Ok(&bytes[12..end])
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], EmulatorProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(EmulatorProtocolError::LengthOverflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(EmulatorProtocolError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, EmulatorProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, EmulatorProtocolError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, EmulatorProtocolError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, EmulatorProtocolError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn i16s(&mut self, maximum: usize) -> Result<Vec<i16>, EmulatorProtocolError> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(EmulatorProtocolError::InvalidAudio);
        }
        let byte_length = length
            .checked_mul(2)
            .ok_or(EmulatorProtocolError::LengthOverflow)?;
        Ok(self
            .take(byte_length)?
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()))
            .collect())
    }

    fn bytes(&mut self, maximum: usize, rom: bool) -> Result<Vec<u8>, EmulatorProtocolError> {
        let length = self.u32()? as usize;
        if length > maximum || (rom && length == 0) {
            return Err(if rom {
                EmulatorProtocolError::RomTooLarge(length)
            } else {
                EmulatorProtocolError::SpriteDataTooLarge(length)
            });
        }
        Ok(self.take(length)?.to_vec())
    }

    fn viewport(&mut self) -> Result<EmulatorViewport, EmulatorProtocolError> {
        Ok(EmulatorViewport {
            x: i32::from_le_bytes(self.take(4)?.try_into().unwrap()),
            y: i32::from_le_bytes(self.take(4)?.try_into().unwrap()),
            width: self.u32()?,
            height: self.u32()?,
        })
    }

    fn finish(self) -> Result<(), EmulatorProtocolError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(EmulatorProtocolError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<EmulatorBackendCommand> {
        vec![
            EmulatorBackendCommand::Initialize {
                revision: u64::MAX,
                level: 0x1ff,
                flags: 0x0f,
                rom: vec![1, 2, 3],
                sprites: vec![4, 5],
            },
            EmulatorBackendCommand::ReloadRom {
                revision: 9,
                rom: vec![6],
            },
            EmulatorBackendCommand::LoadLevel(0x105),
            EmulatorBackendCommand::ReloadSprites(vec![7, 8]),
            EmulatorBackendCommand::SetPauseMode(EmulatorPauseMode::Running),
            EmulatorBackendCommand::SetPauseMode(EmulatorPauseMode::SoftPaused),
            EmulatorBackendCommand::SetPauseMode(EmulatorPauseMode::HardPaused),
            EmulatorBackendCommand::StepFrame,
            EmulatorBackendCommand::SetFlags(0x0f),
            EmulatorBackendCommand::SetViewport(EmulatorViewport {
                x: -32,
                y: 64,
                width: 256,
                height: 224,
            }),
            EmulatorBackendCommand::Stop,
            EmulatorBackendCommand::SetJoypad(EMULATOR_JOYPAD_B | EMULATOR_JOYPAD_RIGHT),
        ]
    }

    #[test]
    fn every_command_round_trips_and_every_truncation_rejects() {
        for command in commands() {
            let encoded = command.encode().unwrap();
            assert_eq!(EmulatorBackendCommand::decode(&encoded).unwrap(), command);
            for end in 0..encoded.len() {
                assert!(EmulatorBackendCommand::decode(&encoded[..end]).is_err());
            }
            let mut trailing = encoded;
            trailing.push(0);
            assert_eq!(
                EmulatorBackendCommand::decode(&trailing),
                Err(EmulatorProtocolError::TrailingBytes)
            );
        }
    }

    #[test]
    fn every_event_round_trips_and_every_truncation_rejects() {
        let events = vec![
            EmulatorBackendEvent::Ready {
                capabilities: 0x1234,
            },
            EmulatorBackendEvent::Acknowledged,
            EmulatorBackendEvent::Active(false),
            EmulatorBackendEvent::Active(true),
            EmulatorBackendEvent::Viewport(EmulatorViewport {
                x: i32::MIN,
                y: i32::MAX,
                width: 1,
                height: 1,
            }),
            EmulatorBackendEvent::Frame {
                width: 2,
                height: 1,
                rgba: vec![0, 1, 2, 3, 4, 5, 6, 7],
            },
            EmulatorBackendEvent::RuntimeFrame {
                width: 1,
                height: 1,
                rgba: vec![4, 3, 2, 1],
                state: EmulatorRuntimeState {
                    game_mode: 0x14,
                    sublevel: 0x105,
                    translevel: 0x29,
                    camera_x: 0x1234,
                    camera_y: 0x0056,
                },
            },
            EmulatorBackendEvent::RuntimeFrameAudio {
                width: 1,
                height: 1,
                rgba: vec![9, 8, 7, 6],
                state: EmulatorRuntimeState {
                    game_mode: 0x14,
                    sublevel: 0x106,
                    translevel: 0x2a,
                    camera_x: 0x0040,
                    camera_y: 0x00c0,
                },
                sample_rate: 32_040,
                audio: vec![i16::MIN, i16::MAX, -1, 1],
            },
            EmulatorBackendEvent::Error("backend failed".into()),
        ];
        for event in events {
            let encoded = event.encode().unwrap();
            assert_eq!(EmulatorBackendEvent::decode(&encoded).unwrap(), event);
            for end in 0..encoded.len() {
                assert!(EmulatorBackendEvent::decode(&encoded[..end]).is_err());
            }
        }
    }

    #[test]
    fn variable_inputs_and_geometry_are_bounded_before_publication() {
        assert_eq!(
            EmulatorBackendCommand::ReloadRom {
                revision: 0,
                rom: Vec::new()
            }
            .encode(),
            Err(EmulatorProtocolError::RomTooLarge(0))
        );
        assert!(matches!(
            EmulatorBackendCommand::ReloadSprites(vec![0; MAX_EMULATOR_SPRITE_BYTES + 1]).encode(),
            Err(EmulatorProtocolError::SpriteDataTooLarge(_))
        ));
        assert_eq!(
            EmulatorBackendCommand::SetViewport(EmulatorViewport {
                x: 0,
                y: 0,
                width: 0,
                height: 224
            })
            .encode(),
            Err(EmulatorProtocolError::InvalidViewport)
        );
        assert_eq!(
            EmulatorBackendEvent::Frame {
                width: 2,
                height: 2,
                rgba: vec![0; 15]
            }
            .encode(),
            Err(EmulatorProtocolError::InvalidFrame)
        );
        assert_eq!(
            EmulatorBackendCommand::SetJoypad(0x8000).encode(),
            Err(EmulatorProtocolError::InvalidJoypad(0x8000))
        );
        let audio_event = |sample_rate, audio| EmulatorBackendEvent::RuntimeFrameAudio {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
            state: EmulatorRuntimeState {
                game_mode: 0x14,
                sublevel: 0x105,
                translevel: 0x28,
                camera_x: 0,
                camera_y: 0,
            },
            sample_rate,
            audio,
        };
        assert_eq!(
            audio_event(MIN_EMULATOR_AUDIO_RATE - 1, vec![]).encode(),
            Err(EmulatorProtocolError::InvalidAudio)
        );
        assert_eq!(
            audio_event(32_040, vec![0]).encode(),
            Err(EmulatorProtocolError::InvalidAudio)
        );
        assert_eq!(
            audio_event(32_040, vec![0; MAX_EMULATOR_AUDIO_FRAMES * 2 + 2]).encode(),
            Err(EmulatorProtocolError::InvalidAudio)
        );
    }

    #[test]
    fn malformed_tags_pause_modes_utf8_and_boolean_values_reject() {
        for payload in [[0xfe].as_slice(), &[4, 3]] {
            assert!(EmulatorBackendCommand::decode(&frame(payload).unwrap()).is_err());
        }
        assert!(EmulatorBackendEvent::decode(&frame(&[0x82, 2]).unwrap()).is_err());
        let mut invalid_utf8 = vec![0xff];
        put_bytes(&mut invalid_utf8, &[0xff]).unwrap();
        assert_eq!(
            EmulatorBackendEvent::decode(&frame(&invalid_utf8).unwrap()),
            Err(EmulatorProtocolError::InvalidUtf8)
        );
    }
}
