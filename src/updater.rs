//! Selbst-Update: prüft im Hintergrund, ob es eine neue Version gibt, lädt sie herunter
//! und tauscht die laufende exe aus – ohne dass man selbst eine neue Datei herunterladen muss.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::i18n::{t, t1};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Hier liegen `version.txt` (z. B. „1.2.0“) und `SkinStudio.exe` der neuesten Version.
pub const UPDATE_BASE: &str = "https://raw.githubusercontent.com/hullihup/.exe/main/";

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Downloading(String),
    /// Neue Version liegt bereit – Neustart nötig
    Ready(String),
    /// Server nicht erreichbar o. Ä.
    Failed(String),
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<State>>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater { state: Arc::new(Mutex::new(State::Idle)) }
    }
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches('v').split('.').map(|p| p.trim().parse().unwrap_or(0)).collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (r, l) = (parse_version(remote), parse_version(local));
    for i in 0..r.len().max(l.len()) {
        let (a, b) = (r.get(i).copied().unwrap_or(0), l.get(i).copied().unwrap_or(0));
        if a != b {
            return a > b;
        }
    }
    false
}

/// Adresse der Update-Dateien (zum Testen per Umgebungsvariable änderbar).
fn base_url() -> String {
    std::env::var("SKIN_STUDIO_UPDATE_URL").unwrap_or_else(|_| UPDATE_BASE.to_string())
}

#[cfg(windows)]
fn hide(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
fn hide(_: &mut std::process::Command) {}

/// Lädt eine URL mit dem in Windows eingebauten `curl.exe` (Windows 10/11),
/// ersatzweise mit PowerShell. So braucht das Programm keinen eigenen HTTPS-Code.
pub fn http_get(url: &str, out: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    cmd.args(["-fsSL", "--max-time", "120", "-A", "SkinStudio-Updater"]);
    if let Some(p) = out {
        cmd.arg("-o").arg(p);
    }
    cmd.arg(url);
    hide(&mut cmd);
    match cmd.output() {
        Ok(res) if res.status.success() => return Ok(res.stdout),
        Ok(_) => return Err(t("Der Update-Server ist nicht erreichbar.").into()),
        Err(e) if !cfg!(windows) => return Err(t1("Download nicht möglich: {0}", e)),
        Err(_) => {}
    }
    // Windows ohne curl.exe: PowerShell
    let script = match out {
        Some(p) => format!(
            "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile '{}'",
            p.display()
        ),
        None => format!("$ProgressPreference='SilentlyContinue'; (Invoke-WebRequest -UseBasicParsing -Uri '{url}').Content"),
    };
    let mut ps = std::process::Command::new("powershell");
    ps.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    hide(&mut ps);
    let res = ps.output().map_err(|e| t1("Download nicht möglich: {0}", e))?;
    if !res.status.success() {
        return Err(t("Der Update-Server ist nicht erreichbar.").into());
    }
    Ok(res.stdout)
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Idle)
    }

    fn set(&self, s: State) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
    }

    /// Im Hintergrund nach einer neuen Version suchen.
    pub fn check(&self, ctx: &egui::Context) {
        if matches!(self.state(), State::Checking | State::Downloading(_)) {
            return;
        }
        self.set(State::Checking);
        let me = self.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = http_get(&format!("{}version.txt", base_url()), None);
            let new_state = match result {
                Ok(bytes) => {
                    let remote = String::from_utf8_lossy(&bytes).trim().to_string();
                    if parse_version(&remote).iter().all(|&n| n == 0) {
                        State::Failed(t("Ungültige Versionsangabe auf dem Server.").into())
                    } else if is_newer(&remote, VERSION) {
                        State::Available(remote)
                    } else {
                        State::UpToDate
                    }
                }
                Err(e) => State::Failed(e),
            };
            me.set(new_state);
            ctx.request_repaint();
        });
    }

    /// Neue Version herunterladen und die laufende exe ersetzen.
    pub fn install(&self, ctx: &egui::Context) {
        let State::Available(version) = self.state() else { return };
        self.set(State::Downloading(version.clone()));
        let me = self.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = download_and_swap();
            me.set(match r {
                Ok(()) => State::Ready(version),
                Err(e) => State::Failed(e),
            });
            ctx.request_repaint();
        });
    }
}

fn download_and_swap() -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = me.parent().ok_or("?")?.to_path_buf();
    let new_file = dir.join("SkinStudio.update.exe");
    let _ = std::fs::remove_file(&new_file);
    http_get(&format!("{}SkinStudio.exe", base_url()), Some(&new_file))?;
    // Prüfen, ob wirklich ein Windows-Programm angekommen ist
    let data = std::fs::read(&new_file).map_err(|e| e.to_string())?;
    if data.len() < 1_000_000 || &data[..2] != b"MZ" {
        let _ = std::fs::remove_file(&new_file);
        return Err(t("Die heruntergeladene Datei ist beschädigt.").into());
    }
    // Die laufende exe kann umbenannt (aber nicht überschrieben) werden
    let old = dir.join("SkinStudio.old.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&me, &old).map_err(|e| t1("Update fehlgeschlagen: {0}", e))?;
    if let Err(e) = std::fs::rename(&new_file, &me) {
        let _ = std::fs::rename(&old, &me);
        return Err(t1("Update fehlgeschlagen: {0}", e));
    }
    Ok(())
}

/// Nach einem Update: alte exe aufräumen.
pub fn cleanup_old() {
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            let _ = std::fs::remove_file(dir.join("SkinStudio.old.exe"));
            let _ = std::fs::remove_file(dir.join("SkinStudio.update.exe"));
        }
    }
}

/// Startet die (neue) exe und gibt ihren Pfad zurück.
pub fn restart() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    std::process::Command::new(&me).arg("--updated").spawn().ok()?;
    Some(me)
}

/// Update-Symbol (zwei Pfeile im Kreis), grau oder grün.
pub fn paint_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    use std::f32::consts::PI;
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.34;
    let stroke = egui::Stroke::new(r * 0.32, color);
    for half in [0.0, PI] {
        let pts: Vec<egui::Pos2> = (0..=14)
            .map(|k| {
                let a = half + 0.35 + (PI - 0.7) * k as f32 / 14.0;
                c + egui::vec2(a.cos(), a.sin()) * r
            })
            .collect();
        let tip = *pts.last().unwrap();
        p.add(egui::Shape::line(pts, stroke));
        // Pfeilspitze
        let a = half + PI - 0.35;
        let dir = egui::vec2(-a.sin(), a.cos());
        let out = egui::vec2(a.cos(), a.sin());
        let s = r * 0.55;
        p.add(egui::Shape::convex_polygon(
            vec![tip + dir * s * 0.9, tip + out * s * 0.6, tip - out * s * 0.6],
            color,
            egui::Stroke::NONE,
        ));
    }
}
