//! 3D-Vorschau-Widget: drehen, zoomen und (optional) direkt auf dem Modell malen.

use eframe::egui::{self, Color32, ColorImage, PointerButton, Rect, Sense, TextureHandle, TextureOptions, Vec2};

use crate::render3d::{render, Camera, RenderParams, Visibility};
use crate::skin::{Model, Skin, SIZE};

#[derive(Default)]
pub struct Preview {
    pub cam: Camera,
    tex: Option<TextureHandle>,
    last_params: Option<RenderParams>,
    last_skin: Option<Skin>,
    pick: Vec<u16>,
    pick_size: (usize, usize),
    painting: bool,
    rotating: bool,
    last_pointer: Option<egui::Pos2>,
    pub antialias: bool,
}

pub struct PreviewInput {
    /// Texel unter dem Mauszeiger
    pub hover: Option<(usize, usize)>,
    /// Malstrich aktiv (linke Maustaste auf dem Modell gedrückt)
    pub stroke_started: bool,
    pub stroke_ended: bool,
    /// Alle Texel entlang der Mausbewegung seit dem letzten Bild (lückenlose Striche)
    pub texels: Vec<(usize, usize)>,
    pub response: egui::Response,
}

impl Preview {
    pub fn new() -> Self {
        Preview { antialias: true, ..Default::default() }
    }

    pub fn set_view(&mut self, yaw: f32, pitch: f32) {
        self.cam.yaw = yaw;
        self.cam.pitch = pitch;
    }

    fn pick_at(&self, rect: Rect, pos: egui::Pos2, ppp: f32) -> Option<(usize, usize)> {
        if !rect.contains(pos) || self.pick.is_empty() {
            return None;
        }
        let x = ((pos.x - rect.min.x) * ppp) as usize;
        let y = ((pos.y - rect.min.y) * ppp) as usize;
        if x >= self.pick_size.0 || y >= self.pick_size.1 {
            return None;
        }
        let id = self.pick[y * self.pick_size.0 + x];
        if id == 0 {
            None
        } else {
            let t = (id - 1) as usize;
            Some((t % SIZE, t / SIZE))
        }
    }

    /// Zeigt die Vorschau. `highlight` sind Texel, die hervorgehoben werden.
    /// `paint_enabled`: linke Maustaste malt auf dem Modell statt zu drehen.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        size: Vec2,
        skin: &Skin,
        model: Model,
        vis: Visibility,
        highlight: &[(usize, usize)],
        paint_enabled: bool,
    ) -> PreviewInput {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let mut stroke_started = false;
        let mut stroke_ended = false;

        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        let hover = pointer.and_then(|p| self.pick_at(rect, p, ppp));

        // Eingabe
        if response.drag_started_by(PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).and_then(|p| self.pick_at(rect, p, ppp));
            if paint_enabled && start.is_some() {
                self.painting = true;
                stroke_started = true;
            } else {
                self.rotating = true;
            }
        }
        if response.drag_started_by(PointerButton::Secondary) || response.drag_started_by(PointerButton::Middle) {
            self.rotating = true;
        }
        if paint_enabled && response.clicked() && hover.is_some() && !self.painting {
            // Einzelner Klick: kurzer Strich
            stroke_started = true;
            stroke_ended = true;
        }
        if self.rotating && response.dragged() {
            let d = response.drag_delta();
            self.cam.yaw += d.x * 0.012;
            self.cam.pitch = (self.cam.pitch + d.y * 0.012).clamp(-1.45, 1.45);
        }
        if response.drag_stopped() {
            if self.painting {
                stroke_ended = true;
            }
            self.painting = false;
            self.rotating = false;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.cam.zoom = (self.cam.zoom * (1.0 + scroll * 0.0015)).clamp(0.4, 4.0);
            }
        }
        if response.double_clicked() && hover.is_none() {
            self.cam = Camera::default();
        }

        // Texel entlang des Mauswegs sammeln (gegen Lücken bei schnellen Bewegungen)
        let mut texels = Vec::new();
        if stroke_started || self.painting {
            if let Some(cur) = pointer {
                let from = if stroke_started { cur } else { self.last_pointer.unwrap_or(cur) };
                let steps = ((cur - from).length() * ppp).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let p = from + (cur - from) * (k as f32 / steps as f32);
                    if let Some(t) = self.pick_at(rect, p, ppp) {
                        if texels.last() != Some(&t) && !texels.contains(&t) {
                            texels.push(t);
                        }
                    }
                }
            }
        }
        self.last_pointer = pointer;

        // Rendern nur bei Änderungen
        let rotating_now = self.rotating && response.dragged();
        let ss = if self.antialias && !rotating_now { 2 } else { 1 };
        let mut hl: Vec<u16> = highlight.iter().map(|&(x, y)| (y * SIZE + x) as u16 + 1).collect();
        hl.sort_unstable();
        let params = RenderParams {
            cam: self.cam,
            model,
            vis,
            highlight: hl,
            width: (rect.width() * ppp).round().max(1.0) as usize,
            height: (rect.height() * ppp).round().max(1.0) as usize,
            supersample: ss,
        };
        let changed = self.last_params.as_ref() != Some(&params) || self.last_skin.as_ref() != Some(skin);
        if changed {
            let out = render(skin, &params);
            let img = ColorImage::from_rgba_premultiplied([out.width, out.height], &out.rgba);
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None => self.tex = Some(ui.ctx().load_texture("preview3d", img, TextureOptions::NEAREST)),
            }
            self.pick = out.pick;
            self.pick_size = (out.width, out.height);
            self.last_params = Some(params);
            self.last_skin = Some(skin.clone());
        }

        let painter = ui.painter_at(rect);
        // weicher Bodenschatten
        let ground = rect.center_bottom() - egui::vec2(0.0, rect.height() * 0.08);
        for k in 0..6 {
            let r = 1.0 - k as f32 * 0.15;
            painter.add(egui::Shape::ellipse_filled(
                ground,
                egui::vec2(rect.width().min(rect.height() * 0.6) * 0.32 * r, 9.0 * r),
                Color32::from_black_alpha(18),
            ));
        }
        if let Some(t) = &self.tex {
            painter.image(t.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }

        let cursor = if self.rotating {
            egui::CursorIcon::Grabbing
        } else if paint_enabled && hover.is_some() {
            egui::CursorIcon::Crosshair
        } else {
            egui::CursorIcon::Grab
        };
        let response = response.on_hover_cursor(cursor);

        PreviewInput { hover, stroke_started, stroke_ended, texels, response }
    }

    /// Rendert ein großes Vorschaubild (z. B. zum Exportieren).
    pub fn render_image(&self, skin: &Skin, model: Model, vis: Visibility, w: usize, h: usize) -> Vec<u8> {
        let params = RenderParams { cam: self.cam, model, vis, highlight: vec![], width: w, height: h, supersample: 3 };
        let out = render(skin, &params);
        // vormultipliziert -> normal
        let mut rgba = out.rgba;
        for px in rgba.chunks_exact_mut(4) {
            let a = px[3] as f32 / 255.0;
            if a > 0.0 {
                for c in &mut px[..3] {
                    *c = (*c as f32 / a).round().min(255.0) as u8;
                }
            }
        }
        rgba
    }
}
