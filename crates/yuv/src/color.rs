//! Colour maths: R'G'B' <-> Y'CbCr for one pixel.

/// Which set of luma coefficients to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMatrix {
    /// SD video (and JPEG).
    Bt601,
    /// HD video. What almost all OTT SDR content uses.
    Bt709,
}

impl ColorMatrix {
    /// Returns (Kr, Kb). Kg is always 1 - Kr - Kb.
    pub fn coefficients(self) -> (f32, f32) {
        match self {
            ColorMatrix::Bt601 => (0.299, 0.114),
            ColorMatrix::Bt709 => (0.2126, 0.0722),
        }
    }
}

/// How the 0.0..=1.0 signal is mapped onto 8-bit code values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRange {
    /// "TV" / "MPEG" range: Y 16..=235, Cb/Cr 16..=240. The default for video.
    Limited,
    /// "PC" / "JPEG" range: everything 0..=255.
    Full,
}

/// Normalised Y'CbCr: y in 0.0..=1.0, cb and cr in -0.5..=0.5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YCbCr {
    pub y: f32,
    pub cb: f32,
    pub cr: f32,
}

/// Convert one gamma-encoded 8-bit RGB pixel to normalised Y'CbCr.
pub fn rgb_to_ycbcr(r: u8, g: u8, b: u8, matrix: ColorMatrix) -> YCbCr {
    let (kr, kb) = matrix.coefficients();
    let kg = 1.0 - kr - kb;
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

    let y = kr * r + kg * g + kb * b;
    let cb = (b - y) / (2.0 * (1.0 - kb));
    let cr = (r - y) / (2.0 * (1.0 - kr));
    YCbCr { y, cb, cr }
}

/// Convert normalised Y'CbCr back to 8-bit RGB (clamped).
pub fn ycbcr_to_rgb(c: YCbCr, matrix: ColorMatrix) -> (u8, u8, u8) {
    let (kr, kb) = matrix.coefficients();
    let kg = 1.0 - kr - kb;

    let r = c.y + 2.0 * (1.0 - kr) * c.cr;
    let b = c.y + 2.0 * (1.0 - kb) * c.cb;
    let g = (c.y - kr * r - kb * b) / kg;
    (to_u8(r * 255.0), to_u8(g * 255.0), to_u8(b * 255.0))
}

/// Normalised luma (0..=1) -> 8-bit code value.
pub fn quantize_luma(y: f32, range: ColorRange) -> u8 {
    match range {
        ColorRange::Limited => to_u8(16.0 + 219.0 * y),
        ColorRange::Full => to_u8(255.0 * y),
    }
}

/// Normalised chroma (-0.5..=0.5) -> 8-bit code value.
pub fn quantize_chroma(c: f32, range: ColorRange) -> u8 {
    match range {
        ColorRange::Limited => to_u8(128.0 + 224.0 * c),
        ColorRange::Full => to_u8(128.0 + 255.0 * c),
    }
}

/// 8-bit luma code value -> normalised (may go outside 0..=1 for "illegal" values).
pub fn dequantize_luma(v: u8, range: ColorRange) -> f32 {
    match range {
        ColorRange::Limited => (v as f32 - 16.0) / 219.0,
        ColorRange::Full => v as f32 / 255.0,
    }
}

/// 8-bit chroma code value -> normalised.
pub fn dequantize_chroma(v: u8, range: ColorRange) -> f32 {
    match range {
        ColorRange::Limited => (v as f32 - 128.0) / 224.0,
        ColorRange::Full => (v as f32 - 128.0) / 255.0,
    }
}

fn to_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(r: u8, g: u8, b: u8, m: ColorMatrix, range: ColorRange) -> (u8, u8, u8) {
        let c = rgb_to_ycbcr(r, g, b, m);
        (
            quantize_luma(c.y, range),
            quantize_chroma(c.cb, range),
            quantize_chroma(c.cr, range),
        )
    }

    #[test]
    fn white_and_black_limited() {
        assert_eq!(
            encode(255, 255, 255, ColorMatrix::Bt709, ColorRange::Limited),
            (235, 128, 128)
        );
        assert_eq!(
            encode(0, 0, 0, ColorMatrix::Bt709, ColorRange::Limited),
            (16, 128, 128)
        );
    }

    #[test]
    fn white_and_black_full() {
        assert_eq!(
            encode(255, 255, 255, ColorMatrix::Bt709, ColorRange::Full),
            (255, 128, 128)
        );
        assert_eq!(
            encode(0, 0, 0, ColorMatrix::Bt709, ColorRange::Full),
            (0, 128, 128)
        );
    }

    #[test]
    fn pure_red_reference_values() {
        // Well-known reference values for 100% red.
        assert_eq!(
            encode(255, 0, 0, ColorMatrix::Bt601, ColorRange::Limited),
            (81, 90, 240)
        );
        assert_eq!(
            encode(255, 0, 0, ColorMatrix::Bt709, ColorRange::Limited),
            (63, 102, 240)
        );
    }

    #[test]
    fn round_trip_is_close() {
        for &(r, g, b) in &[(12, 200, 77), (255, 128, 0), (3, 3, 250), (128, 128, 128)] {
            for m in [ColorMatrix::Bt601, ColorMatrix::Bt709] {
                let c = rgb_to_ycbcr(r, g, b, m);
                let (r2, g2, b2) = ycbcr_to_rgb(c, m);
                assert!((r as i32 - r2 as i32).abs() <= 1);
                assert!((g as i32 - g2 as i32).abs() <= 1);
                assert!((b as i32 - b2 as i32).abs() <= 1);
            }
        }
    }
}
