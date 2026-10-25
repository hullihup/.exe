//! 3D-Ansicht auf der Grafikkarte (OpenGL über egui_glow).
//!
//! Die Grafikkarte zeichnet das Modell mit Tiefenpuffer und Pixelraster; der Prozessor
//! berechnet nur, welches Texel unter der Maus liegt (ein einzelner Strahl).
//! Fällt OpenGL aus, nutzt [`crate::preview`] den Software-Renderer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use eframe::egui_glow;
use eframe::glow::{self, HasContext as _};

use crate::render3d::{Camera, Visibility};
use crate::skin::{self, boxes, Face, Layer, Model, Skin, SIZE};

/// Läuft die Grafikkarten-Darstellung? (wird beim Start festgelegt)
pub static GPU_ENABLED: AtomicBool = AtomicBool::new(false);
/// Gab es einen Fehler beim Einrichten der Shader? Dann auf Software umschalten.
pub static GPU_FAILED: AtomicBool = AtomicBool::new(false);

pub fn gpu_active() -> bool {
    GPU_ENABLED.load(Ordering::Relaxed) && !GPU_FAILED.load(Ordering::Relaxed)
}

type V3 = [f32; 3];

#[inline]
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
#[inline]
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

const DIST: f32 = 120.0;
const NEAR: f32 = 1.0;
const FAR: f32 = 1000.0;

/// Kamera-Geometrie für eine Ansicht der Größe `w` × `h` (physische Pixel).
#[derive(Clone, Copy)]
pub struct View {
    cy_: f32,
    sy_: f32,
    cp: f32,
    sp: f32,
    f: f32,
    w: f32,
    h: f32,
}

impl View {
    pub fn new(cam: Camera, w: f32, h: f32) -> Self {
        let s = (h / 43.0).min(w / 27.0) * cam.zoom;
        View { cy_: cam.yaw.cos(), sy_: cam.yaw.sin(), cp: cam.pitch.cos(), sp: cam.pitch.sin(), f: s * DIST, w, h }
    }

    /// Modellraum → Kameraraum (Modellmitte im Ursprung, Kamera bei z = DIST)
    #[inline]
    pub fn to_cam(&self, p: V3) -> V3 {
        self.rot(sub(p, [0.0, 16.0, 0.0]))
    }

    #[inline]
    pub fn rot(&self, v: V3) -> V3 {
        let x = v[0] * self.cy_ + v[2] * self.sy_;
        let z = -v[0] * self.sy_ + v[2] * self.cy_;
        let y = v[1] * self.cp - z * self.sp;
        let z2 = v[1] * self.sp + z * self.cp;
        [x, y, z2]
    }

    /// Kameraraum → Bildschirm (physische Pixel, y nach unten)
    #[inline]
    pub fn project(&self, c: V3) -> (f32, f32) {
        let zc = DIST - c[2];
        (self.w * 0.5 + self.f * c[0] / zc, self.h * 0.5 - self.f * c[1] / zc)
    }

    #[inline]
    fn clip(&self, c: V3) -> [f32; 4] {
        let zc = DIST - c[2];
        let a = (FAR + NEAR) / (FAR - NEAR);
        let b = -2.0 * FAR * NEAR / (FAR - NEAR);
        [self.f * c[0] / (self.w * 0.5), self.f * c[1] / (self.h * 0.5), a * zc + b, zc]
    }

    #[inline]
    fn ray(&self, px: f32, py: f32) -> V3 {
        [(px - self.w * 0.5) / self.f, -(py - self.h * 0.5) / self.f, -1.0]
    }
}

fn layer_visible(vis: &Visibility, layer: Layer, part: skin::Part) -> bool {
    match layer {
        Layer::Base => vis.base[part.idx()],
        Layer::Overlay => vis.overlay[part.idx()],
    }
}

/// Welches Texel liegt unter dem Bildpunkt (`px`, `py`)? Leere Texel zählen als Treffer,
/// wenn `ghosts` an ist (1. Ebene immer, 2. Ebene nur bei ausgeblendeter 1. Ebene).
#[allow(clippy::too_many_arguments)]
pub fn pick(
    skin: &Skin,
    model: Model,
    vis: &Visibility,
    view: &View,
    px: f32,
    py: f32,
    ghosts: bool,
) -> Option<(usize, usize)> {
    let origin: V3 = [0.0, 0.0, DIST];
    let dir = view.ray(px, py);
    let mut best: Option<(f32, (usize, usize))> = None;
    for b in boxes(model) {
        if !layer_visible(vis, b.layer, b.part) {
            continue;
        }
        for face in Face::ALL {
            let (fx, fy, fw, fh) = b.face_rect(face);
            let (p0, e1, e2, n) = b.face_geometry(face);
            let p0 = view.to_cam(p0);
            let e1 = view.rot(e1);
            let e2 = view.rot(e2);
            let n = view.rot(n);
            if b.layer == Layer::Base && dot(n, sub(origin, p0)) <= 0.0 {
                continue;
            }
            let denom = dot(dir, n);
            if denom.abs() < 1e-6 {
                continue;
            }
            let t = dot(sub(p0, origin), n) / denom;
            if t <= 0.0 || best.is_some_and(|(bt, _)| t >= bt) {
                continue;
            }
            let rel = sub(add(origin, scale(dir, t)), p0);
            let s = dot(rel, e1) / dot(e1, e1);
            let v = dot(rel, e2) / dot(e2, e2);
            if !(0.0..1.0).contains(&s) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let tx = fx + ((s * fw as f32) as usize).min(fw - 1);
            let ty = fy + ((v * fh as f32) as usize).min(fh - 1);
            let a = skin.get(tx, ty)[3];
            let ghost_ok = ghosts && (b.layer == Layer::Base || !vis.base[b.part.idx()]);
            if a > 0 || ghost_ok {
                best = Some((t, (tx, ty)));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Bildschirm-Viereck eines Texels (oder `None`, wenn es verdeckt ist / nach hinten zeigt).
pub fn texel_quad(
    skin: &Skin,
    model: Model,
    vis: &Visibility,
    view: &View,
    x: usize,
    y: usize,
    ghosts: bool,
) -> Option<[(f32, f32); 4]> {
    let t = skin::texel_info(model, x, y)?;
    if !layer_visible(vis, t.layer, t.part) {
        return None;
    }
    let b = skin::box_of(model, t.part, t.layer);
    let (p0, e1, e2, _) = b.face_geometry(t.face);
    let (fw, fh) = (t.fw as f32, t.fh as f32);
    let corner = |a: f32, c: f32| view.to_cam(add(p0, add(scale(e1, a / fw), scale(e2, c / fh))));
    let (i, j) = (t.i as f32, t.j as f32);
    let cs = [corner(i, j), corner(i + 1.0, j), corner(i + 1.0, j + 1.0), corner(i, j + 1.0)];
    let center = view.to_cam(add(p0, add(scale(e1, (i + 0.5) / fw), scale(e2, (j + 0.5) / fh))));
    let (cx, cy) = view.project(center);
    // nur zeichnen, wenn an dieser Stelle wirklich dieses Texel zu sehen ist
    if pick(skin, model, vis, view, cx, cy, ghosts) != Some((x, y)) {
        return None;
    }
    Some(cs.map(|c| view.project(c)))
}

// ------------------------------------------------------------------ Vertexdaten

/// Pro Vertex: Clip-Position (4), UV (2), Licht (1), „leere Pixel andeuten“ (1)
const FLOATS: usize = 8;

pub struct Geometry {
    pub data: Vec<f32>,
    /// Anzahl Vertices der 1. Ebene (danach folgt die 2. Ebene)
    pub base_count: i32,
    pub total_count: i32,
}

pub fn build_geometry(model: Model, vis: &Visibility, view: &View, ghosts: bool) -> Geometry {
    let origin: V3 = [0.0, 0.0, DIST];
    let light = {
        let l = [-0.45f32, 0.75, 0.6];
        let n = dot(l, l).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    let mut data = Vec::with_capacity(72 * 6 * FLOATS);
    let mut base_count = 0;
    for layer in [Layer::Base, Layer::Overlay] {
        for b in boxes(model) {
            if b.layer != layer || !layer_visible(vis, b.layer, b.part) {
                continue;
            }
            let ghostable = ghosts && (layer == Layer::Base || !vis.base[b.part.idx()]);
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                let (p0, e1, e2, n) = b.face_geometry(face);
                let c0 = view.to_cam(p0);
                let nr = view.rot(n);
                let facing = dot(nr, sub(origin, c0)) > 0.0;
                if layer == Layer::Base && !facing {
                    continue;
                }
                let mut bright = 0.62 + 0.38 * dot(nr, light).max(0.0);
                if !facing {
                    bright *= 0.6;
                }
                // UVs ganz leicht nach innen, damit nie das Nachbar-Texel gelesen wird
                let eps = 0.0005;
                let (u0, v0) = ((fx as f32 + eps) / SIZE as f32, (fy as f32 + eps) / SIZE as f32);
                let (u1, v1) = (((fx + fw) as f32 - eps) / SIZE as f32, ((fy + fh) as f32 - eps) / SIZE as f32);
                let corners = [
                    (view.clip(view.to_cam(p0)), [u0, v0]),
                    (view.clip(view.to_cam(add(p0, e1))), [u1, v0]),
                    (view.clip(view.to_cam(add(add(p0, e1), e2))), [u1, v1]),
                    (view.clip(view.to_cam(add(p0, e2))), [u0, v1]),
                ];
                for k in [0, 1, 2, 0, 2, 3] {
                    let (c, uv) = corners[k];
                    data.extend_from_slice(&[c[0], c[1], c[2], c[3], uv[0], uv[1], bright, if ghostable { 1.0 } else { 0.0 }]);
                }
            }
        }
        if layer == Layer::Base {
            base_count = (data.len() / FLOATS) as i32;
        }
    }
    let total_count = (data.len() / FLOATS) as i32;
    Geometry { data, base_count, total_count }
}

// ------------------------------------------------------------------ OpenGL

struct GlRes {
    program: glow::Program,
    vbo: glow::Buffer,
    vao: Option<glow::VertexArray>,
    a_pos: u32,
    a_uv: u32,
    a_extra: u32,
    u_tex: Option<glow::UniformLocation>,
    u_mode: Option<glow::UniformLocation>,
    u_grid: Option<glow::UniformLocation>,
}

// glow-Handles sind einfache Zahlen; der Zugriff passiert nur im Render-Thread.
unsafe impl Send for GlRes {}

static GL_RES: Mutex<Option<GlRes>> = Mutex::new(None);

const VERT: &str = r#"
#if NEW_SHADER_INTERFACE
#define I in
#define O out
#else
#define I attribute
#define O varying
#endif
I vec4 a_pos;
I vec2 a_uv;
I vec2 a_extra;
O vec2 v_uv;
O vec2 v_extra;
void main() {
    gl_Position = a_pos;
    v_uv = a_uv;
    v_extra = a_extra;
}
"#;

const FRAG: &str = r#"
#ifdef GL_ES
precision highp float;
#endif
uniform sampler2D u_tex;
uniform float u_mode;  // 0 = 1. Ebene, 1 = 2. Ebene deckend, 2 = 2. Ebene halbtransparent
uniform float u_grid;
#if NEW_SHADER_INTERFACE
#define I in
out vec4 f_color;
#define gl_FragColor f_color
#define texture2D texture
#else
#define I varying
#endif
I vec2 v_uv;
I vec2 v_extra;
void main() {
    vec2 t = v_uv * 64.0;
    // Ableitungen vor allen Verzweigungen berechnen
    vec2 w = max(fwidth(t), vec2(0.0001));
    vec2 cell = floor(t);
    vec4 c = texture2D(u_tex, (cell + 0.5) / 64.0);  // vormultipliziert, Gamma-Raum
    float light = v_extra.x;
    bool ghost = false;
    vec4 col;
    if (c.a < 0.004) {
        if (v_extra.y < 0.5 || u_mode > 1.5) discard;
        ghost = true;
        col = vec4(0.62 * 0.13, 0.62 * 0.13, 0.62 * 0.13, 0.13);
    } else if (u_mode > 1.5) {
        if (c.a > 0.996) discard;
        col = vec4(c.rgb * light, c.a);
    } else if (u_mode > 0.5) {
        if (c.a <= 0.996) discard;
        col = vec4(c.rgb * light, 1.0);
    } else {
        col = vec4(c.rgb / max(c.a, 0.004) * light, 1.0);
    }
    if (u_grid > 0.5) {
        vec2 g = fract(t);
        vec2 d = min(g, 1.0 - g) / w;
        float line = 1.0 - clamp(min(d.x, d.y) - 0.25, 0.0, 1.0);
        // feine, aber deutliche Linien – auf leeren Pixeln hell, sonst dunkel
        if (ghost) {
            col = mix(col, vec4(0.42, 0.42, 0.45, 0.55), line);
        } else {
            col.rgb = mix(col.rgb, col.rgb * 0.5, line * 0.85);
        }
    }
    gl_FragColor = col;
}
"#;

unsafe fn create_res(gl: &glow::Context) -> Result<GlRes, String> {
    unsafe {
        let sv = egui_glow::ShaderVersion::get(gl);
        let header = format!(
            "{}\n#define NEW_SHADER_INTERFACE {}\n",
            sv.version_declaration(),
            if sv.is_new_shader_interface() { 1 } else { 0 }
        );
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, VERT), (glow::FRAGMENT_SHADER, FRAG)] {
            let sh = gl.create_shader(kind)?;
            let src = if kind == glow::FRAGMENT_SHADER && sv.is_embedded() {
                format!("{header}#extension GL_OES_standard_derivatives : enable\n{src}")
            } else {
                format!("{header}{src}")
            };
            gl.shader_source(sh, &src);
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                return Err(gl.get_shader_info_log(sh));
            }
            gl.attach_shader(program, sh);
            shaders.push(sh);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for sh in shaders {
            gl.detach_shader(program, sh);
            gl.delete_shader(sh);
        }
        let attr = |name: &str| gl.get_attrib_location(program, name).ok_or(format!("Attribut {name} fehlt"));
        let res = GlRes {
            a_pos: attr("a_pos")?,
            a_uv: attr("a_uv")?,
            a_extra: attr("a_extra")?,
            u_tex: gl.get_uniform_location(program, "u_tex"),
            u_mode: gl.get_uniform_location(program, "u_mode"),
            u_grid: gl.get_uniform_location(program, "u_grid"),
            vbo: gl.create_buffer()?,
            vao: gl.create_vertex_array().ok(),
            program,
        };
        Ok(res)
    }
}

/// Erzeugt den Zeichenbefehl für egui.
pub fn paint_callback(rect: egui::Rect, geo: Geometry, tex: egui::TextureId, grid: bool) -> egui::PaintCallback {
    let geo = Arc::new(geo);
    let cb = egui_glow::CallbackFn::new(move |_info, painter| {
        let gl = painter.gl();
        let Some(texture) = painter.texture(tex) else { return };
        let mut guard = GL_RES.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            match unsafe { create_res(gl) } {
                Ok(r) => *guard = Some(r),
                Err(e) => {
                    eprintln!("3D-Shader fehlgeschlagen, nutze Software-Darstellung: {e}");
                    GPU_FAILED.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
        let r = guard.as_ref().unwrap();
        unsafe {
            gl.use_program(Some(r.program));
            if let Some(vao) = r.vao {
                gl.bind_vertex_array(Some(vao));
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(r.vbo));
            let bytes: &[u8] =
                std::slice::from_raw_parts(geo.data.as_ptr() as *const u8, geo.data.len() * std::mem::size_of::<f32>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            let stride = (FLOATS * 4) as i32;
            for (loc, size, off) in [(r.a_pos, 4, 0), (r.a_uv, 2, 16), (r.a_extra, 2, 24)] {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, stride, off);
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.uniform_1_i32(r.u_tex.as_ref(), 0);
            gl.uniform_1_f32(r.u_grid.as_ref(), if grid { 1.0 } else { 0.0 });

            // Tiefenpuffer nur innerhalb der Ansicht leeren (Scissor ist gesetzt)
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);

            gl.uniform_1_f32(r.u_mode.as_ref(), 0.0);
            gl.draw_arrays(glow::TRIANGLES, 0, geo.base_count);
            let over = geo.total_count - geo.base_count;
            if over > 0 {
                gl.uniform_1_f32(r.u_mode.as_ref(), 1.0);
                gl.draw_arrays(glow::TRIANGLES, geo.base_count, over);
                gl.depth_mask(false);
                gl.uniform_1_f32(r.u_mode.as_ref(), 2.0);
                gl.draw_arrays(glow::TRIANGLES, geo.base_count, over);
                gl.depth_mask(true);
            }

            gl.disable(glow::DEPTH_TEST);
            for loc in [r.a_pos, r.a_uv, r.a_extra] {
                gl.disable_vertex_attrib_array(loc);
            }
            if r.vao.is_some() {
                gl.bind_vertex_array(None);
            }
        }
    });
    egui::PaintCallback { rect, callback: Arc::new(cb) }
}
