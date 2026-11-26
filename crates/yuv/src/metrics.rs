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
