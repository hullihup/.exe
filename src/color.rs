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
    let mut r = rng.f32() * 2.0 - 1.0;
    if levels >= 2 {
        let l = (levels - 1) as f32;
        r = ((r + 1.0) * 0.5 * l).round() / l * 2.0 - 1.0;
    }
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
