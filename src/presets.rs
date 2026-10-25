//! Outfit-Vorlagen für den Skin-Mixer (ohne Kopf). Alles wird hier Pixel für Pixel
//! erzeugt – wie bei handgepixelten Skins: jedes Material hat eine kleine Farbpalette
//! (Schatten kühler, Lichter wärmer), Licht kommt von oben, Arme und Beine wirken rund.
//! Sichtbare Haut bekommt die Platzhalter-Hautfarben aus [`skin_ramp`], die der Mixer
//! automatisch in die Hautfarbe des eigenen Kopfes umrechnet.

use crate::color::{self, Hsv};
use crate::skin::{self, Face, Layer, Model, Part, Rgba, Skin};

/// Platzhalter für nackte Haut (mittlerer Ton)
pub const SKIN_PH: Rgba = [205, 150, 118, 255];

/// Alle Hauttöne der Vorlagen (dunkel → hell). Nur genau diese Farben gelten als Haut.
pub fn skin_ramp() -> [Rgba; 5] {
    let lab = color::to_lab(SKIN_PH);
    let mut out = [SKIN_PH; 5];
    for (k, dl) in [-15.0f32, -7.5, 0.0, 6.0, 11.0].iter().enumerate() {
        out[k] = if *dl == 0.0 { SKIN_PH } else { color::from_lab([lab[0] + dl, lab[1], lab[2]], 255) };
    }
    out
}

/// Ein Pixel innerhalb einer Fläche: Fläche, Spalte, Zeile, Breite, Höhe.
#[derive(Clone, Copy)]
pub struct Px {
    pub face: Face,
    pub i: usize,
    pub j: usize,
    pub w: usize,
    pub h: usize,
    pub part: Part,
}

impl Px {
    fn side(&self) -> bool {
        matches!(self.face, Face::Left | Face::Right)
    }
    fn vertical(&self) -> bool {
        !matches!(self.face, Face::Top | Face::Bottom)
    }
    fn front(&self) -> bool {
        self.face == Face::Front
    }
    fn back(&self) -> bool {
        self.face == Face::Back
    }
    fn top(&self) -> bool {
        self.face == Face::Top
    }
    fn bottom(&self) -> bool {
        self.face == Face::Bottom
    }
    fn limb(&self) -> bool {
        self.part != Part::Body && self.part != Part::Head
    }
    /// äußere Seite eines Arms/Beins (weg vom Körper)
    fn outer(&self) -> bool {
        match self.part {
            Part::RightArm | Part::RightLeg => self.face == Face::Right,
            Part::LeftArm | Part::LeftLeg => self.face == Face::Left,
            _ => false,
        }
    }
    /// Zeile von unten gezählt (0 = unterste)
    fn fb(&self) -> usize {
        self.h - 1 - self.j
    }
    fn rows(&self, a: usize, b: usize) -> bool {
        self.vertical() && (a..=b).contains(&self.j)
    }
    fn cols(&self, a: usize, b: usize) -> bool {
        (a..=b).contains(&self.i)
    }
    /// herabhängendes Ende der Schärpe an der rechten Seite
    fn right_side_tail(&self) -> bool {
        self.face == Face::Right && self.rows(10, 11) && self.cols(1, 2)
    }
    /// Pseudo-Zufall 0..1, fest pro Pixel
    fn rnd(&self, salt: u32) -> f32 {
        let mut h = (self.i as u32).wrapping_mul(374_761_393)
            ^ (self.j as u32).wrapping_mul(668_265_263)
            ^ (self.face as u32 + 1).wrapping_mul(2_246_822_519)
            ^ (self.part.idx() as u32 + 7).wrapping_mul(3_266_489_917)
            ^ salt.wrapping_mul(1_597_334_677);
        h = (h ^ (h >> 15)).wrapping_mul(2_246_822_519);
        h ^= h >> 13;
        (h & 0xffff) as f32 / 65535.0
    }
}

/// Ein Material: 5 Farbstufen (0 = Schatten … 4 = Licht) und wie stark es strukturiert ist.
#[derive(Clone, Copy)]
pub struct Mat {
    ramp: [Rgba; 5],
    tex: f32,
}

fn toward(h: f32, target: f32, amount: f32) -> f32 {
    let diff = (target - h + 540.0).rem_euclid(360.0) - 180.0;
    h + diff.signum() * amount.min(diff.abs())
}

fn mat_t(rgb: [u8; 3], tex: f32) -> Mat {
    let base = color::to_hsv([rgb[0], rgb[1], rgb[2], 255]);
    let mut ramp = [[0u8; 4]; 5];
    // Grau bekommt kaum Farbverschiebung, kräftige Farben mehr
    let shift = 9.0 * (base.s / 0.5).min(1.0);
    // Weiß braucht sanftere Schatten, sonst wirkt es grau und fleckig
    let dark = if base.v > 0.85 && base.s < 0.2 { 0.11 } else { 0.19 };
    for (k, slot) in ramp.iter_mut().enumerate() {
        let s = k as f32 - 2.0;
        let (mut h, mut sat, mut v) = (base.h, base.s, base.v);
        if s < 0.0 {
            h = toward(h, 240.0, shift * -s);
            sat = (sat * (1.0 + 0.12 * -s) + 0.02 * -s).min(1.0);
            v *= 1.0 - dark * -s;
        } else if s > 0.0 {
            h = toward(h, 50.0, shift * s);
            sat *= 1.0 - 0.13 * s;
            v = (v + s * (0.07f32).max((1.0 - v) * 0.3)).min(1.0);
        }
        *slot = color::from_hsv(Hsv { h, s: sat, v }, 255);
    }
    Mat { ramp, tex }
}

fn mat(rgb: [u8; 3]) -> Mat {
    mat_t(rgb, 0.5)
}

/// Was ein Pixel bekommt
#[derive(Clone, Copy)]
pub enum T {
    /// Material mit Helligkeits-Versatz (in Stufen)
    M(Mat, f32),
    /// Haut mit Helligkeits-Versatz
    Skin(f32),
}

fn m(x: Mat) -> Option<T> {
    Some(T::M(x, 0.0))
}
fn mo(x: Mat, o: f32) -> Option<T> {
    Some(T::M(x, o))
}
const SKIN: Option<T> = Some(T::Skin(0.0));
fn skin_o(o: f32) -> Option<T> {
    Some(T::Skin(o))
}

type Paint<'a> = &'a dyn Fn(Px) -> Option<T>;

pub struct Canvas {
    s: Skin,
    skin: [Rgba; 5],
}

/// Licht und Form: oben hell, unten dunkel, Seiten und Kanten etwas dunkler.
fn light(p: Px, layer: Layer) -> f32 {
    let limb = p.limb();
    let face = match p.face {
        Face::Top => 0.8,
        Face::Bottom => -1.3,
        Face::Front => 0.0,
        Face::Back => -0.35,
        Face::Left | Face::Right => {
            if !limb {
                -0.45
            } else if p.outer() {
                -0.4
            } else {
                -0.85
            }
        }
    };
    let grad = if p.vertical() && p.h > 1 {
        let t = p.j as f32 / (p.h - 1) as f32;
        if limb { 0.35 - 0.75 * t } else { 0.3 - 0.55 * t }
    } else {
        0.0
    };
    let edge = if p.vertical() && !p.side() && (p.i == 0 || p.i + 1 == p.w) {
        if limb { -0.5 } else { -0.3 }
    } else {
        0.0
    };
    let lift = if layer == Layer::Overlay { 0.25 } else { 0.0 };
    face + grad + edge + lift
}

impl Canvas {
    fn new() -> Self {
        Canvas { s: Skin::blank(), skin: skin_ramp() }
    }

    /// Bemalt eine Ebene der angegebenen Körperteile.
    fn paint(&mut self, parts: &[Part], layer: Layer, f: Paint) {
        for &part in parts {
            let b = skin::box_of(Model::Classic, part, layer);
            for face in Face::ALL {
                let (fx, fy, w, h) = b.face_rect(face);
                for j in 0..h {
                    for i in 0..w {
                        let p = Px { face, i, j, w, h, part };
                        let Some(t) = f(p) else { continue };
                        let base = light(p, layer);
                        let out = match t {
                            T::M(mat, o) => {
                                // Fell u. Ä. darf unruhig sein, Stoff bekommt nur vereinzelte Struktur-Pixel
                                let r = p.rnd(99);
                                let n = if mat.tex >= 0.8 {
                                    (r - 0.5) * 0.9 * mat.tex
                                } else if r < 0.07 * mat.tex {
                                    -0.6
                                } else {
                                    0.0
                                };
                                let lvl = (2.0 + base + o + n).round().clamp(0.0, 4.0) as usize;
                                mat.ramp[lvl]
                            }
                            T::Skin(o) => {
                                let lvl = (2.0 + base * 0.8 + o).round().clamp(0.0, 4.0) as usize;
                                self.skin[lvl]
                            }
                        };
                        self.s.set(fx + i, fy + j, out);
                    }
                }
            }
        }
    }
}

const BODY: &[Part] = &[Part::Body];
const ARMS: &[Part] = &[Part::RightArm, Part::LeftArm];
const LEGS: &[Part] = &[Part::RightLeg, Part::LeftLeg];

/// Hände: unterste `n` Reihen und Unterseite der Arme
fn hand(p: Px, n: usize) -> bool {
    p.bottom() || (p.vertical() && p.fb() < n)
}

/// Rippenbündchen (abwechselnd hell/dunkel)
fn rib(p: Px) -> f32 {
    if p.i % 2 == 0 { -0.3 } else { -0.9 }
}

/// Schuh mit Sohle: unterste Reihe und Unterseite dunkler
fn shoe(p: Px, upper: Mat, sole: Mat) -> Option<T> {
    if p.bottom() || p.fb() == 0 {
        return mo(sole, if p.bottom() { 0.6 } else { 0.0 });
    }
    m(upper)
}

/// Schnürsenkel vorne
fn laces(p: Px) -> bool {
    p.front() && p.cols(1, 2) && (p.i + p.j) % 2 == 0
}

// ------------------------------------------------------------------ Vorlagen

fn winter(cv: &mut Canvas) {
    let jacket = mat([38, 98, 182]);
    let fur = mat_t([232, 222, 204], 1.0);
    let scarf = mat_t([196, 44, 56], 0.6);
    let zip = mat_t([205, 208, 216], 0.1);
    let glove = mat([52, 52, 62]);
    let pants = mat([54, 58, 74]);
    let boot = mat_t([128, 80, 46], 0.6);
    let sole = mat([50, 36, 28]);
    // gesteppte Daunenjacke: Naht dunkel, darunter aufgeplustert hell
    let quilt = |p: Px| -> f32 {
        if !p.vertical() {
            return 0.0;
        }
        match p.j % 4 {
            3 => -1.1,
            0 => 0.6,
            _ => 0.1,
        }
    };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() && p.i == 4 && p.j >= 1 {
            return mo(zip, if p.j % 2 == 0 { 0.7 } else { -0.5 });
        }
        if p.front() && p.i == 3 && p.j >= 1 {
            return mo(jacket, -1.0);
        }
        if p.front() && p.j == 8 && (p.i == 1 || p.i == 6) {
            return mo(jacket, -1.6); // Taschen
        }
        if p.rows(11, 11) {
            return mo(jacket, -0.6 + rib(p) * 0.6);
        }
        mo(jacket, quilt(p))
    });
    cv.paint(BODY, Layer::Overlay, &|p| {
        // Fellkragen hinten und seitlich, Schal vorne
        if (p.back() || p.side()) && p.j <= 1 {
            return if p.j == 0 || p.rnd(3) > 0.4 { m(fur) } else { None };
        }
        if p.front() && p.j <= 1 {
            return mo(scarf, if (p.i + p.j) % 2 == 0 { 0.3 } else { -0.4 });
        }
        if p.front() && p.cols(5, 6) && p.rows(2, 6) {
            if p.j == 6 {
                return if p.i % 2 == 0 { m(scarf) } else { None }; // Fransen
            }
            return mo(scarf, if (p.i + p.j) % 2 == 0 { 0.2 } else { -0.5 } + if p.i == 6 { -0.4 } else { 0.0 });
        }
        if p.top() && (p.j == 0 || p.j == 3) {
            return m(fur);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return mo(glove, if p.vertical() && p.fb() == 0 { -0.3 } else { 0.0 });
        }
        mo(jacket, quilt(p))
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(8, 8) { mo(jacket, rib(p)) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 4) {
            if laces(p) && p.fb() >= 1 {
                return m(fur);
            }
            return shoe(p, boot, sole);
        }
        if p.front() && ((p.i == 1 && p.j == 5) || (p.i == 2 && p.j == 6)) {
            return mo(pants, -1.0); // Knie-Falte
        }
        m(pants)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| {
        if p.rows(7, 7) {
            return m(fur);
        }
        if p.rows(6, 6) && p.rnd(5) > 0.55 {
            return m(fur);
        }
        None
    });
}

fn suit(cv: &mut Canvas) {
    let cloth = mat_t([46, 48, 62], 0.35);
    let shirt = mat_t([226, 230, 238], 0.15);
    let tie = mat_t([176, 30, 46], 0.25);
    let shoe_m = mat_t([34, 28, 28], 0.1);
    let button = mat_t([22, 22, 26], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            let (i, j) = (p.i as i32, p.j as i32);
            // V-Ausschnitt mit Hemd und Krawatte
            let (l, r) = match j {
                0 => (1, 6),
                1..=4 => (2, 5),
                5 => (3, 4),
                _ => (9, -9),
            };
            if i >= l && i <= r {
                if (3..=4).contains(&i) && (1..=5).contains(&j) {
                    let knot = if j == 1 { 0.6 } else if j >= 4 { -0.2 } else { 0.2 };
                    let stripe = if (i + j) % 3 == 0 { -0.8 } else { 0.0 };
                    return mo(tie, knot + stripe);
                }
                return mo(shirt, if j == 1 && (i == 2 || i == 5) { -0.6 } else { 0.0 });
            }
            if i == l - 1 || i == r + 1 {
                return mo(cloth, 1.3); // Revers-Kante
            }
            if j <= 6 && (i == l - 2 || i == r + 2) {
                return mo(cloth, -0.7);
            }
            if p.i == 3 && (p.j == 7 || p.j == 9) {
                return m(button);
            }
            if p.i == 4 && p.j >= 6 {
                return mo(cloth, -0.7); // Knopfleiste
            }
            if p.i == 6 && p.j == 3 {
                return mo(tie, 0.8); // Einstecktuch
            }
            if p.j == 9 && (p.cols(0, 1) || p.cols(6, 7)) {
                return mo(cloth, -1.0); // Taschen
            }
        }
        if p.back() && p.i == 4 && p.j >= 9 {
            return mo(cloth, -1.0); // Schlitz
        }
        if p.back() && p.j == 0 {
            return mo(cloth, 0.6);
        }
        m(cloth)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return mo(shirt, -0.2); // Manschette
        }
        if p.rows(7, 7) && p.outer() && p.cols(1, 2) {
            return m(button);
        }
        if p.front() && ((p.i == 1 && p.j == 4) || (p.i == 2 && p.j == 5)) {
            return mo(cloth, -0.9); // Ellbogen-Falte
        }
        m(cloth)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            if p.front() && p.fb() == 1 && p.cols(1, 2) {
                return mo(shoe_m, 2.0); // Glanz
            }
            return shoe(p, shoe_m, button);
        }
        let crease = match p.part {
            Part::RightLeg => 1,
            _ => 2,
        };
        if p.front() && p.i == crease {
            return mo(cloth, 0.8); // Bügelfalte
        }
        m(cloth)
    });
    cv.paint(BODY, Layer::Overlay, &|p| if p.side() && p.j >= 10 { mo(cloth, -0.4) } else { None });
}

fn pajamas(cv: &mut Canvas) {
    let blue = mat_t([118, 160, 222], 0.3);
    let white = mat_t([232, 238, 248], 0.2);
    let navy = mat_t([42, 58, 116], 0.2);
    let slipper = mat_t([168, 150, 196], 1.0);
    let stripes = |p: Px| -> Option<T> { if p.vertical() && p.i % 3 == 0 { m(white) } else { m(blue) } };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j == 0 && p.cols(3, 4) {
                return SKIN;
            }
            if p.j <= 1 && p.cols(2, 5) {
                return mo(navy, 0.4); // Kragen
            }
            if p.i == 4 && p.j >= 1 {
                return if p.j % 3 == 2 { m(white) } else { mo(navy, -0.2) }; // Knopfleiste
            }
            if p.cols(5, 6) && p.rows(3, 4) {
                return if p.j == 3 { m(navy) } else { mo(blue, -0.4) }; // Brusttasche
            }
        }
        if p.vertical() && p.j == 0 {
            return m(navy);
        }
        stripes(p)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return m(navy);
        }
        stripes(p)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            return mo(slipper, if p.bottom() { -1.0 } else { 0.0 });
        }
        if p.rows(9, 9) {
            return m(navy);
        }
        stripes(p)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(9, 9) && p.rnd(7) > 0.35 { m(slipper) } else { None });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(8, 8) { mo(blue, 0.3) } else { None });
}

fn hoodie(cv: &mut Canvas) {
    let hood = mat([112, 72, 178]);
    let cord = mat_t([236, 236, 242], 0.1);
    let jog = mat([40, 42, 52]);
    let sneaker = mat_t([232, 234, 238], 0.15);
    let sole = mat_t([150, 150, 160], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if (p.i == 2 || p.i == 5) && p.j <= 4 {
                return if p.j == 4 { mo(sole, -0.5) } else { mo(cord, if p.j == 0 { -0.5 } else { 0.0 }) };
            }
            if p.j == 0 && p.cols(1, 6) {
                return mo(hood, 1.0); // Kapuzenrand
            }
            if p.rows(7, 10) && p.cols(1, 6) {
                if p.j == 7 {
                    return mo(hood, -0.9);
                }
                if (p.i == 1 || p.i == 6) && p.j <= 9 {
                    return mo(hood, -1.4); // Eingriff
                }
                return mo(hood, 0.3);
            }
        }
        if p.back() && p.j <= 4 && p.cols(1, 6) {
            if p.i == 1 || p.i == 6 || p.j == 4 {
                return mo(hood, -0.8);
            }
            return mo(hood, 0.4);
        }
        if p.back() && p.j == 5 && p.cols(3, 4) {
            return mo(hood, -0.6);
        }
        if p.rows(11, 11) {
            return mo(hood, rib(p));
        }
        m(hood)
    });
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.back() && p.j <= 3 && p.cols(1, 6) {
            return mo(hood, if p.i == 1 || p.i == 6 || p.j == 3 { -0.5 } else { 0.3 });
        }
        if p.side() && p.j == 0 {
            return mo(hood, 0.3);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return mo(hood, rib(p));
        }
        if p.front() && ((p.i == 1 && p.j == 5) || (p.i == 2 && p.j == 6)) {
            return mo(hood, -0.9);
        }
        m(hood)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            if p.outer() && p.fb() == 1 && p.cols(1, 2) {
                return m(hood); // Streifen am Schuh
            }
            if laces(p) && p.fb() == 2 {
                return m(sole);
            }
            return shoe(p, sneaker, sole);
        }
        if p.rows(8, 8) {
            return mo(jog, rib(p));
        }
        if p.outer() && p.i == 2 {
            return mo(jog, -0.6); // Seitennaht
        }
        if p.front() && ((p.i == 1 && p.j == 5) || (p.i == 2 && p.j == 4)) {
            return mo(jog, 0.8);
        }
        m(jog)
    });
}

fn tracksuit(cv: &mut Canvas) {
    let red = mat([198, 40, 50]);
    let white = mat_t([238, 238, 242], 0.15);
    let navy = mat([36, 44, 86]);
    let sneaker = mat_t([234, 234, 238], 0.15);
    let sole = mat_t([60, 60, 68], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() && p.i == 4 && p.j >= 1 {
            return mo(white, if p.j % 2 == 0 { 0.0 } else { -0.8 });
        }
        if p.front() && p.i == 3 && p.j >= 1 {
            return mo(red, -0.8);
        }
        if p.vertical() && p.j == 0 {
            return m(white); // Kragen-Paspel
        }
        if p.vertical() && p.j == 1 {
            return mo(red, 0.6); // Stehkragen
        }
        if p.side() && p.cols(1, 2) && p.j < 11 {
            return m(white);
        }
        if p.front() && p.i == 6 && p.j == 3 {
            return m(white); // kleines Abzeichen
        }
        if p.rows(11, 11) {
            return mo(red, rib(p) * 0.8);
        }
        m(red)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return mo(red, rib(p));
        }
        if (p.outer() || p.top()) && p.cols(1, 2) {
            return m(white);
        }
        m(red)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            if p.outer() && p.fb() == 1 && p.cols(1, 2) {
                return m(red);
            }
            if laces(p) && p.fb() == 2 {
                return mo(sole, 1.0);
            }
            return shoe(p, sneaker, sole);
        }
        if p.outer() && p.cols(1, 2) {
            return m(white);
        }
        if p.rows(8, 8) {
            return mo(navy, rib(p));
        }
        if p.front() && ((p.i == 1 && p.j == 5) || (p.i == 2 && p.j == 6)) {
            return mo(navy, 0.9);
        }
        m(navy)
    });
}

fn summer(cv: &mut Canvas) {
    let tee = mat([36, 168, 178]);
    let sun = [mat_t([252, 220, 80], 0.1), mat_t([250, 160, 60], 0.1), mat_t([236, 90, 90], 0.1)];
    let denim = mat([72, 112, 172]);
    let flip = mat_t([238, 96, 64], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j == 0 && p.cols(2, 5) {
                return if p.cols(3, 4) { SKIN } else { mo(tee, -0.6) };
            }
            // Retro-Sonnenuntergang: runde Sonne mit Streifen
            let (dx, dy) = (p.i as f32 - 3.5, p.j as f32 - 4.5);
            if dx * dx + dy * dy <= 7.0 && p.rows(2, 7) {
                return match p.j {
                    2 | 3 => m(sun[0]),
                    4 => m(sun[1]),
                    5 => mo(tee, -1.0),
                    _ => m(sun[2]),
                };
            }
            if p.j == 8 && p.cols(1, 6) {
                return mo(tee, 0.9);
            }
        }
        if p.rows(11, 11) {
            return mo(tee, -0.5);
        }
        m(tee)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if p.top() || p.rows(0, 2) {
            return m(tee);
        }
        if p.rows(3, 3) {
            return skin_o(-1.0); // Schatten unter dem Ärmel
        }
        SKIN
    });
    cv.paint(ARMS, Layer::Overlay, &|p| {
        if p.rows(0, 2) || p.top() {
            return mo(tee, if p.j == 2 && p.vertical() { -0.6 } else { 0.0 });
        }
        None
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.top() || p.rows(0, 5) {
            return mo(denim, if p.j == 5 && p.vertical() { 1.0 } else { 0.0 });
        }
        if p.rows(6, 6) {
            return skin_o(-1.0);
        }
        if p.bottom() || p.fb() == 0 {
            return m(flip);
        }
        if p.fb() == 1 && p.front() && p.cols(1, 2) {
            return mo(flip, 0.5);
        }
        SKIN
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(5, 5) { mo(denim, 0.8) } else { None });
}

fn knight(cv: &mut Canvas) {
    let chain = mat_t([140, 146, 158], 0.3);
    let plate = mat_t([186, 194, 208], 0.15);
    let tabard = mat([34, 66, 150]);
    let gold = mat_t([226, 182, 64], 0.2);
    let leather = mat([112, 72, 42]);
    let mail = |p: Px| -> Option<T> { mo(chain, if (p.i + p.j) % 2 == 0 { 0.5 } else { -0.7 }) };
    let shine = |p: Px| -> f32 { if p.vertical() && !p.side() && p.i == 1 { 1.2 } else { 0.0 } };
    cv.paint(BODY, Layer::Base, &|p| {
        if (p.front() || p.back()) && p.cols(1, 6) {
            if p.rows(7, 7) {
                return if p.cols(3, 4) && p.front() { mo(gold, 0.5) } else { m(leather) };
            }
            if p.i == 1 || p.i == 6 {
                return m(gold); // Borte
            }
            let cross = (p.cols(3, 4) && p.rows(1, 10)) || (p.rows(3, 4) && p.cols(2, 5));
            if cross && p.front() {
                return mo(gold, 0.2);
            }
            if p.j == 11 {
                return mo(tabard, -0.8);
            }
            return m(tabard);
        }
        if p.rows(7, 7) {
            return m(leather);
        }
        mail(p)
    });
    cv.paint(BODY, Layer::Overlay, &|p| {
        if (p.front() || p.back()) && p.cols(1, 6) && p.rows(10, 11) {
            return mo(tabard, if p.j == 11 { -0.8 } else { 0.0 });
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 4) {
            let o = shine(p) + if p.vertical() && p.fb() == 3 { 0.6 } else { 0.0 } + if p.vertical() && p.fb() == 0 { -0.6 } else { 0.0 };
            return mo(plate, o);
        }
        mail(p)
    });
    cv.paint(ARMS, Layer::Overlay, &|p| {
        // Schulterpanzer aus überlappenden Platten mit Nieten
        if p.top() || p.rows(0, 3) {
            if p.vertical() && p.j % 2 == 1 {
                return mo(plate, -0.9);
            }
            if p.vertical() && p.j == 2 && p.i == 2 {
                return m(gold);
            }
            return mo(plate, shine(p));
        }
        if p.rows(8, 8) {
            return mo(plate, 0.5);
        }
        None
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            return mo(plate, if p.bottom() || p.fb() == 0 { -1.0 } else { 0.0 } + shine(p));
        }
        if p.top() || p.rows(0, 4) {
            return mail(p);
        }
        mo(plate, shine(p) + if p.j == 5 { -0.8 } else { 0.0 })
    });
    cv.paint(LEGS, Layer::Overlay, &|p| {
        if p.front() && p.rows(5, 7) && p.cols(1, 2) {
            return mo(plate, if p.j == 6 { 1.2 } else { 0.0 }); // Kniekachel
        }
        None
    });
}

fn doctor(cv: &mut Canvas) {
    let coat = mat_t([228, 232, 240], 0.2);
    let scrubs = mat([56, 140, 158]);
    let steth = mat_t([48, 50, 60], 0.15);
    let silver = mat_t([196, 202, 214], 0.1);
    let pen = mat_t([40, 80, 200], 0.1);
    let shoe_m = mat_t([214, 218, 226], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            let (l, r) = match p.j {
                0 => (1, 6),
                1..=2 => (2, 5),
                3 => (3, 4),
                _ => (9, 0),
            };
            if p.i >= l && p.i <= r {
                if p.j <= 1 && p.cols(3, 4) {
                    return SKIN;
                }
                return mo(scrubs, if p.j == 0 { 0.0 } else { -0.3 });
            }
            if p.i == 1 && p.rows(1, 4) {
                return m(steth);
            }
            if p.cols(1, 2) && p.j == 5 {
                return m(silver);
            }
            if p.i == 6 && p.rows(1, 2) {
                return m(steth);
            }
            if p.i + 1 == l || p.i == r + 1 {
                return mo(coat, -0.9); // Revers-Schatten
            }
            if p.i == 6 && p.j == 3 {
                return m(pen);
            }
            if p.cols(5, 6) && p.j == 4 {
                return mo(coat, -1.0); // Brusttasche
            }
            if p.i == 4 && p.j >= 4 {
                return if p.j == 6 || p.j == 9 { m(silver) } else { mo(coat, -0.6) };
            }
            if p.j == 9 && (p.cols(0, 1) || p.cols(6, 7)) {
                return mo(coat, -1.0);
            }
        }
        if p.back() && p.j == 6 && p.cols(2, 5) {
            return mo(coat, -0.7); // Rückengurt
        }
        m(coat)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return mo(coat, -0.6);
        }
        if p.front() && ((p.i == 1 && p.j == 5) || (p.i == 2 && p.j == 6)) {
            return mo(coat, -1.0);
        }
        m(coat)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            return shoe(p, shoe_m, steth);
        }
        m(scrubs)
    });
    // langer Kittel bis über die Oberschenkel
    cv.paint(LEGS, Layer::Overlay, &|p| {
        if p.rows(0, 3) {
            let inner = matches!((p.part, p.face), (Part::RightLeg, Face::Left) | (Part::LeftLeg, Face::Right));
            if inner {
                return None;
            }
            return mo(coat, if p.j == 3 { -0.7 } else { 0.0 });
        }
        None
    });
}

fn police(cv: &mut Canvas) {
    let shirt = mat([42, 58, 104]);
    let pants = mat([28, 34, 58]);
    let belt = mat_t([26, 26, 30], 0.15);
    let silver = mat_t([200, 206, 218], 0.1);
    let gold = mat_t([230, 186, 64], 0.15);
    let boot = mat_t([24, 24, 28], 0.1);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j == 0 && p.cols(3, 4) {
                return SKIN;
            }
            if p.j == 0 && (p.i == 2 || p.i == 5) {
                return mo(shirt, 1.0); // Kragen
            }
            if p.cols(3, 4) && p.rows(1, 6) {
                return mo(belt, if p.j == 1 { 0.8 } else { 0.3 }); // Krawatte
            }
            if p.cols(5, 6) && p.rows(1, 2) {
                return mo(gold, if p.j == 1 && p.i == 5 { 1.0 } else { 0.0 }); // Abzeichen
            }
            if (p.cols(1, 2) || p.cols(5, 6)) && p.rows(3, 4) {
                return mo(shirt, if p.j == 3 { -1.0 } else { 0.2 }); // Taschen
            }
            if p.cols(1, 2) && p.j == 2 {
                return m(silver); // Namensschild
            }
            if p.i == 4 && p.j == 7 {
                return m(silver);
            }
        }
        if p.rows(8, 8) {
            if p.front() && p.cols(3, 4) {
                return m(silver); // Schnalle
            }
            return m(belt);
        }
        if p.side() && p.rows(9, 9) && p.cols(1, 2) {
            return mo(belt, 0.6); // Gürteltasche
        }
        if p.rows(9, 11) {
            return m(pants);
        }
        m(shirt)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.top() {
            return mo(shirt, if p.j == 1 { 0.9 } else { 0.0 }); // Schulterklappe
        }
        if p.outer() && p.rows(1, 2) && p.cols(1, 2) {
            return if p.j == 1 { m(gold) } else { mo(gold, -1.5) }; // Ärmelabzeichen
        }
        if p.rows(8, 8) {
            return mo(shirt, -0.7);
        }
        m(shirt)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            if p.front() && p.fb() == 2 && p.cols(1, 2) {
                return mo(boot, 2.0);
            }
            return shoe(p, boot, belt);
        }
        if p.outer() && p.i == 2 {
            return mo(shirt, 0.6); // Seitenstreifen
        }
        m(pants)
    });
}

fn chef(cv: &mut Canvas) {
    let white = mat_t([232, 234, 240], 0.2);
    let knot = mat_t([200, 40, 50], 0.2);
    let button = mat_t([40, 40, 46], 0.1);
    let check_dark = mat_t([34, 34, 40], 0.1);
    let clog = mat_t([30, 30, 34], 0.1);
    // Hahnentritt-Muster (4×4)
    const TOOTH: [[u8; 4]; 4] = [[1, 1, 0, 1], [1, 1, 1, 0], [0, 1, 0, 0], [1, 0, 0, 0]];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j == 0 && p.cols(2, 5) {
                return mo(knot, if p.cols(3, 4) { 0.0 } else { -0.4 }); // Halstuch
            }
            if p.j == 1 && p.cols(3, 4) {
                return mo(knot, -0.6);
            }
            if (p.i == 2 || p.i == 5) && (p.j == 2 || p.j == 5 || p.j == 8) {
                return m(button);
            }
            if p.i == 6 && p.j >= 1 {
                return mo(white, -0.9); // Überschlag
            }
        }
        if p.vertical() && p.j == 0 {
            return mo(white, 0.8); // Stehkragen
        }
        m(white)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(7, 8) {
            return mo(white, if p.j == 7 { -0.9 } else { 0.7 }); // hochgekrempelt
        }
        m(white)
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(8, 8) { mo(white, 0.4) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            return shoe(p, clog, button);
        }
        if TOOTH[p.j % 4][p.i % 4] == 1 { m(check_dark) } else { mo(white, -0.4) }
    });
}

fn santa(cv: &mut Canvas) {
    let red = mat([192, 30, 40]);
    let fur = mat_t([240, 238, 234], 1.0);
    let belt = mat_t([26, 22, 22], 0.15);
    let gold = mat_t([236, 190, 60], 0.15);
    let glove = mat_t([228, 228, 234], 0.3);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j == 0 {
            return m(fur);
        }
        if p.rows(6, 7) {
            if p.front() && p.cols(2, 5) && (p.j == 6 || p.i == 2 || p.i == 5) {
                return mo(gold, if p.j == 6 { 0.6 } else { 0.0 });
            }
            return m(belt);
        }
        if p.front() && p.cols(3, 4) {
            return m(fur);
        }
        if p.rows(10, 11) {
            return m(fur);
        }
        m(red)
    });
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.rows(10, 11) && (p.j == 11 || p.rnd(11) > 0.3) {
            return m(fur);
        }
        if p.front() && p.cols(3, 4) && p.j <= 9 && p.j != 6 && p.j != 7 && p.rnd(12) > 0.35 {
            return m(fur);
        }
        if p.vertical() && p.j == 0 {
            return m(fur);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return m(glove);
        }
        if p.rows(7, 8) {
            return m(fur);
        }
        m(red)
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(7, 8) && (p.j == 8 || p.rnd(13) > 0.3) { m(fur) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 5) {
            if p.vertical() && p.fb() == 4 {
                return mo(belt, 1.2); // Stiefel-Umschlag
            }
            if p.front() && p.fb() == 2 && p.cols(1, 2) {
                return mo(belt, 2.0);
            }
            return shoe(p, belt, belt);
        }
        m(red)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(7, 7) { mo(belt, 1.0) } else { None });
}

fn ninja(cv: &mut Canvas) {
    let gi = mat([38, 40, 50]);
    let wrap = mat_t([70, 72, 86], 0.4);
    let sash = mat([176, 30, 42]);
    let handle = mat_t([30, 46, 110], 0.2);
    let guard = mat_t([214, 172, 60], 0.15);
    let sheath = mat([96, 30, 34]);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            // überkreuzter Kragen
            let edge = 1 + (p.j * 5) / 6;
            if p.j <= 6 && p.i == edge {
                return mo(gi, 1.2);
            }
            if p.j <= 6 && p.i == edge + 1 {
                return mo(gi, -1.0);
            }
            if p.j <= 2 && p.cols(2, 4) && p.i < edge {
                return mo(gi, -1.6); // Netz-Unterhemd
            }
        }
        if p.rows(7, 8) {
            if p.front() && p.i == 6 {
                return mo(sash, 0.8); // Knoten
            }
            return mo(sash, if p.j == 8 { -0.4 } else { 0.0 });
        }
        if p.front() && ((p.i == 6 && p.j == 9) || (p.i == 7 && p.rows(9, 10))) {
            return mo(sash, -0.3); // Bänder
        }
        m(gi)
    });
    // Katana auf dem Rücken (2. Ebene, steht ab)
    cv.paint(BODY, Layer::Overlay, &|p| {
        if !p.back() {
            return None;
        }
        let k = 1 + p.j / 2;
        if p.i != k && p.i != k + 1 {
            return None;
        }
        let first = p.i == k;
        match p.j {
            0..=3 => mo(handle, if (p.i + p.j) % 2 == 0 { 0.8 } else { -0.6 }),
            4 => m(guard),
            _ => mo(sheath, if first { 0.8 } else { -0.3 }),
        }
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return mo(gi, -0.4);
        }
        if p.rows(5, 8) {
            return mo(wrap, if (p.i + p.j) % 3 == 0 { -0.9 } else { 0.2 }); // Bandagen
        }
        m(gi)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 2) {
            if p.front() && p.i == 1 {
                return mo(gi, -1.6); // geteilte Zehe
            }
            return mo(gi, -0.5);
        }
        if p.rows(6, 9) {
            return mo(wrap, if (p.i + 2 * p.j) % 3 == 0 { -0.9 } else { 0.2 });
        }
        if p.front() && ((p.i == 1 && p.j == 3) || (p.i == 2 && p.j == 4)) {
            return mo(gi, 0.9);
        }
        m(gi)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(5, 5) { mo(gi, 0.3) } else { None });
}

fn astronaut(cv: &mut Canvas) {
    let suit = mat_t([226, 228, 234], 0.2);
    let metal = mat_t([150, 156, 170], 0.15);
    let orange = mat([234, 112, 40]);
    let screen = mat_t([24, 36, 66], 0.1);
    let lights = [mat_t([230, 50, 50], 0.0), mat_t([60, 210, 90], 0.0), mat_t([60, 140, 240], 0.0), mat_t([250, 210, 60], 0.0)];
    let glove = mat_t([182, 188, 204], 0.2);
    let boot = mat_t([196, 200, 208], 0.15);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j <= 1 {
            return mo(metal, if p.j == 0 { 0.7 } else { -0.5 }); // Helmring
        }
        if p.front() && p.rows(3, 7) && p.cols(1, 6) {
            if p.i == 1 || p.i == 6 || p.j == 3 || p.j == 7 {
                return mo(metal, -0.3);
            }
            if p.j == 4 {
                return m(lights[p.i - 2]);
            }
            if p.j == 5 && p.i == 3 {
                return mo(lights[1], 0.5);
            }
            return m(screen);
        }
        if p.rows(9, 9) {
            return if p.front() && p.cols(3, 4) { mo(metal, 1.0) } else { m(orange) };
        }
        if p.vertical() && !p.side() && (p.i == 0 || p.i == 7) {
            return mo(suit, -0.5); // Nähte
        }
        m(suit)
    });
    // Lebenserhaltungs-Rucksack
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.back() && p.rows(1, 10) {
            if p.j == 1 || p.j == 10 || p.i == 0 || p.i == 7 {
                return mo(metal, 0.2);
            }
            if (p.j == 4 || p.j == 6 || p.j == 8) && p.cols(2, 5) {
                return mo(metal, -1.0);
            }
            return mo(suit, 0.2);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return m(glove);
        }
        if p.rows(8, 8) {
            return mo(metal, 0.4);
        }
        if p.rows(5, 5) {
            return mo(suit, -1.0); // Gelenk
        }
        if p.outer() && p.rows(1, 2) && p.cols(1, 2) {
            return mo(lights[2], if p.j == 1 { 0.0 } else { -1.0 }); // Missions-Abzeichen
        }
        if p.rows(3, 3) {
            return m(orange);
        }
        m(suit)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            return shoe(p, boot, metal);
        }
        if p.rows(8, 8) {
            return m(orange);
        }
        if p.front() && p.rows(4, 5) {
            return mo(metal, 0.4); // Knieschoner
        }
        m(suit)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(9, 9) { mo(suit, 0.5) } else { None });
}

fn pirate(cv: &mut Canvas) {
    let shirt = mat_t([234, 226, 208], 0.4);
    let vest = mat([98, 58, 36]);
    let sash = mat([172, 34, 46]);
    let pants = mat([48, 42, 52]);
    let boot = mat_t([42, 32, 28], 0.3);
    let sole = mat([24, 20, 18]);
    let gold = mat_t([228, 184, 64], 0.15);
    let strap = mat([128, 86, 48]);
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            let open = (p.j == 0 && p.cols(2, 5)) || (p.rows(1, 2) && p.cols(3, 4));
            if open {
                if (p.i == 3 && p.j == 2) || (p.i == 4 && p.j == 1) {
                    return mo(strap, -0.5); // Schnürung
                }
                return SKIN;
            }
        }
        if p.rows(8, 9) {
            return mo(sash, if (p.i + p.j) % 3 == 0 { -0.6 } else { 0.1 });
        }
        if p.right_side_tail() {
            return mo(sash, -0.3);
        }
        if p.rows(10, 11) {
            return m(pants);
        }
        mo(shirt, if p.vertical() && p.i % 3 == 1 { -0.6 } else { 0.0 })
    });
    // Weste und Patronengurt (2. Ebene)
    cv.paint(BODY, Layer::Overlay, &|p| {
        if !p.vertical() || p.j > 7 {
            return None;
        }
        if p.front() {
            let k = 6 - (p.j * 5) / 7; // Gurt schräg über die Brust
            if p.i == k {
                return if p.j == 4 { m(gold) } else { m(strap) };
            }
            if p.cols(0, 1) || p.cols(6, 7) {
                if p.i == 1 && (p.j == 3 || p.j == 5) {
                    return m(gold);
                }
                return mo(vest, if p.i == 1 || p.i == 6 { 0.7 } else { 0.0 });
            }
            return None;
        }
        if p.back() && p.i == 1 + (p.j * 5) / 7 {
            return m(strap);
        }
        mo(vest, if p.j == 7 { -0.6 } else { 0.0 })
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(8, 8) {
            return mo(shirt, 0.8);
        }
        mo(shirt, if p.i % 2 == 0 { -0.5 } else { 0.2 }) // weite Ärmel mit Falten
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(8, 8) && p.rnd(21) > 0.25 { mo(shirt, 1.0) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 8) {
            if p.vertical() && p.fb() == 7 {
                return mo(strap, 0.6); // Stulpe
            }
            if p.front() && p.fb() == 4 && p.cols(1, 2) {
                return m(gold);
            }
            if p.front() && p.fb() == 2 && p.i == 1 {
                return mo(boot, 2.0);
            }
            return shoe(p, boot, sole);
        }
        m(pants)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(3, 4) { mo(strap, if p.j == 3 { 0.6 } else { -0.4 }) } else { None });
}

fn overalls(cv: &mut Canvas) {
    let denim = mat([62, 98, 164]);
    let plaid = mat([180, 40, 46]);
    let stitch = mat_t([218, 164, 82], 0.1);
    let gold = mat_t([226, 182, 66], 0.1);
    let boot = mat([134, 86, 48]);
    let sole = mat([60, 40, 30]);
    let flannel = |p: Px| -> Option<T> {
        let a = p.i % 4 == 0;
        let b = p.j % 4 == 0;
        let o = match (a, b) {
            (true, true) => -1.6,
            (true, false) | (false, true) => -0.9,
            _ if p.i % 4 == 2 => 0.4,
            _ => 0.0,
        };
        mo(plaid, o)
    };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if (p.i == 1 || p.i == 6) && p.j <= 2 {
                return m(denim); // Träger
            }
            if (p.i == 1 || p.i == 6) && p.j == 3 {
                return m(gold);
            }
            if p.cols(1, 6) && p.j >= 3 {
                if p.j == 3 {
                    return if p.i % 2 == 0 { m(stitch) } else { mo(denim, 0.4) };
                }
                if p.cols(2, 5) && p.rows(5, 7) {
                    return if p.j == 5 { mo(stitch, -0.2) } else { mo(denim, 0.3) }; // Latztasche
                }
                return m(denim);
            }
            if p.j >= 6 {
                return m(denim);
            }
        }
        if p.back() && p.j <= 6 {
            let a = 1 + (p.j * 5) / 6;
            let b = 6 - (p.j * 5) / 6;
            if p.i == a || p.i == b {
                return m(denim); // gekreuzte Träger
            }
        }
        if p.vertical() && p.j >= 6 {
            if p.side() && p.j == 6 && p.i == 1 {
                return m(gold);
            }
            return m(denim);
        }
        flannel(p)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.rows(7, 8) {
            return mo(plaid, if p.j == 7 { -0.9 } else { 0.6 });
        }
        flannel(p)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            if laces(p) && p.fb() >= 1 {
                return mo(stitch, -0.4);
            }
            return shoe(p, boot, sole);
        }
        if p.rows(8, 8) {
            return mo(denim, 1.3); // Umschlag
        }
        if p.outer() && p.i == 2 {
            return m(stitch);
        }
        if p.front() && p.rows(5, 6) && p.cols(1, 2) {
            return mo(denim, if p.j == 5 { 0.6 } else { 0.2 }); // Kniefleck
        }
        m(denim)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.rows(8, 8) { mo(denim, 1.0) } else { None });
}

fn skeleton(cv: &mut Canvas) {
    let black = mat_t([28, 28, 36], 0.4);
    let bone = mat_t([226, 222, 204], 0.3);
    cv.paint(BODY, Layer::Base, &|p| {
        let b = |o: f32| mo(bone, o);
        if p.front() {
            if p.j == 0 && (p.cols(1, 2) || p.cols(5, 6)) {
                return b(0.0); // Schlüsselbeine
            }
            if p.cols(3, 4) && p.rows(1, 7) {
                return b(0.3); // Brustbein
            }
            if ((p.j == 2 || p.j == 4) && p.cols(1, 6)) || (p.j == 6 && p.cols(2, 5)) {
                return b(if p.i == 1 || p.i == 6 { -0.6 } else { 0.0 }); // Rippen
            }
            if p.rows(9, 10) && p.cols(1, 6) && !(p.j == 10 && p.cols(3, 4)) {
                return b(if p.j == 10 { -0.5 } else { 0.0 }); // Becken
            }
        }
        if p.back() {
            if p.cols(3, 4) {
                return b(if p.j % 2 == 1 { -0.9 } else { 0.2 }); // Wirbelsäule
            }
            if p.rows(1, 3) && (p.i == 1 || p.i == 6) {
                return b(-0.3);
            }
        }
        if p.side() && (p.j == 2 || p.j == 4) && p.cols(0, 1) {
            return b(-0.4);
        }
        m(black)
    });
    let limb_bones = |p: Px| -> Option<T> {
        if p.vertical() {
            if p.rows(5, 6) && (!p.side() || p.cols(1, 2)) {
                return mo(bone, if p.j == 5 { 0.4 } else { -0.4 }); // Gelenk
            }
            if p.cols(1, 2) && !p.side() {
                return mo(bone, if p.i == 2 { -0.4 } else { 0.0 });
            }
        }
        if p.top() && p.cols(1, 2) && (1..=2).contains(&p.j) {
            return m(bone);
        }
        m(black)
    };
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) && p.vertical() {
            return if p.i % 2 == 0 || p.fb() == 2 { mo(bone, -0.2) } else { m(black) }; // Finger
        }
        limb_bones(p)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.vertical() && p.fb() == 0 {
            return mo(bone, -0.3);
        }
        limb_bones(p)
    });
}

fn leather(cv: &mut Canvas) {
    let jacket = mat_t([40, 38, 44], 0.3);
    let tee = mat_t([232, 234, 238], 0.15);
    let chrome = mat_t([200, 206, 216], 0.1);
    let jeans = mat([66, 102, 162]);
    let boot = mat_t([30, 26, 26], 0.15);
    let sole = mat([16, 14, 14]);
    let shine = |p: Px| -> f32 { if p.vertical() && !p.side() && p.i == 1 { 0.9 } else { 0.0 } };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.rows(0, 3) && p.cols(2, 4) {
                return mo(tee, if p.j == 0 { -0.4 } else { 0.0 }); // T-Shirt
            }
            if (p.i == 1 && p.rows(0, 3)) || (p.i == 5 && p.rows(0, 2)) {
                return mo(jacket, 1.3); // Revers
            }
            if p.i == 5 && p.j >= 3 {
                return mo(chrome, if p.j % 2 == 0 { 0.4 } else { -0.6 }); // schräger Reißverschluss
            }
            if (p.i == 1 && p.j == 6) || (p.i == 2 && p.j == 7) {
                return m(chrome); // Taschen-Reißverschluss
            }
            if p.rows(10, 11) {
                return if p.i == 6 && p.j == 10 { m(chrome) } else { mo(jacket, -0.5) };
            }
        }
        if p.back() && p.j == 4 && p.cols(1, 6) {
            return mo(jacket, -0.9); // Rückennaht
        }
        mo(jacket, shine(p))
    });
    cv.paint(BODY, Layer::Overlay, &|p| if p.front() && (p.i == 1 || p.i == 5) && p.rows(0, 2) { mo(jacket, 1.0) } else { None });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return SKIN;
        }
        if p.outer() && p.rows(7, 8) && p.i == 2 {
            return m(chrome);
        }
        if p.top() && p.cols(1, 2) && p.j == 1 {
            return m(chrome); // Nieten
        }
        if p.front() && ((p.i == 1 && p.j == 4) || (p.i == 2 && p.j == 5)) {
            return mo(jacket, 1.2);
        }
        mo(jacket, shine(p))
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 3) {
            if p.front() && p.fb() == 2 && p.i == 1 {
                return mo(boot, 2.0);
            }
            return shoe(p, boot, sole);
        }
        if p.front() && p.j == 5 && p.cols(1, 2) {
            return SKIN; // Riss am Knie
        }
        if p.front() && (p.j == 4 || p.j == 6) && p.cols(1, 2) {
            return mo(tee, -0.4);
        }
        if p.front() && p.rows(1, 3) && p.cols(1, 2) {
            return mo(jeans, 1.0); // ausgewaschen
        }
        m(jeans)
    });
}

fn firefighter(cv: &mut Canvas) {
    let coat = mat([46, 48, 56]);
    let yellow = mat_t([214, 224, 60], 0.1);
    let silver = mat_t([206, 210, 218], 0.1);
    let glove = mat([118, 74, 42]);
    let boot = mat_t([26, 26, 30], 0.15);
    // Reflexstreifen: gelb – silber – gelb
    let band = |p: Px, at: usize| -> Option<Option<T>> {
        if !p.vertical() {
            return None;
        }
        if p.j == at + 1 {
            return Some(m(silver));
        }
        if p.j == at || p.j == at + 2 {
            return Some(m(yellow));
        }
        None
    };
    cv.paint(BODY, Layer::Base, &|p| {
        if let Some(b) = band(p, 8) {
            return b;
        }
        if p.front() {
            if p.cols(3, 4) && p.j >= 1 {
                if p.i == 4 && (p.j == 3 || p.j == 6) {
                    return m(silver);
                }
                return mo(coat, if p.i == 3 { -0.9 } else { 0.3 });
            }
            if p.i == 6 && p.rows(2, 4) {
                return mo(boot, if p.j == 2 { 1.0 } else { 0.0 }); // Funkgerät
            }
        }
        if p.vertical() && p.j == 0 {
            return mo(coat, 0.8); // Kragen
        }
        if p.back() && p.rows(2, 4) && p.cols(1, 6) {
            return if p.j == 3 { m(yellow) } else { mo(coat, 0.2) }; // Rückenschild
        }
        m(coat)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return m(glove);
        }
        if let Some(b) = band(p, 5) {
            return b;
        }
        m(coat)
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.rows(8, 8) { mo(coat, -0.4) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if hand(p, 4) {
            if p.front() && p.fb() == 1 && p.cols(1, 2) {
                return m(yellow); // Stahlkappe
            }
            return shoe(p, boot, boot);
        }
        if let Some(b) = band(p, 4) {
            return b;
        }
        m(coat)
    });
}

pub const OUTFITS: &[(&str, &str, fn(&mut Canvas))] = &[
    ("winter", "Winterjacke", winter),
    ("suit", "Anzug", suit),
    ("pajamas", "Schlafanzug", pajamas),
    ("hoodie", "Hoodie", hoodie),
    ("tracksuit", "Trainingsanzug", tracksuit),
    ("summer", "Sommer", summer),
    ("leather", "Lederjacke", leather),
    ("knight", "Ritter", knight),
    ("doctor", "Arzt", doctor),
    ("police", "Polizei", police),
    ("firefighter", "Feuerwehr", firefighter),
    ("chef", "Koch", chef),
    ("santa", "Weihnachtsmann", santa),
    ("ninja", "Ninja", ninja),
    ("astronaut", "Astronaut", astronaut),
    ("pirate", "Pirat", pirate),
    ("overalls", "Latzhose", overalls),
    ("skeleton", "Skelett", skeleton),
];

/// Erzeugt das Outfit mit der Kennung `id`.
pub fn make(id: &str) -> Option<Skin> {
    let (_, _, f) = OUTFITS.iter().find(|(k, _, _)| *k == id)?;
    let mut cv = Canvas::new();
    f(&mut cv);
    Some(cv.s)
}

pub fn name_of(id: &str) -> Option<&'static str> {
    OUTFITS.iter().find(|(k, _, _)| *k == id).map(|(_, n, _)| crate::i18n::t(n))
}

#[cfg(test)]
mod tests {
    /// Rendert alle Vorlagen groß als PNG (nur wenn PRESET_OUT gesetzt ist).
    #[test]
    fn render_presets() {
        let Ok(dir) = std::env::var("PRESET_OUT") else { return };
        use crate::render3d::{render, Camera, RenderParams};
        use crate::skin::{copy_part, starter_skin, Layer, Model, Part};
        let head = starter_skin();
        for (id, _, _) in super::OUTFITS {
            let mut s = super::make(id).unwrap();
            copy_part(&head, Model::Classic, &mut s, Model::Classic, Part::Head, Layer::Base);
            let mut imgs = Vec::new();
            for yaw in [-0.5f32, std::f32::consts::PI - 0.5] {
                let p = RenderParams {
                    cam: Camera { yaw, pitch: 0.15, zoom: 1.0 },
                    model: Model::Classic,
                    vis: Default::default(),
                    highlight: vec![],
                    width: 200,
                    height: 260,
                    supersample: 2,
                    ghosts: false,
                    grid: false,
                };
                imgs.push(render(&s, &p));
            }
            let (w, h) = (imgs[0].width, imgs[0].height);
            let mut out = vec![0u8; w * 2 * h * 4];
            for (k, im) in imgs.iter().enumerate() {
                for y in 0..h {
                    out[(y * w * 2 + k * w) * 4..(y * w * 2 + k * w + w) * 4].copy_from_slice(&im.rgba[y * w * 4..(y + 1) * w * 4]);
                }
            }
            std::fs::write(format!("{dir}/{id}.png"), crate::skin::encode_png((w * 2) as u32, h as u32, &out)).unwrap();
        }
    }
}

