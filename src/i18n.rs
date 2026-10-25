//! Sprachen: Deutsch (Standard der Quelltexte) und Englisch (US).
//!
//! Texte werden mit `t("Deutscher Text")` geschrieben; für Englisch wird die Übersetzung
//! aus [`EN`] geholt. Platzhalter: `{0}`, `{1}` … (mit [`t1`] / [`t2`] befüllen).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }
    pub fn from_code(s: &str) -> Option<Lang> {
        match s.trim() {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English (US)",
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::De }
}

/// Sprache aus den Systemeinstellungen raten (für den allerersten Start).
pub fn system_lang() -> Lang {
    for var in ["LANG", "LC_ALL", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if v.to_lowercase().starts_with("de") {
                return Lang::De;
            }
            if !v.is_empty() {
                return Lang::En;
            }
        }
    }
    #[cfg(windows)]
    {
        // Windows: Benutzersprache aus der Registry
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Control Panel\\International") {
            if let Ok(name) = k.get_value::<String, _>("LocaleName") {
                return if name.to_lowercase().starts_with("de") { Lang::De } else { Lang::En };
            }
        }
    }
    Lang::De
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| EN.iter().copied().collect())
}

/// Übersetzt einen deutschen Text in die aktuelle Sprache.
pub fn t(de: &'static str) -> &'static str {
    if lang() == Lang::De {
        return de;
    }
    table().get(de).copied().unwrap_or(de)
}

/// Übersetzen und `{0}` ersetzen.
pub fn t1(de: &'static str, a: impl std::fmt::Display) -> String {
    t(de).replace("{0}", &a.to_string())
}

/// Übersetzen und `{0}`, `{1}` ersetzen.
pub fn t2(de: &'static str, a: impl std::fmt::Display, b: impl std::fmt::Display) -> String {
    t(de).replace("{0}", &a.to_string()).replace("{1}", &b.to_string())
}

include!("i18n_en.rs");

#[cfg(test)]
mod tests {
    /// Jeder Text, der mit t()/t1()/t2() benutzt wird, muss eine englische Übersetzung haben.
    #[test]
    fn all_texts_translated() {
        let sources = [
            include_str!("main.rs"),
            include_str!("editor.rs"),
            include_str!("mixer.rs"),
            include_str!("skin.rs"),
            include_str!("project.rs"),
            include_str!("installer.rs"),
            include_str!("updater.rs"),
            include_str!("mcskin.rs"),
            include_str!("presets.rs"),
        ];
        let table = super::table();
        let mut missing = Vec::new();
        for src in sources {
            for marker in ["t(\"", "t1(\"", "t2(\""] {
                let mut rest = src;
                while let Some(pos) = rest.find(marker) {
                    // nur echte Aufrufe (nicht z. B. "set(" o. Ä.)
                    let before = rest[..pos].chars().last();
                    rest = &rest[pos + marker.len()..];
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let mut end = 0;
                    let bytes = rest.as_bytes();
                    while end < bytes.len() && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\')) {
                        end += 1;
                    }
                    let key = rest[..end].replace("\\\"", "\"").replace("\\n", "\n");
                    if !table.contains_key(key.as_str()) && !missing.contains(&key) {
                        missing.push(key);
                    }
                }
            }
        }
        assert!(missing.is_empty(), "Fehlende Übersetzungen ({}):\n{}", missing.len(), missing.join("\n"));
    }
}
