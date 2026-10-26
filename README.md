# Skin Studio – Minecraft-Skin-Editor (.exe)

Ein schlanker, schneller Skin-Editor für Windows mit 3D-Vorschau und **Skin-Mixer** für Partnerlooks.

**Download:** [`SkinStudio.exe`](SkinStudio.exe) herunterladen und doppelklicken. Beim ersten Start führt ein **Installations-Assistent** (Deutsch / English) durch die Installation. Danach findest du Skin Studio über die **Windows-Suche**, im **Startmenü** und unter **Einstellungen → Apps** (dort auch zum Deinstallieren). Wer nicht installieren möchte, wählt „Ohne Installation verwenden (portabel)“.

> Windows zeigt bei unsignierten Programmen eventuell „Der Computer wurde durch Windows geschützt“ an.
> Dann auf **Weitere Informationen → Trotzdem ausführen** klicken.

## Funktionen

### 🌐 Sprachen, Tabs & Updates
- Komplett auf **Deutsch** und **Englisch (US)**, umschaltbar auf der Startseite oder unter Hilfe
- **Tabs** wie in Blockbench: mehrere Projekte gleichzeitig offen, ➕ für ein weiteres, ✖ zum Schließen (Strg+T / Strg+W)
- **Update-Symbol** oben rechts: grau = aktuell, **grün = Update verfügbar**. Ein Klick öffnet ein kleines Fenster mit dem Stand („Keine Updates – du bist auf dem neuesten Stand“ oder „Jetzt installieren“). Skin Studio prüft alle 5 Sekunden im Hintergrund, ob es eine neue Version gibt, und installiert sie auf Knopfdruck, ohne dass du selbst eine neue exe herunterladen musst.

### 🏠 Startseite & Projekte
- Beim Start erscheint eine Startseite. Mit **➕ Neues Projekt** gibst du einen Namen ein und kannst optional gleich deinen Skin (.png) hinzufügen – oder du machst mit dem **letzten Projekt** weiter.
- Statt einer PNG kannst du auch einen **Minecraft-Benutzernamen** eingeben: direkt nach dem Tippen wird der aktuelle Skin des Spielers über die offizielle Mojang-Schnittstelle geladen und angezeigt.
- Darunter stehen als kleine Gesichter (wie bei NameMC) **frühere Skins** dieses Spielers, die Skin Studio schon einmal gesehen hat, und die Skins aus der **Bibliothek deines Minecraft-Launchers** (`%APPDATA%\.minecraft`). Ein Klick wählt den Skin.
- **🌐 Alle früheren Skins auf NameMC** öffnet das NameMC-Profil im Browser. Dort bei jedem Skin einfach auf „Download“ klicken: Skin Studio beobachtet den Download-Ordner, übernimmt neue Skins automatisch und speichert sie dauerhaft bei diesem Spieler (Ziehen ins Fenster geht auch).
- Jedes Projekt hat **Editor und Skin-Mixer** zusammen, oben einfach umschalten. Der Mixer-Stand wird mit dem Projekt gespeichert.
- Projekte werden **im Programm gespeichert** (unter `%APPDATA%\SkinStudio`). Du musst keine Dateien verwalten.
- Beim Schließen fragt das Programm, ob du speichern möchtest. Beim nächsten Start ist das Projekt wieder da.
- Projekte lassen sich umbenennen und löschen. Ein PNG kannst du als neues Projekt importieren – oder im Editor jederzeit über „📂 Importieren“ in ein bestehendes Projekt laden (auch bei leerem Start).

### 🎨 Editor
- **Stift**, **Radierer**, **Füllen** (Eimer), **Pipette**, **Linie**, **Rechteck** (direkt auf dem 3D-Modell oder in der 2D-Textur ziehen, mit Vorschau; bleibt auf der Fläche, auf der du anfängst)
- **Farbe ersetzen**: erst die alte Farbe im Skin anklicken, dann die neue – die alte wird überall ausgetauscht (die betroffenen Pixel werden vorher markiert; Toleranz einstellbar)
- **Rauschpinsel**: malt die gewählte Farbe in zufälligen Helligkeitsstufen, für lebendige Flächen wie Stoff, Haare oder Stein (Stärke, Stufen und Farbton-Streuung einstellbar)
- **Licht/Schatten**: hellt auf (Linksklick) oder dunkelt ab (Rechtsklick), auf Wunsch mit Farbton-Verschiebung wie in der Pixel-Art
- **Spiegeln**: alles, was du malst, erscheint auch auf der anderen Körperseite
- **Rückgängig / Wiederholen** (bis zu 300 Schritte)
- **Die große 3D-Ansicht ist die Hauptansicht**: direkt auf dem Modell malen, mit der rechten Maustaste drehen. Sie läuft auf der Grafikkarte (OpenGL) und ist dadurch flüssig, mit einem gut erkennbaren **Pixelraster** auf dem Modell.
- Nach dem Farbe-Aufnehmen mit der **Pipette** geht es automatisch mit dem **Stift** weiter
- **Körperteile ausblenden wie bei Skindex** (rechte Leiste): 1. und 2. Ebene sowie Kopf, Körper, Arme und Beine per Klick ausblenden, damit man an verdeckte Stellen kommt (Rechtsklick: nur dieses Teil zeigen)
- Wegradierte Stellen bleiben anklickbar und lassen sich wieder bemalen
- Die 2D-Textur ist standardmäßig eingeklappt und lässt sich über den Pfeil rechts ausklappen
- Classic- (4 px) und Slim-Arme (3 px), mit automatischer Umrechnung
- Farbwähler mit Hex-Eingabe, automatischen Schattierungen, Palette und zuletzt benutzten Farben
- Farben anpassen (Farbton, Sättigung, Helligkeit) für den ganzen Skin, eine Ebene oder ein Körperteil
- Links und rechts spiegeln, Ebenen zusammenführen oder kopieren, Graustufen
- Alte 64×32-Skins und HD-Skins werden automatisch umgewandelt
- PNG per Drag & Drop ins Fenster ziehen

### 👫 Skin-Mixer
- **18 Outfit-Vorlagen** (Winterjacke, Anzug, Schlafanzug, Hoodie, Trainingsanzug, Sommer, Lederjacke, Ritter, Arzt, Polizei, Feuerwehr, Koch, Weihnachtsmann, Ninja, Astronaut, Pirat, Latzhose, Skelett) im Stil handgepixelter Skins: eigene Farbpaletten mit Licht und Schatten, Stoffstruktur, Nähte, Taschen und 3D-Teile auf der 2. Ebene. Ein Klick zieht deinem Kopf das Outfit an, die Hautfarbe (samt Schattierung) wird automatisch angepasst. Die Vorschaubilder zeigen schon deinen eigenen Kopf.
- Dein Projekt-Skin ist automatisch als Skin A dabei
- **Bis zu 26 Skins (A–Z)** laden (mehrere Dateien auf einmal oder per Drag & Drop) und für jedes Körperteil und jede Ebene wählen, von welchem Skin es kommt
- **1. und 2. Ebene gleich** (Standard): wählst du z. B. beim Kopf B, bekommt auch die 2. Ebene B
- Der kleine **⬇-Pfeil** übernimmt die Auswahl einer Zeile für alle Körperteile außer dem Kopf
- **Hände** separat wählen: so wie beim Arm, von einem beliebigen Skin oder **in der Hautfarbe des Kopfes** (Handlänge einstellbar)
- Vorlagen: **Kopf A + Outfit B** (Partnerlook), Oberteil tauschen, Hose tauschen, Frisur oder Hut tauschen, Zufallsmix
- **Smarte Hautton-Erkennung** (wahrnehmungsnaher Lab-Farbraum): erkennt die Hautfarbe samt aller Schattierungen am Gesicht und färbt sichtbare Haut des Outfits (Hände, Arme, Beine) passend um, mit allen Schattierungen. Die 2. Ebene (Kleidung) wird dabei nie umgefärbt. Hat ein Skin kein erkennbares Gesicht (z. B. eine Maske), wird die Haut an Händen und Hals gesucht. Mit „Erkannte Haut anzeigen“ siehst du, was umgefärbt wird, und mit „Eigene Hautfarbe“ legst du sie selbst fest.
- Outfit-Farbvarianten (Farbton, Sättigung, Helligkeit oder per 🎲 zufällig)
- **Editor und Mixer sind immer gleich:** Was du im Mixer änderst (z. B. ein Outfit wählen), ist sofort im Editor – ohne Übernehmen-Knopf. Was du im Editor malst, siehst du sofort im Mixer; Skin A ist immer der aktuelle Editor-Stand.
- Oder das Ergebnis direkt exportieren

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

## Update-Server einrichten

Das Update-Symbol liest `version.txt` und lädt `SkinStudio.exe` von
`https://raw.githubusercontent.com/hullihup/.exe/main/` (siehe `UPDATE_BASE` in `src/updater.rs`).
Damit das funktioniert, muss das Repository **öffentlich** sein, und beides muss auf dem Branch **`main`** liegen.
Für ein neues Update: Versionsnummer in `Cargo.toml` erhöhen, neu bauen, `SkinStudio.exe` ersetzen,
dieselbe Nummer in `version.txt` eintragen und auf `main` pushen.

## Selbst bauen

Geschrieben in Rust mit [egui](https://github.com/emilk/egui).

```sh
# unter Windows
cargo build --release

# von Linux aus (MinGW nötig)
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```
