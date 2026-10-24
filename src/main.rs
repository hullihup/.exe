#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Skin Studio – ein schlanker Minecraft-Skin-Editor mit Skin-Mixer.

mod color;
mod editor;
mod icon_gen;
mod mixer;
mod preview;
mod project;
mod render3d;
mod skin;
mod ui_util;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Color32, Key, Modifiers, Rect, RichText, Sense, Stroke, StrokeKind};

use editor::{detect_model, Editor};
use mixer::{Mixer, MixerAction};
use project::ProjectMeta;
use skin::{Layer, Model, Skin};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Start,
    Editor,
    Mixer,
}

/// Was nach der Frage „Projekt speichern?“ passieren soll.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Close,
    Home,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartWith {
    Figure,
    Blank,
    File,
}

struct NewDialog {
    name: String,
    start: StartWith,
    model: Model,
}

struct ProjectCard {
    meta: ProjectMeta,
    thumb: egui::TextureHandle,
}

struct App {
    screen: Screen,
    editor: Editor,
    mixer: Mixer,
    project: Option<ProjectMeta>,
    cards: Option<Vec<ProjectCard>>,
    new_dialog: Option<NewDialog>,
    rename: Option<(String, String)>,
    delete_confirm: Option<(String, String)>,
    pending: Option<Pending>,
    allow_close: bool,
    toast: Option<(String, f64, bool)>,
    show_help: bool,
    title: String,
    logo: Option<egui::TextureHandle>,
}

const ORANGE: Color32 = Color32::from_rgb(242, 140, 40);

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let mut app = App {
            screen: Screen::Start,
            editor: Editor::new(),
            mixer: Mixer::new(),
            project: None,
            cards: None,
            new_dialog: None,
            rename: None,
            delete_confirm: None,
            pending: None,
            allow_close: false,
            toast: None,
            show_help: false,
            title: String::new(),
            logo: None,
        };
        // Datei per Kommandozeile / „Öffnen mit“: eine Datei → neues Projekt, mehrere → Mixer
        let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
        if args.len() == 1 {
            if let Err(e) = app.project_from_file(&args[0]) {
                // Programmzeit beginnt bei 0 → Meldung die ersten 5 Sekunden zeigen
                app.toast = Some((e, 5.0, true));
            }
        } else if args.len() > 1 {
            for p in &args {
                if let Ok((s, m)) = Self::read_skin(p) {
                    app.mixer.set_slot(None, s, m, Self::file_stem(p));
                }
            }
            app.screen = Screen::Mixer;
        }
        app
    }

    fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>, error: bool) {
        let t = ctx.input(|i| i.time);
        self.toast = Some((text.into(), t + if error { 5.0 } else { 2.6 }, error));
    }

    // ------------------------------------------------------------------ Dateien

    fn read_skin(path: &Path) -> Result<(Skin, Model), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Datei konnte nicht gelesen werden: {e}"))?;
        let s = Skin::from_png(&bytes)?;
        let m = detect_model(&s);
        Ok((s, m))
    }

    fn file_name(path: &Path) -> String {
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Skin".into())
    }

    fn file_stem(path: &Path) -> String {
        path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Skin".into())
    }

    fn pick_png() -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("Minecraft-Skin (PNG)", &["png"]).set_title("Skin öffnen").pick_file()
    }

    fn save_png_dialog(default_name: &str, title: &str) -> Option<PathBuf> {
        let mut p = rfd::FileDialog::new()
            .add_filter("PNG-Bild", &["png"])
            .set_title(title)
            .set_file_name(default_name)
            .save_file()?;
        if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("png")) {
            p.set_extension("png");
        }
        Some(p)
    }

    fn write_file(&mut self, ctx: &egui::Context, path: &Path, data: &[u8]) {
        match std::fs::write(path, data) {
            Ok(_) => self.notify(ctx, format!("✔ Exportiert: {}", Self::file_name(path)), false),
            Err(e) => self.notify(ctx, format!("Exportieren fehlgeschlagen: {e}"), true),
        }
    }

    fn safe_file_name(name: &str) -> String {
        let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        format!("{}.png", if s.is_empty() { "skin".into() } else { s })
    }

    fn export_editor(&mut self, ctx: &egui::Context) {
        let name = self.project.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "skin".into());
        if let Some(p) = Self::save_png_dialog(&Self::safe_file_name(&name), "Skin als PNG exportieren") {
            let mut s = self.editor.skin.clone();
            s.clean_unused(self.editor.model);
            self.write_file(ctx, &p, &s.to_png());
        }
    }

    fn export_mix(&mut self, ctx: &egui::Context) {
        if let Some(p) = Self::save_png_dialog("partnerlook_skin.png", "Gemixten Skin exportieren") {
            let data = self.mixer.result.to_png();
            self.write_file(ctx, &p, &data);
        }
    }

    fn action_export(&mut self, ctx: &egui::Context) {
        match self.screen {
            Screen::Editor => self.export_editor(ctx),
            Screen::Mixer => self.export_mix(ctx),
            Screen::Start => {}
        }
    }

    fn export_render(&mut self, ctx: &egui::Context) {
        let (skin, model, prev, vis) = match self.screen {
            Screen::Mixer => (&self.mixer.result, self.mixer.result_model, &self.mixer.preview, self.mixer.vis),
            _ => (&self.editor.skin, self.editor.model, &self.editor.preview, self.editor.vis),
        };
        let (w, h) = (900, 1200);
        let rgba = prev.render_image(skin, model, vis, w, h);
        if let Some(p) = Self::save_png_dialog("skin_vorschau.png", "3D-Ansicht als Bild speichern") {
            let data = skin::encode_png(w as u32, h as u32, &rgba);
            self.write_file(ctx, &p, &data);
        }
    }

    /// PNG in das geöffnete Projekt laden (rückgängig machbar).
    fn import_into_project(&mut self, ctx: &egui::Context) {
        if let Some(p) = Self::pick_png() {
            match Self::read_skin(&p) {
                Ok((s, m)) => {
                    self.editor.replace_skin(s, m);
                    self.notify(ctx, format!("Importiert: {} (Strg+Z macht es rückgängig)", Self::file_name(&p)), false);
                }
                Err(e) => self.notify(ctx, e, true),
            }
        }
    }

    // ------------------------------------------------------------------ Projekte

    fn open_project_with(&mut self, meta: ProjectMeta, skin: Skin) {
        self.editor.open_fresh(skin, meta.model);
        self.editor.preview.cam = Default::default();
        project::set_last(&meta.id);
        self.project = Some(meta);
        self.screen = Screen::Editor;
    }

    fn create_project(&mut self, name: String, skin: Skin, model: Model) -> Result<(), String> {
        let meta = ProjectMeta { id: project::new_id(), name, model, modified: project::now() };
        project::save(&meta, &skin)?;
        self.open_project_with(meta, skin);
        self.cards = None;
        Ok(())
    }

    fn project_from_file(&mut self, path: &Path) -> Result<(), String> {
        let (s, m) = Self::read_skin(path)?;
        self.create_project(Self::file_stem(path), s, m)
    }

    fn open_project(&mut self, ctx: &egui::Context, id: &str) {
        match project::load(id) {
            Some(p) => self.open_project_with(p.meta, p.skin),
            None => {
                self.notify(ctx, "Projekt konnte nicht geladen werden.", true);
                self.cards = None;
            }
        }
    }

    fn save_project(&mut self, ctx: &egui::Context) -> bool {
        let Some(meta) = &mut self.project else { return true };
        meta.model = self.editor.model;
        meta.modified = project::now();
        let meta = meta.clone();
        match project::save(&meta, &self.editor.skin) {
            Ok(_) => {
                self.editor.modified = false;
                self.cards = None;
                self.notify(ctx, format!("✔ „{}“ gespeichert", meta.name), false);
                true
            }
            Err(e) => {
                self.notify(ctx, e, true);
                false
            }
        }
    }

    fn has_unsaved(&self) -> bool {
        self.project.is_some() && self.editor.modified
    }

    fn go_home(&mut self) {
        if self.has_unsaved() {
            self.pending = Some(Pending::Home);
        } else {
            self.screen = Screen::Start;
            self.project = None;
            self.cards = None;
        }
    }

    fn load_cards(&mut self, ctx: &egui::Context) {
        if self.cards.is_some() {
            return;
        }
        let cam = render3d::Camera { yaw: -0.5, pitch: 0.2, zoom: 1.0 };
        let cards = project::list()
            .into_iter()
            .map(|p| {
                let params = render3d::RenderParams {
                    cam,
                    model: p.meta.model,
                    vis: Default::default(),
                    highlight: vec![],
                    width: 150,
                    height: 180,
                    supersample: 2,
                    ghosts: false,
                };
                let out = render3d::render(&p.skin, &params);
                let img = egui::ColorImage::from_rgba_premultiplied([out.width, out.height], &out.rgba);
                let thumb = ctx.load_texture(format!("thumb_{}", p.meta.id), img, egui::TextureOptions::LINEAR);
                ProjectCard { meta: p.meta, thumb }
            })
            .collect();
        self.cards = Some(cards);
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let path = f.path().to_path_buf();
            match f.bytes().and_then(|b| Skin::from_png(&b)) {
                Ok(s) => {
                    let m = detect_model(&s);
                    match self.screen {
                        Screen::Start => {
                            if let Err(e) = self.create_project(Self::file_stem(&path), s, m) {
                                self.notify(ctx, e, true);
                            }
                        }
                        Screen::Editor => {
                            self.editor.replace_skin(s, m);
                            self.notify(ctx, format!("Importiert: {}", Self::file_name(&path)), false);
                        }
                        Screen::Mixer => match self.mixer.set_slot(None, s, m, Self::file_stem(&path)) {
                            Some(l) => self.notify(ctx, format!("Als Skin {l} geladen"), false),
                            None => self.notify(ctx, "Es passen höchstens 26 Skins in den Mixer.", true),
                        },
                    }
                }
                Err(e) => self.notify(ctx, e, true),
            }
        }
    }

    fn modal_open(&self) -> bool {
        self.new_dialog.is_some() || self.rename.is_some() || self.delete_confirm.is_some() || self.pending.is_some()
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal_open() {
            return;
        }
        let sc = |m, k| egui::KeyboardShortcut::new(m, k);
        let (open, save, export) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::E)),
            )
        });
        if open && self.screen == Screen::Editor {
            self.import_into_project(ctx);
        }
        if save {
            self.save_project(ctx);
        }
        if export {
            self.action_export(ctx);
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_help = !self.show_help;
        }
    }

    fn logo(&mut self, ctx: &egui::Context) -> egui::TextureId {
        self.logo
            .get_or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_gen::icon_rgba(64));
                ctx.load_texture("logo", img, egui::TextureOptions::NEAREST)
            })
            .id()
    }

    // ------------------------------------------------------------------ Startseite

    fn start_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.load_cards(&ctx);
        let logo = self.logo(&ctx);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width().min(1040.0);
            let margin = ((ui.available_width() - width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.add_space(28.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new((logo, egui::vec2(56.0, 56.0))));
                        ui.vertical(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Skin Studio").size(30.0).strong().color(Color32::WHITE));
                            ui.label(RichText::new("Was möchtest du heute erstellen?").size(15.0).weak());
                        });
                    });
                    ui.add_space(22.0);

                    // Weitermachen
                    let last_id = project::last();
                    let last = self.cards.as_ref().and_then(|c| {
                        c.iter().find(|p| Some(&p.meta.id) == last_id.as_ref()).or(c.first())
                    });
                    if let Some(card) = last {
                        let (id, name, time, tex) =
                            (card.meta.id.clone(), card.meta.name.clone(), card.meta.modified, card.thumb.id());
                        section(ui, "WEITERMACHEN");
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 120.0), Sense::click());
                        let hovered = resp.hovered();
                        let p = ui.painter();
                        p.rect_filled(rect, 12.0, if hovered { Color32::from_rgb(52, 58, 70) } else { Color32::from_rgb(44, 48, 58) });
                        p.rect_stroke(rect, 12.0, Stroke::new(1.5, if hovered { ORANGE } else { Color32::from_gray(70) }), StrokeKind::Inside);
                        let img = Rect::from_min_size(rect.min + egui::vec2(16.0, 10.0), egui::vec2(83.0, 100.0));
                        p.image(tex, img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        p.text(rect.min + egui::vec2(120.0, 34.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(22.0), Color32::WHITE);
                        p.text(
                            rect.min + egui::vec2(120.0, 64.0),
                            egui::Align2::LEFT_CENTER,
                            format!("Minecraft Skin · zuletzt gespeichert {}", project::format_time(time)),
                            egui::FontId::proportional(14.0),
                            Color32::from_gray(170),
                        );
                        let btn = Rect::from_min_size(rect.right_center() - egui::vec2(200.0, 20.0), egui::vec2(180.0, 40.0));
                        p.rect_filled(btn, 8.0, if hovered { ORANGE } else { ORANGE.gamma_multiply(0.85) });
                        p.text(btn.center(), egui::Align2::CENTER_CENTER, "▶  Weiter bearbeiten", egui::FontId::proportional(16.0), Color32::WHITE);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.open_project(&ctx, &id);
                        }
                        ui.add_space(18.0);
                    }

                    // Neues Projekt
                    section(ui, "NEUES PROJEKT");
                    let cw = (width - 2.0 * 14.0) / 3.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        if big_card(ui, cw, "🎨", "Minecraft Skin Editor", "Skins Pixel für Pixel gestalten – mit 3D-Ansicht, Rauschpinsel und mehr.", true).clicked() {
                            let existing = project::list();
                            self.new_dialog =
                                Some(NewDialog { name: project::next_name(&existing), start: StartWith::Figure, model: Model::Classic });
                        }
                        if big_card(ui, cw, "👫", "Skin-Mixer", "Zwei Skins kombinieren – z. B. dein Kopf mit einem anderen Outfit für den Partnerlook.", true).clicked() {
                            self.project = None;
                            self.screen = Screen::Mixer;
                        }
                        big_card(ui, cw, "✨", "Bald mehr …", "Weitere Editoren kommen in zukünftigen Versionen.", false);
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("📂  PNG-Skin importieren …").on_hover_text("Erstellt aus einer Skin-Datei ein neues Projekt").clicked() {
                            if let Some(p) = Self::pick_png() {
                                if let Err(e) = self.project_from_file(&p) {
                                    self.notify(&ctx, e, true);
                                }
                            }
                        }
                        ui.label(RichText::new("oder PNG-Datei einfach ins Fenster ziehen").small().weak());
                    });

                    // Meine Projekte
                    ui.add_space(20.0);
                    let cards: Vec<(String, String, u64, egui::TextureId)> = self
                        .cards
                        .as_ref()
                        .map(|c| c.iter().map(|p| (p.meta.id.clone(), p.meta.name.clone(), p.meta.modified, p.thumb.id())).collect())
                        .unwrap_or_default();
                    section(ui, &format!("MEINE PROJEKTE ({})", cards.len()));
                    if cards.is_empty() {
                        ui.label(RichText::new("Noch keine Projekte – starte oben ein neues!").weak());
                    }
                    let tile = egui::vec2(160.0, 232.0);
                    let per_row = ((width + 12.0) / (tile.x + 12.0)).floor().max(1.0) as usize;
                    for row in cards.chunks(per_row) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            for (id, name, time, tex) in row {
                                let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                                let hovered = resp.hovered();
                                let p = ui.painter();
                                p.rect_filled(rect, 10.0, if hovered { Color32::from_rgb(52, 58, 70) } else { Color32::from_rgb(42, 46, 55) });
                                p.rect_stroke(rect, 10.0, Stroke::new(1.0, if hovered { ORANGE } else { Color32::from_gray(64) }), StrokeKind::Inside);
                                let img = Rect::from_min_size(rect.min + egui::vec2(5.0, 6.0), egui::vec2(150.0, 180.0));
                                p.image(*tex, img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                                let title = elide(name, 18);
                                p.text(rect.left_bottom() + egui::vec2(10.0, -36.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(14.0), Color32::WHITE);
                                p.text(rect.left_bottom() + egui::vec2(10.0, -16.0), egui::Align2::LEFT_CENTER, project::format_time(*time), egui::FontId::proportional(12.0), Color32::from_gray(150));
                                // kleine Knöpfe zum Umbenennen / Löschen
                                let mut consumed = false;
                                if hovered || ui.rect_contains_pointer(rect) {
                                    let b1 = Rect::from_min_size(rect.right_top() + egui::vec2(-62.0, 6.0), egui::vec2(26.0, 24.0));
                                    let b2 = Rect::from_min_size(rect.right_top() + egui::vec2(-32.0, 6.0), egui::vec2(26.0, 24.0));
                                    let r1 = ui.put(b1, egui::Button::new("✏").small()).on_hover_text("Umbenennen");
                                    let r2 = ui.put(b2, egui::Button::new("🗑").small()).on_hover_text("Löschen");
                                    if r1.clicked() {
                                        self.rename = Some((id.clone(), name.clone()));
                                        consumed = true;
                                    }
                                    if r2.clicked() {
                                        self.delete_confirm = Some((id.clone(), name.clone()));
                                        consumed = true;
                                    }
                                    consumed |= r1.hovered() || r2.hovered();
                                }
                                if !consumed && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    self.open_project(&ctx, id);
                                }
                            }
                        });
                        ui.add_space(12.0);
                    }
                    ui.add_space(20.0);
                });
            });
        });
    }

    // ------------------------------------------------------------------ Leisten

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let logo = self.logo(&ctx);
        egui::MenuBar::new().ui(ui, |ui| {
            ui.add(egui::Image::new((logo, egui::vec2(20.0, 20.0))));
            ui.label(RichText::new("Skin Studio").strong().color(Color32::WHITE));
            ui.add_space(4.0);
            if ui.button("🏠").on_hover_text("Zur Startseite").clicked() {
                self.go_home();
            }
            ui.menu_button("Datei", |ui| {
                if ui.button("🏠 Zur Startseite").clicked() {
                    self.go_home();
                }
                ui.separator();
                if ui.add_enabled(self.project.is_some(), egui::Button::new("💾 Projekt speichern  (Strg+S)")).clicked() {
                    self.save_project(&ctx);
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new("✏ Projekt umbenennen …")).clicked() {
                    if let Some(p) = &self.project {
                        self.rename = Some((p.id.clone(), p.name.clone()));
                    }
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new("📂 PNG in Projekt importieren …  (Strg+O)")).clicked() {
                    self.import_into_project(&ctx);
                }
                ui.separator();
                if ui.button("⬇ Als PNG exportieren …  (Strg+E)").clicked() {
                    self.action_export(&ctx);
                }
                if ui.button("📷 3D-Ansicht als Bild …").clicked() {
                    self.export_render(&ctx);
                }
                ui.separator();
                if ui.button("Beenden").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if self.screen == Screen::Editor {
                ui.menu_button("Bearbeiten", |ui| {
                    let e = &mut self.editor;
                    if ui.add_enabled(e.can_undo(), egui::Button::new("Rückgängig  (Strg+Z)")).clicked() {
                        e.undo();
                    }
                    if ui.add_enabled(e.can_redo(), egui::Button::new("Wiederholen  (Strg+Y)")).clicked() {
                        e.redo();
                    }
                    ui.separator();
                    if ui.button("🎨 Farben anpassen …").clicked() {
                        e.start_adjust();
                    }
                    if ui.button("↔ Links und rechts spiegeln").clicked() {
                        e.mirror_all();
                    }
                    if ui.button("Graustufen").clicked() {
                        e.grayscale();
                    }
                    ui.separator();
                    if ui.button("2. Ebene auf 1. Ebene zusammenführen").clicked() {
                        e.merge_overlay();
                    }
                    if ui.button("1. Ebene in 2. Ebene kopieren").clicked() {
                        e.copy_base_to_overlay();
                    }
                    if ui.button("2. Ebene leeren").clicked() {
                        e.clear_layer(Layer::Overlay);
                    }
                    if ui.button("1. Ebene leeren").clicked() {
                        e.clear_layer(Layer::Base);
                    }
                    ui.separator();
                    let target = if e.model == Model::Classic { Model::Slim } else { Model::Classic };
                    let label =
                        if target == Model::Slim { "Arme zu Slim (3 px) umrechnen" } else { "Arme zu Classic (4 px) umrechnen" };
                    if ui.button(label).clicked() {
                        e.convert_arms(target);
                    }
                });
                ui.menu_button("Ansicht", |ui| {
                    ui.checkbox(&mut self.editor.show_2d, "2D-Textur anzeigen");
                    ui.checkbox(&mut self.editor.show_grid, "Pixelraster (2D)");
                    ui.checkbox(&mut self.editor.show_regions, "Körperteil-Bereiche (2D)");
                    ui.checkbox(&mut self.editor.preview.antialias, "3D-Kantenglättung");
                });
            }
            ui.menu_button("Hilfe", |ui| {
                if ui.button("⌨ Tastenkürzel  (F1)").clicked() {
                    self.show_help = true;
                }
            });

            ui.add_space(14.0);
            let has_project = self.project.is_some();
            if ui
                .add_enabled(has_project, egui::Button::selectable(self.screen == Screen::Editor, RichText::new("🎨  Editor").size(14.0)))
                .on_disabled_hover_text("Erst ein Projekt öffnen oder erstellen")
                .clicked()
            {
                self.screen = Screen::Editor;
            }
            if ui.add(egui::Button::selectable(self.screen == Screen::Mixer, RichText::new("👫  Skin-Mixer").size(14.0))).clicked() {
                self.screen = Screen::Mixer;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = egui::Button::new(RichText::new("⬇  Exportieren").strong().color(Color32::WHITE))
                    .fill(ui.visuals().selection.bg_fill)
                    .min_size(egui::vec2(124.0, 26.0));
                if ui.add(btn).on_hover_text("Skin als PNG-Datei speichern, z. B. für Minecraft (Strg+E)").clicked() {
                    self.action_export(&ctx);
                }
                if let Some(p) = &self.project {
                    let label = if self.editor.modified { "💾  Speichern •" } else { "💾  Gespeichert" };
                    let name = p.name.clone();
                    if ui
                        .add_enabled(self.editor.modified, egui::Button::new(label).min_size(egui::vec2(120.0, 26.0)))
                        .on_hover_text("Projekt im Programm speichern (Strg+S)")
                        .clicked()
                    {
                        self.save_project(&ctx);
                    }
                    ui.label(RichText::new(elide(&name, 28)).strong());
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match self.screen {
            Screen::Editor => {
                let e = &self.editor;
                ui.label(RichText::new(e.tool.name()).strong());
                ui.separator();
                ui.monospace(e.status_text());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(e.model.name());
                    if e.mirror {
                        ui.separator();
                        ui.label(RichText::new("↔ Spiegeln an").color(ORANGE));
                    }
                    ui.separator();
                    ui.label(RichText::new("Alt+Klick: Pipette · Strg+Z: zurück · F1: alle Tastenkürzel").weak());
                });
            }
            Screen::Mixer => {
                ui.label("Skin-Mixer – kombiniere zwei Skins zu einem neuen.");
            }
            Screen::Start => {
                ui.label(RichText::new("Deine Projekte werden im Programm gespeichert – du musst keine Dateien verwalten.").weak());
            }
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("⌨ Tastenkürzel").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).striped(true).show(ui, |ui| {
                let rows = [
                    ("B", "Stift"),
                    ("E", "Radierer"),
                    ("N", "Rauschpinsel"),
                    ("H", "Licht/Schatten"),
                    ("G / F", "Füllen"),
                    ("R", "Farbe ersetzen"),
                    ("L", "Linie (2D)"),
                    ("U", "Rechteck (2D)"),
                    ("I", "Pipette (oder Alt + Klick)"),
                    ("M", "Spiegeln an/aus"),
                    ("X", "Haupt- und Zweitfarbe tauschen"),
                    ("[  ]  /  − +", "Pinselgröße"),
                    ("Strg+Z / Strg+Y", "Rückgängig / Wiederholen"),
                    ("Strg+S", "Projekt speichern"),
                    ("Strg+O", "PNG ins Projekt importieren"),
                    ("Strg+E", "Als PNG exportieren"),
                    ("Rechte Maustaste ziehen", "3D-Modell drehen"),
                    ("Mausrad", "Zoomen"),
                ];
                for (k, v) in rows {
                    ui.monospace(k);
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
        self.show_help = open;
    }

    // ------------------------------------------------------------------ Dialoge

    fn dialogs(&mut self, ctx: &egui::Context) {
        // Neues Projekt
        if let Some(mut d) = self.new_dialog.take() {
            let mut result: Option<bool> = None;
            let modal = egui::Modal::new(egui::Id::unique("new_project")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading("🎨 Neues Skin-Projekt");
                ui.add_space(8.0);
                ui.label("Name");
                let r = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    result = Some(true);
                }
                ui.add_space(8.0);
                ui.label("Starten mit");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.start, StartWith::Figure, "👤 Start-Figur");
                    ui.selectable_value(&mut d.start, StartWith::Blank, "⬜ Leer");
                    ui.selectable_value(&mut d.start, StartWith::File, "📂 PNG-Datei …");
                });
                if d.start != StartWith::File {
                    ui.add_space(6.0);
                    ui.label("Modell");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut d.model, Model::Classic, "Classic (4 px Arme)");
                        ui.selectable_value(&mut d.model, Model::Slim, "Slim (3 px Arme)");
                    });
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let create = egui::Button::new(RichText::new("Erstellen").strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(create).clicked() {
                        result = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        result = Some(false);
                    }
                });
            });
            if modal.should_close() && result.is_none() {
                result = Some(false);
            }
            match result {
                Some(true) => {
                    let name = if d.name.trim().is_empty() { "Mein Skin".to_string() } else { d.name.trim().to_string() };
                    let res = match d.start {
                        StartWith::Figure => {
                            let s = skin::starter_skin();
                            let s = skin::convert_model(&s, Model::Classic, d.model);
                            self.create_project(name, s, d.model)
                        }
                        StartWith::Blank => self.create_project(name, Skin::blank(), d.model),
                        StartWith::File => match Self::pick_png() {
                            Some(p) => Self::read_skin(&p).and_then(|(s, m)| self.create_project(name, s, m)),
                            None => Ok(()),
                        },
                    };
                    if let Err(e) = res {
                        self.notify(ctx, e, true);
                    }
                }
                Some(false) => {}
                None => self.new_dialog = Some(d),
            }
        }

        // Umbenennen
        if let Some((id, mut name)) = self.rename.take() {
            let mut result: Option<bool> = None;
            let modal = egui::Modal::new(egui::Id::unique("rename")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading("Projekt umbenennen");
                ui.add_space(6.0);
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                r.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    result = Some(true);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("✔ Übernehmen").clicked() {
                        result = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        result = Some(false);
                    }
                });
            });
            if modal.should_close() && result.is_none() {
                result = Some(false);
            }
            match result {
                Some(true) if !name.trim().is_empty() => {
                    let name = name.trim().to_string();
                    if let Some(p) = self.project.as_mut().filter(|p| p.id == id) {
                        p.name = name;
                        // nur die Projektinfo aktualisieren, den Skin-Stand nicht ungefragt speichern
                        if let Some(stored) = project::load(&id) {
                            let mut meta = stored.meta;
                            meta.name = p.name.clone();
                            let _ = project::save(&meta, &stored.skin);
                        }
                    } else if let Some(stored) = project::load(&id) {
                        let mut meta = stored.meta;
                        meta.name = name;
                        let _ = project::save(&meta, &stored.skin);
                    }
                    self.cards = None;
                }
                None => self.rename = Some((id, name)),
                _ => {}
            }
        }

        // Löschen
        if let Some((id, name)) = self.delete_confirm.take() {
            let mut result: Option<bool> = None;
            let modal = egui::Modal::new(egui::Id::unique("delete")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading("Projekt löschen?");
                ui.label(format!("„{name}“ wird endgültig gelöscht."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let del = egui::Button::new(RichText::new("🗑 Löschen").color(Color32::WHITE)).fill(Color32::from_rgb(170, 50, 55));
                    if ui.add(del).clicked() {
                        result = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        result = Some(false);
                    }
                });
            });
            if modal.should_close() && result.is_none() {
                result = Some(false);
            }
            match result {
                Some(true) => {
                    project::delete(&id);
                    self.cards = None;
                    self.notify(ctx, format!("„{name}“ gelöscht"), false);
                }
                Some(false) => {}
                None => self.delete_confirm = Some((id, name)),
            }
        }

        // Speichern?
        if let Some(pending) = self.pending {
            let name = self.project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            let modal = egui::Modal::new(egui::Id::unique("save_prompt")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading("💾 Projekt speichern?");
                ui.add_space(4.0);
                ui.label(format!("Möchtest du deine Änderungen an „{name}“ speichern?"));
                ui.label(RichText::new("Das Projekt bleibt im Programm – beim nächsten Start kannst du direkt weitermachen.").small().weak());
                ui.add_space(10.0);
                let mut r = 0;
                ui.horizontal(|ui| {
                    let save = egui::Button::new(RichText::new("💾 Speichern").strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(save).clicked() {
                        r = 1;
                    }
                    if ui.button("Nicht speichern").clicked() {
                        r = 2;
                    }
                    if ui.button("Abbrechen").clicked() {
                        r = 3;
                    }
                });
                r
            });
            let choice = if modal.inner == 0 && modal.should_close() { 3 } else { modal.inner };
            let proceed = match choice {
                1 => self.save_project(ctx),
                2 => true,
                3 => {
                    self.pending = None;
                    false
                }
                _ => false,
            };
            if proceed {
                self.pending = None;
                match pending {
                    Pending::Close => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Pending::Home => {
                        self.editor.modified = false;
                        self.project = None;
                        self.screen = Screen::Start;
                        self.cards = None;
                    }
                }
            }
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let Some((text, until, error)) = &self.toast else { return };
        if now > *until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((until - now).max(0.05)));
        let (text, error) = (text.clone(), *error);
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -44.0))
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(if error { Color32::from_rgb(120, 34, 40) } else { Color32::from_rgb(34, 40, 52) })
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).color(Color32::WHITE));
                    });
            });
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.project {
            Some(p) if self.screen != Screen::Start => {
                format!("{}{} – Skin Studio", p.name, if self.editor.modified { " •" } else { "" })
            }
            _ => "Skin Studio".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn mixer_action(&mut self, ctx: &egui::Context, action: MixerAction) {
        match action {
            MixerAction::None => {}
            MixerAction::Load(at) => {
                let files: Vec<PathBuf> = match at {
                    Some(_) => Self::pick_png().into_iter().collect(),
                    None => rfd::FileDialog::new()
                        .add_filter("Minecraft-Skin (PNG)", &["png"])
                        .set_title("Skins für den Mixer laden")
                        .pick_files()
                        .unwrap_or_default(),
                };
                let mut full = false;
                for p in files {
                    match Self::read_skin(&p) {
                        Ok((s, m)) => {
                            if self.mixer.set_slot(at, s, m, Self::file_stem(&p)).is_none() {
                                full = true;
                            }
                        }
                        Err(e) => self.notify(ctx, format!("{}: {e}", Self::file_name(&p)), true),
                    }
                }
                if full {
                    self.notify(ctx, "Es passen höchstens 26 Skins in den Mixer.", true);
                }
            }
            MixerAction::FromEditor(at) => {
                let name = self.project.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "Editor-Skin".into());
                self.mixer.set_slot(at, self.editor.skin.clone(), self.editor.model, name);
            }
            MixerAction::OpenInEditor => {
                let (skin, model) = (self.mixer.result.clone(), self.mixer.result_model);
                if self.project.is_some() {
                    self.editor.replace_skin(skin, model);
                    self.screen = Screen::Editor;
                    self.notify(ctx, "Gemixter Skin ist jetzt im Projekt (Strg+Z macht es rückgängig)", false);
                } else {
                    let existing = project::list();
                    let name = (1..)
                        .map(|n| format!("Partnerlook {n}"))
                        .find(|n| !existing.iter().any(|p| &p.meta.name == n))
                        .unwrap();
                    if let Err(e) = self.create_project(name.clone(), skin, model) {
                        self.notify(ctx, e, true);
                    } else {
                        self.notify(ctx, format!("Neues Projekt „{name}“ erstellt"), false);
                    }
                }
            }
            MixerAction::Export => self.export_mix(ctx),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Beim Schließen fragen, ob das Projekt gespeichert werden soll
        if ctx.input(|i| i.viewport().close_requested()) && self.has_unsaved() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Close);
        }

        self.handle_drops(&ctx);
        self.global_shortcuts(&ctx);
        if self.screen == Screen::Editor && !self.modal_open() {
            self.editor.handle_shortcuts(&ctx);
        }

        if self.screen != Screen::Start {
            egui::Panel::top("top")
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 6)))
                .show(ui, |ui| self.top_bar(ui));
        }
        egui::Panel::bottom("status")
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| self.status_bar(ui));

        match self.screen {
            Screen::Start => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)))
                    .show(ui, |ui| self.start_screen(ui));
            }
            Screen::Editor => {
                egui::Panel::left("tools").resizable(false).exact_size(262.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.editor.ui_tools(ui));
                });
                egui::Panel::right("colors").resizable(false).exact_size(254.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.editor.ui_colors(ui);
                        ui.add_space(8.0);
                        ui.separator();
                        self.editor.ui_view_options(ui);
                    });
                });
                if self.editor.show_2d {
                    egui::Panel::right("tex2d").resizable(true).default_size(440.0).size_range(320.0..=1000.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("▶").on_hover_text("2D-Textur einklappen").clicked() {
                                self.editor.show_2d = false;
                            }
                            ui.label(RichText::new("2D-TEXTUR").small().strong().weak());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new("Mausrad: Zoom · mittlere Taste: verschieben").small().weak());
                            });
                        });
                        egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)).show(ui, |ui| self.editor.ui_canvas(ui));
                    });
                } else {
                    egui::Panel::right("tex2d_closed").resizable(false).exact_size(34.0).show(ui, |ui| {
                        collapsed_strip(ui, &mut self.editor.show_2d);
                    });
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)).inner_margin(egui::Margin::same(8)))
                    .show(ui, |ui| self.editor.ui_preview(ui));
                self.editor.ui_adjust_window(&ctx);
            }
            Screen::Mixer => {
                self.mixer.recompute();
                let mut action = MixerAction::None;
                let has_project = self.project.is_some();
                egui::Panel::left("mix_in").resizable(false).exact_size(300.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| action = self.mixer.ui_inputs(ui, has_project));
                });
                egui::Panel::right("mix_settings").resizable(false).exact_size(300.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.mixer.ui_settings(ui));
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    let a = self.mixer.ui_result(ui);
                    if !matches!(a, MixerAction::None) {
                        action = a;
                    }
                });
                self.mixer_action(&ctx, action);
            }
        }

        self.dialogs(&ctx);
        self.help_window(&ctx);
        self.toast_ui(&ctx);
        self.update_title(&ctx);
    }
}

// ------------------------------------------------------------------ kleine Bausteine

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(Color32::from_gray(150)));
    ui.add_space(4.0);
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// Große Auswahlkarte auf der Startseite.
fn big_card(ui: &mut egui::Ui, w: f32, icon: &str, title: &str, text: &str, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 150.0), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let p = ui.painter();
    let fill = if !enabled {
        Color32::from_rgb(36, 39, 46)
    } else if hovered {
        Color32::from_rgb(54, 60, 72)
    } else {
        Color32::from_rgb(44, 48, 58)
    };
    p.rect_filled(rect, 12.0, fill);
    let stroke = if hovered { Stroke::new(2.0, ORANGE) } else { Stroke::new(1.0, Color32::from_gray(if enabled { 70 } else { 55 })) };
    p.rect_stroke(rect, 12.0, stroke, StrokeKind::Inside);
    let alpha = if enabled { 1.0 } else { 0.4 };
    let circle = rect.min + egui::vec2(36.0, 38.0);
    p.circle_filled(circle, 20.0, ORANGE.gamma_multiply(0.25 * alpha + 0.05));
    p.text(circle, egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), Color32::WHITE.gamma_multiply(alpha));
    p.text(
        rect.min + egui::vec2(68.0, 38.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(18.0),
        Color32::WHITE.gamma_multiply(alpha),
    );
    let galley = p.layout(text.to_string(), egui::FontId::proportional(13.5), Color32::from_gray(175).gamma_multiply(alpha), w - 36.0);
    p.galley(rect.min + egui::vec2(18.0, 72.0), galley, Color32::from_gray(175));
    if enabled {
        p.text(rect.right_bottom() - egui::vec2(18.0, 18.0), egui::Align2::RIGHT_CENTER, "Starten  ➡", egui::FontId::proportional(13.0), if hovered { ORANGE } else { Color32::from_gray(140) });
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Eingeklappte 2D-Textur: schmale Leiste mit Pfeil zum Ausklappen.
fn collapsed_strip(ui: &mut egui::Ui, open: &mut bool) {
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, Sense::click()).on_hover_text("2D-Textur ausklappen");
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 0.0, Color32::from_white_alpha(8));
    }
    let col = if resp.hovered() { ORANGE } else { Color32::from_gray(180) };
    p.text(rect.center_top() + egui::vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, "◀", egui::FontId::proportional(16.0), col);
    for (k, ch) in "2D-TEXTUR".chars().enumerate() {
        p.text(
            rect.center_top() + egui::vec2(0.0, 48.0 + k as f32 * 15.0),
            egui::Align2::CENTER_CENTER,
            ch,
            egui::FontId::proportional(12.0),
            col,
        );
    }
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *open = true;
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|style| {
        let accent = Color32::from_rgb(64, 132, 245);
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = Color32::from_rgb(36, 39, 46);
        v.window_fill = Color32::from_rgb(40, 43, 51);
        v.extreme_bg_color = Color32::from_rgb(24, 26, 31);
        v.faint_bg_color = Color32::from_rgb(42, 45, 53);
        v.selection.bg_fill = accent;
        v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = accent;
        v.window_corner_radius = egui::CornerRadius::same(10);
        v.menu_corner_radius = egui::CornerRadius::same(8);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
            &mut v.widgets.noninteractive,
        ] {
            w.corner_radius = egui::CornerRadius::same(6);
        }
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(50, 54, 63);
        v.widgets.inactive.bg_fill = Color32::from_rgb(50, 54, 63);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(62, 67, 78);
        v.widgets.hovered.bg_fill = Color32::from_rgb(62, 67, 78);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(72, 78, 92);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 4.0);
        style.spacing.slider_width = 120.0;
        style.spacing.interact_size.y = 22.0;
    });
}

fn main() -> eframe::Result {
    let icon = egui::IconData { rgba: icon_gen::icon_rgba(64), width: 64, height: 64 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Skin Studio")
            .with_inner_size([1440.0, 880.0])
            .with_maximized(true)
            .with_min_inner_size([1060.0, 640.0])
            .with_drag_and_drop(true)
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native("Skin Studio", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
