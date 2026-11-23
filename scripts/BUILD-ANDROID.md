# Blatt für Android bauen

Debug-APK bauen und auf Emulator oder Gerät installieren.

## Bauen

```bash
/home/kev/blatt-android-build.sh          # nutzt npx @tauri-apps/cli android build --debug --apk
adb install -r crates/blatt-app/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb shell am start -n dev.grace.blatt/.MainActivity
```

Voraussetzungen (stehen im Skript): `JAVA_HOME` auf JDK 17, `ANDROID_HOME`,
`NDK_HOME` auf das NDK, `CARGO_TARGET_DIR=target-android`.

## Wo die Einstellungen liegen

Auf Android gibt es kein Benutzer-Konfigurationsverzeichnis — `dirs::config_dir()`
liefert `None`, und der Rückfall `.` zeigt auf `/`, das schreibgeschützt ist
(EROFS). `blatt-app` setzt den Pfad deshalb im `setup`-Hook, **bevor**
`AppState::new()` die Konfiguration lädt:

```
/data/user/0/dev.grace.blatt/config.json
```

Das ist `app.path().app_config_dir()`, also `Context.getDataDir()` — das
Wurzelverzeichnis der App, **nicht** `files/`. Dateien unter
`files/config.json` oder `files/Blatt/config.json` werden nicht gelesen.

Zum Vorbefüllen (Debug-Build, `run-as` erlaubt):

```bash
adb push config.json /data/local/tmp/config.json
adb shell run-as dev.grace.blatt cp /data/local/tmp/config.json /data/user/0/dev.grace.blatt/config.json
adb shell run-as dev.grace.blatt chmod 600 /data/user/0/dev.grace.blatt/config.json
```

Format:

```json
{
  "server_url": "http://10.0.2.2:8642",
  "token": "…"
}
```

`10.0.2.2` ist vom Emulator aus der Host-Rechner; ein echtes Gerät braucht die
LAN-Adresse des Servers.

## Nachsehen, ob es geklappt hat

Der Start protokolliert Pfad, Server und ob ein Schlüssel geladen wurde
(stderr landet in logcat):

```bash
adb logcat -d | grep '\[blatt\] Konfiguration'
# [blatt] Konfiguration /data/user/0/dev.grace.blatt/config.json: server=http://10.0.2.2:8642 schluessel=gesetzt
```

Beim Speichern aus der Oberfläche muss der Zeitstempel dieser Datei wandern:

```bash
adb shell run-as dev.grace.blatt ls -la --full-time /data/user/0/dev.grace.blatt/config.json
```
