//! Projekte werden im Programm selbst gespeichert (im App-Datenordner),
//! nicht als einzelne Dateien, um die man sich kümmern muss.

use crate::i18n::{t, t1};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::skin::{Model, Skin};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub model: Model,
    /// Unix-Zeit der letzten Speicherung
    pub modified: u64,
}

pub struct StoredProject {
    pub meta: ProjectMeta,
    pub skin: Skin,
}

/// Ordner für alle Programmdaten (Windows: %APPDATA%\SkinStudio).
pub fn data_dir() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = var("APPDATA")
        .or_else(|| var("XDG_DATA_HOME"))
        .or_else(|| var("HOME").map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("SkinStudio")
}

/// Ordner eines Projekts
pub fn dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}

fn projects_dir() -> PathBuf {
    data_dir().join("projects")
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn new_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("p{n:x}")
}

pub fn save(meta: &ProjectMeta, skin: &Skin) -> Result<(), String> {
    let dir = projects_dir().join(&meta.id);
    std::fs::create_dir_all(&dir).map_err(|e| t1("Ordner konnte nicht angelegt werden: {0}", e))?;
    let model = if meta.model == Model::Slim { "slim" } else { "classic" };
    let info = format!("name={}\nmodel={}\nmodified={}\n", meta.name.replace('\n', " "), model, meta.modified);
    // Erst in temporäre Dateien schreiben, dann umbenennen: so geht nie ein Projekt kaputt.
    let write = |name: &str, data: &[u8]| -> Result<(), String> {
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, data).map_err(|e| t1("Speichern fehlgeschlagen: {0}", e))?;
        std::fs::rename(&tmp, dir.join(name)).map_err(|e| t1("Speichern fehlgeschlagen: {0}", e))
    };
    write("skin.png", &skin.to_png())?;
    write("project.txt", info.as_bytes())?;
    set_last(&meta.id);
    Ok(())
}

pub fn load(id: &str) -> Option<StoredProject> {
    let dir = projects_dir().join(id);
    let info = std::fs::read_to_string(dir.join("project.txt")).ok()?;
    let bytes = std::fs::read(dir.join("skin.png")).ok()?;
    let skin = Skin::from_png(&bytes).ok()?;
    let mut meta = ProjectMeta { id: id.to_string(), name: t("Projekt").into(), model: Model::Classic, modified: 0 };
    for line in info.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "name" => meta.name = v.to_string(),
                "model" => meta.model = if v == "slim" { Model::Slim } else { Model::Classic },
                "modified" => meta.modified = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    Some(StoredProject { meta, skin })
}

/// Alle Projekte, das zuletzt bearbeitete zuerst.
pub fn list() -> Vec<StoredProject> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(projects_dir()) {
        for e in entries.flatten() {
            if let Some(id) = e.file_name().to_str() {
                if let Some(p) = load(id) {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by(|a, b| b.meta.modified.cmp(&a.meta.modified));
    out
}

pub fn delete(id: &str) {
    if !id.is_empty() && !id.contains(['/', '\\', '.']) {
        let _ = std::fs::remove_dir_all(projects_dir().join(id));
    }
}

pub fn last() -> Option<String> {
    std::fs::read_to_string(data_dir().join("last.txt")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn set_last(id: &str) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(data_dir().join("last.txt"), id);
}

/// Ein freier Name wie „Projekt 3“.
pub fn next_name(existing: &[StoredProject]) -> String {
    (1..)
        .map(|n| t1("Projekt {0}", n))
        .find(|n| !existing.iter().any(|p| &p.meta.name == n))
        .unwrap()
}

pub fn format_time(time: u64) -> String {
    let age = now().saturating_sub(time);
    match age {
        0..=59 => t("gerade eben").into(),
        60..=3599 => t1("vor {0} min", age / 60),
        3600..=86_399 => t1("vor {0} h", age / 3600),
        86_400..=172_799 => t("gestern").into(),
        _ => t1("vor {0} Tagen", age / 86_400),
    }
}

// ------------------------------------------------------------------ Einstellungen

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub lang: Option<crate::i18n::Lang>,
    /// ohne Installation verwenden
    pub portable: bool,
}

pub fn load_settings() -> Settings {
    let mut s = Settings::default();
    if let Ok(txt) = std::fs::read_to_string(data_dir().join("settings.txt")) {
        for line in txt.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "lang" => s.lang = crate::i18n::Lang::from_code(v),
                    "portable" => s.portable = v.trim() == "1",
                    _ => {}
                }
            }
        }
    }
    s
}

pub fn save_settings(s: &Settings) {
    let _ = std::fs::create_dir_all(data_dir());
    let mut txt = String::new();
    if let Some(l) = s.lang {
        txt += &format!("lang={}\n", l.code());
    }
    txt += &format!("portable={}\n", if s.portable { 1 } else { 0 });
    let _ = std::fs::write(data_dir().join("settings.txt"), txt);
}
