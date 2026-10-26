#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Skin Studio – ein schlanker Minecraft-Skin-Editor mit Skin-Mixer.

mod color;
mod editor;
mod gpu3d;
mod i18n;
mod icon_gen;
mod installer;
mod mcskin;
mod mixer;
mod presets;
mod preview;
mod project;
mod render3d;
mod skin;
mod ui_util;
mod updater;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Color32, Key, Modifiers, Rect, RichText, Sense, Stroke, StrokeKind};

use editor::{detect_model, Editor};
use i18n::{t, t1, t2, Lang};
use mixer::{Mixer, MixerAction};
use project::ProjectMeta;
use skin::{Layer, Model, Skin};
use updater::{State as UpdateState, Updater};

pub const ORANGE: Color32 = Color32::from_rgb(242, 140, 40);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Start,
    Editor,
    Mixer,
}

/// Was nach der Frage „Speichern?“ passieren soll.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// Programm beenden
    Close,
    /// Neustart nach einem Update
    Restart,
    /// Einen Tab schließen
    CloseTab(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartWith {
    Figure,
    Blank,
}

struct NewDialog {
    name: String,
    start: StartWith,
    model: Model,
    /// optional: eigener Skin (Bezeichnung, Skin, Modell) – aus einer PNG oder per Benutzername
    file: Option<(String, Skin, Model)>,
    /// vorgeschlagener Name (wird durch den Spielernamen ersetzt, solange er unverändert ist)
    auto_name: String,
    user: String,
    user_edit: f64,
    lookup: Option<(String, Arc<std::sync::Mutex<mcskin::Lookup>>)>,
    /// für welchen Spieler das Suchergebnis schon übernommen wurde
    found_for: Option<String>,
    /// 3D-Bild des gewählten Skins
    thumb: Option<(Skin, egui::TextureHandle)>,
    /// Gesichter für die Skin-Auswahl
    faces: Vec<(Skin, egui::TextureHandle)>,
    /// Skins aus dem Minecraft-Launcher
    launcher: Vec<mcskin::Known>,
}

impl NewDialog {
    /// Skin aus einer Datei übernehmen; ist ein Spieler gefunden, kommt er auch in dessen Skin-Liste.
    fn take_skin(&mut self, label: String, skin: Skin, model: Model) {
        if let Some(name) = self.found_for.clone() {
            mcskin::add_to_history(&name, &skin, model);
            if let Some((_, st)) = &self.lookup {
                if let Ok(mut g) = st.lock() {
                    if let mcskin::Lookup::Found { skin: cur, history, .. } = &mut *g {
                        *history = mcskin::history(&name).into_iter().filter(|k| k.skin != *cur).collect();
                    }
                }
            }
        }
        self.file = Some((label, skin, model));
    }
}

/// Öffnet eine Internetseite im Standard-Browser.
fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Kleines Gesicht eines Skins (Kopf vorne mit 2. Ebene), wie bei NameMC
fn face_texture(ctx: &egui::Context, faces: &mut Vec<(Skin, egui::TextureHandle)>, skin: &Skin) -> egui::TextureId {
    if let Some((_, t)) = faces.iter().find(|(s, _)| s == skin) {
        return t.id();
    }
    let mut px = Vec::with_capacity(8 * 8 * 4);
    for y in 8..16 {
        for x in 8..16 {
            let base = skin.get(x, y);
            let over = skin.get(x + 32, y);
            let c = if over[3] > 0 { over } else { base };
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([8, 8], &px);
    let tex = ctx.load_texture(format!("face_{}", faces.len()), img, egui::TextureOptions::NEAREST);
    let id = tex.id();
    faces.push((skin.clone(), tex));
    id
}

/// Reihe anklickbarer Gesichter. Gibt den angeklickten Eintrag zurück.
fn face_row(ui: &mut egui::Ui, faces: &mut Vec<(Skin, egui::TextureHandle)>, items: &[mcskin::Known], selected: Option<&Skin>) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
        for (k, it) in items.iter().enumerate() {
            let tex = face_texture(ui.ctx(), faces, &it.skin);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), Sense::click());
            let sel = selected == Some(&it.skin);
            let p = ui.painter();
            p.rect_filled(rect, 4.0, Color32::from_rgb(44, 48, 58));
            p.image(tex, rect.shrink(3.0), Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            if sel || resp.hovered() {
                p.rect_stroke(rect, 4.0, Stroke::new(2.0, if sel { ORANGE } else { Color32::from_gray(160) }), StrokeKind::Inside);
            }
            if resp.on_hover_text(&it.label).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(k);
            }
        }
    });
    clicked
}

struct ProjectCard {
    meta: ProjectMeta,
    thumb: egui::TextureHandle,
}

/// Ein offenes Projekt in einem Tab (inaktive Tabs liegen hier, der aktive in `App`).
type TabData = (ProjectMeta, Editor, Mixer);

struct App {
    screen: Screen,
    /// aktives Projekt
    editor: Editor,
    project: Option<ProjectMeta>,
    /// alle Tabs; an der Stelle `active` steht `None` (das liegt in `editor`/`project`)
    tabs: Vec<Option<TabData>>,
    active: usize,
    /// Skin-Mixer des aktiven Projekts
    mixer: Mixer,
    cards: Option<Vec<ProjectCard>>,
    new_dialog: Option<NewDialog>,
    rename: Option<(String, String)>,
    delete_confirm: Option<(String, String)>,
    pending: Option<Pending>,
    update_confirm: bool,
    allow_close: bool,
    toast: Option<(String, f64, bool)>,
    show_help: bool,
    title: String,
    logo: Option<egui::TextureHandle>,
    updater: Updater,
    update_checked: bool,
    last_update_check: f64,
    /// Nach dem Download einmal fragen, ob neu gestartet werden soll
    restart_prompt: bool,
    restart_prompted: bool,
    /// Fehler beim Installieren eines Updates (wird als Fenster angezeigt)
    update_installing: bool,
    update_error: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        updater::cleanup_old();
        let mut app = App {
            screen: Screen::Start,
            editor: Editor::new(),
            project: None,
            tabs: Vec::new(),
            active: 0,
            mixer: Mixer::new(),
            cards: None,
            new_dialog: None,
            rename: None,
            delete_confirm: None,
            pending: None,
            update_confirm: false,
            allow_close: false,
            toast: None,
            show_help: false,
            title: String::new(),
            logo: None,
            updater: Updater::default(),
            update_checked: false,
            last_update_check: 0.0,
            restart_prompt: false,
            restart_prompted: false,
            update_installing: false,
            update_error: None,
        };
        // Datei per Kommandozeile / „Öffnen mit“: eine Datei → neues Projekt, mehrere → Mixer
        let args: Vec<PathBuf> =
            std::env::args().skip(1).filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();
        if args.len() == 1 {
            if let Err(e) = app.project_from_file(&args[0]) {
                // Programmzeit beginnt bei 0 → Meldung die ersten 5 Sekunden zeigen
                app.toast = Some((e, 5.0, true));
            }
        } else if args.len() > 1 && app.project_from_file(&args[0]).is_ok() {
            for p in &args[1..] {
                if let Ok((s, m)) = Self::read_skin(p) {
                    app.mixer.set_slot(None, s, m, Self::file_stem(p));
                }
            }
            app.screen = Screen::Mixer;
        }
        if std::env::args().any(|a| a == "--updated") {
            app.toast = Some((t1("✔ Skin Studio wurde auf Version {0} aktualisiert", updater::VERSION), 5.0, false));
        }
        app
    }

    fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>, error: bool) {
        let time = ctx.input(|i| i.time);
        self.toast = Some((text.into(), time + if error { 5.0 } else { 2.6 }, error));
    }

    // ------------------------------------------------------------------ Dateien

    fn read_skin(path: &Path) -> Result<(Skin, Model), String> {
        let bytes = std::fs::read(path).map_err(|e| t1("Datei konnte nicht gelesen werden: {0}", e))?;
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
        rfd::FileDialog::new().add_filter(t("Minecraft-Skin (PNG)"), &["png"]).set_title(t("Skin öffnen")).pick_file()
    }

    fn save_png_dialog(default_name: &str, title: &str) -> Option<PathBuf> {
        let mut p = rfd::FileDialog::new()
            .add_filter(t("PNG-Bild"), &["png"])
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
            Ok(_) => self.notify(ctx, t1("✔ Exportiert: {0}", Self::file_name(path)), false),
            Err(e) => self.notify(ctx, t1("Exportieren fehlgeschlagen: {0}", e), true),
        }
    }

    fn safe_file_name(name: &str) -> String {
        let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        format!("{}.png", if s.is_empty() { "skin".into() } else { s })
    }

    fn export_editor(&mut self, ctx: &egui::Context) {
        let name = self.project.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "skin".into());
        if let Some(p) = Self::save_png_dialog(&Self::safe_file_name(&name), t("Skin als PNG exportieren")) {
            let mut s = self.editor.skin.clone();
            s.clean_unused(self.editor.model);
            self.write_file(ctx, &p, &s.to_png());
        }
    }

    fn export_mix(&mut self, ctx: &egui::Context) {
        let name = self.project.as_ref().map(|p| format!("{}_mix", p.name)).unwrap_or_else(|| "mix".into());
        if let Some(p) = Self::save_png_dialog(&Self::safe_file_name(&name), t("Gemixten Skin exportieren")) {
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
        if let Some(p) = Self::save_png_dialog("skin_vorschau.png", t("3D-Ansicht als Bild speichern")) {
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
                    self.notify(ctx, t1("Importiert: {0} (Strg+Z macht es rückgängig)", Self::file_name(&p)), false);
                }
                Err(e) => self.notify(ctx, e, true),
            }
        }
    }

    // ------------------------------------------------------------------ Tabs & Projekte

    fn tab_index_of(&self, id: &str) -> Option<usize> {
        if self.project.as_ref().is_some_and(|p| p.id == id) {
            return Some(self.active);
        }
        self.tabs.iter().position(|t| t.as_ref().is_some_and(|(m, _, _)| m.id == id))
    }

    fn tab_meta(&self, i: usize) -> Option<&ProjectMeta> {
        if i == self.active { self.project.as_ref() } else { self.tabs.get(i)?.as_ref().map(|(m, _, _)| m) }
    }

    fn tab_modified(&self, i: usize) -> bool {
        if i == self.active {
            self.project.is_some() && (self.editor.modified || self.mixer.changed)
        } else {
            self.tabs.get(i).and_then(|t| t.as_ref()).is_some_and(|(_, e, m)| e.modified || m.changed)
        }
    }

    fn switch_tab(&mut self, i: usize) {
        if i >= self.tabs.len() || i == self.active || self.project.is_none() {
            return;
        }
        let Some((meta, ed, mx)) = self.tabs[i].take() else { return };
        let old_meta = self.project.replace(meta).unwrap();
        let old_editor = std::mem::replace(&mut self.editor, ed);
        let old_mixer = std::mem::replace(&mut self.mixer, mx);
        self.tabs[self.active] = Some((old_meta, old_editor, old_mixer));
        self.active = i;
        if let Some(p) = &self.project {
            project::set_last(&p.id);
        }
    }

    fn open_project_with(&mut self, meta: ProjectMeta, skin: Skin) {
        if let Some(i) = self.tab_index_of(&meta.id) {
            self.switch_tab(i);
            self.screen = Screen::Editor;
            return;
        }
        let mut ed = self.editor.fresh_from();
        let mut mx = Mixer::load_from(&project::dir(&meta.id)).unwrap_or_else(Mixer::new);
        mx.ensure_project_slot(&skin, meta.model);
        ed.open_fresh(skin, meta.model);
        project::set_last(&meta.id);
        if let Some(old_meta) = self.project.take() {
            let old_editor = std::mem::replace(&mut self.editor, ed);
            let old_mixer = std::mem::replace(&mut self.mixer, mx);
            self.tabs[self.active] = Some((old_meta, old_editor, old_mixer));
            self.tabs.push(None);
            self.active = self.tabs.len() - 1;
        } else {
            self.editor = ed;
            self.mixer = mx;
            self.tabs = vec![None];
            self.active = 0;
        }
        self.project = Some(meta);
        self.screen = Screen::Editor;
    }

    fn close_tab_now(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if i == self.active {
            self.tabs.remove(i);
            if self.tabs.is_empty() {
                self.project = None;
                self.editor = self.editor.fresh_from();
                self.mixer = Mixer::new();
                self.active = 0;
                if self.screen != Screen::Start {
                    self.screen = Screen::Start;
                }
                self.cards = None;
            } else {
                let next = i.min(self.tabs.len() - 1);
                let (meta, ed, mx) = self.tabs[next].take().unwrap();
                self.project = Some(meta);
                self.editor = ed;
                self.mixer = mx;
                self.active = next;
            }
        } else {
            self.tabs.remove(i);
            if i < self.active {
                self.active -= 1;
            }
        }
    }

    fn request_close_tab(&mut self, i: usize) {
        if self.tab_modified(i) {
            self.pending = Some(Pending::CloseTab(i));
        } else {
            self.close_tab_now(i);
        }
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
        if let Some(i) = self.tab_index_of(id) {
            self.switch_tab(i);
            self.screen = Screen::Editor;
            return;
        }
        match project::load(id) {
            Some(p) => self.open_project_with(p.meta, p.skin),
            None => {
                self.notify(ctx, t("Projekt konnte nicht geladen werden."), true);
                self.cards = None;
            }
        }
    }

    fn save_pair(meta: &mut ProjectMeta, editor: &mut Editor, mixer: &mut Mixer) -> Result<(), String> {
        meta.model = editor.model;
        meta.modified = project::now();
        project::save(meta, &editor.skin)?;
        editor.modified = false;
        if mixer.changed || mixer.applied_base.is_some() || !mixer.slots.is_empty() {
            mixer.save_to(&project::dir(&meta.id)).map_err(|e| t1("Speichern fehlgeschlagen: {0}", e))?;
        }
        mixer.changed = false;
        Ok(())
    }

    fn save_project(&mut self, ctx: &egui::Context) -> bool {
        let Some(meta) = &mut self.project else { return true };
        match Self::save_pair(meta, &mut self.editor, &mut self.mixer) {
            Ok(_) => {
                let name = meta.name.clone();
                self.cards = None;
                project::set_last(&meta.id);
                self.notify(ctx, t1("✔ „{0}“ gespeichert", name), false);
                true
            }
            Err(e) => {
                self.notify(ctx, e, true);
                false
            }
        }
    }

    /// Alle geänderten Tabs speichern.
    fn save_all(&mut self, ctx: &egui::Context) -> bool {
        let mut ok = true;
        if self.tab_modified(self.active) {
            ok &= self.save_project(ctx);
        }
        for (meta, ed, mx) in self.tabs.iter_mut().flatten() {
            if ed.modified || mx.changed {
                if let Err(e) = Self::save_pair(meta, ed, mx) {
                    ok = false;
                    self.toast = Some((e, ctx.input(|i| i.time) + 5.0, true));
                }
            }
        }
        self.cards = None;
        ok
    }

    fn unsaved_count(&self) -> usize {
        (0..self.tabs.len()).filter(|&i| self.tab_modified(i)).count()
    }

    fn open_new_dialog(&mut self) {
        let existing = project::list();
        let name = project::next_name(&existing);
        self.new_dialog = Some(NewDialog {
            auto_name: name.clone(),
            name,
            start: StartWith::Figure,
            model: Model::Classic,
            file: None,
            user: String::new(),
            user_edit: 0.0,
            lookup: None,
            found_for: None,
            thumb: None,
            faces: Vec::new(),
            launcher: mcskin::launcher_skins(),
        });
    }

    fn go_home(&mut self) {
        self.screen = Screen::Start;
        self.cards = None;
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
                    grid: false,
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
                    // Dialog „Neues Projekt“ offen: Skin dort übernehmen
                    if let Some(d) = self.new_dialog.as_mut() {
                        d.take_skin(Self::file_name(&path), s, m);
                        continue;
                    }
                    match self.screen {
                        Screen::Start => {
                            if let Err(e) = self.create_project(Self::file_stem(&path), s, m) {
                                self.notify(ctx, e, true);
                            }
                        }
                        Screen::Editor => {
                            self.editor.replace_skin(s, m);
                            self.notify(ctx, t1("Importiert: {0}", Self::file_name(&path)), false);
                        }
                        Screen::Mixer => match self.mixer.set_slot(None, s, m, Self::file_stem(&path)) {
                            Some(l) => self.notify(ctx, t1("Als Skin {0} geladen", l), false),
                            None => self.notify(ctx, t("Es passen höchstens 26 Skins in den Mixer."), true),
                        },
                    }
                }
                Err(e) => self.notify(ctx, e, true),
            }
        }
    }

    fn modal_open(&self) -> bool {
        self.new_dialog.is_some()
            || self.rename.is_some()
            || self.delete_confirm.is_some()
            || self.pending.is_some()
            || self.update_confirm
            || self.restart_prompt
            || self.update_error.is_some()
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal_open() {
            return;
        }
        let sc = |m, k| egui::KeyboardShortcut::new(m, k);
        let (open, save, export, new_tab, close_tab) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::E)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::T)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::W)),
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
        if new_tab {
            self.go_home();
        }
        if close_tab && self.project.is_some() {
            self.request_close_tab(self.active);
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

    fn set_language(&mut self, l: Lang) {
        i18n::set_lang(l);
        let mut s = project::load_settings();
        s.lang = Some(l);
        project::save_settings(&s);
        self.cards = None;
    }

    // ------------------------------------------------------------------ Update-Knopf

    fn update_button(&mut self, ui: &mut egui::Ui) {
        let state = self.updater.state();
        let green = Color32::from_rgb(80, 210, 110);
        let col = match &state {
            UpdateState::Available(_) | UpdateState::Ready(_) => green,
            UpdateState::Downloading(_) => ORANGE,
            UpdateState::Checking => Color32::from_gray(150),
            UpdateState::Failed(_) => Color32::from_gray(100),
            _ => Color32::from_gray(120),
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 6.0, Color32::from_white_alpha(14));
        }
        updater::paint_icon(ui.painter(), rect, col);
        if matches!(state, UpdateState::Available(_) | UpdateState::Ready(_)) {
            ui.painter().circle_filled(rect.right_top() + egui::vec2(-4.0, 4.0), 3.5, green);
        }
        if matches!(state, UpdateState::Checking | UpdateState::Downloading(_)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        let resp = resp.on_hover_text(t("Updates")).on_hover_cursor(egui::CursorIcon::PointingHand);
        // Kleines Fenster mit dem Update-Stand
        egui::Popup::from_toggle_button_response(&resp)
            .align(egui::RectAlign::BOTTOM_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(270.0);
                match &state {
                    UpdateState::Available(v) => {
                        ui.label(RichText::new(t("🔄 Update verfügbar")).strong().size(15.0).color(green));
                        ui.label(t2("Version {0} ist da (du hast {1}).", v, updater::VERSION));
                        ui.add_space(6.0);
                        let b = egui::Button::new(RichText::new(t("Jetzt installieren")).strong().color(Color32::WHITE)).fill(ORANGE);
                        if ui.add(b).clicked() {
                            self.updater.install(ui.ctx());
                            self.update_installing = true;
                        }
                    }
                    UpdateState::Downloading(v) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t1("Version {0} wird heruntergeladen …", v));
                        });
                    }
                    UpdateState::Ready(v) => {
                        ui.label(RichText::new(t("✔ Update installiert")).strong().size(15.0).color(green));
                        ui.label(t1("Starte neu, um Version {0} zu benutzen.", v));
                        ui.add_space(6.0);
                        let b = egui::Button::new(RichText::new(t("Jetzt neu starten")).strong().color(Color32::WHITE)).fill(ORANGE);
                        if ui.add(b).clicked() {
                            self.restart_for_update(ui.ctx());
                        }
                    }
                    UpdateState::Failed(e) => {
                        ui.label(RichText::new(t("Keine Verbindung zum Update-Server")).strong());
                        ui.label(RichText::new(e).small().color(ORANGE));
                        ui.label(RichText::new(t1("Deine Version: {0}", updater::VERSION)).small().weak());
                        if ui.button(t("🔄 Nochmal prüfen")).clicked() {
                            self.updater.check(ui.ctx());
                        }
                    }
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Suche nach Updates …"));
                        });
                    }
                    UpdateState::UpToDate | UpdateState::Idle => {
                        ui.label(RichText::new(t("✔ Keine Updates")).strong().size(15.0));
                        ui.label(t1("Du bist auf dem neuesten Stand (Version {0}).", updater::VERSION));
                    }
                }
                ui.add_space(4.0);
                let checked = match self.updater.seconds_since_check() {
                    Some(s) => t1("Zuletzt geprüft vor {0} s", s),
                    None => t("Noch nicht geprüft").to_string(),
                };
                ui.label(RichText::new(format!("{checked} · {}", t("prüft alle 5 Sekunden"))).small().weak());
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            });
    }

    fn restart_for_update(&mut self, ctx: &egui::Context) {
        if self.unsaved_count() > 0 {
            self.pending = Some(Pending::Restart);
        } else {
            self.do_restart(ctx);
        }
    }

    fn do_restart(&mut self, ctx: &egui::Context) {
        if updater::restart().is_some() {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.notify(ctx, t("Neustart fehlgeschlagen – bitte Skin Studio selbst neu öffnen."), true);
        }
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
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new((logo, egui::vec2(56.0, 56.0))));
                        ui.vertical(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Skin Studio").size(30.0).strong().color(Color32::WHITE));
                            ui.label(RichText::new(t("Was möchtest du heute erstellen?")).size(15.0).weak());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.update_button(ui);
                            self.language_menu(ui);
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
                        section(ui, t("WEITERMACHEN"));
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
                            t1("Minecraft Skin · zuletzt gespeichert {0}", project::format_time(time)),
                            egui::FontId::proportional(14.0),
                            Color32::from_gray(170),
                        );
                        let btn = Rect::from_min_size(rect.right_center() - egui::vec2(200.0, 20.0), egui::vec2(180.0, 40.0));
                        p.rect_filled(btn, 8.0, if hovered { ORANGE } else { ORANGE.gamma_multiply(0.85) });
                        p.text(btn.center(), egui::Align2::CENTER_CENTER, t("▶  Weiter bearbeiten"), egui::FontId::proportional(16.0), Color32::WHITE);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.open_project(&ctx, &id);
                        }
                        ui.add_space(18.0);
                    }

                    // Neues Projekt
                    section(ui, t("NEUES PROJEKT"));
                    if big_card(
                        ui,
                        width,
                        "➕",
                        t("Neues Projekt"),
                        t("Name eingeben und optional deinen Skin (.png) hinzufügen. Jedes Projekt hat Editor und Skin-Mixer mit Outfit-Vorlagen – oben einfach umschalten."),
                        true,
                    )
                    .clicked()
                    {
                        self.open_new_dialog();
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(t("oder PNG-Datei einfach ins Fenster ziehen")).small().weak());

                    // Meine Projekte
                    ui.add_space(20.0);
                    let cards: Vec<(String, String, u64, egui::TextureId)> = self
                        .cards
                        .as_ref()
                        .map(|c| c.iter().map(|p| (p.meta.id.clone(), p.meta.name.clone(), p.meta.modified, p.thumb.id())).collect())
                        .unwrap_or_default();
                    section(ui, &t1("MEINE PROJEKTE ({0})", cards.len()));
                    if cards.is_empty() {
                        ui.label(RichText::new(t("Noch keine Projekte – starte oben ein neues!")).weak());
                    }
                    let tile = egui::vec2(160.0, 232.0);
                    let per_row = ((width + 12.0) / (tile.x + 12.0)).floor().max(1.0) as usize;
                    for row in cards.chunks(per_row) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            for (id, name, time, tex) in row {
                                let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                                let hovered = resp.hovered();
                                let open = self.tab_index_of(id).is_some();
                                let p = ui.painter();
                                p.rect_filled(rect, 10.0, if hovered { Color32::from_rgb(52, 58, 70) } else { Color32::from_rgb(42, 46, 55) });
                                p.rect_stroke(rect, 10.0, Stroke::new(1.0, if hovered || open { ORANGE } else { Color32::from_gray(64) }), StrokeKind::Inside);
                                let img = Rect::from_min_size(rect.min + egui::vec2(5.0, 6.0), egui::vec2(150.0, 180.0));
                                p.image(*tex, img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                                let title = elide(name, 18);
                                p.text(rect.left_bottom() + egui::vec2(10.0, -36.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(14.0), Color32::WHITE);
                                let sub = if open { t("● geöffnet").to_string() } else { project::format_time(*time) };
                                p.text(rect.left_bottom() + egui::vec2(10.0, -16.0), egui::Align2::LEFT_CENTER, sub, egui::FontId::proportional(12.0), if open { ORANGE } else { Color32::from_gray(150) });
                                // kleine Knöpfe zum Umbenennen / Löschen
                                let mut consumed = false;
                                if hovered || ui.rect_contains_pointer(rect) {
                                    // Knöpfe nur zeichnen, ohne Platz im Layout zu belegen (sonst verrutschen die Karten)
                                    let small_btn = |ui: &mut egui::Ui, r: Rect, icon: &str, salt: &str| {
                                        let resp = ui.interact(r, ui.scope_id().with((salt, id.as_str())), Sense::click());
                                        let fill = if resp.hovered() { Color32::from_rgb(70, 76, 90) } else { Color32::from_rgb(52, 56, 66) };
                                        ui.painter().rect_filled(r, 4.0, fill);
                                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(13.0), Color32::WHITE);
                                        resp
                                    };
                                    let b1 = Rect::from_min_size(rect.right_top() + egui::vec2(-62.0, 6.0), egui::vec2(26.0, 24.0));
                                    let b2 = Rect::from_min_size(rect.right_top() + egui::vec2(-32.0, 6.0), egui::vec2(26.0, 24.0));
                                    let r1 = small_btn(ui, b1, "✏", "ren").on_hover_text(t("Umbenennen"));
                                    let r2 = small_btn(ui, b2, "🗑", "del").on_hover_text(t("Löschen"));
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

    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let cur = i18n::lang();
        ui.menu_button(format!("🌐 {}", cur.label()), |ui| {
            for l in [Lang::En, Lang::De] {
                if ui.selectable_label(cur == l, l.label()).clicked() {
                    self.set_language(l);
                    ui.close();
                }
            }
        });
    }

    // ------------------------------------------------------------------ Leisten

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch = None;
        let mut close = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for i in 0..self.tabs.len() {
                let Some(meta) = self.tab_meta(i) else { continue };
                let name = elide(&meta.name, 22);
                let modified = self.tab_modified(i);
                let active = i == self.active && self.screen != Screen::Start;
                let fill = if active { Color32::from_rgb(52, 56, 66) } else { Color32::from_rgb(38, 41, 48) };
                let frame = egui::Frame::NONE
                    .fill(fill)
                    .inner_margin(egui::Margin { left: 10, right: 4, top: 4, bottom: 4 })
                    .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 });
                let r = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (icon, icon_resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::click());
                        let logo = self.logo.as_ref().map(|l| l.id());
                        if let Some(l) = logo {
                            ui.painter().image(l, icon, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        let text = RichText::new(format!("{name}{}", if modified { " •" } else { "" }))
                            .color(if active { Color32::WHITE } else { Color32::from_gray(170) });
                        let label = ui.add(egui::Label::new(text).sense(Sense::click()).selectable(false));
                        if (label.clicked() || icon_resp.clicked()) && close.is_none() {
                            switch = Some(i);
                        }
                        label.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if ui.add(egui::Button::new(RichText::new("✖").size(10.0)).frame(false)).on_hover_text(t("Tab schließen (Strg+W)")).clicked() {
                            close = Some(i);
                        }
                    });
                });
                if active {
                    let rr = r.response.rect;
                    ui.painter().line_segment([rr.left_bottom(), rr.right_bottom()], Stroke::new(2.0, ORANGE));
                }
            }
            if ui
                .add(egui::Button::new(RichText::new("➕").size(13.0)).frame(false).min_size(egui::vec2(26.0, 24.0)))
                .on_hover_text(t("Neues oder anderes Projekt öffnen (Strg+T)"))
                .clicked()
            {
                self.go_home();
            }
        });
        if let Some(i) = close {
            self.request_close_tab(i);
        } else if let Some(i) = switch {
            self.switch_tab(i);
            if self.screen == Screen::Start {
                self.screen = Screen::Editor;
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let logo = self.logo(&ctx);
        egui::MenuBar::new().ui(ui, |ui| {
            ui.add(egui::Image::new((logo, egui::vec2(20.0, 20.0))));
            ui.label(RichText::new("Skin Studio").strong().color(Color32::WHITE));
            ui.add_space(2.0);
            if ui.button("🏠").on_hover_text(t("Zur Startseite")).clicked() {
                self.go_home();
            }
            ui.menu_button(t("Datei"), |ui| {
                if ui.button(t("🏠 Zur Startseite")).clicked() {
                    self.go_home();
                }
                ui.separator();
                if ui.add_enabled(self.project.is_some(), egui::Button::new(t("💾 Projekt speichern  (Strg+S)"))).clicked() {
                    self.save_project(&ctx);
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new(t("✏ Projekt umbenennen …"))).clicked() {
                    if let Some(p) = &self.project {
                        self.rename = Some((p.id.clone(), p.name.clone()));
                    }
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new(t("📂 PNG in Projekt importieren …  (Strg+O)"))).clicked() {
                    self.import_into_project(&ctx);
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new(t("✖ Tab schließen  (Strg+W)"))).clicked() {
                    self.request_close_tab(self.active);
                }
                ui.separator();
                if ui.button(t("⬇ Als PNG exportieren …  (Strg+E)")).clicked() {
                    self.action_export(&ctx);
                }
                if ui.button(t("📷 3D-Ansicht als Bild …")).clicked() {
                    self.export_render(&ctx);
                }
                ui.separator();
                if ui.button(t("Beenden")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if self.screen == Screen::Editor {
                ui.menu_button(t("Bearbeiten"), |ui| {
                    let e = &mut self.editor;
                    if ui.add_enabled(e.can_undo(), egui::Button::new(t("Rückgängig  (Strg+Z)"))).clicked() {
                        e.undo();
                    }
                    if ui.add_enabled(e.can_redo(), egui::Button::new(t("Wiederholen  (Strg+Y)"))).clicked() {
                        e.redo();
                    }
                    ui.separator();
                    if ui.button(t("🎨 Farben anpassen …")).clicked() {
                        e.start_adjust();
                    }
                    if ui.button(t("↔ Links und rechts spiegeln")).clicked() {
                        e.mirror_all();
                    }
                    if ui.button(t("Graustufen")).clicked() {
                        e.grayscale();
                    }
                    ui.separator();
                    if ui.button(t("2. Ebene auf 1. Ebene zusammenführen")).clicked() {
                        e.merge_overlay();
                    }
                    if ui.button(t("1. Ebene in 2. Ebene kopieren")).clicked() {
                        e.copy_base_to_overlay();
                    }
                    if ui.button(t("2. Ebene leeren")).clicked() {
                        e.clear_layer(Layer::Overlay);
                    }
                    if ui.button(t("1. Ebene leeren")).clicked() {
                        e.clear_layer(Layer::Base);
                    }
                    ui.separator();
                    let target = if e.model == Model::Classic { Model::Slim } else { Model::Classic };
                    let label =
                        if target == Model::Slim { t("Arme zu Slim (3 px) umrechnen") } else { t("Arme zu Classic (4 px) umrechnen") };
                    if ui.button(label).clicked() {
                        e.convert_arms(target);
                    }
                });
                ui.menu_button(t("Ansicht"), |ui| {
                    ui.checkbox(&mut self.editor.show_2d, t("2D-Textur anzeigen"));
                    ui.checkbox(&mut self.editor.show_grid, t("Pixelraster (2D)"));
                    ui.checkbox(&mut self.editor.show_regions, t("Körperteil-Bereiche (2D)"));
                    ui.checkbox(&mut self.editor.preview.grid, t("Pixelraster (3D)"));
                });
            }
            ui.menu_button(t("Hilfe"), |ui| {
                if ui.button(t("⌨ Tastenkürzel  (F1)")).clicked() {
                    self.show_help = true;
                }
                ui.separator();
                ui.label(RichText::new(t("Sprache / Language")).small().weak());
                let cur = i18n::lang();
                for l in [Lang::En, Lang::De] {
                    if ui.selectable_label(cur == l, l.label()).clicked() {
                        self.set_language(l);
                    }
                }
                ui.separator();
                if ui.button(t("🔄 Nach Updates suchen")).clicked() {
                    self.updater.check(&ctx);
                }
                ui.label(RichText::new(t1("Version {0}", updater::VERSION)).small().weak());
            });

            ui.add_space(14.0);
            let has_project = self.project.is_some();
            if ui
                .add_enabled(has_project, egui::Button::selectable(self.screen == Screen::Editor, RichText::new(t("🎨  Editor")).size(14.0)))
                .on_disabled_hover_text(t("Erst ein Projekt öffnen oder erstellen"))
                .clicked()
            {
                self.screen = Screen::Editor;
            }
            if ui
                .add_enabled(has_project, egui::Button::selectable(self.screen == Screen::Mixer, RichText::new(t("👫  Skin-Mixer")).size(14.0)))
                .on_disabled_hover_text(t("Erst ein Projekt öffnen oder erstellen"))
                .clicked()
            {
                self.screen = Screen::Mixer;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.update_button(ui);
                ui.add_space(4.0);
                let btn = egui::Button::new(RichText::new(t("⬇  Exportieren")).strong().color(Color32::WHITE))
                    .fill(ui.visuals().selection.bg_fill)
                    .min_size(egui::vec2(124.0, 26.0));
                if ui.add(btn).on_hover_text(t("Skin als PNG-Datei speichern, z. B. für Minecraft (Strg+E)")).clicked() {
                    self.action_export(&ctx);
                }
                if self.project.is_some() {
                    let modified = self.tab_modified(self.active);
                    let label = if modified { t("💾  Speichern •") } else { t("💾  Gespeichert") };
                    if ui
                        .add_enabled(modified, egui::Button::new(label).min_size(egui::vec2(120.0, 26.0)))
                        .on_hover_text(t("Projekt im Programm speichern (Strg+S)"))
                        .clicked()
                    {
                        self.save_project(&ctx);
                    }
                    if self.screen == Screen::Editor
                        && ui
                            .add(egui::Button::new(t("📂  Importieren")).min_size(egui::vec2(110.0, 26.0)))
                            .on_hover_text(t("Einen PNG-Skin in dieses Projekt laden (Strg+O) – Strg+Z macht es rückgängig"))
                            .clicked()
                    {
                        self.import_into_project(&ctx);
                    }
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
                        ui.label(RichText::new(t("↔ Spiegeln an")).color(ORANGE));
                    }
                    ui.separator();
                    ui.label(RichText::new(t("Alt+Klick: Pipette · Strg+Z: zurück · F1: alle Tastenkürzel")).weak());
                });
            }
            Screen::Mixer => {
                ui.label(t("Skin-Mixer – kombiniere Skins und Outfit-Vorlagen. „Ins Projekt übernehmen“ speichert den Mix als Projekt-Skin."));
            }
            Screen::Start => {}
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new(t("⌨ Tastenkürzel")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).striped(true).show(ui, |ui| {
                let rows = [
                    ("B", t("Stift")),
                    ("E", t("Radierer")),
                    ("N", t("Rauschpinsel")),
                    ("H", t("Licht/Schatten")),
                    ("G / F", t("Füllen")),
                    ("R", t("Farbe ersetzen")),
                    ("L", t("Linie (2D)")),
                    ("U", t("Rechteck (2D)")),
                    ("I", t("Pipette (oder Alt + Klick)")),
                    ("M", t("Spiegeln an/aus")),
                    ("X", t("Haupt- und Zweitfarbe tauschen")),
                    ("[  ]  /  − +", t("Pinselgröße")),
                    (t("Strg+Z / Strg+Y"), t("Rückgängig / Wiederholen")),
                    (t("Strg+S"), t("Projekt speichern")),
                    (t("Strg+O"), t("PNG ins Projekt importieren")),
                    (t("Strg+E"), t("Als PNG exportieren")),
                    (t("Strg+T / Strg+W"), t("Neuer Tab / Tab schließen")),
                    (t("Rechte Maustaste ziehen"), t("3D-Modell drehen")),
                    (t("Mausrad"), t("Zoomen")),
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
            let mut pick_err = None;
            let mut clear_file = false;
            let modal = egui::Modal::new(egui::Id::unique("new_project")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading(t("➕ Neues Projekt"));
                ui.add_space(8.0);
                ui.label(t("Name"));
                let r = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    result = Some(true);
                }
                ui.add_space(10.0);
                ui.label(t("Dein Skin (optional)"));
                ui.horizontal(|ui| {
                    if ui.button(t("📂 PNG auswählen …")).clicked() {
                        if let Some(p) = Self::pick_png() {
                            match Self::read_skin(&p) {
                                Ok((s, m)) => d.take_skin(Self::file_name(&p), s, m),
                                Err(e) => pick_err = Some(e),
                            }
                        }
                    }
                    ui.label(RichText::new(t("oder Benutzername:")).weak());
                    let r = ui.add(egui::TextEdit::singleline(&mut d.user).hint_text(t("Minecraft-Name")).desired_width(f32::INFINITY));
                    let now = ui.input(|i| i.time);
                    if r.changed() {
                        d.user_edit = now;
                    }
                    let name = d.user.trim().to_string();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    let searched = d.lookup.as_ref().is_some_and(|(n, _)| n.eq_ignore_ascii_case(&name));
                    if !name.is_empty() && !searched {
                        // 2 Sekunden nach dem letzten Tippen (oder sofort mit Enter) suchen
                        let wait = 2.0 - (now - d.user_edit);
                        if enter || wait <= 0.0 {
                            d.lookup = Some((name.clone(), mcskin::fetch_async(name, ui.ctx())));
                        } else {
                            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.05)));
                        }
                    }
                });
                // Ergebnis der Spielersuche
                let state = d.lookup.as_ref().and_then(|(n, st)| st.lock().ok().map(|g| (n.clone(), g.clone())));
                match state {
                    Some((n, mcskin::Lookup::Loading)) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t1("Suche „{0}“ …", n));
                        });
                    }
                    Some((_, mcskin::Lookup::Failed(e))) => {
                        ui.label(RichText::new(e).color(ORANGE));
                    }
                    Some((_, mcskin::Lookup::Found { name, skin, model, history })) => {
                        if d.found_for.as_deref() != Some(name.as_str()) {
                            // gefundenen Skin direkt übernehmen
                            d.found_for = Some(name.clone());
                            d.file = Some((name.clone(), skin.clone(), model));
                            if d.name == d.auto_name {
                                d.name = name.clone();
                                d.auto_name = name.clone();
                            }
                        }
                        let mut items = vec![mcskin::Known { label: t1("{0} – aktueller Skin", &name), skin, model }];
                        items.extend(history);
                        ui.add_space(4.0);
                        ui.label(RichText::new(t2("Skins von {0} ({1})", &name, items.len())).strong());
                        let sel = d.file.as_ref().map(|(_, s, _)| s.clone());
                        if let Some(k) = face_row(ui, &mut d.faces, &items, sel.as_ref()) {
                            let it = &items[k];
                            d.file = Some((if k == 0 { name.clone() } else { format!("{name} ({})", it.label) }, it.skin.clone(), it.model));
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(t("🌐 Alle früheren Skins auf NameMC"))
                                .on_hover_text(t("Öffnet das NameMC-Profil im Browser. Dort einen Skin herunterladen und die PNG hierher ziehen – er wird dann bei diesem Spieler gespeichert."))
                                .clicked()
                            {
                                open_url(&format!("https://namemc.com/profile/{name}"));
                            }
                        });
                        ui.label(RichText::new(t1("Skin von NameMC heruntergeladen? Einfach die PNG hierher ziehen – sie wird bei {0} gespeichert.", &name)).small().weak());
                    }
                    None => {}
                }
                if !d.launcher.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t1("Aus deinem Minecraft-Launcher ({0})", d.launcher.len())).strong());
                    let sel = d.file.as_ref().map(|(_, s, _)| s.clone());
                    let launcher = d.launcher.clone();
                    if let Some(k) = face_row(ui, &mut d.faces, &launcher, sel.as_ref()) {
                        let it = &launcher[k];
                        d.file = Some((it.label.clone(), it.skin.clone(), it.model));
                    }
                }
                if let Some((label, skin, model)) = d.file.clone() {
                    if d.thumb.as_ref().is_none_or(|(s, _)| *s != skin) {
                        let params = render3d::RenderParams {
                            cam: render3d::Camera { yaw: -0.5, pitch: 0.15, zoom: 1.0 },
                            model,
                            vis: Default::default(),
                            highlight: vec![],
                            width: 96,
                            height: 120,
                            supersample: 2,
                            ghosts: false,
                            grid: false,
                        };
                        let out = render3d::render(&skin, &params);
                        let img = egui::ColorImage::from_rgba_premultiplied([out.width, out.height], &out.rgba);
                        d.thumb = Some((skin.clone(), ui.ctx().load_texture("chosen_skin", img, egui::TextureOptions::LINEAR)));
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if let Some((_, tex)) = &d.thumb {
                            ui.add(egui::Image::new((tex.id(), egui::vec2(88.0, 110.0))));
                        }
                        ui.vertical(|ui| {
                            ui.add_space(30.0);
                            ui.label(RichText::new(t("Gewählter Skin")).small().weak());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&label).strong().color(ORANGE));
                                if ui.small_button("✖").on_hover_text(t("Skin entfernen")).clicked() {
                                    clear_file = true;
                                }
                            });
                            ui.label(RichText::new(model.name()).small().weak());
                        });
                    });
                }
                if d.file.is_none() {
                    ui.label(RichText::new(t("Ohne Skin startest du mit:")).small().weak());
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut d.start, StartWith::Figure, t("👤 Start-Figur"));
                        ui.selectable_value(&mut d.start, StartWith::Blank, t("⬜ Leer"));
                        ui.separator();
                        ui.selectable_value(&mut d.model, Model::Classic, t("Classic"));
                        ui.selectable_value(&mut d.model, Model::Slim, t("Slim"));
                    });
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let create = egui::Button::new(RichText::new(t("Erstellen")).strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(create).clicked() {
                        result = Some(true);
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        result = Some(false);
                    }
                });
            });
            if let Some(e) = pick_err {
                self.notify(ctx, e, true);
            }
            if clear_file {
                d.file = None;
            }
            if modal.should_close() && result.is_none() {
                result = Some(false);
            }
            match result {
                Some(true) => {
                    let name = if d.name.trim().is_empty() { t("Mein Skin").to_string() } else { d.name.trim().to_string() };
                    let res = match (d.file.take(), d.start) {
                        (Some((_, s, m)), _) => self.create_project(name, s, m),
                        (None, StartWith::Figure) => {
                            let s = skin::starter_skin();
                            let s = skin::convert_model(&s, Model::Classic, d.model);
                            self.create_project(name, s, d.model)
                        }
                        (None, StartWith::Blank) => self.create_project(name, Skin::blank(), d.model),
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
                ui.heading(t("Projekt umbenennen"));
                ui.add_space(6.0);
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                r.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    result = Some(true);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("✔ Übernehmen")).clicked() {
                        result = Some(true);
                    }
                    if ui.button(t("Abbrechen")).clicked() {
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
                    // offene Tabs aktualisieren
                    if let Some(p) = self.project.as_mut().filter(|p| p.id == id) {
                        p.name = name.clone();
                    }
                    for (m, _, _) in self.tabs.iter_mut().flatten() {
                        if m.id == id {
                            m.name = name.clone();
                        }
                    }
                    // nur die Projektinfo speichern, den Skin-Stand nicht ungefragt
                    if let Some(stored) = project::load(&id) {
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
                ui.heading(t("Projekt löschen?"));
                ui.label(t1("„{0}“ wird endgültig gelöscht.", &name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let del = egui::Button::new(RichText::new(t("🗑 Löschen")).color(Color32::WHITE)).fill(Color32::from_rgb(170, 50, 55));
                    if ui.add(del).clicked() {
                        result = Some(true);
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        result = Some(false);
                    }
                });
            });
            if modal.should_close() && result.is_none() {
                result = Some(false);
            }
            match result {
                Some(true) => {
                    if let Some(i) = self.tab_index_of(&id) {
                        self.close_tab_now(i);
                    }
                    project::delete(&id);
                    self.cards = None;
                    self.notify(ctx, t1("„{0}“ gelöscht", name), false);
                }
                Some(false) => {}
                None => self.delete_confirm = Some((id, name)),
            }
        }

        // Update installieren?
        if self.update_confirm {
            let version = match self.updater.state() {
                UpdateState::Available(v) => v,
                _ => {
                    self.update_confirm = false;
                    String::new()
                }
            };
            if self.update_confirm {
                let modal = egui::Modal::new(egui::Id::unique("update_confirm")).show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.heading(t("🔄 Update verfügbar"));
                    ui.label(t2("Version {0} ist verfügbar (du hast {1}).", &version, updater::VERSION));
                    ui.label(RichText::new(t("Das Update wird heruntergeladen und installiert. Deine Projekte bleiben erhalten.")).small().weak());
                    ui.add_space(10.0);
                    let mut r = 0;
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(RichText::new(t("Jetzt installieren")).strong().color(Color32::WHITE)).fill(ORANGE);
                        if ui.add(b).clicked() {
                            r = 1;
                        }
                        if ui.button(t("Später")).clicked() {
                            r = 2;
                        }
                    });
                    r
                });
                if modal.inner == 1 {
                    self.updater.install(ctx);
                    self.update_installing = true;
                    self.update_confirm = false;
                } else if modal.inner == 2 || modal.should_close() {
                    self.update_confirm = false;
                }
            }
        }

        // Update fehlgeschlagen → deutlich anzeigen
        if self.update_installing {
            match self.updater.state() {
                UpdateState::Failed(e) => {
                    self.update_installing = false;
                    self.update_error = Some(e);
                }
                UpdateState::Downloading(_) => {}
                _ => self.update_installing = false,
            }
        }
        if let Some(err) = self.update_error.clone() {
            let modal = egui::Modal::new(egui::Id::unique("update_error")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading(t("⚠ Update hat nicht geklappt"));
                ui.add_space(4.0);
                ui.label(RichText::new(&err).color(ORANGE));
                ui.add_space(6.0);
                ui.label(RichText::new(t("Tipp: Prüfe die Internetverbindung. Wenn der Virenschutz die Datei blockiert, versuch es nochmal oder lade die neue SkinStudio.exe von GitHub herunter und starte sie – sie aktualisiert die Installation.")).small().weak());
                ui.add_space(10.0);
                let mut r = 0;
                ui.horizontal(|ui| {
                    let b = egui::Button::new(RichText::new(t("🔄 Nochmal versuchen")).strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(b).clicked() {
                        r = 1;
                    }
                    if ui.button(t("Schließen")).clicked() {
                        r = 2;
                    }
                });
                r
            });
            if modal.inner == 1 {
                self.update_error = None;
                self.updater.check(ctx);
            } else if modal.inner == 2 || modal.should_close() {
                self.update_error = None;
            }
        }

        // Update fertig → neu starten?
        if !self.restart_prompted && matches!(self.updater.state(), UpdateState::Ready(_)) {
            self.restart_prompted = true;
            self.restart_prompt = true;
        }
        if self.restart_prompt {
            let modal = egui::Modal::new(egui::Id::unique("restart_prompt")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading(t("✔ Update installiert"));
                ui.label(t("Starte Skin Studio neu, um die neue Version zu benutzen."));
                ui.add_space(10.0);
                let mut r = 0;
                ui.horizontal(|ui| {
                    let b = egui::Button::new(RichText::new(t("Jetzt neu starten")).strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(b).clicked() {
                        r = 1;
                    }
                    if ui.button(t("Später")).clicked() {
                        r = 2;
                    }
                });
                r
            });
            if modal.inner == 1 {
                self.restart_prompt = false;
                self.restart_for_update(ctx);
            } else if modal.inner == 2 || modal.should_close() {
                self.restart_prompt = false;
            }
        }

        // Speichern?
        if let Some(pending) = self.pending {
            let (title, text) = match pending {
                Pending::CloseTab(i) => {
                    let name = self.tab_meta(i).map(|m| m.name.clone()).unwrap_or_default();
                    (t("💾 Projekt speichern?"), t1("Möchtest du deine Änderungen an „{0}“ speichern?", name))
                }
                _ => {
                    let n = self.unsaved_count();
                    if n == 1 {
                        let i = (0..self.tabs.len()).find(|&i| self.tab_modified(i)).unwrap_or(self.active);
                        let name = self.tab_meta(i).map(|m| m.name.clone()).unwrap_or_default();
                        (t("💾 Projekt speichern?"), t1("Möchtest du deine Änderungen an „{0}“ speichern?", name))
                    } else {
                        (t("💾 Projekte speichern?"), t1("{0} Projekte haben ungespeicherte Änderungen. Alle speichern?", n))
                    }
                }
            };
            let modal = egui::Modal::new(egui::Id::unique("save_prompt")).show(ctx, |ui| {
                ui.set_width(400.0);
                ui.heading(title);
                ui.add_space(4.0);
                ui.label(text);
                ui.label(RichText::new(t("Das Projekt bleibt im Programm – beim nächsten Start kannst du direkt weitermachen.")).small().weak());
                ui.add_space(10.0);
                let mut r = 0;
                ui.horizontal(|ui| {
                    let save = egui::Button::new(RichText::new(t("💾 Speichern")).strong().color(Color32::WHITE)).fill(ORANGE);
                    if ui.add(save).clicked() {
                        r = 1;
                    }
                    if ui.button(t("Nicht speichern")).clicked() {
                        r = 2;
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        r = 3;
                    }
                });
                r
            });
            let choice = if modal.inner == 0 && modal.should_close() { 3 } else { modal.inner };
            let proceed = match (choice, pending) {
                (1, Pending::CloseTab(i)) => {
                    if i == self.active {
                        self.save_project(ctx)
                    } else if let Some(Some((meta, ed, mx))) = self.tabs.get_mut(i) {
                        Self::save_pair(meta, ed, mx).is_ok()
                    } else {
                        true
                    }
                }
                (1, _) => self.save_all(ctx),
                (2, _) => true,
                (3, _) => {
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
                    Pending::Restart => self.do_restart(ctx),
                    Pending::CloseTab(i) => {
                        self.close_tab_now(i);
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
                format!("{}{} – Skin Studio", p.name, if self.tab_modified(self.active) { " •" } else { "" })
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
                        .add_filter(t("Minecraft-Skin (PNG)"), &["png"])
                        .set_title(t("Skins für den Mixer laden"))
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
                    self.notify(ctx, t("Es passen höchstens 26 Skins in den Mixer."), true);
                }
            }
            MixerAction::FromProject(at) => {
                if self.mixer.set_project_slot(at, self.editor.skin.clone(), self.editor.model).is_none() {
                    self.notify(ctx, t("Es passen höchstens 26 Skins in den Mixer."), true);
                }
            }
            MixerAction::Apply => {
                self.mixer.recompute();
                let (skin, raw, model) = (self.mixer.result.clone(), self.mixer.raw_result.clone(), self.mixer.result_model);
                self.editor.replace_skin(skin, model);
                self.mixer.applied_base = Some(raw);
                self.mixer.changed = true;
                self.screen = Screen::Editor;
                self.notify(ctx, t("✔ Mix ins Projekt übernommen – Änderungen im Editor fließen in den Mixer zurück (Strg+Z macht es rückgängig)"), false);
            }
            MixerAction::Export => self.export_mix(ctx),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Beim Start sichtbar prüfen, danach alle 5 Sekunden still im Hintergrund
        let now = ctx.input(|i| i.time);
        if !self.update_checked {
            self.update_checked = true;
            self.last_update_check = now;
            self.updater.check(&ctx);
        } else if now - self.last_update_check >= 5.0 {
            self.last_update_check = now;
            self.updater.check_quiet(&ctx);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((5.0 - (now - self.last_update_check)).max(0.1)));

        // Beim Schließen fragen, ob gespeichert werden soll
        if ctx.input(|i| i.viewport().close_requested()) && self.unsaved_count() > 0 && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Close);
        }

        self.handle_drops(&ctx);
        self.global_shortcuts(&ctx);
        if self.screen == Screen::Editor && !self.modal_open() {
            self.editor.handle_shortcuts(&ctx);
        }

        let show_tabs = !self.tabs.is_empty();
        if self.screen != Screen::Start {
            egui::Panel::top("top")
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: if show_tabs { 0 } else { 6 } }))
                .show(ui, |ui| {
                    self.top_bar(ui);
                    if show_tabs {
                        ui.add_space(4.0);
                        self.tab_bar(ui);
                    }
                });
        } else if show_tabs {
            egui::Panel::top("top_tabs")
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 0 }))
                .show(ui, |ui| self.tab_bar(ui));
        }
        if self.screen != Screen::Start {
            egui::Panel::bottom("status")
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 4)))
                .show(ui, |ui| self.status_bar(ui));
        }

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
                        ui.add_space(4.0);
                        self.editor.ui_parts(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        self.editor.ui_colors(ui);
                        ui.add_space(8.0);
                        ui.separator();
                        self.editor.ui_view_options(ui);
                    });
                });
                if self.editor.show_2d {
                    egui::Panel::right("tex2d").resizable(true).default_size(440.0).size_range(320.0..=1000.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("▶").on_hover_text(t("2D-Textur einklappen")).clicked() {
                                self.editor.show_2d = false;
                            }
                            ui.label(RichText::new(t("2D-TEXTUR")).small().strong().weak());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(t("Mausrad: Zoom · mittlere Taste: verschieben")).small().weak());
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
                if std::mem::take(&mut self.editor.import_requested) {
                    self.import_into_project(&ctx);
                }
            }
            Screen::Mixer => {
                if self.project.is_none() {
                    self.screen = Screen::Start;
                }
                // Änderungen aus dem Editor übernehmen
                self.mixer.sync_from_editor(&self.editor.skin);
                self.mixer.recompute();
                let mut action = MixerAction::None;
                egui::Panel::left("mix_in").resizable(false).exact_size(300.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| action = self.mixer.ui_inputs(ui));
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

// ------------------------------------------------------------------ Installer-Fenster

struct InstallerApp {
    inst: installer::Installer,
    logo: Option<egui::TextureHandle>,
    portable_requested: Arc<std::sync::atomic::AtomicBool>,
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let logo = self
            .logo
            .get_or_insert_with(|| {
                ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_gen::icon_rgba(64)), egui::TextureOptions::NEAREST)
            })
            .id();
        match self.inst.ui(ui, logo) {
            installer::InstallerResult::None => {}
            installer::InstallerResult::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            installer::InstallerResult::RunPortable => {
                self.portable_requested.store(true, std::sync::atomic::Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

struct UninstallerApp {
    un: installer::Uninstaller,
    logo: Option<egui::TextureHandle>,
}

impl eframe::App for UninstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let logo = self
            .logo
            .get_or_insert_with(|| {
                ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_gen::icon_rgba(64)), egui::TextureOptions::NEAREST)
            })
            .id();
        if self.un.ui(ui, logo) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
        p.text(rect.right_bottom() - egui::vec2(18.0, 18.0), egui::Align2::RIGHT_CENTER, t("Starten  ➡"), egui::FontId::proportional(13.0), if hovered { ORANGE } else { Color32::from_gray(140) });
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Eingeklappte 2D-Textur: schmale Leiste mit Pfeil zum Ausklappen.
fn collapsed_strip(ui: &mut egui::Ui, open: &mut bool) {
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, Sense::click()).on_hover_text(t("2D-Textur ausklappen"));
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 0.0, Color32::from_white_alpha(8));
    }
    let col = if resp.hovered() { ORANGE } else { Color32::from_gray(180) };
    p.text(rect.center_top() + egui::vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, "◀", egui::FontId::proportional(16.0), col);
    for (k, ch) in t("2D-TEXTUR").chars().enumerate() {
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

fn icon() -> Arc<egui::IconData> {
    Arc::new(egui::IconData { rgba: icon_gen::icon_rgba(64), width: 64, height: 64 })
}

/// Startet ein Fenster; bei Grafikproblemen wird schrittweise zurückgeschaltet.
fn run_window(
    viewport: egui::ViewportBuilder,
    make: impl Fn(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> eframe::Result {
    // Bevorzugt: Grafikkarte mit Tiefenpuffer und Kantenglättung (MSAA).
    let attempts: [(u16, u8); 3] = [(4, 24), (0, 24), (0, 0)];
    let mut last_err = None;
    for (msaa, depth) in attempts {
        gpu3d::GPU_ENABLED.store(depth > 0, std::sync::atomic::Ordering::Relaxed);
        let options = eframe::NativeOptions {
            viewport: viewport.clone(),
            multisampling: msaa,
            depth_buffer: depth,
            ..Default::default()
        };
        match eframe::run_native("Skin Studio", options, Box::new(|cc| Ok(make(cc)))) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("Start mit MSAA {msaa} / Tiefe {depth} fehlgeschlagen: {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("{}", updater::VERSION_MARKER);
        return Ok(());
    }
    let settings = project::load_settings();
    i18n::set_lang(settings.lang.unwrap_or_else(i18n::system_lang));

    if args.iter().any(|a| a == "--uninstall") {
        let vp = egui::ViewportBuilder::default()
            .with_title("Skin Studio")
            .with_inner_size([520.0, 300.0])
            .with_resizable(false)
            .with_icon(icon());
        return run_window(vp, |cc| {
            setup_style(&cc.egui_ctx);
            Box::new(UninstallerApp { un: installer::Uninstaller::new(), logo: None })
        });
    }

    if installer::should_show_installer(&args) {
        let portable = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let p2 = portable.clone();
        let vp = egui::ViewportBuilder::default()
            .with_title(t("Skin Studio – Installation"))
            .with_inner_size([640.0, 460.0])
            .with_resizable(false)
            .with_icon(icon());
        run_window(vp, move |cc| {
            setup_style(&cc.egui_ctx);
            Box::new(InstallerApp { inst: installer::Installer::new(), logo: None, portable_requested: p2.clone() })
        })?;
        if !portable.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        // „Ohne Installation verwenden“ → direkt weiter ins Programm
        if let Some(l) = project::load_settings().lang {
            i18n::set_lang(l);
        }
    }

    let vp = egui::ViewportBuilder::default()
        .with_title("Skin Studio")
        .with_inner_size([1440.0, 880.0])
        .with_maximized(true)
        .with_min_inner_size([1060.0, 640.0])
        .with_drag_and_drop(true)
        .with_icon(icon());
    run_window(vp, |cc| Box::new(App::new(cc)))
}
