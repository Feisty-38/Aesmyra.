# Aesmyra. für Windows bauen

Aesmyra. ist eine Tauri-2-App. Für Windows entstehen zwei Installationsdateien:

| Datei | Typ | Erzeugt mit |
| --- | --- | --- |
| `Aesmyra._0.1.0_x64_en-US.msi` | MSI (WiX) | WiX Toolset 3.14, von Tauri automatisch geladen |
| `Aesmyra._0.1.0_x64-setup.exe` | NSIS-Setup | NSIS 3.13, von Tauri automatisch geladen |

Die Version stammt aus `crates/aesmyra-app/tauri.conf.json` (`"version": "0.1.0"`), **nicht** aus dem Git-Tag. Vor einem Release dort erhöhen.

## Warum Windows und nicht dieser Linux-Rechner

- **MSI geht nur unter Windows.** Tauri baut MSI mit dem WiX Toolset, das Windows-Binaries sind.
- Die Tauri-CLI bietet unter Linux nur `deb`, `rpm` und `appimage` als Bundle-Ziele an — `msi` und `nsis` sind dort nicht wählbar.
- `crates/aesmyra-app/tauri.conf.json` listet unter `bundle.targets` nur `["deb", "appimage"]`. Unter Windows **muss** deshalb `--bundles "msi,nsis"` mitgegeben werden; ohne diese Option will die CLI die Linux-Ziele bauen und bricht ab.
- NSIS ließe sich theoretisch aus Linux cross-kompilieren (Mingw + LLVM + NSIS). Tauri nennt das ausdrücklich einen letzten Ausweg; hier wird dieser Weg nicht verwendet.
- Das Icon ist vorhanden: `crates/aesmyra-app/icons/icon.ico`. `tauri-build` nimmt es automatisch, auch ohne `.ico`-Eintrag in `bundle.icon` — der Fallback ist genau dieser Pfad.

## Weg 1: GitHub Actions (empfohlen)

Die Workflow-Datei liegt unter `.github/workflows/build.yml`.

- **Auslöser:** manuell (`workflow_dispatch`), Push auf `main`, Push eines Tags `v*`.
- **Runner:** `windows-latest` (x64), Rust stable MSVC, Node.js LTS.
- **Befehl:** `tauri build --bundles "msi,nsis" --ci` im Ordner `crates/aesmyra-app`.
- **Ergebnis:** Artefakt `aesmyra-windows-x64` mit MSI, Setup.exe und `SHA256SUMS.txt`, 30 Tage aufbewahrt.

Download: Repository → **Actions** → den Lauf öffnen → unten bei **Artifacts** das Paket `aesmyra-windows-x64` herunterladen.

Der Workflow signiert nicht. Die Installer laufen daher beim ersten Start durch SmartScreen („Unbekannter Herausgeber“). Siehe Abschnitt „Signieren“.

## Weg 2: echter Windows-Rechner

### Voraussetzungen

1. **Windows 10 ab 1803 oder Windows 11**, x64.
2. **Visual Studio 2022 Build Tools** mit der Workload **„Desktop development with C++“** (MSVC v143, Windows 10/11 SDK). Das ist der Linker und die C/C++-Toolchain, ohne die Rust-MSVC nicht linken kann.
3. **Rust** (stable, MSVC-Target) über [rustup](https://rustup.rs).
4. **WebView2-Runtime** (Evergreen). Auf Windows 11 und Windows 10 ab 1803 vorinstalliert; sonst nachinstallieren.
5. **Node.js LTS** — nur nötig, wenn die Tauri-CLI über npm installiert wird. Mit `cargo install tauri-cli` wird kein Node gebraucht.
6. **VBSCRIPT** als optionales Windows-Feature — nur für MSI nötig. Standardmäßig aktiv; ab Windows 11 24H2 ist VBScript ein Feature on Demand und kann fehlen. Fehlerbild: `failed to run light.exe`. Aktivieren über *Einstellungen → Apps → Optionale Features → Weitere Windows-Features → VBSCRIPT*.
7. **Internet beim ersten Build.** Tauri lädt WiX 3.14 und NSIS 3.13 selbst herunter. WiX und NSIS müssen **nicht** manuell installiert werden.

Installation per PowerShell (winget):

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup --exact
winget install --id Microsoft.EdgeWebView2Runtime --exact
winget install --id OpenJS.NodeJS.LTS --exact
```

Danach die Shell neu starten und prüfen:

```powershell
rustup default stable-msvc
rustc -vV     # host: x86_64-pc-windows-msvc
```

### Bauen mit der npm-CLI

```powershell
npm install --global "@tauri-apps/cli@^2"

cd C:\Pfad\zu\aesmyra\crates\aesmyra-app
tauri build --bundles "msi,nsis"
```

### Bauen ohne Node (nur Rust)

```powershell
cargo install tauri-cli --version "^2.0.0" --locked

cd C:\Pfad\zu\aesmyra\crates\aesmyra-app
cargo tauri build --bundles "msi,nsis"
```

`cargo install` übersetzt die CLI einmalig und braucht einige Minuten. Beide Varianten erzeugen dieselben Installer.

### Ergebnis

`crates/aesmyra-app` ist ein Member des Workspace, deshalb liegt `target/` im **Projektwurzelverzeichnis**, nicht im Crate-Ordner:

```text
aesmyra\target\release\bundle\msi\Aesmyra._0.1.0_x64_en-US.msi
aesmyra\target\release\bundle\nsis\Aesmyra._0.1.0_x64-setup.exe
```

Prüfsummen bilden:

```powershell
Get-FileHash .\target\release\bundle\nsis\Aesmyra._0.1.0_x64-setup.exe -Algorithm SHA256
```

### Wenn MSI Ärger macht

Nur NSIS bauen — das umgeht WiX und VBSCRIPT vollständig:

```powershell
tauri build --bundles nsis
```

WiX 3 wird seit Februar 2025 nicht mehr gepflegt. Scheitert `light.exe` ohne eigene Fehlermeldung („failed to launch“), liegt das häufig am Virenscanner, der genau diese Binärdatei blockiert. Abhilfe: den Ordner `%LOCALAPPDATA%\tauri\WixTools` von der Defender-Prüfung ausnehmen oder beim NSIS-Installer bleiben. Das NSIS-Setup erkennt eine frühere MSI-Installation und ersetzt sie sauber.

## WebView2: online oder offline

Standard ist `downloadBootstrapper`: Der Installer lädt die WebView2-Runtime bei der Installation nach (wenige MB, Internet nötig). Für Rechner ohne Internet lässt sich das umstellen — Datei `crates/aesmyra-app/tauri.windows.conf.json` anlegen:

```json
{
  "bundle": {
    "windows": {
      "webviewInstallMode": { "type": "offlineInstaller" }
    }
  }
}
```

Das macht die Installer ca. 130 MB größer. Weitere Modi: `embedBootstrapper` (Bootstrapper eingebettet, lädt trotzdem nach), `skip` (Runtime muss vorhanden sein), `fixedRuntime` (feste Runtime mitliefern).

Tauri merged `tauri.windows.conf.json` automatisch über die Basis-Konfiguration; die Datei gilt nur unter Windows.

## Signieren (optional)

Ohne Signatur zeigt Windows SmartScreen beim Start eine Warnung. Tauri kann beim Bundling signieren, wenn ein Zertifikat im Windows-Zertifikatsspeicher liegt. In `tauri.conf.json` bzw. `tauri.windows.conf.json`:

```json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "…",
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.digicert.com"
    }
  }
}
```

Für TSP-Zeitstempel (z. B. SSL.com) zusätzlich `"tsp": true`. Alternativ lässt sich über `signCommand` ein eigenes Werkzeug aufrufen.

In GitHub Actions muss das Zertifikat vor dem Build in den Zertifikatsspeicher importiert werden (PFX aus einem Secret, Schritt mit `Import-PfxCertificate`). Das PFX gehört nie ins Repository. Für neue Projekte ist Azure Trusted Signing der übliche Weg.

## Optionaler Feinschliff

Ohne `publisher` steht im MSI als Hersteller der zweite Teil der Kennung, also `grace` (`dev.grace.aesmyra`). Das lässt sich zusammen mit den Installer-Icons in `crates/aesmyra-app/tauri.windows.conf.json` sauber setzen:

```json
{
  "bundle": {
    "publisher": "Grace",
    "windows": {
      "nsis": {
        "installerIcon": "icons/icon.ico",
        "uninstallerIcon": "icons/icon.ico"
      }
    }
  }
}
```

Ohne `installerIcon` verwendet das NSIS-Setup das Standard-Icon von NSIS, nicht das Aesmyra.-Icon. Das MSI nutzt bereits `icons/icon.ico`.

## Bekannte Eigenheiten unter Windows

- **Konfiguration:** `%APPDATA%\Aesmyra.\config.json`.
- **Zugangsschlüssel:** Aesmyra. versucht beim Start zusätzlich, `%HOME%\.hermes\.env` zu lesen. Bei aus dem Explorer gestarteten Programmen ist `HOME` unter Windows normalerweise nicht gesetzt (nur `USERPROFILE`). Den Schlüssel deshalb in den Einstellungen der App eintragen.
- **Installationsart:** Das MSI installiert systemweit (`perMachine`, UAC-Abfrage). Das NSIS-Setup installiert standardmäßig nur für den aktuellen Benutzer, ohne Adminrechte.
- **Erster Build:** Dauert einige Minuten und lädt WiX/NSIS nach. Ein Proxy oder eine Firewall kann das blockieren.
- **Virenscanner:** Unsigned NSIS-Installer werden gelegentlich verzögert geprüft; das ist kein Build-Fehler.

## Stand

Geprüft gegen Tauri 2.12.7 (`tauri-build` 2.7.6, Tauri-CLI 2.12.6). Die Bundle-Namen und -Pfade stammen aus dem Quelltext von `tauri-bundler`; die CLI-Optionen aus `tauri build --help`. Ein tatsächlicher Windows-Build wurde hier nicht ausgeführt — dieser Rechner ist Linux.
