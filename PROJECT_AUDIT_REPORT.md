# Projekt-Audit: Aesmyra (aesmyra)

Durchgeführt am 23. November 2026, headless (keine grafischen Fenster).
Projektstand: Arbeitskopie mit nicht festgeschriebenen Änderungen (`git status` nicht leer).

## A. Projektübersicht

| Bereich | Technik |
|---|---|
| Kern | Rust-Workspace, Crate `aesmyra-core` (API-Client, SSE-Parser, Slash-Befehle, Konfiguration, Medien) |
| Oberfläche | Tauri 2 (`aesmyra-app`), Vanilla-JS ohne Bundler (`ui/app.js` 2 005 Zeilen, `ui/slash.js`, `ui/aesmyra.css`) |
| Entwicklungsdienst | `aesmyra-devserver`, eigener HTTP-Server auf `127.0.0.1:8121` (Tokio, ein Task je Verbindung) |
| Ergänzungsdienst | `dev/command_service.py` auf `127.0.0.1:8122` (Python-Standardbibliothek) |
| Gegenstelle | Hermes-Gateway `http://127.0.0.1:8642` (Bearer-Auth, REST + SSE) |
| Tests | 21 Tests (2 App, 19 Kern, davon 7 für den SSE-Parser) |
| Build | Cargo-Workspace; Release-Build vollständig 48,5 s (inkrementell, nach vorherigem Lauf) |

Datenfluss: Oberfläche → `invoke` (Tauri) oder HTTP auf `:8121` → `aesmyra-core::Api` → Gateway
→ SSE-Ereignisse → `UiEvent` → DOM. Slash-Befehle antwortet der Kern selbst über HTTP
(`/api/sessions`, `/v1/toolsets`, `/v1/skills`); nur `/compress` geht an den Python-Dienst.

```
ui/app.js  ──invoke──►  aesmyra-app (Tauri)  ──►  aesmyra-core::Api  ──►  Gateway :8642
    ▲                        │                                              │
    └────── UiEvent ◄────────┴──────────── SSE-Parser ◄──────────────────────┘
```

## B. Gefundene Probleme

### B1 (behoben, hoch): Freigabe-Entscheidung wurde vom Server verworfen

Die App sendete `{"choice", "request_id"}` an `POST /v1/runs/{run_id}/approval`.
Der Gateway prüft im Authority-Pfad die Feldmenge **exakt** (`gateway/platforms/api_server_authority_runs.py`):
`set(body) != {"request_id", "execution_generation", "choice"}` → `RuntimeStoreError("invalid_params")` → HTTP 400.
Das Ereignis `approval.request` enthält `execution_generation` (im Test: `1`), die App warf das Feld weg.

Beleg vorher: `400 {"error":{"message":"invalid_params",...}}`, Befehl wurde nicht ausgeführt.
Beleg nachher: `200 {"ok":true}`, Lauf endete mit `done`, Zielverzeichnis entfernt.

### B2 (behoben, mittel): SSE-Parser kopierte den Puffer je Rahmen

`SseParser::push` schnitt jeden Rahmen einzeln mit `drain(..).collect()` heraus. Bei gebündelten
Rahmen (ein TCP-Stück mit n Rahmen) entstanden n Kopien des jeweils verbleibenden Rests, also
quadratischer Aufwand in der Blockgröße. Jetzt werden alle Rahmen gelesen und der verbrauchte
Teil einmal am Ende entfernt.

### B3 (behoben, mittel): Unbegrenzter Medien-Cache in der Oberfläche

`MEDIA_CACHE` sammelte Data-URLs ohne Obergrenze; jede neue Bilddatei blieb für die gesamte
Sitzung im Speicher. Jetzt FIFO mit 40 Einträgen.

### B4 (behoben, niedrig): Vollständiges Neuzeichnen des Fließtextes bei jedem Takt

`paintLive` setzte `innerHTML` des gesamten Antworttextes pro Animationsbild neu, inklusive
erneutem Markdown-Durchlauf und Mediensuche. Bei langen Antworten wächst der Aufwand quadratisch
mit der Textlänge. Jetzt ab 20 000 Zeichen gedrosselt (60 ms), mit Nachhol-Lauf, damit kein Text
verloren geht.

### B5 (behoben, niedrig): Clippy-Hinweis `manual_div_ceil`

`crates/aesmyra-core/src/media.rs`: `(len + 2) / 3 * 4` → `len.div_ceil(3) * 4`.
Danach `cargo clippy --workspace --all-targets` ohne Warnung.

### B6 (offen, mittel): Devserver liefert bei freigabepflichtigen Läufen keine Ereignisse

Reproduktion: `POST :8121/chat` mit einem Auftrag, der eine Freigabe auslöst (`rm -rf <pfad>`).
Der Server antwortet sofort mit `200` und `Content-Type: text/event-stream`, danach kommen
über 42 s **keine** Ereignisse (auch kein `run_started`). Serverseitig läuft der Lauf jedoch:
`GET /api/sessions/{id}` zeigt `tool_call_count: 1`, also wurde das Werkzeug aufgerufen und
wartet auf die Entscheidung.

Gegenprobe mit einem Auftrag, der ohne Nachfrage durchläuft (`chmod 644 <verzeichnis>`):
12 Ereignisse in 16,3 s (`run_started`, dreimal `tool_started`/`tool_finished`, `delta`,
`completed`). Gegenprobe direkt gegen den Gateway-Stream: `approval.request` kommt nach
wenigen Sekunden an.

Damit ist die Grenze klar: Der Stream bricht genau dann ab, wenn der Lauf in den Zustand
„wartet auf Freigabe" geht. Die Freigabe selbst funktioniert: `POST :8121/approve` mit
`executionGeneration` liefert `200 {"ok":true}` und der Befehl wird ausgeführt.
Betroffen ist nur die Anzeige der Karte beim Start über den Devserver.
Weitere Eingrenzung verlangt Instrumentierung oder einen Neustart des Dienstes mit
Debug-Ausgabe; der Neustart braucht eine Bestätigung.

### B7 (offen, mittel): Lokale Dienste ohne Herkunftsprüfung

`aesmyra-devserver` sendet `Access-Control-Allow-Origin: *`. `GET /state`, `/sessions` und
`/slash` sind damit aus jeder Webseite lesbar (Sitzungstitel, Verläufe). Schreibende Aufrufe
mit `application/json` scheitern zwar am fehlenden Preflight, lesende nicht.
`dev/command_service.py` setzt keinen CORS-Kopf, führt aber Slash-Befehle für jeden lokalen
Aufrufer aus. Beides ist lokal begrenzt, aber unnötig weit.

### B8 (offen, mittel): Wartende Freigabe blockiert ihre Sitzung dauerhaft

Ohne Antwort bleibt der Lauf stehen; neue Läufe derselben Sitzung bleiben `queued`.
`DELETE /api/sessions/{id}` antwortet dann `409`. Ein Sitzungs-Stopp-Endpunkt existiert nicht.
Der Zustand löst sich erst mit dem Freigabe-Timeout (`approvals.timeout`, Vorgabe 300 s) oder
einem Neustart des Gateways. Ursache liegt im Gateway, nicht in Aesmyra — dort ist nichts geändert worden.

### B9 (niedrig): Kein Zeitlimit für Freigaben in der Oberfläche

Die Karte zeigt keinen Ablauf. Bleibt sie unbeantwortet, sieht der Nutzer keinen Hinweis,
dass der Lauf inzwischen verworfen wurde.

## C. Durchgeführte Änderungen

| Datei | Änderung | Grund |
|---|---|---|
| `crates/aesmyra-core/src/api.rs` | `UiEvent::Approval` trägt `execution_generation`; `approve()` sendet es mit; Dispatch liest es aus dem Ereignis | B1 |
| `crates/aesmyra-app/src/lib.rs` | `run_approve` nimmt `execution_generation` an | B1 |
| `crates/aesmyra-devserver/src/main.rs` | `/approve` liest `executionGeneration` aus dem Rumpf | B1 |
| `ui/app.js` | Freigabe-Eintrag speichert die Generation und sendet sie zurück | B1 |
| `crates/aesmyra-core/src/sse.rs` | Puffer wird einmal statt je Rahmen ausgeschnitten | B2 |
| `ui/app.js` | `cacheMedia` mit FIFO-Grenze 40; `paintLive` drosselt ab 20 000 Zeichen | B3, B4 |
| `crates/aesmyra-core/src/media.rs` | `div_ceil` statt `(n+2)/3` | B5 |

Nicht geändert: Gateway (`gateway/**` im Hermes-Verzeichnis unverändert), Konfigurationsdateien
außerhalb des Projekts, öffentliche Schnittstellen des Kerns (nur additive Felder).

## D. Performance-Vergleich

Nur tatsächlich gemessene Werte.

| Metrik | Vorher | Nachher | Veränderung |
|---|---|---|---|
| Freigabe-Rücksendung (HTTP-Status) | 400 `invalid_params` | 200 `{"ok":true}`, Befehl ausgeführt | funktional behoben |
| `cargo build --workspace --release` | nicht separat gemessen | 48,5 s (inkrementell) | — |
| `cargo test --workspace` (21 Tests) | — | 1,0 s Kerntests, gesamt 1,0 s | — |
| `GET :8121/health` (15 Messungen) | — | Median 0,30 ms, p95 0,40 ms | — |
| `GET :8642/api/sessions` (10 Messungen) | — | Median 11,5 ms, max 19,4 ms | — |
| Chat bis `completed` (harmloser Lauf) | — | 1,9 s | — |

Für B2, B3 und B4 liegen **keine** Messwerte vor: Der SSE-Parser hat keinen Lastpfad-Benchmark,
und die Oberflächenänderungen lassen sich ohne sichtbares Fenster nicht messen (siehe E).
Die Änderungen sind begründet (Kopieraufwand bzw. Speicherwachstum), nicht belegt beschleunigt.

## E. Testergebnisse

Erfolgreich:
- `cargo test --workspace`: 21 Tests, 0 Fehler.
- `cargo clippy --workspace --all-targets`: keine Warnung, keine Fehlermeldung.
- `cargo build --workspace` (debug) und `--release`: erfolgreich.
- `node --check ui/app.js` und `ui/slash.js`: Syntax in Ordnung.
- Freigabe über `:8642` direkt: Ereignis empfangen, `200 resolved`, Befehl ausgeführt, Zielpfad entfernt.
- Freigabe über `:8121/approve` (der Weg der App): `200 {"ok":true}`, Befehl ausgeführt.
- Zehn aufeinanderfolgende Sitzungslisten-Abrufe ohne Fehler.

Fehlgeschlagen bzw. nicht ausführbar:
- Anzeige der Freigabekarte über den Devserver-Stream (B6) — reproduzierbar ohne Ereignisse.
- Oberflächenmessungen (Bildrate, Speicher) — Headless-Vorgabe erlaubt kein sichtbares Fenster;
  ein Headless-Browser wurde nicht eingesetzt, weil die Prüfung ohne ihn entscheidbar war.
- Lasttest gegen das Gateway — bewusst unterlassen (fremder, produktiv genutzter Dienst).

Nicht geprüft:
- Tauri-Paketbau (`.deb`/AppImage) und Android-APK.
- Verhalten auf Mobilgeräten.
- Gateway-interne Zeitsteuerung der Freigabe.

## F. Noch offene Probleme

1. **B6** — Devserver-Stream ohne Ereignisse bei freigabepflichtigen Läufen. Verlangt Instrumentierung
   oder Dienstneustart mit Debug-Ausgabe; Neustart braucht Freigabe des Nutzers.
2. **B7** — `Access-Control-Allow-Origin: *` auf dem Devserver; Herkunftsprüfung fehlt.
3. **B8** — wartende Freigabe blockiert die Sitzung; Ursache im Gateway, nicht im Projekt.
4. **B9** — kein sichtbarer Ablaufhinweis auf der Freigabekarte.
5. Laufender Prozess `aesmyra-devapi` nutzt das Debug-Binary; nach dem Neubau ist ein Neustart
   nötig, damit B2 wirksam wird. Neustart wurde nicht ohne Freigabe ausgeführt.

## G. Gesamtbewertung

Bestätigt: Der Kern ist sauber gebaut — klare Modulgrenzen, durchgängige Fehlerweitergabe
(`ApiError`, `SseError`), Zeitlimits an allen Netzwerkpfaden, Rechte 0600 für Zugangsdaten,
atomares Schreiben der Konfiguration, Tests für Randfälle des Parsers und der Schlüsselanzeige.
Clippy ohne Beanstandung.

Behoben: ein blockierender Funktionsfehler (Freigabe), ein quadratischer Kopierpfad, ein
unbegrenzter Speicher und ein unnötiges Vollneuzeichnen.

Plausibel, nicht belegt: Die drei Optimierungen wirken, sind aber nicht gemessen.

Ungeprüfte Annahme: dass die Freigabekarte im ausgelieferten Tauri-Fenster erscheint — der
Ereignisstrom des Devservers zeigt sie in meinen Tests nicht, der Rücksende-Weg funktioniert.

Gesamteindruck: von „funktioniert, aber eine Kernfunktion ist blockiert" zu
„Kernfunktionen arbeiten, zwei Randprobleme offen".

## Nachtrag 28. November 2026

Nachgeprüft und geändert, jeweils mit Messung statt Vermutung.

- **B6 — Ereignisstrom bleibt still:** Nachgestellt mit `POST /chat?sessionId=…`.
  Der Strom liefert `run_started`, `delta`, `usage`, `completed`, `stream_ended`,
  `finished` — aber nur auf Sitzungen mit Herkunft `api_server`. Der Gateway
  weist Läufe auf Sitzungen anderer Oberflächen ab (`admission`, Abweisung
  `permission_denied`), und zwar bevor irgendein Ereignis entsteht. Der stille
  Fall im Audit war also eine Sitzung aus dem Terminal, kein Fehler des Servers.
  Zwei Änderungen daraus: der Strom schickt nun alle zehn Sekunden eine
  Kommentarzeile (`: ping`), damit ein wartender Lauf nicht wie ein toter
  aussieht; und die Oberfläche sperrt solche Sitzungen sichtbar als „nur lesbar".
- **B7 — offener CORS-Kopf:** `Access-Control-Allow-Origin: *` ist ersetzt durch
  eine feste Liste (Vorschau auf 8120, beide Dienstports, die drei
  Tauri-Herkünfte). Gemessen: ohne Herkunft und mit fremder Herkunft kein Kopf,
  mit `http://127.0.0.1:8120` und `tauri://localhost` wird sie zurückgegeben;
  Vorabanfrage (OPTIONS) beantwortet mit 204 für erlaubte, mit 403 für fremde
  Herkünfte. `dev/command_service.py` prüft dasselbe und lehnt fremde Herkünfte
  mit 403 ab (gemessen: `evil.example` → 403, ohne Herkunft → 200).
- **B9 — kein Ablaufhinweis:** Die Freigabekarte sagt jetzt, dass eine
  unbeantwortete Freigabe vom Server verworfen wird und der Lauf endet; in der
  GPUI-Fassung zusätzlich mit laufender Sekundenzahl. Zieht der Server eine
  offene Anfrage zurück, erscheint statt des wortlosen Verschwindens der Hinweis
  „Freigabe zurückgezogen — der Lauf endet ohne Antwort."
- **B8 — wartende Freigabe blockiert die Sitzung:** unverändert und außerhalb
  dieses Projekts; die Sitzungssperre sitzt im Gateway, nicht im Client.
- **Klartext statt Fehlercode:** `permission_denied` und verwandte Codes werden
  im Ereignisstrom in Sätze übersetzt, damit die Oberfläche nicht mit
  `permission_denied` dasteht.

Weiterhin ungeprüft: das Aussehen der Karten im laufenden Fenster — dafür
braucht es eine Aufnahme, und die steht noch aus.
