//! Konten-Logik: speichert pro Konto die Anmeldedaten der Claude-Desktop-App
//! und von Claude Code (CLI) und tauscht sie beim Wechseln aus.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env_dir("USERPROFILE").or_else(|| env_dir("HOME")).unwrap_or_else(|| PathBuf::from("."))
}

fn roaming() -> PathBuf {
    env_dir("APPDATA").unwrap_or_else(|| home().join("AppData").join("Roaming"))
}

fn local() -> PathBuf {
    env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local"))
}

/// Ordner dieses Programms: %APPDATA%\ClaudeAccountWechsler
pub fn base_dir() -> PathBuf {
    roaming().join("ClaudeAccountWechsler")
}

fn profiles_dir() -> PathBuf {
    base_dir().join("profiles")
}

fn state_file() -> PathBuf {
    base_dir().join("state.json")
}

/// Ordner, in denen die Desktop-App ihre Daten (inkl. Anmeldung) ablegt.
/// (Schlüssel im Profil, echter Pfad)
fn desktop_locations() -> Vec<(String, PathBuf)> {
    let mut v = vec![("desktop_roaming".to_string(), roaming().join("Claude"))];
    // Store-/MSIX-Installation: Daten liegen virtualisiert im Paketordner
    if let Ok(rd) = fs::read_dir(local().join("Packages")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("Claude_") || name.starts_with("AnthropicPBC.Claude") {
                let p = e.path().join("LocalCache").join("Roaming").join("Claude");
                v.push((format!("desktop_msix_{name}"), p));
            }
        }
    }
    v
}

fn cli_credentials() -> PathBuf {
    home().join(".claude").join(".credentials.json")
}

fn cli_config() -> PathBuf {
    home().join(".claude.json")
}

// ---------------------------------------------------------------- Zustand

#[derive(Clone, Debug, Default)]
pub struct State {
    pub active: Option<String>,
    pub profiles: Vec<String>,
}

pub fn load_state() -> State {
    let mut st = State::default();
    if let Ok(txt) = fs::read_to_string(state_file()) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            st.active = v["active"].as_str().map(str::to_string);
            if let Some(arr) = v["profiles"].as_array() {
                st.profiles = arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
            }
        }
    }
    st
}

pub fn save_state(st: &State) -> Result<(), String> {
    fs::create_dir_all(base_dir()).map_err(|e| format!("Ordner anlegen: {e}"))?;
    let v = serde_json::json!({ "active": st.active, "profiles": st.profiles });
    let tmp = state_file().with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap()).map_err(|e| format!("Speichern: {e}"))?;
    fs::rename(&tmp, state_file()).map_err(|e| format!("Speichern: {e}"))
}

/// Ordnername für ein Konto (nur sichere Zeichen).
fn profile_path(name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_.@ ".contains(c) { c } else { '_' })
        .collect();
    profiles_dir().join(safe.trim())
}

pub fn valid_name(st: &State, name: &str, except: Option<&str>) -> Result<String, String> {
    let n = name.trim().to_string();
    if n.is_empty() {
        return Err("Bitte einen Namen eingeben.".into());
    }
    if n.len() > 60 {
        return Err("Name ist zu lang.".into());
    }
    let clash = st.profiles.iter().any(|p| {
        Some(p.as_str()) != except
            && (p.eq_ignore_ascii_case(&n) || profile_path(p) == profile_path(&n))
    });
    if clash {
        return Err("Ein Konto mit diesem Namen gibt es schon.".into());
    }
    Ok(n)
}

/// E-Mail des aktuell in Claude Code angemeldeten Kontos (für Vorschläge).
pub fn current_cli_email() -> Option<String> {
    let txt = fs::read_to_string(cli_config()).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    v["oauthAccount"]["emailAddress"].as_str().map(str::to_string)
}

// ---------------------------------------------------------------- Dateien

fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(p) = to.parent() {
        fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    // Dateien können kurz nach dem Beenden der App noch gesperrt sein → mehrmals versuchen
    let mut last = String::new();
    for _ in 0..20 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    // Anderes Laufwerk o. Ä.: kopieren und dann löschen
    copy_dir(from, to).map_err(|e| format!("{} → {}: {last} / {e}", from.display(), to.display()))?;
    fs::remove_dir_all(from).map_err(|e| format!("{} löschen: {e}", from.display()))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            fs::copy(e.path(), t)?;
        }
    }
    Ok(())
}

/// Sichert die Claude-Code-Anmeldung des aktuellen Kontos in dessen Profil.
fn stash_cli(profile: &Path) -> Result<(), String> {
    fs::create_dir_all(profile).map_err(|e| e.to_string())?;
    let saved_cred = profile.join("cli_credentials.json");
    match fs::read(cli_credentials()) {
        Ok(b) => fs::write(&saved_cred, b).map_err(|e| format!("Anmeldung sichern: {e}"))?,
        Err(_) => {
            let _ = fs::remove_file(&saved_cred);
        }
    }
    let saved_acc = profile.join("cli_oauthAccount.json");
    let acc = fs::read_to_string(cli_config())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("oauthAccount").cloned());
    match acc {
        Some(a) => fs::write(&saved_acc, a.to_string()).map_err(|e| format!("Konto sichern: {e}"))?,
        None => {
            let _ = fs::remove_file(&saved_acc);
        }
    }
    Ok(())
}

/// Setzt die Claude-Code-Anmeldung aus einem Profil ein (oder meldet ab, wenn keine da ist).
fn restore_cli(profile: &Path) -> Result<(), String> {
    match fs::read(profile.join("cli_credentials.json")) {
        Ok(b) => {
            if let Some(p) = cli_credentials().parent() {
                let _ = fs::create_dir_all(p);
            }
            fs::write(cli_credentials(), b).map_err(|e| format!("Anmeldung einsetzen: {e}"))?;
        }
        Err(_) => {
            let _ = fs::remove_file(cli_credentials());
        }
    }
    let acc: Option<Value> = fs::read_to_string(profile.join("cli_oauthAccount.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    if let Ok(txt) = fs::read_to_string(cli_config()) {
        if let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&txt) {
            match acc {
                Some(a) => {
                    map.insert("oauthAccount".into(), a);
                }
                None => {
                    map.remove("oauthAccount");
                }
            }
            let out = serde_json::to_string_pretty(&Value::Object(map)).unwrap();
            fs::write(cli_config(), out).map_err(|e| format!(".claude.json schreiben: {e}"))?;
        }
    } else if let Some(a) = acc {
        let out = serde_json::to_string_pretty(&serde_json::json!({ "oauthAccount": a })).unwrap();
        fs::write(cli_config(), out).map_err(|e| format!(".claude.json schreiben: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- Desktop-App

/// Wie die Desktop-App wieder gestartet werden kann.
#[derive(Clone, Debug)]
pub enum Launch {
    Exe(PathBuf),
    Store(String),
}

#[cfg(windows)]
fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

/// Laufende Prozesse der Desktop-App (PID, Pfad). Claude Code im Terminal
/// heißt ebenfalls claude.exe – das wird hier bewusst ausgelassen.
#[cfg(windows)]
fn desktop_processes() -> Vec<(u32, String)> {
    let out = hidden(std::process::Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-Process -Name claude -ErrorAction SilentlyContinue | ForEach-Object { \"$($_.Id)|$($_.Path)\" }",
    ]))
    .output();
    let Ok(out) = out else { return vec![] };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (pid, path) = l.trim().split_once('|')?;
            let lower = path.to_lowercase();
            let is_cli = lower.contains("\\.local\\bin\\")
                || lower.contains("\\npm\\")
                || lower.contains("node_modules")
                || lower.contains("\\.claude\\")
                || lower.contains("claude-code");
            let is_desktop = lower.contains("anthropicclaude")
                || lower.contains("\\windowsapps\\")
                || lower.contains("\\programs\\claude\\")
                || lower.contains("\\program files\\claude");
            if is_desktop && !is_cli { Some((pid.parse().ok()?, path.to_string())) } else { None }
        })
        .collect()
}

#[cfg(not(windows))]
fn desktop_processes() -> Vec<(u32, String)> {
    vec![]
}

fn store_family() -> Option<String> {
    fs::read_dir(local().join("Packages")).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        (n.starts_with("Claude_") || n.starts_with("AnthropicPBC.Claude")).then_some(n)
    })
}

fn find_launch(running: &[(u32, String)]) -> Option<Launch> {
    if let Some((_, p)) = running.first() {
        if p.to_lowercase().contains("\\windowsapps\\") {
            return store_family().map(Launch::Store);
        }
        return Some(Launch::Exe(PathBuf::from(p)));
    }
    let candidates = [
        local().join("AnthropicClaude").join("claude.exe"),
        local().join("Programs").join("Claude").join("Claude.exe"),
    ];
    if let Some(c) = candidates.into_iter().find(|c| c.exists()) {
        return Some(Launch::Exe(c));
    }
    store_family().map(Launch::Store)
}

/// Beendet die Desktop-App. Gibt zurück, ob sie lief.
fn stop_desktop() -> Result<bool, String> {
    let procs = desktop_processes();
    if procs.is_empty() {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        for (pid, _) in &procs {
            let _ = hidden(std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"])).output();
        }
        for _ in 0..30 {
            if desktop_processes().is_empty() {
                std::thread::sleep(Duration::from_millis(800));
                return Ok(true);
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        return Err("Die Claude-App ließ sich nicht beenden. Bitte schließe sie (auch im Infobereich) und versuche es erneut.".into());
    }
    #[allow(unreachable_code)]
    Ok(true)
}

pub fn start_desktop(launch: &Option<Launch>) -> Result<(), String> {
    let launch = launch.clone().or_else(|| find_launch(&[]));
    match launch {
        Some(Launch::Exe(p)) => std::process::Command::new(&p)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{} starten: {e}", p.display())),
        Some(Launch::Store(fam)) => std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{fam}!Claude"))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Claude starten: {e}")),
        None => Err("Claude-App nicht gefunden – bitte selbst starten.".into()),
    }
}

// ---------------------------------------------------------------- Wechseln

pub struct SwitchResult {
    pub was_running: bool,
    pub launch: Option<Launch>,
}

/// Wechselt vom aktiven Konto zu `target`. `target` darf ein leeres (neues) Konto sein:
/// dann zeigt Claude danach die Anmeldung.
pub fn switch_to(st: &mut State, target: &str) -> Result<SwitchResult, String> {
    let launch = find_launch(&desktop_processes());
    let was_running = stop_desktop()?;

    let locations = desktop_locations();
    let target_dir = profile_path(target);

    // 1. Aktuelles Konto wegräumen
    let mut moved_out: Vec<(PathBuf, PathBuf)> = vec![];
    if let Some(cur) = st.active.clone() {
        let cur_dir = profile_path(&cur);
        stash_cli(&cur_dir)?;
        for (key, live) in &locations {
            if live.exists() {
                let dest = cur_dir.join(key);
                if dest.exists() {
                    let _ = fs::remove_dir_all(&dest);
                }
                if let Err(e) = move_dir(live, &dest) {
                    for (live, dest) in moved_out.iter().rev() {
                        let _ = move_dir(dest, live);
                    }
                    return Err(format!("Konnte Daten nicht sichern: {e}"));
                }
                moved_out.push((live.clone(), dest));
            }
        }
    } else {
        return Err("Kein aktives Konto – bitte zuerst das aktuelle Konto benennen.".into());
    }

    // 2. Zielkonto einsetzen
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    for (key, live) in &locations {
        let src = target_dir.join(key);
        if src.exists() && !live.exists() {
            move_dir(&src, live).map_err(|e| format!("Konnte Konto nicht einsetzen: {e}"))?;
        }
    }
    restore_cli(&target_dir)?;

    st.active = Some(target.to_string());
    if !st.profiles.iter().any(|p| p == target) {
        st.profiles.push(target.to_string());
    }
    save_state(st)?;
    Ok(SwitchResult { was_running, launch })
}

pub fn rename_profile(st: &mut State, old: &str, new: &str) -> Result<(), String> {
    let new = valid_name(st, new, Some(old))?;
    let (a, b) = (profile_path(old), profile_path(&new));
    if a.exists() && a != b {
        fs::rename(&a, &b).map_err(|e| format!("Umbenennen: {e}"))?;
    }
    for p in st.profiles.iter_mut() {
        if p == old {
            *p = new.clone();
        }
    }
    if st.active.as_deref() == Some(old) {
        st.active = Some(new);
    }
    save_state(st)
}

pub fn delete_profile(st: &mut State, name: &str) -> Result<(), String> {
    if st.active.as_deref() == Some(name) {
        return Err("Das aktive Konto kann nicht gelöscht werden.".into());
    }
    let p = profile_path(name);
    if p.exists() {
        fs::remove_dir_all(&p).map_err(|e| format!("Löschen: {e}"))?;
    }
    st.profiles.retain(|x| x != name);
    save_state(st)
}

pub fn add_first(st: &mut State, name: &str) -> Result<(), String> {
    let n = valid_name(st, name, None)?;
    fs::create_dir_all(profile_path(&n)).map_err(|e| e.to_string())?;
    st.profiles.push(n.clone());
    st.active = Some(n);
    save_state(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let t = std::env::temp_dir().join(format!("caw-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        unsafe {
            std::env::set_var("USERPROFILE", &t);
            std::env::set_var("APPDATA", t.join("Roaming"));
            std::env::set_var("LOCALAPPDATA", t.join("Local"));
        }
        fs::create_dir_all(t.join("Roaming/Claude")).unwrap();
        fs::write(t.join("Roaming/Claude/Cookies"), "A-cookie").unwrap();
        fs::create_dir_all(t.join(".claude")).unwrap();
        fs::write(t.join(".claude/.credentials.json"), "A-cred").unwrap();
        fs::write(t.join(".claude.json"), r#"{"x":1,"oauthAccount":{"emailAddress":"a@x"},"z":2}"#).unwrap();

        let mut st = load_state();
        assert_eq!(current_cli_email().as_deref(), Some("a@x"));
        add_first(&mut st, "A").unwrap();
        switch_to(&mut st, "B").unwrap();
        assert!(!t.join("Roaming/Claude").exists());
        assert!(!t.join(".claude/.credentials.json").exists());
        let cfg = fs::read_to_string(t.join(".claude.json")).unwrap();
        assert!(!cfg.contains("oauthAccount") && cfg.contains("\"z\""));
        // B meldet sich an
        fs::create_dir_all(t.join("Roaming/Claude")).unwrap();
        fs::write(t.join("Roaming/Claude/Cookies"), "B-cookie").unwrap();
        fs::write(t.join(".claude/.credentials.json"), "B-cred").unwrap();
        switch_to(&mut st, "A").unwrap();
        assert_eq!(fs::read_to_string(t.join("Roaming/Claude/Cookies")).unwrap(), "A-cookie");
        assert_eq!(fs::read_to_string(t.join(".claude/.credentials.json")).unwrap(), "A-cred");
        assert_eq!(current_cli_email().as_deref(), Some("a@x"));
        switch_to(&mut st, "B").unwrap();
        assert_eq!(fs::read_to_string(t.join("Roaming/Claude/Cookies")).unwrap(), "B-cookie");
        assert_eq!(fs::read_to_string(t.join(".claude/.credentials.json")).unwrap(), "B-cred");
        rename_profile(&mut st, "A", "Privat").unwrap();
        switch_to(&mut st, "Privat").unwrap();
        assert_eq!(fs::read_to_string(t.join("Roaming/Claude/Cookies")).unwrap(), "A-cookie");
        delete_profile(&mut st, "B").unwrap();
        assert_eq!(load_state().profiles, vec!["Privat".to_string()]);
        let _ = fs::remove_dir_all(&t);
    }
}
