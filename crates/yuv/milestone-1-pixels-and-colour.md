# Milestone 1: Pixels and Colour (the `yuv` crate)

**Time:** 1–2 weeks part-time  ·  **Language:** Rust (std only, no external crates)  ·  **Part of:** `vidlab` learning plan

By the end of this milestone you will have written, from scratch:

- RGB → Y′CbCr colour conversion with BT.601 and BT.709 matrices, limited and full range
- 4:2:0 chroma subsampling
- Writers and readers for raw I420, raw NV12 and `.y4m` files
- A PSNR / difference tool (you'll reuse it in Milestone 2)
- A `vidlab` CLI that generates test patterns, converts, decodes, splits planes and compares files

Then you will **check your output against ffmpeg**, and **break things on purpose** to learn what wrong width, swapped chroma, wrong range and wrong matrix look like on screen. Those are the same bugs you will one day diagnose in production.

> **How to use this tutorial.** Each step explains the concept, then gives the complete code. You'll learn far more if you try writing each file yourself first, using the explanation and the tests, and only then compare with the code here. A finished reference copy of the whole project, `vidlab-milestone1.zip`, comes with this file. Every command and number in this guide was run and checked against that code with Rust 1.97 and ffmpeg.

---

## Contents

1. [Concepts you need first](#1-concepts-you-need-first)
2. [Set up the workspace and tools](#2-set-up-the-workspace-and-tools)
3. [Step 1: Error type](#step-1-error-type-errorrs)
4. [Step 2: Colour maths](#step-2-colour-maths-colorrs)
5. [Step 3: Reading and writing images (PPM)](#step-3-reading-and-writing-images-imagers)
6. [Step 4: The 4:2:0 frame](#step-4-the-420-frame-framers)
7. [Step 5: Y4M files](#step-5-y4m-files-y4mrs)
8. [Step 6: Comparing frames (PSNR)](#step-6-comparing-frames-metricsrs)
9. [Step 7: Wire up the library](#step-7-wire-up-the-library-librs)
10. [Step 8: The `vidlab` CLI](#step-8-the-vidlab-cli)
11. [Check your work against ffmpeg](#11-check-your-work-against-ffmpeg)
12. [Break things on purpose](#12-break-things-on-purpose)
13. [Connect it to your day job](#13-connect-it-to-your-day-job)
14. [Challenges](#14-challenges)
15. [Done-when checklist](#15-done-when-checklist)
16. [Further reading](#16-further-reading)

---

## 1. Concepts you need first

### 1.1 Why video isn't stored as RGB

Your eyes are much more sensitive to **brightness detail** than to **colour detail**. Video exploits that by splitting each pixel into:

- **Y′ (luma):** brightness, a weighted sum of R′, G′ and B′
- **Cb and Cr (chroma):** how much bluer and how much redder the pixel is than its brightness suggests

Once brightness and colour are separate, you can store colour at lower resolution and almost nobody notices. That's **chroma subsampling**, and it halves the data before any codec even starts.

> **About the prime (′):** R′G′B′ and Y′ mean the values are *gamma-encoded*, as they are in every normal image file. We do all the maths on those encoded values and never convert to linear light. That's what video standards specify, so it's correct here, not a shortcut.

### 1.2 The conversion maths

Every Y′CbCr standard uses the same formulas and differs only in two constants, **Kr** and **Kb**:

```
Y′ = Kr·R′ + Kg·G′ + Kb·B′          where Kg = 1 − Kr − Kb
Cb = (B′ − Y′) / (2·(1 − Kb))        range −0.5 … +0.5
Cr = (R′ − Y′) / (2·(1 − Kr))        range −0.5 … +0.5
```

(R′, G′, B′ normalised to 0.0–1.0.)

| Matrix | Kr | Kb | Used for |
|---|---|---|---|
| **BT.601** | 0.299 | 0.114 | SD video, JPEG |
| **BT.709** | 0.2126 | 0.0722 | HD video; nearly all SDR OTT content |
| BT.2020 | 0.2627 | 0.0593 | UHD / HDR (a later challenge) |

The weights reflect how bright each primary *looks*: green contributes most to perceived brightness, blue least.

### 1.3 Range: how 0.0–1.0 becomes 8-bit numbers

| Range | a.k.a. | Y′ codes | Cb/Cr codes | Black → white |
|---|---|---|---|---|
| **Limited** | TV, MPEG, "studio" | 16–235 | 16–240 (128 = no colour) | 16 → 235 |
| **Full** | PC, JPEG | 0–255 | 0–255 (128 = no colour) | 0 → 255 |

```
limited:  Y = 16 + 219·Y′        C = 128 + 224·Cb  (or Cr)
full:     Y = 255·Y′             C = 128 + 255·Cb  (clamped to 0–255)
```

Limited range is the default for broadcast and streaming video. **If the encoder and the decoder disagree about range, you get one of the most common picture bugs there is:** washed-out greys instead of black, or crushed shadows and clipped highlights. You'll create both on purpose in section 12.

### 1.4 Chroma subsampling

```
4:4:4               4:2:2               4:2:0
Y Y Y Y             Y Y Y Y             Y Y Y Y
C C C C             C   C               C   C        ← one chroma sample per 2×2 block
Y Y Y Y             Y Y Y Y             Y Y Y Y
C C C C             C   C
```

**4:2:0** (one Cb and one Cr per 2×2 block of pixels) is what nearly all distribution video uses. Bytes per 8-bit 1920×1080 frame:

| Format | Calculation | Bytes |
|---|---|---|
| RGB | 1920 × 1080 × 3 | 6,220,800 |
| YUV 4:2:0 | 1920 × 1080 × 1.5 | 3,110,400 |

That's 50% saved before compression has even started.

### 1.5 Memory layouts: I420 vs NV12

Same samples, different order in memory:

```
I420 (ffmpeg: yuv420p) — "planar"
[ YYYYYYYYYYYYYYYY… ][ UUUU… ][ VVVV… ]

NV12 — "semi-planar", common in hardware decoders and GPUs
[ YYYYYYYYYYYYYYYY… ][ UVUVUVUV… ]
```

**A raw `.yuv` file has no header.** Nothing in the bytes says the width, height or layout. Get any of them wrong and the file still "opens", just as garbage. That's why `.y4m` exists: it is the same raw data with a one-line text header in front.

---

## 2. Set up the workspace and tools

### Tools

| Tool | Why | Get it |
|---|---|---|
| Rust ≥ 1.85 | Edition 2024 | `rustup update` |
| ffmpeg / ffprobe | Answer key | `brew install ffmpeg` (macOS) or your package manager |
| **YUView** | View raw YUV, single planes, pixel values | https://github.com/IENT/YUView/releases |

To view `.ppm` / `.pgm` files: many image viewers open them directly. If yours doesn't, convert with `ffmpeg -i x.ppm x.png`.

### Create the workspace

```bash
mkdir vidlab && cd vidlab
git init                       # do this first so cargo doesn't create nested git repos
mkdir crates
cargo new --lib crates/yuv
cargo new cli --name vidlab
```

Then create the root `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*", "cli"]
```

Add a root `.gitignore` so build output and test media stay out of git:

```
/target
*.ppm
*.pgm
*.yuv
*.y4m
```

> Create the crates *before* writing a `members` list with globs or paths. If `members` points at a folder that doesn't exist yet, every cargo command fails.

Let the CLI use the library by adding the path dependency to `cli/Cargo.toml`:

```toml
[package]
name = "vidlab"
version = "0.1.0"
edition = "2024"

[dependencies]
yuv = { path = "../crates/yuv" }
```

Inside `crates/yuv/src/`, you'll end up with:

```
lib.rs      – module list + re-exports
error.rs    – our Error type
color.rs    – per-pixel colour maths
image.rs    – RgbImage + PPM/PGM I/O
frame.rs    – YuvFrame, 4:2:0 conversion, I420/NV12
y4m.rs      – .y4m reader/writer
metrics.rs  – PSNR and differences
```

**Why no external crates?** Every byte you parse in this milestone is simple enough to handle yourself, and doing it is the point. In later milestones, once you've written something by hand, you're free to swap in a crate (`clap` for arguments, `image` for PNG, `thiserror` for errors).

---

## Step 1: Error type (`error.rs`)

**Rust concepts:** enums with data, the `Display` and `Error` traits, `From` conversions so `?` works, type aliases.

Everything that can fail returns `Result<T>`. Implementing `From<std::io::Error>` lets us write `file.read_exact(&mut buf)?` and have the I/O error converted into our type automatically.

```rust
use std::fmt;

/// Everything that can go wrong in the `yuv` crate.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// A file header (PPM, Y4M) we could not understand.
    BadHeader(String),
    /// A buffer whose length does not match width × height × format.
    WrongSize {
        expected: usize,
        actual: usize,
    },
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::BadHeader(msg) => write!(f, "bad header: {msg}"),
            Error::WrongSize { expected, actual } => {
                write!(
                    f,
                    "wrong buffer size: expected {expected} bytes, got {actual}"
                )
            }
            Error::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

// Lets us use `?` on std::io results inside functions returning our Result.
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
```

> In Milestone 3 you'll replace this hand-written boilerplate with the `thiserror` crate. Writing it once by hand shows you what that crate generates.

---

## Step 2: Colour maths (`color.rs`)

**Rust concepts:** `#[derive]` (`Copy`, `PartialEq`, `Debug`), `match` on enums, `impl` blocks, `f32` maths, unit tests in a `#[cfg(test)]` module.

### Try it yourself first

Before looking at the code, **work out by hand** what 100% red (R′=1, G′=0, B′=0) becomes in BT.709 limited range:

1. Y′ = 0.2126 · 1 = 0.2126 → Y = 16 + 219 · 0.2126 = 62.56 → **63**
2. Cb = (0 − 0.2126) / (2 · 0.9278) = −0.1146 → 128 + 224 · (−0.1146) = 102.3 → **102**
3. Cr = (1 − 0.2126) / (2 · 0.7874) = 0.5 → 128 + 112 = **240**

Now do the same for BT.601. You should get **(81, 90, 240)**. These two triplets are well-known reference values, and they're in the tests below.

Notice we split the conversion into two stages: **normalised floats** (`YCbCr`) and **quantising to 8-bit**. That matters in Step 4, where chroma has to be *averaged* before it's rounded.

```rust
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
```

Run the tests:

```bash
cargo test -p yuv
```

> **Why `.round()` and then `.clamp()`?** Real-world RGB→YUV→RGB can overshoot: a highly saturated colour can produce R = 256.7. Clamping keeps it in 0–255. Leave the clamp out and Rust's `as u8` cast saturates anyway; in C the same mistake would silently wrap around, a classic source of speckled pixels.

---

## Step 3: Reading and writing images (`image.rs`)

**Rust concepts:** generic functions over `BufRead` / `Write` (so the same code works with files *and* in-memory `&[u8]` / `Vec<u8>` in tests), `read_exact`, building a `String` byte by byte.

We need an RGB input. Rather than pull in a PNG decoder, we'll use **PPM**, the simplest image format there is:

```
P6            ← magic: binary RGB
# comment     ← optional comments
1280 720      ← width height
255           ← max value
<1280×720×3 raw bytes of R,G,B,R,G,B…>
```

`ffmpeg -i photo.jpg photo.ppm` converts any image into one, so you can feed real photos into your own code.

We also write **PGM** (`P5`), the greyscale version, so you can view a single Y, U or V plane as a picture.

```rust
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
```

> **Testing trick:** the tests write to `Vec<u8>` and read back from `&buf[..]`. Because the functions are generic over `Write` and `BufRead`, the tests never touch the disk. You'll use the same trick for every parser in Milestones 3 and 4.

---

## Step 4: The 4:2:0 frame (`frame.rs`)

**Rust concepts:** iterator chains (`flat_map`, `map`, `collect`), `Vec::with_capacity`, `zip`, `step_by`, associated functions vs methods, returning `Result` from a constructor.

This is the heart of the milestone.

### Subsampling: average, then round

For each 2×2 block, average the four **floating-point** Cb values, *then* quantise. Rounding first and averaging after loses precision.

```
 x:  0    1    2    3              chroma x:  0         1
   ┌────┬────┬────┬────┐                   ┌─────────┬─────────┐
y0 │ a  │ b  │ e  │ f  │                   │ avg     │ avg     │
   ├────┼────┼────┼────┤        ──►        │ (a,b,   │ (e,f,   │
y1 │ c  │ d  │ g  │ h  │                   │  c,d)   │  g,h)   │
   └────┴────┴────┴────┘                   └─────────┴─────────┘
```

**Odd sizes:** a 5-pixel-wide image needs 3 chroma columns, not 2. Hence `div_ceil(2)`, and the `if x < w && y < h` guard for the partial blocks at the edges.

> **Chroma siting (advanced, for later):** plain averaging puts each chroma sample at the *centre* of its 2×2 block (JPEG / MPEG-1 style, which is what `C420jpeg` means in the Y4M header). MPEG-2, H.264 and HEVC default to *left* siting. It's a subtle difference; the challenges section comes back to it.

### Going back to RGB

`to_rgb` uses **nearest-neighbour** upsampling: every pixel in a 2×2 block reuses the same chroma sample. It's simple and makes chroma resolution visible. A real player uses a smoother filter (another challenge).

```rust
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
```

> **Read the comment on `from_bytes`.** The only thing it can check is the total length. It cannot know whether the bytes are really 1280×720 I420 or 640×1440 NV12. You'll exploit that in section 12.

---

## Step 5: Y4M files (`y4m.rs`)

**Rust concepts:** generic structs (`Y4mWriter<W: Write>`), **implementing `Iterator`**, `split_once`, `split_at`, match guards (`"C" if …`), the `?` operator inside `Option`-returning code.

Y4M is raw I420 with headers:

```
YUV4MPEG2 W1280 H720 F25:1 Ip A1:1 C420jpeg XCOLORRANGE=LIMITED\n
FRAME\n<1,382,400 bytes of I420>
FRAME\n<1,382,400 bytes of I420>
…
```

| Tag | Meaning |
|---|---|
| `W` / `H` | width / height |
| `F25:1` | frame rate as a fraction (`F30000:1001` = 29.97) |
| `Ip` | progressive (`It` / `Ib` = interlaced, top/bottom field first) |
| `A1:1` | pixel aspect ratio |
| `C420jpeg` | 4:2:0, centre-sited chroma |
| `XCOLORRANGE=` | extension tag that ffmpeg reads and writes |

Note what Y4M **does not** record: the colour matrix. That fact comes back in section 12.

Making the reader an `Iterator` means callers can write `for frame in reader { … }`. You'll need multi-frame reading in Milestone 2.

```rust
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
```

---

## Step 6: Comparing frames (`metrics.rs`)

**Rust concepts:** `u8::abs_diff`, widening to `u64` to avoid overflow, `f64::INFINITY`.

**PSNR** (peak signal-to-noise ratio) is the classic objective quality metric:

```
MSE  = mean((a − b)²)
PSNR = 10 · log10(255² / MSE)   dB
```

Rough guide for 8-bit video: **> 50 dB** is practically identical, **40 dB** is high quality, **30 dB** shows visible artefacts, and **< 25 dB** is bad. In Milestone 2 you'll plot PSNR against QP for your own codec; in Milestone 6 you'll find out why VMAF replaced it.

```rust
//! Compare two planes: how different are they? You will reuse PSNR in Milestone 2.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneStats {
    pub max_abs_diff: u8,
    pub mean_abs_diff: f64,
    /// Peak signal-to-noise ratio in dB. `f64::INFINITY` when the planes are identical.
    pub psnr: f64,
}

pub fn compare_planes(a: &[u8], b: &[u8]) -> PlaneStats {
    assert_eq!(a.len(), b.len(), "planes must be the same size");
    let (mut max, mut sum_abs, mut sum_sq) = (0u8, 0u64, 0u64);
    for (&x, &y) in a.iter().zip(b) {
        let d = x.abs_diff(y);
        max = max.max(d);
        sum_abs += d as u64;
        sum_sq += (d as u64) * (d as u64);
    }
    let n = a.len() as f64;
    let mse = sum_sq as f64 / n;
    let psnr = if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    };
    PlaneStats {
        max_abs_diff: max,
        mean_abs_diff: sum_abs as f64 / n,
        psnr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_infinite() {
        assert_eq!(compare_planes(&[1, 2, 3], &[1, 2, 3]).psnr, f64::INFINITY);
    }

    #[test]
    fn known_psnr() {
        // Every sample off by 1 -> MSE 1 -> PSNR = 10*log10(65025) ≈ 48.13 dB
        let s = compare_planes(&[10; 100], &[11; 100]);
        assert_eq!(s.max_abs_diff, 1);
        assert!((s.psnr - 48.13).abs() < 0.01);
    }
}
```

---

## Step 7: Wire up the library (`lib.rs`)

**Rust concepts:** modules, `pub use` re-exports.

```rust
//! `yuv`: pixel formats and colour conversion for vidlab.

pub mod color;
pub mod error;
pub mod frame;
pub mod image;
pub mod metrics;
pub mod y4m;

// Re-export the main types so users can write `use yuv::{YuvFrame, ColorMatrix}`.
pub use color::{ColorMatrix, ColorRange};
pub use error::{Error, Result};
pub use frame::{PixelFormat, YuvFrame};
pub use image::RgbImage;
```

Run everything:

```bash
cargo test -p yuv
```

You should see **13 passed**.

---

## Step 8: The `vidlab` CLI

**Rust concepts:** `std::env::args`, `HashMap`, `Box<dyn Error>` for "any error" in application code, `let … else`, `ExitCode`, `std::mem::swap`, `Option::transpose`, `is_some_and`.

Five subcommands:

| Command | What it does |
|---|---|
| `bars` | Generates a test image: 8 colour bars, a grey ramp, and 16 grey steps |
| `convert` | PPM → `.y4m` or raw `.yuv` (I420/NV12), any matrix/range |
| `decode` | `.y4m`/`.yuv` → PPM, with **deliberately overridable** settings |
| `planes` | Splits a frame into `_y.pgm`, `_u.pgm` and `_v.pgm` |
| `compare` | Per-plane max diff, mean diff and PSNR |

The argument parser is hand-written to avoid dependencies. Swapping it for `clap` later is a good small exercise.

`cli/src/main.rs`:

```rust
//! vidlab: a hand-built toolkit for learning video engineering.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use yuv::image::{read_ppm, write_pgm, write_ppm};
use yuv::metrics::compare_planes;
use yuv::y4m::{Y4mReader, Y4mWriter};
use yuv::{ColorMatrix, ColorRange, PixelFormat, RgbImage, YuvFrame};

const USAGE: &str = "\
usage:
  vidlab bars    <out.ppm> [--width 1280] [--height 720]
  vidlab convert <in.ppm> <out.y4m|out.yuv> [--matrix 709|601] [--range limited|full] [--format i420|nv12]
  vidlab decode  <in.y4m|in.yuv> <out.ppm> [--width W --height H] [--matrix ..] [--range ..] [--format ..] [--swap-uv]
  vidlab planes  <in.y4m|in.yuv> <out_prefix> [--width W --height H] [--format ..]
  vidlab compare <a.y4m|a.yuv> <b.y4m|b.yuv> [--width W --height H]";

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> AnyResult<()> {
    let args = Args::parse(std::env::args().skip(1))?;
    let Some(cmd) = args.positional.first() else {
        return Err("no command given".into());
    };
    match cmd.as_str() {
        "bars" => cmd_bars(&args),
        "convert" => cmd_convert(&args),
        "decode" => cmd_decode(&args),
        "planes" => cmd_planes(&args),
        "compare" => cmd_compare(&args),
        other => Err(format!("unknown command {other:?}").into()),
    }
}

// ---------- commands ----------

/// Generate a test image: 8 colour bars on top, a grey ramp and grey steps underneath.
fn cmd_bars(args: &Args) -> AnyResult<()> {
    let out = args.pos(1, "output path")?;
    let w = args.num("width", 1280)?;
    let h = args.num("height", 720)?;
    let bars = [
        (255, 255, 255),
        (255, 255, 0),
        (0, 255, 255),
        (0, 255, 0),
        (255, 0, 255),
        (255, 0, 0),
        (0, 0, 255),
        (0, 0, 0),
    ];
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let px = if y < h * 2 / 3 {
                bars[x * bars.len() / w]
            } else if y < h * 5 / 6 {
                let g = (x * 255 / (w - 1)) as u8; // smooth ramp 0..255
                (g, g, g)
            } else {
                let g = ((x * 16 / w) * 17) as u8; // 16 steps: 0, 17, 34 .. 255
                (g, g, g)
            };
            img.set(x, y, px);
        }
    }
    write_ppm(BufWriter::new(File::create(out)?), &img)?;
    println!("wrote {w}x{h} test image to {out}");
    Ok(())
}

fn cmd_convert(args: &Args) -> AnyResult<()> {
    let input = args.pos(1, "input .ppm")?;
    let output = args.pos(2, "output .y4m or .yuv")?;
    let matrix = args.matrix()?;
    let range = args.range()?.unwrap_or(ColorRange::Limited);

    let img = read_ppm(BufReader::new(File::open(input)?))?;
    let frame = YuvFrame::from_rgb(&img, matrix, range);
    let mut out = BufWriter::new(File::create(output)?);

    if is_y4m(output) {
        let mut w = Y4mWriter::new(out, frame.width, frame.height, (25, 1), range)?;
        w.write_frame(&frame)?;
    } else {
        let format = args.format()?;
        out.write_all(&frame.to_bytes(format))?;
        println!(
            "raw output has no header: remember {}x{} {:?}",
            frame.width, frame.height, format
        );
    }
    println!(
        "{input}: {} bytes as RGB -> {} bytes as 4:2:0 ({matrix:?}, {range:?})",
        img.data.len(),
        YuvFrame::frame_size(frame.width, frame.height)
    );
    Ok(())
}

fn cmd_decode(args: &Args) -> AnyResult<()> {
    let input = args.pos(1, "input")?;
    let output = args.pos(2, "output .ppm")?;
    let (mut frame, file_range) = load_frame(input, args)?;
    if args.flag("swap-uv") {
        std::mem::swap(&mut frame.u, &mut frame.v);
    }
    // An explicit --range overrides what the file says. That is how you simulate a buggy player.
    let range = args.range()?.or(file_range).unwrap_or(ColorRange::Limited);
    let img = frame.to_rgb(args.matrix()?, range);
    write_ppm(BufWriter::new(File::create(output)?), &img)?;
    println!(
        "decoded {}x{} as {:?} / {range:?} -> {output}",
        frame.width,
        frame.height,
        args.matrix()?
    );
    Ok(())
}

fn cmd_planes(args: &Args) -> AnyResult<()> {
    let input = args.pos(1, "input")?;
    let prefix = args.pos(2, "output prefix")?;
    let (f, _) = load_frame(input, args)?;
    let (cw, ch) = (f.chroma_width(), f.chroma_height());
    for (name, plane, w, h) in [
        ("y", &f.y, f.width, f.height),
        ("u", &f.u, cw, ch),
        ("v", &f.v, cw, ch),
    ] {
        let path = format!("{prefix}_{name}.pgm");
        write_pgm(BufWriter::new(File::create(&path)?), w, h, plane)?;
        println!("{path}: {w}x{h}");
    }
    Ok(())
}

fn cmd_compare(args: &Args) -> AnyResult<()> {
    let (a, _) = load_frame(args.pos(1, "first file")?, args)?;
    let (b, _) = load_frame(args.pos(2, "second file")?, args)?;
    if (a.width, a.height) != (b.width, b.height) {
        return Err(format!(
            "size mismatch: {}x{} vs {}x{}",
            a.width, a.height, b.width, b.height
        )
        .into());
    }
    println!("plane   max_diff  mean_diff   psnr_db");
    for (name, pa, pb) in [("Y", &a.y, &b.y), ("U", &a.u, &b.u), ("V", &a.v, &b.v)] {
        let s = compare_planes(pa, pb);
        println!(
            "{name:<7} {:>8}  {:>9.4}  {:>8.2}",
            s.max_abs_diff, s.mean_abs_diff, s.psnr
        );
    }
    Ok(())
}

// ---------- helpers ----------

/// Load the first frame of a .y4m, or a raw .yuv (which needs --width/--height/--format).
fn load_frame(path: &str, args: &Args) -> AnyResult<(YuvFrame, Option<ColorRange>)> {
    if is_y4m(path) {
        let mut reader = Y4mReader::new(BufReader::new(File::open(path)?))?;
        let range = reader.header.range;
        let frame = reader.next().ok_or("y4m file has no frames")??;
        Ok((frame, range))
    } else {
        let (Some(w), Some(h)) = (args.opt("width"), args.opt("height")) else {
            return Err(format!("{path} is raw YUV: pass --width and --height").into());
        };
        let (w, h): (usize, usize) = (w.parse()?, h.parse()?);
        let data = std::fs::read(path)?;
        let size = YuvFrame::frame_size(w, h);
        if data.len() < size {
            return Err(format!(
                "{path} has {} bytes, one {w}x{h} frame needs {size}",
                data.len()
            )
            .into());
        }
        Ok((
            YuvFrame::from_bytes(&data[..size], w, h, args.format()?)?,
            None,
        ))
    }
}

fn is_y4m(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("y4m"))
}

/// Minimal argument parser: positional args plus `--key value` options and `--flag`s.
struct Args {
    positional: Vec<String>,
    options: HashMap<String, String>,
}

impl Args {
    fn parse(iter: impl Iterator<Item = String>) -> AnyResult<Self> {
        const FLAGS: [&str; 1] = ["swap-uv"];
        let mut positional = Vec::new();
        let mut options = HashMap::new();
        let mut iter = iter.peekable();
        while let Some(a) = iter.next() {
            if let Some(key) = a.strip_prefix("--") {
                if FLAGS.contains(&key) {
                    options.insert(key.to_string(), String::new());
                } else {
                    let val = iter.next().ok_or(format!("--{key} needs a value"))?;
                    options.insert(key.to_string(), val);
                }
            } else {
                positional.push(a);
            }
        }
        Ok(Args {
            positional,
            options,
        })
    }

    fn pos(&self, i: usize, what: &str) -> AnyResult<&str> {
        self.positional
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}").into())
    }

    fn opt(&self, key: &str) -> Option<&str> {
        self.options.get(key).map(String::as_str)
    }

    fn flag(&self, key: &str) -> bool {
        self.options.contains_key(key)
    }

    fn num(&self, key: &str, default: usize) -> AnyResult<usize> {
        Ok(self
            .opt(key)
            .map(str::parse)
            .transpose()?
            .unwrap_or(default))
    }

    fn matrix(&self) -> AnyResult<ColorMatrix> {
        match self.opt("matrix").unwrap_or("709") {
            "709" => Ok(ColorMatrix::Bt709),
            "601" => Ok(ColorMatrix::Bt601),
            m => Err(format!("unknown matrix {m:?}").into()),
        }
    }

    fn range(&self) -> AnyResult<Option<ColorRange>> {
        match self.opt("range") {
            None => Ok(None),
            Some("limited" | "tv") => Ok(Some(ColorRange::Limited)),
            Some("full" | "pc") => Ok(Some(ColorRange::Full)),
            Some(r) => Err(format!("unknown range {r:?}").into()),
        }
    }

    fn format(&self) -> AnyResult<PixelFormat> {
        match self.opt("format").unwrap_or("i420") {
            "i420" | "yuv420p" => Ok(PixelFormat::I420),
            "nv12" => Ok(PixelFormat::Nv12),
            f => Err(format!("unknown format {f:?}").into()),
        }
    }
}
```

Try it:

```bash
cargo build
alias vidlab=./target/debug/vidlab   # or use: cargo run -q -p vidlab -- <args>

vidlab bars bars.ppm
vidlab convert bars.ppm bars.y4m
vidlab decode bars.y4m back.ppm
vidlab planes bars.y4m bars
```

Expected:

```
wrote 1280x720 test image to bars.ppm
bars.ppm: 2764800 bytes as RGB -> 1382400 bytes as 4:2:0 (Bt709, Limited)
decoded 1280x720 as Bt709 / Limited -> back.ppm
bars_y.pgm: 1280x720
bars_u.pgm: 640x360
bars_v.pgm: 640x360
```

Open `bars.ppm` and `back.ppm` side by side. They should look identical.

---

## 11. Check your work against ffmpeg

This is where ffmpeg becomes your **answer key**.

### Does ffmpeg understand your Y4M?

```bash
ffprobe -v error -show_entries stream=width,height,pix_fmt,color_range,r_frame_rate \
        -of default=nw=1 bars.y4m
```

```
width=1280
height=720
pix_fmt=yuv420p
color_range=tv
r_frame_rate=25/1
```

ffmpeg read your header correctly, including `XCOLORRANGE` (shown as `tv`).

### Does your conversion match ffmpeg's?

```bash
ffmpeg -i bars.ppm -vf "scale=out_color_matrix=bt709:out_range=tv" \
       -pix_fmt yuv420p -f rawvideo ff709.yuv

vidlab compare bars.y4m ff709.yuv --width 1280 --height 720
```

Result:

```
plane   max_diff  mean_diff   psnr_db
Y              1     0.0020     75.22
U              7     0.0236     57.54
V              7     0.1059     54.77
```

**How to read it:**

- **Y: max difference 1.** Your luma matches ffmpeg's to within rounding. ✅
- **U/V: max 7, but a mean of only about 0.02–0.1.** Nearly every chroma sample matches. The few that differ sit on the **edges between bars**, because ffmpeg's scaler uses a smoother filter than your box average when it downsamples chroma. That's a filter choice, not a bug. Open `ff709.yuv` in YUView (1280×720, YUV 4:2:0 8-bit) and look at the U plane at a bar edge to see it.

### The gotcha you just discovered

Now run ffmpeg **without** saying which matrix to use:

```bash
ffmpeg -i bars.ppm -pix_fmt yuv420p -f rawvideo ffdefault.yuv
vidlab compare bars.y4m ffdefault.yuv --width 1280 --height 720
```

```
plane   max_diff  mean_diff   psnr_db
Y             28     9.1673     25.16
```

Luma is off by up to 28! Now try your **BT.601** conversion:

```bash
vidlab convert bars.ppm bars601.y4m --matrix 601
vidlab compare bars601.y4m ffdefault.yuv --width 1280 --height 720
```

```
plane   max_diff  mean_diff   psnr_db
Y              1     0.0007     79.99
```

A match. **When you don't specify a matrix, ffmpeg's RGB→YUV conversion uses BT.601**, even for HD-sized frames. Many HD files in the wild have subtly wrong colours because of exactly this: they were converted with 601 and played back as 709. You found it with your own tool.

---

## 12. Break things on purpose

Each experiment simulates a real bug. Run it, open the output, and memorise the look. Next time you see it in production, you'll recognise it at a glance.

```bash
vidlab convert bars.ppm bars.yuv                    # raw I420
vidlab convert bars.ppm bars_nv12.yuv --format nv12 # raw NV12
vidlab convert bars.ppm barsfull.y4m --range full   # full range
```

### A. Wrong width (stride bug)

```bash
vidlab decode bars.yuv wrong_width.ppm --width 1276 --height 720
```

**You'll see:** the picture **sheared diagonally**, with bars running at an angle and colour bands smeared across. Each row starts 4 bytes "early", and the error accumulates row by row.

**In real life:** the "stride" or "line size" bug. Decoders often pad rows for memory alignment (a 1280-wide row may be stored in 1344 bytes). Code that assumes stride equals width produces exactly this picture.

### B. U and V swapped

```bash
vidlab decode bars.y4m swapped.ppm --swap-uv
```

**You'll see:** brightness and shapes are correct, but the hues trade places: **red ↔ blue, yellow ↔ cyan**. White, black and the grey ramp are unaffected, because grey has no chroma.

**In real life:** mixing up I420 with YV12 (same layout but V before U), or NV12 with NV21. **Faces turning blue or purple** is the classic giveaway.

### C. NV12 read as I420

```bash
vidlab decode bars_nv12.yuv nv12_as_i420.ppm --width 1280 --height 720 --format i420
```

**You'll see:** the shapes are right (luma is fine) but the colour is nonsense: a repeating tiled pattern of tints with fine stripes. The interleaved `UVUV…` bytes are being read as if all the U came first.

**In real life:** passing a hardware decoder's NV12 output to code expecting planar data.

### D. Limited range decoded as full

```bash
vidlab decode bars.y4m limited_as_full.ppm --range full
```

**You'll see:** a **washed-out, low-contrast** picture. Black is dark grey (code 16 is treated as if 0 were black) and white is light grey (235 treated as if 255 were white).

**In real life:** the most common range bug of all. Browsers, TVs or screen-capture tools that assume full range on normal video produce it.

### E. Full range decoded as limited

```bash
vidlab decode barsfull.y4m full_as_limited.ppm --range limited
```

**You'll see:** **too much contrast**. Compare the grey steps at the bottom with `bars.ppm`: the darkest steps merge into black (**crushed blacks**) and the brightest merge into white (**clipped highlights**). Detail in shadows and highlights is lost.

**In real life:** content mastered in full range (game capture, screen recordings, some phone video) played by a pipeline that assumes limited range.

### F. Wrong matrix (709 decoded as 601)

```bash
vidlab decode bars.y4m wrong_matrix.ppm --matrix 601
```

**You'll see:** a much subtler change. The white, grey and black areas are perfect, but colours shift. Measured on the bars:

| Bar | Correct | Decoded as 601 |
|---|---|---|
| Red | (255, 0, 0) | (233, 0, 2) |
| Green | (0, 255, 0) | (20, 255, 9) |
| Cyan | (0, 255, 255) | (22, 255, 253) |
| Magenta | (255, 0, 255) | (235, 0, 246) |

**In real life:** reds slightly dull, greens slightly yellow, skin tones a bit off. It's hard to spot without a reference. Remember that Y4M doesn't store the matrix, and plenty of streams don't signal it either. The decoder guesses, and usually picks 709 for HD and 601 for SD.

### G. Look at the planes

```bash
vidlab planes bars.y4m bars
```

Open `bars_y.pgm`, `bars_u.pgm` and `bars_v.pgm`:

- **Y** is a perfect black-and-white version of the picture. That's literally what a black-and-white TV displayed.
- **U and V** are a quarter of the size, mostly mid-grey (128 = no colour), with light and dark areas where a colour is bluer/redder or less blue/red. The grey ramp at the bottom is **flat mid-grey** in both, because grey has no chroma.

Now do the same with a **real photo**:

```bash
ffmpeg -i some_photo.jpg -vf "scale=1280:720" photo.ppm
vidlab convert photo.ppm photo.y4m
vidlab planes photo.y4m photo
```

The chroma planes of a natural image are blurry and low-detail. That's why throwing away three-quarters of the chroma is nearly invisible, and why codecs spend most of their bits on luma.

### H. Explore in YUView

Open `bars.y4m` in YUView. Then:

1. Turn on the **pixel value overlay** (zoom right in) and confirm white is `Y=235 U=128 V=128` and black is `Y=16`.
2. Switch the displayed component to **Y only**, **U only** and **V only**.
3. Open `bars.yuv` (raw). YUView will ask for the size and format. Enter them wrongly on purpose, then correctly.

---

## 13. Connect it to your day job

Every concept here appears in your OTT pipeline:

```bash
ffprobe -v error -select_streams v:0 \
  -show_entries stream=pix_fmt,color_range,color_space,color_transfer,color_primaries,chroma_location \
  -of default=nw=1 your_mediaconvert_output.mp4
```

| Field | Means | Typical HD SDR value |
|---|---|---|
| `pix_fmt` | sampling + layout | `yuv420p` |
| `color_range` | limited/full | `tv` (limited) |
| `color_space` | the **matrix** | `bt709` |
| `color_transfer` | gamma curve | `bt709` |
| `color_primaries` | which red/green/blue | `bt709` |
| `chroma_location` | chroma siting | `left` |

Things to try at work:

1. Run this on a few of your MediaConvert or MediaLive outputs, and on the **source mezzanine** files. Are the colour fields present? Do the outputs match the inputs?
2. Look at MediaConvert's video colour settings (colour metadata insertion and colour space conversion) and work out how each one maps to these fields.
3. Fields shown as `unknown` mean the stream **doesn't signal** that information, so every player guesses. Different guesses on different devices is how you get "the colours look different on the TV app".

> In **Milestone 3** you'll parse these same fields yourself from the H.264 SPS (the VUI section: `video_full_range_flag`, `colour_primaries`, `matrix_coefficients`).

---

## 14. Challenges

These are deliberately left without solutions. Each one teaches something new.

1. **4:2:2 and 4:4:4.** Add a `ChromaSubsampling` enum and generalise `YuvFrame`. *(Rust: designing an enum that changes buffer sizes.)*
2. **Moving test pattern.** Add `vidlab bars-anim out.y4m --frames 60` that shifts the bars a few pixels each frame. Play it in YUView or with `ffplay out.y4m`. **You'll use this in Milestone 2** to test motion search.
3. **Better upsampling.** Implement bilinear chroma upsampling in `to_rgb`. Measure the difference: add PPM-to-PPM comparison to `compare` and check PSNR of RGB→YUV→RGB with nearest vs bilinear.
4. **Left-sited chroma.** Downsample with a `[1, 2, 1] / 4` horizontal filter (left siting). Compare with ffmpeg again: do the U/V edge differences get smaller?
5. **10-bit.** Make planes generic: `struct Plane<T> { width, height, data: Vec<T> }` for `u8` and `u16`. 10-bit limited range is 64–940 for luma. *(Rust: generics with trait bounds. This is the road to HDR.)*
6. **BT.2020.** Add the matrix. Then read about why HDR needs a different *transfer function* (PQ / HLG), not only a different matrix.
7. **Real footage.** Download a raw `.y4m` clip from Xiph (https://media.xiph.org/video/derf/) and run your tools on it. Make sure your reader handles every frame.
8. **Speed.** Convert a 1920×1080 frame 100 times and time it. Then run with `cargo run --release` and compare. Then parallelise rows with `rayon` (your first external crate) and measure again.
9. **Replace the argument parser with `clap`.** Use derive macros (`#[derive(Parser)]`).

---

## 15. Done-when checklist

- [ ] `cargo test -p yuv` passes (13 tests)
- [ ] You can calculate the BT.709 limited-range Y/Cb/Cr for pure red by hand and get (63, 102, 240)
- [ ] `vidlab compare` against ffmpeg's BT.709 output shows **Y max diff ≤ 1**
- [ ] You can explain why chroma differs slightly at the bar edges
- [ ] You know ffmpeg's default RGB→YUV matrix and how to override it
- [ ] You've run all of experiments A–F and can name each bug from a screenshot alone
- [ ] You've run `ffprobe` on a real work output and can explain every colour field
- [ ] At least challenge 2 (moving bars) is done, ready for Milestone 2

---

## 16. Further reading

Read these alongside the code, not before it:

- **Charles Poynton, *Digital Video and HD: Algorithms and Interfaces*** (2nd ed.): the chapters on luma, Y′CbCr, colour range and chroma subsampling. The authoritative source for everything in this milestone.
- **ITU-R BT.601** and **ITU-R BT.709**: the standards themselves, free from https://www.itu.int/rec/R-REC-BT.709 and https://www.itu.int/rec/R-REC-BT.601. Find the Kr/Kb values and the 16–235 range in them.
- **YUV4MPEG2 format** (MultimediaWiki): https://wiki.multimedia.cx/index.php/YUV4MPEG2
- **Netpbm PPM format**: https://netpbm.sourceforge.net/doc/ppm.html
- **Leandro Moreira, *Digital Video Introduction***, the sections on colour and chroma subsampling: https://github.com/leandromoreira/digital_video_introduction
- **The Rust Book**: chapters 6 (enums and match), 8 (collections), 9 (error handling), 10 (generics and traits) and 13 (iterators) map directly onto Steps 1–5.

**Next up: Milestone 2, the toy codec.** You'll take the moving test pattern from challenge 2, split each frame into 8×8 blocks, run them through a DCT you write yourself, and watch your PSNR tool tell you exactly what quantisation costs.
