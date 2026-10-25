//! Aktuellen Skin eines Minecraft-Spielers über die offizielle Mojang-Schnittstelle laden.

use std::sync::{Arc, Mutex};

use crate::i18n::t;
use crate::skin::{Model, Skin};
use crate::updater::http_get;

#[derive(Clone)]
pub enum Lookup {
    Loading,
    /// aktueller Skin und alle bisher gesehenen (neueste zuerst, ohne den aktuellen)
    Found { name: String, skin: Skin, model: Model, history: Vec<Known> },
    Failed(String),
}

/// Ein gespeicherter Skin (z. B. aus dem Verlauf oder dem Minecraft-Launcher)
#[derive(Clone)]
pub struct Known {
    pub label: String,
    pub skin: Skin,
    pub model: Model,
}

fn player_dir(name: &str) -> std::path::PathBuf {
    crate::project::data_dir().join("players").join(name.to_ascii_lowercase())
}

/// Verlauf eines Spielers: Skin Studio merkt sich jeden Skin, den es für diesen Namen gefunden hat.
pub fn history(name: &str) -> Vec<Known> {
    let dir = player_dir(name);
    let Ok(list) = std::fs::read_to_string(dir.join("list.txt")) else { return vec![] };
    list.lines()
        .filter_map(|l| {
            let mut it = l.split('|');
            let (file, model, time) = (it.next()?, it.next()?, it.next().and_then(|t| t.parse::<u64>().ok()).unwrap_or(0));
            let skin = Skin::from_png(&std::fs::read(dir.join(file)).ok()?).ok()?;
            let model = if model == "s" { Model::Slim } else { Model::Classic };
            Some(Known { label: crate::i18n::t1("gesehen {0}", crate::project::format_time(time)), skin, model })
        })
        .collect()
}

/// Neuen Skin im Verlauf ablegen (falls noch nicht vorhanden). Gibt den Verlauf ohne den aktuellen Skin zurück.
fn remember(name: &str, skin: &Skin, model: Model) -> Vec<Known> {
    let mut known = history(name);
    let dir = player_dir(name);
    if let Some(pos) = known.iter().position(|k| k.skin == *skin) {
        known.remove(pos);
        return known;
    }
    let _ = std::fs::create_dir_all(&dir);
    let file = format!("skin{}.png", crate::project::now());
    if std::fs::write(dir.join(&file), skin.to_png()).is_ok() {
        let old = std::fs::read_to_string(dir.join("list.txt")).unwrap_or_default();
        let line = format!("{file}|{}|{}\n", if model == Model::Slim { "s" } else { "c" }, crate::project::now());
        let _ = std::fs::write(dir.join("list.txt"), line + &old);
    }
    known
}

/// Skins aus der Bibliothek des offiziellen Minecraft-Launchers auf diesem PC.
pub fn launcher_skins() -> Vec<Known> {
    let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) else { return vec![] };
    launcher_skins_in(&std::path::PathBuf::from(appdata).join(".minecraft"))
}

fn launcher_skins_in(mc: &std::path::Path) -> Vec<Known> {
    let mut out = Vec::new();
    for file in ["launcher_custom_skins.json", "launcher_skins.json"] {
        let Ok(json) = std::fs::read_to_string(mc.join(file)) else { continue };
        // Jeder Eintrag hat "name", "skinImage" (data:image/png;base64,…) und "slim"
        let mut rest = json.as_str();
        while let Some(pos) = rest.find("\"skinImage\"") {
            let before = &rest[..pos];
            let entry_start = before.rfind('{').unwrap_or(0);
            let after = &rest[pos..];
            let entry_end = after.find('}').map(|e| pos + e).unwrap_or(rest.len());
            let entry = &rest[entry_start..entry_end];
            rest = &rest[entry_end.min(rest.len())..];
            let Some(img) = json_str(entry, "skinImage") else { continue };
            let Some(b64) = img.split(',').nth(1) else { continue };
            let Some(skin) = base64_decode(b64).and_then(|b| Skin::from_png(&b).ok()) else { continue };
            if out.iter().any(|k: &Known| k.skin == skin) {
                continue;
            }
            let slim = entry.contains("\"slim\" : true") || entry.contains("\"slim\":true");
            let label = json_str(entry, "name").filter(|n| !n.is_empty()).unwrap_or_else(|| "Skin".into());
            out.push(Known { label, skin, model: if slim { Model::Slim } else { Model::Classic } });
        }
    }
    out
}

/// Wert eines JSON-Felds `"key" : "wert"` (für die einfachen Mojang-Antworten reicht das).
fn json_str(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let rest = &json[json.find(&pat)? + pat.len()..];
    let rest = &rest[rest.find(':')? + 1..];
    let rest = &rest[rest.find('"')? + 1..];
    Some(rest[..rest.find('"')?].to_string())
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        buf = (buf << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn fetch(name: &str) -> Lookup {
    let not_found = || Lookup::Failed(t("Spieler nicht gefunden (oder keine Internetverbindung).").into());
    if !valid_name(name) {
        return Lookup::Failed(t("Benutzernamen haben 3–16 Zeichen: Buchstaben, Zahlen und _").into());
    }
    let Ok(profile) = http_get(&format!("https://api.mojang.com/users/profiles/minecraft/{name}"), None) else {
        return not_found();
    };
    let profile = String::from_utf8_lossy(&profile);
    let (Some(id), Some(real_name)) = (json_str(&profile, "id"), json_str(&profile, "name")) else {
        return not_found();
    };
    let Ok(session) = http_get(&format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"), None) else {
        return not_found();
    };
    let session = String::from_utf8_lossy(&session);
    let textures = json_str(&session, "value").and_then(|v| base64_decode(&v)).map(|b| String::from_utf8_lossy(&b).into_owned());
    let Some(textures) = textures else { return not_found() };
    let Some(url) = json_str(&textures, "url") else {
        return Lookup::Failed(t("Dieser Spieler benutzt den Standard-Skin.").into());
    };
    let url = url.replacen("http://", "https://", 1);
    let model = if textures.contains("\"slim\"") { Model::Slim } else { Model::Classic };
    // Binärdaten über eine Datei laden (PowerShell kann sonst kein PNG zurückgeben)
    let tmp = std::env::temp_dir().join(format!("skinstudio_{}.png", id));
    let res = http_get(&url, Some(&tmp)).and_then(|_| std::fs::read(&tmp).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    match res.ok().and_then(|b| Skin::from_png(&b).ok()) {
        Some(skin) => {
            let history = remember(&real_name, &skin, model);
            Lookup::Found { name: real_name, skin, model, history }
        }
        None => Lookup::Failed(t("Der Skin konnte nicht geladen werden.").into()),
    }
}

/// Startet die Suche im Hintergrund.
pub fn fetch_async(name: String, ctx: &eframe::egui::Context) -> Arc<Mutex<Lookup>> {
    let state = Arc::new(Mutex::new(Lookup::Loading));
    let (s, ctx) = (state.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = fetch(&name);
        if let Ok(mut g) = s.lock() {
            *g = r;
        }
        ctx.request_repaint();
    });
    state
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64_decode("SGFsbG8=").unwrap(), b"Hallo");
        assert_eq!(super::json_str(r#"{ "id" : "abc", "name":"X" }"#, "name").as_deref(), Some("X"));
    }

    #[test]
    fn launcher_file() {
        let dir = std::env::temp_dir().join("skinstudio_launcher_test");
        let mc = dir.join(".minecraft");
        std::fs::create_dir_all(&mc).unwrap();
        let png = crate::skin::starter_skin().to_png();
        let b64 = encode(&png);
        let json = format!(
            r#"{{"customSkins":{{"skin_1":{{"created":"x","id":"skin_1","modelImage":"data:image/png;base64,AAAA","name":"Mein Anzug","skinImage":"data:image/png;base64,{b64}","slim":true}}}},"version":1}}"#
        );
        std::fs::write(mc.join("launcher_custom_skins.json"), json).unwrap();
        let found = super::launcher_skins_in(&mc);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label, "Mein Anzug");
        assert!(found[0].model == crate::skin::Model::Slim);
    }

    fn encode(data: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in data.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                if k <= c.len() {
                    s.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }
}
