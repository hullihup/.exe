# Skin Studio – Minecraft-Skin-Editor (.exe)

Ein schlanker, schneller Skin-Editor für Windows mit 3D-Vorschau und **Skin-Mixer** für Partnerlooks.

**Download:** [`SkinStudio.exe`](SkinStudio.exe) herunterladen und doppelklicken – keine Installation nötig.

> Windows zeigt bei unsignierten Programmen eventuell „Der Computer wurde durch Windows geschützt“ an.
> Dann auf **Weitere Informationen → Trotzdem ausführen** klicken.

## Funktionen

### 🏠 Startseite & Projekte
- Beim Start erscheint eine Startseite. Dort startest du ein **neues Projekt** (Minecraft Skin Editor oder Skin-Mixer) oder machst mit dem **letzten Projekt** weiter.
- Projekte werden **im Programm gespeichert** (unter `%APPDATA%\SkinStudio`). Du musst keine Dateien verwalten.
- Beim Schließen fragt das Programm, ob du speichern möchtest. Beim nächsten Start ist das Projekt wieder da.
- Projekte lassen sich umbenennen und löschen. Ein PNG kannst du als neues Projekt importieren.

### 🎨 Editor
- **Stift**, **Radierer**, **Füllen** (Eimer), **Pipette**, **Linie**, **Rechteck**, **Farbe ersetzen**
- **Rauschpinsel**: malt die gewählte Farbe in zufälligen Helligkeitsstufen, für lebendige Flächen wie Stoff, Haare oder Stein (Stärke, Stufen und Farbton-Streuung einstellbar)
- **Licht/Schatten**: hellt auf (Linksklick) oder dunkelt ab (Rechtsklick), auf Wunsch mit Farbton-Verschiebung wie in der Pixel-Art
- **Spiegeln**: alles, was du malst, erscheint auch auf der anderen Körperseite
- **Rückgängig / Wiederholen** (bis zu 300 Schritte)
- **Die große 3D-Ansicht ist die Hauptansicht**: direkt auf dem Modell malen, mit der rechten Maustaste drehen
- **Körperteile ausblenden wie bei Skindex**: 1. und 2. Ebene sowie Kopf, Körper, Arme und Beine per Klick ausblenden, damit man an verdeckte Stellen kommt (Rechtsklick: nur dieses Teil zeigen)
- Wegradierte Stellen bleiben anklickbar und lassen sich wieder bemalen
- Die 2D-Textur ist standardmäßig eingeklappt und lässt sich über den Pfeil rechts ausklappen
- Classic- (4 px) und Slim-Arme (3 px), mit automatischer Umrechnung
- Farbwähler mit Hex-Eingabe, automatischen Schattierungen, Palette und zuletzt benutzten Farben
- Farben anpassen (Farbton, Sättigung, Helligkeit) für den ganzen Skin, eine Ebene oder ein Körperteil
- Links und rechts spiegeln, Ebenen zusammenführen oder kopieren, Graustufen
- Alte 64×32-Skins und HD-Skins werden automatisch umgewandelt
- PNG per Drag & Drop ins Fenster ziehen

### 👫 Skin-Mixer
- **Bis zu 26 Skins (A–Z)** laden (mehrere Dateien auf einmal oder per Drag & Drop) und für jedes Körperteil und jede Ebene wählen, von welchem Skin es kommt
- **Hände** separat wählen: so wie beim Arm, von einem beliebigen Skin oder **in der Hautfarbe des Kopfes** (Handlänge einstellbar)
- Vorlagen: **Kopf A + Outfit B** (Partnerlook), Oberteil tauschen, Hose tauschen, Frisur oder Hut tauschen, Zufallsmix
- **Smarte Hautton-Erkennung** (wahrnehmungsnaher Lab-Farbraum): erkennt die Hautfarbe samt aller Schattierungen am Gesicht und färbt sichtbare Haut des Outfits (Hände, Arme, Beine) passend um, mit allen Schattierungen. Die 2. Ebene (Kleidung) wird dabei nie umgefärbt. Hat ein Skin kein erkennbares Gesicht (z. B. eine Maske), bleibt er unverändert.
- Outfit-Farbvarianten (Farbton, Sättigung, Helligkeit oder per 🎲 zufällig)
- Ergebnis im Editor weiterbearbeiten oder direkt exportieren

### ⬇ Export
- Skin als 64×64-PNG (direkt in Minecraft verwendbar)
- 3D-Vorschau als Bild (900×1200, transparenter Hintergrund)

## Tastenkürzel

| Taste | Funktion |
|---|---|
| B / E / N / H | Stift / Radierer / Rauschpinsel / Licht & Schatten |
| G oder F / R | Füllen / Farbe ersetzen |
| L / U / I | Linie / Rechteck / Pipette (oder Alt + Klick) |
| M | Spiegeln an/aus |
| X | Haupt- und Zweitfarbe tauschen |
| [ ] oder − + | Pinselgröße |
| Strg+Z / Strg+Y | Rückgängig / Wiederholen |
| Strg+S | Projekt speichern |
| Strg+O / Strg+E | PNG importieren / als PNG exportieren |
| Mausrad, mittlere Maustaste | Zoomen, Verschieben |
| F1 | Hilfe |

## Ressourcenschonend
- Das Fenster wird nur neu gezeichnet, wenn du etwas tust. Im Leerlauf braucht es praktisch keine CPU.
- Die 3D-Vorschau wird nur bei Änderungen neu berechnet.
- Eine einzelne Datei mit etwa 7 MB, ohne Installation und ohne Laufzeitumgebung.

## Selbst bauen

Geschrieben in Rust mit [egui](https://github.com/emilk/egui).

```sh
# unter Windows
cargo build --release

# von Linux aus (MinGW nötig)
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```
