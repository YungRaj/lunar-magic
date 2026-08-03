//! Lossless title-screen recording and emulator interchange models.

mod recording;
mod recording_file;
mod snes9x;
mod zsnes;

pub use recording::{TitleScreenRecording, TitleScreenRecordingError};
pub use recording_file::TitleScreenRecordingFileError;
pub use snes9x::{Snes9xTitleRecordingError, decode_snes9x_title_recording, decode_snes9x_wram};
pub use zsnes::{
    ZsnesTitleRecordingError, decode_zsnes_title_recording, encode_zsnes_title_recording,
};
