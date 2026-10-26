//! Skin-Mixer (gehört zu einem Projekt): bis zu 26 Skins (A–Z) und fertige Outfit-Vorlagen
//! kombinieren – z. B. der eigene Kopf mit einem Winter-Outfit. Mit Hände-Auswahl,
//! automatischer Hautfarben-Anpassung und Outfit-Farbvarianten.

use std::path::Path;

use crate::i18n::{t, t1};
use eframe::egui::{self, Color32, ColorImage, RichText, Sense, TextureHandle, TextureOptions};

use crate::color::{self, Rng};
use crate::preview::Preview;
use crate::render3d::{self, Visibility};
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

impl Src {
    fn code(self) -> String {
        match self {
            Src::Slot(i) => format!("S{i}"),
            Src::Empty => "E".into(),
            Src::Same => "A".into(),
            Src::SkinTone => "K".into(),
        }
    }
    fn from_code(s: &str) -> Src {
        match s {
            "E" => Src::Empty,
            "A" => Src::Same,
            "K" => Src::SkinTone,
            _ => s.strip_prefix('S').and_then(|n| n.parse().ok()).map(Src::Slot).unwrap_or(Src::Slot(0)),
        }
    }
    /// passende Auswahl für die 2. Ebene, wenn beide Ebenen gleich sein sollen
    fn for_overlay(self) -> Src {
        match self {
            Src::SkinTone => Src::Empty,
            s => s,
        }
    }
}

pub fn letter(i: usize) -> char {
    (b'A' + i.min(25) as u8) as char
}

/// Unterscheidbare Farbe pro Buchstabe
pub fn letter_color(i: usize) -> Color32 {
    c32(color::from_hsv(color::Hsv { h: (i as f32 * 137.5 + 210.0) % 360.0, s: 0.62, v: 0.88 }, 255))
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SlotKind {
    /// selbst geladene PNG
    User,
    /// der Skin des Projekts
    Project,
    /// Outfit-Vorlage (Kennung)
    Outfit(String),
}

pub struct Slot {
    pub skin: Skin,
    pub model: Model,
    pub name: String,
    pub kind: SlotKind,
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

#[derive(Clone, PartialEq)]
struct Inputs {
    slots: Vec<(Skin, Model, SlotKind)>,
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
    /// Projekt-Skin: `None` = hinzufügen, `Some(i)` = Skin i ersetzen
    FromProject(Option<usize>),
    Export,
}

/// Hautfarbe eines Skins: Hauptton plus alle Schattierungen.
#[derive(Clone, Debug)]
pub struct TonePalette {
    pub main: Rgba,
    /// Bei Outfit-Vorlagen: genau diese Farben sind Haut (sonst leer)
    exact: Vec<Rgba>,
    main_lab: [f32; 3],
    /// Lab-Werte aller Hautschattierungen, nach Helligkeit sortiert
    shades: Vec<[f32; 3]>,
}

impl TonePalette {
    fn from_color(c: Rgba) -> Self {
        let lab = color::to_lab(c);
        let shades = [-14.0, -8.0, -4.0, 0.0, 4.0, 8.0].iter().map(|d| [lab[0] + d, lab[1], lab[2]]).collect();
        TonePalette { main: c, exact: Vec::new(), main_lab: lab, shades }
    }
    fn l_range(&self) -> (f32, f32) {
        let lo = self.shades.first().map(|s| s[0]).unwrap_or(self.main_lab[0]).min(self.main_lab[0]);
        let hi = self.shades.last().map(|s| s[0]).unwrap_or(self.main_lab[0]).max(self.main_lab[0]);
        // Mindestens etwas Spielraum, damit flache Paletten nicht alles plattdrücken
        let mid = (lo + hi) * 0.5;
        let half = ((hi - lo) * 0.5).max(5.0);
        (mid - half, mid + half)
    }
}

pub struct Mixer {
    pub slots: Vec<Slot>,
    choice: [[Src; 2]; 7],
    hand_len: usize,
    /// 1. und 2. Ebene immer gleich wählen
    sync_layers: bool,
    tone_transfer: bool,
    tone_tol: f32,
    tone_override: Option<Rgba>,
    show_skin: bool,
    outfit_hue: f32,
    outfit_sat: f32,
    outfit_bright: f32,
    model_choice: Option<Model>,
    pub result: Skin,
    /// Ergebnis ohne die Änderungen aus dem Editor
    pub raw_result: Skin,
    pub result_model: Model,
    /// Mix-Stand, der zuletzt ins Projekt übernommen wurde (Grundlage für Editor-Änderungen)
    pub applied_base: Option<Skin>,
    /// Pixel, die im Editor geändert wurden (überschreiben das Mix-Ergebnis)
    patch: Vec<Option<Rgba>>,
    pub vis: Visibility,
    pub preview: Preview,
    last_inputs: Option<Inputs>,
    /// gewählter Skin für die Vorlagen-Knöpfe
    donor: Option<usize>,
    /// Mix-Einstellungen, die zuletzt automatisch in den Editor übernommen wurden
    applied_recipe: Option<Inputs>,
    tones: Vec<Option<TonePalette>>,
    head_tone: Option<TonePalette>,
    skin_mask: Vec<(usize, usize)>,
    tex_result: Option<TextureHandle>,
    tex_result_src: Option<Skin>,
    outfit_thumbs: Option<(Skin, Vec<TextureHandle>)>,
    rng: Rng,
    next_uid: u64,
    pub last_preset: String,
    /// Wurde etwas geändert, das gespeichert werden sollte?
    pub changed: bool,
}

impl Mixer {
    pub fn new() -> Self {
        let mut m = Mixer {
            slots: Vec::new(),
            choice: [[Src::Slot(0); 2]; 7],
            hand_len: 3,
            sync_layers: true,
            tone_transfer: true,
            tone_tol: 0.5,
            tone_override: None,
            show_skin: false,
            outfit_hue: 0.0,
            outfit_sat: 0.0,
            outfit_bright: 0.0,
            model_choice: None,
            result: Skin::blank(),
            raw_result: Skin::blank(),
            result_model: Model::Classic,
            applied_base: None,
            patch: Vec::new(),
            vis: Visibility::default(),
            preview: Preview::new(),
            last_inputs: None,
            donor: None,
            applied_recipe: None,
            tones: Vec::new(),
            head_tone: None,
            skin_mask: Vec::new(),
            tex_result: None,
            tex_result_src: None,
            outfit_thumbs: None,
            rng: Rng::from_time(),
            next_uid: 0,
            last_preset: String::new(),
            changed: false,
        };
        m.choice[HANDS] = [Src::Same, Src::Same];
        m
    }

    fn push_slot(&mut self, at: Option<usize>, skin: Skin, model: Model, name: String, kind: SlotKind) -> Option<usize> {
        self.next_uid += 1;
        let slot = Slot { skin, model, name, kind, uid: self.next_uid, tex: None, tex_src: None };
        self.changed = true;
        match at {
            Some(i) if i < self.slots.len() => {
                self.slots[i] = slot;
                Some(i)
            }
            _ if self.slots.len() < MAX_SLOTS => {
                self.slots.push(slot);
                Some(self.slots.len() - 1)
            }
            _ => None,
        }
    }

    /// Fügt einen Skin hinzu (oder ersetzt Skin `at`). Gibt den Buchstaben zurück.
    pub fn set_slot(&mut self, at: Option<usize>, skin: Skin, model: Model, name: String) -> Option<char> {
        self.push_slot(at, skin, model, name, SlotKind::User).map(letter)
    }

    /// Projekt-Skin hinzufügen bzw. ersetzen.
    pub fn set_project_slot(&mut self, at: Option<usize>, skin: Skin, model: Model) -> Option<char> {
        self.push_slot(at, skin, model, t("Dein Skin").to_string(), SlotKind::Project).map(letter)
    }

    /// Beim ersten Öffnen: den eigenen Skin als A eintragen.
    pub fn ensure_project_slot(&mut self, skin: &Skin, model: Model) {
        if self.slots.is_empty() && skin.px.iter().any(|p| p[3] > 0) {
            self.set_project_slot(None, skin.clone(), model);
            self.changed = false;
        }
    }

    pub fn is_full(&self) -> bool {
        self.slots.len() >= MAX_SLOTS
    }

    /// Projekt-Skin im Mixer auf den Stand aus dem Editor bringen (nur solange noch nicht gemixt wurde).
    pub fn sync_project_slot(&mut self, skin: &Skin, model: Model) {
        if let Some(k) = self.project_slot() {
            let s = &mut self.slots[k];
            if s.skin != *skin || s.model != model {
                s.skin = skin.clone();
                s.model = model;
            }
        }
    }

    /// Die Mix-Einstellungen ohne den Projekt-Skin und die Editor-Änderungen
    fn recipe(&self) -> Option<Inputs> {
        let mut r = self.last_inputs.clone()?;
        for (skin, _, kind) in r.slots.iter_mut() {
            if *kind == SlotKind::Project {
                *skin = Skin::blank();
            }
        }
        r.patch.clear();
        Some(r)
    }

    /// Hat sich im Mixer etwas geändert, kommt das Ergebnis sofort in den Editor (ohne Übernehmen-Knopf).
    pub fn auto_apply(&mut self) -> Option<(Skin, Model)> {
        self.recompute();
        let recipe = self.recipe()?;
        let first = self.applied_recipe.is_none();
        if self.applied_recipe.as_ref() == Some(&recipe) {
            return None;
        }
        self.applied_recipe = Some(recipe);
        if first || self.slots.is_empty() {
            return None;
        }
        self.applied_base = Some(self.raw_result.clone());
        self.changed = true;
        Some((self.result.clone(), self.result_model))
    }

    fn project_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.kind == SlotKind::Project)
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
        self.changed = true;
    }

    /// Änderungen aus dem Projekt übernehmen (`skin` = aktueller Stand im Editor).
    pub fn sync_from_editor(&mut self, skin: &Skin) {
        let Some(base) = &self.applied_base else {
            self.patch.clear();
            return;
        };
        let patch: Vec<Option<Rgba>> =
            skin.px.iter().zip(&base.px).map(|(e, b)| if e != b { Some(*e) } else { None }).collect();
        if patch.iter().all(|p| p.is_none()) {
            self.patch.clear();
        } else if patch != self.patch {
            self.patch = patch;
        }
    }

    pub fn edited_pixels(&self) -> usize {
        self.patch.iter().filter(|p| p.is_some()).count()
    }

    // ------------------------------------------------------------- Vorlagen

    /// Der eigene Skin (Projekt-Skin), sonst A
    fn base_slot(&self) -> usize {
        self.project_slot().unwrap_or(0)
    }

    /// Von welchem Skin die Vorlagen Teile nehmen (nie der eigene Skin)
    fn donor_slot(&self) -> Option<usize> {
        let base = self.base_slot();
        self.donor
            .filter(|&d| d < self.slots.len() && d != base)
            .or_else(|| (0..self.slots.len()).find(|&i| i != base))
    }

    fn preset_partner(&mut self, head: u8, outfit: u8) {
        for p in Part::ALL {
            let s = Src::Slot(if p == Part::Head { head } else { outfit });
            self.choice[p.idx()] = [s, s];
        }
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.tone_transfer = true;
        self.last_preset = crate::i18n::t2("Partnerlook: Kopf {0} + Outfit {1}", letter(head as usize), letter(outfit as usize));
        self.changed = true;
    }

    /// `parts` kommen vom gewählten anderen Skin, alles andere vom eigenen Skin.
    fn preset_parts(&mut self, parts: &[Part], name: &str) {
        let Some(donor) = self.donor_slot() else { return };
        let base = self.base_slot();
        for p in Part::ALL {
            let s = Src::Slot(if parts.contains(&p) { donor } else { base } as u8);
            self.choice[p.idx()] = [s, s];
        }
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.last_preset = name.to_string();
        self.changed = true;
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
        self.changed = true;
    }

    /// Outfit-Vorlage anziehen: eigener Kopf + Outfit, Haut wird angepasst.
    pub fn apply_outfit(&mut self, id: &str) {
        let Some(skin) = crate::presets::make(id) else { return };
        let name = crate::presets::name_of(id).unwrap_or("Outfit").to_string();
        let existing = self.slots.iter().position(|s| s.kind == SlotKind::Outfit(id.to_string()));
        // eine andere Vorlage wird ersetzt, damit sich nicht immer mehr Skins ansammeln
        let other = self.slots.iter().position(|s| matches!(s.kind, SlotKind::Outfit(_)));
        let Some(k) = existing.or_else(|| self.push_slot(other, skin, Model::Classic, name.clone(), SlotKind::Outfit(id.to_string())))
        else {
            return;
        };
        let head = self.project_slot().or(if k == 0 { None } else { Some(0) });
        for p in Part::ALL {
            let s = match (p, head) {
                (Part::Head, Some(h)) => Src::Slot(h as u8),
                (Part::Head, None) => Src::Empty,
                _ => Src::Slot(k as u8),
            };
            self.choice[p.idx()] = [s, s];
        }
        self.choice[HANDS] = [Src::Same, Src::Same];
        self.tone_transfer = true;
        self.last_preset = t1("Outfit: {0}", name);
        self.changed = true;
    }

    // ------------------------------------------------------------- Berechnung

    fn slot_idx(&self, s: Src) -> Option<usize> {
        match s {
            Src::Slot(i) if (i as usize) < self.slots.len() => Some(i as usize),
            _ => None,
        }
    }

    fn palette_of(&self, slot: &Slot) -> Option<TonePalette> {
        match &slot.kind {
            SlotKind::Outfit(_) => Some(outfit_palette(&slot.skin)),
            _ => detect_palette(&slot.skin, slot.model),
        }
    }

    pub fn recompute(&mut self) {
        let inputs = Inputs {
            slots: self.slots.iter().map(|s| (s.skin.clone(), s.model, s.kind.clone())).collect(),
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
        self.tones = self.slots.iter().map(|s| self.palette_of(s)).collect();

        // Modell des Ergebnisses: das der Arme
        let arm_src = self.slot_idx(self.choice[Part::RightArm.idx()][0]);
        let auto_model = arm_src.or(if self.slots.is_empty() { None } else { Some(0) }).map(|i| self.slots[i].model);
        let dst_model = self.model_choice.or(auto_model).unwrap_or_default();

        let head_src = self.slot_idx(self.choice[Part::Head.idx()][0]);
        let head_pal: Option<TonePalette> =
            self.tone_override.map(TonePalette::from_color).or_else(|| head_src.and_then(|i| self.tones[i].clone()));
        self.head_tone = head_pal.clone();

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
        let mut mask = Vec::new();
        for idx in 0..SIZE * SIZE {
            let k = src_of[idx];
            let Some(tinfo) = map[idx] else { continue };
            if k >= FILLED || tinfo.part == Part::Head {
                continue;
            }
            let c = out.px[idx];
            if c[3] == 0 {
                continue;
            }
            let src_pal = self.tones[k as usize].as_ref();
            // Haut gibt es nur auf der 1. Ebene; die 2. Ebene ist Kleidung/Accessoires
            let skin_px = tinfo.layer == Layer::Base && src_pal.is_some_and(|p| is_skin(c, p, self.tone_tol));
            if skin_px {
                mask.push((idx % SIZE, idx / SIZE));
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
        self.skin_mask = mask;

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

    // ------------------------------------------------------------- Speichern

    pub fn save_to(&self, dir: &Path) -> Result<(), String> {
        let mdir = dir.join("mixer");
        let _ = std::fs::remove_dir_all(&mdir);
        std::fs::create_dir_all(&mdir).map_err(|e| e.to_string())?;
        let mut txt = String::new();
        for (i, s) in self.slots.iter().enumerate() {
            std::fs::write(mdir.join(format!("slot{i}.png")), s.skin.to_png()).map_err(|e| e.to_string())?;
            let kind = match &s.kind {
                SlotKind::User => "user".to_string(),
                SlotKind::Project => "project".to_string(),
                SlotKind::Outfit(id) => format!("outfit:{id}"),
            };
            let model = if s.model == Model::Slim { "s" } else { "c" };
            txt += &format!("slot={model}|{kind}|{}\n", s.name.replace('\n', " "));
        }
        let rows: Vec<String> = self.choice.iter().map(|r| format!("{},{}", r[0].code(), r[1].code())).collect();
        txt += &format!("choice={}\n", rows.join(";"));
        txt += &format!("hand_len={}\nsync={}\ntone={}\ntol={}\n", self.hand_len, self.sync_layers as u8, self.tone_transfer as u8, self.tone_tol);
        if let Some(c) = self.tone_override {
            txt += &format!("override={}\n", color::to_hex(c));
        }
        txt += &format!("preset={}\n", self.last_preset.replace('\n', " "));
        if let Some(b) = &self.applied_base {
            std::fs::write(mdir.join("base.png"), b.to_png()).map_err(|e| e.to_string())?;
        }
        std::fs::write(mdir.join("mixer.txt"), txt).map_err(|e| e.to_string())
    }

    pub fn load_from(dir: &Path) -> Option<Mixer> {
        let mdir = dir.join("mixer");
        let txt = std::fs::read_to_string(mdir.join("mixer.txt")).ok()?;
        let mut m = Mixer::new();
        let mut slot_i = 0;
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "slot" => {
                    let mut parts = v.splitn(3, '|');
                    let model = if parts.next() == Some("s") { Model::Slim } else { Model::Classic };
                    let kind = match parts.next().unwrap_or("user") {
                        "project" => SlotKind::Project,
                        k if k.starts_with("outfit:") => SlotKind::Outfit(k[7..].to_string()),
                        _ => SlotKind::User,
                    };
                    let name = parts.next().unwrap_or("Skin").to_string();
                    let png = std::fs::read(mdir.join(format!("slot{slot_i}.png"))).ok();
                    slot_i += 1;
                    if let Some(skin) = png.and_then(|b| Skin::from_png(&b).ok()) {
                        m.push_slot(None, skin, model, name, kind);
                    }
                }
                "choice" => {
                    for (r, row) in v.split(';').enumerate().take(7) {
                        let mut it = row.split(',');
                        m.choice[r] = [
                            Src::from_code(it.next().unwrap_or("S0")),
                            Src::from_code(it.next().unwrap_or("S0")),
                        ];
                    }
                }
                "hand_len" => m.hand_len = v.parse().unwrap_or(3),
                "sync" => m.sync_layers = v == "1",
                "tone" => m.tone_transfer = v == "1",
                "tol" => m.tone_tol = v.parse().unwrap_or(0.5),
                "override" => m.tone_override = color::from_hex(v),
                "preset" => m.last_preset = v.to_string(),
                _ => {}
            }
        }
        if let Ok(b) = std::fs::read(mdir.join("base.png")) {
            m.applied_base = Skin::from_png(&b).ok();
        }
        m.changed = false;
        Some(m)
    }

    // ------------------------------------------------------------- Oberfläche

    pub fn ui_inputs(&mut self, ui: &mut egui::Ui) -> MixerAction {
        let mut action = MixerAction::None;
        let ctx = ui.ctx().clone();
        heading(ui, &t1("SKINS ({0}/26)", self.slots.len()));
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
                    ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, letter(k), egui::FontId::proportional(14.0), Color32::BLACK);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), Sense::hover());
                    let tex = s.texture(&ctx).clone();
                    paint_skin_thumb(ui, rect, &tex);
                    ui.vertical(|ui| {
                        let name: String = if s.name.chars().count() > 15 {
                            format!("{}…", s.name.chars().take(14).collect::<String>())
                        } else {
                            s.name.clone()
                        };
                        let badge = match &s.kind {
                            SlotKind::Project => " 🏠",
                            SlotKind::Outfit(_) => " 👕",
                            SlotKind::User => "",
                        };
                        ui.label(RichText::new(format!("{name}{badge}")).strong()).on_hover_text(&s.name);
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
                                        .on_hover_text(t("Keine Hautfarbe gefunden – hier wird nichts umgefärbt."));
                                }
                            }
                            ui.add_space(4.0);
                            if ui.small_button("📂").on_hover_text(t("Durch andere PNG-Datei ersetzen")).clicked() {
                                action = MixerAction::Load(Some(k));
                            }
                            if ui.small_button("🏠").on_hover_text(t("Durch den Projekt-Skin ersetzen")).clicked() {
                                action = MixerAction::FromProject(Some(k));
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
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), Sense::hover());
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
            if ui.add_enabled(!self.is_full(), egui::Button::new(t("➕ Skins laden …"))).on_hover_text(t("Eine oder mehrere PNG-Dateien hinzufügen")).clicked() {
                action = MixerAction::Load(None);
            }
            if ui
                .add_enabled(!self.is_full(), egui::Button::new(t("🏠 Projekt-Skin")))
                .on_hover_text(t("Den aktuellen Skin dieses Projekts hinzufügen"))
                .clicked()
            {
                action = MixerAction::FromProject(None);
            }
        });

        // Outfit-Vorlagen
        ui.add_space(10.0);
        heading(ui, t("OUTFIT-VORLAGEN"));
        ui.label(RichText::new(t("Klick: dein Kopf + dieses Outfit (Hautfarbe wird angepasst)")).small().weak());
        ui.add_space(4.0);
        self.ensure_outfit_thumbs(&ctx);
        let thumbs: Vec<egui::TextureId> = self.outfit_thumbs.as_ref().map(|(_, v)| v.iter().map(|t| t.id()).collect()).unwrap_or_default();
        let w = ((ui.available_width() - 2.0 * 6.0) / 3.0).floor();
        let mut clicked = None;
        for row in crate::presets::OUTFITS.chunks(3).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (k, (id, name, _)) in row.1.iter().enumerate() {
                    let idx = row.0 * 3 + k;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w * 1.25 + 16.0), Sense::click());
                    let hovered = resp.hovered();
                    let p = ui.painter();
                    p.rect_filled(rect, 6.0, if hovered { Color32::from_rgb(56, 62, 74) } else { Color32::from_rgb(44, 48, 58) });
                    if hovered {
                        p.rect_stroke(rect, 6.0, egui::Stroke::new(1.5, crate::ORANGE), egui::StrokeKind::Inside);
                    }
                    if let Some(tex) = thumbs.get(idx) {
                        let img = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 2.0), egui::vec2(w - 8.0, w * 1.25 - 4.0));
                        p.image(*tex, img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    p.text(
                        rect.center_bottom() - egui::vec2(0.0, 9.0),
                        egui::Align2::CENTER_CENTER,
                        t(name),
                        egui::FontId::proportional(11.5),
                        Color32::WHITE,
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        clicked = Some(*id);
                    }
                }
            });
            ui.add_space(6.0);
        }
        if let Some(id) = clicked {
            self.apply_outfit(id);
        }
        action
    }

    /// Kleine 3D-Bilder der Vorlagen – mit dem Kopf des eigenen Skins.
    fn ensure_outfit_thumbs(&mut self, ctx: &egui::Context) {
        let head_skin = self
            .project_slot()
            .or(if self.slots.is_empty() { None } else { Some(0) })
            .map(|i| self.slots[i].skin.clone())
            .unwrap_or_else(skin::starter_skin);
        if self.outfit_thumbs.as_ref().is_some_and(|(h, _)| *h == head_skin) {
            return;
        }
        let head_pal = detect_palette(&head_skin, Model::Classic);
        let cam = render3d::Camera { yaw: -0.45, pitch: 0.15, zoom: 1.0 };
        let mut texs = Vec::new();
        for (k, (id, _, _)) in crate::presets::OUTFITS.iter().enumerate() {
            let mut s = crate::presets::make(id).unwrap_or_default();
            // Haut des Outfits in die Hautfarbe des Kopfes umrechnen
            if let Some(to) = &head_pal {
                let from = outfit_palette(&s);
                for c in s.px.iter_mut() {
                    if c[3] > 0 && is_skin(*c, &from, 0.5) {
                        *c = transfer_tone(*c, &from, to);
                    }
                }
            }
            skin::copy_part(&head_skin, Model::Classic, &mut s, Model::Classic, Part::Head, Layer::Base);
            skin::copy_part(&head_skin, Model::Classic, &mut s, Model::Classic, Part::Head, Layer::Overlay);
            let params = render3d::RenderParams {
                cam,
                model: Model::Classic,
                vis: Visibility::default(),
                highlight: vec![],
                width: 80,
                height: 100,
                supersample: 2,
                ghosts: false,
                grid: false,
            };
            let out = render3d::render(&s, &params);
            let img = ColorImage::from_rgba_premultiplied([out.width, out.height], &out.rgba);
            texs.push(ctx.load_texture(format!("outfit_thumb_{k}"), img, TextureOptions::LINEAR));
        }
        self.outfit_thumbs = Some((head_skin, texs));
    }

    /// Kleiner Pfeil über einer Spalte: klappt mit Animation die Liste aller Skins auf.
    /// Gibt den gewählten Skin zurück (gilt dann für die ganze Spalte außer dem Kopf).
    fn column_menu(&mut self, ui: &mut egui::Ui, layer: usize) -> Option<Src> {
        let id = ui.scope_id().with(("mix_col_menu", layer));
        let open = egui::Popup::is_id_open(ui.ctx(), id);
        let anim = ui.ctx().animate_bool_with_time(id.with("anim"), open, 0.18);
        // kleiner Pfeil, der sich beim Aufklappen umdreht
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 16.0), Sense::click());
        let p = ui.painter();
        let hovered = resp.hovered() || open;
        p.rect_filled(rect, 4.0, if hovered { Color32::from_rgb(70, 76, 90) } else { Color32::from_rgb(52, 56, 66) });
        let c = rect.center();
        let dir = 1.0 - 2.0 * anim; // 1 = nach unten, -1 = nach oben
        let col = if hovered { crate::ORANGE } else { Color32::from_gray(200) };
        p.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(-4.0, -2.5 * dir), c + egui::vec2(4.0, -2.5 * dir), c + egui::vec2(0.0, 3.0 * dir)],
            col,
            egui::Stroke::NONE,
        ));
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let ctx = ui.ctx().clone();
        let mut options: Vec<Src> = (0..self.slots.len()).map(|i| Src::Slot(i as u8)).collect();
        options.push(Src::Empty);
        let mut picked = None;
        egui::Popup::from_toggle_button_response(&resp)
            .id(id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let row_h = 28.0;
                let full = options.len() as f32 * (row_h + 2.0);
                // Ausklapp-Animation: sichtbarer Bereich wächst von 0 auf die volle Liste
                let (area, _) = ui.allocate_exact_size(egui::vec2(190.0, (full * anim).max(1.0)), Sense::hover());
                let p = ui.painter().with_clip_rect(area);
                for (k, &o) in options.iter().enumerate() {
                    let rect = egui::Rect::from_min_size(area.min + egui::vec2(0.0, k as f32 * (row_h + 2.0)), egui::vec2(190.0, row_h));
                    if rect.min.y >= area.max.y {
                        break;
                    }
                    let r = ui.interact(rect.intersect(area), id.with(("row", k)), Sense::click());
                    if r.hovered() {
                        p.rect_filled(rect, 4.0, Color32::from_white_alpha(18));
                    }
                    let badge = rect.left_center() + egui::vec2(14.0, 0.0);
                    match o {
                        Src::Slot(i) => {
                            let i = i as usize;
                            p.circle_filled(badge, 10.0, letter_color(i));
                            p.text(badge, egui::Align2::CENTER_CENTER, letter(i), egui::FontId::proportional(12.0), Color32::BLACK);
                            // Gesicht des Skins
                            let tex = self.slots[i].texture(&ctx).id();
                            let face = egui::Rect::from_center_size(rect.left_center() + egui::vec2(42.0, 0.0), egui::vec2(20.0, 20.0));
                            let uv = egui::Rect::from_min_max(egui::pos2(8.0 / 64.0, 8.0 / 64.0), egui::pos2(16.0 / 64.0, 16.0 / 64.0));
                            p.image(tex, face, uv, Color32::WHITE);
                            let name: String = self.slots[i].name.chars().take(16).collect();
                            p.text(rect.left_center() + egui::vec2(60.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), ui.visuals().text_color());
                        }
                        _ => {
                            p.circle_stroke(badge, 9.0, egui::Stroke::new(1.0, Color32::from_gray(140)));
                            p.text(badge, egui::Align2::CENTER_CENTER, "–", egui::FontId::proportional(12.0), Color32::from_gray(160));
                            p.text(rect.left_center() + egui::vec2(34.0, 0.0), egui::Align2::LEFT_CENTER, t("leer"), egui::FontId::proportional(13.0), ui.visuals().weak_text_color());
                        }
                    }
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        picked = Some(o);
                    }
                }
            });
        if picked.is_some() {
            egui::Popup::close_id(&ctx, id);
        }
        picked
    }

    /// Auswahl der Quelle (A, B, C …) für ein Körperteil. Gibt `true` zurück, wenn geändert.
    fn src_picker(ui: &mut egui::Ui, value: &mut Src, names: &[String], hands: bool, base: bool, id: (usize, usize)) -> bool {
        let n = names.len();
        let before = *value;
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
        if options.len() <= 2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for o in options {
                    ui.selectable_value(value, o, label(o)).on_hover_text(tip(o));
                }
            });
        } else {
            egui::ComboBox::from_id_salt(("mix_src", id.0, id.1))
                .width(52.0)
                .selected_text(RichText::new(label(*value)).strong())
                .show_ui(ui, |ui| {
                    for o in options {
                        ui.selectable_value(value, o, format!("{}  ·  {}", label(o), tip(o)));
                    }
                });
        }
        *value != before
    }

    pub fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let n = self.slots.len();
        let names: Vec<String> = self.slots.iter().map(|s| s.name.clone()).collect();
        heading(ui, t("VORLAGEN"));
        let base = self.base_slot();
        let donor = self.donor_slot();
        let two = donor.is_some();
        if let Some(d) = donor {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Teile nehmen von:")).small());
                egui::ComboBox::from_id_salt("mix_donor")
                    .selected_text(format!("{} · {}", letter(d), names.get(d).cloned().unwrap_or_default()))
                    .show_ui(ui, |ui| {
                        for i in (0..n).filter(|&i| i != base) {
                            if ui.selectable_label(i == d, format!("{} · {}", letter(i), names[i])).clicked() {
                                self.donor = Some(i);
                            }
                        }
                    });
            });
        }
        let dl = donor.map(letter).unwrap_or('B');
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(two, egui::Button::new(t1("💑 Mein Kopf + Outfit von {0}", dl))).clicked() {
                if let Some(d) = donor {
                    self.preset_partner(base as u8, d as u8);
                }
            }
            if ui.add_enabled(two, egui::Button::new(t1("💑 Kopf von {0} + mein Outfit", dl))).clicked() {
                if let Some(d) = donor {
                    self.preset_partner(d as u8, base as u8);
                }
            }
            if ui.add_enabled(two, egui::Button::new(t1("👕 Oberteil von {0}", dl))).clicked() {
                self.preset_parts(&[Part::Body, Part::RightArm, Part::LeftArm], &t1("Oberteil von {0}", dl));
            }
            if ui.add_enabled(two, egui::Button::new(t1("👖 Hose von {0}", dl))).clicked() {
                self.preset_parts(&[Part::RightLeg, Part::LeftLeg], &t1("Hose von {0}", dl));
            }
            if ui.add_enabled(two, egui::Button::new(t1("🎩 Frisur/Hut von {0}", dl))).clicked() {
                self.preset_parts(&[], &t1("Frisur/Hut von {0}", dl));
                if let Some(d) = donor {
                    self.choice[Part::Head.idx()][1] = Src::Slot(d as u8);
                    self.sync_layers = false;
                }
            }
            if ui.add_enabled(n >= 1, egui::Button::new(t("🎲 Zufallsmix"))).clicked() {
                self.preset_random();
            }
        });

        ui.add_space(6.0);
        heading(ui, t("KÖRPERTEILE"));
        if ui.checkbox(&mut self.sync_layers, t("1. und 2. Ebene gleich")).on_hover_text(t("Wählst du bei der 1. Ebene einen Skin, bekommt die 2. Ebene automatisch denselben.")).changed() {
            self.changed = true;
        }
        // Pfeile oben: eine ganze Spalte (außer Kopf) auf einen Skin setzen
        let mut col_pick: Option<(usize, Src)> = None;
        egui::Grid::new("mix_parts").num_columns(3).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
            ui.label("");
            for layer in 0..2 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(RichText::new(if layer == 0 { t("1. Ebene") } else { t("2. Ebene") }).small());
                    if let Some(src) = self.column_menu(ui, layer) {
                        col_pick = Some((layer, src));
                    }
                });
            }
            ui.end_row();
            for p in Part::ALL {
                ui.label(p.name());
                let row = p.idx();
                if Self::src_picker(ui, &mut self.choice[row][0], &names, false, true, (row, 0)) {
                    self.changed = true;
                    if self.sync_layers {
                        self.choice[row][1] = self.choice[row][0].for_overlay();
                    }
                }
                if self.sync_layers {
                    // 2. Ebene folgt der 1. Ebene – nur anzeigen
                    ui.add_enabled_ui(false, |ui| {
                        let mut v = self.choice[row][1];
                        Self::src_picker(ui, &mut v, &names, false, false, (row, 1));
                    });
                } else if Self::src_picker(ui, &mut self.choice[row][1], &names, false, false, (row, 1)) {
                    self.changed = true;
                }
                ui.end_row();
            }
            ui.label(RichText::new(t("✋ Hände")).strong()).on_hover_text(t("Unterer Teil beider Arme"));
            if Self::src_picker(ui, &mut self.choice[HANDS][0], &names, true, true, (HANDS, 0)) {
                self.changed = true;
                if self.sync_layers {
                    self.choice[HANDS][1] = self.choice[HANDS][0].for_overlay();
                }
            }
            if self.sync_layers {
                ui.add_enabled_ui(false, |ui| {
                    let mut v = self.choice[HANDS][1];
                    Self::src_picker(ui, &mut v, &names, true, false, (HANDS, 1));
                });
            } else if Self::src_picker(ui, &mut self.choice[HANDS][1], &names, true, false, (HANDS, 1)) {
                self.changed = true;
            }
            ui.end_row();
        });
        if let Some((layer, src)) = col_pick {
            for p in Part::ALL {
                if p == Part::Head {
                    continue;
                }
                let row = &mut self.choice[p.idx()];
                if layer == 0 {
                    row[0] = src;
                    if self.sync_layers {
                        row[1] = src.for_overlay();
                    }
                } else {
                    row[1] = src;
                }
            }
            // eine eigene Wahl für die 2. Ebene hebt die Kopplung auf
            if layer == 1 {
                self.sync_layers = false;
            }
            self.changed = true;
        }
        ui.horizontal(|ui| {
            ui.label(t("Handlänge"));
            if ui.add(egui::Slider::new(&mut self.hand_len, 1..=8).suffix(t(" px"))).on_hover_text(t("Wie viele Pixel unten am Arm als Hand zählen")).changed() {
                self.changed = true;
            }
        });
        if ui.small_button(t("✋ Hände in Hautfarbe")).on_hover_text(t("Hände in der Hautfarbe des Kopfes einfärben")).clicked() {
            self.choice[HANDS] = [Src::SkinTone, Src::Empty];
            self.changed = true;
        }

        ui.add_space(8.0);
        heading(ui, t("HAUTFARBE"));
        ui.checkbox(&mut self.tone_transfer, t("Hautfarbe automatisch angleichen")).on_hover_text(t(
            "Erkennt die Hautfarbe jedes Skins mit allen Schattierungen und färbt sichtbare Haut (Hände, Hals, Beine …), die von einem anderen Skin kommt, passend zum Kopf um.",
        ));
        ui.horizontal(|ui| {
            ui.label(t("Kopf:"));
            match &self.head_tone {
                Some(p) => {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 14.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, c32(p.main));
                    ui.label(RichText::new(color::to_hex(p.main)).small().weak());
                }
                None => {
                    ui.label(RichText::new(t("nicht erkannt – wähle „Eigene Hautfarbe“")).small().color(crate::ORANGE));
                }
            }
        });
        if self.tone_transfer {
            ui.horizontal(|ui| {
                ui.label(t("Toleranz"));
                ui.add(egui::Slider::new(&mut self.tone_tol, 0.0..=1.0).show_value(false))
                    .on_hover_text(t("Höher = auch stärker abweichende Hauttöne werden erkannt"));
            });
            ui.checkbox(&mut self.show_skin, t("Erkannte Haut anzeigen")).on_hover_text(t("Markiert im Ergebnis alle Pixel, die als Haut erkannt wurden."));
        }
        ui.horizontal(|ui| {
            let mut manual = self.tone_override.is_some();
            if ui.checkbox(&mut manual, t("Eigene Hautfarbe")).changed() {
                let detected = self.head_tone.as_ref().map(|p| p.main);
                self.tone_override = if manual { detected.or(Some([214, 164, 130, 255])) } else { None };
                self.changed = true;
            }
            if let Some(tc) = &mut self.tone_override {
                let mut c = c32(*tc);
                if egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque).changed() {
                    *tc = rgba(c);
                    self.changed = true;
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
                    RichText::new(t("👫  Wähle links eine Outfit-Vorlage oder lade Skins,\num sie zu kombinieren."))
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
            let edits = self.edited_pixels();
            if edits > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(t1("✏ {0} Pixel aus dem Editor übernommen", edits)).small().color(crate::ORANGE))
                        .on_hover_text(t("Was du im Editor an diesem Skin änderst, wird hier automatisch übernommen."));
                });
            }
        });
        let avail = ui.available_size();
        let thumb = 148.0;
        let size = egui::vec2(avail.x, (avail.y - 56.0).max(150.0));
        let res_skin = self.result.clone();
        let highlight: Vec<(usize, usize)> = if self.show_skin && self.tone_transfer { self.skin_mask.clone() } else { vec![] };
        let inp = self.preview.show(ui, size, &res_skin, self.result_model, self.vis, &highlight, false);
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

/// Farbton im Lab-Raum (0–360°)
fn lab_hue(lab: [f32; 3]) -> f32 {
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
}

fn hue_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Kommt diese Farbe als Hautfarbe infrage? (von sehr hell bis sehr dunkel, auch rosig)
fn skin_like(lab: [f32; 3]) -> bool {
    let ch = color::chroma(lab);
    let h = lab_hue(lab);
    (12.0..=97.0).contains(&lab[0]) && (4.0..=70.0).contains(&ch) && (h <= 95.0 || h >= 340.0)
}

/// Abstand, bei dem Helligkeitsunterschiede (Schattierung) weniger zählen als Farbunterschiede.
fn shade_dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dl = (a[0] - b[0]) * 0.4;
    (dl * dl + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Sammelt hautähnliche Farben in einem Bereich (Fläche, Zeilenbereich, Gewicht).
fn collect(s: &Skin, model: Model, part: Part, zones: &[(Face, std::ops::Range<usize>, f32)], out: &mut Vec<([f32; 3], f32)>) {
    let b = skin::box_of(model, part, Layer::Base);
    for (face, rows, weight) in zones {
        let (fx, fy, fw, fh) = b.face_rect(*face);
        for j in rows.clone() {
            if j >= fh {
                continue;
            }
            for i in 0..fw {
                let c = s.get(fx + i, fy + j);
                if c[3] < 255 {
                    continue;
                }
                let lab = color::to_lab(c);
                if skin_like(lab) {
                    // Gesichtsmitte (Wangen, Nase) zählt am meisten
                    let center = *face == Face::Front && part == Part::Head && (1..=6).contains(&i) && (3..=7).contains(&j);
                    out.push((lab, weight * if center { 1.8 } else { 1.0 }));
                }
            }
        }
    }
}

fn palette_from(cand: &[([f32; 3], f32)], min_weight: f32) -> Option<TonePalette> {
    if cand.len() < 5 {
        return None;
    }
    // Hauptton: Farbe, um die herum die meisten (gewichteten) Kandidaten liegen
    let mut best = (0usize, -1.0f32);
    for (i, (a, _)) in cand.iter().enumerate() {
        let score: f32 = cand.iter().filter(|(b, _)| shade_dist(*a, *b) < 9.0).map(|(_, w)| *w).sum();
        if score > best.1 {
            best = (i, score);
        }
    }
    if best.1 < min_weight {
        return None;
    }
    let seed = cand[best.0].0;
    let near: Vec<&[f32; 3]> = cand.iter().map(|(l, _)| l).filter(|l| shade_dist(**l, seed) < 7.0).collect();
    let n = near.len().max(1) as f32;
    let main_lab = [
        near.iter().map(|l| l[0]).sum::<f32>() / n,
        near.iter().map(|l| l[1]).sum::<f32>() / n,
        near.iter().map(|l| l[2]).sum::<f32>() / n,
    ];
    // Alle Schattierungen derselben Hautfarbe einsammeln
    let main_h = lab_hue(main_lab);
    let mut shades: Vec<[f32; 3]> = cand
        .iter()
        .map(|(l, _)| *l)
        .filter(|l| shade_dist(*l, main_lab) < 15.0 && hue_dist(lab_hue(*l), main_h) < 22.0)
        .collect();
    shades.sort_by(|a, b| a[0].total_cmp(&b[0]));
    shades.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.8 && (a[1] - b[1]).abs() < 0.8 && (a[2] - b[2]).abs() < 0.8);
    Some(TonePalette { main: color::from_lab(main_lab, 255), exact: Vec::new(), main_lab, shades })
}

/// Ermittelt die Hautfarbe eines Skins: zuerst am Gesicht, sonst an Händen und Hals.
/// Hautkandidaten im Gesicht. Haare werden herausgerechnet: Farben, die oben auf dem Kopf
/// vorkommen, sind fast immer Haare oder ein Hut – genau wie Pixel, die von der 2. Ebene verdeckt sind.
fn collect_face(s: &Skin, model: Model, out: &mut Vec<([f32; 3], f32)>) {
    let base = skin::box_of(model, Part::Head, Layer::Base);
    let over = skin::box_of(model, Part::Head, Layer::Overlay);
    // Haarfarben: Oberseite des Kopfes (beide Ebenen) und obere Reihen hinten
    let mut hair: Vec<[f32; 3]> = Vec::new();
    for (b, face, rows) in [(&base, Face::Top, 0..8), (&over, Face::Top, 0..8), (&base, Face::Back, 0..3), (&over, Face::Back, 0..3)] {
        let (fx, fy, fw, fh) = b.face_rect(face);
        for j in rows.filter(|&j| j < fh) {
            for i in 0..fw {
                let c = s.get(fx + i, fy + j);
                if c[3] == 255 {
                    hair.push(color::to_lab(c));
                }
            }
        }
    }
    let hair_like = |lab: [f32; 3]| hair.iter().filter(|h| shade_dist(**h, lab) < 9.0).count() >= 3;
    let top_is_skin_colored = {
        // Glatze/kurze Haare in Hautfarbe: dann nicht bestrafen
        let (fx, fy, _, _) = base.face_rect(Face::Front);
        let cheek = color::to_lab(s.get(fx + 1, fy + 5));
        hair.iter().filter(|h| shade_dist(**h, cheek) < 6.0).count() > hair.len() / 2
    };
    for face in [Face::Front, Face::Right, Face::Left, Face::Bottom] {
        let (fx, fy, fw, fh) = base.face_rect(face);
        let (ox, oy, _, _) = over.face_rect(face);
        for j in 0..fh {
            for i in 0..fw {
                let c = s.get(fx + i, fy + j);
                if c[3] < 255 {
                    continue;
                }
                // von der 2. Ebene verdeckt (Haare, Brille, Maske …) → nicht sichtbar
                if s.get(ox + i, oy + j)[3] == 255 {
                    continue;
                }
                let lab = color::to_lab(c);
                if !skin_like(lab) {
                    continue;
                }
                // Wangen und Nase zählen am meisten, Stirn (oft Haare) kaum
                let mut w = match face {
                    Face::Front => match j {
                        0..=1 => 0.2,
                        2..=3 => 0.6,
                        4 => 1.2,
                        5..=6 => 3.0,
                        _ => 1.2,
                    },
                    Face::Bottom => 0.6,
                    _ => {
                        if j >= 3 {
                            0.6
                        } else {
                            0.1
                        }
                    }
                };
                if face == Face::Front && (1..=6).contains(&i) && (5..=6).contains(&j) {
                    w *= 1.5;
                }
                if !top_is_skin_colored && hair_like(lab) {
                    w *= 0.1;
                }
                out.push((lab, w));
            }
        }
    }
}

pub fn detect_palette(s: &Skin, model: Model) -> Option<TonePalette> {
    let mut cand = Vec::new();
    collect_face(s, model, &mut cand);
    if let Some(p) = palette_from(&cand, 6.0) {
        // Hände/Hals mit dazunehmen, damit auch deren Schattierungen erkannt werden
        let mut extra = cand.clone();
        let hand_zone = [(Face::Front, 9..12, 0.5), (Face::Back, 9..12, 0.5), (Face::Right, 9..12, 0.5), (Face::Left, 9..12, 0.5), (Face::Bottom, 0..4, 0.5)];
        collect(s, model, Part::RightArm, &hand_zone, &mut extra);
        collect(s, model, Part::LeftArm, &hand_zone, &mut extra);
        let main_h = lab_hue(p.main_lab);
        let mut shades = p.shades.clone();
        for (l, _) in extra {
            if shade_dist(l, p.main_lab) < 15.0 && hue_dist(lab_hue(l), main_h) < 22.0 {
                shades.push(l);
            }
        }
        shades.sort_by(|a, b| a[0].total_cmp(&b[0]));
        shades.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.8 && (a[1] - b[1]).abs() < 0.8 && (a[2] - b[2]).abs() < 0.8);
        return Some(TonePalette { shades, ..p });
    }
    // Kein Gesicht erkannt (Maske, Helm …): Hände und Hals versuchen
    let mut cand = Vec::new();
    let hand_zone = [(Face::Front, 9..12, 1.0), (Face::Back, 9..12, 1.0), (Face::Right, 9..12, 1.0), (Face::Left, 9..12, 1.0), (Face::Bottom, 0..4, 1.0)];
    collect(s, model, Part::RightArm, &hand_zone, &mut cand);
    collect(s, model, Part::LeftArm, &hand_zone, &mut cand);
    collect(s, model, Part::Body, &[(Face::Front, 0..1, 1.0)], &mut cand);
    palette_from(&cand, 8.0)
}

/// Palette einer Outfit-Vorlage: genau die Platzhalter-Hauttöne samt Licht/Schatten.
fn outfit_palette(_s: &Skin) -> TonePalette {
    let ramp = crate::presets::skin_ramp();
    let mut p = TonePalette::from_color(crate::presets::SKIN_PH);
    p.shades = ramp.iter().map(|c| color::to_lab(*c)).collect();
    p.exact = ramp.to_vec();
    p
}

/// Gehört diese Farbe zur Haut des Skins (inkl. Schattierungen)?
fn is_skin(c: Rgba, pal: &TonePalette, tol: f32) -> bool {
    if !pal.exact.is_empty() {
        return pal.exact.contains(&c);
    }
    let lab = color::to_lab(c);
    // Weiß, Grau, Schwarz sind keine Haut (Haut hat immer etwas Farbe)
    if color::chroma(lab) < (color::chroma(pal.main_lab) * 0.35).max(3.0) {
        return false;
    }
    if hue_dist(lab_hue(lab), lab_hue(pal.main_lab)) > 18.0 + 22.0 * tol {
        return false;
    }
    let near_shade = pal.shades.iter().any(|s| shade_dist(lab, *s) < 4.0 + 9.0 * tol);
    let near_main = shade_dist(lab, pal.main_lab) < 9.0 + 14.0 * tol && (lab[0] - pal.main_lab[0]).abs() < 28.0 + 20.0 * tol;
    near_shade || near_main
}

/// Färbt eine Hautfarbe um: Die Helligkeit wird über die Schattierungs-Spanne abgebildet
/// (dunkelster Ton → dunkelster Ton), Farbton und Sättigung kommen von der Zielhaut.
fn transfer_tone(c: Rgba, from: &TonePalette, to: &TonePalette) -> Rgba {
    let p = color::to_lab(c);
    let (flo, fhi) = from.l_range();
    let (tlo, thi) = to.l_range();
    let r = (p[0] - flo) / (fhi - flo);
    let l = (tlo + r * (thi - tlo)).clamp(2.0, 98.0);
    // nächste Zielschattierung bei dieser Helligkeit → deren Farbstich übernehmen
    let target = to
        .shades
        .iter()
        .min_by(|a, b| (a[0] - l).abs().total_cmp(&(b[0] - l).abs()))
        .copied()
        .unwrap_or(to.main_lab);
    let tint = [target[1] * 0.6 + to.main_lab[1] * 0.4, target[2] * 0.6 + to.main_lab[2] * 0.4];
    let a = tint[0] + (p[1] - from.main_lab[1]) * 0.4;
    let b = tint[1] + (p[2] - from.main_lab[2]) * 0.4;
    color::from_lab([l, a, b], c[3])
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Skin, dessen Körper und Beine eine feste Farbe haben
    fn colored(c: Rgba) -> Skin {
        let mut s = skin::starter_skin();
        for p in [Part::Body, Part::RightArm, Part::LeftArm, Part::RightLeg, Part::LeftLeg] {
            let b = skin::box_of(Model::Classic, p, Layer::Base);
            for face in Face::ALL {
                let (fx, fy, fw, fh) = b.face_rect(face);
                for j in 0..fh {
                    for i in 0..fw {
                        s.set(fx + i, fy + j, c);
                    }
                }
            }
        }
        s
    }

    fn color_of(m: &Mixer, part: Part) -> Rgba {
        let (fx, fy, _, _) = skin::box_of(Model::Classic, part, Layer::Base).face_rect(Face::Front);
        m.result.get(fx + 1, fy + 1)
    }

    #[test]
    fn haare_sind_keine_haut() {
        let hair: Rgba = [0x4B, 0x39, 0x30, 255];
        let hair_dark: Rgba = [0x3A, 0x2B, 0x24, 255];
        let skin_c: Rgba = [0xE0, 0xAE, 0x8C, 255];
        let skin_d: Rgba = [0xD0, 0x9C, 0x7A, 255];
        let mut s = Skin::blank();
        let b = skin::box_of(Model::Classic, Part::Head, Layer::Base);
        for face in Face::ALL {
            let (fx, fy, fw, fh) = b.face_rect(face);
            for j in 0..fh {
                for i in 0..fw {
                    let hairy = match face {
                        Face::Top | Face::Back => true,
                        Face::Front => j <= 2 || (j <= 4 && (i == 0 || i == 7)),
                        Face::Bottom => false,
                        _ => j <= 5,
                    };
                    let c = if hairy {
                        if (i + j) % 3 == 0 { hair_dark } else { hair }
                    } else if (i + j) % 4 == 0 {
                        skin_d
                    } else {
                        skin_c
                    };
                    s.set(fx + i, fy + j, c);
                }
            }
        }
        // Augen
        let (fx, fy, _, _) = b.face_rect(Face::Front);
        for (i, c) in [(1, [255, 255, 255, 255]), (2, [40, 90, 200, 255]), (5, [40, 90, 200, 255]), (6, [255, 255, 255, 255])] {
            s.set(fx + i, fy + 4, c);
        }
        let pal = detect_palette(&s, Model::Classic).expect("Haut gefunden");
        let l = color::to_lab(pal.main)[0];
        assert!(l > 65.0, "Hauptton sollte die helle Haut sein, nicht die Haare: {:?}", pal.main);
    }

    #[test]
    fn oberteil_und_hose_vom_anderen_skin() {
        let other = [10, 200, 10, 255];
        let mine = [200, 10, 10, 255];
        let mut m = Mixer::new();
        // der andere Skin steht auf A, der eigene auf B
        m.set_slot(None, colored(other), Model::Classic, "Anderer".into());
        m.set_project_slot(None, colored(mine), Model::Classic);
        m.tone_transfer = false;

        m.preset_parts(&[Part::Body, Part::RightArm, Part::LeftArm], "Oberteil");
        m.recompute();
        assert_eq!(color_of(&m, Part::Body), other, "Oberteil kommt vom anderen Skin");
        assert_eq!(color_of(&m, Part::RightLeg), mine, "Hose bleibt die eigene");

        m.preset_parts(&[Part::RightLeg, Part::LeftLeg], "Hose");
        m.recompute();
        assert_eq!(color_of(&m, Part::Body), mine, "Oberteil bleibt das eigene");
        assert_eq!(color_of(&m, Part::LeftLeg), other, "Hose kommt vom anderen Skin");
    }
}

