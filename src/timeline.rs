use screendelta::{CpuFrame, Region};

pub struct Canvas {
    pub frame: CpuFrame,
}

impl Canvas {
    pub fn new(frame: CpuFrame) -> Self {
        Self { frame }
    }
    pub fn apply(&mut self, region: Region, pixels: &CpuFrame) -> Result<(), &'static str> {
        if pixels.width != region.size.width || pixels.height != region.size.height {
            return Err("delta size mismatch");
        }
        let x = usize::try_from(region.x).map_err(|_| "negative region")?;
        let y = usize::try_from(region.y).map_err(|_| "negative region")?;
        if x + pixels.width as usize > self.frame.width as usize
            || y + pixels.height as usize > self.frame.height as usize
        {
            return Err("delta outside canvas");
        }
        for row in 0..pixels.height as usize {
            let dst = (y + row) * self.frame.stride + x * 4;
            let src = row * pixels.stride;
            self.frame.data[dst..dst + pixels.width as usize * 4]
                .copy_from_slice(&pixels.data[src..src + pixels.width as usize * 4]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screendelta::PixelFormat;
    fn f(w: u32, h: u32, v: u8) -> CpuFrame {
        CpuFrame {
            width: w,
            height: h,
            stride: w as usize * 4,
            format: PixelFormat::Bgra8,
            data: vec![v; (w * h * 4) as usize],
        }
    }
    #[test]
    fn applies_delta_without_reallocating_canvas() {
        let mut c = Canvas::new(f(4, 4, 0));
        let ptr = c.frame.data.as_ptr();
        c.apply(Region::new(1, 1, 2, 2).unwrap(), &f(2, 2, 7))
            .unwrap();
        assert_eq!(ptr, c.frame.data.as_ptr());
        assert_eq!(c.frame.data[1 * c.frame.stride + 1 * 4], 7);
    }
}
