//! Orthogonal 4-slider Writer page palette with guaranteed WCAG 2.1 contrast (>= 4.5:1).

use serde::{Deserialize, Serialize};

pub const MIN_CONTRAST_RATIO: f64 = 4.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adjustments {
    #[serde(default = "default_100")]
    pub brightness: i32,
    #[serde(default = "default_100")]
    pub contrast: i32,
    #[serde(default)]
    pub sepia: i32,
    #[serde(default)]
    pub grayscale: i32,
    #[serde(default = "default_true")]
    pub dark_mode: bool,
}

fn default_100() -> i32 {
    100
}
fn default_true() -> bool {
    true
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 100,
            contrast: 100,
            sepia: 0,
            grayscale: 0,
            dark_mode: true,
        }
    }
}

pub type Rgb = (u8, u8, u8);

fn srgb_to_linear(channel: f64) -> f64 {
    let c = channel.clamp(0.0, 255.0) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn relative_luminance(rgb: Rgb) -> f64 {
    0.2126 * srgb_to_linear(rgb.0 as f64)
        + 0.7152 * srgb_to_linear(rgb.1 as f64)
        + 0.0722 * srgb_to_linear(rgb.2 as f64)
}

pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

pub fn argb(rgb: Rgb) -> u32 {
    0xFF00_0000 | ((rgb.0 as u32) << 16) | ((rgb.1 as u32) << 8) | (rgb.2 as u32)
}

pub fn hex_color(rgb: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2)
}

fn clamp_rgb(v: [f64; 3]) -> Rgb {
    (
        v[0].round().clamp(0.0, 255.0) as u8,
        v[1].round().clamp(0.0, 255.0) as u8,
        v[2].round().clamp(0.0, 255.0) as u8,
    )
}

fn rec709_luma(v: [f64; 3]) -> f64 {
    0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]
}

fn apply_sepia(rgb: [f64; 3], amount: f64, dark_mode: bool, is_bg: bool) -> [f64; 3] {
    if amount <= 0.0 {
        return rgb;
    }
    let y = rec709_luma(rgb);
    let (wr, wg, wb) = match (dark_mode, is_bg) {
        (true, true) => (1.32, 0.99, 0.56),
        (true, false) => (1.06, 0.99, 0.84),
        (false, true) => (1.05, 0.99, 0.84),
        (false, false) => (1.24, 0.98, 0.64),
    };
    let norm = 0.2126 * wr + 0.7152 * wg + 0.0722 * wb;
    let target = [
        (y * wr / norm).min(255.0),
        (y * wg / norm).min(255.0),
        (y * wb / norm).min(255.0),
    ];
    [
        rgb[0] * (1.0 - amount) + target[0] * amount,
        rgb[1] * (1.0 - amount) + target[1] * amount,
        rgb[2] * (1.0 - amount) + target[2] * amount,
    ]
}

fn apply_grayscale(rgb: [f64; 3], amount: f64) -> [f64; 3] {
    if amount <= 0.0 {
        return rgb;
    }
    let y = rec709_luma(rgb);
    [
        rgb[0] * (1.0 - amount) + y * amount,
        rgb[1] * (1.0 - amount) + y * amount,
        rgb[2] * (1.0 - amount) + y * amount,
    ]
}

fn enforce_min_contrast(bg: [f64; 3], fg: [f64; 3], dark_mode: bool, min_ratio: f64) -> (Rgb, Rgb) {
    let mut bg_t = clamp_rgb(bg);
    let mut fg_t = clamp_rgb(fg);
    if contrast_ratio(bg_t, fg_t) >= min_ratio {
        return (bg_t, fg_t);
    }
    let fg_goal = if dark_mode { 255.0 } else { 0.0 };
    let bg_goal = if dark_mode { 0.0 } else { 255.0 };
    for step in 1..=40 {
        let t = step as f64 / 40.0;
        fg_t = clamp_rgb([
            fg[0] + (fg_goal - fg[0]) * t,
            fg[1] + (fg_goal - fg[1]) * t,
            fg[2] + (fg_goal - fg[2]) * t,
        ]);
        if contrast_ratio(bg_t, fg_t) >= min_ratio {
            return (bg_t, fg_t);
        }
    }
    fg_t = if dark_mode { (255, 255, 255) } else { (0, 0, 0) };
    for step in 1..=40 {
        let t = step as f64 / 40.0;
        bg_t = clamp_rgb([
            bg[0] + (bg_goal - bg[0]) * t,
            bg[1] + (bg_goal - bg[1]) * t,
            bg[2] + (bg_goal - bg[2]) * t,
        ]);
        if contrast_ratio(bg_t, fg_t) >= min_ratio {
            return (bg_t, fg_t);
        }
    }
    (bg_t, fg_t)
}

impl Adjustments {
    pub fn normalized(self) -> Self {
        Self {
            brightness: self.brightness.clamp(40, 140),
            contrast: self.contrast.clamp(50, 150),
            sepia: self.sepia.clamp(0, 100),
            grayscale: self.grayscale.clamp(0, 100),
            dark_mode: self.dark_mode,
        }
    }

    pub fn palette(self) -> (Rgb, Rgb) {
        let a = self.normalized();
        let (mut bg, mut fg) = if a.dark_mode {
            ([38.0, 44.0, 52.0], [224.0, 228.0, 234.0])
        } else {
            ([234.0, 232.0, 224.0], [28.0, 32.0, 38.0])
        };

        // 1) Brightness: monotonic same-direction scaling with soft shoulder compression
        let b_delta = (a.brightness - 100) as f64 / 100.0;
        if a.dark_mode {
            if b_delta < 0.0 {
                for c in &mut bg {
                    *c *= 1.0 + 0.85 * b_delta;
                }
                for c in &mut fg {
                    *c *= 1.0 + 0.34 * b_delta;
                }
            } else if b_delta > 0.0 {
                let t = b_delta / 0.40;
                for c in &mut bg {
                    *c += (108.0 - *c) * (0.78 * t);
                }
                for c in &mut fg {
                    *c += (254.0 - *c) * (0.85 * t);
                }
            }
        } else if b_delta < 0.0 {
            for c in &mut bg {
                *c *= 1.0 + 0.50 * b_delta;
            }
            for c in &mut fg {
                *c *= 1.0 + 0.45 * b_delta;
            }
        } else if b_delta > 0.0 {
            let t = b_delta / 0.40;
            for c in &mut bg {
                *c += (255.0 - *c) * (0.92 * t);
            }
            for c in &mut fg {
                *c += (58.0 - *c) * (0.55 * t);
            }
        }

        // 2) Contrast: expand/compress separation around pair midpoint
        let c_scale = if a.contrast <= 100 {
            0.62 + 0.38 * ((a.contrast - 50) as f64 / 50.0)
        } else {
            1.0 + 0.32 * ((a.contrast - 100) as f64 / 50.0)
        };
        for i in 0..3 {
            let mid = (bg[i] + fg[i]) * 0.5;
            bg[i] = (mid + (bg[i] - mid) * c_scale).clamp(4.0, 255.0);
            fg[i] = (mid + (fg[i] - mid) * c_scale).clamp(4.0, 255.0);
        }

        // 3) Sepia (before grayscale)
        let s_amt = a.sepia as f64 / 100.0;
        bg = apply_sepia(bg, s_amt, a.dark_mode, true);
        fg = apply_sepia(fg, s_amt, a.dark_mode, false);

        // 4) Grayscale (strictly after sepia)
        let g_amt = a.grayscale as f64 / 100.0;
        bg = apply_grayscale(bg, g_amt);
        fg = apply_grayscale(fg, g_amt);

        // 5) WCAG readability guard
        enforce_min_contrast(bg, fg, a.dark_mode, MIN_CONTRAST_RATIO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_grid_min_contrast_and_sepia_grayscale() {
        for dark_mode in [true, false] {
            for b in (40..=140).step_by(10) {
                for c in (50..=150).step_by(10) {
                    for s in (0..=100).step_by(20) {
                        for g in (0..=100).step_by(20) {
                            let (bg, fg) = Adjustments {
                                brightness: b,
                                contrast: c,
                                sepia: s,
                                grayscale: g,
                                dark_mode,
                            }
                            .palette();
                            assert!(contrast_ratio(bg, fg) >= 4.5);
                        }
                    }
                }
            }
            let p0 = Adjustments { sepia: 100, grayscale: 0, dark_mode, ..Default::default() }.palette();
            let p50 = Adjustments { sepia: 100, grayscale: 50, dark_mode, ..Default::default() }.palette();
            let p100 = Adjustments { sepia: 100, grayscale: 100, dark_mode, ..Default::default() }.palette();
            assert_ne!(p0.0, p50.0);
            assert_ne!(p50.0, p100.0);
            assert_eq!(p100.0 .0, p100.0 .1);
            assert_eq!(p100.0 .1, p100.0 .2);
        }
    }
}
