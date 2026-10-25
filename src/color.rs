//! Farb-Hilfen: HSV, Schattierungen, Zufall.

use crate::skin::Rgba;

#[derive(Clone, Copy, Debug)]
pub struct Hsv {
    pub h: f32, // 0..360
    pub s: f32, // 0..1
    pub v: f32, // 0..1
}

pub fn to_hsv(c: Rgba) -> Hsv {
    let (r, g, b) = (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    Hsv { h, s, v: max }
}

pub fn from_hsv(hsv: Hsv, a: u8) -> Rgba {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let f = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [f(r), f(g), f(b), a]
}

/// Heller/dunkler mit leichter Farbton-Verschiebung (wie in der Pixel-Art üblich):
/// Lichter wandern Richtung Gelb, Schatten Richtung Blau.
pub fn shift_light(c: Rgba, amount: f32, hue_shift: bool) -> Rgba {
    let mut hsv = to_hsv(c);
    if amount >= 0.0 {
        hsv.v += (1.0 - hsv.v) * amount + amount * 0.15;
        hsv.s *= 1.0 - amount * 0.35;
    } else {
        hsv.v *= 1.0 + amount;
        hsv.s = (hsv.s * (1.0 - amount * 0.25)).min(1.0);
    }
    if hue_shift && hsv.s > 0.05 {
        let target = if amount >= 0.0 { 60.0 } else { 240.0 };
        let diff = ((target - hsv.h + 540.0) % 360.0) - 180.0;
        hsv.h += diff * amount.abs() * 0.12;
    }
    from_hsv(hsv, c[3])
}

/// Schattierungsleiste einer Farbe, von dunkel bis hell.
pub fn shades(c: Rgba, n: usize) -> Vec<Rgba> {
    (0..n)
        .map(|k| {
            let t = k as f32 / (n - 1) as f32 * 2.0 - 1.0; // -1..1
            shift_light(c, t * 0.6, true)
        })
        .collect()
}

pub fn adjust(c: Rgba, hue: f32, sat: f32, bright: f32) -> Rgba {
    if c[3] == 0 {
        return c;
    }
    let mut hsv = to_hsv(c);
    hsv.h += hue;
    hsv.s = if sat >= 0.0 { hsv.s + (1.0 - hsv.s) * sat } else { hsv.s * (1.0 + sat) };
    hsv.v = if bright >= 0.0 { hsv.v + (1.0 - hsv.v) * bright } else { hsv.v * (1.0 + bright) };
    from_hsv(hsv, c[3])
}

pub fn to_hex(c: Rgba) -> String {
    if c[3] == 255 {
        format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3])
    }
}

pub fn from_hex(s: &str) -> Option<Rgba> {
    let s = s.trim().trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    match s.len() {
        6 => Some([p(0)?, p(2)?, p(4)?, 255]),
        8 => Some([p(0)?, p(2)?, p(4)?, p(6)?]),
        3 => {
            let q = |i: usize| u8::from_str_radix(s.get(i..i + 1)?, 16).ok().map(|v| v * 17);
            Some([q(0)?, q(1)?, q(2)?, 255])
        }
        _ => None,
    }
}

/// Kleiner, schneller Zufallsgenerator (xorshift).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    pub fn from_time() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Rng::new(t ^ 0x9E3779B97F4A7C15)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    /// Gleichverteilt in 0..1
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n.max(1)
    }
}

/// Rausch-Variante einer Farbe (für den Rauschpinsel).
pub fn noise_color(c: Rgba, rng: &mut Rng, amount: f32, levels: u32, hue_jitter: f32) -> Rgba {
    // feste Helligkeitsstufen, alle gleich häufig → ruhiges, stimmiges Muster
    let r = if levels >= 2 {
        let l = (levels - 1) as f32;
        rng.range(levels as usize) as f32 / l * 2.0 - 1.0
    } else {
        rng.f32() * 2.0 - 1.0
    };
    let mut out = shift_light(c, r * amount, false);
    if hue_jitter > 0.0 {
        let mut hsv = to_hsv(out);
        hsv.h += (rng.f32() * 2.0 - 1.0) * hue_jitter * 30.0;
        out = from_hsv(hsv, out[3]);
    }
    out
}

pub fn dist2(a: Rgba, b: Rgba) -> i32 {
    let dr = a[0] as i32 - b[0] as i32;
    let dg = a[1] as i32 - b[1] as i32;
    let db = a[2] as i32 - b[2] as i32;
    let da = a[3] as i32 - b[3] as i32;
    2 * dr * dr + 4 * dg * dg + 3 * db * db + 3 * da * da
}

/// Farbähnlichkeit für Füllen/Ersetzen; `tol` 0..100.
pub fn similar(a: Rgba, b: Rgba, tol: f32) -> bool {
    if a[3] == 0 && b[3] == 0 {
        return true;
    }
    let max = (tol / 100.0 * 255.0).powi(2) * 3.0;
    (dist2(a, b) as f32) <= max
}

// ------------------------------------------------------------------ Lab-Farbraum
// Wahrnehmungsnaher Farbraum: Abstände entsprechen ungefähr dem, was das Auge sieht.

fn srgb_to_lin(c: u8) -> f32 {
    let v = c as f32 / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn lin_to_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

pub fn to_lab(c: Rgba) -> [f32; 3] {
    let (r, g, b) = (srgb_to_lin(c[0]), srgb_to_lin(c[1]), srgb_to_lin(c[2]));
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
    let f = |t: f32| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

pub fn from_lab(lab: [f32; 3], a: u8) -> Rgba {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = fy + lab[1] / 500.0;
    let fz = fy - lab[2] / 200.0;
    let inv = |t: f32| if t.powi(3) > 0.008_856 { t.powi(3) } else { (t - 16.0 / 116.0) / 7.787 };
    let (x, y, z) = (inv(fx) * 0.950_47, inv(fy), inv(fz) * 1.088_83);
    let r = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let g = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let b = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
    [lin_to_srgb(r), lin_to_srgb(g), lin_to_srgb(b), a]
}

pub fn chroma(lab: [f32; 3]) -> f32 {
    (lab[1] * lab[1] + lab[2] * lab[2]).sqrt()
}
