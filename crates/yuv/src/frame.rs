//! A 4:2:0 YUV frame, conversion from/to RGB, and the I420 / NV12 memory layouts.

use crate::color::{self, ColorMatrix, ColorRange, YCbCr};
use crate::error::{Error, Result};
use crate::image::RgbImage;

/// How the three planes are laid out in a raw buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Planar: all Y, then all U (Cb), then all V (Cr). ffmpeg calls it `yuv420p`.
    I420,
    /// Semi-planar: all Y, then U and V interleaved (UVUVUV...). Common in hardware decoders.
    Nv12,
}

/// A 4:2:0 frame: full-resolution luma, chroma at half width and half height.
#[derive(Debug, Clone, PartialEq)]
pub struct YuvFrame {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl YuvFrame {
    /// Chroma plane width. Rounds up so odd widths still have a chroma sample for the last column.
    pub fn chroma_width(&self) -> usize {
        chroma_dim(self.width)
    }

    pub fn chroma_height(&self) -> usize {
        chroma_dim(self.height)
    }

    /// Bytes in one raw 4:2:0 frame of this size (same for I420 and NV12).
    pub fn frame_size(width: usize, height: usize) -> usize {
        width * height + 2 * chroma_dim(width) * chroma_dim(height)
    }

    /// RGB -> 4:2:0. Each chroma sample is the average of a 2×2 block of pixels
    /// (centre-sited, like JPEG / MPEG-1).
    pub fn from_rgb(img: &RgbImage, matrix: ColorMatrix, range: ColorRange) -> Self {
        let (w, h) = (img.width, img.height);
        let (cw, ch) = (chroma_dim(w), chroma_dim(h));

        // Pass 1: full-resolution Y'CbCr in floating point.
        let full: Vec<YCbCr> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (r, g, b) = img.get(x, y);
                color::rgb_to_ycbcr(r, g, b, matrix)
            })
            .collect();

        let y_plane = full
            .iter()
            .map(|c| color::quantize_luma(c.y, range))
            .collect();

        // Pass 2: average each 2×2 block for chroma, then quantise.
        let mut u = Vec::with_capacity(cw * ch);
        let mut v = Vec::with_capacity(cw * ch);
        for cy in 0..ch {
            for cx in 0..cw {
                let (mut sum_cb, mut sum_cr, mut n) = (0.0f32, 0.0f32, 0.0f32);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let (x, y) = (cx * 2 + dx, cy * 2 + dy);
                        if x < w && y < h {
                            let c = full[y * w + x];
                            sum_cb += c.cb;
                            sum_cr += c.cr;
                            n += 1.0;
                        }
                    }
                }
                u.push(color::quantize_chroma(sum_cb / n, range));
                v.push(color::quantize_chroma(sum_cr / n, range));
            }
        }

        YuvFrame {
            width: w,
            height: h,
            y: y_plane,
            u,
            v,
        }
    }

    /// 4:2:0 -> RGB using nearest-neighbour chroma upsampling (each chroma sample covers 2×2 pixels).
    pub fn to_rgb(&self, matrix: ColorMatrix, range: ColorRange) -> RgbImage {
        let mut img = RgbImage::new(self.width, self.height);
        let cw = self.chroma_width();
        for y in 0..self.height {
            for x in 0..self.width {
                let ci = (y / 2) * cw + x / 2;
                let c = YCbCr {
                    y: color::dequantize_luma(self.y[y * self.width + x], range),
                    cb: color::dequantize_chroma(self.u[ci], range),
                    cr: color::dequantize_chroma(self.v[ci], range),
                };
                img.set(x, y, color::ycbcr_to_rgb(c, matrix));
            }
        }
        img
    }

    /// Serialise to a raw buffer in the given layout.
    pub fn to_bytes(&self, format: PixelFormat) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::frame_size(self.width, self.height));
        out.extend_from_slice(&self.y);
        match format {
            PixelFormat::I420 => {
                out.extend_from_slice(&self.u);
                out.extend_from_slice(&self.v);
            }
            PixelFormat::Nv12 => {
                for (u, v) in self.u.iter().zip(&self.v) {
                    out.push(*u);
                    out.push(*v);
                }
            }
        }
        out
    }

    /// Parse a raw buffer. Note that nothing in the bytes says what width, height or
    /// format they are: get any of them wrong and you still "succeed", with garbage.
    pub fn from_bytes(
        data: &[u8],
        width: usize,
        height: usize,
        format: PixelFormat,
    ) -> Result<Self> {
        let expected = Self::frame_size(width, height);
        if data.len() != expected {
            return Err(Error::WrongSize {
                expected,
                actual: data.len(),
            });
        }
        let luma = width * height;
        let chroma = chroma_dim(width) * chroma_dim(height);
        let y = data[..luma].to_vec();
        let rest = &data[luma..];
        let (u, v) = match format {
            PixelFormat::I420 => (rest[..chroma].to_vec(), rest[chroma..].to_vec()),
            PixelFormat::Nv12 => (
                rest.iter().step_by(2).copied().collect(),
                rest.iter().skip(1).step_by(2).copied().collect(),
            ),
        };
        Ok(YuvFrame {
            width,
            height,
            y,
            u,
            v,
        })
    }
}

fn chroma_dim(n: usize) -> usize {
    n.div_ceil(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, rgb: (u8, u8, u8)) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.set(x, y, rgb);
            }
        }
        img
    }

    #[test]
    fn plane_sizes_even_and_odd() {
        let f = YuvFrame::from_rgb(
            &solid(4, 2, (0, 0, 0)),
            ColorMatrix::Bt709,
            ColorRange::Limited,
        );
        assert_eq!((f.y.len(), f.u.len(), f.v.len()), (8, 2, 2));
        let f = YuvFrame::from_rgb(
            &solid(5, 3, (0, 0, 0)),
            ColorMatrix::Bt709,
            ColorRange::Limited,
        );
        assert_eq!((f.y.len(), f.u.len(), f.v.len()), (15, 6, 6));
        assert_eq!(YuvFrame::frame_size(1920, 1080), 1920 * 1080 * 3 / 2);
    }

    #[test]
    fn i420_and_nv12_round_trip() {
        let f = YuvFrame::from_rgb(
            &solid(6, 4, (200, 50, 10)),
            ColorMatrix::Bt709,
            ColorRange::Limited,
        );
        for fmt in [PixelFormat::I420, PixelFormat::Nv12] {
            let bytes = f.to_bytes(fmt);
            assert_eq!(YuvFrame::from_bytes(&bytes, 6, 4, fmt).unwrap(), f);
        }
    }

    #[test]
    fn solid_colour_survives_round_trip() {
        let img = solid(8, 8, (40, 160, 220));
        let f = YuvFrame::from_rgb(&img, ColorMatrix::Bt709, ColorRange::Limited);
        let back = f.to_rgb(ColorMatrix::Bt709, ColorRange::Limited);
        for (a, b) in img.data.iter().zip(&back.data) {
            assert!((*a as i32 - *b as i32).abs() <= 2, "{a} vs {b}");
        }
    }

    #[test]
    fn wrong_size_is_an_error() {
        assert!(YuvFrame::from_bytes(&[0; 10], 4, 4, PixelFormat::I420).is_err());
    }
}
