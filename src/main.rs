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
    let mut frames = Vec::new();
    let mut pacer = FramePacer::new(15)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);

    while std::time::Instant::now() < deadline {
        let cpu = capture.next_frame()?.readback()?;
        frames.push(cpu);
        pacer.wait();
    }
    let first = frames.first().ok_or("No frames captured")?;
    let mut file = File::create(&output)?;
    let mut encoder = Encoder::new(&mut file, first.width as u16, first.height as u16, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;
    for cpu in frames {
        let mut rgba = cpu.data;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut frame = Frame::from_rgba_speed(cpu.width as u16, cpu.height as u16, &mut rgba, 10);
        frame.delay = 7;
        encoder.write_frame(&frame)?;
    }
    println!("Saved {}", output.display());
    Ok(())
}

fn output_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::var_os("USERPROFILE").ok_or("USERPROFILE is not set")?;
    let dir = PathBuf::from(dir).join("Videos").join("QuickGIFlick");
    std::fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(dir.join(format!("QuickGIFlick_{stamp}.gif")))
}
