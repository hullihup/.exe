//! Installation als „richtige“ Windows-App: Programmordner, Startmenü (Windows-Suche),
//! optional Desktop-Verknüpfung, Eintrag unter „Apps“ und Deinstallation.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::i18n::{self, t, t1, Lang};

pub const APP_NAME: &str = "Skin Studio";
pub const EXE_NAME: &str = "SkinStudio.exe";
#[cfg(windows)]
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\SkinStudio";

/// Standard-Installationsordner: %LOCALAPPDATA%\Programs\Skin Studio
pub fn default_install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::project::data_dir().parent().map(Path::to_path_buf).unwrap_or_default());
    base.join("Programs").join(APP_NAME)
}

/// Wo ist Skin Studio installiert? (aus der Registry, sonst Standardordner)
pub fn installed_exe() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY) {
            if let Ok(loc) = k.get_value::<String, _>("InstallLocation") {
                let p = PathBuf::from(loc).join(EXE_NAME);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let p = default_install_dir().join(EXE_NAME);
    p.exists().then_some(p)
}

pub fn installed_version() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY) {
            return k.get_value::<String, _>("DisplayVersion").ok();
        }
    }
    None
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Soll beim Start der Installer erscheinen?
pub fn should_show_installer(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--portable" || a == "--no-install") {
        return false;
    }
    if std::env::var_os("SKIN_STUDIO_INSTALLER").is_some() {
        return true; // zum Testen
    }
    if !cfg!(windows) || crate::project::load_settings().portable {
        return false;
    }
    let Ok(me) = std::env::current_exe() else { return false };
    match installed_exe() {
        // läuft bereits aus dem Installationsordner
        Some(inst) if same_path(&inst, &me) => false,
        _ => true,
    }
}

#[cfg(windows)]
fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

fn start_menu_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs"))
}

fn desktop_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|a| PathBuf::from(a).join("Desktop"))
}

#[cfg(windows)]
fn make_shortcut(target: &Path, lnk: &Path) -> Result<(), String> {
    let mut sl = mslnk::ShellLink::new(target).map_err(|e| e.to_string())?;
    sl.set_working_dir(target.parent().map(|p| p.to_string_lossy().into_owned()));
    sl.set_icon_location(Some(target.to_string_lossy().into_owned()));
    sl.set_name(Some(APP_NAME.to_string()));
    sl.create_lnk(lnk).map_err(|e| e.to_string())
}

/// Führt die Installation aus. Gibt den Pfad der installierten exe zurück.
pub fn install(dir: &Path, desktop: bool) -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| t1("Ordner konnte nicht angelegt werden: {0}", e))?;
    let target = dir.join(EXE_NAME);
    if !same_path(&me, &target) {
        // alte Version zur Seite schieben (falls sie gerade läuft, klappt Umbenennen trotzdem)
        if target.exists() {
            let old = dir.join("SkinStudio.old.exe");
            let _ = std::fs::remove_file(&old);
            if std::fs::rename(&target, &old).is_err() {
                return Err(t("Skin Studio läuft noch. Bitte schließe es und versuche es erneut.").into());
            }
        }
        std::fs::copy(&me, &target).map_err(|e| t1("Kopieren fehlgeschlagen: {0}", e))?;
    }
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Some(sm) = start_menu_dir() {
            let _ = std::fs::create_dir_all(&sm);
            make_shortcut(&target, &sm.join(format!("{APP_NAME}.lnk")))?;
        }
        if desktop {
            if let Some(d) = desktop_dir() {
                let _ = make_shortcut(&target, &d.join(format!("{APP_NAME}.lnk")));
            }
        }
        // Eintrag unter Einstellungen → Apps
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY).map_err(|e| e.to_string())?;
        let exe = target.to_string_lossy().into_owned();
        let size_kb = std::fs::metadata(&target).map(|m| (m.len() / 1024) as u32).unwrap_or(7000);
        let _ = key.set_value("DisplayName", &APP_NAME.to_string());
        let _ = key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION").to_string());
        let _ = key.set_value("Publisher", &"Skin Studio".to_string());
        let _ = key.set_value("DisplayIcon", &exe);
        let _ = key.set_value("InstallLocation", &dir.to_string_lossy().into_owned());
        let _ = key.set_value("UninstallString", &format!("\"{exe}\" --uninstall"));
        let _ = key.set_value("NoModify", &1u32);
        let _ = key.set_value("NoRepair", &1u32);
        let _ = key.set_value("EstimatedSize", &size_kb);
    }
    #[cfg(not(windows))]
    let _ = desktop;
    Ok(target)
}

/// Entfernt Verknüpfungen, Registry-Eintrag und (verzögert) den Programmordner.
pub fn uninstall(delete_projects: bool) -> Result<(), String> {
    if let Some(sm) = start_menu_dir() {
        let _ = std::fs::remove_file(sm.join(format!("{APP_NAME}.lnk")));
    }
    if let Some(d) = desktop_dir() {
        let _ = std::fs::remove_file(d.join(format!("{APP_NAME}.lnk")));
    }
    if delete_projects {
        let _ = std::fs::remove_dir_all(crate::project::data_dir());
    }
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let dir = installed_exe().and_then(|p| p.parent().map(Path::to_path_buf));
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
        if let Some(dir) = dir {
            // Die laufende exe kann sich nicht selbst löschen: sobald das Fenster zu ist,
            // entfernt eine kleine Batch-Datei den Ordner (bis zu 60 Versuche, 1 pro Sekunde)
            // und löscht sich danach selbst.
            let d = dir.display();
            let bat = std::env::temp_dir().join("skinstudio_uninstall.bat");
            let script = format!(
                "@echo off\r\nfor /L %%i in (1,1,60) do (\r\n  ping -n 2 127.0.0.1 >nul\r\n  rmdir /s /q \"{d}\" 2>nul\r\n  if not exist \"{d}\" goto done\r\n)\r\n:done\r\ndel \"%~f0\"\r\n"
            );
            if std::fs::write(&bat, script).is_ok() {
                // nicht im Programmordner starten, sonst ist er gesperrt
                let _ = no_window(std::process::Command::new("cmd").arg("/C").arg(&bat).current_dir(std::env::temp_dir())).spawn();
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ Oberfläche

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Language,
    Options,
    Done,
}

pub struct Installer {
    step: Step,
    lang: Lang,
    dir: PathBuf,
    desktop: bool,
    error: Option<String>,
    installed: Option<PathBuf>,
    already: Option<String>,
}

impl Installer {
    pub fn new() -> Self {
        let lang = crate::project::load_settings().lang.unwrap_or_else(i18n::system_lang);
        i18n::set_lang(lang);
        let dir = installed_exe().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(default_install_dir);
        Installer {
            step: Step::Language,
            lang,
            dir,
            desktop: true,
            error: None,
            installed: None,
            already: installed_version(),
        }
    }

    /// Zeigt den Installer. Gibt `true` zurück, wenn das Fenster schließen soll.
    pub fn ui(&mut self, ui: &mut egui::Ui, logo: egui::TextureId) -> InstallerResult {
        let mut result = InstallerResult::None;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)).inner_margin(28)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new((logo, egui::vec2(48.0, 48.0))));
                ui.vertical(|ui| {
                    ui.label(RichText::new(APP_NAME).size(26.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new(t1("Installation · Version {0}", env!("CARGO_PKG_VERSION"))).weak());
                });
            });
            ui.add_space(14.0);
            // Fortschrittspunkte
            ui.horizontal(|ui| {
                for (k, (s, name)) in [(Step::Language, t("Sprache")), (Step::Options, t("Installieren")), (Step::Done, t("Fertig"))]
                    .into_iter()
                    .enumerate()
                {
                    let active = self.step == s;
                    let col = if active { crate::ORANGE } else { Color32::from_gray(110) };
                    ui.label(RichText::new(format!("{}  {}", k + 1, name)).color(col).strong());
                    if k < 2 {
                        ui.label(RichText::new("—").color(Color32::from_gray(80)));
                    }
                }
            });
            ui.separator();
            ui.add_space(10.0);

            match self.step {
                Step::Language => {
                    ui.label(RichText::new(t("Bitte wähle deine Sprache:")).size(16.0));
                    ui.add_space(8.0);
                    for l in [Lang::En, Lang::De] {
                        let selected = self.lang == l;
                        let btn = egui::Button::selectable(selected, RichText::new(l.label()).size(16.0)).min_size(egui::vec2(260.0, 40.0));
                        if ui.add(btn).clicked() {
                            self.lang = l;
                            i18n::set_lang(l);
                        }
                    }
                    ui.add_space(20.0);
                    if primary_button(ui, t("Weiter  ➡")).clicked() {
                        self.step = Step::Options;
                    }
                }
                Step::Options => {
                    if let Some(v) = &self.already {
                        ui.label(RichText::new(t1("Skin Studio {0} ist bereits installiert. Die Installation wird aktualisiert.", v)).color(crate::ORANGE));
                        ui.add_space(6.0);
                    }
                    ui.label(t("Skin Studio wird für dein Benutzerkonto installiert. Danach findest du es in der Windows-Suche und im Startmenü."));
                    ui.add_space(10.0);
                    ui.label(RichText::new(t("Installationsordner")).strong());
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 110.0).max(100.0);
                        ui.allocate_ui_with_layout(egui::vec2(w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(self.dir.display().to_string()).monospace()).truncate())
                                .on_hover_text(self.dir.display().to_string());
                        });
                        if ui.button(t("Ändern …")).clicked() {
                            if let Some(d) = rfd::FileDialog::new().set_directory(&self.dir).pick_folder() {
                                self.dir = d.join(APP_NAME);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.checkbox(&mut self.desktop, t("Verknüpfung auf dem Desktop erstellen"));
                    ui.label(RichText::new(t("Eine Verknüpfung im Startmenü wird immer erstellt.")).small().weak());
                    if let Some(e) = &self.error {
                        ui.add_space(6.0);
                        ui.label(RichText::new(e).color(Color32::from_rgb(240, 90, 90)));
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui.button(t("⬅  Zurück")).clicked() {
                            self.step = Step::Language;
                        }
                        if primary_button(ui, t("Installieren")).clicked() {
                            let mut settings = crate::project::load_settings();
                            settings.lang = Some(self.lang);
                            settings.portable = false;
                            crate::project::save_settings(&settings);
                            match install(&self.dir, self.desktop) {
                                Ok(p) => {
                                    self.installed = Some(p);
                                    self.error = None;
                                    self.step = Step::Done;
                                }
                                Err(e) => self.error = Some(e),
                            }
                        }
                    });
                    ui.add_space(10.0);
                    if ui
                        .link(RichText::new(t("Ohne Installation verwenden (portabel)")).small())
                        .on_hover_text(t("Startet Skin Studio direkt aus dieser Datei – ohne Startmenü-Eintrag."))
                        .clicked()
                    {
                        let mut settings = crate::project::load_settings();
                        settings.lang = Some(self.lang);
                        settings.portable = true;
                        crate::project::save_settings(&settings);
                        result = InstallerResult::RunPortable;
                    }
                }
                Step::Done => {
                    ui.label(RichText::new(t("✔ Skin Studio wurde installiert!")).size(18.0).color(Color32::from_rgb(110, 210, 120)));
                    ui.add_space(8.0);
                    ui.label(t("Du findest Skin Studio jetzt über die Windows-Suche (einfach „Skin Studio“ eintippen) und im Startmenü."));
                    ui.label(RichText::new(t("Diese heruntergeladene Datei kannst du löschen.")).weak());
                    ui.add_space(18.0);
                    if primary_button(ui, t("Skin Studio starten")).clicked() {
                        if let Some(p) = &self.installed {
                            let _ = std::process::Command::new(p).spawn();
                        }
                        result = InstallerResult::Close;
                    }
                }
            }
        });
        result
    }
}

pub enum InstallerResult {
    None,
    Close,
    RunPortable,
}

fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE).size(15.0))
            .fill(crate::ORANGE)
            .min_size(egui::vec2(170.0, 36.0)),
    )
}

/// Kleines Deinstallations-Fenster.
pub struct Uninstaller {
    delete_projects: bool,
    done: Option<Result<(), String>>,
}

impl Uninstaller {
    pub fn new() -> Self {
        if let Some(l) = crate::project::load_settings().lang {
            i18n::set_lang(l);
        }
        Uninstaller { delete_projects: false, done: None }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, logo: egui::TextureId) -> bool {
        let mut close = false;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 32, 38)).inner_margin(28)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new((logo, egui::vec2(40.0, 40.0))));
                ui.label(RichText::new(t("Skin Studio deinstallieren")).size(22.0).strong().color(Color32::WHITE));
            });
            ui.add_space(14.0);
            match &self.done {
                None => {
                    ui.label(t("Möchtest du Skin Studio wirklich von diesem PC entfernen?"));
                    ui.add_space(8.0);
                    ui.checkbox(&mut self.delete_projects, t("Auch alle meine Projekte löschen"));
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let del = egui::Button::new(RichText::new(t("Deinstallieren")).color(Color32::WHITE).strong())
                            .fill(Color32::from_rgb(170, 50, 55))
                            .min_size(egui::vec2(150.0, 34.0));
                        if ui.add(del).clicked() {
                            self.done = Some(uninstall(self.delete_projects));
                        }
                        if ui.button(t("Abbrechen")).clicked() {
                            close = true;
                        }
                    });
                }
                Some(Ok(())) => {
                    ui.label(RichText::new(t("✔ Skin Studio wurde entfernt.")).color(Color32::from_rgb(110, 210, 120)));
                    ui.add_space(12.0);
                    if ui.button(t("Schließen")).clicked() {
                        close = true;
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e.as_str()).color(Color32::from_rgb(240, 90, 90)));
                    if ui.button(t("Schließen")).clicked() {
                        close = true;
                    }
                }
            }
        });
        close
    }
}
