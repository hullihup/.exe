//! Skin-Mixer: bis zu 26 Skins (A–Z) kombinieren – z. B. Partnerlook: eigener Kopf +
//! fremdes Outfit. Mit Hände-Auswahl, automatischer Hautfarben-Anpassung und Outfit-Farbvarianten.

use crate::i18n::t;
use eframe::egui::{self, Color32, ColorImage, RichText, Sense, TextureHandle, TextureOptions};

use crate::color::{self, Rng};
use crate::preview::Preview;
use crate::render3d::Visibility;
use crate::skin::{self, Face, Layer, Model, Part, Rgba, Skin, SIZE};
use crate::ui_util::{c32, heading, paint_skin_thumb, rgba};

pub const MAX_SLOTS: usize = 26;
/// Zeile für die Hände (zusätzlich zu den 6 Körperteilen)
const HANDS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    /// Skin A, B, C … (Index)
    Slot(u8),
    /// leer lassen
    Empty,
    /// Hände: so lassen, wie es vom Arm kommt
    Same,
    /// Hände: in der Hautfarbe des Kopfes
    SkinTone,
}

pub fn letter(i: usize) -> char {
    (b'A' + i.min(25) as u8) as char
}

/// Unterscheidbare Farbe pro Buchstabe
pub fn letter_color(i: usize) -> Color32 {
    c32(color::from_hsv(color::Hsv { h: (i as f32 * 137.5 + 210.0) % 360.0, s: 0.62, v: 0.88 }, 255))
}

pub struct Slot {
    pub skin: Skin,
    pub model: Model,
    pub name: String,
    uid: u64,
    tex: Option<TextureHandle>,
    tex_src: Option<Skin>,
}

impl Slot {
    fn texture(&mut self, ctx: &egui::Context) -> &TextureHandle {
        if self.tex_src.as_ref() != Some(&self.skin) || self.tex.is_none() {
            let img = ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &self.skin.as_bytes());
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None => self.tex = Some(ctx.load_texture(format!("mix_slot_{}", self.uid), img, TextureOptions::NEAREST)),
            }
            self.tex_src = Some(self.skin.clone());
        }
        self.tex.as_ref().unwrap()
    }
}

/// Verknüpfung mit einem Projekt, in dem das Ergebnis weiterbearbeitet wird.
pub struct MixLink {
    pub project_id: String,
    pub name: String,
    /// Mix-Ergebnis zum Zeitpunkt der Übergabe (ohne eigene Änderungen)
    pub base: Skin,
}

#[derive(Clone, PartialEq)]
struct Inputs {
    slots: Vec<(Skin, Model)>,
    choice: [[Src; 2]; 7],
    hand_len: usize,
    tone_transfer: bool,
    tone_tol: f32,
    tone_override: Option<Rgba>,
    outfit: (f32, f32, f32),
    model_choice: Option<Model>,
    patch: Vec<Option<Rgba>>,
}

pub enum MixerAction {
    None,
    /// Skins laden: `None` = neue hinzufügen, `Some(i)` = Skin i ersetzen
    Load(Option<usize>),
    /// Skin aus dem Editor: `None` = hinzufügen, `Some(i)` = ersetzen
    FromEditor(Option<usize>),
    OpenInEditor,
    Export,
}

/// Hautfarbe eines Skins: Hauptton plus alle Schattierungen aus dem Gesicht.
#[derive(Clone, Debug)]
pub struct TonePalette {
    pub main: Rgba,
    main_lab: [f32; 3],
    /// Lab-Werte aller Hautschattierungen, nach Helligkeit sortiert
    shades: Vec<[f32; 3]>,
}

impl TonePalette {
    fn from_color(c: Rgba) -> Self {
        let lab = color::to_lab(c);
        let shades = [-12.0, -6.0, 0.0, 5.0].iter().map(|d| [lab[0] + d, lab[1], lab[2]]).collect();
        TonePalette { main: c, main_lab: lab, shades }
    }
}

pub struct Mixer {
    pub slots: Vec<Slot>,
    choice: [[Src; 2]; 7],
    hand_len: usize,
    tone_transfer: bool,
    tone_tol: f32,
    tone_override: Option<Rgba>,
    outfit_hue: f32,
    outfit_sat: f32,
    outfit_bright: f32,
    model_choice: Option<Model>,
    pub result: Skin,
    /// Ergebnis ohne die Änderungen aus dem Editor
    pub raw_result: Skin,
    pub result_model: Model,
    pub link: Option<MixLink>,
    /// Pixel, die im Editor geändert wurden (überschreiben das Mix-Ergebnis)
    patch: Vec<Option<Rgba>>,
    pub vis: Visibility,
    pub preview: Preview,
    last_inputs: Option<Inputs>,
    tones: Vec<Option<TonePalette>>,
    tex_result: Option<TextureHandle>,
    tex_result_src: Option<Skin>,
    rng: Rng,
    next_uid: u64,
    pub last_preset: String,
}

impl Mixer {
    pub fn new() -> Self {
        let mut m = Mixer {
            slots: Vec::new(),
            choice: [[Src::Slot(0); 2]; 7],
            hand_len: 3,
            tone_transfer: true,
            tone_tol: 0.5,
            tone_override: None,
            outfit_hue: 0.0,
            outfit_sat: 0.0,
            outfit_bright: 0.0,
            model_choice: None,
            result: Skin::blank(),
            raw_result: Skin::blank(),
            result_model: Model::Classic,
            link: None,
            patch: Vec::new(),
            vis: Visibility::default(),
            preview: Preview::new(),
            last_inputs: None,
            tones: Vec::new(),
            tex_result: None,
            tex_result_src: None,
            rng: Rng::from_time(),
            next_uid: 0,
            last_preset: String::new(),
        };
        m.preset_partner(0, 1);
        m
    }

    /// Fügt einen Skin hinzu (oder ersetzt Skin `at`). Gibt den Buchstaben zurück.
    pub fn set_slot(&mut self, at: Option<usize>, skin: Skin, model: Model, name: String) -> Option<char> {
        self.next_uid += 1;
        let slot = Slot { skin, model, name, uid: self.next_uid, tex: None, tex_src: None };
        match at {
            Some(i) if i < self.slots.len() => {
                self.slots[i] = slot;
                Some(letter(i))
            }
            _ if self.slots.len() < MAX_SLOTS => {
                self.slots.push(slot);
                Some(letter(self.slots.len() - 1))
            }
            _ => None,
        }
    }

    /// Änderungen aus dem verknüpften Projekt übernehmen (`skin` = aktueller Stand im Editor).
    pub fn sync_from_editor(&mut self, skin: &Skin) {
        let Some(link) = &self.link else { return };
        let patch: Vec<Option<Rgba>> =
            skin.px.iter().zip(&link.base.px).map(|(e, b)| if e != b { Some(*e) } else { None }).collect();
        if patch.iter().all(|p| p.is_none()) {
            if !self.patch.is_empty() {
                self.patch.clear();
            }
        } else if patch != self.patch {
            self.patch = patch;
        }
    }

    pub fn unlink(&mut self) {
        self.link = None;
        self.patch.clear();
    }

    pub fn is_full(&self) -> bool {
        self.slots.len() >= MAX_SLOTS
    }

    fn remove_slot(&mut self, k: usize) {
        if k >= self.slots.len() {
            return;
        }
        self.slots.remove(k);
        for row in &mut self.choice {
            for c in row.iter_mut() {
                if let Src::Slot(i) = *c {
                    let i = i as usize;
                    *c = if i == k {
                        Src::Slot(0)
                    } else if i > k {
                        Src::Slot((i - 1) as u8)
                    } else {
                        *c
                    };
                }
            }
        }
    }

    // ------------------------------------------------------------- Vorlagen

    fn preset_partner(&mut self, head: u8, outfit: u8) {
        for p in Part::ALL {
            let s = Src::Slot(if p == Part::Head { head } else { outfit });
            self.choice[p.idx()] = [s, s];
        }
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.tone_transfer = true;
        self.last_preset = crate::i18n::t2("Partnerlook: Kopf {0} + Outfit {1}", letter(head as usize), letter(outfit as usize));
    }

    fn preset_parts(&mut self, from_b: &[Part], name: &str) {
        for p in Part::ALL {
            let s = Src::Slot(if from_b.contains(&p) { 1 } else { 0 });
            self.choice[p.idx()] = [s, s];
        }
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.last_preset = name.to_string();
    }

    fn preset_random(&mut self) {
        let n = self.slots.len().max(1);
        for p in Part::ALL {
            let base = Src::Slot(self.rng.range(n) as u8);
            let over = match self.rng.range(5) {
                0 => Src::Empty,
                1 | 2 => base,
                _ => Src::Slot(self.rng.range(n) as u8),
            };
            self.choice[p.idx()] = [base, over];
        }
        // Gliedmaßen paarweise gleich, damit es stimmig aussieht
        self.choice[Part::LeftArm.idx()] = self.choice[Part::RightArm.idx()];
        self.choice[Part::LeftLeg.idx()] = self.choice[Part::RightLeg.idx()];
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.last_preset = t("Zufallsmix").into();
    }

    // ------------------------------------------------------------- Berechnung

    fn slot_idx(&self, s: Src) -> Option<usize> {
        match s {
            Src::Slot(i) if (i as usize) < self.slots.len() => Some(i as usize),
            _ => None,
        }
    }

    pub fn recompute(&mut self) {
        let inputs = Inputs {
            slots: self.slots.iter().map(|s| (s.skin.clone(), s.model)).collect(),
            choice: self.choice,
            hand_len: self.hand_len,
            tone_transfer: self.tone_transfer,
            tone_tol: self.tone_tol,
            tone_override: self.tone_override,
            outfit: (self.outfit_hue, self.outfit_sat, self.outfit_bright),
            model_choice: self.model_choice,
            patch: self.patch.clone(),
        };
        if self.last_inputs.as_ref() == Some(&inputs) {
            return;
        }
        self.tones = self.slots.iter().map(|s| detect_palette(&s.skin, s.model)).collect();

        // Modell des Ergebnisses: das der Arme
        let arm_src = self.slot_idx(self.choice[Part::RightArm.idx()][0]);
        let auto_model = arm_src.or(if self.slots.is_empty() { None } else { Some(0) }).map(|i| self.slots[i].model);
        let dst_model = self.model_choice.or(auto_model).unwrap_or_default();

        let head_src = self.slot_idx(self.choice[Part::Head.idx()][0]);
        let head_pal: Option<TonePalette> =
            self.tone_override.map(TonePalette::from_color).or_else(|| head_src.and_then(|i| self.tones[i].clone()));

        // Woher stammt jedes Texel? (für die Hautfarben-Anpassung)
        const FILLED: u8 = 254;
        const NONE: u8 = 255;
        let mut src_of = vec![NONE; SIZE * SIZE];
        let mut out = Skin::blank();
        for p in Part::ALL {
            for layer in [Layer::Base, Layer::Overlay] {
                let Some(k) = self.slot_idx(self.choice[p.idx()][layer.idx()]) else { continue };
                let s = &self.slots[k];
                for (x, y) in skin::copy_part_where(&s.skin, s.model, &mut out, dst_model, p, layer, |_, _, _, _, _| true) {
                    src_of[y * SIZE + x] = k as u8;
                }
            }
        }

        // Hände (unterer Teil beider Arme + Unterseite)
        let hl = self.hand_len;
        let is_hand = move |face: Face, _i: usize, j: usize, _w: usize, h: usize| match face {
            Face::Bottom => true,
            Face::Top => false,
            _ => j + hl >= h,
        };
        for layer in [Layer::Base, Layer::Overlay] {
            let hs = self.choice[HANDS][layer.idx()];
            for arm in [Part::RightArm, Part::LeftArm] {
                match hs {
                    Src::Same => {}
                    Src::Slot(_) => {
                        if let Some(k) = self.slot_idx(hs) {
                            let s = &self.slots[k];
                            for (x, y) in skin::copy_part_where(&s.skin, s.model, &mut out, dst_model, arm, layer, is_hand) {
                                src_of[y * SIZE + x] = k as u8;
                            }
                        }
                    }
                    Src::Empty | Src::SkinTone => {
                        let b = skin::box_of(dst_model, arm, layer);
                        for face in Face::ALL {
                            let (fx, fy, fw, fh) = b.face_rect(face);
                            for j in 0..fh {
                                for i in 0..fw {
                                    if !is_hand(face, i, j, fw, fh) {
                                        continue;
                                    }
                                    let (x, y) = (fx + i, fy + j);
                                    let c = match (&head_pal, hs, layer) {
                                        (Some(pal), Src::SkinTone, Layer::Base) => {
                                            // erste Reihe unter dem Ärmel und Unterseite etwas dunkler
                                            let shadow = if face == Face::Bottom {
                                                -7.0
                                            } else if j + hl == fh && hl < fh {
                                                -5.0
                                            } else {
                                                0.0
                                            };
                                            skin_fill(pal, x, y, shadow)
                                        }
                                        _ => skin::CLEAR,
                                    };
                                    out.set(x, y, c);
                                    src_of[y * SIZE + x] = FILLED;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Hautfarbe angleichen & Outfit umfärben
        let outfit = (self.outfit_hue, self.outfit_sat, self.outfit_bright);
        let do_outfit = outfit != (0.0, 0.0, 0.0);
        let map = skin::texel_map(dst_model);
        for idx in 0..SIZE * SIZE {
            let k = src_of[idx];
            let Some(t) = map[idx] else { continue };
            if k >= FILLED || t.part == Part::Head {
                continue;
            }
            let c = out.px[idx];
            if c[3] == 0 {
                continue;
            }
            let src_pal = self.tones[k as usize].as_ref();
            // Haut gibt es nur auf der 1. Ebene; die 2. Ebene ist Kleidung/Accessoires
            let is_skin = t.layer == Layer::Base && src_pal.is_some_and(|p| is_skin(c, p, self.tone_tol));
            if is_skin {
                let other_source = Some(k as usize) != head_src || self.tone_override.is_some();
                if self.tone_transfer && other_source {
                    if let (Some(from), Some(to)) = (src_pal, head_pal.as_ref()) {
                        out.px[idx] = transfer_tone(c, from, to);
                    }
                }
            } else if do_outfit {
                out.px[idx] = color::adjust(c, outfit.0, outfit.1, outfit.2);
            }
        }

        self.raw_result = out.clone();
        // Änderungen aus dem Editor drüberlegen
        for (idx, p) in self.patch.iter().enumerate() {
            if let Some(c) = p {
                out.px[idx] = *c;
            }
        }
        self.result = out;
        self.result_model = dst_model;
        self.last_inputs = Some(inputs);
    }

    // ------------------------------------------------------------- Oberfläche

    pub fn ui_inputs(&mut self, ui: &mut egui::Ui, editor_available: bool) -> MixerAction {
        let mut action = MixerAction::None;
        let ctx = ui.ctx().clone();
        heading(ui, &format!("SKINS ({}/{MAX_SLOTS})", self.slots.len()));
        let mut remove = None;
        for k in 0..self.slots.len() {
            let tone = self.tones.get(k).cloned().flatten();
            let s = &mut self.slots[k];
            egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Buchstabe
                    let (br, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::hover());
                    ui.painter().circle_filled(br.center(), 12.0, letter_color(k));
                    ui.painter().text(
                        br.center(),
                        egui::Align2::CENTER_CENTER,
                        letter(k),
                        egui::FontId::proportional(14.0),
                        Color32::BLACK,
                    );
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), Sense::hover());
                    let tex = s.texture(&ctx).clone();
                    paint_skin_thumb(ui, rect, &tex);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let name: String = if s.name.chars().count() > 14 {
                                format!("{}…", s.name.chars().take(13).collect::<String>())
                            } else {
                                s.name.clone()
                            };
                            ui.label(RichText::new(name).strong()).on_hover_text(&s.name);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.selectable_value(&mut s.model, Model::Classic, RichText::new(t("Classic")).small());
                            ui.selectable_value(&mut s.model, Model::Slim, RichText::new(t("Slim")).small());
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            match &tone {
                                Some(pal) => {
                                    ui.label(RichText::new(t("Haut")).small().weak());
                                    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 11.0), Sense::hover());
                                    ui.painter().rect_filled(r, 3.0, c32(pal.main));
                                }
                                None => {
                                    ui.label(RichText::new(t("Haut: –")).small().weak())
                                        .on_hover_text(t("Keine Hautfarbe im Gesicht gefunden – hier wird nichts umgefärbt."));
                                }
                            }
                            ui.add_space(4.0);
                            if ui.small_button("📂").on_hover_text(t("Durch andere PNG-Datei ersetzen")).clicked() {
                                action = MixerAction::Load(Some(k));
                            }
                            if ui
                                .add_enabled(editor_available, egui::Button::new("✏").small())
                                .on_hover_text(t("Durch Skin aus dem Editor ersetzen"))
                                .clicked()
                            {
                                action = MixerAction::FromEditor(Some(k));
                            }
                            if ui.small_button("✖").on_hover_text(t("Entfernen")).clicked() {
                                remove = Some(k);
                            }
                        });
                    });
                });
            });
        }
        if let Some(k) = remove {
            self.remove_slot(k);
        }

        if self.slots.is_empty() {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 90.0), Sense::hover());
            ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t("PNG-Skins hierher ziehen\n(bis zu 26 Stück)"),
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.is_full(), egui::Button::new(t("➕ Skins laden …")))
                .on_hover_text(t("Eine oder mehrere PNG-Dateien hinzufügen"))
                .clicked()
            {
                action = MixerAction::Load(None);
            }
            if ui
                .add_enabled(editor_available && !self.is_full(), egui::Button::new(t("✏ Aus Editor")))
                .on_hover_text(t("Den Skin aus dem geöffneten Projekt hinzufügen"))
                .clicked()
            {
                action = MixerAction::FromEditor(None);
            }
        });
        if self.slots.len() >= 2 && ui.button(t("🔃  A und B tauschen")).clicked() {
            self.slots.swap(0, 1);
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Tipp: Lade deinen Skin als A und den Outfit-Skin als B. Rechts wählst du für jedes Körperteil, \
                 von welchem Skin es kommt – mit bis zu 26 Skins (A–Z).",
            )
            .small()
            .weak(),
        );
        action
    }

    /// Auswahl der Quelle (A, B, C …) für ein Körperteil
    fn src_picker(ui: &mut egui::Ui, value: &mut Src, names: &[String], hands: bool, base: bool, id: (usize, usize)) {
        let n = names.len();
        let label = |s: Src| -> String {
            match s {
                Src::Slot(i) => letter(i as usize).to_string(),
                Src::Empty => "–".into(),
                Src::Same => t("Arm").into(),
                Src::SkinTone => t("Haut").into(),
            }
        };
        let mut options: Vec<Src> = Vec::new();
        if hands {
            options.push(Src::Same);
            if base {
                options.push(Src::SkinTone);
            }
        }
        options.extend((0..n.max(1)).map(|i| Src::Slot(i as u8)));
        options.push(Src::Empty);
        let tip = |s: Src| match s {
            Src::Slot(i) => names.get(i as usize).cloned().unwrap_or_else(|| format!("Skin {}", letter(i as usize))),
            Src::Empty => t("leer").into(),
            Src::Same => t("so wie beim Arm").into(),
            Src::SkinTone => t("in der Hautfarbe des Kopfes").into(),
        };
        if options.len() <= 3 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for o in options {
                    ui.selectable_value(value, o, label(o)).on_hover_text(tip(o));
                }
            });
        } else {
            egui::ComboBox::from_id_salt(("mix_src", id.0, id.1))
                .width(58.0)
                .selected_text(RichText::new(label(*value)).strong())
                .show_ui(ui, |ui| {
                    for o in options {
                        ui.selectable_value(value, o, format!("{}  ·  {}", label(o), tip(o)));
                    }
                });
        }
    }

    pub fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let n = self.slots.len();
        let names: Vec<String> = self.slots.iter().map(|s| s.name.clone()).collect();
        heading(ui, t("VORLAGEN"));
        ui.horizontal_wrapped(|ui| {
            let two = n >= 2;
            if ui.add_enabled(two, egui::Button::new(t("💑 Kopf A + Outfit B"))).clicked() {
                self.preset_partner(0, 1);
            }
            if ui.add_enabled(two, egui::Button::new(t("💑 Kopf B + Outfit A"))).clicked() {
                self.preset_partner(1, 0);
            }
            if ui.add_enabled(two, egui::Button::new(t("👕 Oberteil von B"))).clicked() {
                self.preset_parts(&[Part::Body, Part::RightArm, Part::LeftArm], t("Oberteil von B"));
            }
            if ui.add_enabled(two, egui::Button::new(t("👖 Hose von B"))).clicked() {
                self.preset_parts(&[Part::RightLeg, Part::LeftLeg], t("Hose von B"));
            }
            if ui.add_enabled(two, egui::Button::new(t("🎩 Frisur/Hut von B"))).clicked() {
                self.preset_parts(&[], t("Frisur/Hut von B"));
                self.choice[Part::Head.idx()][1] = Src::Slot(1);
            }
            if ui.add_enabled(n >= 1, egui::Button::new(t("🎲 Zufallsmix"))).clicked() {
                self.preset_random();
            }
        });

        ui.add_space(6.0);
        heading(ui, t("KÖRPERTEILE"));
        egui::Grid::new("mix_parts").num_columns(3).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t("1. Ebene")).small());
            ui.label(RichText::new(t("2. Ebene")).small());
            ui.end_row();
            for p in Part::ALL {
                ui.label(p.name());
                for l in 0..2 {
                    Self::src_picker(ui, &mut self.choice[p.idx()][l], &names, false, l == 0, (p.idx(), l));
                }
                ui.end_row();
            }
            ui.label(RichText::new(t("✋ Hände")).strong()).on_hover_text(t("Unterer Teil beider Arme"));
            for l in 0..2 {
                Self::src_picker(ui, &mut self.choice[HANDS][l], &names, true, l == 0, (HANDS, l));
            }
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label(t("Handlänge"));
            ui.add(egui::Slider::new(&mut self.hand_len, 1..=8).suffix(t(" px")))
                .on_hover_text(t("Wie viele Pixel unten am Arm als Hand zählen"));
        });
        if ui.small_button(t("✋ Hände in Hautfarbe")).on_hover_text(t("Hände in der Hautfarbe des Kopfes einfärben")).clicked() {
            self.choice[HANDS] = [Src::SkinTone, Src::Empty];
        }

        ui.add_space(8.0);
        heading(ui, t("SMART-ANPASSUNG"));
        ui.checkbox(&mut self.tone_transfer, t("Hautfarbe automatisch angleichen")).on_hover_text(
            "Erkennt die Hautfarbe jedes Skins mit allen Schattierungen am Gesicht und färbt sichtbare Haut \
             (Hände, Hals, Beine …), die von einem anderen Skin kommt, passend zum Kopf um.",
        );
        if self.tone_transfer {
            ui.horizontal(|ui| {
                ui.label(t("Toleranz"));
                ui.add(egui::Slider::new(&mut self.tone_tol, 0.0..=1.0).show_value(false))
                    .on_hover_text(t("Höher = auch stärker abweichende Hauttöne werden erkannt"));
            });
        }
        ui.horizontal(|ui| {
            let mut manual = self.tone_override.is_some();
            if ui.checkbox(&mut manual, t("Eigene Hautfarbe")).changed() {
                let head = self.slot_idx(self.choice[Part::Head.idx()][0]);
                let detected = head.and_then(|i| self.tones.get(i).cloned().flatten()).map(|p| p.main);
                self.tone_override = if manual { detected.or(Some([214, 164, 130, 255])) } else { None };
            }
            if let Some(t) = &mut self.tone_override {
                let mut c = c32(*t);
                if egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque)
                    .changed()
                {
                    *t = rgba(c);
                }
            }
        });
        ui.add_space(6.0);
        ui.label(t("Outfit-Farbe (alles außer Kopf und Haut):"));
        egui::Grid::new("outfit").num_columns(2).show(ui, |ui| {
            ui.label(t("Farbton"));
            ui.add(egui::Slider::new(&mut self.outfit_hue, -180.0..=180.0).suffix("°"));
            ui.end_row();
            ui.label(t("Sättigung"));
            ui.add(egui::Slider::new(&mut self.outfit_sat, -1.0..=1.0));
            ui.end_row();
            ui.label(t("Helligkeit"));
            ui.add(egui::Slider::new(&mut self.outfit_bright, -1.0..=1.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(t("🎲 Farbvariante")).on_hover_text(t("Zufällige neue Outfit-Farbe")).clicked() {
                self.outfit_hue = (self.rng.f32() * 300.0 + 30.0).round() - 180.0;
                self.outfit_sat = (self.rng.f32() * 0.4 - 0.1).clamp(-0.2, 0.3);
                self.outfit_bright = self.rng.f32() * 0.2 - 0.1;
            }
            if ui.button(t("Zurücksetzen")).clicked() {
                self.outfit_hue = 0.0;
                self.outfit_sat = 0.0;
                self.outfit_bright = 0.0;
            }
        });

        ui.add_space(8.0);
        heading(ui, t("ERGEBNIS-MODELL"));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.model_choice, None, t("Automatisch"));
            ui.selectable_value(&mut self.model_choice, Some(Model::Classic), t("Classic"));
            ui.selectable_value(&mut self.model_choice, Some(Model::Slim), t("Slim"));
        });
        ui.label(RichText::new(t("Arme werden bei Bedarf automatisch zwischen 3 px und 4 px umgerechnet.")).small().weak());
    }

    pub fn ui_result(&mut self, ui: &mut egui::Ui) -> MixerAction {
        self.recompute();
        let mut action = MixerAction::None;
        if self.slots.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(t("👫  Lade links Skins (oder ziehe PNG-Dateien ins Fenster),\num sie zu kombinieren."))
                        .size(16.0)
                        .weak(),
                );
            });
            return action;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("ERGEBNIS")).small().strong().weak());
            if !self.last_preset.is_empty() {
                ui.label(RichText::new(format!("· {}", self.last_preset)).small().weak());
            }
            let link_info = self.link.as_ref().map(|l| l.name.clone());
            if let Some(name) = link_info {
                let edits = self.patch.iter().filter(|p| p.is_some()).count();
                let mut unlink = false;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t("Verknüpfung lösen")).clicked() {
                        unlink = true;
                    }
                    ui.label(
                        RichText::new(crate::i18n::t2("🔗 verknüpft mit „{0}“ · {1} Pixel im Editor geändert", &name, edits))
                            .small()
                            .color(crate::ORANGE),
                    )
                    .on_hover_text(t("Was du im Editor an diesem Skin änderst, wird hier automatisch übernommen."));
                });
                if unlink {
                    self.unlink();
                }
            }
        });
        let avail = ui.available_size();
        let thumb = 148.0;
        let size = egui::vec2(avail.x, (avail.y - 56.0).max(150.0));
        let res_skin = self.result.clone();
        let inp = self.preview.show(ui, size, &res_skin, self.result_model, self.vis, &[], false);
        // 2D-Textur oben rechts einblenden
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
            if ui.add(egui::Button::new(RichText::new(t("✏  Im Editor öffnen")).strong()).min_size(egui::vec2(160.0, 32.0))).clicked() {
                action = MixerAction::OpenInEditor;
            }
            if ui.add(egui::Button::new(RichText::new(t("💾  Exportieren")).strong()).min_size(egui::vec2(140.0, 32.0))).clicked() {
                action = MixerAction::Export;
            }
            ui.separator();
            use std::f32::consts::PI;
            if ui.button(t("Vorne")).clicked() {
                self.preview.set_view(0.0, 0.1);
            }
            if ui.button(t("Hinten")).clicked() {
                self.preview.set_view(PI, 0.1);
            }
            if ui.button("⟲").clicked() {
                self.preview.cam = Default::default();
            }
        });
        action
    }
}

// ------------------------------------------------------------- Hautfarben-Erkennung

fn skin_like(c: Rgba) -> bool {
    let h = color::to_hsv(c);
    (h.h <= 50.0 || h.h >= 345.0) && (0.08..=0.8).contains(&h.s) && h.v > 0.2
}

/// Abstand, bei dem Helligkeitsunterschiede (Schattierung) weniger zählen als Farbunterschiede.
fn shade_dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dl = (a[0] - b[0]) * 0.45;
    (dl * dl + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Ermittelt die Hautfarbe eines Skins (Hauptton + Schattierungen) anhand des Gesichts.
/// Gibt `None` zurück, wenn keine hautähnliche Farbe gefunden wird (z. B. Masken oder
/// schwarze Gesichter) – dann wird auch nichts umgefärbt.
pub fn detect_palette(s: &Skin, model: Model) -> Option<TonePalette> {
    let head = skin::box_of(model, Part::Head, Layer::Base);
    let mut cand: Vec<(Rgba, f32)> = Vec::new();
    let mut total = 0usize;
    for face in [Face::Front, Face::Right, Face::Left, Face::Bottom] {
        let (fx, fy, fw, fh) = head.face_rect(face);
        // obere Reihen sind meist Haare
        let start = if face == Face::Bottom { 0 } else { 2 };
        for j in start..fh {
            for i in 0..fw {
                let c = s.get(fx + i, fy + j);
                if c[3] < 255 {
                    continue;
                }
                total += 1;
                if skin_like(c) {
                    let w = match face {
                        Face::Front => 1.6,
                        Face::Bottom => 0.7,
                        _ => 1.0,
                    };
                    cand.push((c, w));
                }
            }
        }
    }
    if cand.len() < 6 || (cand.len() as f32) < total as f32 * 0.12 {
        return None;
    }
    // Hauptton: Farbe, um die herum die meisten (gewichteten) Kandidaten liegen
    let labs: Vec<[f32; 3]> = cand.iter().map(|(c, _)| color::to_lab(*c)).collect();
    let mut best = (0usize, -1.0f32);
    for (i, a) in labs.iter().enumerate() {
        let score: f32 = labs.iter().zip(&cand).filter(|(b, _)| shade_dist(*a, **b) < 8.0).map(|(_, (_, w))| *w).sum();
        if score > best.1 {
            best = (i, score);
        }
    }
    let main_lab = labs[best.0];
    // Alle Schattierungen derselben Hautfarbe einsammeln
    let mut shades: Vec<[f32; 3]> = labs.iter().copied().filter(|l| shade_dist(*l, main_lab) < 14.0).collect();
    shades.sort_by(|a, b| a[0].total_cmp(&b[0]));
    shades.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.8 && (a[1] - b[1]).abs() < 0.8 && (a[2] - b[2]).abs() < 0.8);
    // Hauptton als Mittelwert der nahen Farben (glättet Rauschen)
    let near: Vec<&[f32; 3]> = labs.iter().filter(|l| shade_dist(**l, main_lab) < 6.0).collect();
    let n = near.len().max(1) as f32;
    let avg = [
        near.iter().map(|l| l[0]).sum::<f32>() / n,
        near.iter().map(|l| l[1]).sum::<f32>() / n,
        near.iter().map(|l| l[2]).sum::<f32>() / n,
    ];
    Some(TonePalette { main: color::from_lab(avg, 255), main_lab: avg, shades })
}

/// Gehört diese Farbe zur Haut des Skins (inkl. Schattierungen)?
fn is_skin(c: Rgba, pal: &TonePalette, tol: f32) -> bool {
    let lab = color::to_lab(c);
    // Weiß, Grau, Schwarz sind keine Haut (Haut hat immer etwas Farbe)
    if color::chroma(lab) < color::chroma(pal.main_lab) * 0.45 {
        return false;
    }
    let near_shade = pal.shades.iter().any(|s| shade_dist(lab, *s) < 4.0 + 8.0 * tol);
    let near_main = shade_dist(lab, pal.main_lab) < 10.0 + 14.0 * tol && (lab[0] - pal.main_lab[0]).abs() < 30.0 + 20.0 * tol;
    near_shade || near_main
}

/// Färbt eine Hautfarbe um und behält dabei Schattierung und Feinheiten bei.
fn transfer_tone(c: Rgba, from: &TonePalette, to: &TonePalette) -> Rgba {
    let p = color::to_lab(c);
    let f = from.main_lab;
    let t = to.main_lab;
    // Helligkeitsabstand übernehmen (an den Kontrast der Zielhaut angepasst)
    let range = |pal: &TonePalette| {
        let lo = pal.shades.first().map(|s| s[0]).unwrap_or(pal.main_lab[0]);
        let hi = pal.shades.last().map(|s| s[0]).unwrap_or(pal.main_lab[0]);
        (hi - lo).max(6.0)
    };
    let k = (range(to) / range(from)).clamp(0.6, 1.6);
    let l = t[0] + (p[0] - f[0]) * k;
    let a = t[1] + (p[1] - f[1]) * 0.6;
    let b = t[2] + (p[2] - f[2]) * 0.6;
    color::from_lab([l.clamp(2.0, 98.0), a, b], c[3])
}

/// Hautfarbe für eingefärbte Hände: leicht variierend aus den echten Schattierungen.
fn skin_fill(pal: &TonePalette, x: usize, y: usize, shadow: f32) -> Rgba {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    // nur Schattierungen nahe am Hauptton verwenden
    let close: Vec<&[f32; 3]> = pal.shades.iter().filter(|s| (s[0] - pal.main_lab[0]).abs() < 7.0).collect();
    let base = if close.is_empty() { pal.main_lab } else { *close[(h as usize >> 8) % close.len()] };
    let mut lab = [pal.main_lab[0] * 0.6 + base[0] * 0.4, pal.main_lab[1], pal.main_lab[2]];
    lab[0] += shadow;
    color::from_lab(lab, 255)
}
