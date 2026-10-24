//! Projekte werden im Programm selbst gespeichert (im App-Datenordner),
//! nicht als einzelne Dateien, um die man sich kümmern muss.

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
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("SkinStudio")
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
    std::fs::create_dir_all(&dir).map_err(|e| format!("Ordner konnte nicht angelegt werden: {e}"))?;
    let model = if meta.model == Model::Slim { "slim" } else { "classic" };
    let info = format!("name={}\nmodel={}\nmodified={}\n", meta.name.replace('\n', " "), model, meta.modified);
    // Erst in temporäre Dateien schreiben, dann umbenennen: so geht nie ein Projekt kaputt.
    let write = |name: &str, data: &[u8]| -> Result<(), String> {
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, data).map_err(|e| format!("Speichern fehlgeschlagen: {e}"))?;
        std::fs::rename(&tmp, dir.join(name)).map_err(|e| format!("Speichern fehlgeschlagen: {e}"))
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
    let mut meta = ProjectMeta { id: id.to_string(), name: "Projekt".into(), model: Model::Classic, modified: 0 };
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
        .map(|n| format!("Projekt {n}"))
        .find(|n| !existing.iter().any(|p| &p.meta.name == n))
        .unwrap()
}

pub fn format_time(t: u64) -> String {
    let age = now().saturating_sub(t);
    match age {
        0..=59 => "gerade eben".into(),
        60..=3599 => format!("vor {} min", age / 60),
        3600..=86_399 => format!("vor {} h", age / 3600),
        86_400..=172_799 => "gestern".into(),
        _ => format!("vor {} Tagen", age / 86_400),
    }
}
