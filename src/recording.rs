use screendelta::CpuFrame;
use std::time::Duration;

pub struct Recording {
    pub initial: CpuFrame,
    pub updates: Vec<Update>,
    pub end: Duration,
}
pub struct Update {
    pub at: Duration,
    pub frame: CpuFrame,
}
impl Recording {
    pub fn new(initial: CpuFrame) -> Self {
        Self {
            initial,
            updates: Vec::new(),
            end: Duration::ZERO,
        }
    }
    pub fn append(&mut self, at: Duration, frame: Option<CpuFrame>) {
        self.end = at;
        if let Some(frame) = frame {
            self.updates.push(Update { at, frame });
        }
    }
    pub fn payload_bytes(&self) -> usize {
        self.initial.data.capacity()
            + self
                .updates
                .iter()
                .map(|u| u.frame.data.capacity())
                .sum::<usize>()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use screendelta::PixelFormat;
    fn f() -> CpuFrame {
        CpuFrame {
            width: 1,
            height: 1,
            stride: 4,
            format: PixelFormat::Bgra8,
            data: vec![0; 4],
        }
    }
    #[test]
    fn unchanged_stores_only_time() {
        let mut r = Recording::new(f());
        r.append(Duration::from_secs(1), None);
        assert_eq!(r.updates.len(), 0);
        assert_eq!(r.end, Duration::from_secs(1));
    }
}
