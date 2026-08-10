//! Bounded native playback for audio frames emitted by the isolated libretro backend.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_QUEUED_SECONDS: usize = 2;

#[derive(Default)]
struct PlaybackBuffer {
    samples: VecDeque<i16>,
    source_rate: u32,
    current: Option<(i16, i16)>,
    next: Option<(i16, i16)>,
    phase: f64,
}

impl PlaybackBuffer {
    fn clear(&mut self) {
        self.samples.clear();
        self.current = None;
        self.next = None;
        self.phase = 0.0;
    }

    fn pop_stereo(&mut self) -> Option<(i16, i16)> {
        let left = self.samples.pop_front()?;
        Some((left, self.samples.pop_front().unwrap_or(left)))
    }

    fn output_frame(&mut self, output_rate: u32) -> (i16, i16) {
        if self.current.is_none() {
            self.current = self.pop_stereo();
            self.next = self.pop_stereo();
        }
        let (Some(current), Some(next)) = (self.current, self.next) else {
            self.current = None;
            self.next = None;
            self.phase = 0.0;
            return (0, 0);
        };
        let interpolate = |from: i16, to: i16| {
            (f64::from(from) + (f64::from(to) - f64::from(from)) * self.phase)
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        };
        let output = (
            interpolate(current.0, next.0),
            interpolate(current.1, next.1),
        );
        self.phase += f64::from(self.source_rate) / f64::from(output_rate.max(1));
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.current = self.next;
            self.next = self.pop_stereo();
            if self.next.is_none() {
                break;
            }
        }
        output
    }
}

pub(crate) struct LiveAudio {
    stream: Option<cpal::Stream>,
    queue: Arc<Mutex<PlaybackBuffer>>,
    error: Arc<Mutex<Option<String>>>,
    sample_rate: Option<u32>,
    muted: bool,
}

impl Default for LiveAudio {
    fn default() -> Self {
        Self {
            stream: None,
            queue: Arc::new(Mutex::new(PlaybackBuffer::default())),
            error: Arc::new(Mutex::new(None)),
            sample_rate: None,
            muted: false,
        }
    }
}

impl LiveAudio {
    pub(crate) fn push(&mut self, sample_rate: u32, samples: &[i16]) -> Result<(), String> {
        if samples.len() % 2 != 0 {
            return Err("live emulator audio must contain interleaved stereo pairs".into());
        }
        if self.muted || samples.is_empty() {
            return Ok(());
        }
        if self.sample_rate != Some(sample_rate) {
            self.rebuild(sample_rate)?;
        }
        let maximum = usize::try_from(sample_rate)
            .ok()
            .and_then(|rate| rate.checked_mul(2 * MAX_QUEUED_SECONDS))
            .ok_or_else(|| "live emulator audio queue size overflow".to_string())?;
        let mut queue = self
            .queue
            .lock()
            .map_err(|_| "live emulator audio queue is poisoned".to_string())?;
        queue.source_rate = sample_rate;
        // Latency is preferable to an unbounded backlog. Drop the oldest complete stereo frames
        // if UI scheduling temporarily delivers more than two seconds of samples.
        let excess = queue
            .samples
            .len()
            .saturating_add(samples.len())
            .saturating_sub(maximum);
        let discard = excess.saturating_add(1) & !1;
        for _ in 0..discard.min(queue.samples.len()) {
            let _ = queue.samples.pop_front();
        }
        queue.samples.extend(samples.iter().copied());
        Ok(())
    }

    pub(crate) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if muted {
            self.clear();
        }
    }

    pub(crate) const fn muted(&self) -> bool {
        self.muted
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.clear();
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stream = None;
        self.sample_rate = None;
        self.clear();
    }

    pub(crate) fn take_error(&self) -> Option<String> {
        self.error.lock().ok()?.take()
    }

    fn rebuild(&mut self, sample_rate: u32) -> Result<(), String> {
        self.stop();
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default audio output device is available".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("could not query the default audio output format: {error}"))?;
        if supported.channels() < 2 {
            return Err("default audio output does not provide stereo channels".into());
        }
        let sample_format = supported.sample_format();
        let config = supported.config();
        let channels = usize::from(config.channels);
        let output_rate = config.sample_rate.0;
        let queue = Arc::clone(&self.queue);
        let error_state = Arc::clone(&self.error);
        let error_callback = move |error| {
            if let Ok(mut state) = error_state.lock() {
                *state = Some(format!("live emulator audio output failed: {error}"));
            }
        };
        let stream = match sample_format {
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |output: &mut [i16], _| fill_i16(output, channels, output_rate, &queue),
                error_callback,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                move |output: &mut [u16], _| fill_u16(output, channels, output_rate, &queue),
                error_callback,
                None,
            ),
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |output: &mut [f32], _| fill_f32(output, channels, output_rate, &queue),
                error_callback,
                None,
            ),
            format => return Err(format!("unsupported audio output sample format {format:?}")),
        }
        .map_err(|error| format!("could not create live emulator audio stream: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("could not start live emulator audio stream: {error}"))?;
        self.stream = Some(stream);
        self.sample_rate = Some(sample_rate);
        Ok(())
    }
}

fn fill_i16(
    output: &mut [i16],
    channels: usize,
    output_rate: u32,
    queue: &Arc<Mutex<PlaybackBuffer>>,
) {
    fill(output, channels, output_rate, queue, |sample| sample);
}

fn fill_u16(
    output: &mut [u16],
    channels: usize,
    output_rate: u32,
    queue: &Arc<Mutex<PlaybackBuffer>>,
) {
    fill(output, channels, output_rate, queue, |sample| {
        (i32::from(sample) + 32_768) as u16
    });
}

fn fill_f32(
    output: &mut [f32],
    channels: usize,
    output_rate: u32,
    queue: &Arc<Mutex<PlaybackBuffer>>,
) {
    fill(output, channels, output_rate, queue, |sample| {
        f32::from(sample) / 32_768.0
    });
}

fn fill<T: Copy>(
    output: &mut [T],
    channels: usize,
    output_rate: u32,
    queue: &Arc<Mutex<PlaybackBuffer>>,
    convert: impl Fn(i16) -> T,
) {
    let Ok(mut queue) = queue.lock() else {
        return;
    };
    for frame in output.chunks_mut(channels.max(1)) {
        let (left, right) = queue.output_frame(output_rate);
        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = convert(match channel {
                0 => left,
                1 => right,
                _ => ((i32::from(left) + i32::from(right)) / 2) as i16,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converters_preserve_stereo_order_and_fill_extra_channels_with_the_average() {
        let queue = Arc::new(Mutex::new(PlaybackBuffer {
            samples: VecDeque::from([i16::MIN, i16::MAX, i16::MIN, i16::MAX]),
            source_rate: 48_000,
            ..Default::default()
        }));
        let mut output = [0_i16; 4];
        fill_i16(&mut output, 4, 48_000, &queue);
        assert_eq!(output, [i16::MIN, i16::MAX, 0, 0]);
    }

    #[test]
    fn empty_queue_outputs_silence_in_every_supported_format() {
        let queue = Arc::new(Mutex::new(PlaybackBuffer {
            source_rate: 48_000,
            ..Default::default()
        }));
        let mut signed = [1_i16; 2];
        let mut unsigned = [0_u16; 2];
        let mut float = [1.0_f32; 2];
        fill_i16(&mut signed, 2, 48_000, &queue);
        fill_u16(&mut unsigned, 2, 48_000, &queue);
        fill_f32(&mut float, 2, 48_000, &queue);
        assert_eq!(signed, [0, 0]);
        assert_eq!(unsigned, [32_768, 32_768]);
        assert_eq!(float, [0.0, 0.0]);
    }

    #[test]
    fn resampler_preserves_duration_when_device_rate_differs_from_snes_rate() {
        let queue = Arc::new(Mutex::new(PlaybackBuffer {
            samples: VecDeque::from([0, 0, 1_000, -1_000, 2_000, -2_000, 3_000, -3_000]),
            source_rate: 32_000,
            ..Default::default()
        }));
        let mut output = [0_i16; 8];
        fill_i16(&mut output, 2, 48_000, &queue);
        assert_eq!(output, [0, 0, 667, -667, 1_333, -1_333, 2_000, -2_000]);
    }

    #[test]
    #[ignore = "requires a native stereo audio output device"]
    fn native_output_stream_accepts_snes_rate_audio_and_mute_clears_it() {
        let mut audio = LiveAudio::default();
        let samples = (0..533)
            .flat_map(|index| {
                let sample = ((index * 97) as i16).wrapping_sub(16_000);
                [sample, sample.wrapping_neg()]
            })
            .collect::<Vec<_>>();
        audio.push(32_040, &samples).unwrap();
        assert!(audio.stream.is_some());
        assert_eq!(audio.sample_rate, Some(32_040));
        audio.set_muted(true);
        assert!(audio.muted());
        assert!(audio.queue.lock().unwrap().samples.is_empty());
        audio.stop();
        assert!(audio.stream.is_none());
    }
}
