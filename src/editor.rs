//! Der Pixel-Editor: Werkzeuge, Verlauf, 2D-Zeichenfläche und Farbwahl.

use crate::i18n::t;
use std::collections::VecDeque;

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureHandle, TextureOptions, Vec2,
};

use crate::color::{self, Rng};
use crate::preview::Preview;
use crate::render3d::Visibility;
use crate::skin::{self, Face, Layer, Model, Part, Rgba, Skin, CLEAR, SIZE};
use crate::ui_util::{c32, swatch};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pencil,
    Eraser,
    Noise,
    Shade,
    Bucket,
    Replace,
    Line,
    Rect,
    Picker,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Pencil,
        Tool::Eraser,
        Tool::Noise,
        Tool::Shade,
        Tool::Bucket,
        Tool::Replace,
        Tool::Line,
        Tool::Rect,
        Tool::Picker,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pencil => t("Stift"),
            Tool::Eraser => t("Radierer"),
            Tool::Noise => t("Rauschpinsel"),
            Tool::Shade => t("Licht/Schatten"),
            Tool::Bucket => t("Füllen"),
            Tool::Replace => t("Farbe ersetzen"),
            Tool::Line => t("Linie"),
            Tool::Rect => t("Rechteck"),
            Tool::Picker => t("Pipette"),
        }
    }
    pub fn key(self) -> Key {
        match self {
            Tool::Pencil => Key::B,
            Tool::Eraser => Key::E,
            Tool::Noise => Key::N,
            Tool::Shade => Key::H,
            Tool::Bucket => Key::G,
            Tool::Replace => Key::R,
            Tool::Line => Key::L,
            Tool::Rect => Key::U,
            Tool::Picker => Key::I,
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pencil => t("Malt mit der Hauptfarbe (Rechtsklick: Zweitfarbe)."),
            Tool::Eraser => t("Macht Pixel durchsichtig."),
            Tool::Noise => t("Malt die Farbe in zufälligen Helligkeiten – für lebendige Flächen."),
            Tool::Shade => t("Links aufhellen, rechts abdunkeln. Ideal für Falten und Schatten."),
            Tool::Bucket => t("Füllt zusammenhängende, ähnliche Pixel."),
            Tool::Replace => t("Erst die alte Farbe im Skin anklicken, dann die neue Farbe – überall wird getauscht."),
            Tool::Line => t("Auf dem Modell oder in der 2D-Textur von Anfang bis Ende ziehen. Die Linie bleibt auf der Fläche, auf der du anfängst."),
            Tool::Rect => t("Auf dem Modell oder in der 2D-Textur von einer Ecke zur anderen ziehen. Das Rechteck bleibt auf der Fläche, auf der du anfängst."),
            Tool::Picker => t("Nimmt eine Farbe auf. Tipp: Alt + Klick geht mit jedem Werkzeug."),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillLimit {
    Face,
    Part,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AdjustTarget {
    All,
    Part(Part),
    Layer(Layer),
}

struct AdjustState {
    original: Skin,
    target: AdjustTarget,
    hue: f32,
    sat: f32,
    bright: f32,
}

struct StrokeState {
    painted: Vec<bool>,
    last: Option<(i32, i32)>,
    start: (i32, i32),
    end: (i32, i32),
    secondary: bool,
    before: Skin,
    in3d: bool,
    tool: Tool,
}

pub struct Editor {
    pub skin: Skin,
    pub model: Model,
    undo: VecDeque<(Skin, Model)>,
    redo: Vec<(Skin, Model)>,

    pub tool: Tool,
    pub size: i32,
    pub round_brush: bool,
    pub noise_amount: f32,
    pub noise_levels: u32,
    pub noise_hue: f32,
    pub shade_amount: f32,
    pub hue_shift: bool,
    pub tolerance: f32,
    pub fill_limit: FillLimit,
    pub fill_noise: bool,
    /// Ersetzen: zuerst angeklickte (alte) Farbe
    replace_from: Option<Rgba>,
    pub rect_fill: bool,
    pub mirror: bool,

    pub primary: Rgba,
    pub secondary: Rgba,
    edit_secondary: bool,
    pub recent: Vec<Rgba>,
    hex: String,
    hex_for: Rgba,
    picker_hsv: color::Hsv,
    picker_for: Option<Rgba>,

    zoom: f32,
    pan: Vec2,
    pub show_grid: bool,
    pub show_regions: bool,
    pub vis: Visibility,
    pub layer_on: [bool; 2],
    pub part_on: [bool; 6],
    /// 2D-Texturansicht ausgeklappt?
    pub show_2d: bool,
    /// „Skin importieren“ wurde geklickt (der Dateidialog läuft im Hauptprogramm)
    pub import_requested: bool,
    pub preview: Preview,

    stroke: Option<StrokeState>,
    hover2d: Option<(usize, usize)>,
    hover3d: Option<(usize, usize)>,
    panning: bool,

    tex_skin: Option<TextureHandle>,
    tex_skin_src: Option<Skin>,
    tex_checker: Option<TextureHandle>,
    tex_unused: Option<(Model, TextureHandle)>,

    pub modified: bool,
    rng: Rng,
    adjust: Option<AdjustState>,
}

const PALETTE: [Rgba; 32] = [
    [0, 0, 0, 255],
    [40, 40, 46, 255],
    [85, 85, 92, 255],
    [140, 140, 148, 255],
    [195, 195, 200, 255],
    [255, 255, 255, 255],
    [255, 224, 189, 255],
    [234, 192, 134, 255],
    [214, 164, 130, 255],
    [176, 120, 82, 255],
    [125, 82, 52, 255],
    [80, 50, 32, 255],
    [178, 34, 34, 255],
    [230, 60, 60, 255],
    [255, 130, 60, 255],
    [255, 196, 50, 255],
    [255, 238, 110, 255],
    [150, 210, 70, 255],
    [60, 160, 70, 255],
    [30, 100, 60, 255],
    [40, 190, 170, 255],
    [60, 170, 230, 255],
    [30, 90, 170, 255],
    [25, 50, 110, 255],
    [110, 80, 200, 255],
    [170, 90, 220, 255],
    [230, 110, 190, 255],
    [255, 170, 200, 255],
    [120, 70, 40, 255],
    [150, 110, 70, 255],
    [200, 170, 120, 255],
    [70, 60, 50, 255],
];

impl Editor {
    pub fn new() -> Self {
        let mut preview = Preview::new();
        preview.antialias = true;
        preview.ghosts = true;
        preview.grid = true;
        preview.no_shadow = true;
        Editor {
            skin: skin::starter_skin(),
            model: Model::Classic,
            undo: VecDeque::new(),
            redo: Vec::new(),
            tool: Tool::Pencil,
            size: 1,
            round_brush: false,
            noise_amount: 0.14,
            noise_levels: 3,
            noise_hue: 0.0,
            shade_amount: 0.12,
            hue_shift: true,
            tolerance: 0.0,
            fill_limit: FillLimit::Face,
            fill_noise: false,
            replace_from: None,
            rect_fill: false,
            mirror: false,
            primary: [30, 90, 170, 255],
            secondary: [255, 255, 255, 255],
            edit_secondary: false,
            recent: Vec::new(),
            hex: String::new(),
            hex_for: CLEAR,
            picker_hsv: color::to_hsv([30, 90, 170, 255]),
            picker_for: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            show_grid: true,
            show_regions: true,
            vis: Visibility::default(),
            layer_on: [true; 2],
            part_on: [true; 6],
            show_2d: false,
            import_requested: false,
            preview,
            stroke: None,
            hover2d: None,
            hover3d: None,
            panning: false,
            tex_skin: None,
            tex_skin_src: None,
            tex_checker: None,
            tex_unused: None,
            modified: false,
            rng: Rng::from_time(),
            adjust: None,
        }
    }

    /// Neuer, leerer Editor mit denselben Werkzeug- und Farbeinstellungen (für neue Tabs).
    pub fn fresh_from(&self) -> Editor {
        let mut e = Editor::new();
        e.tool = self.tool;
        e.size = self.size;
        e.round_brush = self.round_brush;
        e.noise_amount = self.noise_amount;
        e.noise_levels = self.noise_levels;
        e.noise_hue = self.noise_hue;
        e.shade_amount = self.shade_amount;
        e.hue_shift = self.hue_shift;
        e.tolerance = self.tolerance;
        e.fill_limit = self.fill_limit;
        e.fill_noise = self.fill_noise;
        e.rect_fill = self.rect_fill;
        e.mirror = self.mirror;
        e.primary = self.primary;
        e.secondary = self.secondary;
        e.recent = self.recent.clone();
        e.show_grid = self.show_grid;
        e.show_regions = self.show_regions;
        e.show_2d = self.show_2d;
        e.preview.grid = self.preview.grid;
        e.preview.antialias = self.preview.antialias;
        e
    }

    // ------------------------------------------------------------------ Verlauf

    pub fn push_undo(&mut self, before: Skin, model: Model) {
        self.undo.push_back((before, model));
        if self.undo.len() > 300 {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.modified = true;
    }

    /// Ersetzt den Skin als ein rückgängig machbarer Schritt.
    pub fn replace_skin(&mut self, skin: Skin, model: Model) {
        if skin == self.skin && model == self.model {
            return;
        }
        let before = std::mem::replace(&mut self.skin, skin);
        let bm = self.model;
        self.model = model;
        self.push_undo(before, bm);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some((s, m)) = self.undo.pop_back() {
            let cur = std::mem::replace(&mut self.skin, s);
            self.redo.push((cur, self.model));
            self.model = m;
            self.modified = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some((s, m)) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.skin, s);
            self.undo.push_back((cur, self.model));
            self.model = m;
            self.modified = true;
        }
    }

    /// Öffnet einen Skin als neues Projekt (frischer Verlauf, nichts geändert).
    pub fn open_fresh(&mut self, skin: Skin, model: Model) {
        self.skin = skin;
        self.model = model;
        self.undo.clear();
        self.redo.clear();
        self.stroke = None;
        self.adjust = None;
        self.modified = false;
        self.part_on = [true; 6];
        self.layer_on = [true; 2];
        self.sync_vis();
    }

    // ------------------------------------------------------------------ Bearbeiten-Funktionen

    pub fn mirror_all(&mut self) {
        let mut out = self.skin.clone();
        for y in 0..SIZE {
            for x in 0..SIZE {
                if let Some((mx, my)) = skin::mirror_texel(self.model, x, y) {
                    out.set(mx, my, self.skin.get(x, y));
                }
            }
        }
        self.replace_skin(out, self.model);
    }

    pub fn clear_layer(&mut self, layer: Layer) {
        let mut out = self.skin.clone();
        for p in Part::ALL {
            fill_box(&mut out, self.model, p, layer, |_| CLEAR);
        }
        self.replace_skin(out, self.model);
    }

    /// 2. Ebene auf die 1. Ebene herunterrechnen (danach ist die 2. Ebene leer).
    pub fn merge_overlay(&mut self) {
        let mut out = self.skin.clone();
        let map = skin::texel_map(self.model);
        for (idx, t) in map.iter().enumerate() {
            let Some(t) = t else { continue };
            if t.layer != Layer::Overlay {
                continue;
            }
            let o = self.skin.px[idx];
            let (bx, by) = skin::texel_at(self.model, t.part, Layer::Base, t.face, t.i, t.j);
            let b = out.get(bx, by);
            out.set(bx, by, blend(b, o));
            out.px[idx] = CLEAR;
        }
        self.replace_skin(out, self.model);
    }

    pub fn copy_base_to_overlay(&mut self) {
        let mut out = self.skin.clone();
        for p in Part::ALL {
            let b = skin::box_of(self.model, p, Layer::Base);
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                for j in 0..fh {
                    for i in 0..fw {
                        let (ox, oy) = skin::texel_at(self.model, p, Layer::Overlay, face, i, j);
                        out.set(ox, oy, self.skin.get(fx + i, fy + j));
                    }
                }
            }
        }
        self.replace_skin(out, self.model);
    }

    pub fn grayscale(&mut self) {
        let mut out = self.skin.clone();
        for c in &mut out.px {
            let l = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32).round() as u8;
            *c = [l, l, l, c[3]];
        }
        self.replace_skin(out, self.model);
    }

    pub fn convert_arms(&mut self, to: Model) {
        let out = skin::convert_model(&self.skin, self.model, to);
        self.replace_skin(out, to);
    }

    pub fn start_adjust(&mut self) {
        if self.adjust.is_none() {
            self.adjust = Some(AdjustState {
                original: self.skin.clone(),
                target: AdjustTarget::All,
                hue: 0.0,
                sat: 0.0,
                bright: 0.0,
            });
        }
    }

    // ------------------------------------------------------------------ Malen

    fn brush_offsets(&self) -> Vec<(i32, i32)> {
        let n = self.size.max(1);
        let lo = -(n - 1) / 2;
        let hi = n / 2;
        let r = n as f32 / 2.0;
        let mut v = Vec::new();
        for dy in lo..=hi {
            for dx in lo..=hi {
                if self.round_brush && n >= 3 {
                    let cx = dx as f32 - (lo + hi) as f32 / 2.0;
                    let cy = dy as f32 - (lo + hi) as f32 / 2.0;
                    if cx * cx + cy * cy > r * r {
                        continue;
                    }
                }
                v.push((dx, dy));
            }
        }
        v
    }

    fn active_color(&self, secondary: bool) -> Rgba {
        if secondary { self.secondary } else { self.primary }
    }

    fn remember_color(&mut self, c: Rgba) {
        if c[3] == 0 {
            return;
        }
        self.recent.retain(|&r| r != c);
        self.recent.insert(0, c);
        self.recent.truncate(16);
    }

    fn begin_stroke(&mut self, x: i32, y: i32, secondary: bool, in3d: bool) {
        let tool = self.tool;
        self.stroke = Some(StrokeState {
            painted: vec![false; SIZE * SIZE],
            last: None,
            start: (x, y),
            end: (x, y),
            secondary,
            before: self.skin.clone(),
            in3d,
            tool,
        });
        match tool {
            Tool::Pencil | Tool::Noise | Tool::Bucket | Tool::Replace | Tool::Line | Tool::Rect => {
                let c = self.active_color(secondary);
                self.remember_color(c);
            }
            _ => {}
        }
        match tool {
            Tool::Bucket => {
                self.flood_fill(x, y, secondary);
                if self.mirror {
                    if let Some((mx, my)) = skin::mirror_texel(self.model, x as usize, y as usize) {
                        self.flood_fill(mx as i32, my as i32, secondary);
                    }
                }
            }
            Tool::Replace => self.replace_color(x, y, secondary),
            Tool::Picker => self.pick_color(x, y, secondary),
            Tool::Line | Tool::Rect => {}
            _ => self.stroke_to(x, y),
        }
    }

    fn stroke_to(&mut self, x: i32, y: i32) {
        let model = self.model;
        let Some(st) = &mut self.stroke else { return };
        st.end = (x, y);
        // Im 3D-Modell bleiben Linie und Rechteck auf der Fläche, auf der sie anfangen
        if st.in3d && matches!(st.tool, Tool::Line | Tool::Rect) {
            if let Some(ti) = skin::texel_info(model, st.start.0 as usize, st.start.1 as usize) {
                let (fx, fy) = (st.start.0 - ti.i as i32, st.start.1 - ti.j as i32);
                st.end = (x.clamp(fx, fx + ti.fw as i32 - 1), y.clamp(fy, fy + ti.fh as i32 - 1));
            }
        }
        match st.tool {
            Tool::Pencil | Tool::Eraser | Tool::Noise | Tool::Shade => {}
            Tool::Picker => {
                let sec = st.secondary;
                self.pick_color(x, y, sec);
                return;
            }
            _ => return,
        }
        let points = match (st.last, st.in3d) {
            (Some(l), false) => line_points(l, (x, y)),
            _ => vec![(x, y)],
        };
        st.last = Some((x, y));
        for (px, py) in points {
            self.dab(px, py);
        }
    }

    fn stroke_to_clamped(&mut self, (x, y): (i32, i32)) {
        if let Some(st) = &self.stroke {
            if st.last == Some((x, y)) && !matches!(st.tool, Tool::Line | Tool::Rect) {
                return;
            }
        }
        self.stroke_to(x, y);
    }

    fn dab(&mut self, x: i32, y: i32) {
        for (dx, dy) in self.brush_offsets() {
            let (tx, ty) = (x + dx, y + dy);
            if tx < 0 || ty < 0 || tx >= SIZE as i32 || ty >= SIZE as i32 {
                continue;
            }
            let c = self.paint_texel(tx as usize, ty as usize, None);
            if self.mirror {
                if let Some((mx, my)) = skin::mirror_texel(self.model, tx as usize, ty as usize) {
                    self.paint_texel(mx, my, c);
                }
            }
        }
    }

    /// Malt ein Texel mit dem aktuellen Werkzeug. `forced` = Farbe vom Spiegel-Partner übernehmen.
    fn paint_texel(&mut self, x: usize, y: usize, forced: Option<Rgba>) -> Option<Rgba> {
        if !self.texel_visible(x, y) {
            return None;
        }
        let st = self.stroke.as_mut()?;
        let idx = y * SIZE + x;
        if st.painted[idx] {
            return None;
        }
        st.painted[idx] = true;
        let secondary = st.secondary;
        let tool = st.tool;
        let cur = self.skin.get(x, y);
        let c = match tool {
            Tool::Eraser => CLEAR,
            Tool::Shade => {
                if cur[3] == 0 {
                    return None;
                }
                let amt = if secondary { -self.shade_amount } else { self.shade_amount };
                color::shift_light(cur, amt, self.hue_shift)
            }
            Tool::Noise => match forced {
                Some(f) => f,
                None => color::noise_color(
                    self.active_color(secondary),
                    &mut self.rng,
                    self.noise_amount,
                    self.noise_levels,
                    self.noise_hue,
                ),
            },
            _ => forced.unwrap_or(self.active_color(secondary)),
        };
        self.skin.set(x, y, c);
        Some(c)
    }

    fn fill_color(&mut self, secondary: bool) -> Rgba {
        let base = self.active_color(secondary);
        if self.fill_noise {
            color::noise_color(base, &mut self.rng, self.noise_amount, self.noise_levels, self.noise_hue)
        } else {
            base
        }
    }

    fn flood_fill(&mut self, x: i32, y: i32, secondary: bool) {
        let (x, y) = (x as usize, y as usize);
        let Some(seed_info) = skin::texel_info(self.model, x, y) else { return };
        if !self.texel_visible(x, y) {
            return;
        }
        let target = self.skin.get(x, y);
        let mut seen = vec![false; SIZE * SIZE];
        let mut stack = vec![(x, y)];
        let mut region = Vec::new();
        seen[y * SIZE + x] = true;
        while let Some((cx, cy)) = stack.pop() {
            region.push((cx, cy));
            let neighbors = [(cx.wrapping_sub(1), cy), (cx + 1, cy), (cx, cy.wrapping_sub(1)), (cx, cy + 1)];
            for (nx, ny) in neighbors {
                if nx >= SIZE || ny >= SIZE || seen[ny * SIZE + nx] {
                    continue;
                }
                let Some(t) = skin::texel_info(self.model, nx, ny) else { continue };
                if !self.texel_visible(nx, ny) {
                    continue;
                }
                let ok_region = match self.fill_limit {
                    FillLimit::Face => t.part == seed_info.part && t.layer == seed_info.layer && t.face == seed_info.face,
                    FillLimit::Part => t.part == seed_info.part && t.layer == seed_info.layer,
                    FillLimit::All => true,
                };
                if ok_region && color::similar(self.skin.get(nx, ny), target, self.tolerance) {
                    seen[ny * SIZE + nx] = true;
                    stack.push((nx, ny));
                }
            }
        }
        for (rx, ry) in region {
            let c = self.fill_color(secondary);
            self.skin.set(rx, ry, c);
        }
    }

    /// Ersetzen in zwei Schritten: 1. Klick = alte Farbe, 2. Klick = neue Farbe.
    fn replace_color(&mut self, x: i32, y: i32, _secondary: bool) {
        if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
            return;
        }
        let clicked = self.skin.get(x as usize, y as usize);
        match self.replace_from {
            None => self.replace_from = Some(clicked),
            Some(from) => {
                if !color::similar(clicked, from, 0.0) {
                    self.replace_all(from, clicked);
                }
                self.replace_from = None;
            }
        }
    }

    /// Alle (ähnlichen) Pixel der Farbe `from` durch `to` ersetzen.
    fn replace_all(&mut self, from: Rgba, to: Rgba) {
        let map = skin::texel_map(self.model);
        for idx in 0..SIZE * SIZE {
            if map[idx].is_some()
                && self.texel_visible(idx % SIZE, idx / SIZE)
                && color::similar(self.skin.px[idx], from, self.tolerance)
            {
                let c = if self.fill_noise {
                    color::noise_color(to, &mut self.rng, self.noise_amount, self.noise_levels, self.noise_hue)
                } else {
                    to
                };
                self.skin.px[idx] = c;
            }
        }
    }

    /// Pixel, die beim Ersetzen getauscht würden (zum Hervorheben).
    pub fn replace_matches(&self) -> Vec<(usize, usize)> {
        let Some(from) = self.replace_from else { return vec![] };
        let map = skin::texel_map(self.model);
        (0..SIZE * SIZE)
            .filter(|&idx| {
                map[idx].is_some() && self.texel_visible(idx % SIZE, idx / SIZE) && color::similar(self.skin.px[idx], from, self.tolerance)
            })
            .map(|idx| (idx % SIZE, idx / SIZE))
            .take(700)
            .collect()
    }

    fn pick_color(&mut self, x: i32, y: i32, secondary: bool) {
        if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
            return;
        }
        let c = self.skin.get(x as usize, y as usize);
        if secondary {
            self.secondary = c;
        } else {
            self.primary = c;
        }
    }

    fn shape_points(&self, st: &StrokeState) -> Vec<(i32, i32)> {
        match st.tool {
            Tool::Line => line_points(st.start, st.end),
            Tool::Rect => {
                let (x0, x1) = (st.start.0.min(st.end.0), st.start.0.max(st.end.0));
                let (y0, y1) = (st.start.1.min(st.end.1), st.start.1.max(st.end.1));
                let mut v = Vec::new();
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if self.rect_fill || x == x0 || x == x1 || y == y0 || y == y1 {
                            v.push((x, y));
                        }
                    }
                }
                v
            }
            _ => vec![],
        }
    }

    fn end_stroke(&mut self) {
        let Some(st) = self.stroke.take() else { return };
        // Nach dem Farbe-Aufnehmen direkt weiter mit dem Stift malen
        if st.tool == Tool::Picker && self.tool == Tool::Picker {
            self.tool = Tool::Pencil;
        }
        if matches!(st.tool, Tool::Line | Tool::Rect) {
            let pts = self.shape_points(&st);
            let secondary = st.secondary;
            let shape_tool = st.tool;
            let before = st.before.clone();
            self.stroke = Some(StrokeState { tool: Tool::Pencil, ..st });
            for (x, y) in pts {
                let offsets = if shape_tool == Tool::Line { self.brush_offsets() } else { vec![(0, 0)] };
                for (dx, dy) in offsets {
                    let (tx, ty) = (x + dx, y + dy);
                    if tx < 0 || ty < 0 || tx >= SIZE as i32 || ty >= SIZE as i32 {
                        continue;
                    }
                    let c = self.fill_color(secondary);
                    let used = self.paint_texel(tx as usize, ty as usize, Some(c));
                    if self.mirror {
                        if let Some((mx, my)) = skin::mirror_texel(self.model, tx as usize, ty as usize) {
                            self.paint_texel(mx, my, used.or(Some(c)));
                        }
                    }
                }
            }
            self.stroke = None;
            if self.skin != before {
                self.push_undo(before, self.model);
            }
            return;
        }
        if self.skin != st.before {
            self.push_undo(st.before, self.model);
        }
    }

    // ------------------------------------------------------------------ Tastatur

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, redo2) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
                i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
            )
        });
        if redo || redo2 {
            self.redo();
        } else if undo {
            self.undo();
        }
        ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return;
            }
            for t in Tool::ALL {
                if i.key_pressed(t.key()) {
                    self.tool = t;
                }
            }
            if i.key_pressed(Key::F) {
                self.tool = Tool::Bucket;
            }
            if i.key_pressed(Key::M) {
                self.mirror = !self.mirror;
            }
            if i.key_pressed(Key::X) {
                std::mem::swap(&mut self.primary, &mut self.secondary);
            }
            if i.key_pressed(Key::OpenBracket) || i.key_pressed(Key::Minus) {
                self.size = (self.size - 1).max(1);
            }
            if i.key_pressed(Key::CloseBracket) || i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                self.size = (self.size + 1).min(16);
            }
            if i.key_pressed(Key::Num0) {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
        });
    }

    // ------------------------------------------------------------------ Oberfläche

    pub fn ui_tools(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new(t("WERKZEUGE")).small().strong().weak());
        ui.add_space(2.0);
        egui::Grid::new("tools").num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
            for (k, t) in Tool::ALL.into_iter().enumerate() {
                let selected = self.tool == t;
                let text = RichText::new(format!("       {}", t.name())).size(13.0);
                let btn = egui::Button::selectable(selected, text)
                    .min_size(egui::vec2(119.0, 30.0))
                    .wrap_mode(egui::TextWrapMode::Extend);
                let r = ui.add(btn).on_hover_text(format!("{}\n{}: {:?}", t.hint(), crate::i18n::t("Taste"), t.key()));
                let icon_rect = Rect::from_center_size(r.rect.left_center() + egui::vec2(17.0, 0.0), Vec2::splat(16.0));
                let col = if selected { Color32::WHITE } else { ui.visuals().text_color() };
                crate::ui_util::paint_tool_icon(ui.painter(), icon_rect, t, col);
                if r.clicked() {
                    self.tool = t;
                }
                if k % 2 == 1 {
                    ui.end_row();
                }
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(t("OPTIONEN")).small().strong().weak());
        ui.label(RichText::new(self.tool.hint()).small().weak());
        ui.add_space(4.0);

        let brush = matches!(self.tool, Tool::Pencil | Tool::Eraser | Tool::Noise | Tool::Shade | Tool::Line);
        if brush {
            ui.horizontal(|ui| {
                ui.label(t("Größe"));
                ui.add(egui::Slider::new(&mut self.size, 1..=16).suffix(t(" px")));
            });
            if self.tool != Tool::Line {
                ui.checkbox(&mut self.round_brush, t("Runde Pinselspitze"));
            }
        }
        match self.tool {
            Tool::Noise => self.noise_options(ui),
            Tool::Shade => {
                ui.horizontal(|ui| {
                    ui.label(t("Stärke"));
                    ui.add(egui::Slider::new(&mut self.shade_amount, 0.02..=0.5).show_value(false));
                });
                ui.checkbox(&mut self.hue_shift, t("Farbton mitschieben"))
                    .on_hover_text(t("Lichter werden etwas wärmer, Schatten etwas kühler – wirkt natürlicher."));
            }
            Tool::Bucket | Tool::Replace => {
                if self.tool == Tool::Replace {
                    self.replace_options(ui);
                }
                ui.horizontal(|ui| {
                    ui.label(t("Toleranz"));
                    ui.add(egui::Slider::new(&mut self.tolerance, 0.0..=100.0).suffix(" %"));
                });
                if self.tool == Tool::Bucket {
                    ui.label(t("Füllen begrenzen auf:"));
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.fill_limit, FillLimit::Face, t("Fläche"));
                        ui.selectable_value(&mut self.fill_limit, FillLimit::Part, t("Körperteil"));
                        ui.selectable_value(&mut self.fill_limit, FillLimit::All, t("Alles"));
                    });
                }
                ui.checkbox(&mut self.fill_noise, t("Mit Rauschen füllen"));
                if self.fill_noise {
                    self.noise_options(ui);
                }
            }
            Tool::Line | Tool::Rect => {
                if self.tool == Tool::Rect {
                    ui.checkbox(&mut self.rect_fill, t("Gefüllt"));
                }
                ui.checkbox(&mut self.fill_noise, t("Mit Rauschen"));
                if self.fill_noise {
                    self.noise_options(ui);
                }
            }
            _ => {}
        }

        ui.add_space(8.0);
        ui.separator();
        ui.toggle_value(&mut self.mirror, t("↔  Spiegeln (M)"))
            .on_hover_text(t("Alles, was du malst, erscheint auch auf der anderen Körperseite."));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(self.can_undo(), egui::Button::new(t("⟲ Zurück"))).on_hover_text(t("Strg+Z")).clicked() {
                self.undo();
            }
            if ui.add_enabled(self.can_redo(), egui::Button::new(t("⟳ Vor"))).on_hover_text(t("Strg+Y")).clicked() {
                self.redo();
            }
        });
        ui.add_space(10.0);
        ui.separator();
        let btn = egui::Button::new(RichText::new(t("📂  Skin importieren …")).strong()).min_size(egui::vec2(ui.available_width(), 32.0));
        if ui
            .add(btn)
            .on_hover_text(t("Einen PNG-Skin in dieses Projekt laden (Strg+O).\nStrg+Z macht es rückgängig.\nOder PNG einfach ins Fenster ziehen."))
            .clicked()
        {
            self.import_requested = true;
        }
    }

    fn replace_options(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("1. Alte Farbe:")).strong());
                match self.replace_from {
                    Some(c) => {
                        swatch(ui, c, egui::vec2(28.0, 18.0), false);
                        if ui.small_button(t("✖ neu wählen")).clicked() {
                            self.replace_from = None;
                        }
                    }
                    None => {
                        ui.label(RichText::new(t("im Skin anklicken")).weak());
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("2. Neue Farbe:")).strong());
                ui.label(RichText::new(if self.replace_from.is_some() { t("jetzt im Skin anklicken") } else { t("danach anklicken") }).weak());
            });
            if let Some(from) = self.replace_from {
                let n = self.replace_matches().len();
                ui.label(RichText::new(crate::i18n::t1("{0} Pixel werden getauscht (orange markiert)", n)).small().color(Color32::from_rgb(255, 170, 60)));
                ui.horizontal(|ui| {
                    ui.label(t("oder:"));
                    let to = self.primary;
                    if ui.add(egui::Button::new(t("Mit Hauptfarbe ersetzen"))).clicked() {
                        let before = self.skin.clone();
                        self.replace_all(from, to);
                        self.replace_from = None;
                        if self.skin != before {
                            self.push_undo(before, self.model);
                        }
                    }
                    swatch(ui, to, egui::vec2(18.0, 18.0), false);
                });
            }
        });
    }

    fn noise_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Stärke"));
            ui.add(egui::Slider::new(&mut self.noise_amount, 0.02..=0.4).show_value(false));
        });
        ui.horizontal(|ui| {
            ui.label(t("Stufen"));
            ui.add(egui::Slider::new(&mut self.noise_levels, 2..=6).suffix(t(" Farben")))
                .on_hover_text(t("Wie viele verschiedene Helligkeiten der Pinsel benutzt"));
        });
        ui.horizontal(|ui| {
            ui.label(t("Farbton"));
            ui.add(egui::Slider::new(&mut self.noise_hue, 0.0..=1.0).show_value(false));
        });
        // kleine Vorschau des Rauschens
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(200.0), 18.0), Sense::hover());
        let mut rng = Rng::new(7);
        let n = 20;
        let w = rect.width() / n as f32;
        for k in 0..n {
            for row in 0..2 {
                let c = color::noise_color(self.primary, &mut rng, self.noise_amount, self.noise_levels, self.noise_hue);
                let r = Rect::from_min_size(
                    rect.min + egui::vec2(k as f32 * w, row as f32 * 9.0),
                    egui::vec2(w.ceil(), 9.0),
                );
                ui.painter().rect_filled(r, 0.0, c32(c));
            }
        }
    }

    pub fn ui_colors(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new(t("FARBE")).small().strong().weak());
        ui.horizontal(|ui| {
            for sec in [false, true] {
                let c = self.active_color(sec);
                let sel = self.edit_secondary == sec;
                let r = swatch(ui, c, egui::vec2(46.0, 34.0), sel);
                if r.clicked() {
                    self.edit_secondary = sec;
                }
                r.on_hover_text(if sec { t("Zweitfarbe (Rechtsklick)") } else { t("Hauptfarbe (Linksklick)") });
            }
            if ui.button("⬌").on_hover_text(t("Farben tauschen (X)")).clicked() {
                std::mem::swap(&mut self.primary, &mut self.secondary);
            }
        });
        ui.add_space(4.0);
        let cur = self.active_color(self.edit_secondary);
        if self.picker_for != Some(cur) {
            let hsv = color::to_hsv(cur);
            // Farbton behalten, wenn die Farbe grau/schwarz ist
            self.picker_hsv = if hsv.s < 0.01 || hsv.v < 0.01 { color::Hsv { h: self.picker_hsv.h, ..hsv } } else { hsv };
            self.picker_for = Some(cur);
        }
        let mut alpha = cur[3];
        let width = ui.available_width().min(236.0);
        if crate::ui_util::hsv_picker(ui, &mut self.picker_hsv, &mut alpha, width) {
            let v = color::from_hsv(self.picker_hsv, alpha);
            self.picker_for = Some(v);
            if self.edit_secondary {
                self.secondary = v;
            } else {
                self.primary = v;
            }
        }
        ui.add_space(4.0);
        // Hex-Eingabe
        let cur = self.active_color(self.edit_secondary);
        if self.hex_for != cur {
            self.hex = color::to_hex(cur);
            self.hex_for = cur;
        }
        ui.horizontal(|ui| {
            ui.label(t("Hex"));
            let r = ui.add(egui::TextEdit::singleline(&mut self.hex).desired_width(90.0));
            if r.changed() {
                if let Some(v) = color::from_hex(&self.hex) {
                    if self.edit_secondary {
                        self.secondary = v;
                    } else {
                        self.primary = v;
                    }
                    self.hex_for = v;
                }
            }
        });

        ui.add_space(6.0);
        ui.label(RichText::new(t("Schattierungen")).small().weak());
        let shades = color::shades(cur, 9);
        self.swatch_row(ui, &shades, 9);

        ui.add_space(6.0);
        ui.label(RichText::new(t("Palette")).small().weak());
        self.swatch_row(ui, &PALETTE, 8);

        if !self.recent.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(t("Zuletzt benutzt")).small().weak());
            let recent = self.recent.clone();
            self.swatch_row(ui, &recent, 8);
        }
    }

    fn swatch_row(&mut self, ui: &mut egui::Ui, colors: &[Rgba], per_row: usize) {
        let w = ((ui.available_width() - (per_row as f32 - 1.0) * 3.0) / per_row as f32).clamp(14.0, 28.0);
        egui::Grid::new(ui.next_auto_id()).spacing([3.0, 3.0]).min_col_width(w).show(ui, |ui| {
            for (k, &c) in colors.iter().enumerate() {
                let r = swatch(ui, c, egui::vec2(w, w), false);
                if r.clicked() {
                    self.primary = c;
                    self.edit_secondary = false;
                }
                if r.secondary_clicked() {
                    self.secondary = c;
                }
                r.on_hover_text(format!("{}\n{}", color::to_hex(c), t("Links: Hauptfarbe · Rechts: Zweitfarbe")));
                if k % per_row == per_row - 1 {
                    ui.end_row();
                }
            }
        });
    }

    pub fn ui_view_options(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new(t("MODELL")).small().strong().weak());
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.model, Model::Classic, t("Classic")).on_hover_text(Model::Classic.name());
            ui.selectable_value(&mut self.model, Model::Slim, t("Slim")).on_hover_text(Model::Slim.name());
        });
    }

    /// Körperteile und Ebenen ein-/ausblenden (wie bei Skindex), damit man besser an
    /// verdeckte Stellen kommt.
    pub fn ui_parts(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(t("KÖRPERTEILE")).small().strong().weak());
        let orange = Color32::from_rgb(242, 140, 40);
        let bw = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).floor();
        ui.horizontal(|ui| {
            for (k, name) in [(0usize, t("1. Ebene")), (1, t("2. Ebene"))] {
                let on = self.layer_on[k];
                let text = RichText::new(name).strong().color(if on { Color32::WHITE } else { ui.visuals().weak_text_color() });
                let btn = egui::Button::new(text)
                    .corner_radius(0.0)
                    .fill(if on { orange } else { ui.visuals().widgets.inactive.weak_bg_fill })
                    .min_size(egui::vec2(bw, 24.0));
                let tip = if k == 0 { t("Körper (1. Ebene) ein-/ausblenden") } else { t("Äußere Ebene (Hut, Jacke …) ein-/ausblenden") };
                if ui.add(btn).on_hover_text(tip).clicked() {
                    self.layer_on[k] = !on;
                }
            }
        });
        ui.add_space(4.0);

        // Figur: Kopf, Arme, Körper, Beine
        let slim = self.model == Model::Slim;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 140.0), Sense::hover());
        let u = (rect.height() / 34.0).min(rect.width() / 20.0);
        let gap = u * 0.6;
        let aw = if slim { 3.0 } else { 4.0 };
        let total_w = 8.0 * u + 2.0 * (aw * u + gap);
        let x0 = rect.center().x - total_w / 2.0;
        let y0 = rect.top() + (rect.height() - (32.0 * u + 2.0 * gap)) / 2.0;
        let body_x = x0 + aw * u + gap;
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
        let parts = [
            (Part::Head, r(body_x, y0, 8.0 * u, 8.0 * u)),
            (Part::Body, r(body_x, y0 + 8.0 * u + gap, 8.0 * u, 12.0 * u)),
            // rechter Arm der Figur = links im Bild (Ansicht von vorne)
            (Part::RightArm, r(x0, y0 + 8.0 * u + gap, aw * u, 12.0 * u)),
            (Part::LeftArm, r(body_x + 8.0 * u + gap, y0 + 8.0 * u + gap, aw * u, 12.0 * u)),
            (Part::RightLeg, r(body_x, y0 + 20.0 * u + 2.0 * gap, 4.0 * u - gap * 0.5, 12.0 * u)),
            (Part::LeftLeg, r(body_x + 4.0 * u + gap * 0.5, y0 + 20.0 * u + 2.0 * gap, 4.0 * u - gap * 0.5, 12.0 * u)),
        ];
        for (part, pr) in parts {
            let id = ui.scope_id().with(("part_toggle", part.idx()));
            let resp = ui.interact(pr, id, Sense::click());
            if resp.clicked() {
                self.part_on[part.idx()] = !self.part_on[part.idx()];
            }
            if resp.secondary_clicked() {
                // nur dieses Teil zeigen (oder wieder alle)
                let solo = self.part_on.iter().filter(|&&b| b).count() == 1 && self.part_on[part.idx()];
                self.part_on = if solo { [true; 6] } else { std::array::from_fn(|i| i == part.idx()) };
            }
            let on = self.part_on[part.idx()];
            let p = ui.painter();
            let fill = if on {
                if resp.hovered() { orange.gamma_multiply(0.55) } else { orange.gamma_multiply(0.32) }
            } else if resp.hovered() {
                Color32::from_white_alpha(14)
            } else {
                Color32::TRANSPARENT
            };
            p.rect_filled(pr, 0.0, fill);
            let stroke = if on { Stroke::new(2.0, orange) } else { Stroke::new(1.5, orange.gamma_multiply(0.35)) };
            p.rect_stroke(pr, 0.0, stroke, StrokeKind::Inside);
            resp.on_hover_text(crate::i18n::t1("{0} – Klick: ein-/ausblenden\nRechtsklick: nur dieses Teil zeigen", part.name()));
        }
        if self.part_on.iter().any(|b| !b) || self.layer_on.iter().any(|b| !b) {
            if ui.small_button(t("Alle zeigen")).clicked() {
                self.part_on = [true; 6];
                self.layer_on = [true; 2];
            }
        } else {
            ui.add(egui::Label::new(RichText::new(t("Klick: aus-/einblenden\nRechtsklick: nur dieses Teil")).small().weak()));
        }
        self.sync_vis();
    }

    fn sync_vis(&mut self) {
        self.vis = Visibility {
            base: std::array::from_fn(|i| self.layer_on[0] && self.part_on[i]),
            overlay: std::array::from_fn(|i| self.layer_on[1] && self.part_on[i]),
        };
    }

    /// Ist dieses Texel gerade sichtbar (Teil und Ebene eingeblendet)?
    fn texel_visible(&self, x: usize, y: usize) -> bool {
        match skin::texel_info(self.model, x, y) {
            Some(t) => match t.layer {
                Layer::Base => self.vis.base[t.part.idx()],
                Layer::Overlay => self.vis.overlay[t.part.idx()],
            },
            None => false,
        }
    }

    /// 3D-Vorschau mit Malen auf dem Modell.
    pub fn ui_preview(&mut self, ui: &mut egui::Ui) {
        self.sync_vis();
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("3D-ANSICHT")).small().strong().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.preview.grid, t("Raster")).on_hover_text(t("Pixelraster auf dem Modell"));
                if !crate::gpu3d::gpu_active() {
                    ui.checkbox(&mut self.preview.antialias, t("Glätten"));
                }
            });
        });
        let h = (ui.available_height() - 64.0).max(120.0);
        let size = egui::vec2(ui.available_width(), h);
        let mut highlight = self.hover_footprint(self.hover2d.or(self.hover3d));
        if let Some(st) = &self.stroke {
            highlight.clear();
            // Live-Vorschau für Linie und Rechteck
            if matches!(st.tool, Tool::Line | Tool::Rect) {
                highlight = self.shape_points(st).into_iter().filter(|&(x, y)| x >= 0 && y >= 0).map(|(x, y)| (x as usize, y as usize)).collect();
            }
        }
        if self.tool == Tool::Replace {
            highlight.extend(self.replace_matches());
        }
        let alt = ui.input(|i| i.modifiers.alt);
        let skin_copy = self.skin.clone();
        let inp = self.preview.show(ui, size, &skin_copy, self.model, self.vis, &highlight, true);
        self.hover3d = inp.hover;
        if inp.stroke_started {
            if let Some((x, y)) = inp.texels.first().copied().or(inp.hover) {
                let prev_tool = self.tool;
                if alt {
                    self.tool = Tool::Picker;
                }
                self.begin_stroke(x as i32, y as i32, false, true);
                self.tool = prev_tool;
            }
        }
        if self.stroke.as_ref().is_some_and(|s| s.in3d) {
            for &(x, y) in inp.texels.iter() {
                self.stroke_to(x as i32, y as i32);
            }
        }
        if inp.stroke_ended {
            self.end_stroke();
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            use std::f32::consts::PI;
            if ui.button(t("Vorne")).clicked() {
                self.preview.set_view(0.0, 0.1);
            }
            if ui.button(t("Hinten")).clicked() {
                self.preview.set_view(PI, 0.1);
            }
            if ui.button(t("Links")).clicked() {
                self.preview.set_view(-PI / 2.0, 0.1);
            }
            if ui.button(t("Rechts")).clicked() {
                self.preview.set_view(PI / 2.0, 0.1);
            }
            if ui.button("⟲").on_hover_text(t("Ansicht zurücksetzen")).clicked() {
                self.preview.cam = Default::default();
            }
        });
        ui.label(
            RichText::new(t("Linksklick: malen · Rechte Maustaste oder leere Fläche ziehen: drehen · Mausrad: Zoom · Doppelklick daneben: Ansicht zurücksetzen"))
                .small()
                .weak(),
        );
    }

    fn hover_footprint(&self, hover: Option<(usize, usize)>) -> Vec<(usize, usize)> {
        let Some((x, y)) = hover else { return vec![] };
        let mut v = Vec::new();
        let offs = if matches!(self.tool, Tool::Bucket | Tool::Replace | Tool::Picker | Tool::Rect) {
            vec![(0, 0)]
        } else {
            self.brush_offsets()
        };
        for (dx, dy) in offs {
            let (tx, ty) = (x as i32 + dx, y as i32 + dy);
            if tx < 0 || ty < 0 || tx >= SIZE as i32 || ty >= SIZE as i32 {
                continue;
            }
            let (tx, ty) = (tx as usize, ty as usize);
            v.push((tx, ty));
            if self.mirror {
                if let Some(m) = skin::mirror_texel(self.model, tx, ty) {
                    v.push(m);
                }
            }
        }
        v
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.tex_checker.is_none() {
            let n = SIZE * 2;
            let mut px = Vec::with_capacity(n * n);
            for y in 0..n {
                for x in 0..n {
                    px.push(if (x + y) % 2 == 0 { Color32::from_gray(58) } else { Color32::from_gray(70) });
                }
            }
            self.tex_checker = Some(ctx.load_texture("checker", ColorImage::new([n, n], px), TextureOptions::NEAREST));
        }
        if self.tex_unused.as_ref().map(|(m, _)| *m) != Some(self.model) {
            let map = skin::texel_map(self.model);
            let px: Vec<Color32> = map
                .iter()
                .map(|t| if t.is_none() { Color32::from_rgba_unmultiplied(22, 24, 29, 235) } else { Color32::TRANSPARENT })
                .collect();
            let tex = ctx.load_texture("unused", ColorImage::new([SIZE, SIZE], px), TextureOptions::NEAREST);
            self.tex_unused = Some((self.model, tex));
        }
        if self.tex_skin_src.as_ref() != Some(&self.skin) {
            let img = ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &self.skin.as_bytes());
            match &mut self.tex_skin {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None => self.tex_skin = Some(ctx.load_texture("skin", img, TextureOptions::NEAREST)),
            }
            self.tex_skin_src = Some(self.skin.clone());
        }
    }

    /// Die große 2D-Zeichenfläche mit dem Skin-Layout.
    pub fn ui_canvas(&mut self, ui: &mut egui::Ui) {
        self.ensure_textures(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let painter = ui.painter_at(rect);

        // Maßstab: ganze physische Pixel pro Texel für gestochen scharfe Darstellung
        let fit = ((rect.width().min(rect.height()) - 24.0) / SIZE as f32).max(1.0);
        let scale_px = (fit * self.zoom * ppp).round().max(1.0);
        let scale = scale_px / ppp;
        let mut origin = rect.center() - Vec2::splat(SIZE as f32 * scale / 2.0) + self.pan;
        origin = Pos2::new((origin.x * ppp).round() / ppp, (origin.y * ppp).round() / ppp);
        let canvas = Rect::from_min_size(origin, Vec2::splat(SIZE as f32 * scale));
        let to_texel = |p: Pos2| -> (i32, i32) {
            let v = (p - origin) / scale;
            (v.x.floor() as i32, v.y.floor() as i32)
        };
        let texel_rect = |x: i32, y: i32| -> Rect {
            Rect::from_min_size(origin + egui::vec2(x as f32, y as f32) * scale, Vec2::splat(scale))
        };

        // Zoom mit Mausrad um den Mauszeiger
        if let Some(hp) = response.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom;
                self.zoom = (self.zoom * (1.0 + scroll * 0.0025)).clamp(0.5, 8.0);
                let k = self.zoom / old;
                let center = rect.center() + self.pan;
                self.pan += (hp - center) * (1.0 - k);
            }
        }

        // Pointer-Status
        let (primary_down, secondary_down, middle_down, alt, space) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.middle_down(),
                i.modifiers.alt,
                i.key_down(Key::Space),
            )
        });
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        self.hover2d = pointer.and_then(|p| {
            let (x, y) = to_texel(p);
            (x >= 0 && y >= 0 && x < SIZE as i32 && y < SIZE as i32).then_some((x as usize, y as usize))
        });

        // Verschieben (mittlere Maustaste oder Leertaste + ziehen)
        if response.is_pointer_button_down_on() && (middle_down || (space && primary_down) || self.panning) {
            self.panning = middle_down || primary_down;
            self.pan += response.drag_delta();
        } else if !response.is_pointer_button_down_on() {
            self.panning = false;
        }

        // Malen: alle Maus-Ereignisse einzeln auswerten, damit auch sehr schnelle
        // Striche lückenlos und vollständig ankommen.
        if !self.panning && !space {
            let events = ui.input(|i| i.events.clone());
            let clamp = |(x, y): (i32, i32)| (x.clamp(-1, SIZE as i32), y.clamp(-1, SIZE as i32));
            for ev in events {
                match ev {
                    egui::Event::PointerButton { pos, button, pressed: true, .. }
                        if self.stroke.is_none()
                            && response.hovered()
                            && rect.contains(pos)
                            && matches!(button, egui::PointerButton::Primary | egui::PointerButton::Secondary) =>
                    {
                        let (x, y) = to_texel(pos);
                        let prev = self.tool;
                        if alt {
                            self.tool = Tool::Picker;
                        }
                        self.begin_stroke(x, y, button == egui::PointerButton::Secondary, false);
                        self.tool = prev;
                    }
                    egui::Event::PointerMoved(pos) if self.stroke.as_ref().is_some_and(|s| !s.in3d) => {
                        self.stroke_to_clamped(clamp(to_texel(pos)));
                    }
                    egui::Event::PointerButton { pos, pressed: false, .. }
                        if self.stroke.as_ref().is_some_and(|s| !s.in3d) =>
                    {
                        self.stroke_to_clamped(clamp(to_texel(pos)));
                        self.end_stroke();
                    }
                    _ => {}
                }
            }
        }
        // Sicherheitsnetz, falls das Loslassen außerhalb des Fensters passiert ist
        if self.stroke.as_ref().is_some_and(|s| !s.in3d) && !primary_down && !secondary_down {
            self.end_stroke();
        }

        // --- Zeichnen
        let full_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.rect_filled(canvas.expand(1.0), 2.0, Color32::from_gray(20));
        if let Some(t) = &self.tex_checker {
            painter.image(t.id(), canvas, full_uv, Color32::WHITE);
        }
        if let Some(t) = &self.tex_skin {
            painter.image(t.id(), canvas, full_uv, Color32::WHITE);
        }
        if let Some((_, t)) = &self.tex_unused {
            painter.image(t.id(), canvas, full_uv, Color32::WHITE);
        }

        // Vorschau für Linie/Rechteck
        if let Some(st) = &self.stroke {
            if matches!(st.tool, Tool::Line | Tool::Rect) && !st.in3d {
                let c = c32(self.active_color(st.secondary));
                for (x, y) in self.shape_points(st) {
                    painter.rect_filled(texel_rect(x, y), 0.0, c);
                    if self.mirror && x >= 0 && y >= 0 {
                        if let Some((mx, my)) = skin::mirror_texel(self.model, x as usize, y as usize) {
                            painter.rect_filled(texel_rect(mx as i32, my as i32), 0.0, c);
                        }
                    }
                }
            }
        }

        // Raster
        if self.show_grid && scale_px >= 5.0 {
            let col = Color32::from_black_alpha(if scale_px >= 12.0 { 110 } else { 70 });
            for k in 1..SIZE {
                let x = origin.x + k as f32 * scale;
                let y = origin.y + k as f32 * scale;
                painter.line_segment([Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())], Stroke::new(1.0 / ppp, col));
                painter.line_segment([Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)], Stroke::new(1.0 / ppp, col));
            }
        }

        // ausgeblendete Körperteile abdunkeln
        for b in skin::boxes(self.model) {
            let visible = match b.layer {
                Layer::Base => self.vis.base[b.part.idx()],
                Layer::Overlay => self.vis.overlay[b.part.idx()],
            };
            if visible {
                continue;
            }
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                let r = Rect::from_min_size(
                    origin + egui::vec2(fx as f32, fy as f32) * scale,
                    egui::vec2(fw as f32, fh as f32) * scale,
                );
                painter.rect_filled(r, 0.0, Color32::from_black_alpha(170));
            }
        }

        // Bereiche der Körperteile
        let hover_info = self.hover2d.or(self.hover3d).and_then(|(x, y)| skin::texel_info(self.model, x, y));
        if self.show_regions {
            for b in skin::boxes(self.model) {
                for face in Face::ALL {
                    let (fx, fy, fw, fh) = b.face_rect(face);
                    let r = Rect::from_min_size(
                        origin + egui::vec2(fx as f32, fy as f32) * scale,
                        egui::vec2(fw as f32, fh as f32) * scale,
                    );
                    let is_hover = hover_info.is_some_and(|t| t.part == b.part && t.layer == b.layer && t.face == face);
                    let base_col = if b.layer == Layer::Base {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 46)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 190, 80, 46)
                    };
                    let col = if is_hover { Color32::from_rgb(120, 180, 255) } else { base_col };
                    painter.rect_stroke(r, 0.0, Stroke::new(if is_hover { 1.5 } else { 1.0 }, col), StrokeKind::Inside);
                }
            }
        }

        // Ersetzen: Pixel der alten Farbe markieren
        if self.tool == Tool::Replace {
            for (x, y) in self.replace_matches() {
                painter.rect_stroke(texel_rect(x as i32, y as i32), 0.0, Stroke::new(1.5, Color32::from_rgb(255, 170, 60)), StrokeKind::Inside);
            }
        }

        // Pinsel-Cursor
        let hover_any = self.hover2d.or(self.hover3d);
        let fp = self.hover_footprint(hover_any);
        for (k, &(x, y)) in fp.iter().enumerate() {
            let r = texel_rect(x as i32, y as i32);
            let mirrored = self.mirror && k % 2 == 1;
            let col = if mirrored { Color32::from_rgb(255, 170, 60) } else { Color32::WHITE };
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, col), StrokeKind::Inside);
            if self.tool != Tool::Eraser && !mirrored && matches!(self.tool, Tool::Pencil | Tool::Noise) {
                painter.rect_filled(r.shrink(scale * 0.3), 0.0, c32(self.primary).gamma_multiply(0.85));
            }
        }

        let cursor = if self.panning {
            egui::CursorIcon::Grabbing
        } else if self.hover2d.is_some() {
            egui::CursorIcon::Crosshair
        } else {
            egui::CursorIcon::Default
        };
        response.on_hover_cursor(cursor);

        // Bereichs-Name oben links
        if let Some(t) = hover_info {
            let text = format!("{} · {} · {}", t.part.name(), t.face.name(), t.layer.name());
            let pos = rect.left_top() + egui::vec2(10.0, 8.0);
            let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
            let bg = Rect::from_min_size(pos - egui::vec2(6.0, 3.0), galley.size() + egui::vec2(12.0, 6.0));
            painter.rect_filled(bg, CornerRadius::same(6), Color32::from_black_alpha(160));
            painter.galley(pos, galley, Color32::WHITE);
        }
    }

    /// Fenster „Farben anpassen“ (Farbton / Sättigung / Helligkeit mit Live-Vorschau).
    pub fn ui_adjust_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.adjust.take() else { return };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut changed = false;
        egui::Window::new(t("🎨 Farben anpassen"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(t("Bereich:"));
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.selectable_value(&mut st.target, AdjustTarget::All, t("Ganzer Skin")).changed();
                    changed |= ui.selectable_value(&mut st.target, AdjustTarget::Layer(Layer::Base), t("1. Ebene")).changed();
                    changed |= ui.selectable_value(&mut st.target, AdjustTarget::Layer(Layer::Overlay), t("2. Ebene")).changed();
                });
                ui.horizontal_wrapped(|ui| {
                    for p in Part::ALL {
                        changed |= ui.selectable_value(&mut st.target, AdjustTarget::Part(p), p.name()).changed();
                    }
                });
                ui.add_space(6.0);
                egui::Grid::new("adj").num_columns(2).show(ui, |ui| {
                    ui.label(t("Farbton"));
                    changed |= ui.add(egui::Slider::new(&mut st.hue, -180.0..=180.0).suffix("°")).changed();
                    ui.end_row();
                    ui.label(t("Sättigung"));
                    changed |= ui.add(egui::Slider::new(&mut st.sat, -1.0..=1.0)).changed();
                    ui.end_row();
                    ui.label(t("Helligkeit"));
                    changed |= ui.add(egui::Slider::new(&mut st.bright, -1.0..=1.0)).changed();
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("✔ Übernehmen")).clicked() {
                        apply = true;
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        cancel = true;
                    }
                    if ui.button(t("Zurücksetzen")).clicked() {
                        st.hue = 0.0;
                        st.sat = 0.0;
                        st.bright = 0.0;
                        changed = true;
                    }
                });
            });
        if changed {
            let mut out = st.original.clone();
            let map = skin::texel_map(self.model);
            for (idx, t) in map.iter().enumerate() {
                let Some(t) = t else { continue };
                let hit = match st.target {
                    AdjustTarget::All => true,
                    AdjustTarget::Part(p) => t.part == p,
                    AdjustTarget::Layer(l) => t.layer == l,
                };
                if hit {
                    out.px[idx] = color::adjust(out.px[idx], st.hue, st.sat, st.bright);
                }
            }
            self.skin = out;
        }
        if apply {
            if self.skin != st.original {
                self.push_undo(st.original, self.model);
            }
        } else if cancel || !open {
            self.skin = st.original;
        } else {
            self.adjust = Some(st);
        }
    }

    pub fn status_text(&self) -> String {
        let mut s = String::new();
        if let Some((x, y)) = self.hover2d.or(self.hover3d) {
            s += &format!("X {x:>2}  Y {y:>2}");
            let c = self.skin.get(x, y);
            s += &format!("   {}", if c[3] == 0 { t("durchsichtig").to_string() } else { color::to_hex(c) });
        }
        s
    }
}

fn fill_box(skin: &mut Skin, model: Model, part: Part, layer: Layer, mut f: impl FnMut(Rgba) -> Rgba) {
    let b = skin::box_of(model, part, layer);
    for face in Face::ALL {
        let (x, y, w, h) = b.face_rect(face);
        for j in 0..h {
            for i in 0..w {
                let c = skin.get(x + i, y + j);
                skin.set(x + i, y + j, f(c));
            }
        }
    }
}

fn blend(below: Rgba, above: Rgba) -> Rgba {
    let a = above[3] as f32 / 255.0;
    if a <= 0.0 {
        return below;
    }
    let b = below[3] as f32 / 255.0;
    let out_a = a + b * (1.0 - a);
    let mut out = [0u8; 4];
    for k in 0..3 {
        let v = (above[k] as f32 * a + below[k] as f32 * b * (1.0 - a)) / out_a;
        out[k] = v.round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round() as u8;
    out
}

pub fn line_points(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x0, mut y0) = a;
    let (x1, y1) = b;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

/// Erkennt anhand ungenutzter Pixel, ob ein Skin Slim-Arme hat.
pub fn detect_model(skin: &Skin) -> Model {
    let slim_gap = (20..32).all(|y| (54..56).all(|x| skin.get(x, y)[3] == 0))
        && (52..64).all(|y| (46..48).all(|x| skin.get(x, y)[3] == 0));
    if slim_gap { Model::Slim } else { Model::Classic }
}
