//! Voice loopback spike: microphone -> Opus encode -> Opus decode -> distance gain and
//! muffle -> speakers. This is the voice pipeline of the game without the network.
//!
//! Run: `cargo run --example voice_loopback -- [seconds]` (default 10 s).
//! Use headphones, or the speakers feed back into the microphone.

use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

/// Opus works internally at 48 kHz; capture is resampled to it so any mic rate works.
const OPUS_RATE: u32 = 48_000;
/// 20 ms is the Opus default for VoIP: 50 packets per second per speaker.
const FRAME_SAMPLES: usize = (OPUS_RATE / 50) as usize;
/// Upper bound from the libopus docs for one packet.
const MAX_PACKET_BYTES: usize = 4_000;
/// Caps playback latency: older audio is dropped instead of piling up.
const MAX_QUEUE_SAMPLES: usize = FRAME_SAMPLES * 10;

#[derive(Default)]
struct Counters {
    captured_samples: AtomicU64,
    frames_encoded: AtomicU64,
    encoded_bytes: AtomicU64,
    played_samples: AtomicU64,
    underrun_samples: AtomicU64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let seconds: u64 = match std::env::args().nth(1) {
        Some(arg) => arg
            .parse()
            .map_err(|e| format!("the duration argument `{arg}` is not a whole number: {e}"))?,
        None => 10,
    };

    let host = cpal::default_host();
    let input = host
        .default_input_device()
        .ok_or("the default host has no default input device")?;
    let output = host
        .default_output_device()
        .ok_or("the default host has no default output device")?;
    let in_cfg = input
        .default_input_config()
        .map_err(|e| format!("the default input device has no default config: {e}"))?;
    let out_cfg = output
        .default_output_config()
        .map_err(|e| format!("the default output device has no default config: {e}"))?;
    println!(
        "input:  {} | {} Hz, {} ch, {:?}",
        describe(&input),
        in_cfg.sample_rate(),
        in_cfg.channels(),
        in_cfg.sample_format()
    );
    println!(
        "output: {} | {} Hz, {} ch, {:?}",
        describe(&output),
        out_cfg.sample_rate(),
        out_cfg.channels(),
        out_cfg.sample_format()
    );
    // Shared-mode WASAPI on Windows hands out f32; other formats are out of scope for a spike.
    if in_cfg.sample_format() != SampleFormat::F32 || out_cfg.sample_format() != SampleFormat::F32 {
        return Err(format!(
            "the sample formats {:?} (input) and {:?} (output) are not f32",
            in_cfg.sample_format(),
            out_cfg.sample_format()
        )
        .into());
    }

    let counters = Arc::new(Counters::default());
    let in_config: StreamConfig = in_cfg.config();
    let out_config: StreamConfig = out_cfg.config();

    // The cpal callbacks run on audio threads; a channel and a locked queue keep the
    // codec work off them, as the game would do.
    let (mic_tx, mic_rx) = mpsc::channel::<Vec<f32>>();
    let in_channels = usize::from(in_config.channels);
    let in_counters = Arc::clone(&counters);
    let in_stream = input.build_input_stream(
        &in_config,
        move |data: &[f32], _| {
            let mono: Vec<f32> = data
                .chunks(in_channels)
                .map(|f| f.iter().sum::<f32>() / f.len() as f32)
                .collect();
            in_counters
                .captured_samples
                .fetch_add(mono.len() as u64, Ordering::Relaxed);
            // A send error means main has ended; the stream is about to drop.
            let _ = mic_tx.send(mono);
        },
        |e| eprintln!("input stream error: {e}"),
        None,
    )?;

    let playback: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let out_channels = usize::from(out_config.channels);
    let out_queue = Arc::clone(&playback);
    let out_counters = Arc::clone(&counters);
    let out_stream = output.build_output_stream(
        &out_config,
        move |data: &mut [f32], _| {
            let Ok(mut queue) = out_queue.lock() else {
                data.fill(0.0);
                return;
            };
            for frame in data.chunks_mut(out_channels) {
                let sample = match queue.pop_front() {
                    Some(s) => {
                        out_counters.played_samples.fetch_add(1, Ordering::Relaxed);
                        s
                    }
                    None => {
                        out_counters
                            .underrun_samples
                            .fetch_add(1, Ordering::Relaxed);
                        0.0
                    }
                };
                frame.fill(sample);
            }
        },
        |e| eprintln!("output stream error: {e}"),
        None,
    )?;

    in_stream.play()?;
    out_stream.play()?;

    let mut encoder = opus::Encoder::new(OPUS_RATE, opus::Channels::Mono, opus::Application::Voip)?;
    let mut decoder = opus::Decoder::new(OPUS_RATE, opus::Channels::Mono)?;
    let mut to_opus = Resampler::new(in_config.sample_rate, OPUS_RATE);
    let mut to_output = Resampler::new(OPUS_RATE, out_config.sample_rate);
    let mut muffle = LowPass::default();
    let mut pending: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES * 2);
    let mut packet = vec![0u8; MAX_PACKET_BYTES];
    let mut decoded = vec![0f32; FRAME_SAMPLES];
    let mut last_rms_db = f32::NEG_INFINITY;

    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    let mut next_log = start + Duration::from_secs(1);
    while Instant::now() < end {
        match mic_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => to_opus.push(&chunk, &mut pending),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("the input stream stopped sending audio".into());
            }
        }

        let t = start.elapsed().as_secs_f32();
        // Sweep a virtual speaker from 1 m to 25 m and back, and wind from calm to storm,
        // so one run exercises the whole gain and muffle range.
        let distance = 1.0 + 24.0 * triangle(t / 8.0);
        let wind = triangle(t / 5.0);

        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
            last_rms_db = rms_dbfs(&frame);
            let len = encoder.encode_float(&frame, &mut packet)?;
            counters.frames_encoded.fetch_add(1, Ordering::Relaxed);
            counters
                .encoded_bytes
                .fetch_add(len as u64, Ordering::Relaxed);

            // Here the packet would cross the network as an unreliable message.
            let n = decoder.decode_float(&packet[..len], &mut decoded, false)?;

            let gain = distance_gain(distance, max_range(wind));
            muffle.set_cutoff(muffle_cutoff_hz(wind), OPUS_RATE);
            let shaped: Vec<f32> = decoded[..n]
                .iter()
                .map(|&s| muffle.process(s) * gain)
                .collect();
            let mut out = Vec::with_capacity(shaped.len() * 2);
            to_output.push(&shaped, &mut out);
            if let Ok(mut queue) = playback.lock() {
                queue.extend(out);
                let excess = queue.len().saturating_sub(MAX_QUEUE_SAMPLES);
                queue.drain(..excess);
            }
        }

        if Instant::now() >= next_log {
            next_log += Duration::from_secs(1);
            let frames = counters.frames_encoded.load(Ordering::Relaxed);
            let bytes = counters.encoded_bytes.load(Ordering::Relaxed);
            println!(
                "t={t:4.1}s captured={} frames={frames} avg_packet={}B played={} underrun={} \
                 rms={last_rms_db:.1}dBFS dist={distance:.1}m wind={wind:.2}",
                counters.captured_samples.load(Ordering::Relaxed),
                bytes.checked_div(frames).unwrap_or(0),
                counters.played_samples.load(Ordering::Relaxed),
                counters.underrun_samples.load(Ordering::Relaxed),
            );
        }
    }
    Ok(())
}

fn describe(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|e| format!("<no name: {e}>"))
}

/// 0 -> 1 -> 0 over one period; drives the demo sweeps.
fn triangle(phase: f32) -> f32 {
    let p = phase.fract();
    1.0 - (2.0 * p - 1.0).abs()
}

/// Storms shrink the hearing range from 30 m to 10 m.
fn max_range(wind: f32) -> f32 {
    30.0 - 20.0 * wind
}

/// Full volume up to 2 m, then a linear fade to silence at `max_range`.
fn distance_gain(distance: f32, max_range: f32) -> f32 {
    let near = 2.0;
    ((max_range - distance) / (max_range - near)).clamp(0.0, 1.0)
}

/// Calm air keeps speech clear (8 kHz); a storm cuts it to 600 Hz.
fn muffle_cutoff_hz(wind: f32) -> f32 {
    8_000.0 * (600.0f32 / 8_000.0).powf(wind)
}

/// Loudness of one frame. The host needs this per speaker for the sleeping passenger.
fn rms_dbfs(frame: &[f32]) -> f32 {
    let mean_sq = frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32;
    10.0 * mean_sq.max(1e-12).log10()
}

/// One-pole low-pass: cheap enough to run per speaker per frame.
#[derive(Default)]
struct LowPass {
    alpha: f32,
    state: f32,
}

impl LowPass {
    fn set_cutoff(&mut self, cutoff_hz: f32, rate: u32) {
        let dt = 1.0 / rate as f32;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        self.alpha = dt / (rc + dt);
    }

    fn process(&mut self, x: f32) -> f32 {
        self.state += self.alpha * (x - self.state);
        self.state
    }
}

/// Linear-interpolation resampler. Good enough for voice; a real build would use a
/// proper filter (e.g. `rubato`).
struct Resampler {
    step: f64,
    pos: f64,
    prev: f32,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Self {
            step: f64::from(from) / f64::from(to),
            pos: 0.0,
            prev: 0.0,
        }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        // `pos` is relative to `prev`, which sits at index -1 before `input`.
        while self.pos < input.len() as f64 {
            let i = self.pos.floor();
            let frac = (self.pos - i) as f32;
            let idx = i as usize;
            let a = if idx == 0 { self.prev } else { input[idx - 1] };
            let b = input[idx];
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= input.len() as f64;
        if let Some(&last) = input.last() {
            self.prev = last;
        }
    }
}
