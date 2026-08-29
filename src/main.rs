mod recording;
mod timeline;

use std::{
    fs::File,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use gif::{Encoder, Frame, Repeat};
use recording::Recording;
use screendelta::{
    CaptureConfig, CaptureSession, CaptureSource, CaptureUpdate, CpuFrame, FramePacer, monitors,
};
use timeline::Canvas;

const DEFAULT_RECORDING_MEMORY_BYTES: usize = 32 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = monitors()?
        .into_iter()
        .next()
        .ok_or("No monitor available")?;
    let mut capture = CaptureSession::start(CaptureConfig {
        source: CaptureSource::Monitor(monitor.id),
    })?;
    let initial = capture.next_frame()?;
    let capture_origin = initial.timestamp();
    let mut recording = Recording::new(initial.into_readback()?, recording_memory_budget())?;
    let recording_started = Instant::now();
    let mut pacer = FramePacer::new(15)?;
    let seconds = std::env::var("QUICKGIFFLICK_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let deadline = recording_started + Duration::from_secs(seconds);

    while Instant::now() < deadline {
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
    let output = output_path()?;
    let encode_started = Instant::now();
    let mode = GifMode::from_env();
    let encode = encode_recording(&mut recording, &output, mode)?;
    eprintln!(
        "encode mode={} wall_ms={:.3} reconstruction_ms={:.3} conversion_ms={:.3} quantization_ms={:.3} encoder_ms={:.3} finalize_ms={:.3} frames={}",
        mode.name(),
        encode_started.elapsed().as_secs_f64() * 1_000.0,
        encode.reconstruction.as_secs_f64() * 1_000.0,
        encode.conversion.as_secs_f64() * 1_000.0,
        encode.quantization.as_secs_f64() * 1_000.0,
        encode.encoder.as_secs_f64() * 1_000.0,
        encode.finalize.as_secs_f64() * 1_000.0,
        encode.frames,
    );
    println!("Saved {}", output.display());
    Ok(())
}

fn recording_memory_budget() -> usize {
    std::env::var("QUICKGIFFLICK_RECORDING_MEMORY_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|megabytes| megabytes.saturating_mul(1024 * 1024))
        .unwrap_or(DEFAULT_RECORDING_MEMORY_BYTES)
}

fn encode_recording(
    recording: &mut Recording,
    output: &PathBuf,
    mode: GifMode,
) -> Result<EncodeStats, Box<dyn std::error::Error>> {
    let mut stats = EncodeStats::default();
    let reconstruction_started = Instant::now();
    let mut canvas = Canvas::new(recording.initial()?);
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
    let mut last = Duration::ZERO;
    let mut gif_pixels = Vec::with_capacity(canvas.frame.data.len());
    let full = full_bounds(&canvas.frame);
    let mut pending = full;
    for index in 0..recording.update_len() {
        let at = recording.update_time(index);
        write_region(
            &mut encoder,
            &canvas.frame,
            pending,
            clock.advance(at.saturating_sub(last)),
            &mut gif_pixels,
            &mut stats,
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
        clock.advance(recording.end().saturating_sub(last)).max(1),
        &mut gif_pixels,
        &mut stats,
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
        10,
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
enum GifMode {
    Full,
    Partial,
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

fn output_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::var_os("USERPROFILE").ok_or("USERPROFILE is not set")?;
    let dir = PathBuf::from(dir).join("Videos").join("QuickGIFlick");
    std::fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
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
        encode_recording(&mut recording, &path, GifMode::Partial).unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(File::open(&path).unwrap()).unwrap();
        let mut canvas = vec![0; 2 * 2 * 4];
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
}
