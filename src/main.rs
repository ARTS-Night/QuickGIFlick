#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod recording;
mod timeline;
#[cfg(windows)]
mod windows_ui;

use std::{
    fs::File,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use gif::{Encoder, Frame, Repeat};
use recording::Recording;
use screendelta::{
    CaptureConfig, CaptureSession, CaptureSource, CaptureUpdate, CpuFrame, CursorCapture,
    FramePacer, monitors,
};

const DEFAULT_RECORDING_MEMORY_BYTES: usize = 32 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("QUICKGIFFLICK_BENCH").is_none() {
        #[cfg(windows)]
        return windows_ui::run();
        #[cfg(not(windows))]
        return Err("QuickGIFlick's interactive recorder is Windows-only".into());
    }
    run_recording(default_source()?).map(|_| ())
}

fn default_source() -> Result<CaptureSource, Box<dyn std::error::Error>> {
    let monitor = monitors()?
        .into_iter()
        .next()
        .ok_or("No monitor available")?;
    Ok(CaptureSource::Monitor(monitor.id))
}

pub(crate) fn run_recording(source: CaptureSource) -> Result<PathBuf, Box<dyn std::error::Error>> {
    run_recording_until(source, None)
}

/// Stops capture promptly when the UI sets `stop`, while preserving the actual
/// elapsed timestamp as the recording end time.
pub(crate) fn run_recording_until(
    source: CaptureSource,
    stop: Option<&AtomicBool>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut recording = capture_recording_until(source, stop)?;
    let output = output_path()?;
    let encode_started = Instant::now();
    let mode = GifMode::from_env();
    let quality = GifQuality::from_env();
    let encode = encode_recording(&mut recording, &output, mode, quality)?;
    eprintln!(
        "encode mode={} quality={} wall_ms={:.3} reconstruction_ms={:.3} conversion_ms={:.3} quantization_ms={:.3} encoder_ms={:.3} finalize_ms={:.3} frames={}",
        mode.name(),
        quality.name(),
        encode_started.elapsed().as_secs_f64() * 1_000.0,
        encode.reconstruction.as_secs_f64() * 1_000.0,
        encode.conversion.as_secs_f64() * 1_000.0,
        encode.quantization.as_secs_f64() * 1_000.0,
        encode.encoder.as_secs_f64() * 1_000.0,
        encode.finalize.as_secs_f64() * 1_000.0,
        encode.frames,
    );
    println!("Saved {}", output.display());
    Ok(output)
}

/// Captures a bounded Delta timeline without encoding it. The native review UI
/// owns the resulting timeline and may encode a trimmed range later.
pub(crate) fn capture_recording_until(
    source: CaptureSource,
    stop: Option<&AtomicBool>,
) -> Result<Recording, Box<dyn std::error::Error>> {
    capture_recording_with_cursor(source, stop, cursor_capture())
}

pub(crate) fn capture_recording_with_cursor(
    source: CaptureSource,
    stop: Option<&AtomicBool>,
    cursor: CursorCapture,
) -> Result<Recording, Box<dyn std::error::Error>> {
    let mut capture = CaptureSession::start(CaptureConfig { source, cursor })?;
    let initial = capture.next_frame()?;
    let capture_origin = initial.timestamp();
    let mut recording = Recording::new(initial.into_readback()?, recording_memory_budget())?;
    let recording_started = Instant::now();
    let mut pacer = FramePacer::new(recording_fps())?;
    let seconds = std::env::var("QUICKGIFFLICK_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let deadline = recording_started + Duration::from_secs(seconds);

    while Instant::now() < deadline && !stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        pacer.wait();
        match capture.try_next_update(Duration::ZERO)? {
            CaptureUpdate::Full(frame) => {
                let at = frame.timestamp().saturating_sub(capture_origin);
                recording.append_full(at, frame.into_readback()?)?;
            }
            CaptureUpdate::Delta(update) => {
                let at = update.timestamp.saturating_sub(capture_origin);
                recording.append_delta(at, update.regions)?;
            }
            CaptureUpdate::Unchanged { timestamp, .. } => {
                recording.observe_unchanged(timestamp.saturating_sub(capture_origin));
            }
        }
    }
    recording.finish(recording_started.elapsed());
    eprintln!(
        "recording updates={} resident_payload_bytes={} spilled_payload_bytes={} store_write_ms={:.3} capture_stats={:?}",
        recording.update_len(),
        recording.resident_payload_bytes(),
        recording.spilled_payload_bytes(),
        recording.store_time().as_secs_f64() * 1_000.0,
        capture.stats(),
    );
    Ok(recording)
}

fn recording_memory_budget() -> usize {
    std::env::var("QUICKGIFFLICK_RECORDING_MEMORY_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|megabytes| megabytes.saturating_mul(1024 * 1024))
        .unwrap_or(DEFAULT_RECORDING_MEMORY_BYTES)
}

fn recording_fps() -> u32 {
    recording_fps_from(std::env::var("QUICKGIFFLICK_FPS").ok().as_deref())
}

fn cursor_capture() -> CursorCapture {
    cursor_capture_from(std::env::var("QUICKGIFFLICK_CURSOR").ok().as_deref())
}

fn cursor_capture_from(value: Option<&str>) -> CursorCapture {
    match value {
        Some("hidden") | Some("off") => CursorCapture::Exclude,
        Some("standard") => CursorCapture::System,
        _ => CursorCapture::Include,
    }
}

fn recording_fps_from(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|fps| *fps > 0 && *fps <= 240)
        .unwrap_or(15)
}

fn encode_recording(
    recording: &mut Recording,
    output: &PathBuf,
    mode: GifMode,
    quality: GifQuality,
) -> Result<EncodeStats, Box<dyn std::error::Error>> {
    encode_recording_range(
        recording,
        output,
        mode,
        quality,
        Duration::ZERO,
        recording.end(),
    )
}

/// Encodes the recording range `[start, end]`. The first GIF frame is rebuilt
/// from the Delta timeline at `start`, so trim points between updates retain
/// every unchanged pixel.
pub(crate) fn encode_recording_range(
    recording: &mut Recording,
    output: &PathBuf,
    mode: GifMode,
    quality: GifQuality,
    start: Duration,
    end: Duration,
) -> Result<EncodeStats, Box<dyn std::error::Error>> {
    if start >= end || end > recording.end() {
        return Err("GIF range must satisfy 0 <= start < end <= recording end".into());
    }
    let mut stats = EncodeStats::default();
    let reconstruction_started = Instant::now();
    let mut canvas = recording.canvas_at(start)?;
    stats.reconstruction += reconstruction_started.elapsed();
    let mut file = File::create(output)?;
    let mut encoder = Encoder::new(
        &mut file,
        canvas.frame.width as u16,
        canvas.frame.height as u16,
        &[],
    )?;
    encoder.set_repeat(Repeat::Infinite)?;
    let mut clock = GifClock::default();
    let mut last = start;
    let mut gif_pixels = Vec::with_capacity(canvas.frame.data.len());
    let full = full_bounds(&canvas.frame);
    let mut pending = full;
    for index in 0..recording.update_len() {
        let at = recording.update_time(index);
        if at <= start {
            continue;
        }
        if at > end {
            break;
        }
        write_region(
            &mut encoder,
            &canvas.frame,
            pending,
            clock.advance(at.saturating_sub(last)),
            &mut gif_pixels,
            &mut stats,
            quality,
        )?;
        let reconstruction_started = Instant::now();
        recording.apply_update(index, &mut canvas)?;
        stats.reconstruction += reconstruction_started.elapsed();
        pending = match mode {
            GifMode::Full => full,
            GifMode::Partial => recording.update_bounds(index),
        };
        last = at;
    }
    write_region(
        &mut encoder,
        &canvas.frame,
        pending,
        clock.advance(end.saturating_sub(last)).max(1),
        &mut gif_pixels,
        &mut stats,
        quality,
    )?;
    let finalize_started = Instant::now();
    drop(encoder);
    file.sync_all()?;
    stats.finalize += finalize_started.elapsed();
    Ok(stats)
}

fn write_region(
    encoder: &mut Encoder<&mut File>,
    canvas: &CpuFrame,
    region: screendelta::Region,
    delay: u16,
    rgba: &mut Vec<u8>,
    stats: &mut EncodeStats,
    quality: GifQuality,
) -> Result<(), gif::EncodingError> {
    let conversion_started = Instant::now();
    rgba.clear();
    let x = region.x as usize;
    let y = region.y as usize;
    let row = region.size.width as usize * 4;
    for row_index in y..y + region.size.height as usize {
        let offset = row_index * canvas.stride + x * 4;
        rgba.extend_from_slice(&canvas.data[offset..offset + row]);
    }
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    stats.conversion += conversion_started.elapsed();
    let quantization_started = Instant::now();
    let mut frame = Frame::from_rgba_speed(
        region.size.width as u16,
        region.size.height as u16,
        rgba,
        quality.quantizer_speed(),
    );
    stats.quantization += quantization_started.elapsed();
    frame.delay = delay;
    frame.left = region.x as u16;
    frame.top = region.y as u16;
    let encoder_started = Instant::now();
    encoder.write_frame(&frame)?;
    stats.encoder += encoder_started.elapsed();
    stats.frames += 1;
    Ok(())
}

fn full_bounds(frame: &CpuFrame) -> screendelta::Region {
    screendelta::Region::new(0, 0, frame.width, frame.height).expect("capture frame is nonempty")
}

#[derive(Clone, Copy)]
pub(crate) enum GifMode {
    Full,
    Partial,
}

/// Encoder presets intentionally expose only the quantizer work/size tradeoff.
/// Capture resolution and ScreenDelta transport remain independent decisions.
#[derive(Clone, Copy)]
pub(crate) enum GifQuality {
    Fast,
    Balanced,
    Best,
}

impl GifQuality {
    fn from_env() -> Self {
        match std::env::var("QUICKGIFFLICK_QUALITY").as_deref() {
            Ok("fast") => Self::Fast,
            Ok("best") => Self::Best,
            _ => Self::Balanced,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Best => "best",
        }
    }

    fn quantizer_speed(self) -> i32 {
        match self {
            Self::Fast => 20,
            Self::Balanced => 10,
            Self::Best => 1,
        }
    }
}

impl GifMode {
    fn from_env() -> Self {
        match std::env::var("QUICKGIFFLICK_GIF_MODE").as_deref() {
            Ok("partial") => Self::Partial,
            _ => Self::Full,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }
}

#[derive(Default)]
struct EncodeStats {
    reconstruction: Duration,
    conversion: Duration,
    quantization: Duration,
    encoder: Duration,
    finalize: Duration,
    frames: u64,
}

#[derive(Default)]
struct GifClock {
    remainder_us: u128,
}

impl GifClock {
    fn advance(&mut self, elapsed: Duration) -> u16 {
        let total = self.remainder_us + elapsed.as_micros();
        self.remainder_us = total % 10_000;
        (total / 10_000).min(u16::MAX as u128) as u16
    }
}

pub(crate) fn output_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::var_os("USERPROFILE").ok_or("USERPROFILE is not set")?;
    let dir = PathBuf::from(dir).join("Videos").join("QuickGIFlick");
    std::fs::create_dir_all(&dir)?;
    #[cfg(windows)]
    let stamp = unsafe {
        let time = windows::Win32::System::SystemInformation::GetLocalTime();
        format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        )
    };
    #[cfg(not(windows))]
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();
    Ok(dir.join(format!("QuickGIFlick_{stamp}.gif")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use screendelta::{DeltaRegion, PixelFormat, Region};

    fn frame(width: u32, height: u32, pixel: [u8; 4]) -> CpuFrame {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width * height {
            data.extend_from_slice(&pixel);
        }
        CpuFrame {
            width,
            height,
            stride: width as usize * 4,
            format: PixelFormat::Bgra8,
            data,
        }
    }

    #[test]
    fn gif_clock_preserves_fractional_centiseconds() {
        let mut clock = GifClock::default();
        assert_eq!(clock.advance(Duration::from_micros(66_666)), 6);
        assert_eq!(clock.advance(Duration::from_micros(66_667)), 7);
    }

    #[test]
    fn recording_fps_accepts_benchmark_rates_and_rejects_invalid_values() {
        for fps in [10, 15, 20, 30, 240] {
            assert_eq!(recording_fps_from(Some(&fps.to_string())), fps);
        }
        for value in [None, Some("0"), Some("241"), Some("invalid")] {
            assert_eq!(recording_fps_from(value), 15);
        }
    }

    #[test]
    fn cursor_mode_defaults_to_original_and_allows_hidden_capture() {
        assert_eq!(cursor_capture_from(None), CursorCapture::Include);
        assert_eq!(cursor_capture_from(Some("hidden")), CursorCapture::Exclude);
        assert_eq!(cursor_capture_from(Some("off")), CursorCapture::Exclude);
        assert_eq!(cursor_capture_from(Some("standard")), CursorCapture::System);
    }

    #[test]
    fn partial_gif_keeps_unchanged_pixels() {
        let mut recording = Recording::new(frame(2, 2, [0, 0, 0, 255]), 1024).unwrap();
        recording
            .append_delta(
                Duration::from_millis(10),
                vec![DeltaRegion {
                    region: Region::new(1, 1, 1, 1).unwrap(),
                    pixels: frame(1, 1, [0, 0, 255, 255]),
                }],
            )
            .unwrap();
        recording.finish(Duration::from_millis(20));
        let path = std::env::temp_dir().join(format!(
            "QuickGIFlick_partial_test_{}.gif",
            std::process::id()
        ));
        encode_recording(
            &mut recording,
            &path,
            GifMode::Partial,
            GifQuality::Balanced,
        )
        .unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(File::open(&path).unwrap()).unwrap();
        let mut canvas = [0; 2 * 2 * 4];
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            for row in 0..frame.height as usize {
                let dst = ((frame.top as usize + row) * 2 + frame.left as usize) * 4;
                let src = row * frame.width as usize * 4;
                canvas[dst..dst + frame.width as usize * 4]
                    .copy_from_slice(&frame.buffer[src..src + frame.width as usize * 4]);
            }
        }
        assert_eq!(&canvas[0..3], &[0, 0, 0]);
        assert_eq!(&canvas[12..15], &[255, 0, 0]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn trim_starts_from_canvas_reconstructed_at_delta_time() {
        let mut recording = Recording::new(frame(2, 1, [0, 0, 0, 255]), 1024).unwrap();
        recording
            .append_delta(
                Duration::from_millis(10),
                vec![DeltaRegion {
                    region: Region::new(1, 0, 1, 1).unwrap(),
                    pixels: frame(1, 1, [0, 0, 255, 255]),
                }],
            )
            .unwrap();
        recording.finish(Duration::from_millis(30));
        let path =
            std::env::temp_dir().join(format!("QuickGIFlick_trim_test_{}.gif", std::process::id()));
        encode_recording_range(
            &mut recording,
            &path,
            GifMode::Full,
            GifQuality::Balanced,
            Duration::from_millis(15),
            Duration::from_millis(30),
        )
        .unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(File::open(&path).unwrap()).unwrap();
        let first = decoder.read_next_frame().unwrap().unwrap();
        assert_eq!(&first.buffer[4..7], &[255, 0, 0]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn trim_rejects_empty_or_out_of_bounds_range() {
        let mut recording = Recording::new(frame(1, 1, [0, 0, 0, 255]), 1024).unwrap();
        recording.finish(Duration::from_millis(20));
        let path = std::env::temp_dir().join("QuickGIFlick_invalid_trim.gif");
        assert!(
            encode_recording_range(
                &mut recording,
                &path,
                GifMode::Full,
                GifQuality::Balanced,
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .is_err()
        );
        assert!(
            encode_recording_range(
                &mut recording,
                &path,
                GifMode::Full,
                GifQuality::Balanced,
                Duration::ZERO,
                Duration::from_millis(21),
            )
            .is_err()
        );
    }
}
