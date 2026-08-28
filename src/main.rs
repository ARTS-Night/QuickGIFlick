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
        delay: 7,
    }];
    let mut pacer = FramePacer::new(15)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);

    while std::time::Instant::now() < deadline {
        append_sample(&mut frames, capture.try_next_frame(Duration::ZERO)?.map(|frame| frame.into_readback()).transpose()?);
        pacer.wait();
    }
    let first = frames.first().ok_or("No frames captured")?;
    let mut file = File::create(&output)?;
    let mut encoder = Encoder::new(&mut file, first.pixels.width as u16, first.pixels.height as u16, &[])?;
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

fn append_sample(frames: &mut Vec<RecordedFrame>, update: Option<screendelta::CpuFrame>) {
    if let Some(pixels) = update {
        frames.push(RecordedFrame { pixels, delay: 7 });
    } else if let Some(frame) = frames.last_mut() {
        frame.delay = frame.delay.saturating_add(7);
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
        CpuFrame { width: 1, height: 1, stride: 4, format: PixelFormat::Bgra8, data: vec![0; 4] }
    }

    #[test]
    fn unchanged_sample_extends_the_previous_gif_frame() {
        let mut frames = vec![RecordedFrame { pixels: frame(), delay: 7 }];
        append_sample(&mut frames, None);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].delay, 14);
    }
}
