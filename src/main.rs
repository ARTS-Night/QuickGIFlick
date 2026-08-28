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
        "recording updates={} resident_payload_bytes={} spilled_payload_bytes={} capture_stats={:?}",
        recording.update_len(),
        recording.resident_payload_bytes(),
        recording.spilled_payload_bytes(),
        capture.stats(),
    );
    let output = output_path()?;
    encode_recording(&mut recording, &output)?;
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
) -> Result<(), Box<dyn std::error::Error>> {
    let mut canvas = Canvas::new(recording.initial()?);
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
    for index in 0..recording.update_len() {
        let at = recording.update_time(index);
        write_canvas(
            &mut encoder,
            &canvas.frame,
            clock.advance(at.saturating_sub(last)),
            &mut gif_pixels,
        )?;
        recording.apply_update(index, &mut canvas)?;
        last = at;
    }
    write_canvas(
        &mut encoder,
        &canvas.frame,
        clock.advance(recording.end().saturating_sub(last)).max(1),
        &mut gif_pixels,
    )?;
    Ok(())
}

fn write_canvas(
    encoder: &mut Encoder<&mut File>,
    canvas: &CpuFrame,
    delay: u16,
    rgba: &mut Vec<u8>,
) -> Result<(), gif::EncodingError> {
    rgba.clear();
    rgba.extend_from_slice(&canvas.data);
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mut frame = Frame::from_rgba_speed(canvas.width as u16, canvas.height as u16, rgba, 10);
    frame.delay = delay;
    encoder.write_frame(&frame)
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

    #[test]
    fn gif_clock_preserves_fractional_centiseconds() {
        let mut clock = GifClock::default();
        assert_eq!(clock.advance(Duration::from_micros(66_666)), 6);
        assert_eq!(clock.advance(Duration::from_micros(66_667)), 7);
    }
}
