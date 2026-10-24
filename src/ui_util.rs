//! Kleine UI-Helfer, die Editor und Mixer gemeinsam nutzen.

use eframe::egui::{self, Color32, ColorImage, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2};

use crate::skin::{Rgba, SIZE};

pub fn c32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub fn rgba(c: Color32) -> Rgba {
    c.to_srgba_unmultiplied()
}

/// Farbfeld mit Schachbrett hinter transparenten Farben.
pub fn swatch(ui: &mut egui::Ui, c: Rgba, size: Vec2, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let r = 4.0;
    if c[3] < 255 {
        p.rect_filled(rect, r, Color32::from_gray(200));
        let h = rect.height() / 2.0;
        let w = rect.width() / 2.0;
        p.rect_filled(Rect::from_min_size(rect.min, egui::vec2(w, h)), 0.0, Color32::from_gray(150));
        p.rect_filled(Rect::from_min_size(rect.min + egui::vec2(w, h), egui::vec2(w, h)), 0.0, Color32::from_gray(150));
    }
    p.rect_filled(rect, r, c32(c));
    let stroke = if selected {
        Stroke::new(2.0, Color32::WHITE)
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::from_gray(200))
    } else {
        Stroke::new(1.0, Color32::from_black_alpha(120))
    };
    p.rect_stroke(rect, r, stroke, StrokeKind::Inside);
    resp
}

pub fn checker_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::unique("shared_checker");
    if let Some(t) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return t;
    }
    let n = SIZE * 2;
    let mut px = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            px.push(if (x / 2 + y / 2) % 2 == 0 { Color32::from_gray(58) } else { Color32::from_gray(68) });
        }
    }
    let t = ctx.load_texture("shared_checker", ColorImage::new([n, n], px), TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Zeichnet eine Skin-Textur (z. B. als Vorschaubild) mit Schachbrett-Hintergrund.
pub fn paint_skin_thumb(ui: &egui::Ui, rect: Rect, tex: &TextureHandle) {
    let checker = checker_texture(ui.ctx());
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let p = ui.painter();
    p.rect_filled(rect.expand(1.0), 3.0, Color32::from_gray(18));
    p.image(checker.id(), rect, uv, Color32::WHITE);
    p.image(tex.id(), rect, uv, Color32::WHITE);
}

pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).small().strong().weak());
    ui.add_space(2.0);
}

/// Kompakter HSV-Farbwähler: Sättigung/Helligkeit-Feld, Farbton- und Deckkraft-Leiste.
pub fn hsv_picker(ui: &mut egui::Ui, hsv: &mut crate::color::Hsv, alpha: &mut u8, width: f32) -> bool {
    use crate::color::{from_hsv, Hsv};
    let mut changed = false;
    let painter_stroke = Stroke::new(1.0, Color32::from_black_alpha(160));

    // Sättigung (x) / Helligkeit (y)
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, (width * 0.7).round()), Sense::click_and_drag());
    if resp.is_pointer_button_down_on() {
        if let Some(p) = resp.interact_pointer_pos() {
            hsv.s = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            hsv.v = 1.0 - ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            changed = true;
        }
    }
    let n = 14;
    let mut mesh = egui::Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (s, v) = (i as f32 / n as f32, 1.0 - j as f32 / n as f32);
            let c = from_hsv(Hsv { h: hsv.h, s, v }, 255);
            let pos = rect.min + egui::vec2(s * rect.width(), (1.0 - v) * rect.height());
            mesh.colored_vertex(pos, c32(c));
        }
    }
    let w = n + 1;
    for j in 0..n {
        for i in 0..n {
            let a = (j * w + i) as u32;
            mesh.add_triangle(a, a + 1, a + w as u32);
            mesh.add_triangle(a + 1, a + w as u32 + 1, a + w as u32);
        }
    }
    let p = ui.painter();
    p.add(egui::Shape::mesh(mesh));
    p.rect_stroke(rect, 0.0, painter_stroke, StrokeKind::Outside);
    let marker = rect.min + egui::vec2(hsv.s * rect.width(), (1.0 - hsv.v) * rect.height());
    p.circle_stroke(marker, 5.0, Stroke::new(2.0, Color32::BLACK));
    p.circle_stroke(marker, 4.0, Stroke::new(1.5, Color32::WHITE));
    resp.on_hover_cursor(egui::CursorIcon::Crosshair);

    // Farbton
    ui.add_space(2.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 14.0), Sense::click_and_drag());
    if resp.is_pointer_button_down_on() {
        if let Some(pt) = resp.interact_pointer_pos() {
            hsv.h = ((pt.x - rect.left()) / rect.width()).clamp(0.0, 0.9999) * 360.0;
            if hsv.s < 0.05 {
                hsv.s = 1.0;
                hsv.v = hsv.v.max(0.5);
            }
            changed = true;
        }
    }
    let segs = 36;
    let mut mesh = egui::Mesh::default();
    for k in 0..=segs {
        let t = k as f32 / segs as f32;
        let c = c32(from_hsv(Hsv { h: t * 360.0, s: 1.0, v: 1.0 }, 255));
        let x = rect.left() + t * rect.width();
        mesh.colored_vertex(egui::pos2(x, rect.top()), c);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), c);
        if k > 0 {
            let b = (k * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    let p = ui.painter();
    p.add(egui::Shape::mesh(mesh));
    p.rect_stroke(rect, 0.0, painter_stroke, StrokeKind::Outside);
    let x = rect.left() + hsv.h / 360.0 * rect.width();
    let knob = Rect::from_center_size(egui::pos2(x, rect.center().y), egui::vec2(5.0, rect.height() + 4.0));
    p.rect_stroke(knob, 2.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Middle);

    // Deckkraft
    ui.add_space(2.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 14.0), Sense::click_and_drag());
    if resp.is_pointer_button_down_on() {
        if let Some(pt) = resp.interact_pointer_pos() {
            *alpha = (((pt.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 255.0).round() as u8;
            changed = true;
        }
    }
    let p = ui.painter();
    let cells = (rect.width() / 7.0).ceil() as i32;
    for k in 0..cells {
        for row in 0..2 {
            let c = if (k + row) % 2 == 0 { Color32::from_gray(170) } else { Color32::from_gray(110) };
            let r = Rect::from_min_size(rect.min + egui::vec2(k as f32 * 7.0, row as f32 * 7.0), egui::vec2(7.0, 7.0))
                .intersect(rect);
            p.rect_filled(r, 0.0, c);
        }
    }
    let solid = from_hsv(*hsv, 255);
    let mut mesh = egui::Mesh::default();
    let c0 = Color32::from_rgba_unmultiplied(solid[0], solid[1], solid[2], 0);
    let c1 = c32(solid);
    mesh.colored_vertex(rect.left_top(), c0);
    mesh.colored_vertex(rect.left_bottom(), c0);
    mesh.colored_vertex(rect.right_top(), c1);
    mesh.colored_vertex(rect.right_bottom(), c1);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    p.add(egui::Shape::mesh(mesh));
    p.rect_stroke(rect, 0.0, painter_stroke, StrokeKind::Outside);
    let x = rect.left() + *alpha as f32 / 255.0 * rect.width();
    let knob = Rect::from_center_size(egui::pos2(x, rect.center().y), egui::vec2(5.0, rect.height() + 4.0));
    p.rect_stroke(knob, 2.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Middle);
    changed
}

/// Gezeichnete Werkzeug-Symbole (unabhängig von Schriftarten, gestochen scharf).
pub fn paint_tool_icon(p: &egui::Painter, rect: Rect, tool: crate::editor::Tool, col: Color32) {
    use crate::editor::Tool;
    use egui::{pos2, vec2};
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 16.0;
    let at = |x: f32, y: f32| pos2(c.x + (x - 8.0) * s, c.y + (y - 8.0) * s);
    let st = Stroke::new(1.6 * s, col);
    match tool {
        Tool::Pencil => {
            p.line_segment([at(4.0, 12.0), at(12.0, 4.0)], Stroke::new(3.2 * s, col));
            p.add(egui::Shape::convex_polygon(vec![at(2.2, 13.8), at(3.2, 10.6), at(5.4, 12.8)], col, Stroke::NONE));
            p.line_segment([at(11.0, 3.0), at(13.0, 5.0)], Stroke::new(1.0 * s, col.gamma_multiply(0.5)));
        }
        Tool::Eraser => {
            let pts = vec![at(2.5, 10.5), at(8.5, 4.5), at(13.5, 9.5), at(7.5, 15.5) - vec2(0.0, 1.0 * s)];
            p.add(egui::Shape::closed_line(pts, st));
            p.add(egui::Shape::convex_polygon(
                vec![at(2.5, 10.5), at(5.5, 7.5), at(10.5, 12.5), at(7.5, 14.5)],
                col,
                Stroke::NONE,
            ));
        }
        Tool::Noise => {
            let a = [1.0, 0.45, 0.75, 0.6, 1.0, 0.35, 0.8, 0.5, 0.9];
            for k in 0..9 {
                let (x, y) = ((k % 3) as f32, (k / 3) as f32);
                let r = Rect::from_min_size(at(2.5 + x * 4.0, 2.5 + y * 4.0), vec2(3.4 * s, 3.4 * s));
                p.rect_filled(r, 0.5 * s, col.gamma_multiply(a[k]));
            }
        }
        Tool::Shade => {
            p.circle_stroke(at(8.0, 8.0), 5.5 * s, st);
            let mut pts = vec![at(8.0, 2.5)];
            for k in 0..=12 {
                let ang = std::f32::consts::PI * (k as f32 / 12.0) - std::f32::consts::FRAC_PI_2;
                pts.push(pos2(c.x + ang.cos() * 5.5 * s, c.y + ang.sin() * 5.5 * s));
            }
            p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        Tool::Bucket => {
            p.add(egui::Shape::closed_line(vec![at(3.0, 6.0), at(11.0, 6.0), at(10.0, 14.0), at(4.0, 14.0)], st));
            p.add(egui::Shape::convex_polygon(
                vec![at(3.4, 9.0), at(10.6, 9.0), at(10.0, 14.0), at(4.0, 14.0)],
                col,
                Stroke::NONE,
            ));
            p.circle_filled(at(13.2, 11.0), 1.4 * s, col);
            p.line_segment([at(5.0, 6.0), at(7.0, 2.5)], st);
            p.line_segment([at(7.0, 2.5), at(9.0, 6.0)], st);
        }
        Tool::Replace => {
            p.rect_filled(Rect::from_min_size(at(2.0, 2.0), vec2(6.0 * s, 6.0 * s)), 1.0 * s, col.gamma_multiply(0.45));
            p.rect_filled(Rect::from_min_size(at(8.0, 8.0), vec2(6.0 * s, 6.0 * s)), 1.0 * s, col);
            p.line_segment([at(10.0, 3.0), at(13.0, 6.0)], st);
            p.line_segment([at(13.0, 6.0), at(13.0, 3.5)], st);
            p.line_segment([at(13.0, 6.0), at(10.5, 6.0)], st);
        }
        Tool::Line => {
            p.line_segment([at(3.0, 13.0), at(13.0, 3.0)], Stroke::new(2.0 * s, col));
            p.circle_filled(at(3.0, 13.0), 1.8 * s, col);
            p.circle_filled(at(13.0, 3.0), 1.8 * s, col);
        }
        Tool::Rect => {
            p.rect_stroke(Rect::from_min_max(at(2.5, 4.0), at(13.5, 12.5)), 1.0 * s, Stroke::new(2.0 * s, col), StrokeKind::Middle);
        }
        Tool::Picker => {
            p.line_segment([at(3.0, 13.0), at(9.5, 6.5)], Stroke::new(2.4 * s, col));
            p.circle_filled(at(11.5, 4.5), 2.8 * s, col);
            p.line_segment([at(7.5, 5.0), at(11.0, 8.5)], Stroke::new(1.8 * s, col));
        }
    }
}
