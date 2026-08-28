use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use screendelta::{CpuFrame, DeltaRegion, PixelFormat};

static NEXT_SPILL_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
enum Payload {
    Memory(Vec<u8>),
    Spill { offset: u64, len: usize },
}

#[derive(Clone)]
pub struct StoredFrame {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: PixelFormat,
    payload: Payload,
}

pub enum Update {
    Full {
        at: Duration,
        frame: StoredFrame,
    },
    Delta {
        at: Duration,
        regions: Vec<StoredRegion>,
    },
}

pub struct StoredRegion {
    pub region: screendelta::Region,
    pub pixels: StoredFrame,
}

pub struct Recording {
    initial: StoredFrame,
    updates: Vec<Update>,
    end: Duration,
    memory_budget: usize,
    resident_payload_bytes: usize,
    spilled_payload_bytes: usize,
    spill_file: Option<File>,
    spill_path: Option<PathBuf>,
}

impl Recording {
    pub fn new(initial: CpuFrame, memory_budget: usize) -> std::io::Result<Self> {
        let mut recording = Self {
            initial: StoredFrame {
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::Bgra8,
                payload: Payload::Memory(Vec::new()),
            },
            updates: Vec::new(),
            end: Duration::ZERO,
            memory_budget,
            resident_payload_bytes: 0,
            spilled_payload_bytes: 0,
            spill_file: None,
            spill_path: None,
        };
        recording.initial = recording.store_frame(initial)?;
        Ok(recording)
    }

    pub fn append_full(&mut self, at: Duration, frame: CpuFrame) -> std::io::Result<()> {
        let frame = self.store_frame(frame)?;
        self.end = at;
        self.updates.push(Update::Full { at, frame });
        Ok(())
    }

    pub fn append_delta(&mut self, at: Duration, regions: Vec<DeltaRegion>) -> std::io::Result<()> {
        let mut stored = Vec::with_capacity(regions.len());
        for DeltaRegion { region, pixels } in regions {
            stored.push(StoredRegion {
                region,
                pixels: self.store_frame(pixels)?,
            });
        }
        self.end = at;
        self.updates.push(Update::Delta {
            at,
            regions: stored,
        });
        Ok(())
    }

    pub fn observe_unchanged(&mut self, at: Duration) {
        self.end = at;
    }

    pub fn finish(&mut self, at: Duration) {
        self.end = at.max(self.end);
    }

    pub fn initial(&mut self) -> std::io::Result<CpuFrame> {
        self.load_frame(&self.initial.clone())
    }

    pub fn update_len(&self) -> usize {
        self.updates.len()
    }

    pub fn update_time(&self, index: usize) -> Duration {
        match &self.updates[index] {
            Update::Full { at, .. } | Update::Delta { at, .. } => *at,
        }
    }

    pub fn apply_update(
        &mut self,
        index: usize,
        canvas: &mut crate::timeline::Canvas,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let update = &self.updates[index];
        let frames = match update {
            Update::Full { frame, .. } => {
                let frame = frame.clone();
                canvas.replace(self.load_frame(&frame)?);
                return Ok(());
            }
            Update::Delta { regions, .. } => regions
                .iter()
                .map(|region| (region.region, region.pixels.clone()))
                .collect::<Vec<_>>(),
        };
        for (region, frame) in frames {
            let pixels = self.load_frame(&frame)?;
            canvas.apply(region, &pixels)?;
        }
        Ok(())
    }

    pub fn end(&self) -> Duration {
        self.end
    }

    pub fn resident_payload_bytes(&self) -> usize {
        self.resident_payload_bytes
    }

    pub fn spilled_payload_bytes(&self) -> usize {
        self.spilled_payload_bytes
    }

    fn store_frame(&mut self, frame: CpuFrame) -> std::io::Result<StoredFrame> {
        let CpuFrame {
            width,
            height,
            stride,
            format,
            data,
        } = frame;
        let len = data.len();
        let payload = if self.resident_payload_bytes.saturating_add(len) <= self.memory_budget {
            self.resident_payload_bytes += len;
            Payload::Memory(data)
        } else {
            let file = self.spill_file()?;
            let offset = file.seek(SeekFrom::End(0))?;
            file.write_all(&data)?;
            self.spilled_payload_bytes += len;
            Payload::Spill { offset, len }
        };
        Ok(StoredFrame {
            width,
            height,
            stride,
            format,
            payload,
        })
    }

    fn load_frame(&mut self, frame: &StoredFrame) -> std::io::Result<CpuFrame> {
        let data = match &frame.payload {
            Payload::Memory(data) => data.clone(),
            Payload::Spill { offset, len } => {
                let file = self
                    .spill_file
                    .as_mut()
                    .expect("spill metadata requires a spill file");
                file.seek(SeekFrom::Start(*offset))?;
                let mut data = vec![0; *len];
                file.read_exact(&mut data)?;
                data
            }
        };
        Ok(CpuFrame {
            width: frame.width,
            height: frame.height,
            stride: frame.stride,
            format: frame.format,
            data,
        })
    }

    fn spill_file(&mut self) -> std::io::Result<&mut File> {
        if self.spill_file.is_none() {
            let id = NEXT_SPILL_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "QuickGIFlick_recording_{}_{}.bin",
                std::process::id(),
                id
            ));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)?;
            self.spill_path = Some(path);
            self.spill_file = Some(file);
        }
        Ok(self.spill_file.as_mut().expect("spill file was created"))
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.spill_file.take();
        if let Some(path) = self.spill_path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screendelta::{PixelFormat, Region};

    fn frame(width: u32, value: u8) -> CpuFrame {
        CpuFrame {
            width,
            height: 1,
            stride: width as usize * 4,
            format: PixelFormat::Bgra8,
            data: vec![value; width as usize * 4],
        }
    }

    #[test]
    fn unchanged_stores_only_time() {
        let mut recording = Recording::new(frame(1, 0), 4).unwrap();
        recording.observe_unchanged(Duration::from_secs(1));
        assert_eq!(recording.update_len(), 0);
        assert_eq!(recording.end(), Duration::from_secs(1));
    }

    #[test]
    fn spills_payload_and_reconstructs_delta() {
        let mut recording = Recording::new(frame(2, 0), 8).unwrap();
        recording
            .append_delta(
                Duration::from_millis(10),
                vec![DeltaRegion {
                    region: Region::new(1, 0, 1, 1).unwrap(),
                    pixels: frame(1, 7),
                }],
            )
            .unwrap();
        assert_eq!(recording.resident_payload_bytes(), 8);
        assert_eq!(recording.spilled_payload_bytes(), 4);
        let mut canvas = crate::timeline::Canvas::new(recording.initial().unwrap());
        recording.apply_update(0, &mut canvas).unwrap();
        assert_eq!(canvas.frame.data[4], 7);
    }
}
