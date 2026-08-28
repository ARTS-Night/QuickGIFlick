use screendelta::{CpuFrame, Region};

pub struct Canvas {
    pub frame: CpuFrame,
}

impl Canvas {
    pub fn new(frame: CpuFrame) -> Self {
        Self { frame }
    }

    pub fn replace(&mut self, frame: CpuFrame) {
        if self.frame.width == frame.width
            && self.frame.height == frame.height
            && self.frame.stride == frame.stride
            && self.frame.format == frame.format
        {
            self.frame.data.copy_from_slice(&frame.data);
        } else {
            self.frame = frame;
        }
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

    fn frame(width: u32, height: u32, value: u8) -> CpuFrame {
        CpuFrame {
            width,
            height,
            stride: width as usize * 4,
            format: PixelFormat::Bgra8,
            data: vec![value; (width * height * 4) as usize],
        }
    }

    #[test]
    fn applies_delta_without_reallocating_canvas() {
        let mut canvas = Canvas::new(frame(4, 4, 0));
        let ptr = canvas.frame.data.as_ptr();
        canvas
            .apply(Region::new(1, 1, 2, 2).unwrap(), &frame(2, 2, 7))
            .unwrap();
        assert_eq!(ptr, canvas.frame.data.as_ptr());
        assert_eq!(canvas.frame.data[canvas.frame.stride + 4], 7);
    }

    #[test]
    fn replaces_same_canvas_without_reallocating() {
        let mut canvas = Canvas::new(frame(2, 2, 0));
        let ptr = canvas.frame.data.as_ptr();
        canvas.replace(frame(2, 2, 5));
        assert_eq!(ptr, canvas.frame.data.as_ptr());
        assert_eq!(canvas.frame.data[0], 5);
    }
}
