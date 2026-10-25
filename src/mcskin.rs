//! Aktuellen Skin eines Minecraft-Spielers über die offizielle Mojang-Schnittstelle laden.

use std::sync::{Arc, Mutex};

use crate::i18n::t;
use crate::skin::{Model, Skin};
use crate::updater::http_get;

#[derive(Clone)]
pub enum Lookup {
    Loading,
    Found { name: String, skin: Skin, model: Model },
    Failed(String),
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
        Some(skin) => Lookup::Found { name: real_name, skin, model },
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
}
