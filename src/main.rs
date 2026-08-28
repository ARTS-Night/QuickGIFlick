use std::{
    fs::File,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use gif::{Encoder, Frame, Repeat};
use screendelta::{CaptureConfig, CaptureSession, CaptureSource, FramePacer, monitors};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = monitors()?
        .into_iter()
        .next()
        .ok_or("No monitor available")?;
    let mut capture = CaptureSession::start(CaptureConfig {
        source: CaptureSource::Monitor(monitor.id),
    })?;
    let output = output_path()?;
    let mut frames = vec![RecordedFrame {
        pixels: capture.next_frame()?.into_readback()?,
        delay: 0,
    }];
    let recording_started = std::time::Instant::now();
    let mut last_sample = recording_started;
    let mut gif_clock = GifClock::default();
    let mut pacer = FramePacer::new(15)?;
    let seconds = std::env::var("QUICKGIFFLICK_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let deadline = recording_started + Duration::from_secs(seconds);

    while std::time::Instant::now() < deadline {
        pacer.wait();
        let now = std::time::Instant::now();
        let delay = gif_clock.advance(now.duration_since(last_sample));
        last_sample = now;
        append_sample(
            &mut frames,
            capture
                .try_next_frame(Duration::ZERO)?
                .map(|frame| frame.into_readback())
                .transpose()?,
            delay,
        );
    }
    let tail = gif_clock.advance(std::time::Instant::now().duration_since(last_sample));
    if let Some(frame) = frames.last_mut() {
        frame.delay = frame.delay.saturating_add(tail.max(1));
    }
    let stored_payload_bytes: usize = frames
        .iter()
        .map(|frame| frame.pixels.data.capacity())
        .sum();
    eprintln!(
        "recording frames={} stored_payload_bytes={stored_payload_bytes}",
        frames.len()
    );
    let first = frames.first().ok_or("No frames captured")?;
    let mut file = File::create(&output)?;
    let mut encoder = Encoder::new(
        &mut file,
        first.pixels.width as u16,
        first.pixels.height as u16,
        &[],
    )?;
    encoder.set_repeat(Repeat::Infinite)?;
    for RecordedFrame { pixels: cpu, delay } in frames {
        let mut rgba = cpu.data;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut frame = Frame::from_rgba_speed(cpu.width as u16, cpu.height as u16, &mut rgba, 10);
        frame.delay = delay;
        encoder.write_frame(&frame)?;
    }
    println!("Saved {}", output.display());
    Ok(())
}

struct RecordedFrame {
    pixels: screendelta::CpuFrame,
    delay: u16,
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

fn append_sample(
    frames: &mut Vec<RecordedFrame>,
    update: Option<screendelta::CpuFrame>,
    delay: u16,
) {
    if let Some(pixels) = update {
        if let Some(frame) = frames.last_mut() {
            frame.delay = frame.delay.saturating_add(delay);
        }
        frames.push(RecordedFrame { pixels, delay: 0 });
    } else if let Some(frame) = frames.last_mut() {
        frame.delay = frame.delay.saturating_add(delay);
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
    use screendelta::{CpuFrame, PixelFormat};

    fn frame() -> CpuFrame {
        CpuFrame {
            width: 1,
            height: 1,
            stride: 4,
            format: PixelFormat::Bgra8,
            data: vec![0; 4],
        }
    }

    #[test]
    fn unchanged_sample_extends_the_previous_gif_frame() {
        let mut frames = vec![RecordedFrame {
            pixels: frame(),
            delay: 0,
        }];
        append_sample(&mut frames, None, 7);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].delay, 7);
    }

    #[test]
    fn gif_clock_preserves_fractional_centiseconds() {
        let mut clock = GifClock::default();
        assert_eq!(clock.advance(Duration::from_micros(66_666)), 6);
        assert_eq!(clock.advance(Duration::from_micros(66_667)), 7);
    }
}
