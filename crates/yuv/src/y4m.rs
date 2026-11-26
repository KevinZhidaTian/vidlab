//! YUV4MPEG2 (.y4m): raw YUV frames with a one-line text header that records the size.
//! Format: `YUV4MPEG2 W<w> H<h> F<num>:<den> Ip A1:1 C420jpeg ...\n`, then per frame `FRAME\n` + I420 bytes.

use crate::color::ColorRange;
use crate::error::{Error, Result};
use crate::frame::{PixelFormat, YuvFrame};
use std::io::{BufRead, Write};

pub struct Y4mWriter<W: Write> {
    out: W,
    width: usize,
    height: usize,
}

impl<W: Write> Y4mWriter<W> {
    /// Writes the stream header. `fps` is a (numerator, denominator) pair, e.g. (25, 1) or (30000, 1001).
    pub fn new(
        mut out: W,
        width: usize,
        height: usize,
        fps: (u32, u32),
        range: ColorRange,
    ) -> Result<Self> {
        let range_tag = match range {
            ColorRange::Limited => "LIMITED",
            ColorRange::Full => "FULL",
        };
        writeln!(
            out,
            "YUV4MPEG2 W{width} H{height} F{}:{} Ip A1:1 C420jpeg XCOLORRANGE={range_tag}",
            fps.0, fps.1
        )?;
        Ok(Y4mWriter { out, width, height })
    }

    pub fn write_frame(&mut self, frame: &YuvFrame) -> Result<()> {
        if (frame.width, frame.height) != (self.width, self.height) {
            return Err(Error::Unsupported(
                "frame size differs from stream header".into(),
            ));
        }
        self.out.write_all(b"FRAME\n")?;
        self.out.write_all(&frame.to_bytes(PixelFormat::I420))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Y4mHeader {
    pub width: usize,
    pub height: usize,
    pub fps: (u32, u32),
    /// None if the file does not say.
    pub range: Option<ColorRange>,
}

pub struct Y4mReader<R: BufRead> {
    input: R,
    pub header: Y4mHeader,
}

impl<R: BufRead> Y4mReader<R> {
    pub fn new(mut input: R) -> Result<Self> {
        let mut line = String::new();
        input.read_line(&mut line)?;
        let mut parts = line.trim_end().split(' ');
        if parts.next() != Some("YUV4MPEG2") {
            return Err(Error::BadHeader("missing YUV4MPEG2 signature".into()));
        }
        let mut header = Y4mHeader {
            width: 0,
            height: 0,
            fps: (25, 1),
            range: None,
        };
        for p in parts {
            let (tag, val) = p.split_at(1);
            match tag {
                "W" => header.width = num(val)?,
                "H" => header.height = num(val)?,
                "F" => {
                    let (n, d) = val
                        .split_once(':')
                        .ok_or_else(|| Error::BadHeader(p.into()))?;
                    header.fps = (num(n)?, num(d)?);
                }
                "C" if !val.starts_with("420") => {
                    return Err(Error::Unsupported(format!("chroma format {val}")));
                }
                "X" => match val {
                    "COLORRANGE=LIMITED" => header.range = Some(ColorRange::Limited),
                    "COLORRANGE=FULL" => header.range = Some(ColorRange::Full),
                    _ => {}
                },
                _ => {} // ignore interlacing, aspect ratio and other tags
            }
        }
        if header.width == 0 || header.height == 0 {
            return Err(Error::BadHeader("missing W or H".into()));
        }
        Ok(Y4mReader { input, header })
    }
}

/// Reading frames is an Iterator, so callers can write `for frame in reader { ... }`.
impl<R: BufRead> Iterator for Y4mReader<R> {
    type Item = Result<YuvFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) => return None, // clean end of file
            Ok(_) if line.starts_with("FRAME") => {}
            Ok(_) => {
                return Some(Err(Error::BadHeader(format!(
                    "expected FRAME, got {line:?}"
                ))));
            }
            Err(e) => return Some(Err(e.into())),
        }
        let (w, h) = (self.header.width, self.header.height);
        let mut buf = vec![0u8; YuvFrame::frame_size(w, h)];
        if let Err(e) = self.input.read_exact(&mut buf) {
            return Some(Err(e.into()));
        }
        Some(YuvFrame::from_bytes(&buf, w, h, PixelFormat::I420))
    }
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T> {
    s.parse()
        .map_err(|_| Error::BadHeader(format!("not a number: {s:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorMatrix;
    use crate::image::RgbImage;

    #[test]
    fn write_then_read_two_frames() {
        let img = RgbImage::new(4, 4);
        let f = YuvFrame::from_rgb(&img, ColorMatrix::Bt709, ColorRange::Limited);
        let mut buf = Vec::new();
        {
            let mut w = Y4mWriter::new(&mut buf, 4, 4, (30000, 1001), ColorRange::Limited).unwrap();
            w.write_frame(&f).unwrap();
            w.write_frame(&f).unwrap();
        }
        let r = Y4mReader::new(&buf[..]).unwrap();
        assert_eq!(r.header.fps, (30000, 1001));
        assert_eq!(r.header.range, Some(ColorRange::Limited));
        let frames: Vec<_> = r.collect::<Result<_>>().unwrap();
        assert_eq!(frames, vec![f.clone(), f]);
    }
}
