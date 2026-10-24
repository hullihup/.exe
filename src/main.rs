#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Skin Studio – ein schlanker Minecraft-Skin-Editor mit Skin-Mixer.

mod color;
mod editor;
mod icon_gen;
mod mixer;
mod preview;
mod render3d;
mod skin;
mod ui_util;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Color32, Key, Modifiers, RichText};

use editor::{detect_model, Editor};
use mixer::{Mixer, MixerAction};
use skin::{Layer, Model, Skin};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Editor,
    Mixer,
}

struct App {
    tab: Tab,
    editor: Editor,
    mixer: Mixer,
    toast: Option<(String, f64, bool)>,
    show_help: bool,
    confirm_close: bool,
    allow_close: bool,
    confirm_new: bool,
    title: String,
    logo: Option<egui::TextureHandle>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let mut app = App {
            tab: Tab::Editor,
            editor: Editor::new(),
            mixer: Mixer::new(),
            toast: None,
            show_help: false,
            confirm_close: false,
            allow_close: false,
            confirm_new: false,
            title: String::new(),
            logo: None,
        };
        // Datei per Kommandozeile / "Öffnen mit"
        if let Some(arg) = std::env::args().nth(1) {
            app.open_path(Path::new(&arg));
        }
        app
    }

    fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>, error: bool) {
        let t = ctx.input(|i| i.time);
        self.toast = Some((text.into(), t + if error { 5.0 } else { 2.6 }, error));
    }

    fn read_skin(path: &Path) -> Result<(Skin, Model), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Datei konnte nicht gelesen werden: {e}"))?;
        let s = Skin::from_png(&bytes)?;
        let m = detect_model(&s);
        Ok((s, m))
    }

    fn file_name(path: &Path) -> String {
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Skin".into())
    }

    fn open_path(&mut self, path: &Path) -> Option<String> {
        match Self::read_skin(path) {
            Ok((s, m)) => {
                self.editor.load(s, m, Some(path.to_path_buf()));
                None
            }
            Err(e) => Some(e),
        }
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

    fn write_file(&mut self, ctx: &egui::Context, path: &Path, data: &[u8]) -> bool {
        match std::fs::write(path, data) {
            Ok(_) => {
                self.notify(ctx, format!("✔ Gespeichert: {}", Self::file_name(path)), false);
                true
            }
            Err(e) => {
                self.notify(ctx, format!("Speichern fehlgeschlagen: {e}"), true);
                false
            }
        }
    }

    fn action_open(&mut self, ctx: &egui::Context) {
        if let Some(p) = Self::pick_png() {
            if let Some(e) = self.open_path(&p) {
                self.notify(ctx, e, true);
            } else {
                self.tab = Tab::Editor;
                self.notify(ctx, format!("Geöffnet: {}", Self::file_name(&p)), false);
            }
        }
    }

    fn action_save(&mut self, ctx: &egui::Context, save_as: bool) {
        let path = match (&self.editor.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let name = self.editor.path.as_deref().map(Self::file_name).unwrap_or_else(|| "mein_skin.png".into());
                Self::save_png_dialog(&name, "Skin speichern")
            }
        };
        if let Some(p) = path {
            let mut s = self.editor.skin.clone();
            s.clean_unused(self.editor.model);
            if self.write_file(ctx, &p, &s.to_png()) {
                self.editor.path = Some(p);
                self.editor.modified = false;
            }
        }
    }

    fn action_export(&mut self, ctx: &egui::Context) {
        match self.tab {
            Tab::Editor => self.action_save(ctx, true),
            Tab::Mixer => self.export_mix(ctx),
        }
    }

    fn export_mix(&mut self, ctx: &egui::Context) {
        if let Some(p) = Self::save_png_dialog("partnerlook_skin.png", "Gemixten Skin exportieren") {
            let data = self.mixer.result.to_png();
            self.write_file(ctx, &p, &data);
        }
    }

    fn export_render(&mut self, ctx: &egui::Context) {
        let (skin, model, prev, vis) = match self.tab {
            Tab::Editor => (&self.editor.skin, self.editor.model, &self.editor.preview, self.editor.vis),
            Tab::Mixer => (&self.mixer.result, self.mixer.result_model, &self.mixer.preview, self.mixer.vis),
        };
        let (w, h) = (900, 1200);
        let rgba = prev.render_image(skin, model, vis, w, h);
        if let Some(p) = Self::save_png_dialog("skin_vorschau.png", "3D-Vorschau als Bild speichern") {
            let data = skin::encode_png(w as u32, h as u32, &rgba);
            self.write_file(ctx, &p, &data);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let path = f.path().to_path_buf();
            let result = f.bytes().and_then(|b| Skin::from_png(&b));
            match result {
                Ok(s) => {
                    let m = detect_model(&s);
                    match self.tab {
                        Tab::Editor => {
                            self.editor.load(s, m, Some(path.clone()));
                            self.notify(ctx, format!("Geöffnet: {}", Self::file_name(&path)), false);
                        }
                        Tab::Mixer => {
                            let b = self.mixer.first_free_is_b();
                            self.mixer.set_slot(b, s, m, Self::file_name(&path));
                            self.notify(ctx, format!("Als Skin {} geladen", if b { "B" } else { "A" }), false);
                        }
                    }
                }
                Err(e) => self.notify(ctx, e, true),
            }
        }
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m, k| egui::KeyboardShortcut::new(m, k);
        let (open, save, save_as, export, new) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::E)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::N)),
            )
        });
        if open {
            self.action_open(ctx);
        }
        if save_as {
            self.action_save(ctx, true);
        } else if save {
            self.action_save(ctx, false);
        }
        if export {
            self.action_export(ctx);
        }
        if new {
            self.confirm_new = true;
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_help = !self.show_help;
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            let logo = self.logo.get_or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([32, 32], &icon_gen::icon_rgba(32));
                ctx.load_texture("logo", img, egui::TextureOptions::NEAREST)
            });
            ui.add(egui::Image::new((logo.id(), egui::vec2(20.0, 20.0))));
            ui.label(RichText::new("Skin Studio").strong().color(Color32::WHITE));
            ui.add_space(6.0);
            ui.menu_button("Datei", |ui| {
                if ui.button("Neu…  (Strg+N)").clicked() {
                    self.confirm_new = true;
                }
                if ui.button("Öffnen…  (Strg+O)").clicked() {
                    self.action_open(&ctx);
                }
                ui.separator();
                if ui.button("Speichern  (Strg+S)").clicked() {
                    self.action_save(&ctx, false);
                }
                if ui.button("Speichern unter…  (Strg+Umschalt+S)").clicked() {
                    self.action_save(&ctx, true);
                }
                if ui.button("3D-Vorschau als Bild…").clicked() {
                    self.export_render(&ctx);
                }
                ui.separator();
                if ui.button("Beenden").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Bearbeiten", |ui| {
                let e = &mut self.editor;
                if ui.add_enabled(e.can_undo(), egui::Button::new("Rückgängig  (Strg+Z)")).clicked() {
                    e.undo();
                }
                if ui.add_enabled(e.can_redo(), egui::Button::new("Wiederholen  (Strg+Y)")).clicked() {
                    e.redo();
                }
                ui.separator();
                if ui.button("🎨 Farben anpassen…").clicked() {
                    e.start_adjust();
                    self.tab = Tab::Editor;
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
                let label = if target == Model::Slim { "Arme zu Slim (3 px) umrechnen" } else { "Arme zu Classic (4 px) umrechnen" };
                if ui.button(label).clicked() {
                    e.convert_arms(target);
                }
            });
            ui.menu_button("Ansicht", |ui| {
                ui.checkbox(&mut self.editor.show_grid, "Pixelraster");
                ui.checkbox(&mut self.editor.show_regions, "Körperteil-Bereiche");
                ui.checkbox(&mut self.editor.preview.antialias, "3D-Kantenglättung");
            });
            ui.menu_button("Hilfe", |ui| {
                if ui.button("Tastenkürzel  (F1)").clicked() {
                    self.show_help = true;
                }
            });

            ui.add_space(18.0);
            ui.selectable_value(&mut self.tab, Tab::Editor, RichText::new("🎨  Editor").size(14.0));
            ui.selectable_value(&mut self.tab, Tab::Mixer, RichText::new("👫  Skin-Mixer").size(14.0));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let accent = ui.visuals().selection.bg_fill;
                let btn = egui::Button::new(RichText::new("⬇  Exportieren").strong().color(Color32::WHITE))
                    .fill(accent)
                    .min_size(egui::vec2(130.0, 26.0));
                if ui.add(btn).on_hover_text("Skin als PNG speichern (Strg+E)").clicked() {
                    self.action_export(&ctx);
                }
                if self.tab == Tab::Editor && ui.button("📂 Öffnen").clicked() {
                    self.action_open(&ctx);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match self.tab {
                Tab::Editor => {
                    let e = &self.editor;
                    ui.label(RichText::new(e.tool.name()).strong());
                    ui.separator();
                    ui.monospace(e.status_text());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("Zoom {} %", e.zoom_percent()));
                        ui.separator();
                        ui.label(e.model.name());
                        if e.mirror {
                            ui.separator();
                            ui.label(RichText::new("↔ Spiegeln an").color(Color32::from_rgb(255, 170, 60)));
                        }
                        ui.separator();
                        ui.label(RichText::new("Mausrad: Zoom · Mittlere Taste/Leertaste: verschieben · Alt+Klick: Pipette").weak());
                    });
                }
                Tab::Mixer => {
                    ui.label("Skin-Mixer – kombiniere zwei Skins zu einem neuen.");
                }
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
                    ("L", "Linie"),
                    ("U", "Rechteck"),
                    ("I", "Pipette (oder Alt + Klick)"),
                    ("M", "Spiegeln an/aus"),
                    ("X", "Haupt- und Zweitfarbe tauschen"),
                    ("[  ]  /  − +", "Pinselgröße"),
                    ("0", "Zoom zurücksetzen"),
                    ("Strg+Z / Strg+Y", "Rückgängig / Wiederholen"),
                    ("Strg+O / Strg+S", "Öffnen / Speichern"),
                    ("Strg+E", "Exportieren"),
                    ("Rechtsklick", "Mit Zweitfarbe malen (2D)"),
                    ("Rechts ziehen", "3D-Modell drehen"),
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

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_new {
            let modal = egui::Modal::new(egui::Id::unique("new_modal")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading("Neuer Skin");
                ui.label("Womit möchtest du starten?");
                ui.add_space(8.0);
                let mut close = false;
                ui.horizontal(|ui| {
                    if ui.button("👤 Start-Figur").clicked() {
                        self.editor.load(skin::starter_skin(), Model::Classic, None);
                        close = true;
                    }
                    if ui.button("⬜ Leer").clicked() {
                        self.editor.load(Skin::blank(), self.editor.model, None);
                        close = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        close = true;
                    }
                });
                close
            });
            if modal.inner || modal.should_close() {
                self.confirm_new = false;
                self.tab = Tab::Editor;
            }
        }
        if self.confirm_close {
            let modal = egui::Modal::new(egui::Id::unique("close_modal")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading("Ungespeicherte Änderungen");
                ui.label("Dein Skin wurde noch nicht gespeichert. Trotzdem beenden?");
                ui.add_space(8.0);
                let mut r = 0;
                ui.horizontal(|ui| {
                    if ui.button("💾 Speichern").clicked() {
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
            match modal.inner {
                1 => {
                    self.confirm_close = false;
                    self.action_save(ctx, false);
                    if !self.editor.modified {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                2 => {
                    self.confirm_close = false;
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                3 => self.confirm_close = false,
                _ => {
                    if modal.should_close() {
                        self.confirm_close = false;
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
        let name = self.editor.path.as_deref().map(Self::file_name).unwrap_or_else(|| "Unbenannt".into());
        let title = format!("{}{} – Skin Studio", name, if self.editor.modified { " •" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Schließen mit ungespeicherten Änderungen abfangen
        if ctx.input(|i| i.viewport().close_requested()) && self.editor.modified && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }

        self.handle_drops(&ctx);
        self.global_shortcuts(&ctx);
        if self.tab == Tab::Editor {
            self.editor.handle_shortcuts(&ctx);
        }

        egui::Panel::top("top")
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 6)))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| self.status_bar(ui));

        match self.tab {
            Tab::Editor => {
                egui::Panel::left("tools").resizable(false).exact_size(262.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.editor.ui_tools(ui));
                });
                egui::Panel::right("colors").resizable(false).exact_size(250.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.editor.ui_colors(ui);
                        ui.add_space(8.0);
                        ui.separator();
                        self.editor.ui_view_options(ui);
                    });
                });

                egui::Panel::right("preview3d").resizable(true).default_size(330.0).size_range(220.0..=700.0).show(ui, |ui| {
                    self.editor.ui_preview(ui);
                });
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)))
                    .show(ui, |ui| self.editor.ui_canvas(ui));
                self.editor.ui_adjust_window(&ctx);
            }
            Tab::Mixer => {
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

impl App {
    fn mixer_action(&mut self, ctx: &egui::Context, action: MixerAction) {
        match action {
            MixerAction::None => {}
            MixerAction::LoadA | MixerAction::LoadB => {
                let b = matches!(action, MixerAction::LoadB);
                if let Some(p) = Self::pick_png() {
                    match Self::read_skin(&p) {
                        Ok((s, m)) => self.mixer.set_slot(b, s, m, Self::file_name(&p)),
                        Err(e) => self.notify(ctx, e, true),
                    }
                }
            }
            MixerAction::FromEditorA | MixerAction::FromEditorB => {
                let b = matches!(action, MixerAction::FromEditorB);
                let name = self.editor.path.as_deref().map(Self::file_name).unwrap_or_else(|| "Editor-Skin".into());
                self.mixer.set_slot(b, self.editor.skin.clone(), self.editor.model, name);
            }
            MixerAction::OpenInEditor => {
                self.editor.replace_skin(self.mixer.result.clone(), self.mixer.result_model);
                self.editor.path = None;
                self.tab = Tab::Editor;
                self.notify(ctx, "Gemixter Skin ist jetzt im Editor (Strg+Z macht es rückgängig)", false);
            }
            MixerAction::Export => self.export_mix(ctx),
        }
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
            .with_min_inner_size([1060.0, 640.0])
            .with_drag_and_drop(true)
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native("Skin Studio", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
