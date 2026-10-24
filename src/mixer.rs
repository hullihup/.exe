//! Skin-Mixer: zwei Skins kombinieren (z. B. Partnerlook: eigener Kopf + fremdes Outfit),
//! mit automatischer Hautton-Erkennung und Outfit-Farbvarianten.

use eframe::egui::{self, ColorImage, RichText, Sense, TextureHandle, TextureOptions};

use crate::color::{self, Rng};
use crate::preview::Preview;
use crate::render3d::Visibility;
use crate::skin::{self, Layer, Model, Part, Rgba, Skin, SIZE};
use crate::ui_util::{c32, heading, paint_skin_thumb, rgba};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    A,
    B,
    Empty,
}

pub struct Slot {
    pub skin: Skin,
    pub model: Model,
    pub name: String,
    tex: Option<TextureHandle>,
    tex_src: Option<Skin>,
}

impl Slot {
    pub fn new(skin: Skin, model: Model, name: String) -> Self {
        Slot { skin, model, name, tex: None, tex_src: None }
    }
    fn texture(&mut self, ctx: &egui::Context, id: &str) -> &TextureHandle {
        if self.tex_src.as_ref() != Some(&self.skin) || self.tex.is_none() {
            let img = ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &self.skin.as_bytes());
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None => self.tex = Some(ctx.load_texture(id, img, TextureOptions::NEAREST)),
            }
            self.tex_src = Some(self.skin.clone());
        }
        self.tex.as_ref().unwrap()
    }
}

#[derive(Clone, PartialEq)]
struct Inputs {
    a: Option<(Skin, Model)>,
    b: Option<(Skin, Model)>,
    choice: [[Src; 2]; 6],
    tone_transfer: bool,
    tone_tol: f32,
    tone_override: Option<Rgba>,
    outfit: (f32, f32, f32),
    model_choice: Option<Model>,
}

pub enum MixerAction {
    None,
    LoadA,
    LoadB,
    FromEditorA,
    FromEditorB,
    OpenInEditor,
    Export,
}

pub struct Mixer {
    pub a: Option<Slot>,
    pub b: Option<Slot>,
    choice: [[Src; 2]; 6],
    tone_transfer: bool,
    tone_tol: f32,
    tone_override: Option<Rgba>,
    outfit_hue: f32,
    outfit_sat: f32,
    outfit_bright: f32,
    model_choice: Option<Model>,
    pub result: Skin,
    pub result_model: Model,
    pub vis: Visibility,
    pub preview: Preview,
    last_inputs: Option<Inputs>,
    tone_a: Option<Rgba>,
    tone_b: Option<Rgba>,
    tex_result: Option<TextureHandle>,
    tex_result_src: Option<Skin>,
    rng: Rng,
    pub last_preset: &'static str,
}

impl Mixer {
    pub fn new() -> Self {
        let mut m = Mixer {
            a: None,
            b: None,
            choice: [[Src::A; 2]; 6],
            tone_transfer: true,
            tone_tol: 0.5,
            tone_override: None,
            outfit_hue: 0.0,
            outfit_sat: 0.0,
            outfit_bright: 0.0,
            model_choice: None,
            result: Skin::blank(),
            result_model: Model::Classic,
            vis: Visibility::default(),
            preview: Preview::new(),
            last_inputs: None,
            tone_a: None,
            tone_b: None,
            tex_result: None,
            tex_result_src: None,
            rng: Rng::from_time(),
            last_preset: "",
        };
        m.preset_partner(true);
        m
    }

    pub fn set_slot(&mut self, which_b: bool, skin: Skin, model: Model, name: String) {
        let slot = Some(Slot::new(skin, model, name));
        if which_b {
            self.b = slot;
        } else {
            self.a = slot;
        }
    }

    pub fn first_free_is_b(&self) -> bool {
        self.a.is_some()
    }

    // ------------------------------------------------------------- Vorlagen

    fn preset_partner(&mut self, head_from_a: bool) {
        let (head, body) = if head_from_a { (Src::A, Src::B) } else { (Src::B, Src::A) };
        for p in Part::ALL {
            let s = if p == Part::Head { head } else { body };
            self.choice[p.idx()] = [s, s];
        }
        self.tone_transfer = true;
        self.last_preset = if head_from_a { "Partnerlook: Kopf A + Outfit B" } else { "Partnerlook: Kopf B + Outfit A" };
    }

    fn preset_parts(&mut self, from_b: &[Part], name: &'static str) {
        for p in Part::ALL {
            let s = if from_b.contains(&p) { Src::B } else { Src::A };
            self.choice[p.idx()] = [s, s];
        }
        self.last_preset = name;
    }

    fn preset_random(&mut self) {
        for p in Part::ALL {
            let base = if self.rng.f32() < 0.5 { Src::A } else { Src::B };
            let over = match self.rng.range(5) {
                0 => Src::Empty,
                1 | 2 => base,
                _ => if base == Src::A { Src::B } else { Src::A },
            };
            self.choice[p.idx()] = [base, over];
        }
        // Gliedmaßen paarweise gleich, damit es stimmig aussieht
        self.choice[Part::LeftArm.idx()] = self.choice[Part::RightArm.idx()];
        self.choice[Part::LeftLeg.idx()] = self.choice[Part::RightLeg.idx()];
        self.last_preset = "Zufallsmix";
    }

    // ------------------------------------------------------------- Berechnung

    fn slot(&self, s: Src) -> Option<&Slot> {
        match s {
            Src::A => self.a.as_ref(),
            Src::B => self.b.as_ref(),
            Src::Empty => None,
        }
    }

    fn recompute(&mut self) {
        let inputs = Inputs {
            a: self.a.as_ref().map(|s| (s.skin.clone(), s.model)),
            b: self.b.as_ref().map(|s| (s.skin.clone(), s.model)),
            choice: self.choice,
            tone_transfer: self.tone_transfer,
            tone_tol: self.tone_tol,
            tone_override: self.tone_override,
            outfit: (self.outfit_hue, self.outfit_sat, self.outfit_bright),
            model_choice: self.model_choice,
        };
        if self.last_inputs.as_ref() == Some(&inputs) {
            return;
        }
        self.tone_a = self.a.as_ref().and_then(|s| detect_skin_tone(&s.skin, s.model));
        self.tone_b = self.b.as_ref().and_then(|s| detect_skin_tone(&s.skin, s.model));

        // Modell des Ergebnisses: das der Arme
        let arm_src = self.choice[Part::RightArm.idx()][0];
        let auto_model = self.slot(arm_src).or(self.a.as_ref()).or(self.b.as_ref()).map(|s| s.model).unwrap_or_default();
        let dst_model = self.model_choice.unwrap_or(auto_model);

        let head_src = self.choice[Part::Head.idx()][0];
        let head_tone = self.tone_override.or(match head_src {
            Src::A => self.tone_a,
            Src::B => self.tone_b,
            Src::Empty => None,
        });
        let mut out = Skin::blank();
        for p in Part::ALL {
            for layer in [Layer::Base, Layer::Overlay] {
                let src = self.choice[p.idx()][layer.idx()];
                let Some(slot) = self.slot(src) else { continue };
                let mut part_skin = Skin::blank();
                skin::copy_part(&slot.skin, slot.model, &mut part_skin, dst_model, p, layer);

                let src_tone = match src {
                    Src::A => self.tone_a,
                    Src::B => self.tone_b,
                    Src::Empty => None,
                };
                let transfer = self.tone_transfer && p != Part::Head && (src != head_src || self.tone_override.is_some());
                let outfit = (self.outfit_hue, self.outfit_sat, self.outfit_bright);
                let do_outfit = p != Part::Head && outfit != (0.0, 0.0, 0.0);
                let b = skin::box_of(dst_model, p, layer);
                for face in skin::Face::ALL {
                    let (fx, fy, fw, fh) = b.face_rect(face);
                    for j in 0..fh {
                        for i in 0..fw {
                            let (x, y) = (fx + i, fy + j);
                            let mut c = part_skin.get(x, y);
                            if c[3] == 0 {
                                out.set(x, y, c);
                                continue;
                            }
                            let is_skin = src_tone.is_some_and(|t| is_tone(c, t, self.tone_tol));
                            if is_skin {
                                if transfer {
                                    if let (Some(from), Some(to)) = (src_tone, head_tone) {
                                        c = transfer_tone(c, from, to);
                                    }
                                }
                            } else if do_outfit {
                                c = color::adjust(c, outfit.0, outfit.1, outfit.2);
                            }
                            out.set(x, y, c);
                        }
                    }
                }
            }
        }
        self.result = out;
        self.result_model = dst_model;
        self.last_inputs = Some(inputs);
    }

    // ------------------------------------------------------------- Oberfläche

    pub fn ui_inputs(&mut self, ui: &mut egui::Ui) -> MixerAction {
        let mut action = MixerAction::None;
        for which_b in [false, true] {
            let label = if which_b { "SKIN B" } else { "SKIN A" };
            heading(ui, label);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let ctx = ui.ctx().clone();
                let slot = if which_b { &mut self.b } else { &mut self.a };
                let tone = if which_b { self.tone_b } else { self.tone_a };
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(112.0, 112.0), Sense::hover());
                    match slot {
                        Some(s) => {
                            let tex = s.texture(&ctx, if which_b { "mix_b" } else { "mix_a" }).clone();
                            paint_skin_thumb(ui, rect, &tex);
                        }
                        None => {
                            ui.painter().rect_stroke(
                                rect,
                                6.0,
                                egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "PNG hierher\nziehen",
                                egui::FontId::proportional(12.0),
                                ui.visuals().weak_text_color(),
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        if let Some(s) = slot.as_mut() {
                            ui.label(RichText::new(&s.name).strong());
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut s.model, Model::Classic, "Classic");
                                ui.selectable_value(&mut s.model, Model::Slim, "Slim");
                            });
                            if let Some(t) = tone {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Hautton:").small());
                                    let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 12.0), Sense::hover());
                                    ui.painter().rect_filled(r, 3.0, c32(t));
                                });
                            }
                        } else {
                            ui.label(RichText::new("Noch kein Skin").weak());
                        }
                        if ui.button("📂 Laden…").clicked() {
                            action = if which_b { MixerAction::LoadB } else { MixerAction::LoadA };
                        }
                        if ui.button("✏ Aus Editor").on_hover_text("Den Skin aus dem Editor-Tab übernehmen").clicked() {
                            action = if which_b { MixerAction::FromEditorB } else { MixerAction::FromEditorA };
                        }
                    });
                });
            });
        }
        ui.add_space(6.0);
        if ui.add_enabled(self.a.is_some() || self.b.is_some(), egui::Button::new("🔃  A und B tauschen")).clicked() {
            std::mem::swap(&mut self.a, &mut self.b);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "Tipp: Für einen Partnerlook lädst du deinen Skin als A und den Skin mit dem Outfit als B. \
                 Der Mixer übernimmt Kopf & Frisur von A, die Kleidung von B und passt die Hautfarbe an.",
            )
            .small()
            .weak(),
        );
        action
    }

    pub fn ui_settings(&mut self, ui: &mut egui::Ui) {
        heading(ui, "VORLAGEN");
        ui.horizontal_wrapped(|ui| {
            if ui.button("💑 Kopf A + Outfit B").clicked() {
                self.preset_partner(true);
            }
            if ui.button("💑 Kopf B + Outfit A").clicked() {
                self.preset_partner(false);
            }
            if ui.button("👕 Oberteil von B").clicked() {
                self.preset_parts(&[Part::Body, Part::RightArm, Part::LeftArm], "Oberteil von B");
            }
            if ui.button("👖 Hose von B").clicked() {
                self.preset_parts(&[Part::RightLeg, Part::LeftLeg], "Hose von B");
            }
            if ui.button("🎩 Frisur/Hut von B").clicked() {
                self.preset_parts(&[], "Frisur/Hut von B");
                self.choice[Part::Head.idx()][1] = Src::B;
            }
            if ui.button("🎲 Zufallsmix").clicked() {
                self.preset_random();
            }
        });

        ui.add_space(6.0);
        heading(ui, "KÖRPERTEILE");
        egui::Grid::new("mix_parts").num_columns(3).spacing([8.0, 4.0]).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("1. Ebene").small());
            ui.label(RichText::new("2. Ebene").small());
            ui.end_row();
            for p in Part::ALL {
                ui.label(p.name());
                for l in 0..2 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let c = &mut self.choice[p.idx()][l];
                        ui.selectable_value(c, Src::A, "A");
                        ui.selectable_value(c, Src::B, "B");
                        ui.selectable_value(c, Src::Empty, "–").on_hover_text("leer");
                    });
                }
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        heading(ui, "SMART-ANPASSUNG");
        ui.checkbox(&mut self.tone_transfer, "Hautfarbe automatisch angleichen").on_hover_text(
            "Erkennt die Hautfarbe beider Skins am Gesicht und färbt sichtbare Haut \
             (Hände, Hals, Beine …) des Outfits passend zum Kopf um – inklusive Schattierungen.",
        );
        if self.tone_transfer {
            ui.horizontal(|ui| {
                ui.label("Toleranz");
                ui.add(egui::Slider::new(&mut self.tone_tol, 0.0..=1.0).show_value(false));
            });
            ui.horizontal(|ui| {
                let mut manual = self.tone_override.is_some();
                if ui.checkbox(&mut manual, "Eigene Hautfarbe").changed() {
                    self.tone_override = if manual { self.tone_a.or(Some([214, 164, 130, 255])) } else { None };
                }
                if let Some(t) = &mut self.tone_override {
                    let mut c = c32(*t);
                    if egui::widgets::color_picker::color_edit_button_srgba(
                        ui,
                        &mut c,
                        egui::widgets::color_picker::Alpha::Opaque,
                    )
                    .changed()
                    {
                        *t = rgba(c);
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.label("Outfit-Farbe (alles außer Kopf und Haut):");
        egui::Grid::new("outfit").num_columns(2).show(ui, |ui| {
            ui.label("Farbton");
            ui.add(egui::Slider::new(&mut self.outfit_hue, -180.0..=180.0).suffix("°"));
            ui.end_row();
            ui.label("Sättigung");
            ui.add(egui::Slider::new(&mut self.outfit_sat, -1.0..=1.0));
            ui.end_row();
            ui.label("Helligkeit");
            ui.add(egui::Slider::new(&mut self.outfit_bright, -1.0..=1.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("🎲 Farbvariante").on_hover_text("Zufällige neue Outfit-Farbe").clicked() {
                self.outfit_hue = (self.rng.f32() * 300.0 + 30.0).round() - 180.0;
                self.outfit_sat = (self.rng.f32() * 0.4 - 0.1 * 1.0).clamp(-0.2, 0.3);
                self.outfit_bright = self.rng.f32() * 0.2 - 0.1;
            }
            if ui.button("Zurücksetzen").clicked() {
                self.outfit_hue = 0.0;
                self.outfit_sat = 0.0;
                self.outfit_bright = 0.0;
            }
        });

        ui.add_space(8.0);
        heading(ui, "ERGEBNIS-MODELL");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.model_choice, None, "Automatisch");
            ui.selectable_value(&mut self.model_choice, Some(Model::Classic), "Classic");
            ui.selectable_value(&mut self.model_choice, Some(Model::Slim), "Slim");
        });
        ui.label(RichText::new("Arme werden bei Bedarf automatisch zwischen 3 px und 4 px umgerechnet.").small().weak());
    }

    pub fn ui_result(&mut self, ui: &mut egui::Ui) -> MixerAction {
        self.recompute();
        let mut action = MixerAction::None;
        if self.a.is_none() && self.b.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("👫  Lade links zwei Skins (oder ziehe PNG-Dateien ins Fenster),\num sie zu kombinieren.")
                        .size(16.0)
                        .weak(),
                );
            });
            return action;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("ERGEBNIS").small().strong().weak());
            if !self.last_preset.is_empty() {
                ui.label(RichText::new(format!("· {}", self.last_preset)).small().weak());
            }
        });
        let avail = ui.available_size();
        let thumb = 148.0;
        let size = egui::vec2(avail.x, (avail.y - 56.0).max(150.0));
        let res_skin = self.result.clone();
        let inp = self.preview.show(ui, size, &res_skin, self.result_model, self.vis, &[], false);
        // 2D-Textur unten rechts einblenden
        if self.tex_result_src.as_ref() != Some(&self.result) {
            let img = ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &self.result.as_bytes());
            match &mut self.tex_result {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None => self.tex_result = Some(ui.ctx().load_texture("mix_result", img, TextureOptions::NEAREST)),
            }
            self.tex_result_src = Some(self.result.clone());
        }
        if let Some(t) = &self.tex_result {
            let r = egui::Rect::from_min_size(inp.response.rect.right_top() + egui::vec2(-thumb - 8.0, 8.0), egui::vec2(thumb, thumb));
            paint_skin_thumb(ui, r, t);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("✏  Im Editor öffnen").strong()).min_size(egui::vec2(160.0, 32.0))).clicked() {
                action = MixerAction::OpenInEditor;
            }
            if ui.add(egui::Button::new(RichText::new("💾  Exportieren").strong()).min_size(egui::vec2(140.0, 32.0))).clicked() {
                action = MixerAction::Export;
            }
            ui.separator();
            use std::f32::consts::PI;
            if ui.button("Vorne").clicked() {
                self.preview.set_view(0.0, 0.1);
            }
            if ui.button("Hinten").clicked() {
                self.preview.set_view(PI, 0.1);
            }
            if ui.button("⟲").clicked() {
                self.preview.cam = Default::default();
            }
        });
        action
    }
}

// ------------------------------------------------------------- Hautton-Erkennung

fn skin_like(h: color::Hsv) -> bool {
    (h.h <= 50.0 || h.h >= 340.0) && (0.1..=0.8).contains(&h.s) && h.v > 0.25
}

/// Ermittelt die typische Hautfarbe eines Skins anhand des Gesichts.
pub fn detect_skin_tone(s: &Skin, model: Model) -> Option<Rgba> {
    let mut buckets: std::collections::HashMap<(u8, u8, u8), (f32, [u32; 3], u32)> = Default::default();
    let head = skin::box_of(model, Part::Head, Layer::Base);
    for face in [skin::Face::Front, skin::Face::Right, skin::Face::Left] {
        let (fx, fy, fw, fh) = head.face_rect(face);
        for j in 2..fh {
            for i in 0..fw {
                let c = s.get(fx + i, fy + j);
                if c[3] < 255 {
                    continue;
                }
                let hsv = color::to_hsv(c);
                let weight = if skin_like(hsv) { 3.0 } else { 1.0 } * if face == skin::Face::Front { 1.5 } else { 1.0 };
                let key = (c[0] >> 4, c[1] >> 4, c[2] >> 4);
                let e = buckets.entry(key).or_insert((0.0, [0; 3], 0));
                e.0 += weight;
                for k in 0..3 {
                    e.1[k] += c[k] as u32;
                }
                e.2 += 1;
            }
        }
    }
    let best = buckets.values().max_by(|a, b| a.0.total_cmp(&b.0))?;
    let n = best.2.max(1);
    Some([(best.1[0] / n) as u8, (best.1[1] / n) as u8, (best.1[2] / n) as u8, 255])
}

fn hue_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn is_tone(c: Rgba, tone: Rgba, tol: f32) -> bool {
    let a = color::to_hsv(c);
    let t = color::to_hsv(tone);
    if t.s < 0.08 {
        // graue/weiße "Haut" (z. B. Roboter): nur nahe Farben
        return color::dist2(c, tone) < (900.0 * (0.5 + tol * 2.0)) as i32;
    }
    let ratio = a.v / t.v.max(0.01);
    hue_diff(a.h, t.h) <= 8.0 + 22.0 * tol
        && (a.s - t.s).abs() <= 0.08 + 0.3 * tol
        && ratio >= 0.75 - 0.35 * tol
        && ratio <= 1.2 + 0.3 * tol
}

/// Färbt eine Hautfarbe um und behält dabei die Schattierung bei.
fn transfer_tone(c: Rgba, from: Rgba, to: Rgba) -> Rgba {
    let p = color::to_hsv(c);
    let f = color::to_hsv(from);
    let t = color::to_hsv(to);
    let h = t.h + {
        let d = (p.h - f.h + 540.0).rem_euclid(360.0) - 180.0;
        d
    };
    let s = t.s * (p.s / f.s.max(0.02));
    let v = t.v * (p.v / f.v.max(0.02));
    color::from_hsv(color::Hsv { h, s, v }, c[3])
}
