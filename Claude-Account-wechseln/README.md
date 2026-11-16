# Claude – Account wechseln

Eigenständiges Windows-Programm (hat nichts mit Skin Studio zu tun) zum Wechseln
zwischen mehreren Claude-Konten (Claude-Desktop-App und Claude Code), ohne sich
jedes Mal ab- und wieder anzumelden.

## Herunterladen

**[Claude-Account-wechseln.exe](https://github.com/hullihup/.exe/raw/claude/confident-meitner-waf49j/Claude-Account-wechseln/Claude-Account-wechseln.exe)**: herunterladen und starten, keine Installation nötig.

Zeigt Windows „Der Computer wurde durch Windows geschützt“, dann auf
**Weitere Informationen → Trotzdem ausführen** klicken (die Datei ist nicht signiert).

## Benutzung

1. Beim ersten Start das aktuelle Konto benennen (z. B. „Privat“).
2. **➕ Konto hinzufügen**: Claude startet ohne Anmeldung neu, dort mit dem zweiten Konto anmelden.
3. Danach mit **Wechseln** jederzeit zwischen den Konten umschalten.

Die Konten werden unter `%APPDATA%\ClaudeAccountWechsler` gespeichert.

## Selbst bauen

```
cargo build --release --target x86_64-pc-windows-gnu
```
