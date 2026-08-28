use std::{env, fs::File};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: inspect_gif <path>")?;
    let mut decoder = gif::DecodeOptions::new().read_info(File::open(path)?)?;
    let mut frames = 0u64;
    let mut centiseconds = 0u64;
    while let Some(frame) = decoder.read_next_frame()? {
        frames += 1;
        centiseconds += u64::from(frame.delay);
    }
    println!("frames={frames} duration_centiseconds={centiseconds}");
    Ok(())
}
