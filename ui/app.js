'use strict';

/* ═══════════════════════════════════════════════════════════════════
   Blatt — Client für Hermes (Rust-Kern, Tauri-Hülle)

   Drei Flächen: Verzeichnis, Leseseite, Freigaben. Der Kern liefert
   Sitzungen, Verlauf und Ereignisse fertig; hier wird nur gezeichnet.
   ═══════════════════════════════════════════════════════════════════ */

const CORE = (window.__TAURI__ && window.__TAURI__.core) || null;
const invoke = (cmd, args) => (CORE ? CORE.invoke(cmd, args) : Promise.reject('Tauri-Hülle fehlt'));
const Channel = CORE ? CORE.Channel : null;

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

/* ── Zustand ──────────────────────────────────────────────────────── */

const S = {
  sessions: [],
  cur: null,
  turns: [],
  streaming: false,
  runId: null,
  /* Lauf-Generation: jedes neue Ereignis-Abo zählt hoch, alte werden verworfen. */
  gen: 0,
  /* Ein neues Kapitel ist erst beim ersten Senden eine Sitzung auf dem Server. */
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
  suppress: 0,
  err: '',
  restores: readStore('blatt.scroll', {}),
};

/* Zwischenspeicher: zwölf Kapitel reichen für flüssiges Wischen. */
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

const ROMAN = [[1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'], [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'], [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I']];

function roman(n) {
  n = Math.floor(Number(n) || 0);
  if (n <= 0) return 'I';
  if (n >= 4000) return String(n);
  let out = '';
  for (const [v, s] of ROMAN) {
    while (n >= v) { out += s; n -= v; }
  }
  return out;
}

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

/* ── Markdown, knapp und ehrlich ──────────────────────────────────── */

function inline(t) {
  return t
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[\s(])\*([^*\n]+)\*/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" data-href="$2">$1</a>');
}

function fmtUser(text) {
  return esc(text || '')
    .split('\n')
    .map((line) => (/^(\[User attached|@image:)/.test(line.trim()) ? `<span class="att">${line}</span>` : line))
    .join('\n');
}

function md(src) {
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

  const flushP = () => { if (buf.length) { out.push(`<p>${inline(buf.join(' '))}</p>`); buf = []; } };
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

/* ── Werkzeuge als Randnotizen ────────────────────────────────────── */

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

/* Fußnotenzeichen hinter den letzten Satz setzen; ohne Absatz anhängen. */
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

  const req = turn.user !== null && turn.user !== undefined ? `<div class="req">${fmtUser(turn.user)}</div>` : '';

  return `<section class="sec">
    <div class="col">${req}<div class="prose">${prose}</div></div>
    <aside class="notes">${notes.join('')}</aside>
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

function renderChapter() {
  const page = $('#page');
  if (!page) return;
  S.seq = 0;
  S.hits = [];
  S.hitIdx = -1;

  const title = S.cur ? (S.cur.title || S.cur.preview || 'Neues Kapitel') : 'Neues Kapitel';
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
  updateHeader();

  const q = $('#findInput') ? $('#findInput').value.trim() : '';
  if (document.body.classList.contains('find-open') && q) highlight(q);
}

function updateHeader() {
  const n = S.sessions.length;
  const idx = S.cur ? S.sessions.findIndex((s) => s.id === S.cur.id) : -1;
  const kep = $('#kep');
  if (kep) {
    kep.innerHTML = idx >= 0
      ? `Kapitel <b>${roman(idx + 1)}</b> von ${roman(n)}`
      : `Blatt · <b>${n} Kapitel</b>`;
  }
  const pg = $('#pgnum');
  if (pg) pg.textContent = idx >= 0 ? `Blatt ${idx + 1}/${n}` : `${n} Kapitel`;
  renderDots(idx);
}

function renderDots(idx) {
  const host = $('#dots');
  if (!host) return;
  const n = S.sessions.length;
  if (idx < 0 || !n) { host.innerHTML = ''; return; }

  const span = 9;
  const from = Math.max(0, Math.min(idx - 4, n - span));
  const frag = document.createDocumentFragment();
  for (let i = from; i < Math.min(n, from + span); i++) {
    const d = document.createElement('i');
    if (i === idx) d.className = 'on';
    d.title = S.sessions[i].title || '';
    d.addEventListener('click', () => openSession(S.sessions[i].id));
    frag.appendChild(d);
  }
  host.innerHTML = '';
  host.appendChild(frag);
}

/* ── Verzeichnis ──────────────────────────────────────────────────── */

function updatePageNo() {
  const el = $('#pageNo');
  if (!el) return;
  const id = S.lastId || (S.cur && S.cur.id);
  const i = id ? S.sessions.findIndex((s) => s.id === id) : -1;
  el.textContent = 'S. ' + pad3(i >= 0 ? i + 1 : 1);
}

function chapterRow(s, i) {
  const num = `<span class="ch-num">${roman(i + 1)}${s.pinned ? '<i class="dot"></i>' : ''}</span>`;
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
    : '<div class="empty">Noch kein Kapitel.</div>';

  /* Zeilen laufen gestaffelt ein, aber nur beim Wechsel ins Verzeichnis. */
  host.classList.remove('enter');
  if (animate && S.sessions.length) {
    void host.offsetWidth;
    host.classList.add('enter');
    clearTimeout(renderIndex._t);
    renderIndex._t = setTimeout(() => host.classList.remove('enter'), 600);
  }
  updatePageNo();
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

/* ── Aktionsblatt ─────────────────────────────────────────────────── */

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
      if (!confirm('Kapitel wirklich löschen?')) return;
      if (S.streaming && S.cur && S.cur.id === id) await abortStream();
      await invoke('session_delete', { sessionId: id });
      CACHE.delete(id);
      if (S.lastId === id) { S.lastId = null; writeStore('blatt.last', ''); }
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

/* „Neues Kapitel" legt nur eine leere Seite an. Die Sitzung auf dem Server
   entsteht erst beim ersten Senden — wer die Seite verlässt, hinterlässt nichts. */
async function newChapter() {
  if (S.streaming) {
    if (!confirm('Es läuft noch eine Antwort. Trotzdem ein neues Kapitel?')) return;
    await abortStream();
  }

  S.cur = null;
  S.draft = true;
  S.turns = [];
  S.loading = false;

  showView('reading', 'new');
  renderChapter();
  scrollToY(0);
  setStatus('bereit');
  setTimeout(() => { const i = $('#input'); if (i) i.focus(); }, 80);
}

/* Erste Sitzung eines Entwurfs anlegen; danach ist es ein gewöhnliches Kapitel.
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
    S.lastId = s.id;
    writeStore('blatt.last', s.id);
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
  if (!target) return;

  if (S.streaming && S.cur && S.cur.id !== id) {
    if (!confirm('Es läuft noch eine Antwort. Trotzdem wechseln?')) return;
    await abortStream();
  }

  if (S.cur?.id === id && document.body.dataset.view === 'reading') {
    S.draft = false;
    showReading();
    return;
  }

  if (S.cur && S.cur.id !== id) saveScroll();

  S.draft = false;
  S.cur = target;
  S.lastId = id;
  writeStore('blatt.last', id);
  updatePageNo();
  showView('reading');

  const cached = CACHE.get(id);
  if (cached) {
    S.turns = cached;
    S.loading = false;
    renderChapter();
    requestAnimationFrame(() => restoreScroll(id));
    setStatus('bereit');
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
      renderChapter();
      requestAnimationFrame(() => restoreScroll(id));
    } else if (turns.length !== S.turns.length) {
      const y = scrollTopNow();
      S.turns = turns;
      renderChapter();
      scrollToY(y);
    } else {
      S.turns = turns;
    }
    setStatus('bereit');
  } catch (err) {
    S.loading = false;
    if (!cached) {
      S.turns = [];
      renderChapter();
      setStatus('Fehler', 'err');
      toast('Kapitel konnte nicht geladen werden: ' + errText(err), true);
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
async function reloadMessages() {
  if (!S.cur) return;
  const id = S.cur.id;
  const before = scrollTopNow();
  const nearBottom = before + window.innerHeight > document.body.scrollHeight - 240;

  try {
    const turns = await invoke('messages_load', { sessionId: id });
    if (!S.cur || S.cur.id !== id) return;
    CACHE.set(id, turns);
    S.turns = turns;
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
  writeStore('blatt.scroll', S.restores);
}

function restoreScroll(id) {
  const y = S.restores[id];
  if (typeof y === 'number') scrollToY(y);
  else scrollBottom();
}

/* ── Senden und Strom ─────────────────────────────────────────────── */

function createLiveSection() {
  const sec = document.createElement('section');
  sec.className = 'sec fresh';
  sec.innerHTML = `<div class="col"><div class="prose"></div>
    <div class="writing"><i></i><i></i><i></i><i></i><i></i><span class="wtext">ich schreibe …</span></div></div>
    <aside class="notes"></aside>`;
  $('#page').appendChild(sec);
  return {
    sec,
    prose: sec.querySelector('.prose'),
    notes: sec.querySelector('.notes'),
    writing: sec.querySelector('.writing'),
    text: '',
    tools: new Map(),
    dirty: false,
  };
}

function paintLive() {
  if (!S.live || S.live.dirty) return;
  S.live.dirty = true;
  requestAnimationFrame(() => {
    if (!S.live) return;
    S.live.prose.innerHTML = md(S.live.text);
    S.live.dirty = false;
  });
}

async function send(text) {
  text = String(text || '').trim();
  if (!text) return;

  if (S.streaming) { await steer(text); return; }

  /* Erste Zeile eines neuen Kapitels: Sitzung erst jetzt anlegen. */
  if (!S.cur) {
    try {
      await ensureSession();
    } catch (err) {
      toast('Neues Kapitel fehlgeschlagen: ' + errText(err), true);
      return;
    }
    S.turns = [];
    /* Mit der Sitzung ist das Kapitel der Grund der Ansicht, nicht der Knopf. */
    S.navActive = 'reading';
    markNav();
  }

  const input = $('#input');
  input.value = '';
  grow();

  S.turns.push({ user: text, parts: [] });
  renderChapter();

  S.live = createLiveSection();
  scrollBottom(true);
  setBusy(true);
  setStatus('arbeitet …', 'warn');
  sendBtnBusy(true);

  await streamTurn(text);
}

async function streamTurn(text) {
  if (!Channel) {
    toast('Ereigniskanal fehlt', true);
    finishTurn(false);
    return;
  }

  const gen = ++S.gen;
  S.streaming = true;
  S.runId = null;

  const channel = new Channel();
  channel.onmessage = (event) => handleEvent(event, gen);

  try {
    await invoke('chat_start', { sessionId: S.cur.id, text, onEvent: channel });
  } catch (err) {
    if (gen !== S.gen) return;
    toast(errText(err), true);
    finishTurn(false);
  }
}

function handleEvent(ev, gen) {
  if (!ev || !ev.kind) return;

  /* Ereignisse eines abgebrochenen oder überholten Laufs verwerfen: sonst
     reißt ein spätes `finished` den neuen Lauf nieder. */
  if (gen !== S.gen) return;

  /* Zweites Kennzeichen, sobald der Kern `run_id` mitliefert. */
  if (ev.run_id && S.runId && ev.run_id !== S.runId && ev.kind !== 'run_started') return;

  switch (ev.kind) {
    case 'run_started':
      S.runId = ev.run_id;
      setStatus('arbeitet …', 'warn');
      break;
    case 'delta':
      if (S.live) { S.live.text += ev.text || ''; paintLive(); }
      break;
    case 'commentary':
      if (S.live && ev.text) {
        S.live.text += (S.live.text ? '\n\n' : '') + ev.text;
        paintLive();
      }
      break;
    case 'tool_started':
      addLiveTool(ev);
      break;
    case 'tool_finished':
      markLiveTool(ev);
      break;
    case 'approval':
      addApproval(ev);
      break;
    case 'completed':
      if (S.live) {
        if (ev.content) S.live.text = ev.content;
        paintLive();
        if (S.live.writing) S.live.writing.remove();
      }
      break;
    case 'failed':
      toast(ev.message || 'Lauf fehlgeschlagen', true);
      break;
    case 'cancelled':
      setStatus('abgebrochen', 'warn');
      break;
    case 'error':
      toast(String(ev.message || 'Fehler').slice(0, 160), true);
      break;
    case 'reconnecting':
      setStatus(`unterbrochen · Versuch ${ev.attempt}`, 'warn');
      break;
    case 'recovered':
      setStatus('läuft weiter …', 'warn');
      break;
    case 'stream_ended':
      setStatus('Verbindung beendet', 'warn');
      break;
    case 'finished':
      finishTurn(ev.ok !== false);
      break;
    default:
      break;
  }
}

function addLiveTool(p) {
  if (!S.live) return;
  const n = ++S.seq;
  const t = { name: p.name || 'werkzeug', args: p.args || '', out: '' };
  const wrap = document.createElement('div');
  wrap.innerHTML = toolNote(t, n);
  const el = wrap.firstElementChild;
  if (!el) return;
  el.querySelector('[data-state]').textContent = ' …';
  el.dataset.tool = t.name;
  S.live.notes.appendChild(el);
  S.live.tools.set(t.name + ':' + n, el);
}

function markLiveTool(p) {
  if (!S.live) return;
  const name = p.name || 'werkzeug';
  let found = null;
  for (const [k, el] of S.live.tools) {
    if (k.startsWith(name + ':') && !el.dataset.done) { found = el; break; }
  }
  if (!found) {
    addLiveTool(p);
    for (const [k, el] of S.live.tools) {
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

function finishTurn(ok = true) {
  if (!S.streaming && !S.live) return;
  const runId = S.runId;

  /* Generation schließen: was jetzt noch ankommt, gehört zu diesem Lauf. */
  S.gen++;
  S.streaming = false;
  S.runId = null;
  sendBtnBusy(false);
  setBusy(false);
  setStatus(ok ? 'bereit' : 'beendet', ok ? '' : 'warn');

  if (S.live && S.live.sec) S.live.sec.remove();
  S.live = null;

  /* Offene Freigaben eines beendeten Laufs sind erledigt; entschiedene bleiben. */
  if (runId) {
    S.approvals = S.approvals.filter((a) => a.run_id !== runId || a.decided);
    if (document.body.dataset.view === 'approvals') renderApprovals();
  }

  reloadMessages().then(() => loadSessions(true));
}

async function abortStream() {
  /* Erst die Generation schließen, dann stoppen — sonst kommen noch
     Ereignisse des alten Laufs durch. */
  S.gen++;

  try { await invoke('chat_stop'); } catch { /* egal */ }
  const runId = S.runId;

  S.streaming = false;
  S.runId = null;
  setBusy(false);
  sendBtnBusy(false);
  if (S.live && S.live.sec) S.live.sec.remove();
  S.live = null;

  if (runId) {
    S.approvals = S.approvals.filter((a) => a.run_id !== runId || a.decided);
    if (document.body.dataset.view === 'approvals') renderApprovals();
  }

  setStatus('abgebrochen', 'warn');
  await reloadMessages();
  loadSessions(true);
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

async function steer(text) {
  if (!S.runId) { toast('Lauf noch nicht bereit'); return; }
  try {
    await invoke('run_steer', { runId: S.runId, message: text });
    const input = $('#input');
    input.value = '';
    grow();
    toast('nachgeschoben');
    const w = S.live && S.live.writing ? S.live.writing.querySelector('.wtext') : null;
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
    run_id: p.run_id || S.runId || '',
    request_id: rid,
    command: p.command || '',
    choices: Array.isArray(p.choices) && p.choices.length ? p.choices : ['once', 'session', 'deny'],
    decided: false,
    choice: null,
    ts: Date.now(),
  };
  S.approvals.push(entry);
  return entry;
}

async function decideApproval(entry, choice) {
  await invoke('run_approve', {
    runId: entry.run_id || S.runId || '',
    choice,
    requestId: entry.request_id,
  });
  entry.decided = true;
  entry.choice = choice;
  if (document.body.dataset.view === 'approvals') renderApprovals();
}

function addApproval(p) {
  const entry = pushApproval(p);
  if (!S.live) return;

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
      el.querySelectorAll('button').forEach((x) => { x.disabled = false; });
      toast('Freigabe fehlgeschlagen: ' + errText(err), true);
    }
  });

  S.live.notes.appendChild(el);
}

function renderApprovals() {
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
      ? `<div class="row"><span class="tstate ok">entschieden · ${esc(LABELS[a.choice] || a.choice || '')}</span></div>`
      : `<div class="row">${a.choices.map((c) => `<button data-choice="${esc(c)}"${c === 'once' ? ' class="primary"' : ''}>${esc(LABELS[c] || c)}</button>`).join('')}</div>`;
    return `<div class="ap${a.decided ? ' done' : ''}" data-rid="${esc(a.request_id)}">
      <b>Freigabe nötig</b>
      <code>${esc(a.command || 'Werkzeugaufruf')}</code>${row}
    </div>`;
  }).join('');
}

/* ── Suche im Kapitel ─────────────────────────────────────────────── */

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

  /* Nur das Gespräch: Anfrage und Fließtext. Randnotizen, Werkzeugausgaben,
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

/* ── Kapitel wechseln: Punkt halten, dann ziehen ──────────────────── */

function chapterStep(dir) {
  if (document.body.dataset.view !== 'reading') return;
  const n = S.sessions.length;
  if (!n) return;
  const i = S.sessions.findIndex((s) => s.id === S.cur?.id);
  if (i < 0) return;
  const j = i + dir;
  if (j < 0 || j >= n) { toast(dir > 0 ? 'Letztes Kapitel' : 'Erstes Kapitel'); return; }

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
  setStatus('Kapitel wischen', 'warn');
  navigator.vibrate?.(10);
}

function scrubTo(i) {
  const n = S.sessions.length;
  i = Math.max(0, Math.min(n - 1, i));
  if (i === SCRUB.index) return;
  SCRUB.index = i;
  const s = S.sessions[i];
  const kep = $('#kep');
  if (kep) kep.innerHTML = `Kapitel <b>${roman(i + 1)}</b> von ${roman(n)}`;
  const pg = $('#pgnum');
  if (pg) pg.textContent = trim(s.title || s.preview || 'ohne Titel', 30);
  renderDots(i);
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
  /* Zurück auf der Leseseite wieder dorthin, wo man war. */
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
  { id: 'reading', icon: 'menu_book', label: 'Lesen' },
  { id: 'index', icon: 'list', label: 'Verzeichnis' },
  { id: 'write', icon: 'edit', label: 'Schreiben' },
  { id: 'approvals', icon: 'verified', label: 'Freigaben' },
  { id: 'new', icon: 'inbox', label: 'Neues Kapitel' },
  { id: 'setup', icon: 'settings', label: 'Einstellungen' },
];

function buildShell() {
  const host = $('#shell');
  if (!host) return;
  host.innerHTML = NAV.map((n) =>
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
}

function showView(name, nav) {
  const prev = document.body.dataset.view;
  if (prev === 'reading' && name !== 'reading') saveScroll();

  /* Für das Schließen der Einrichtung merken, woher sie kam. */
  if (name === 'setup' && prev !== 'setup') {
    S.setupBack = { view: prev, nav: S.navActive };
  }

  document.body.dataset.view = name;

  /* „Schreiben" und „Neues Kapitel" bleiben hervorgehoben, solange sie der
     Grund für die Leseseite sind. Jeder andere Weg setzt auf die Ansicht zurück. */
  if (name === 'reading' && (nav === 'write' || nav === 'new')) S.navActive = nav;
  else S.navActive = name;

  markNav();

  /* Die Suche gehört zur Leseseite und schließt mit ihr. */
  if (name !== 'reading' && document.body.classList.contains('find-open')) {
    document.body.classList.remove('find-open');
    clearMarks();
  }

  if (name === 'index') {
    renderIndex(prev !== 'index');
    scrollToY(0);
  } else if (name === 'approvals') {
    renderApprovals();
    scrollToY(0);
  } else if (name === 'reading') {
    updateHeader();
    grow();
  } else if (name === 'setup') {
    scrollToY(0);
  }
}

function showReading() {
  if (S.draft && !S.cur) {
    showView('reading');
    if (!$('#page').childElementCount) renderChapter();
    return;
  }

  if (!S.cur) {
    const id = S.lastId || localStorage.getItem('blatt.last');
    const target = S.sessions.find((s) => s.id === id) || S.sessions[0];
    if (!target) { toast('Noch kein Kapitel'); showView('index'); return; }
    openSession(target.id);
    return;
  }

  showView('reading');
  if (!$('#page').childElementCount) renderChapter();
  requestAnimationFrame(() => restoreScroll(S.cur.id));
}

async function focusComposer() {
  if (S.draft && !S.cur) {
    showView('reading', 'write');
  } else if (!S.cur) {
    if (!S.sessions.length) await newChapter();
    else await openSession(S.lastId || S.sessions[0].id);
    /* Der gedrückte Eintrag bleibt der Grund der Ansicht. */
    S.navActive = 'write';
    markNav();
  } else {
    showView('reading', 'write');
  }
  setTimeout(() => {
    if (document.body.dataset.view !== 'reading') return;
    const i = $('#input');
    if (i) i.focus();
  }, 100);
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
  if (b) b.disabled = S.streaming ? false : !i.value.trim();
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
      case 'write': focusComposer(); break;
      case 'approvals': showView('approvals'); break;
      case 'new': newChapter(); break;
      case 'setup': openSetup(); break;
    }
  });

  $('#setupClose').addEventListener('click', closeSetup);
  $('#setSave').addEventListener('click', setupSave);
  $('#setTest').addEventListener('click', setupTest);
  $('#setToken').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); setupSave(); }
  });

  $('#composer').addEventListener('submit', (e) => {
    e.preventDefault();
    if (S.streaming) {
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
      el.textContent = `Blatt ${info.version} · ${plat}/${info.arch}`;
    }
  } catch {
    document.body.dataset.platform = 'pc';
  }

  buildShell();

  /* index.html setzt bei Avatar und Neu-Zeile nur die Symbolklasse;
     die Maskenbasis `ni` fehlt dort — hier ergänzen. */
  $$('#avatar i, #idxNew i').forEach((el) => el.classList.add('ni'));

  let view = null;
  try { view = await invoke('settings_get'); } catch { /* egal */ }
  applySettings(view);

  const ok = await loadSessions(true);

  const last = localStorage.getItem('blatt.last');
  if (last && S.sessions.some((s) => s.id === last)) S.lastId = last;

  showView('index');

  if (!ok || !view || !view.has_token) {
    await openSetup();
    if (!ok) setupState('Keine Verbindung: ' + (S.err || 'Server nicht erreichbar'), 'bad');
  }

  grow();

  /* Der erste Aufbau bleibt still; erst danach wird Bewegung zugeschaltet. */
  requestAnimationFrame(() => requestAnimationFrame(() => document.body.classList.remove('boot')));
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
