#![windows_subsystem = "windows"]

mod switcher;

use std::sync::{Arc, Mutex};
use switcher::State;

enum Dialog {
    None,
    NameFirst(String),
    New(String),
    Rename(String, String),
    Delete(String),
}

struct App {
    st: State,
    dialog: Dialog,
    error: Option<String>,
    info: Option<String>,
    restart_app: bool,
    busy: Option<Arc<Mutex<Option<Result<String, String>>>>>,
}

impl App {
    fn new() -> Self {
        let st = switcher::load_state();
        let dialog = if st.active.is_none() {
            Dialog::NameFirst(switcher::current_cli_email().unwrap_or_default())
        } else {
            Dialog::None
        };
        App { st, dialog, error: None, info: None, restart_app: true, busy: None }
    }

    fn start_switch(&mut self, target: String, ctx: &egui::Context) {
        self.error = None;
        self.info = None;
        let slot = Arc::new(Mutex::new(None));
        self.busy = Some(slot.clone());
        let mut st = self.st.clone();
        let restart = self.restart_app;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = switcher::switch_to(&mut st, &target).and_then(|r| {
                if restart || r.was_running {
                    switcher::start_desktop(&r.launch)?;
                }
                Ok(format!("Gewechselt zu „{target}“."))
            });
            *slot.lock().unwrap() = Some(res);
            ctx.request_repaint();
        });
    }

    fn poll_busy(&mut self) {
        let done = self.busy.as_ref().and_then(|s| s.lock().unwrap().take());
        if let Some(res) = done {
            self.busy = None;
            self.st = switcher::load_state();
            match res {
                Ok(m) => self.info = Some(m),
                Err(e) => self.error = Some(e),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_busy();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading("Claude – Account wechseln");
            ui.label(egui::RichText::new("Für die Claude-Desktop-App und Claude Code").weak());
            ui.add_space(8.0);
            ui.separator();

            if self.busy.is_some() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label("Wechsle Konto … Claude wird kurz geschlossen.");
                });
                return;
            }

            let mut switch: Option<String> = None;
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for name in self.st.profiles.clone() {
                    let active = self.st.active.as_deref() == Some(name.as_str());
                    ui.horizontal(|ui| {
                        let label = if active {
                            egui::RichText::new(format!("● {name}")).strong()
                        } else {
                            egui::RichText::new(format!("○ {name}"))
                        };
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !active && ui.small_button("🗑").on_hover_text("Löschen").clicked() {
                                self.dialog = Dialog::Delete(name.clone());
                            }
                            if ui.small_button("✏").on_hover_text("Umbenennen").clicked() {
                                self.dialog = Dialog::Rename(name.clone(), name.clone());
                            }
                            if active {
                                ui.label(egui::RichText::new("aktiv").weak());
                            } else if ui.button("Wechseln").clicked() {
                                switch = Some(name.clone());
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
            if let Some(t) = switch {
                self.start_switch(t, &ctx);
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("➕ Konto hinzufügen").clicked() && self.st.active.is_some() {
                    self.dialog = Dialog::New(String::new());
                }
                ui.checkbox(&mut self.restart_app, "Claude danach starten");
            });

            if let Some(e) = &self.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), e);
            }
            if let Some(i) = &self.info {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(90, 180, 110), i);
            }
        });

        self.dialogs(&ctx);
    }
}

impl App {
    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut action: Option<Box<dyn FnOnce(&mut App, &egui::Context)>> = None;
        match &mut self.dialog {
            Dialog::None => {}
            Dialog::NameFirst(name) => {
                modal(ctx, "Aktuelles Konto benennen", |ui| {
                    ui.label("Mit welchem Konto bist du gerade in Claude angemeldet?");
                    ui.label(egui::RichText::new("z. B. „Privat“, „Arbeit“ oder deine E-Mail").weak());
                    ui.text_edit_singleline(name);
                    if ui.button("Speichern").clicked() {
                        let n = name.clone();
                        action = Some(Box::new(move |a, _| match switcher::add_first(&mut a.st, &n) {
                            Ok(()) => a.dialog = Dialog::None,
                            Err(e) => a.error = Some(e),
                        }));
                    }
                });
            }
            Dialog::New(name) => {
                modal(ctx, "Konto hinzufügen", |ui| {
                    ui.label("Name für das neue Konto:");
                    ui.text_edit_singleline(name);
                    ui.label(
                        egui::RichText::new(
                            "Claude wird geschlossen und ohne Anmeldung neu gestartet.\nMelde dich dort mit dem neuen Konto an. Dein bisheriges Konto bleibt gespeichert.",
                        )
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Hinzufügen & wechseln").clicked() {
                            let n = name.clone();
                            action = Some(Box::new(move |a, ctx| match switcher::valid_name(&a.st, &n, None) {
                                Ok(n) => {
                                    a.dialog = Dialog::None;
                                    a.start_switch(n, ctx);
                                }
                                Err(e) => a.error = Some(e),
                            }));
                        }
                        if ui.button("Abbrechen").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::Rename(old, new) => {
                modal(ctx, "Umbenennen", |ui| {
                    ui.text_edit_singleline(new);
                    ui.horizontal(|ui| {
                        if ui.button("Speichern").clicked() {
                            let (o, n) = (old.clone(), new.clone());
                            action = Some(Box::new(move |a, _| match switcher::rename_profile(&mut a.st, &o, &n) {
                                Ok(()) => a.dialog = Dialog::None,
                                Err(e) => a.error = Some(e),
                            }));
                        }
                        if ui.button("Abbrechen").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::Delete(name) => {
                modal(ctx, "Konto löschen", |ui| {
                    ui.label(format!("Gespeichertes Konto „{name}“ wirklich löschen?\nDu musst dich danach dort neu anmelden."));
                    ui.horizontal(|ui| {
                        if ui.button("Löschen").clicked() {
                            let n = name.clone();
                            action = Some(Box::new(move |a, _| {
                                if let Err(e) = switcher::delete_profile(&mut a.st, &n) {
                                    a.error = Some(e);
                                }
                                a.dialog = Dialog::None;
                            }));
                        }
                        if ui.button("Abbrechen").clicked() {
                            close = true;
                        }
                    });
                });
            }
        }
        if close {
            self.dialog = Dialog::None;
        }
        if let Some(f) = action {
            self.error = None;
            f(self, ctx);
        }
    }
}

fn modal(ctx: &egui::Context, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, body);
}

fn main() -> eframe::Result {
    let vp = egui::ViewportBuilder::default()
        .with_title("Claude – Account wechseln")
        .with_inner_size([440.0, 420.0])
        .with_min_inner_size([380.0, 300.0]);
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native("Claude Account wechseln", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}
