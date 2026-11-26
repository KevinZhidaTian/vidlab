//! A tiny RGB image type plus PPM/PGM (Netpbm) reading and writing.
//! PPM is about the simplest image format there is: a text header, then raw bytes.

use crate::error::{Error, Result};
use std::io::{BufRead, Write};

/// Interleaved 8-bit RGB: [r, g, b, r, g, b, ...], row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl RgbImage {
    pub fn new(width: usize, height: usize) -> Self {
        RgbImage {
            width,
            height,
            data: vec![0; width * height * 3],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let i = (y * self.width + x) * 3;
        (self.data[i], self.data[i + 1], self.data[i + 2])
    }

    pub fn set(&mut self, x: usize, y: usize, (r, g, b): (u8, u8, u8)) {
        let i = (y * self.width + x) * 3;
        self.data[i] = r;
        self.data[i + 1] = g;
        self.data[i + 2] = b;
    }
}

/// Read a binary PPM (`P6`, maxval 255).
pub fn read_ppm<R: BufRead>(mut r: R) -> Result<RgbImage> {
    let magic = read_token(&mut r)?;
    if magic != "P6" {
        return Err(Error::Unsupported(format!(
            "expected P6 PPM, got {magic:?}"
        )));
    }
    let width = parse_num(&read_token(&mut r)?)?;
    let height = parse_num(&read_token(&mut r)?)?;
    let maxval = parse_num(&read_token(&mut r)?)?;
    if maxval != 255 {
        return Err(Error::Unsupported(format!(
            "maxval {maxval} (only 255 supported)"
        )));
    }
    // read_token consumed exactly one whitespace byte after maxval; pixel data follows.
    let mut img = RgbImage::new(width, height);
    r.read_exact(&mut img.data)?;
    Ok(img)
}

pub fn write_ppm<W: Write>(mut w: W, img: &RgbImage) -> Result<()> {
    write!(w, "P6\n{} {}\n255\n", img.width, img.height)?;
    w.write_all(&img.data)?;
    Ok(())
}

/// Write one 8-bit plane as a greyscale PGM (`P5`). Handy for looking at Y, U or V alone.
pub fn write_pgm<W: Write>(mut w: W, width: usize, height: usize, plane: &[u8]) -> Result<()> {
    write!(w, "P5\n{width} {height}\n255\n")?;
    w.write_all(plane)?;
    Ok(())
}

/// Read one whitespace-separated header token, skipping `#` comments.
/// Consumes the single whitespace byte that ends the token.
fn read_token<R: BufRead>(r: &mut R) -> Result<String> {
    let mut token = String::new();
    let mut byte = [0u8; 1];
    loop {
        r.read_exact(&mut byte)?;
        let c = byte[0];
        if c == b'#' && token.is_empty() {
            let mut skip = Vec::new();
            r.read_until(b'\n', &mut skip)?;
        } else if c.is_ascii_whitespace() {
            if !token.is_empty() {
                return Ok(token);
            }
        } else {
            token.push(c as char);
        }
    }
}

fn parse_num(s: &str) -> Result<usize> {
    s.parse()
        .map_err(|_| Error::BadHeader(format!("not a number: {s:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_round_trip() {
        let mut img = RgbImage::new(3, 2);
        img.set(0, 0, (255, 0, 0));
        img.set(2, 1, (1, 2, 3));
        let mut buf = Vec::new();
        write_ppm(&mut buf, &img).unwrap();
        let back = read_ppm(&buf[..]).unwrap();
        assert_eq!(img, back);
    }

    #[test]
    fn ppm_with_comment() {
        let mut data = b"P6\n# made by hand\n1 1\n255\n".to_vec();
        data.extend_from_slice(&[10, 20, 30]);
        let img = read_ppm(&data[..]).unwrap();
        assert_eq!(img.get(0, 0), (10, 20, 30));
    }
}
