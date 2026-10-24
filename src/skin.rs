//! Minecraft-Skin-Datenmodell: Pixel, Körperteile, UV-Layout, Laden/Speichern.

use std::sync::OnceLock;

pub const SIZE: usize = 64;
pub type Rgba = [u8; 4];
pub const CLEAR: Rgba = [0, 0, 0, 0];

#[derive(Clone, PartialEq, Eq)]
pub struct Skin {
    pub px: Vec<Rgba>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Model {
    #[default]
    Classic,
    Slim,
}

impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Model::Classic => "Classic (Steve, 4px)",
            Model::Slim => "Slim (Alex, 3px)",
        }
    }
    fn idx(self) -> usize {
        match self {
            Model::Classic => 0,
            Model::Slim => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Head,
    Body,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
}

impl Part {
    pub const ALL: [Part; 6] = [
        Part::Head,
        Part::Body,
        Part::RightArm,
        Part::LeftArm,
        Part::RightLeg,
        Part::LeftLeg,
    ];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Part::Head => "Kopf",
            Part::Body => "Körper",
            Part::RightArm => "Rechter Arm",
            Part::LeftArm => "Linker Arm",
            Part::RightLeg => "Rechtes Bein",
            Part::LeftLeg => "Linkes Bein",
        }
    }
    pub fn mirror(self) -> Part {
        match self {
            Part::RightArm => Part::LeftArm,
            Part::LeftArm => Part::RightArm,
            Part::RightLeg => Part::LeftLeg,
            Part::LeftLeg => Part::RightLeg,
            p => p,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Base,
    Overlay,
}

impl Layer {
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Layer::Base => "1. Ebene",
            Layer::Overlay => "2. Ebene",
        }
    }
}

/// Flächen eines Quaders. `Right` liegt auf der rechten Seite der Figur (-x),
/// `Left` auf der linken (+x). Die Figur schaut Richtung +z.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Top,
    Bottom,
    Right,
    Front,
    Left,
    Back,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back];
    pub fn name(self) -> &'static str {
        match self {
            Face::Top => "oben",
            Face::Bottom => "unten",
            Face::Right => "rechts",
            Face::Front => "vorne",
            Face::Left => "links",
            Face::Back => "hinten",
        }
    }
    pub fn mirror(self) -> Face {
        match self {
            Face::Right => Face::Left,
            Face::Left => Face::Right,
            f => f,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BoxDef {
    pub part: Part,
    pub layer: Layer,
    pub uv: (usize, usize),
    /// Breite (x), Höhe (y), Tiefe (z) in Pixeln.
    pub size: (usize, usize, usize),
    /// Minimale Ecke im Modellraum (Füße bei y = 0).
    pub min: [f32; 3],
    pub inflate: f32,
}

impl BoxDef {
    pub fn face_rect(&self, face: Face) -> (usize, usize, usize, usize) {
        let (u, v) = self.uv;
        let (w, h, d) = self.size;
        match face {
            Face::Top => (u + d, v, w, d),
            Face::Bottom => (u + d + w, v, w, d),
            Face::Right => (u, v + d, d, h),
            Face::Front => (u + d, v + d, w, h),
            Face::Left => (u + d + w, v + d, d, h),
            Face::Back => (u + 2 * d + w, v + d, w, h),
        }
    }

    /// Fläche im 3D-Raum: Ecke für Texel (0,0), Kante entlang u, Kante entlang v, äußere Normale.
    pub fn face_geometry(&self, face: Face) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
        let i = self.inflate;
        let (w, h, d) = (self.size.0 as f32, self.size.1 as f32, self.size.2 as f32);
        let (x0, y0, z0) = (self.min[0] - i, self.min[1] - i, self.min[2] - i);
        let (x1, y1, z1) = (self.min[0] + w + i, self.min[1] + h + i, self.min[2] + d + i);
        let (dx, dy, dz) = (x1 - x0, y1 - y0, z1 - z0);
        match face {
            Face::Front => ([x0, y1, z1], [dx, 0.0, 0.0], [0.0, -dy, 0.0], [0.0, 0.0, 1.0]),
            Face::Back => ([x1, y1, z0], [-dx, 0.0, 0.0], [0.0, -dy, 0.0], [0.0, 0.0, -1.0]),
            Face::Right => ([x0, y1, z0], [0.0, 0.0, dz], [0.0, -dy, 0.0], [-1.0, 0.0, 0.0]),
            Face::Left => ([x1, y1, z1], [0.0, 0.0, -dz], [0.0, -dy, 0.0], [1.0, 0.0, 0.0]),
            Face::Top => ([x0, y1, z0], [dx, 0.0, 0.0], [0.0, 0.0, dz], [0.0, 1.0, 0.0]),
            Face::Bottom => ([x0, y0, z0], [dx, 0.0, 0.0], [0.0, 0.0, dz], [0.0, -1.0, 0.0]),
        }
    }
}

pub fn boxes(model: Model) -> [BoxDef; 12] {
    let aw = if model == Model::Slim { 3 } else { 4 };
    let awf = aw as f32;
    let mk = |part, layer, uv, size, min, inflate| BoxDef { part, layer, uv, size, min, inflate };
    use Layer::*;
    use Part::*;
    [
        mk(Head, Base, (0, 0), (8, 8, 8), [-4.0, 24.0, -4.0], 0.0),
        mk(Body, Base, (16, 16), (8, 12, 4), [-4.0, 12.0, -2.0], 0.0),
        mk(RightArm, Base, (40, 16), (aw, 12, 4), [-4.0 - awf, 12.0, -2.0], 0.0),
        mk(LeftArm, Base, (32, 48), (aw, 12, 4), [4.0, 12.0, -2.0], 0.0),
        mk(RightLeg, Base, (0, 16), (4, 12, 4), [-4.0, 0.0, -2.0], 0.0),
        mk(LeftLeg, Base, (16, 48), (4, 12, 4), [0.0, 0.0, -2.0], 0.0),
        mk(Head, Overlay, (32, 0), (8, 8, 8), [-4.0, 24.0, -4.0], 0.5),
        mk(Body, Overlay, (16, 32), (8, 12, 4), [-4.0, 12.0, -2.0], 0.25),
        mk(RightArm, Overlay, (40, 32), (aw, 12, 4), [-4.0 - awf, 12.0, -2.0], 0.25),
        mk(LeftArm, Overlay, (48, 48), (aw, 12, 4), [4.0, 12.0, -2.0], 0.25),
        mk(RightLeg, Overlay, (0, 32), (4, 12, 4), [-4.0, 0.0, -2.0], 0.25),
        mk(LeftLeg, Overlay, (0, 48), (4, 12, 4), [0.0, 0.0, -2.0], 0.25),
    ]
}

pub fn box_of(model: Model, part: Part, layer: Layer) -> BoxDef {
    boxes(model)[layer.idx() * 6 + part.idx()]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexelInfo {
    pub part: Part,
    pub layer: Layer,
    pub face: Face,
    pub i: usize,
    pub j: usize,
    pub fw: usize,
    pub fh: usize,
}

/// Für jedes Texel: zu welchem Körperteil/welcher Fläche es gehört (None = ungenutzt).
pub fn texel_map(model: Model) -> &'static [Option<TexelInfo>] {
    static MAPS: OnceLock<[Vec<Option<TexelInfo>>; 2]> = OnceLock::new();
    let maps = MAPS.get_or_init(|| {
        let build = |m: Model| {
            let mut map = vec![None; SIZE * SIZE];
            for b in boxes(m) {
                for face in Face::ALL {
                    let (fx, fy, fw, fh) = b.face_rect(face);
                    for j in 0..fh {
                        for i in 0..fw {
                            map[(fy + j) * SIZE + fx + i] = Some(TexelInfo {
                                part: b.part,
                                layer: b.layer,
                                face,
                                i,
                                j,
                                fw,
                                fh,
                            });
                        }
                    }
                }
            }
            map
        };
        [build(Model::Classic), build(Model::Slim)]
    });
    &maps[model.idx()]
}

pub fn texel_info(model: Model, x: usize, y: usize) -> Option<TexelInfo> {
    if x >= SIZE || y >= SIZE {
        return None;
    }
    texel_map(model)[y * SIZE + x]
}

pub fn texel_at(model: Model, part: Part, layer: Layer, face: Face, i: usize, j: usize) -> (usize, usize) {
    let (fx, fy, _, _) = box_of(model, part, layer).face_rect(face);
    (fx + i, fy + j)
}

/// Spiegelt ein Texel an der Symmetrieachse der Figur (links ↔ rechts).
pub fn mirror_texel(model: Model, x: usize, y: usize) -> Option<(usize, usize)> {
    let t = texel_info(model, x, y)?;
    Some(texel_at(model, t.part.mirror(), t.layer, t.face.mirror(), t.fw - 1 - t.i, t.j))
}

impl Default for Skin {
    fn default() -> Self {
        Self::blank()
    }
}

impl Skin {
    pub fn blank() -> Self {
        Skin { px: vec![CLEAR; SIZE * SIZE] }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.px[y * SIZE + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: Rgba) {
        self.px[y * SIZE + x] = c;
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.px.iter().flatten().copied().collect()
    }

    pub fn from_png(bytes: &[u8]) -> Result<Skin, String> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| format!("Kein gültiges PNG: {e}"))?;
        let mut buf = vec![0; reader.output_buffer_size().ok_or("Bild zu groß")?];
        let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG-Fehler: {e}"))?;
        let (w, h) = (info.width as usize, info.height as usize);
        let channels = info.color_type.samples();
        let pixel = |x: usize, y: usize| -> Rgba {
            let o = (y * w + x) * channels;
            match channels {
                1 => [buf[o], buf[o], buf[o], 255],
                2 => [buf[o], buf[o], buf[o], buf[o + 1]],
                3 => [buf[o], buf[o + 1], buf[o + 2], 255],
                _ => [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]],
            }
        };
        if w < 64 || w % 64 != 0 || (h != w && h * 2 != w) {
            return Err(format!(
                "Das Bild hat {w}×{h} Pixel. Ein Minecraft-Skin muss 64×64 (oder alt 64×32) groß sein."
            ));
        }
        // HD-Skins (128, 256 …) werden auf 64 px heruntergerechnet.
        let scale = w / 64;
        let legacy = h * 2 == w;
        let mut skin = Skin::blank();
        let rows = if legacy { 32 } else { 64 };
        for y in 0..rows {
            for x in 0..64 {
                skin.set(x, y, pixel(x * scale, y * scale));
            }
        }
        if legacy {
            skin.convert_legacy();
        }
        Ok(skin)
    }

    /// Altes 64×32-Format: linke Gliedmaßen aus den rechten spiegeln.
    fn convert_legacy(&mut self) {
        // Notch-Transparenz: Ist die Hut-Ebene komplett deckend, war sie nicht gemeint.
        let hat_opaque = (0..16).all(|y| (32..64).all(|x| self.get(x, y)[3] == 255));
        if hat_opaque {
            for y in 0..16 {
                for x in 32..64 {
                    self.set(x, y, CLEAR);
                }
            }
        }
        for (src, dst) in [(Part::RightArm, Part::LeftArm), (Part::RightLeg, Part::LeftLeg)] {
            let b = box_of(Model::Classic, src, Layer::Base);
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                for j in 0..fh {
                    for i in 0..fw {
                        let (x, y) = texel_at(Model::Classic, dst, Layer::Base, face.mirror(), fw - 1 - i, j);
                        let c = self.get(fx + i, fy + j);
                        self.set(x, y, c);
                    }
                }
            }
        }
    }

    pub fn to_png(&self) -> Vec<u8> {
        encode_png(SIZE as u32, SIZE as u32, &self.as_bytes())
    }

    /// Alle Texel, die zu keinem Körperteil gehören, leeren.
    pub fn clean_unused(&mut self, model: Model) {
        let map = texel_map(model);
        for (i, m) in map.iter().enumerate() {
            if m.is_none() {
                self.px[i] = CLEAR;
            }
        }
    }
}

pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("PNG-Header");
        writer.write_image_data(rgba).expect("PNG-Daten");
    }
    out
}

/// Kopiert einen Körperteil (eine Ebene) von einem Skin in einen anderen.
/// Unterschiedliche Armbreiten (Classic/Slim) werden automatisch umgerechnet.
pub fn copy_part(src: &Skin, src_model: Model, dst: &mut Skin, dst_model: Model, part: Part, layer: Layer) {
    let sb = box_of(src_model, part, layer);
    let db = box_of(dst_model, part, layer);
    for face in Face::ALL {
        let (sx, sy, sw, sh) = sb.face_rect(face);
        let (dx, dy, dw, dh) = db.face_rect(face);
        for j in 0..dh {
            for i in 0..dw {
                let si = resample(i, dw, sw);
                let sj = resample(j, dh, sh);
                dst.set(dx + i, dy + j, src.get(sx + si, sy + sj));
            }
        }
    }
}

/// Wandelt einen Skin zwischen Classic- und Slim-Armen um.
pub fn convert_model(skin: &Skin, from: Model, to: Model) -> Skin {
    if from == to {
        return skin.clone();
    }
    let mut out = skin.clone();
    for part in [Part::RightArm, Part::LeftArm] {
        for layer in [Layer::Base, Layer::Overlay] {
            // alten Bereich leeren
            let ob = box_of(from, part, layer);
            for face in Face::ALL {
                let (x, y, w, h) = ob.face_rect(face);
                for j in 0..h {
                    for i in 0..w {
                        out.set(x + i, y + j, CLEAR);
                    }
                }
            }
            copy_part(skin, from, &mut out, to, part, layer);
        }
    }
    out
}

fn resample(i: usize, dst_len: usize, src_len: usize) -> usize {
    if dst_len == src_len {
        return i;
    }
    if src_len > dst_len {
        // Schmaler werden: mittlere Spalte entfernen, Ränder bleiben erhalten.
        let drop = src_len / 2;
        if i >= drop { i + (src_len - dst_len) } else { i }
    } else {
        // Breiter werden: mittlere Spalte verdoppeln.
        let dup = dst_len / 2;
        if i > dup { i - (dst_len - src_len) } else { i.min(src_len - 1) }
    }
}

/// Eine eigene Start-Figur (kein Mojang-Skin), damit man sofort etwas sieht.
pub fn starter_skin() -> Skin {
    let mut s = Skin::blank();
    let model = Model::Classic;
    let skin_c = [214, 164, 130];
    let hair = [92, 58, 33];
    let shirt = [46, 139, 170];
    let pants = [52, 64, 120];
    let shoe = [60, 60, 66];
    let noise = |x: usize, y: usize, amp: f32| -> f32 {
        let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        ((h >> 16) & 0xff) as f32 / 255.0 * 2.0 * amp - amp
    };
    let shade = |c: [u8; 3], f: f32| -> Rgba {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
            255,
        ]
    };
    for part in Part::ALL {
        let b = box_of(model, part, Layer::Base);
        for face in Face::ALL {
            let (fx, fy, fw, fh) = b.face_rect(face);
            for j in 0..fh {
                for i in 0..fw {
                    let (x, y) = (fx + i, fy + j);
                    let n = 1.0 + noise(x, y, 0.07);
                    let c = match part {
                        Part::Head => match face {
                            Face::Top => shade(hair, n),
                            Face::Bottom => shade(skin_c, 0.9 * n),
                            Face::Back => shade(hair, n * if j > 5 { 0.85 } else { 1.0 }),
                            Face::Right | Face::Left => {
                                if j < 2 || (j < 5 && ((face == Face::Right && i < 4) || (face == Face::Left && i > 3))) {
                                    shade(hair, n)
                                } else {
                                    shade(skin_c, n)
                                }
                            }
                            Face::Front => {
                                if j < 2 || (j == 2 && (i == 0 || i == 7)) {
                                    shade(hair, n)
                                } else if j == 4 && (i == 1 || i == 6) {
                                    [245, 245, 245, 255]
                                } else if j == 4 && (i == 2 || i == 5) {
                                    [58, 96, 170, 255]
                                } else if j == 5 && (i == 3 || i == 4) {
                                    shade(skin_c, 0.82)
                                } else if j == 6 && (2..6).contains(&i) {
                                    shade([150, 90, 80], 1.0)
                                } else {
                                    shade(skin_c, n)
                                }
                            }
                        },
                        Part::Body => {
                            if j == fh - 1 && matches!(face, Face::Front | Face::Back | Face::Left | Face::Right) {
                                shade([40, 40, 44], n)
                            } else {
                                shade(shirt, n * if face == Face::Front && (i == 3 || i == 4) && j < 3 { 0.8 } else { 1.0 })
                            }
                        }
                        Part::RightArm | Part::LeftArm => {
                            if face == Face::Top || (face != Face::Bottom && j < 4) {
                                shade(shirt, n * if j == 3 { 0.85 } else { 1.0 })
                            } else {
                                shade(skin_c, n)
                            }
                        }
                        Part::RightLeg | Part::LeftLeg => {
                            if face == Face::Bottom || (face != Face::Top && j >= 10) {
                                shade(shoe, n)
                            } else {
                                shade(pants, n)
                            }
                        }
                    };
                    s.set(x, y, c);
                }
            }
        }
    }
    s
}
