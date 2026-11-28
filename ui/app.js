'use strict';

/* ═══════════════════════════════════════════════════════════════════
   Aesmyra. — Client für Hermes (Rust-Kern, Tauri-Hülle)

   Drei Flächen: Übersicht, Gespräch, Freigaben. Der Kern liefert
   Sitzungen, Verlauf und Ereignisse fertig; hier wird nur gezeichnet.
   ═══════════════════════════════════════════════════════════════════ */

const CORE = (window.__TAURI__ && window.__TAURI__.core) || null;
const invoke = (cmd, args) => (CORE ? CORE.invoke(cmd, args) : Promise.reject('Tauri-Hülle fehlt'));
const Channel = CORE ? CORE.Channel : null;

/* Fehlerspur: nur wenn ein Protokoll gewünscht ist (`aesmyra.trace` im
   Speicher), sonst still. Kein Einfluss auf die Anzeige. */
/* Fehlerspur: vorübergehend immer an, damit ein Fehler in der echten Hülle
   ohne Vorbereitung belegt werden kann. Später wieder auf den Speicher-
   schalter `aesmyra.trace` zurückstellen. */
let TRACE = true;
try { if (localStorage.getItem('aesmyra.trace') === '0') TRACE = false; } catch { /* egal */ }
function trace(msg) {
  if (!TRACE) return;
  try { Promise.resolve(invoke('trace', { line: String(msg) })).catch(() => {}); } catch { /* egal */ }
}

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

/* ── Zustand ──────────────────────────────────────────────────────── */

/* Sitzungen dieser Herkunft nimmt der Server zum Schreiben an. */
const WRITABLE = ['api_server', 'bot_room'];

const S = {
  sessions: [],
  cur: null,
  turns: [],
  streaming: false,
  runId: null,
  /* Sitzung einer anderen Oberfläche: nur lesen. */
  readOnly: false,
  /* Zuletzt abgeschickter Text — bei einem Fehlschlag zurück ins Feld. */
  sent: '',
  failedText: '',
  /* Lauf-Generation: jedes neue Ereignis-Abo zählt hoch, alte werden verworfen. */
  gen: 0,
  /* Ein neues Gespräch ist erst beim ersten Senden eine Sitzung auf dem Server. */
  draft: false,
  navActive: 'index',
  setupBack: null,
  live: null,
  seq: 0,
  hits: [],
  hitIdx: -1,
  loading: false,
  lastId: null,
  sheetId: null,
  approvals: [],
  approvalWatch: 0,
  /* Läufe je Gespräch: Kennung → { sid, gen, runId, stream, live, watch, … }.
     Ein Lauf hängt an seiner Sitzung, nicht an der Ansicht; deshalb darf in
     einem zweiten Gespräch gesendet werden, während das erste antwortet. */
  runs: new Map(),
  /* Zuletzt gezogener Verlauf, damit nicht doppelt geladen wird. */
  lastPull: { runId: '', at: 0 },
  suppress: 0,
  err: '',
  restores: readStore('aesmyra.scroll', {}),
};

/* Zwischenspeicher: zwölf Gespräche reichen für flüssiges Wischen. */
const CACHE = new Map();
const CACHE_MAX = 12;

const LABELS = { once: 'einmal', session: 'diese Sitzung', always: 'immer', deny: 'ablehnen' };

const PLAT = { linux: 'Linux', windows: 'Windows', android: 'Android', macos: 'macOS', ios: 'iOS' };

/* ── Kleine Helfer ────────────────────────────────────────────────── */

function readStore(key, fallback) {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const val = JSON.parse(raw);
    return val && typeof val === 'object' ? val : fallback;
  } catch {
    return fallback;
  }
}

function writeStore(key, value) {
  try {
    localStorage.setItem(key, typeof value === 'string' ? value : JSON.stringify(value));
  } catch {
    /* ohne Speicher geht es auch weiter */
  }
}

const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const trim = (s, n) => { const t = String(s ?? ''); return t.length > n ? t.slice(0, n) + ' …' : t; };
const errText = (e) => (typeof e === 'string' ? e : (e && e.message) || 'Unbekannter Fehler');
const pad3 = (n) => String(n).padStart(3, '0');

const fmtClock = (ts) => (ts ? new Date(ts * 1000).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' }) : '');
const money = (v) => (typeof v === 'number' && v > 0.004 ? v.toFixed(2).replace('.', ',') + ' $' : '');

/* ── Modell und Kontextgröße ──────────────────────────────────────── */

/* Der Server nennt am Ende jedes Laufs die Prompt-Token. Die werden je
   Gespräch gemerkt und überdauern einen Neustart. Fehlt eine Messung, wird
   aus dem Verlauf geschätzt (vier Zeichen je Token) — daher das „ca.“.
   Schlüssel mit 2: unter `aesmyra.ctx` lagen die Summen ganzer Werkzeugläufe,
   die die Fußzeile um ein Vielfaches übertrieben haben. */
const CTX = readStore('aesmyra.ctx2', {});

function shortModel(name) {
  const s = String(name || '').trim();
  if (!s) return '';
  return s.split('/').pop().replace(/-\d{3,4}$/, '');
}

/* Grundlast jeder Anfrage: Systemanweisung, Werkzeuge, Gedächtnis. Sie steht
   im Verlauf nicht, macht aber den größten Teil des Kontexts aus. */
const PROMPT_BASE = 18000;

function estimateTokens(turns) {
  if (!turns || !turns.length) return 0;
  let chars = 0;
  for (const t of turns) {
    if (t.user) chars += String(t.user).length;
    for (const p of t.parts || []) {
      chars += (p.text || '').length + (p.reasoning || '').length;
      for (const tool of p.tools || []) chars += (tool.args || '').length + (tool.out || '').length;
    }
  }
  return PROMPT_BASE + Math.round(chars / 4);
}

function fmtTokens(n) {
  if (!n) return '';
  if (n < 1000) return String(n);
  const k = n / 1000;
  if (k < 100) return k.toFixed(1).replace('.', ',') + 'k';
  return String(Math.round(k)) + 'k';
}

function rememberTokens(id, tokens, calls) {
  if (!id || !tokens) return;
  // Mehr als eine Anfrage im Lauf: der Wert ist ein Mittel, kein Einzelmaß.
  CTX[id] = calls > 1 ? { t: tokens, n: calls } : tokens;
  const keys = Object.keys(CTX);
  if (keys.length > 200) delete CTX[keys[0]];
  writeStore('aesmyra.ctx', CTX);
}

function renderGauge() {
  const host = $('#gauge');
  if (!host) return;
  const full = (S.cur && S.cur.model) || '';
  const model = shortModel(full);
  const id = S.cur && S.cur.id;
  const raw = id ? CTX[id] : 0;
  const exact = raw && typeof raw === 'object' ? Number(raw.t) || 0 : Number(raw) || 0;
  const calls = raw && typeof raw === 'object' ? Number(raw.n) || 1 : 1;
  const tokens = exact || estimateTokens(S.turns);

  if (!model && !tokens) { host.innerHTML = ''; return; }

  const parts = [];
  if (model) {
    parts.push(`<span class="g-model" title="${esc(full)}">${esc(model)}</span>`);
    if (tokens) parts.push('<span class="g-sep">·</span>');
  }
  if (tokens) {
    /* Der Server meldet am Laufende die Summe aller Anfragen des Laufs. Bei einem
       Werkzeuglauf wächst der Prompt mit jeder Runde und wird dadurch mehrfach
       gezählt — gemessen ist nur das Mittel, das der Kern daraus bildet. */
    const hint = exact
      ? (calls > 1
        ? `Kontext: etwa ${tokens.toLocaleString('de-DE')} Token, Mittel aus ${calls} Anfragen des Laufs (der Server nennt nur deren Summe)`
        : `Kontext: ${tokens.toLocaleString('de-DE')} Token, vom Server gemessen`)
      : `Kontext: etwa ${tokens.toLocaleString('de-DE')} Token, aus Verlauf und Grundlast geschätzt`;
    parts.push(`<span class="g-ctx" title="${esc(hint)}">${exact && calls === 1 ? '' : 'ca. '}${fmtTokens(tokens)}</span>`);
  }
  host.innerHTML = parts.join('');
}

/* Nummern sind arabisch: 1, 2, 3 … — keine römischen Ziffern mehr. */
/* Relativzeit wie im Entwurf: Minuten, Stunden, dann Kalendertage. */
function relTime(ts) {
  if (!ts) return '';
  const ms = ts > 1e12 ? ts : ts * 1000;
  const min = Math.floor(Math.max(0, Date.now() - ms) / 60000);
  if (min < 1) return 'gerade eben';
  if (min < 60) return `vor ${min} Min.`;
  const hours = Math.floor(min / 60);
  if (hours < 24) return `vor ${hours} Std.`;

  const then = new Date(ms);
  const today = new Date();
  const midnight = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const thenMidnight = new Date(then.getFullYear(), then.getMonth(), then.getDate()).getTime();
  const days = Math.max(1, Math.round((midnight - thenMidnight) / 86400000));
  if (days === 1) return 'gestern';
  if (days < 7) return `vor ${days} Tagen`;
  if (days < 14) return 'vor 1 Woche';
  if (days < 30) return `vor ${Math.floor(days / 7)} Wochen`;

  const months = Math.max(1, Math.round(days / 30.44));
  return months === 1 ? 'vor 1 Monat' : `vor ${months} Monaten`;
}

/* ── Darstellung ──────────────────────────────────────────────────── */

/* Thema und Akzent liegen im Geraet, nicht auf dem Server: sie gehoeren
   zur Oberflaeche und sollen ohne Verbindung sofort stimmen. */
const LOOK = {
  theme: localStorage.getItem('aesmyra.theme') || 'dark',
  accent: localStorage.getItem('aesmyra.accent') || 'violett',
};

function applyLook() {
  document.documentElement.dataset.theme = LOOK.theme;
  document.documentElement.dataset.accent = LOOK.accent;
  $$('#setTheme button').forEach((b) => b.classList.toggle('on', b.dataset.themeChoice === LOOK.theme));
  $$('#setAccent button').forEach((b) => b.classList.toggle('on', b.dataset.accentChoice === LOOK.accent));
}

function setLook(art, wert) {
  LOOK[art] = wert;
  try { localStorage.setItem(art === 'theme' ? 'aesmyra.theme' : 'aesmyra.accent', wert); } catch { /* egal */ }
  applyLook();
}

/* ── Anzeige ──────────────────────────────────────────────────────── */

function setStatus(text, cls = '') {
  const el = $('#status');
  if (!el) return;
  el.textContent = text;
  el.className = 'status ' + cls;
}

function toast(text, isErr = false) {
  const el = $('#toast');
  if (!el) return;
  el.textContent = text;
  el.className = 'toast show' + (isErr ? ' err' : '');
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.remove('show'), 3600);
}

function setBusy(v) {
  document.body.classList.toggle('busy', !!v);
}

/* Der Rand lebt, solange wirklich etwas läuft — Ereignisstrom **oder** Wächter.
   Vorher schaltete `finishTurn` die Leuchte aus, obwohl der Server noch
   arbeitete; nach einem Sitzungswechsel kam sie dann nie wieder. */
function runActive(r) {
  return !!(r && (r.stream || (r.watch && !r.watch.done)));
}

/* Ein beendeter Lauf bleibt nicht in der Liste: sonst zählt er weiter als
   „läuft“, und der Rand leuchtet nach der Antwort endlos weiter. */
function pruneRun(run) {
  if (runActive(run)) return;
  if (S.runs.get(run.sid) === run) S.runs.delete(run.sid);
}

function syncBusy() {
  const active = busyHere();
  document.body.classList.toggle('busy', active);
  if (document.body.dataset.view === 'index') markRunningRows();
  return active;
}

/* In der Übersicht zeigt ein kleiner Punkt, in welchem Gespräch im
   Hintergrund gearbeitet wird — der Rand bleibt der offenen Ansicht
   vorbehalten. */
function markRunningRows() {
  const host = $('#idxList');
  if (!host) return;
  for (const row of host.querySelectorAll('.ch')) {
    row.classList.toggle('running', runActive(runOf(row.dataset.id)));
  }
}

/* Der Rand und „arbeitet …“ gehören zur **offenen** Ansicht: sie leben nur,
   wenn dieses Gespräch arbeitet. Läuft anderswo etwas, bleibt der Rand ruhig
   und die Übersicht markiert die arbeitende Zeile. */
function busyHere() {
  if (document.body.dataset.view !== 'reading') return false;
  return !!(S.cur && runActive(runOf(S.cur.id)));
}

/* WebKit stellt die Umlauf-Animationen des Randes ein, wenn die Ansicht
   zwischendurch nicht gezeichnet wurde (Tab im Hintergrund, Android im
   Ruhezustand). Zurück im Gespräch werden sie hier neu gestartet. */
function rearmGlow() {
  for (const el of document.querySelectorAll('.edge, .bloom')) {
    el.style.animation = 'none';
    void el.offsetWidth;
    el.style.animation = '';
    for (const s of el.querySelectorAll('.sweep')) {
      s.style.animation = 'none';
      void s.offsetWidth;
      s.style.animation = '';
    }
  }
}

/* ── Markdown, knapp und ehrlich ──────────────────────────────────── */

const IMG_EXT = /\.(png|jpe?g|gif|webp|bmp)$/i;

/* Bilder: entweder als Markdown-Bild mit Daten- oder Webadresse, oder als
   MEDIA:-Kennung des Kerns. Letztere wird erst nach dem Einsetzen geholt —
   der Pfad liegt im Dateisystem, nicht im Netz. */
function inline(t) {
  return t
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/!\[([^\]]*)\]\((data:image\/[^()\s]+|https?:\/\/[^\s)]+)\)/g,
      (_m, alt, src) => `<img class="pic" alt="${alt}" src="${src}" loading="lazy">`)
    .replace(/(?:^|\s)MEDIA:([^\s<>"'()]+)/g,
      (m, p) => (IMG_EXT.test(p) ? `${m.startsWith(' ') ? ' ' : ''}<img class="pic lazy" alt="" data-media="${p}">` : m))
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[\s(])\*([^*\n]+)\*/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" data-href="$2">$1</a>');
}

/* Nach dem Einsetzen: Pfadbilder vom Kern als Datenadresse holen.
   Ein Zwischenspeicher hält den laufenden Text davon ab, dieselbe Datei
   bei jedem Takt neu zu lesen. */
/* Bilder als Data-URL: jede Datei nur einmal holen. Der Vorrat ist begrenzt,
   weil Data-URLs großer Bilder viel Speicher halten. */
const MEDIA_CACHE = new Map();
const MEDIA_CACHE_MAX = 40;

function cacheMedia(path, url) {
  MEDIA_CACHE.delete(path);
  MEDIA_CACHE.set(path, url);
  while (MEDIA_CACHE.size > MEDIA_CACHE_MAX) {
    MEDIA_CACHE.delete(MEDIA_CACHE.keys().next().value);
  }
}

function hydrateMedia(root) {
  if (!root) return;
  for (const img of root.querySelectorAll('img[data-media]')) {
    const path = img.dataset.media;
    img.removeAttribute('data-media');
    if (MEDIA_CACHE.has(path)) {
      img.src = MEDIA_CACHE.get(path);
      img.classList.remove('lazy');
      continue;
    }
    invoke('media_data_url', { path })
      .then((url) => {
        if (!url) throw 'leer';
        cacheMedia(path, url);
        img.src = url;
        img.classList.remove('lazy');
      })
      .catch(() => {
        img.classList.remove('lazy');
        img.replaceWith(Object.assign(document.createElement('span'), {
          className: 'picfail',
          textContent: 'Bild nicht lesbar · ' + path,
        }));
      });
  }
}

function fmtUser(text) {
  return esc(text || '')
    .split('\n')
    .map((line) => (/^(\[User attached|@image:)/.test(line.trim()) ? `<span class="att">${line}</span>` : line))
    .join('\n');
}

function md(src, hart) {
  if (!src) return '';
  const blocks = [];
  let s = String(src).replace(/```(\w*)\n?([\s\S]*?)```/g, (_m, _lang, code) => {
    blocks.push(`<pre><code>${esc(code.replace(/\n+$/, ''))}</code></pre>`);
    return `\u0000${blocks.length - 1}\u0000`;
  });
  s = esc(s);

  const out = [];
  let buf = [];
  let list = null;

  const flushP = () => {
    if (buf.length) {
      const t = inline(buf.join(hart ? '\n' : ' '));
      out.push(`<p>${hart ? t.replace(/\n/g, '<br>') : t}</p>`);
      buf = [];
    }
  };
  const flushL = () => {
    if (list) {
      out.push(`<${list.t}>${list.items.map((i) => `<li>${inline(i)}</li>`).join('')}</${list.t}>`);
      list = null;
    }
  };

  for (const raw of s.split('\n')) {
    const line = raw.replace(/\s+$/, '');
    if (!line.trim()) { flushP(); flushL(); continue; }
    if (/^\u0000\d+\u0000$/.test(line.trim())) { flushP(); flushL(); out.push(line.trim()); continue; }

    let m;
    if ((m = line.match(/^(#{1,3})\s+(.*)$/))) {
      flushP(); flushL();
      const h = Math.min(m[1].length + 1, 4);
      out.push(`<h${h}>${inline(m[2])}</h${h}>`);
      continue;
    }
    if (/^(---|\*\*\*)$/.test(line.trim())) { flushP(); flushL(); out.push('<hr>'); continue; }
    if ((m = line.match(/^\s*[-*+]\s+(.*)$/))) {
      flushP();
      if (!list || list.t !== 'ul') { flushL(); list = { t: 'ul', items: [] }; }
      list.items.push(m[1]);
      continue;
    }
    if ((m = line.match(/^\s*\d+\.\s+(.*)$/))) {
      flushP();
      if (!list || list.t !== 'ol') { flushL(); list = { t: 'ol', items: [] }; }
      list.items.push(m[1]);
      continue;
    }
    if (list) { list.items[list.items.length - 1] += ' ' + line.trim(); continue; }
    buf.push(line.trim());
  }
  flushP(); flushL();

  return out.join('\n').replace(/\u0000(\d+)\u0000/g, (_m, i) => blocks[+i] || '');
}

/* ── Werkzeuge als Notizen ────────────────────────────────────── */

function parseJson(s) {
  try { return JSON.parse(s); } catch { return null; }
}

function prettyArgs(args) {
  const o = parseJson(args);
  return o ? JSON.stringify(o, null, 1) : String(args || '');
}

function summarizeArgs(args) {
  const o = parseJson(args);
  if (!o || typeof o !== 'object') return trim(String(args || '').replace(/\s+/g, ' '), 42);
  for (const k of ['command', 'path', 'query', 'url', 'file_path', 'pattern', 'name', 'prompt']) {
    if (typeof o[k] === 'string') return trim(o[k].replace(/\s+/g, ' '), 42);
  }
  const first = Object.values(o).find((v) => typeof v === 'string');
  return trim(String(first || '').replace(/\s+/g, ' '), 42);
}

function toolNote(t, n) {
  const preview = summarizeArgs(t.args);
  const out = t.out ? trim(String(t.out).replace(/\s+/g, ' '), 160) : '';
  return `<details class="note tool" id="n${n}" data-num="${n}">
    <summary><span class="num">${n}</span><b>${esc(t.name || 'werkzeug')}</b>${
      preview ? `<span class="prev">· ${esc(preview)}</span>` : ''
    }<span class="tstate" data-state></span></summary>
    <div class="nbody">${
      t.args && t.args !== '{}' ? `<pre>${esc(prettyArgs(t.args))}</pre>` : ''
    }${out ? `<pre>${esc(out)}</pre>` : ''}</div>
  </details>`;
}

function reasonNote(text, n) {
  return `<details class="note reason"><summary><span class="num">${n}</span><b>Denken</b></summary><div class="nbody"><p>${esc(trim(text, 4000))}</p></div></details>`;
}

/* ── Abschnitte zeichnen ──────────────────────────────────────────── */

/* Markierungszeichen hinter den letzten Satz setzen; ohne Absatz anhängen. */
function withMarkers(html, extra) {
  if (!extra) return html;
  if (/<\/p>\s*$/.test(html)) return html.replace(/<\/p>\s*$/, ` ${extra}</p>`);
  return html + `<p class="fnrow">${extra}</p>`;
}

function renderTurn(turn) {
  const parts = Array.isArray(turn.parts) ? turn.parts : [];
  const totalTools = parts.reduce((a, p) => a + ((p.tools && p.tools.length) || 0), 0);
  const groupMode = totalTools > 10;
  const notes = [];
  let prose = '';
  let group = null;
  let pending = '';

  const thinkingText = parts.map((p) => p.reasoning).filter(Boolean).join('\n\n');
  if (thinkingText) notes.push(reasonNote(thinkingText, ++S.seq));
  for (const s of (turn.steer || [])) {
    notes.push(`<div class="note steer"><span class="num">↳</span><b>nachgeschoben</b>· <span class="prev">${esc(s)}</span></div>`);
  }

  for (const p of parts) {
    const tools = Array.isArray(p.tools) ? p.tools : [];
    let markers = '';

    if (tools.length) {
      const nums = [];
      const items = [];
      for (const t of tools) {
        const n = ++S.seq;
        nums.push(n);
        items.push(toolNote(t, n));
      }
      markers = groupMode ? '' : nums.map((n) => `<a class="fn" data-target="n${n}">${n}</a>`).join('');

      if (groupMode) {
        if (!group) group = { html: [], nums: [] };
        group.html.push(...items);
        group.nums.push(nums[0], nums[nums.length - 1]);
      } else {
        notes.push(...items);
      }
    }

    if (p.text) {
      prose += withMarkers(md(p.text), pending + markers);
      pending = '';
    } else if (markers) {
      pending += markers;
    }
  }

  if (pending) prose += `<p class="fnrow">${pending}</p>`;

  if (group) {
    const a = group.nums[0];
    const b = group.nums[group.nums.length - 1];
    notes.push(`<details class="note group"><summary><span class="num">${a}–${b}</span><b>${totalTools} Werkzeuge</b></summary>${group.html.join('')}</details>`);
  }

  const req = turn.user !== null && turn.user !== undefined
    ? `<div class="req${turn.steered ? ' steered' : ''}">${
        turn.steered ? '<span class="tag">nachgeschoben</span>' : ''
      }${fmtUser(turn.user)}</div>`
    : '';

  return `<section class="sec">
    ${req}<aside class="notes">${notes.join('')}</aside><div class="prose">${prose}</div>
  </section>`;
}

function addCopy(pre) {
  if (pre.querySelector('.copy')) return;
  const b = document.createElement('button');
  b.type = 'button';
  b.className = 'copy';
  b.textContent = 'kopieren';
  b.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    const code = pre.querySelector('code');
    navigator.clipboard?.writeText((code ? code.textContent : pre.textContent) || '');
    b.textContent = 'kopiert';
    setTimeout(() => { b.textContent = 'kopieren'; }, 1400);
  });
  pre.appendChild(b);
}

function renderChapter(animate = false) {
  const page = $('#page');
  if (!page) return;
  trace('render cur=' + (S.cur && S.cur.id) + ' runs=' + S.runs.size + ' turns=' + S.turns.length);
  S.seq = 0;
  S.hits = [];
  S.hitIdx = -1;

  const title = S.cur ? (S.cur.title || S.cur.preview || 'Neues Gespräch') : 'Neues Gespräch';
  const dekParts = [];
  if (S.cur) {
    if (S.cur.started_at) dekParts.push(fmtClock(S.cur.started_at));
    if (S.cur.message_count) dekParts.push(`${S.cur.message_count} Nachrichten`);
    if (S.cur.estimated_cost_usd) dekParts.push(money(S.cur.estimated_cost_usd));
  }

  const body = S.turns.length
    ? S.turns.map(renderTurn).join('')
    : `<div class="empty">${S.loading ? 'Lädt …' : 'Noch nichts geschrieben. Fang unten an.'}</div>`;

  page.innerHTML =
    `<h1 class="title">${esc(trim(title, 90))}</h1>` +
    `<div class="dek">${esc(dekParts.filter(Boolean).join(' · ') || 'noch leer')}</div>` +
    body;

  page.querySelectorAll('pre').forEach(addCopy);
  hydrateMedia(page);
  updateHeader();

  const q = $('#findInput') ? $('#findInput').value.trim() : '';
  if (document.body.classList.contains('find-open') && q) highlight(q);

  /* Frisch geladener Verlauf fliegt gestaffelt herein; beim Nachzeichnen
     mitten im Lauf bleibt er ruhig. */
  if (animate) flyIn(page, 3);

  /* Ein Lauf, der zu dieser Sitzung gehört, hängt seinen Live-Abschnitt
     wieder an — auch nach einem Wechsel hin und zurück. */
  attachLive();
}

/* Live-Abschnitt wieder anhängen, wenn er zu dieser Sitzung gehört. */
function attachLive() {
  const run = activeRun();
  trace('attach run=' + (run && run.sid) + ' live=' + (run && run.live ? run.live.text.length : '-')
    + ' stream=' + (run && run.stream ? 1 : 0) + ' view=' + document.body.dataset.view);
  if (!run) return;
  /* Der Abschnitt kann beim Wegschalten verloren gegangen sein (Strom endete,
     während ein anderes Gespräch offen war). Dann hier neu aufbauen, statt
     bis zum Ende nichts zu zeigen. */
  if (!run.live) run.live = createLiveSection();
  const live = run.live;
  if (!live || !live.sec) return;
  /* Verwaiste Live-Abschnitte dieses Fensters entfernen — sonst steht oben
     ein leerer Abschnitt, während der laufende Text weiter unten wächst. */
  for (const el of document.querySelectorAll('.sec.fresh')) {
    if (el !== live.sec) el.remove();
  }
  const sc = document.scrollingElement || document.documentElement;
  /* Nur mitgehen, wenn man ohnehin unten stand — wer weiter oben liest,
     soll beim Zurückkommen nicht ans Ende gerissen werden. Ein laufender
     Lauf zählt dabei als „unten“. */
  const nearBottom = !!run.stream || sc.scrollHeight - (sc.scrollTop + sc.clientHeight) < 160;
  mountLive(live);
  /* Zurück in diesem Gespräch: sofort neu zeichnen (rAF könnte geruht
     haben), Randlicht neu anwerfen und den Betriebszustand nachziehen. */
  paintLive(live, true);
  sendBtnBusy(streamingHere());
  if (syncBusy()) rearmGlow();
  if (nearBottom) scrollBottom(true);
}

function updateHeader() {
  const n = S.sessions.length;
  const idx = S.cur ? S.sessions.findIndex((s) => s.id === S.cur.id) : -1;
  const kep = $('#kep');
  if (kep) {
    kep.innerHTML = idx >= 0
      ? `Gespräch <b>${idx + 1}</b> von ${n}`
      : `${n} ${n === 1 ? 'Gespräch' : 'Gespräche'}`;
  }
  const pg = $('#pgnum');
  if (pg) pg.textContent = idx >= 0 ? `${idx + 1}/${n}` : `${n} ${n === 1 ? 'Gespräch' : 'Gespräche'}`;
  renderGauge();
}

/* Nachgeladenes gestaffelt einfliegen lassen. `max` deckelt die Verzögerung:
   auch bei fünfzig Abschnitten bleibt der ganze Aufbau unter einer Sekunde. */
function flyIn(host, max = 6, ms = 1000) {
  if (!host || !host.children.length) return;
  const kids = Array.from(host.children);
  kids.forEach((el, i) => el.style.setProperty('--i', String(Math.min(i, max))));
  host.classList.remove('fly');
  void host.offsetWidth;
  host.classList.add('fly');
  clearTimeout(host._fly);
  host._fly = setTimeout(() => {
    host.classList.remove('fly');
    kids.forEach((el) => el.style.removeProperty('--i'));
  }, ms);
}

/* ── Verzeichnis ──────────────────────────────────────────────────── */

function updatePageNo() {
  const el = $('#pageNo');
  if (!el) return;
  const id = S.lastId || (S.cur && S.cur.id);
  const i = id ? S.sessions.findIndex((s) => s.id === id) : -1;
  el.textContent = 'Nr. ' + pad3(i >= 0 ? i + 1 : 1);
}

function chapterRow(s, i) {
  const num = `<span class="ch-num">${i + 1}${s.pinned ? '<i class="dot"></i>' : ''}</span>`;
  const title = esc(trim(s.title || s.preview || 'ohne Titel', 90));
  const time = esc(relTime(s.last_active || s.started_at));
  const prev = String(s.preview || '').trim();
  const prevHtml = prev && prev !== String(s.title || '').trim()
    ? `<p class="ch-prev">${esc(trim(prev, 140))}</p>`
    : '';

  return `<button class="ch" data-id="${esc(s.id)}" style="--i:${Math.min(i, 8)}">
    <div class="ch-top"><div class="ch-l">${num}<h2>${title}</h2></div><time>${time}</time></div>${prevHtml}
  </button>`;
}

function renderIndex(animate = false) {
  const host = $('#idxList');
  if (!host) return;
  host.innerHTML = S.sessions.length
    ? S.sessions.map(chapterRow).join('')
    : '<div class="empty">Noch kein Gespräch.</div>';

  /* Zeilen laufen gestaffelt ein: beim Wechsel ins Verzeichnis und beim
     ersten Aufbau nach dem Start. Spätere Auffrischungen bleiben still. */
  host.classList.remove('enter');
  const first = !renderIndex._ready;
  if (S.sessions.length) renderIndex._ready = true;
  if ((animate || first) && S.sessions.length) {
    void host.offsetWidth;
    host.classList.add('enter');
    clearTimeout(renderIndex._t);
    renderIndex._t = setTimeout(() => host.classList.remove('enter'), 1200);
  }
  updatePageNo();
  /* Läuft ein Gespräch im Hintergrund, bekommt seine Zeile den Punkt. */
  markRunningRows();
}

async function loadSessions(silent = false) {
  try {
    const list = await invoke('sessions_list');
    S.sessions = Array.isArray(list) ? list : [];
    if (S.cur) S.cur = S.sessions.find((s) => s.id === S.cur.id) || null;
    if (S.lastId && !S.sessions.some((s) => s.id === S.lastId)) S.lastId = null;

    if (document.body.dataset.view === 'index') renderIndex();
    if (document.body.dataset.view === 'reading') updateHeader();
    updatePageNo();
    return true;
  } catch (err) {
    S.err = errText(err);
    if (!silent) {
      setStatus('keine Verbindung', 'err');
      toast(S.err, true);
    }
    return false;
  }
}

/* ── Aktionsaesmyra ─────────────────────────────────────────────────── */

function openSheet(id) {
  const s = S.sessions.find((x) => x.id === id);
  if (!s) return;
  S.sheetId = id;
  const title = $('#sheetTitle');
  if (title) title.textContent = trim(s.title || s.preview || 'ohne Titel', 60);
  const pin = $('#sheet [data-act="pin"]');
  if (pin) pin.textContent = s.pinned ? 'Nicht mehr anheften' : 'Anheften';
  document.body.classList.add('sheet-open');
}

function closeSheet() {
  document.body.classList.remove('sheet-open');
  S.sheetId = null;
}

async function sheetAction(act) {
  const id = S.sheetId;
  const s = S.sessions.find((x) => x.id === id);
  if (!id || !s) return;

  try {
    if (act === 'pin') {
      await invoke('session_patch', { sessionId: id, pinned: !s.pinned });
      await loadSessions(true);
      closeSheet();
    } else if (act === 'rename') {
      const t = prompt('Neuer Titel', s.title || s.preview || '');
      if (t === null) return;
      const title = t.trim();
      if (!title) { toast('Titel bleibt'); return; }
      await invoke('session_patch', { sessionId: id, title });
      await loadSessions(true);
      if (S.cur && S.cur.id === id) renderChapter();
      closeSheet();
    } else if (act === 'del') {
      if (!confirm('Gespräch wirklich löschen?')) return;
      if (runOf(id)) {
        if (S.cur && S.cur.id === id) await abortStream();
        else closeRun(runOf(id));
      }
      await invoke('session_delete', { sessionId: id });
      CACHE.delete(id);
      if (S.lastId === id) { S.lastId = null; writeStore('aesmyra.last', ''); }
      if (S.cur && S.cur.id === id) {
        S.cur = null;
        S.turns = [];
        S.loading = false;
        renderChapter();
        if (document.body.dataset.view === 'reading') showView('index');
      }
      await loadSessions(true);
      closeSheet();
      toast('gelöscht');
    }
  } catch (err) {
    toast(errText(err), true);
  }
}

/* ── Sitzungen öffnen und anlegen ─────────────────────────────────── */

/* „Neues Gespräch" legt nur einen leeren Entwurf an. Die Sitzung auf dem Server
   entsteht erst beim ersten Senden — wer die Seite verlässt, hinterlässt nichts. */
async function newChapter() {
  /* Ein laufender Lauf gehört dem Server. Ein neues Gespräch daneben zu
     beginnen ist erlaubt und fragt nicht mehr nach — der Lauf schreibt seine
     Antwort in sein eigenes Gespräch, der Entwurf bleibt davon unberührt. */
  S.cur = null;
  S.draft = true;
  writeStore('aesmyra.open', '');
  S.readOnly = false;
  setComposerRO(false);
  S.turns = [];
  S.loading = false;

  showView('reading', 'new');
  renderChapter();
  scrollToY(0);
  setStatus('bereit');
  setTimeout(() => { const i = $('#input'); if (i) i.focus(); }, 80);
}

/* Erste Sitzung eines Entwurfs anlegen; danach ist es ein gewöhnliches Gespräch.
   Ein zweiter Aufruf während des Anlegens hängt sich an denselben Vorgang. */
let creatingSession = null;

async function ensureSession() {
  if (S.cur) return S.cur;
  if (creatingSession) return creatingSession;

  creatingSession = (async () => {
    const s = await invoke('session_create');
    if (!s || !s.id) throw 'Keine Sitzung erhalten';
    s.message_count = s.message_count || 0;
    s.estimated_cost_usd = s.estimated_cost_usd || 0;

    S.sessions.unshift(s);
    CACHE.set(s.id, []);
    S.cur = s;
    S.draft = false;
    /* Der Entwurf ist jetzt ein echtes Gespräch — die Markierung wandert mit. */
    if (S.navActive === 'new') { S.navActive = 'reading'; markNav(); }
    S.lastId = s.id;
    writeStore('aesmyra.last', s.id);
    return s;
  })();

  try {
    return await creatingSession;
  } finally {
    creatingSession = null;
  }
}

async function openSession(id) {
  const target = S.sessions.find((s) => s.id === id);
  trace('open ' + id + ' found=' + (target ? 1 : 0) + ' runs=' + S.runs.size);
  if (!target) return;

  /* Ein Wechsel bricht nichts ab: der Lauf gehört dem Server, der Wächter
     bleibt an ihm hängen und holt die Antwort nach. Die Ansicht ist frei. */

  if (S.cur?.id === id && document.body.dataset.view === 'reading') {
    S.draft = false;
    showReading();
    return;
  }

  if (S.cur && S.cur.id !== id) saveScroll();

  S.draft = false;
  S.cur = target;
  S.readOnly = !WRITABLE.includes(target.source || '');
  setComposerRO(S.readOnly);
  S.lastId = id;
  writeStore('aesmyra.last', id);
  writeStore('aesmyra.open', id);
  updatePageNo();
  showView('reading');

  const cached = CACHE.get(id);
  if (cached) {
    S.turns = cached;
    S.loading = false;
    renderChapter(true);
    requestAnimationFrame(() => restoreScroll(id));
    setStatus(S.readOnly ? 'nur lesbar' : 'bereit', S.readOnly ? 'warn' : '');
    if (syncBusy()) setStatus('arbeitet …', 'warn');
  } else {
    S.turns = [];
    S.loading = true;
    renderChapter();
    setStatus('lade …', 'warn');
  }

  try {
    const turns = await invoke('messages_load', { sessionId: id });
    CACHE.set(id, turns);
    if (CACHE.size > CACHE_MAX) CACHE.delete(CACHE.keys().next().value);
    if (!S.cur || S.cur.id !== id) return;
    S.loading = false;

    if (!cached) {
      S.turns = turns;
      renderChapter(true);
      requestAnimationFrame(() => restoreScroll(id));
    } else if (turns.length !== S.turns.length) {
      const y = scrollTopNow();
      S.turns = turns;
      renderChapter();
      scrollToY(y);
    } else {
      S.turns = turns;
    }
    setStatus(S.readOnly ? 'nur lesbar' : 'bereit', S.readOnly ? 'warn' : '');
    /* Läuft hier gerade ein Lauf, bleibt der Betriebszustand stehen und der
       Rand leuchtet weiter — auch wenn der Verlauf neu gezeichnet wurde. */
    if (syncBusy()) {
      setStatus('arbeitet …', 'warn');
      sendBtnBusy(streamingHere());
      if (streamingHere() || liveHere()) rearmGlow();
    }
  } catch (err) {
    S.loading = false;
    if (!cached) {
      S.turns = [];
      renderChapter();
      setStatus('Fehler', 'err');
      toast('Gespräch konnte nicht geladen werden: ' + errText(err), true);
    }
  }
}

function scrollTopNow() {
  return Math.max(window.scrollY || 0, document.documentElement.scrollTop || 0, document.body.scrollTop || 0);
}

/* body ist hier der Scrollcontainer (height:100% + overflow:auto);
   window und documentElement werden trotzdem mitgesetzt. */
function scrollToY(y, smooth = false) {
  const top = Math.max(0, Math.round(Number(y) || 0));
  const opts = { top, behavior: smooth ? 'smooth' : 'auto' };
  window.scrollTo(opts);
  if (document.documentElement.scrollTo) document.documentElement.scrollTo(opts);
  if (document.body.scrollTo) document.body.scrollTo(opts);
}

/* Nach einem Lauf den Verlauf neu holen; Position halten, wenn man liest. */
/* Nach einer Verdichtung liefert der Server frühere Antworten als leere
   Zeilen (die Zusammenfassung ersetzt sie). Was die App schon gelesen hat,
   bleibt sichtbar: gleiche Frage, bekannte Antwort. */
function keepKnownText(turns, previous) {
  if (!Array.isArray(turns) || !Array.isArray(previous) || !previous.length) return turns;
  const known = new Map();
  for (const t of previous) {
    const key = (t.user || '').trim();
    if (!key) continue;
    const parts = Array.isArray(t.parts) ? t.parts : [];
    if (parts.some((p) => (p.text || '').trim())) known.set(key, parts);
  }
  if (!known.size) return turns;
  let changed = false;
  const out = turns.map((t) => {
    const parts = Array.isArray(t.parts) ? t.parts : [];
    if (parts.some((p) => (p.text || '').trim())) return t;
    const old = known.get((t.user || '').trim());
    if (!old) return t;
    changed = true;
    return { ...t, parts: old };
  });
  return changed ? out : turns;
}

async function reloadMessages(dropLive = true) {
  if (!S.cur) return;
  const id = S.cur.id;
  const before = scrollTopNow();
  const nearBottom = before + window.innerHeight > document.body.scrollHeight - 240;

  try {
    const turns = keepKnownText(
      await invoke('messages_load', { sessionId: id }),
      CACHE.get(id) || S.turns,
    );
    if (!S.cur || S.cur.id !== id) return;
    CACHE.set(id, turns);
    S.turns = turns;
    const act = activeRun();
    if (dropLive && act && act.live) { unmountLive(act.live); act.live = null; }
    renderChapter();
    if (nearBottom) scrollBottom();
    else scrollToY(before);
  } catch {
    /* still: der Live-Abschnitt bleibt bis zum nächsten Laden stehen */
  }
}

function scrollBottom(smooth = false) {
  const h = Math.max(document.body.scrollHeight, document.documentElement.scrollHeight);
  scrollToY(h, smooth);
}

function saveScroll() {
  if (!S.cur) return;
  if (document.body.dataset.view !== 'reading') return;
  S.restores[S.cur.id] = scrollTopNow();
  writeStore('aesmyra.scroll', S.restores);
}

function restoreScroll(id) {
  /* Läuft in dieser Sitzung etwas, gehört der Blick nach unten zum Lauf —
     eine gemerkte Leseposition zeigt nur den alten Verlauf und lässt den
     laufenden Lauf unsichtbar wirken. */
  if (runActive(runOf(id))) { scrollBottom(); return; }
  const y = S.restores[id];
  if (typeof y === 'number') scrollToY(y);
  else scrollBottom();
}

/* ── Senden und Strom ─────────────────────────────────────────────── */

/* Läufe liegen je Gespräch in `S.runs`. Wer in einem zweiten Gespräch
   schreibt, während das erste noch antwortet, stört den ersten Lauf nicht:
   er hängt an seiner Sitzung, nicht an der Ansicht. Gezeichnet wird nur,
   was gerade offen ist — alles andere läuft im Hintergrund weiter. */
function runOf(sid) { return (sid && S.runs.get(sid)) || null; }
function activeRun() { return S.cur ? runOf(S.cur.id) : null; }
function liveHere() { const r = activeRun(); return r ? r.live : null; }
function streamingHere() { const r = activeRun(); return !!(r && r.stream); }
function anyWatch() {
  for (const r of S.runs.values()) if (r.watch && !r.watch.done) return true;
  return false;
}

function createLiveSection() {
  const sec = document.createElement('section');
  sec.className = 'sec fresh';
  sec.innerHTML = `<aside class="notes"></aside><div class="prose"></div>
    <div class="writing"><i></i><i></i><i></i><i></i><i></i><span class="wtext">denke nach …</span></div>`;
  return {
    sec,
    prose: sec.querySelector('.prose'),
    notes: sec.querySelector('.notes'),
    writing: sec.querySelector('.writing'),
    text: '',
    tools: new Map(),
    dirty: false,
    pending: false,
  };
}

function mountLive(live) {
  if (!live || !live.sec) return;
  if (live.sec.isConnected) return;
  $('#page').appendChild(live.sec);
}

function unmountLive(live) {
  if (live && live.sec && live.sec.parentNode) live.sec.remove();
}

function paintLive(live = liveHere(), force = false) {
  if (!live) return;
  if (live.dirty && !force) {
    // Schon ein Bild geplant: der läuft gleich und nimmt diesen Text mit.
    live.pending = true;
    return;
  }
  live.dirty = true;
  live.pending = false;
  let painted = false;
  const run = () => {
    if (painted) return;
    painted = true;
    try {
      if (!live.sec || !live.prose) return;
      live.prose.innerHTML = md(live.text);
      hydrateMedia(live.prose);
    } catch {
      /* Ein halbfertiges Stück Markdown oder ein Medium, das noch nicht
         da ist, darf den Text nicht dauerhaft einfrieren. */
      live.prose.textContent = live.text;
    } finally {
      live.dirty = false;
    }
    if (live.pending) {
      live.pending = false;
      paintLive(live);
    }
  };
  // Ab 20 000 Zeichen kostet das Neusetzen des ganzen Textes mehr als ein
  // Takt: lieber 16-mal je Sekunde flüssig als 60-mal ruckelig.
  requestAnimationFrame(run);
  /* Rückfall: rAF ruht, solange die Ansicht nicht gezeichnet wird (anderes
     Gespräch, Android im Hintergrund). Ohne diesen zweiten Weg bliebe das
     Bild aus und der Text wäre bis zum Ende unsichtbar. */
  setTimeout(run, live.text.length > 20000 ? 90 : 40);
}

/* Einen Lauf von der Anzeige lösen, ohne den Server zu bitten. */
function closeRun(run) {
  if (!run) return;
  run.closed = true;
  run.stream = false;
  run.gen++;
  clearInterval(run.thinkPoll);
  run.thinkPoll = 0;
  if (run.thinkTimer) { clearTimeout(run.thinkTimer); run.thinkTimer = 0; }
  if (run.watch) { run.watch.done = true; run.watch = null; }
  unmountLive(run.live);
  run.live = null;
  if (S.runs.get(run.sid) === run) S.runs.delete(run.sid);
  if (!anyWatch()) stopApprovalWatch();
  syncBusy();
  sendBtnBusy(streamingHere());
}

async function send(text) {
  text = String(text || '').trim();
  if (!text) return;

  /* Sitzungen anderer Oberflächen darf nur ihr eigener Besitzer beschreiben.
     Ohne offene Sitzung entsteht gleich eine eigene — die Sperre gilt dann nicht. */
  if (S.readOnly && S.cur) {
    toast('Diese Session gehört zu einer anderen Oberfläche — hier nur lesbar.', true);
    return;
  }

  /* Läuft in *diesem* Gespräch noch eine Antwort, wird nachgeschoben. Ein
     Lauf in einem anderen Gespräch ist kein Hindernis: dort wird gesendet,
     hier läuft es weiter. */
  const here = activeRun();
  if (here && here.stream) { await steer(text, here); return; }
  /* Fertiger Lauf, dessen Wächter noch nachsieht: still beenden, der neue
     Lauf zieht den Verlauf ohnehin nach. */
  if (here) closeRun(here);

  /* Erste Zeile eines neuen Gesprächs: Sitzung erst jetzt anlegen. */
  if (!S.cur) {
    try {
      await ensureSession();
    } catch (err) {
      toast('Neues Gespräch fehlgeschlagen: ' + errText(err), true);
      return;
    }
    S.turns = [];
    /* Mit der Sitzung ist das Gespräch der Grund der Ansicht, nicht der Knopf. */
    S.navActive = 'reading';
    markNav();
  }

  const input = $('#input');
  input.value = '';
  grow();

  const sid = S.cur.id;
  S.turns.push({ user: text, parts: [] });
  renderChapter();

  const live = createLiveSection();
  const run = {
    sid,
    gen: ++S.gen,
    runId: null,
    stream: true,
    live,
    watch: null,
    thinkPoll: 0,
    thinkTimer: 0,
    lastEventAt: Date.now(),
    sent: text,
    failedText: '',
    closed: false,
  };
  const old = S.runs.get(sid);
  if (old) closeRun(old);
  S.runs.set(sid, run);

  mountLive(live);
  scrollBottom(true);
  setBusy(true);
  setStatus('arbeitet …', 'warn');
  sendBtnBusy(true);

  await streamTurn(run, text);
}

async function streamTurn(run, text) {
  if (!Channel) {
    toast('Ereigniskanal fehlt', true);
    finishTurn(run, false);
    return;
  }

  /* Solange der Lauf dauert, regelmäßig nach dem Denken sehen. */
  clearInterval(run.thinkPoll);
  run.thinkPoll = setInterval(() => queueReasonSync(run), 2500);
  startRunWatch(run);

  const channel = new Channel();
  channel.onmessage = (event) => handleEvent(event, run);

  try {
    await invoke('chat_start', { sessionId: run.sid, text, onEvent: channel });
  } catch (err) {
    if (run.closed) return;
    toast(errText(err), true);
    finishTurn(run, false);
  }
}

function handleEvent(ev, run) {
  if (!ev || !ev.kind || !run) return;

  /* Ereignisse eines abgebrochenen oder überholten Laufs verwerfen: sonst
     reißt ein spätes `finished` den neuen Lauf nieder. */
  if (run.closed || run.gen === 0) return;

  /* Zweites Kennzeichen, sobald der Kern `run_id` mitliefert. */
  if (ev.run_id && run.runId && ev.run_id !== run.runId && ev.kind !== 'run_started') return;

  /* Lebenszeichen für den Wächter: daran erkennt er, ob der Strom noch
     wirklich läuft oder nur der Lauf stehen geblieben ist. */
  run.lastEventAt = Date.now();

  const live = run.live;
  const mine = !!(S.cur && S.cur.id === run.sid);
  if (ev.kind !== 'delta' || Date.now() - (run.traceAt || 0) > 1500) {
    run.traceAt = Date.now();
    trace('ev ' + ev.kind + ' sid=' + run.sid + ' live=' + (live ? live.text.length : -1) + ' mine=' + (mine ? 1 : 0));
  }

  switch (ev.kind) {
    case 'run_started':
      run.runId = ev.run_id;
      if (mine) setStatus('arbeitet …', 'warn');
      break;
    case 'delta':
      if (live) {
        live.text += ev.text || '';
        /* Sobald der erste Text da ist, ist das Denken vorbei. */
        const wt = live.writing && live.writing.querySelector('.wtext');
        if (wt && live.text) wt.textContent = 'ich schreibe …';
        paintLive(live);
      }
      break;
    case 'commentary':
      if (live && ev.text) {
        live.text += (live.text ? '\n\n' : '') + ev.text;
        paintLive(live);
      }
      break;
    case 'tool_started':
      addLiveTool(ev, live);
      /* Das Denken dazu ist schon geschrieben, bevor das Werkzeug läuft. */
      queueReasonSync(run);
      break;
    case 'tool_finished':
      markLiveTool(ev, live);
      /* Denken steht nur im gespeicherten Verlauf, nicht im Strom — nachladen. */
      queueReasonSync(run);
      break;
    case 'approval':
      addApproval(ev, live);
      break;
    /* Der Server hat die Freigabe zurückgezogen (neue Laufgeneration oder
       anderweitig beantwortet). Die Karte wird stillgelegt, damit kein
       Klick mehr in ein 409 läuft. */
    case 'approval_settled': {
      const a = S.approvals.find((x) => x.request_id === ev.request_id);
      if (a && !a.decided) { a.decided = true; a.expired = true; }
      const el = document.querySelector('.approve[data-rid="' + String(ev.request_id).replace(/"/g, '') + '"]');
      if (el && !el.classList.contains('done')) {
        el.classList.add('done');
        el.querySelectorAll('button').forEach((b) => { b.disabled = true; });
        const row = el.querySelector('.row');
        if (row) row.innerHTML = '<span class="tstate warn">zurückgezogen</span>';
      }
      if (document.body.dataset.view === 'approvals') renderApprovals();
      break;
    }
    case 'usage':
      rememberTokens(run.sid, Number(ev.input_tokens) || 0, Number(ev.api_calls) || 1);
      renderGauge();
      break;
    case 'completed':
      queueReasonSync(run);
      if (live) {
        /* Der Schlussbericht ist maßgeblich, aber er darf den bereits
           gezeigten Text nicht verkürzen: sonst springt die Antwort am
           Ende sichtbar zurück ("nur teilweise"). */
        const end = String(ev.content || '');
        if (end && end.length >= live.text.length) live.text = end;
        paintLive(live, true);
        if (live.writing) live.writing.remove();
      }
      break;
    case 'failed':
      run.failedText = String(ev.message || 'Lauf fehlgeschlagen');
      toast(run.failedText.slice(0, 160), true);
      break;
    case 'cancelled':
      if (mine) setStatus('abgebrochen', 'warn');
      break;
    case 'error':
      /* Der Text stand vor dem Senden im Feld — nicht verlieren. */
      run.failedText = String(ev.message || 'Fehler');
      toast(run.failedText.slice(0, 160), true);
      break;
    case 'reconnecting':
      if (mine) setStatus(`unterbrochen · Versuch ${ev.attempt}`, 'warn');
      break;
    case 'recovered':
      if (mine) setStatus('läuft weiter …', 'warn');
      break;
    case 'stream_ended':
      if (mine) setStatus('Verbindung beendet', 'warn');
      break;
    case 'finished':
      finishTurn(run, ev.ok !== false);
      break;
    default:
      break;
  }
}

/* Der Server schickt kein Denken im Ereignisstrom; es steht nur in der
   gespeicherten Nachricht. Deshalb während des Laufs kurz nachsehen. */
function queueReasonSync(run) {
  if (!run || run.closed) return;
  if (run.thinkTimer) return;
  run.thinkTimer = setTimeout(() => {
    run.thinkTimer = 0;
    syncLiveReasoning(run);
  }, 700);
}

async function syncLiveReasoning(run) {
  if (!run || run.closed || !run.live) return;
  const id = run.sid;
  try {
    const turns = await invoke('messages_load', { sessionId: id });
    if (run.closed || !run.live) return;
    const last = Array.isArray(turns) ? turns[turns.length - 1] : null;
    if (!last) return;
    const text = (last.parts || []).map((p) => p.reasoning).filter(Boolean).join('\n\n');
    if (text) paintLiveThink(run.live, text);
  } catch {
    /* still: beim nächsten Werkzeugschritt erneut versuchen */
  }
}

function paintLiveThink(live, text) {
  if (!live) return;
  let el = live.think;
  if (!el) {
    el = document.createElement('details');
    el.className = 'note reason';
    /* Zugeklappt bleiben — in jeder Ansicht und auf jedem Gerät. */
    el.open = false;
    el.innerHTML = '<summary><span class="num">·</span><b>Denken</b></summary><div class="nbody"><p></p></div>';
    live.think = el;
    live.notes.prepend(el);
  }
  const p = el.querySelector('p');
  const shown = trim(text, 4000);
  if (p.textContent !== shown) p.textContent = shown;
}

/* Nachgeschobene Eingabe sofort im laufenden Abschnitt zeigen. */
function addLiveSteer(text, live = liveHere()) {
  if (!live || !live.notes) return;
  const el = document.createElement('div');
  el.className = 'note steer';
  el.innerHTML = `<span class="num">↳</span><b>nachgeschoben</b><span class="prev">· ${esc(trim(text, 120))}</span>`;
  live.notes.appendChild(el);
  scrollBottom(true);
}

function addLiveTool(p, live = liveHere()) {
  if (!live) return;
  const n = ++S.seq;
  const t = { name: p.name || 'werkzeug', args: p.args || '', out: '' };
  const wrap = document.createElement('div');
  wrap.innerHTML = toolNote(t, n);
  const el = wrap.firstElementChild;
  if (!el) return;
  el.querySelector('[data-state]').textContent = ' …';
  el.dataset.tool = t.name;
  live.notes.appendChild(el);
  live.tools.set(t.name + ':' + n, el);
}

function markLiveTool(p, live = liveHere()) {
  if (!live) return;
  const name = p.name || 'werkzeug';
  let found = null;
  for (const [k, el] of live.tools) {
    if (k.startsWith(name + ':') && !el.dataset.done) { found = el; break; }
  }
  if (!found) {
    addLiveTool(p, live);
    for (const [k, el] of live.tools) {
      if (k.startsWith(name + ':') && !el.dataset.done) { found = el; break; }
    }
  }
  if (!found) return;

  found.dataset.done = '1';
  const st = found.querySelector('[data-state]');
  if (st) {
    st.textContent = p.ok ? ' ok' : ' fehler';
    st.className = 'tstate ' + (p.ok ? 'ok' : 'bad');
  }
  if (p.preview) {
    const body = found.querySelector('.nbody');
    if (body && !body.querySelector('pre')) {
      const pre = document.createElement('pre');
      pre.textContent = String(p.preview);
      body.appendChild(pre);
    }
  }
}

function finishTurn(run, ok = true) {
  if (!run) return;
  trace('finishTurn sid=' + run.sid + ' ok=' + (ok ? 1 : 0) + ' live=' + (run.live ? run.live.text.length : -1));
  const runId = run.runId;
  const sid = run.sid;
  const viewing = !!(S.cur && S.cur.id === sid);

  /* Der Strom ist zu Ende: was noch ankommt, gehört nicht hierher. */
  run.stream = false;
  run.gen++;
  run.closed = true;
  clearInterval(run.thinkPoll);
  run.thinkPoll = 0;
  if (run.thinkTimer) { clearTimeout(run.thinkTimer); run.thinkTimer = 0; }

  /* Der Lauf kann auf dem Server weitergehen: der Wächter bleibt dran und
     zieht die Antwort nach, sobald sie fertig ist. */
  if (runId) S.lastPull = { runId, at: Date.now() };

  /* Offene Freigaben eines beendeten Laufs sind erledigt; entschiedene bleiben. */
  if (runId) {
    S.approvals = S.approvals.filter((a) => a.run_id !== runId || a.decided);
    if (document.body.dataset.view === 'approvals') renderApprovals();
  }

  /* Ein Lauf, der nie angenommen wurde, darf die Eingabe nicht schlucken. */
  if (!ok && run.failedText && viewing) {
    const field = $('#input');
    if (field && run.sent) {
      field.value = run.sent;
      grow();
      field.focus();
    }
  }

  if (viewing) {
    setStatus(ok ? 'bereit' : 'beendet', ok ? '' : 'warn');
    /* Der Live-Abschnitt bleibt stehen, bis der Verlauf wirklich neu geladen ist. */
    reloadMessages(ok).then(() => loadSessions(true));
  } else {
    /* Der Lauf gehörte zu einem anderen Gespräch. Dort nachziehen, hier
       nichts stehen lassen — der Lauf selbst bleibt für den Wächter, und
       sein Abschnitt bleibt erhalten: kehrt der Nutzer zurück, soll er den
       Stand bis hier sehen, nicht ein leeres Blatt. */
    CACHE.delete(sid);
    loadSessions(true);
    unmountLive(run.live);
    if (S.cur) setStatus('Antwort liegt bereit', '');
  }

  sendBtnBusy(streamingHere());
  /* Der Strom ist zu Ende, der Serverlauf womöglich auch: einmal sofort
     nachfragen statt bis zum nächsten Takt zu leuchten. Antwortet der Server
     noch „läuft“, bleibt der Wächter unverändert dran. */
  if (run.watch && !run.watch.done) pollRun(run);
  /* Nicht blind aus: solange der Wächter noch am Serverlauf hängt, arbeitet
     die Sache weiter und der Rand darf weiter leuchten. */
  pruneRun(run);
  syncBusy();
}

async function abortStream() {
  const run = activeRun();
  if (!run) {
    sendBtnBusy(false);
    return;
  }
  const runId = run.runId;
  const sid = run.sid;

  /* Erst den Lauf schließen, dann stoppen — sonst kommen noch Ereignisse
     des alten Laufs durch. */
  closeRun(run);

  try { await invoke('chat_stop', { sessionId: sid }); } catch { /* egal */ }

  if (runId) {
    S.approvals = S.approvals.filter((a) => a.run_id !== runId || a.decided);
    if (document.body.dataset.view === 'approvals') renderApprovals();
  }

  setStatus('abgebrochen', 'warn');
  await reloadMessages();
  loadSessions(true);
}

/* Sperrt das Eingabefeld für Sitzungen, die einer anderen Oberfläche gehören. */
function setComposerRO(on) {
  const input = $('#input');
  const btn = $('#sendBtn');
  const form = $('#composer');
  if (!input || !btn) return;

  input.disabled = on;
  input.placeholder = on ? 'Diese Session ist nur lesbar' : 'Nachricht schreiben …';
  if (form) form.classList.toggle('readonly', on);

  if (on) {
    btn.disabled = true;
    setStatus('nur lesbar', 'warn');
  } else {
    sendBtnBusy(streamingHere());
    if (!streamingHere()) setStatus('bereit');
  }
}

function sendBtnBusy(v) {
  const b = $('#sendBtn');
  if (!b) return;
  b.classList.toggle('stop', v);
  b.innerHTML = v
    ? '<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="1.5"/></svg>'
    : '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round"><path d="M12 19V5M6 11l6-6 6 6"/></svg>';
  const i = $('#input');
  b.disabled = !v && !(i && i.value.trim());
}

async function steer(text, run = activeRun()) {
  if (!run || !run.runId) { toast('Lauf noch nicht bereit'); return; }
  try {
    await invoke('run_steer', { runId: run.runId, message: text });
    addLiveSteer(text, run.live);
    const input = $('#input');
    input.value = '';
    grow();
    toast('nachgeschoben');
    const w = run.live && run.live.writing ? run.live.writing.querySelector('.wtext') : null;
    if (w) w.textContent = 'arbeitet … mit Zusatz';
  } catch (err) {
    toast('Nachschieben nicht möglich: ' + errText(err), true);
  }
}

/* ── Freigaben ────────────────────────────────────────────────────── */

function pushApproval(p) {
  const rid = p.request_id || ('r' + Date.now());
  const existing = S.approvals.find((a) => a.request_id === rid);
  if (existing) return existing;

  const entry = {
    run_id: p.run_id || '',
    request_id: rid,
    execution_generation: Number(p.execution_generation) || 0,
    command: p.command || '',
    choices: Array.isArray(p.choices) && p.choices.length ? p.choices : ['once', 'session', 'deny'],
    decided: false,
    choice: null,
    ts: Date.now(),
  };
  S.approvals.push(entry);
  return entry;
}

/* Der Server lehnt eine Freigabe mit `stale_generation` ab, sobald die
   Laufgeneration nicht mehr stimmt — Lauf neu gestartet, Freigabe
   zurückgezogen oder Lauf schon beendet. Statt den Fehler nur anzuzeigen,
   holt die Oberfläche den Laufzustand und sendet genau einmal mit den
   Werten, die der Server selbst als gültig meldet. */
const STALE_APPROVAL = /stale_generation|approval_not_pending|Status 40[49]|Status 410/;

/* Der Server führt die offenen Kontrollen eines Laufs; die Karte ist nur eine
   Abschrift davon und kann veralten. Maßgeblich ist deshalb immer der
   Laufzustand, den `run_state` meldet. */
async function launchState(runId) {
  const st = await invoke('run_state', { runId });
  const pend = (st && Array.isArray(st.pending_controls)) ? st.pending_controls : [];
  return {
    status: String((st && st.status) || 'unknown'),
    generation: Number(st && st.execution_generation) || 0,
    controls: pend.filter((p) => p && p.kind !== 'clarify'),
  };
}

/* Kandidaten: der Lauf der Karte und die Läufe, die gerade laufen. */
function runIdCandidates(entry) {
  const running = [];
  for (const r of S.runs.values()) if (r.runId) running.push(r.runId);
  return [entry.run_id].concat(running)
    .filter((v, i, a) => v && a.indexOf(v) === i)
    .reverse();
}

async function resolvePending(entry) {
  for (const runId of runIdCandidates(entry)) {
    let st = null;
    try { st = await launchState(runId); } catch { continue; }
    const control = st.controls.find((p) => (p.request_id || p.prompt_id) === entry.request_id)
      || (st.controls.length === 1 ? st.controls[0] : null);
    if (control) {
      return {
        runId,
        control,
        generation: Number(control.execution_generation) || st.generation,
      };
    }
  }
  return null;
}

function retireApproval(entry) {
  entry.decided = true;
  entry.expired = true;
  if (document.body.dataset.view === 'approvals') renderApprovals();
}

async function decideApproval(entry, choice) {
  const send = (runId, requestId, generation) => invoke('run_approve', {
    runId,
    choice,
    requestId,
    executionGeneration: generation,
  });

  try {
    await send(entry.run_id || '', entry.request_id, entry.execution_generation || 0);
  } catch (err) {
    if (!STALE_APPROVAL.test(errText(err))) throw err;

    /* Der Lauf der Karte kennt die Freigabe nicht mehr — beim laufenden Lauf
       nachsehen und mit dessen Kennung und Generation genau einmal senden. */
    const healed = await resolvePending(entry);
    if (!healed) {
      retireApproval(entry);
      throw new Error('Diese Freigabe ist überholt — der Lauf hat sie nicht mehr offen.');
    }

    entry.run_id = healed.runId;
    entry.request_id = healed.control.request_id || healed.control.prompt_id || entry.request_id;
    entry.execution_generation = healed.generation;

    try {
      await send(entry.run_id, entry.request_id, entry.execution_generation);
    } catch (err2) {
      if (!STALE_APPROVAL.test(errText(err2))) throw err2;
      retireApproval(entry);
      throw new Error('Diese Freigabe ist überholt — der Lauf hat sie nicht mehr offen.');
    }
  }

  entry.decided = true;
  entry.choice = choice;
  if (document.body.dataset.view === 'approvals') renderApprovals();
}

/* Der Ereignisstrom kann unterwegs verloren gehen (Neuladen, Proxy, Mobilfunk).
   Deshalb hängt der Wächter am Lauf und nicht am Strom: er liest alle drei
   Sekunden den Laufzustand, hält Freigaben aktuell, und wenn der Lauf endet,
   ohne dass der Strom es meldet, holt er die Antwort nach. Ein Takt für alle
   Gespräche — jeder Lauf fragt nur nach seinem eigenen Zustand. */
function startRunWatch(run) {
  const tail = (S.turns || [])[S.turns.length - 1];
  const tailParts = tail && Array.isArray(tail.parts) ? tail.parts : [];
  const baseText = tailParts.length ? (tailParts[tailParts.length - 1].text || '') : '';
  run.watch = { runId: '', sessionId: run.sid, tries: 0, done: false,
                base: (S.turns || []).length, baseText, sentAt: Date.now() };
  if (!S.approvalWatch) S.approvalWatch = setInterval(watchTick, 3000);
  pollRun(run);
}

function watchTick() {
  const live = [];
  for (const r of S.runs.values()) if (r.watch && !r.watch.done) live.push(r);
  if (!live.length) { stopApprovalWatch(); return; }
  for (const r of live) pollRun(r);
}

function stopApprovalWatch() {
  if (S.approvalWatch) {
    clearInterval(S.approvalWatch);
    S.approvalWatch = 0;
  }
}

/* Antwort ohne Laufkennung nachziehen. Läuft der Strom noch, liefert er die
   Antwort selbst; erst wenn er weg ist, zählt der gespeicherte Verlauf. */
async function pollTranscript(run) {
  const w = run.watch;
  if (!w || w.done) return;
  if (run.stream) return;
  if (Date.now() - (w.sentAt || 0) < 5000) return;
  let turns = null;
  try { turns = await invoke('messages_load', { sessionId: w.sessionId }); } catch { return; }
  if (run.watch !== w || w.done || !turns) return;
  if (S.runs.get(run.sid) !== run && !run.live) return;
  const last = turns[turns.length - 1];
  const parts = last && Array.isArray(last.parts) ? last.parts : [];
  /* Eine Runde kann wachsen, ohne dass eine neue entsteht: der Text der
     letzten Antwort muss sich geändert haben, sonst wäre es die alte. */
  const text = parts.length ? (parts[parts.length - 1].text || '') : '';
  if (!text.trim() || text === w.baseText) return;
  await finishRun(run, 'completed');
}

async function pollRun(run) {
  const w = run.watch;
  if (!w || w.done) return;
  if (!w.runId && run.runId) w.runId = run.runId;
  if (!w.runId) {
    /* Die Kennung kommt mit dem Ereignisstrom. Bleibt sie aus, ist der
       Verlauf die letzte Quelle — sonst bliebe die Antwort unsichtbar. */
    await pollTranscript(run);
    return;
  }
  const runId = w.runId;

  let st = null;
  try { st = await launchState(runId); } catch { await pollTranscript(run); return; }
  if (run.watch !== w || w.done) return;
  w.tries = (w.tries || 0) + 1;

  for (const p of st.controls) {
    const rid = p.request_id || p.prompt_id;
    if (!rid || S.approvals.some((a) => a.request_id === rid)) continue;
    addApproval({
      run_id: runId,
      request_id: rid,
      execution_generation: Number(p.execution_generation) || st.generation,
      command: p.command || p.description || '',
      choices: p.choices,
    }, run.live);
  }

  for (const a of S.approvals) {
    if (a.decided || a.run_id !== runId) continue;
    if (st.controls.some((p) => (p.request_id || p.prompt_id) === a.request_id)) continue;
    a.decided = true;
    a.expired = true;
    const el = document.querySelector('.approve[data-rid="' + String(a.request_id).replace(/"/g, '') + '"]');
    if (el && !el.classList.contains('done')) {
      el.classList.add('done');
      el.querySelectorAll('button').forEach((b) => { b.disabled = true; });
      const row = el.querySelector('.row');
      if (row) row.innerHTML = '<span class="tstate warn">überholt</span>';
    }
  }

  if (document.body.dataset.view === 'approvals') renderApprovals();

  /* Endet der Lauf, ohne dass der Strom es meldet, wird die Antwort geholt. */
  const status = String(st.status || '');
  const ends = /completed|done|finished|failed|cancelled|error|aborted/i.test(status);
  if (ends && !st.controls.length) { await finishRun(run, status); return; }

  /* Kennt der Server den Lauf nicht (mehr), entscheidet der Verlauf. */
  if (/not_found|unknown|missing/i.test(status)) { await pollTranscript(run); return; }

  /* Notbremse: nach dreißig Minuten ohne Ende nicht ewig weiterfragen. */
  if (w.tries > 600) {
    w.done = true;
    run.watch = null;
    if (!anyWatch()) stopApprovalWatch();
    if (S.runs.get(run.sid) === run) S.runs.delete(run.sid);
    syncBusy();
  }
}

/* Ein Lauf ist serverseitig fertig: Antwort nachziehen, auch wenn der Strom
   längst weg ist. Läuft der Strom noch, beendet er sich selbst. */
async function finishRun(run, status) {
  const w = run.watch;
  if (w) w.done = true;
  run.watch = null;
  if (!anyWatch()) stopApprovalWatch();
  pruneRun(run);

  /* Der Server sagt: fertig. Läuft der Ereignisstrom noch und gibt er
     Lebenszeichen, beendet er sich selbst. Steht er nur noch stumm auf
     „läuft" (IPC weg, Ereignis verloren), wird er hier geschlossen — sonst
     bliebe die fertige Antwort unsichtbar. */
  if (run.stream) {
    if (Date.now() - run.lastEventAt < 8000) return;
    closeRun(run);
  }

  const failed = /failed|cancelled|error|aborted/i.test(String(status || ''));
  const viewing = !!(S.cur && S.cur.id === run.sid);
  const runId = (w && w.runId) || run.runId || '';
  const fresh = S.lastPull && S.lastPull.runId === runId && Date.now() - S.lastPull.at < 12000;

  CACHE.delete(run.sid);
  loadSessions(true);
  syncBusy();

  if (!viewing) {
    toast(failed ? 'Lauf ohne Antwort beendet.' : 'Antwort liegt im Gespräch bereit.', failed);
    return;
  }
  if (!fresh) {
    S.lastPull = { runId, at: Date.now() };
    await reloadMessages();
  }
  setStatus(failed ? 'Lauf beendet' : 'bereit', failed ? 'warn' : '');
}

function addApproval(p, live = liveHere()) {
  const entry = pushApproval(p);
  if (!live) return;

  const el = document.createElement('div');
  el.className = 'approve';
  el.dataset.rid = entry.request_id;
  el.innerHTML = `<b>Freigabe nötig</b>
    <code>${esc(p.command || 'Werkzeugaufruf')}</code>
    <div class="row">${entry.choices.map((c) => `<button data-choice="${esc(c)}"${c === 'once' ? ' class="once"' : ''}>${esc(LABELS[c] || c)}</button>`).join('')}</div>`;

  el.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-choice]');
    if (!b) return;
    el.querySelectorAll('button').forEach((x) => { x.disabled = true; });
    try {
      await decideApproval(entry, b.dataset.choice);
      el.classList.add('done');
      const row = el.querySelector('.row');
      if (row) row.innerHTML = `<span class="tstate ok">entschieden · ${esc(LABELS[b.dataset.choice] || b.dataset.choice)}</span>`;
      setStatus('arbeitet …', 'warn');
    } catch (err) {
      if (entry.expired) {
        el.classList.add('done');
        const row = el.querySelector('.row');
        if (row) row.innerHTML = '<span class="tstate warn">überholt</span>';
      } else {
        el.querySelectorAll('button').forEach((x) => { x.disabled = false; });
      }
      toast(entry.expired ? errText(err) : 'Freigabe fehlgeschlagen: ' + errText(err), true);
    }
  });

  live.notes.appendChild(el);
}

function renderApprovals(animate = false) {
  const host = $('#apprList');
  if (!host) return;
  const open = S.approvals.filter((a) => !a.decided);

  const cnt = $('#apprCount');
  if (cnt) cnt.textContent = String(open.length);
  const foot = $('#apprFoot');
  if (foot) foot.textContent = open.length ? 'Bitte entscheiden' : 'Nichts zu entscheiden';

  if (!S.approvals.length) {
    /* Die Fußzeile sagt bereits, dass nichts zu entscheiden ist. */
    host.innerHTML = '';
    return;
  }

  host.innerHTML = S.approvals.map((a) => {
    const row = a.decided
      ? `<div class="row"><span class="tstate ${a.expired ? 'warn' : 'ok'}">${a.expired ? 'überholt' : 'entschieden · ' + esc(LABELS[a.choice] || a.choice || '')}</span></div>`
      : `<div class="row">${a.choices.map((c) => `<button data-choice="${esc(c)}"${c === 'once' ? ' class="primary"' : ''}>${esc(LABELS[c] || c)}</button>`).join('')}</div>`;
    return `<div class="ap${a.decided ? ' done' : ''}" data-rid="${esc(a.request_id)}">
      <b>Freigabe nötig</b>
      <code>${esc(a.command || 'Werkzeugaufruf')}</code>${row}
      <span class="aph">Ohne Antwort verwirft der Server die Freigabe und der Lauf endet.</span>
    </div>`;
  }).join('');

  if (animate) flyIn(host, 4);
}

/* ── Suche im Gespräch ─────────────────────────────────────────────── */

function clearMarks() {
  const page = $('#page');
  if (!page) return;
  page.querySelectorAll('mark').forEach((m) => m.replaceWith(document.createTextNode(m.textContent)));
  page.normalize();
}

function highlight(q) {
  clearMarks();
  const cnt = $('#findCount');
  if (!q || q.length < 2) {
    if (cnt) cnt.textContent = '0';
    S.hits = [];
    S.hitIdx = -1;
    return;
  }

  const page = $('#page');
  if (!page) return;

  const re = new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');

  /* Nur das Gespräch: Anfrage und Fließtext. Notizen, Werkzeugausgaben,
     Code und Anhänge bleiben außen vor. */
  const roots = $$('.req, .prose', page).filter((el) => !el.closest('.notes, pre, code'));
  const nodes = [];
  for (const root of roots) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) nodes.push(walker.currentNode);
  }

  const hits = [];
  for (const node of nodes) {
    if (!node.nodeValue || node.parentElement.closest('.notes, pre, code, .copy, .att')) continue;
    if (!re.test(node.nodeValue)) continue;
    re.lastIndex = 0;

    const frag = document.createDocumentFragment();
    let last = 0;
    let m;
    while ((m = re.exec(node.nodeValue))) {
      frag.appendChild(document.createTextNode(node.nodeValue.slice(last, m.index)));
      const mk = document.createElement('mark');
      mk.textContent = m[0];
      frag.appendChild(mk);
      hits.push(mk);
      last = m.index + m[0].length;
      if (!m[0].length) break;
    }
    frag.appendChild(document.createTextNode(node.nodeValue.slice(last)));
    node.replaceWith(frag);
  }

  S.hits = hits;
  S.hitIdx = -1;
  if (cnt) cnt.textContent = String(hits.length);
  if (hits.length) nextHit();
}

function nextHit() {
  if (!S.hits.length) return;
  if (S.hitIdx >= 0 && S.hits[S.hitIdx]) S.hits[S.hitIdx].classList.remove('cur');
  S.hitIdx = (S.hitIdx + 1) % S.hits.length;
  const el = S.hits[S.hitIdx];
  el.classList.add('cur');
  el.scrollIntoView({ block: 'center', behavior: 'smooth' });
  const cnt = $('#findCount');
  if (cnt) cnt.textContent = `${S.hitIdx + 1} / ${S.hits.length}`;
}

/* ── Gespräch wechseln: Punkt halten, dann ziehen ──────────────────── */

function chapterStep(dir) {
  if (document.body.dataset.view !== 'reading') return;
  const n = S.sessions.length;
  if (!n) return;
  const i = S.sessions.findIndex((s) => s.id === S.cur?.id);
  if (i < 0) return;
  const j = i + dir;
  if (j < 0 || j >= n) { toast(dir > 0 ? 'Letztes Gespräch' : 'Erstes Gespräch'); return; }

  const page = $('#page');
  page.classList.remove('swap');
  void page.offsetWidth;
  page.classList.add('swap');
  setTimeout(() => page.classList.remove('swap'), 320);
  openSession(S.sessions[j].id);
}

const SCRUB = { active: false, from: 0, index: 0, timer: null, startX: 0, press: null };

function scrubBegin() {
  const i = S.sessions.findIndex((s) => s.id === S.cur?.id);
  if (i < 0) return;
  SCRUB.active = true;
  SCRUB.from = i;
  SCRUB.index = i;
  document.body.classList.add('scrubbing');
  setStatus('Gespräch wischen', 'warn');
  navigator.vibrate?.(10);
}

function scrubTo(i) {
  const n = S.sessions.length;
  i = Math.max(0, Math.min(n - 1, i));
  if (i === SCRUB.index) return;
  SCRUB.index = i;
  const s = S.sessions[i];
  const kep = $('#kep');
  if (kep) kep.innerHTML = `Gespräch <b>${i + 1}</b> von ${n}`;
  const pg = $('#pgnum');
  if (pg) pg.textContent = trim(s.title || s.preview || 'ohne Titel', 30);
  navigator.vibrate?.(4);
  clearTimeout(SCRUB.timer);
  SCRUB.timer = setTimeout(() => prefetch(s.id), 110);
}

function prefetch(id) {
  if (CACHE.has(id)) return;
  invoke('messages_load', { sessionId: id })
    .then((turns) => {
      CACHE.set(id, turns);
      if (CACHE.size > CACHE_MAX) CACHE.delete(CACHE.keys().next().value);
    })
    .catch(() => {});
}

function scrubEnd() {
  if (!SCRUB.active) return;
  SCRUB.active = false;
  document.body.classList.remove('scrubbing');
  clearTimeout(SCRUB.timer);
  const target = S.sessions[SCRUB.index];
  if (target && SCRUB.index !== SCRUB.from) openSession(target.id);
  else { updateHeader(); setStatus('bereit'); }
}

function bindChapterSwipe() {
  const zones = [$('#dots'), $('#pgnum')].filter(Boolean);
  const STEP = 34;

  const down = (e) => {
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    SCRUB.startX = e.clientX;
    clearTimeout(SCRUB.press);
    SCRUB.press = setTimeout(scrubBegin, 150);
  };

  const move = (e) => {
    if (!SCRUB.active) {
      if (SCRUB.press && Math.abs(e.clientX - SCRUB.startX) > 10) {
        clearTimeout(SCRUB.press);
        SCRUB.press = null;
      }
      return;
    }
    const dx = e.clientX - SCRUB.startX;
    if (Math.abs(dx) >= STEP) {
      const steps = Math.trunc(dx / STEP);
      SCRUB.startX += steps * STEP;
      scrubTo(SCRUB.index - steps);
    }
    e.preventDefault();
  };

  const up = () => {
    if (SCRUB.press) { clearTimeout(SCRUB.press); SCRUB.press = null; }
    scrubEnd();
  };

  for (const z of zones) z.addEventListener('pointerdown', down);
  window.addEventListener('pointermove', move, { passive: false });
  window.addEventListener('pointerup', up);
  window.addEventListener('pointercancel', up);
  window.addEventListener('blur', up);
}

/* ── Einrichtung ──────────────────────────────────────────────────── */

function setupState(text, cls = '') {
  const el = $('#setState');
  if (!el) return;
  el.textContent = text;
  el.className = 'state ' + cls;
}

function fillTokenHint(view) {
  const el = $('#setToken');
  if (!el) return;
  const has = !!(view && view.has_token);
  el.placeholder = has ? `gespeichert · ${view.token_hint || 'Schlüssel'}` : 'Schlüssel eintragen';
}

function applySettings(view) {
  const avatar = $('#avatar');
  if (avatar) avatar.classList.toggle('off', !(view && view.has_token));
}

async function openSetup() {
  let view = null;
  try { view = await invoke('settings_get'); } catch { /* egal */ }

  if (view) {
    const server = $('#setServer');
    if (server) server.value = view.server_url || '';
    const token = $('#setToken');
    if (token) token.value = '';
    fillTokenHint(view);
    applySettings(view);
    setupState(view.has_token ? 'Zugang hinterlegt.' : 'Noch kein Zugang hinterlegt.');
  } else {
    setupState('Einrichtung nicht lesbar.', 'bad');
  }

  showView('setup');
  setTimeout(() => { const el = $('#setServer'); if (el) el.focus(); }, 80);
}

/* Schließen führt zurück zu der Ansicht, aus der die Einrichtung geöffnet wurde. */
function closeSetup() {
  if (document.body.dataset.view !== 'setup') return;
  let back = S.setupBack && S.setupBack.view !== 'setup' ? S.setupBack.view : 'index';
  if (back === 'reading' && !S.cur && !S.draft) back = 'index';
  const nav = S.setupBack ? S.setupBack.nav : null;
  S.setupBack = null;
  showView(back, nav);
  /* Zurück in der Gesprächsansicht wieder dorthin, wo man war. */
  if (back === 'reading' && S.cur) requestAnimationFrame(() => restoreScroll(S.cur.id));
}

async function setupSave() {
  const server = $('#setServer').value.trim();
  const token = $('#setToken').value.trim();
  const btn = $('#setSave');
  btn.disabled = true;
  setupState('speichere …');

  try {
    const view = await invoke('settings_save', { serverUrl: server, token: token || null });
    applySettings(view);
    $('#setToken').value = '';
    fillTokenHint(view);
    setupState('gespeichert.', 'ok');

    const ok = await loadSessions(true);
    closeSetup();
    if (!ok) toast('Gespeichert, aber keine Verbindung: ' + (S.err || 'Server nicht erreichbar'), true);
  } catch (err) {
    setupState(errText(err), 'bad');
  } finally {
    btn.disabled = false;
  }
}

async function setupTest() {
  const btn = $('#setTest');
  btn.disabled = true;
  setupState('prüfe …');
  try {
    const msg = await invoke('settings_test');
    setupState(msg || 'Verbindung steht.', 'ok');
  } catch (err) {
    setupState(errText(err), 'bad');
  } finally {
    btn.disabled = false;
  }
}

/* ── Ansichten und Leiste ─────────────────────────────────────────── */

const NAV = [
  { id: 'reading', icon: 'menu_book', label: 'Session' },
  { id: 'new', icon: 'add', label: 'Neue Session' },
  { id: 'index', icon: 'list', label: 'Übersicht' },
  { id: 'approvals', icon: 'verified', label: 'Freigaben' },
  { id: 'setup', icon: 'settings', label: 'Einstellungen' },
];

function buildShell() {
  const host = $('#shell');
  if (!host) return;
  host.innerHTML = '<span class="navpill" id="navpill"></span>' + NAV.map((n) =>
    `<button data-nav="${n.id}" aria-label="${esc(n.label)}">` +
    `<i class="ni ico-${n.icon}"></i><span>${esc(n.label)}</span></button>`
  ).join('');
}

/* Die Leiste liest nur noch S.navActive — kein Raten aus der Ansicht. */
function markNav() {
  $$('#shell button').forEach((b) => {
    const on = b.dataset.nav === S.navActive;
    b.classList.toggle('on', on);
    if (on) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
  placeNavPill();
}

/* Der Scrollbalken des Dokuments (scrollbar-gutter) verkleinert die Seite,
   die feste Eingabeleiste sitzt aber am Viewport — ohne Ausgleich stünde sie
   ein paar Pixel weiter außen als der Text. Die Differenz wandert als
   `--sbw` in das CSS, damit beide dieselbe Kante haben. */
function measureGutter() {
  const b = document.body;
  const w = Math.max(0, Math.round(b.offsetWidth - b.clientWidth));
  document.documentElement.style.setProperty('--sbw', w + 'px');
}

/* Das Feld hinter dem aktiven Eintrag gleitet mit einer weichen Kurve nach.
   Beim ersten Aufbau springt es ohne Übergang an seinen Platz. */
let pillSettled = false;
function placeNavPill() {
  const host = $('#shell');
  const pill = $('#navpill');
  if (!host || !pill) return;
  const on = host.querySelector('button.on');
  if (!on || !on.offsetWidth) {
    pill.style.opacity = '0';
    return;
  }
  const inset = 6;
  pill.style.width = Math.max(0, on.offsetWidth - inset * 2) + 'px';
  pill.style.transform = `translate3d(${on.offsetLeft + inset}px,-50%,0)`;
  if (!pillSettled) {
    pill.classList.add('jump');
    host.classList.add('ready');
    requestAnimationFrame(() => requestAnimationFrame(() => {
      pill.classList.remove('jump');
      pillSettled = true;
    }));
  }
}

function showView(name, nav) {
  const prev = document.body.dataset.view;
  if (prev === 'reading' && name !== 'reading') saveScroll();

  /* Für das Schließen der Einrichtung merken, woher sie kam. */
  if (name === 'setup' && prev !== 'setup') {
    S.setupBack = { view: prev, nav: S.navActive };
  }

  document.body.dataset.view = name;

  /* „Schreiben" und „Neues Gespräch" bleiben hervorgehoben, solange sie der
     Grund für die Gesprächsansicht sind. Jeder andere Weg setzt auf die Ansicht zurück. */
  if (name === 'reading' && nav === 'new') S.navActive = nav;
  else S.navActive = name;

  markNav();

  /* Die Suche gehört zur Gesprächsansicht und schließt mit ihr. */
  if (name !== 'reading' && document.body.classList.contains('find-open')) {
    document.body.classList.remove('find-open');
    clearMarks();
  }

  if (name === 'index') {
    writeStore('aesmyra.open', '');
    renderIndex(prev !== 'index');
    scrollToY(0);
  } else if (name === 'approvals') {
    renderApprovals(prev !== 'approvals');
    scrollToY(0);
  } else if (name === 'reading') {
    updateHeader();
    grow();
  } else if (name === 'setup') {
    scrollToY(0);
  }
  /* Der Rand gehört zur offenen Ansicht: beim Wechsel neu entscheiden. */
  syncBusy();
}

function showReading() {
  if (S.draft && !S.cur) {
    showView('reading');
    if (!$('#page').childElementCount) renderChapter();
    return;
  }

  if (!S.cur) {
    const id = S.lastId || localStorage.getItem('aesmyra.last');
    const target = S.sessions.find((s) => s.id === id) || S.sessions[0];
    if (!target) { toast('Noch kein Gespräch'); showView('index'); return; }
    openSession(target.id);
    return;
  }

  showView('reading');
  if (!$('#page').childElementCount) renderChapter();
  /* Nach dem Zurückkommen Randlicht und Betriebsanzeige wieder aufnehmen. */
  if (syncBusy()) { rearmGlow(); setStatus('arbeitet …', 'warn'); }
  requestAnimationFrame(() => restoreScroll(S.cur.id));
}

/* ── Verdrahtung ──────────────────────────────────────────────────── */

let scrollTimer = null;

function grow() {
  const i = $('#input');
  if (!i) return;
  /* Im versteckten Zustand ist scrollHeight 0 — dann nicht messen. */
  if (i.offsetParent !== null) {
    i.style.height = 'auto';
    i.style.height = Math.min(i.scrollHeight, 150) + 'px';
  }
  const b = $('#sendBtn');
  if (b) b.disabled = streamingHere() ? false : !i.value.trim();
}

function bindIndex() {
  const list = $('#idxList');
  if (!list) return;
  let press = null;

  const cancel = () => {
    if (press) { clearTimeout(press.timer); press = null; }
  };

  list.addEventListener('pointerdown', (e) => {
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    const row = e.target.closest('.ch');
    if (!row) return;
    cancel();
    press = {
      id: row.dataset.id,
      x: e.clientX,
      y: e.clientY,
      timer: setTimeout(() => {
        press = null;
        S.suppress = Date.now() + 700;
        navigator.vibrate?.(12);
        openSheet(row.dataset.id);
      }, 520),
    };
  });

  window.addEventListener('pointermove', (e) => {
    if (!press) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) > 10) cancel();
  });
  window.addEventListener('pointerup', cancel);
  window.addEventListener('pointercancel', cancel);

  list.addEventListener('click', (e) => {
    if (Date.now() < (S.suppress || 0)) { e.preventDefault(); e.stopPropagation(); return; }
    const row = e.target.closest('.ch');
    if (row) openSession(row.dataset.id);
  });

  list.addEventListener('contextmenu', (e) => {
    const row = e.target.closest('.ch');
    if (!row) return;
    e.preventDefault();
    S.suppress = Date.now() + 400;
    openSheet(row.dataset.id);
  });
}

function bind() {
  bindIndex();

  $('#idxNew').addEventListener('click', newChapter);
  $('#avatar').addEventListener('click', openSetup);

  $('#shell').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-nav]');
    if (!b) return;
    switch (b.dataset.nav) {
      case 'reading': showReading(); break;
      case 'index': showView('index'); break;
      case 'approvals': showView('approvals'); break;
      case 'new': newChapter(); break;
      case 'setup': openSetup(); break;
    }
  });

  $('#setupClose').addEventListener('click', closeSetup);
  $('#setTheme').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-theme-choice]');
    if (b) setLook('theme', b.dataset.themeChoice);
  });
  $('#setAccent').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-accent-choice]');
    if (b) setLook('accent', b.dataset.accentChoice);
  });
  $('#setSave').addEventListener('click', setupSave);
  $('#setTest').addEventListener('click', setupTest);
  $('#setToken').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); setupSave(); }
  });

  $('#composer').addEventListener('submit', (e) => {
    e.preventDefault();
    /* Stoppen und Nachschieben gilt nur für den Lauf dieses Gesprächs. */
    const r = activeRun();
    if (r && r.stream) {
      const t = $('#input').value.trim();
      if (t) steer(t); else abortStream();
      return;
    }
    send($('#input').value);
  });

  $('#input').addEventListener('input', grow);
  $('#input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      $('#composer').requestSubmit();
    }
  });

  $('#findBtn').addEventListener('click', () => {
    const open = document.body.classList.toggle('find-open');
    if (open) $('#findInput').focus();
    else clearMarks();
  });
  $('#findClose').addEventListener('click', () => {
    document.body.classList.remove('find-open');
    clearMarks();
  });
  $('#findInput').addEventListener('input', (e) => highlight(e.target.value.trim()));
  $('#findInput').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); nextHit(); }
  });

  $('#page').addEventListener('click', (e) => {
    const link = e.target.closest('a[data-href]');
    if (link) {
      e.preventDefault();
      invoke('open_external', { url: link.dataset.href })
        .catch(() => toast('Link nicht geöffnet', true));
      return;
    }
    const fn = e.target.closest('.fn');
    if (fn) {
      const target = document.getElementById(fn.dataset.target);
      if (target) {
        target.open = true;
        target.scrollIntoView({ block: 'center', behavior: 'smooth' });
        target.style.transition = 'background .4s';
        target.style.background = 'rgba(167,139,250,.12)';
        setTimeout(() => { target.style.background = ''; }, 1200);
      }
    }
  });

  $('#sheet').addEventListener('click', (e) => {
    const b = e.target.closest('[data-act]');
    if (!b) {
      if (e.target.id === 'sheet') closeSheet();
      return;
    }
    const act = b.dataset.act;
    if (act === 'close') closeSheet();
    else sheetAction(act);
  });

  $('#apprList').addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-choice]');
    if (!b) return;
    const row = b.closest('.ap');
    const entry = row ? S.approvals.find((a) => a.request_id === row.dataset.rid) : null;
    if (!entry) return;

    row.querySelectorAll('button').forEach((x) => { x.disabled = true; });
    try {
      await decideApproval(entry, b.dataset.choice);
      renderApprovals();
    } catch (err) {
      row.querySelectorAll('button').forEach((x) => { x.disabled = false; });
      toast('Freigabe fehlgeschlagen: ' + errText(err), true);
    }
  });

  bindChapterSwipe();

  /* Capture: scrollen kann das Fenster oder der Körper — beides einsammeln. */
  document.addEventListener('scroll', () => {
    if (document.body.dataset.view !== 'reading') return;
    clearTimeout(scrollTimer);
    scrollTimer = setTimeout(saveScroll, 350);
  }, true);

  window.addEventListener('keydown', (e) => {
    const editable = e.target instanceof HTMLElement
      && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA');

    if ((e.ctrlKey || e.metaKey) && e.key === ',') {
      e.preventDefault();
      openSetup();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault();
      closeSetup();
      closeSheet();
      showView('index');
      return;
    }
    if (e.key === 'Escape') {
      closeSetup();
      closeSheet();
      if (document.body.classList.contains('find-open')) {
        document.body.classList.remove('find-open');
        clearMarks();
      }
      return;
    }
    if (e.altKey && !editable && e.key === 'ArrowRight') { e.preventDefault(); chapterStep(1); }
    if (e.altKey && !editable && e.key === 'ArrowLeft') { e.preventDefault(); chapterStep(-1); }
  });

  window.addEventListener('beforeunload', saveScroll);
  window.addEventListener('resize', () => { measureGutter(); placeNavPill(); });

  /* Zurück aus dem Hintergrund: rAF und die Rand-Animationen ruhen dort.
     Ohne diesen Anstoß bliebe der laufende Text auf dem Stand von vorhin
     und das Randlicht stumm. */
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState !== 'visible') return;
    const live = liveHere();
    if (live) paintLive(live, true);
    if (syncBusy()) rearmGlow();
  });
  window.addEventListener('pageshow', () => {
    const live = liveHere();
    if (live) paintLive(live, true);
    if (syncBusy()) rearmGlow();
  });
}

/* ── Start ────────────────────────────────────────────────────────── */

async function boot() {
  if (!CORE) {
    setStatus('Hülle fehlt', 'err');
    toast('Tauri-Hülle fehlt', true);
    return;
  }

  bind();

  try {
    const info = await invoke('app_info');
    document.body.dataset.platform = info.platform === 'android' ? 'mobile' : 'pc';
    const el = $('#setInfo');
    if (el) {
      const plat = PLAT[info.platform] || info.platform || '';
      el.textContent = `Aesmyra. ${info.version} · ${plat}/${info.arch}`;
    }
  } catch {
    document.body.dataset.platform = 'pc';
  }

  buildShell();
  applyLook();
  measureGutter();

  /* index.html setzt bei Avatar und Neu-Zeile nur die Symbolklasse;
     die Maskenbasis `ni` fehlt dort — hier ergänzen. */
  $$('#avatar i, #idxNew i').forEach((el) => el.classList.add('ni'));

  let view = null;
  try { view = await invoke('settings_get'); } catch { /* egal */ }
  applySettings(view);

  const ok = await loadSessions(true);

  const last = localStorage.getItem('aesmyra.last');
  if (last && S.sessions.some((s) => s.id === last)) S.lastId = last;

  /* Nach einem Neuladen (Live-Reload, Absturz) dort weitermachen, wo man war:
     die zuletzt geöffnete Sitzung wieder aufschlagen, statt in der Übersicht
     zu landen. Nur wenn sie noch existiert und der Server erreichbar ist. */
  const offen = localStorage.getItem('aesmyra.open');
  const zurueck = (ok && view && view.has_token && offen
    && S.sessions.some((s) => s.id === offen)) ? offen : null;
  if (zurueck) await openSession(zurueck);
  else showView('index');

  if (!ok || !view || !view.has_token) {
    await openSetup();
    if (!ok) setupState('Keine Verbindung: ' + (S.err || 'Server nicht erreichbar'), 'bad');
  }

  grow();

  /* Der erste Aufbau bleibt still; erst danach wird Bewegung zugeschaltet.
     Dann fliegen die bereits gezeichneten Zeilen sichtbar ein — sonst waere
     die Staffelung vorbei, bevor das Fenster sie zeigt. */
  requestAnimationFrame(() => requestAnimationFrame(() => {
    document.body.classList.remove('boot');
    if (document.body.dataset.view === 'index') {
      const host = $('#idxList');
      if (host && host.children.length) {
        host.classList.add('enter');
        clearTimeout(renderIndex._t);
        renderIndex._t = setTimeout(() => host.classList.remove('enter'), 1400);
      }
    } else if (document.body.dataset.view === 'reading') {
      flyIn($('#page'), 3);
    }
  }));
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
