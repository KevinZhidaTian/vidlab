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
