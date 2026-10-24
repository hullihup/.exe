//! Kleiner Software-Renderer für die 3D-Vorschau.
//!
//! Gerendert wird nur, wenn sich etwas ändert (Skin, Kamera, Größe). Neben dem Bild
//! entsteht ein Pick-Puffer, der zu jedem Bildpunkt das getroffene Skin-Texel speichert –
//! damit kann man direkt auf dem 3D-Modell malen.

use crate::skin::{boxes, Face, Layer, Model, Skin, SIZE};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { yaw: -0.55, pitch: 0.25, zoom: 1.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Visibility {
    pub base: [bool; 6],
    pub overlay: [bool; 6],
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility { base: [true; 6], overlay: [true; 6] }
    }
}

#[derive(Clone, PartialEq)]
pub struct RenderParams {
    pub cam: Camera,
    pub model: Model,
    pub vis: Visibility,
    pub highlight: Vec<u16>,
    pub width: usize,
    pub height: usize,
    pub supersample: usize,
}

pub struct RenderOut {
    pub width: usize,
    pub height: usize,
    /// vormultiplizierte RGBA-Werte
    pub rgba: Vec<u8>,
    /// Texel-Index + 1 pro Pixel (Auflösung width × height), 0 = nichts
    pub pick: Vec<u16>,
}

type V3 = [f32; 3];

#[inline]
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

struct Rot {
    cy: f32,
    sy: f32,
    cp: f32,
    sp: f32,
}

impl Rot {
    #[inline]
    fn apply(&self, v: V3) -> V3 {
        // erst um y (yaw), dann um x (pitch)
        let x = v[0] * self.cy + v[2] * self.sy;
        let z = -v[0] * self.sy + v[2] * self.cy;
        let y = v[1] * self.cp - z * self.sp;
        let z2 = v[1] * self.sp + z * self.cp;
        [x, y, z2]
    }
}

pub fn render(skin: &Skin, p: &RenderParams) -> RenderOut {
    let ss = p.supersample.max(1);
    let (w, h) = (p.width.max(1) * ss, p.height.max(1) * ss);
    let mut color = vec![[0f32; 4]; w * h];
    let mut depth = vec![f32::INFINITY; w * h];
    let mut pick_hi = vec![0u16; w * h];

    let rot = Rot { cy: p.cam.yaw.cos(), sy: p.cam.yaw.sin(), cp: p.cam.pitch.cos(), sp: p.cam.pitch.sin() };
    let center: V3 = [0.0, 16.0, 0.0];
    let dist = 120.0;
    let scale = (h as f32 / 43.0).min(w as f32 / 27.0) * p.cam.zoom;
    let f = scale * dist;
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let origin: V3 = [0.0, 0.0, dist];
    let light = {
        let l = [-0.45f32, 0.75, 0.6];
        let n = dot(l, l).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };

    // Durchgang 0: 1. Ebene, 1: 2. Ebene deckend, 2: 2. Ebene halbtransparent
    for pass in 0..3 {
        for b in boxes(p.model) {
            let visible = match b.layer {
                Layer::Base => p.vis.base[b.part.idx()],
                Layer::Overlay => p.vis.overlay[b.part.idx()],
            };
            if !visible || (pass == 0) != (b.layer == Layer::Base) {
                continue;
            }
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                let (p0, e1, e2, n) = b.face_geometry(face);
                let p0 = rot.apply(sub(p0, center));
                let e1 = rot.apply(e1);
                let e2 = rot.apply(e2);
                let n = rot.apply(n);
                let facing = dot(n, sub(origin, p0)) > 0.0;
                if b.layer == Layer::Base && !facing {
                    continue;
                }
                let mut bright = 0.62 + 0.38 * dot(n, light).max(0.0);
                if !facing {
                    bright *= 0.6;
                }
                // Bildschirm-Rechteck der Fläche
                let corners = [p0, add(p0, e1), add(p0, e2), add(add(p0, e1), e2)];
                let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for c in corners {
                    let zz = dist - c[2];
                    let sx = cx + f * c[0] / zz;
                    let sy = cy - f * c[1] / zz;
                    minx = minx.min(sx);
                    maxx = maxx.max(sx);
                    miny = miny.min(sy);
                    maxy = maxy.max(sy);
                }
                let x0 = (minx.floor().max(0.0)) as usize;
                let y0 = (miny.floor().max(0.0)) as usize;
                let x1 = (maxx.ceil().min(w as f32)) as usize;
                let y1 = (maxy.ceil().min(h as f32)) as usize;
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                let inv_e1 = 1.0 / dot(e1, e1);
                let inv_e2 = 1.0 / dot(e2, e2);
                let rel0 = sub(origin, p0);
                let num = dot(sub(p0, origin), n);
                for py in y0..y1 {
                    let dy = -(py as f32 + 0.5 - cy) / f;
                    for px in x0..x1 {
                        let dir = [(px as f32 + 0.5 - cx) / f, dy, -1.0];
                        let denom = dot(dir, n);
                        if denom.abs() < 1e-6 {
                            continue;
                        }
                        let t = num / denom;
                        if t <= 0.0 {
                            continue;
                        }
                        let idx = py * w + px;
                        if t >= depth[idx] {
                            continue;
                        }
                        let rel = [rel0[0] + dir[0] * t, rel0[1] + dir[1] * t, rel0[2] + dir[2] * t];
                        let s = dot(rel, e1) * inv_e1;
                        let v = dot(rel, e2) * inv_e2;
                        if !(0.0..1.0).contains(&s) || !(0.0..1.0).contains(&v) {
                            continue;
                        }
                        let tx = fx + ((s * fw as f32) as usize).min(fw - 1);
                        let ty = fy + ((v * fh as f32) as usize).min(fh - 1);
                        let c = skin.get(tx, ty);
                        let a = c[3];
                        let ok = match pass {
                            0 => a > 0,
                            1 => a == 255,
                            _ => a > 0 && a < 255,
                        };
                        if !ok {
                            continue;
                        }
                        let tid = (ty * SIZE + tx) as u16 + 1;
                        let mut rgb = [
                            c[0] as f32 / 255.0 * bright,
                            c[1] as f32 / 255.0 * bright,
                            c[2] as f32 / 255.0 * bright,
                        ];
                        if p.highlight.contains(&tid) {
                            for ch in &mut rgb {
                                *ch = *ch * 0.45 + 0.55;
                            }
                        }
                        if pass == 2 {
                            let af = a as f32 / 255.0;
                            let dst = &mut color[idx];
                            for k in 0..3 {
                                dst[k] = rgb[k] * af + dst[k] * (1.0 - af);
                            }
                            dst[3] = af + dst[3] * (1.0 - af);
                            pick_hi[idx] = tid;
                        } else {
                            color[idx] = [rgb[0], rgb[1], rgb[2], 1.0];
                            depth[idx] = t;
                            pick_hi[idx] = tid;
                        }
                    }
                }
            }
        }
    }

    // Kantenglättung: Supersampling herunterrechnen
    let (ow, oh) = (p.width.max(1), p.height.max(1));
    let mut rgba = vec![0u8; ow * oh * 4];
    let mut pick = vec![0u16; ow * oh];
    let inv = 1.0 / (ss * ss) as f32;
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                let row = (y * ss + sy) * w + x * ss;
                for sx in 0..ss {
                    let c = color[row + sx];
                    for k in 0..4 {
                        acc[k] += c[k];
                    }
                }
            }
            let o = (y * ow + x) * 4;
            for k in 0..4 {
                rgba[o + k] = (acc[k] * inv * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            pick[y * ow + x] = pick_hi[(y * ss + ss / 2) * w + x * ss + ss / 2];
        }
    }
    RenderOut { width: ow, height: oh, rgba, pick }
}

#[inline]
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
