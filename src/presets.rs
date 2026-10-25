//! Outfit-Vorlagen für den Skin-Mixer (ohne Kopf). Alles wird hier Pixel für Pixel
//! erzeugt – sichtbare Haut bekommt die Platzhalterfarbe [`SKIN_PH`], die der Mixer
//! automatisch in die Hautfarbe des eigenen Kopfes umrechnet.

use crate::skin::{self, Face, Layer, Model, Part, Rgba, Skin};

/// Platzhalter für nackte Haut (Hände, Arme, Beine)
pub const SKIN_PH: Rgba = [205, 150, 118, 255];

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
    /// äußere Seite eines Arms/Beins (weg vom Körper)
    fn outer(&self) -> bool {
        match self.part {
            Part::RightArm | Part::RightLeg => self.face == Face::Right,
            Part::LeftArm | Part::LeftLeg => self.face == Face::Left,
            _ => false,
        }
    }
    /// Zeile von unten gezählt (0 = unterste)
    fn from_bottom(&self) -> usize {
        self.h - 1 - self.j
    }
}

type Paint<'a> = &'a dyn Fn(Px) -> Option<Rgba>;

pub struct Canvas {
    s: Skin,
}

fn mul(c: [u8; 3], f: f32) -> Rgba {
    [
        (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        255,
    ]
}

fn c(rgb: [u8; 3]) -> Option<Rgba> {
    Some([rgb[0], rgb[1], rgb[2], 255])
}

fn d(rgb: [u8; 3], f: f32) -> Option<Rgba> {
    Some(mul(rgb, f))
}

const SKIN: Option<Rgba> = Some(SKIN_PH);

impl Canvas {
    fn new() -> Self {
        Canvas { s: Skin::blank() }
    }

    /// Bemalt eine Ebene der angegebenen Körperteile; Licht und leichtes Rauschen kommen automatisch dazu.
    fn paint(&mut self, parts: &[Part], layer: Layer, f: Paint) {
        for &part in parts {
            let b = skin::box_of(Model::Classic, part, layer);
            for face in Face::ALL {
                let (fx, fy, w, h) = b.face_rect(face);
                for j in 0..h {
                    for i in 0..w {
                        let Some(col) = f(Px { face, i, j, w, h, part }) else { continue };
                        let (x, y) = (fx + i, fy + j);
                        let out = if col == SKIN_PH {
                            col // Haut bleibt exakt, damit sie sicher erkannt wird
                        } else {
                            let mut hsh = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
                            hsh = (hsh ^ (hsh >> 13)).wrapping_mul(1_274_126_177);
                            let n = ((hsh >> 16) & 0xff) as f32 / 255.0 * 0.09 - 0.045;
                            let shade = match face {
                                Face::Top => 1.04,
                                Face::Bottom => 0.82,
                                Face::Left | Face::Right => 0.94,
                                Face::Back => 0.97,
                                Face::Front => 1.0,
                            };
                            mul([col[0], col[1], col[2]], shade * (1.0 + n))
                        };
                        self.s.set(x, y, out);
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
    p.face == Face::Bottom || (p.vertical() && p.from_bottom() < n)
}

// ------------------------------------------------------------------ Vorlagen

fn winter(cv: &mut Canvas) {
    let jacket = [40, 92, 172];
    let quilt = |p: Px| if p.vertical() && p.j % 3 == 2 { 0.82 } else { 1.0 };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() && p.i == 4 && p.j >= 1 {
            return c([190, 195, 205]); // Reißverschluss
        }
        if p.vertical() && p.j == 11 {
            return d(jacket, 0.65);
        }
        d(jacket, quilt(p))
    });
    // Schal
    cv.paint(BODY, Layer::Overlay, &|p| {
        let red = [200, 40, 50];
        if p.face == Face::Top && (p.j == 0 || p.j == 3 || p.i == 0 || p.i == 7) {
            return c(red);
        }
        if p.vertical() && p.j <= 1 {
            return if p.j == 1 && p.i % 2 == 0 { c([235, 235, 235]) } else { c(red) };
        }
        if p.front() && (1..=2).contains(&p.i) && p.j <= 5 {
            return if p.j == 5 { c([235, 235, 235]) } else { d(red, 0.92) };
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 3) {
            return c([42, 42, 48]); // Handschuhe
        }
        if p.face == Face::Top {
            return c(jacket);
        }
        d(jacket, quilt(p))
    });
    cv.paint(ARMS, Layer::Overlay, &|p| if p.vertical() && p.j == 8 { c([238, 238, 238]) } else { None });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom {
            return c([60, 40, 25]);
        }
        if p.vertical() && p.j >= 9 {
            return c([115, 72, 40]);
        }
        let jeans = [46, 62, 112];
        d(jeans, if p.front() && (3..=5).contains(&p.j) && p.i == 1 { 1.15 } else { 1.0 })
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.vertical() && p.j == 8 { c([236, 232, 222]) } else { None });
}

fn suit(cv: &mut Canvas) {
    let black = [30, 30, 36];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            let center = (p.i as f32 - 3.5).abs();
            let v_open = p.j <= 5 && center <= (5.0 - p.j as f32) * 0.45 + 0.6;
            if (p.i == 3 || p.i == 4) && (1..=7).contains(&p.j) {
                return if p.j == 1 { c([150, 20, 30]) } else { c([185, 30, 40]) }; // Krawatte
            }
            if v_open {
                return c([238, 238, 240]); // Hemd
            }
            if p.j <= 6 && center <= (6.0 - p.j as f32) * 0.45 + 1.4 {
                return c([62, 62, 70]); // Revers
            }
            if p.i == 4 && (p.j == 8 || p.j == 10) {
                return c([80, 80, 88]); // Knöpfe
            }
        }
        c(black)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return SKIN;
        }
        if p.vertical() && p.j == 9 {
            return c([236, 236, 240]); // Manschette
        }
        c(black)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return if p.front() && p.j == 10 && p.i == 1 { c([70, 70, 75]) } else { c([16, 16, 18]) };
        }
        d([36, 36, 42], if p.front() && p.i == 1 { 1.2 } else { 1.0 })
    });
}

fn pajamas(cv: &mut Canvas) {
    let blue = [150, 190, 232];
    let stripe = |p: Px| if p.vertical() && p.i % 3 == 0 { c([232, 240, 250]) } else { c(blue) };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j == 0 && (1..=6).contains(&p.i) || p.j == 1 && (p.i == 2 || p.i == 5) {
                return c([88, 118, 170]); // Kragen
            }
            if p.i == 4 && (p.j == 3 || p.j == 6 || p.j == 9) {
                return c([250, 250, 250]); // Knöpfe
            }
            if (1..=2).contains(&p.i) && (3..=4).contains(&p.j) {
                return c([110, 150, 205]); // Brusttasche
            }
        }
        stripe(p)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return SKIN;
        }
        if p.vertical() && p.j == 9 {
            return c([88, 118, 170]);
        }
        stripe(p)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return c([150, 100, 190]); // Hausschuhe
        }
        if p.vertical() && p.j == 9 {
            return c([88, 118, 170]);
        }
        stripe(p)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.vertical() && p.j == 10 { c([205, 175, 225]) } else { None });
}

fn hoodie(cv: &mut Canvas) {
    let h = [118, 64, 168];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if (p.i == 2 || p.i == 5) && (1..=4).contains(&p.j) {
                return c([236, 236, 236]); // Kordeln
            }
            if p.j == 0 && (2..=5).contains(&p.i) {
                return d(h, 0.6);
            }
            if (1..=6).contains(&p.i) && (7..=10).contains(&p.j) {
                return d(h, if p.j == 7 { 0.72 } else { 0.86 }); // Bauchtasche
            }
        }
        if p.vertical() && p.j == 11 {
            return d(h, 0.78);
        }
        c(h)
    });
    // Kapuze hinten
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.back() && p.j <= 3 {
            return d(h, if p.j == 3 { 0.8 } else { 1.06 });
        }
        if p.face == Face::Top && p.j == 0 {
            return d(h, 1.06);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 1) {
            return SKIN;
        }
        if p.vertical() && p.j == 10 {
            return d(h, 0.78);
        }
        c(h)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j == 11) {
            return c([150, 150, 156]);
        }
        if p.vertical() && p.j == 10 {
            return c([238, 238, 238]);
        }
        d([60, 82, 132], if p.front() && (3..=5).contains(&p.j) && p.i <= 1 { 1.18 } else { 1.0 })
    });
}

fn tracksuit(cv: &mut Canvas) {
    let r = [200, 40, 46];
    let white = [240, 240, 240];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() && p.i == 4 {
            return c([200, 200, 205]);
        }
        if p.vertical() && p.j == 0 {
            return c(white);
        }
        if p.side() && (1..=2).contains(&p.i) {
            return c(white);
        }
        c(r)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 1) {
            return SKIN;
        }
        if p.vertical() && p.j == 10 {
            return c(white);
        }
        if p.outer() && (1..=2).contains(&p.i) {
            return c(white);
        }
        c(r)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j == 11) {
            return c([30, 30, 30]);
        }
        if p.vertical() && p.j == 10 {
            return c(white);
        }
        if p.outer() && (1..=2).contains(&p.i) {
            return c(white);
        }
        c(r)
    });
}

fn summer(cv: &mut Canvas) {
    let shirt = [242, 202, 64];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j >= 10 {
            return c([82, 124, 186]); // Shorts-Bund
        }
        if p.front() {
            let (dx, dy) = (p.i as f32 - 3.5, p.j as f32 - 5.5);
            if dx * dx + dy * dy <= 3.2 {
                return c([238, 120, 40]); // Sonne
            }
        }
        if p.front() && p.j == 0 && (2..=5).contains(&p.i) {
            return SKIN; // Ausschnitt
        }
        c(shirt)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if p.face == Face::Top || (p.vertical() && p.j <= 3) {
            return if p.vertical() && p.j == 3 { d(shirt, 0.85) } else { c(shirt) };
        }
        SKIN
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j == 11) {
            return c([120, 80, 45]); // Sandalen
        }
        if p.face == Face::Top || (p.vertical() && p.j <= 4) {
            return c([82, 124, 186]);
        }
        SKIN
    });
}

fn knight(cv: &mut Canvas) {
    let metal = [150, 156, 168];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j == 8 {
            return if p.front() && (3..=4).contains(&p.i) { c([222, 182, 60]) } else { c([102, 70, 40]) };
        }
        if p.front() && (1..=6).contains(&p.j) {
            if p.i == 3 || p.i == 4 {
                return d(metal, if p.i == 3 { 1.18 } else { 0.92 });
            }
            return d(metal, 1.08 - p.j as f32 * 0.02);
        }
        if p.vertical() && (p.i == 0 || p.i + 1 == p.w) && p.j % 3 == 1 {
            return c([92, 96, 104]); // Nieten
        }
        d(metal, if p.j >= 9 { 0.85 } else { 1.0 })
    });
    // Wappenrock
    cv.paint(BODY, Layer::Overlay, &|p| {
        if (p.front() || p.back()) && (2..=5).contains(&p.i) && p.j >= 2 {
            let cross = ((3..=4).contains(&p.i) && (4..=9).contains(&p.j)) || (p.j == 6);
            return if cross { c([228, 190, 64]) } else { c([168, 30, 42]) };
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if p.face == Face::Top || (p.vertical() && p.j <= 3) {
            return d(metal, if p.j == 3 { 0.7 } else { 1.12 });
        }
        if p.vertical() && (4..=8).contains(&p.j) {
            return if (p.i + p.j) % 2 == 0 { c([120, 124, 134]) } else { c([146, 150, 160]) }; // Kettenhemd
        }
        d(metal, 1.02)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j == 11) {
            return c([84, 88, 96]);
        }
        if p.vertical() && (4..=5).contains(&p.j) {
            return d(metal, 1.16); // Knieschutz
        }
        c(metal)
    });
}

fn doctor(cv: &mut Canvas) {
    let scrubs = [62, 162, 152];
    let coat = [242, 244, 246];
    cv.paint(BODY, Layer::Base, &|_| c(scrubs));
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.front() && (3..=4).contains(&p.i) {
            return None; // offener Kittel
        }
        if p.front() && p.i == 1 && p.j == 5 {
            return c([40, 80, 200]); // Kuli
        }
        if p.front() && p.i <= 1 && (6..=7).contains(&p.j) {
            return c([206, 210, 214]); // Tasche
        }
        c(coat)
    });
    cv.paint(ARMS, Layer::Base, &|p| if hand(p, 2) { SKIN } else { c(scrubs) });
    cv.paint(ARMS, Layer::Overlay, &|p| {
        if p.face == Face::Top || (p.vertical() && p.j <= 9) {
            c(coat)
        } else {
            None
        }
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            c([236, 236, 236])
        } else {
            c(scrubs)
        }
    });
    // langer Kittel über den Oberschenkeln
    cv.paint(LEGS, Layer::Overlay, &|p| {
        let inner = match p.part {
            Part::RightLeg => p.face == Face::Left,
            _ => p.face == Face::Right,
        };
        if p.vertical() && p.j <= 3 && !inner {
            c(coat)
        } else {
            None
        }
    });
}

fn police(cv: &mut Canvas) {
    let navy = [32, 44, 86];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j == 8 {
            return if p.front() && (3..=4).contains(&p.i) { c([200, 204, 210]) } else { c([20, 20, 22]) };
        }
        if p.front() {
            if (3..=4).contains(&p.i) && p.j <= 6 {
                return c([18, 22, 44]); // Krawatte
            }
            if (1..=2).contains(&p.i) && (2..=3).contains(&p.j) {
                return c([226, 192, 64]); // Abzeichen
            }
            if (5..=6).contains(&p.i) && (3..=4).contains(&p.j) {
                return d(navy, 0.8); // Tasche
            }
        }
        c(navy)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return SKIN;
        }
        if p.outer() && (1..=2).contains(&p.i) && (1..=2).contains(&p.j) {
            return c([190, 160, 60]); // Ärmelabzeichen
        }
        c(navy)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return c([16, 16, 18]);
        }
        if p.outer() && p.i == 2 {
            return c([60, 80, 140]);
        }
        c([24, 32, 62])
    });
}

fn chef(cv: &mut Canvas) {
    let white = [244, 244, 242];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if p.j <= 1 && (2..=5).contains(&p.i) {
                return c([200, 40, 45]); // Halstuch
            }
            if (p.i == 2 || p.i == 5) && (p.j == 3 || p.j == 6 || p.j == 9) {
                return c([60, 60, 64]); // Knöpfe
            }
        }
        if p.vertical() && p.j == 0 {
            return c([214, 214, 214]);
        }
        c(white)
    });
    // Schürze
    cv.paint(BODY, Layer::Overlay, &|p| if p.front() && p.j >= 7 { c([228, 230, 232]) } else { None });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return SKIN;
        }
        if p.vertical() && p.j == 9 {
            return c([206, 206, 206]);
        }
        c(white)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return c([22, 22, 22]);
        }
        if (p.i + p.j) % 2 == 0 { c([30, 30, 30]) } else { c([236, 236, 236]) }
    });
}

fn santa(cv: &mut Canvas) {
    let red = [192, 30, 36];
    let fur = [244, 244, 240];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j == 8 {
            return if p.front() && (3..=4).contains(&p.i) { c([232, 192, 60]) } else { c([22, 22, 22]) };
        }
        if p.vertical() && p.j == 11 {
            return c(fur);
        }
        if p.front() && (3..=4).contains(&p.i) {
            return c(fur);
        }
        c(red)
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 1) {
            return c([28, 28, 28]); // Handschuhe
        }
        if p.vertical() && (9..=10).contains(&p.j) {
            return c(fur);
        }
        c(red)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 8) {
            return c([22, 22, 22]);
        }
        c(red)
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.vertical() && p.j == 8 { c(fur) } else { None });
}

fn ninja(cv: &mut Canvas) {
    let black = [26, 26, 32];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && p.j == 8 {
            return c([180, 30, 34]); // Gürtel
        }
        if p.front() && p.j <= 4 {
            let center = (p.i as f32 - 3.5).abs();
            if center <= (4.0 - p.j as f32) * 0.5 {
                return c([44, 44, 52]);
            }
        }
        c(black)
    });
    cv.paint(BODY, Layer::Overlay, &|p| if p.front() && (5..=6).contains(&p.i) && (9..=10).contains(&p.j) { c([180, 30, 34]) } else { None });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 1) {
            return SKIN;
        }
        if p.vertical() && (7..=10).contains(&p.j) {
            return if p.j % 2 == 0 { c([70, 70, 76]) } else { c([42, 42, 48]) }; // Bandagen
        }
        c(black)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j == 11) {
            return c([18, 18, 20]);
        }
        if p.vertical() && (8..=10).contains(&p.j) {
            return if p.j % 2 == 0 { c([70, 70, 76]) } else { c([42, 42, 48]) };
        }
        c(black)
    });
}

fn astronaut(cv: &mut Canvas) {
    let white = [232, 234, 238];
    let joint = [140, 144, 152];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() && (2..=5).contains(&p.i) && (4..=7).contains(&p.j) {
            return match (p.i, p.j) {
                (3, 5) => c([220, 50, 50]),
                (4, 5) => c([60, 200, 90]),
                (3, 6) => c([60, 120, 230]),
                (4, 6) => c([240, 200, 60]),
                _ => c([120, 124, 132]),
            };
        }
        if p.vertical() && p.j == 9 {
            return c(joint);
        }
        c(white)
    });
    // Rucksack
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.back() && (1..=6).contains(&p.i) && (1..=10).contains(&p.j) {
            return if p.i == 1 || p.i == 6 || p.j == 1 || p.j == 10 { c([180, 184, 192]) } else { c([246, 246, 248]) };
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return c([170, 174, 182]); // Handschuhe
        }
        if p.vertical() && (5..=6).contains(&p.j) {
            return c(joint);
        }
        if p.outer() && (1..=2).contains(&p.j) {
            return c([236, 120, 40]); // Ärmelstreifen
        }
        c(white)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return c([100, 104, 112]);
        }
        if p.vertical() && (5..=6).contains(&p.j) {
            return c(joint);
        }
        c(white)
    });
}

fn pirate(cv: &mut Canvas) {
    let shirt = [236, 230, 214];
    let vest = [102, 62, 32];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.vertical() && (8..=9).contains(&p.j) {
            return c([170, 30, 34]); // Schärpe
        }
        if p.front() && p.j <= 2 && (3..=4).contains(&p.i) {
            return SKIN; // offener Kragen
        }
        if p.vertical() && p.j >= 10 {
            return c([30, 30, 34]);
        }
        c(shirt)
    });
    cv.paint(BODY, Layer::Overlay, &|p| {
        if p.vertical() && p.j <= 7 && !(p.front() && (2..=5).contains(&p.i)) {
            return c(vest);
        }
        None
    });
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 1) {
            return SKIN;
        }
        if p.vertical() && p.j == 10 {
            return d(shirt, 0.86);
        }
        c(shirt)
    });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 8) {
            return c([92, 56, 30]); // Stiefel
        }
        c([32, 32, 36])
    });
    cv.paint(LEGS, Layer::Overlay, &|p| if p.vertical() && p.j == 8 { c([126, 82, 46]) } else { None });
}

fn overalls(cv: &mut Canvas) {
    let denim = [62, 94, 162];
    let check = |p: Px| {
        let dark = (p.i / 2 + p.j / 2) % 2 == 0;
        if dark { c([140, 24, 26]) } else { c([196, 44, 44]) }
    };
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() {
            if (p.i == 1 || p.i == 6) && p.j == 3 {
                return c([236, 196, 60]); // Knöpfe
            }
            if (1..=6).contains(&p.i) && p.j >= 3 {
                return c(denim);
            }
            if (p.i == 1 || p.i == 6) && p.j <= 2 {
                return c(denim); // Träger
            }
        }
        if p.back() && (p.i == p.j / 2 + 1 || p.i + p.j / 2 == 6) && p.j <= 8 {
            return c(denim);
        }
        if p.vertical() && p.j >= 8 {
            return c(denim);
        }
        check(p)
    });
    cv.paint(ARMS, Layer::Base, &|p| if hand(p, 2) { SKIN } else { check(p) });
    cv.paint(LEGS, Layer::Base, &|p| {
        if p.face == Face::Bottom || (p.vertical() && p.j >= 10) {
            return c([96, 60, 32]);
        }
        c(denim)
    });
}

fn skeleton(cv: &mut Canvas) {
    let black = [24, 24, 26];
    let bone = [226, 226, 216];
    cv.paint(BODY, Layer::Base, &|p| {
        if p.front() || p.back() {
            if (3..=4).contains(&p.i) && (1..=9).contains(&p.j) {
                return c(bone); // Wirbelsäule
            }
            if (p.j == 2 || p.j == 4 || p.j == 6) && (1..=6).contains(&p.i) {
                return c(bone); // Rippen
            }
            if (9..=10).contains(&p.j) && (1..=6).contains(&p.i) && p.front() {
                return c(bone); // Becken
            }
        }
        c(black)
    });
    let limb = |p: Px| {
        if p.vertical() && (1..=2).contains(&p.i) {
            return c(bone);
        }
        if p.vertical() && p.j == 5 {
            return c(bone);
        }
        c(black)
    };
    cv.paint(ARMS, Layer::Base, &|p| {
        if hand(p, 2) {
            return if p.face == Face::Bottom || p.i % 2 == 0 { c(bone) } else { c(black) };
        }
        limb(p)
    });
    cv.paint(LEGS, Layer::Base, &|p| limb(p));
}

/// Alle Vorlagen: (Kennung, Name für die Übersetzung, Zeichenfunktion)
pub const OUTFITS: &[(&str, &str, fn(&mut Canvas))] = &[
    ("winter", "Winterjacke", winter),
    ("suit", "Anzug", suit),
    ("pajamas", "Schlafanzug", pajamas),
    ("hoodie", "Hoodie", hoodie),
    ("tracksuit", "Trainingsanzug", tracksuit),
    ("summer", "Sommer", summer),
    ("knight", "Ritter", knight),
    ("doctor", "Arzt", doctor),
    ("police", "Polizei", police),
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
