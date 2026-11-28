/* Befehlsleiste: tippt man in der Eingabe ein „/“, erscheint die Liste der
   Hermes-Befehle. Ausführen kann diese Oberfläche nur, was sie selbst kennt
   (r-Feld in commands.js); alles andere wird ehrlich als „nur Konsole“ oder
   „nur Bot“ gekennzeichnet, statt es als Text ans Modell zu schicken.

   Die Datei hängt bewusst nicht in app.js: sie beobachtet Eingabe und
   Absenden von außen (Capture-Phase) und lässt alles andere unberührt. */

(() => {
  const CMDS = Array.isArray(window.HERMES_COMMANDS) ? window.HERMES_COMMANDS : [];
  if (!CMDS.length) return;

  const $ = (s) => document.querySelector(s);
  let pop = null;      // Wurzel des Popovers
  let list = null;     // Liste darin
  let offen = false;
  let treffer = [];    // sichtbare Befehle in Anzeigereihenfolge
  let zeiger = 0;      // hervorgehobener Eintrag

  const KURZ = { cli: 'Konsole', gw: 'Bot' };

  /* ── Aufbau ─────────────────────────────────────────────────────── */

  function baue() {
    pop = document.createElement('div');
    pop.className = 'slash-pop';
    pop.setAttribute('role', 'listbox');
    pop.setAttribute('aria-label', 'Hermes-Befehle');
    pop.innerHTML = '<div class="slash-head"><b>Befehle</b>'
      + '<span class="slash-tip">↑↓ wählen · Tab einsetzen · Esc schließen</span></div>'
      + '<div class="slash-list"></div>';
    list = pop.querySelector('.slash-list');
    document.body.appendChild(pop);
    pop.addEventListener('mousedown', (e) => e.preventDefault()); // Fokus im Feld halten
    pop.addEventListener('click', (e) => {
      const zeile = e.target.closest('[data-i]');
      if (!zeile) return;
      zeiger = Number(zeile.dataset.i);
      einsetzen();
    });
  }

  function setzePosition() {
    const feld = $('.composer .field');
    if (!feld || !pop) return;
    const r = feld.getBoundingClientRect();
    const breite = Math.min(Math.max(r.width, 320), 720);
    pop.style.left = Math.round(r.left + (r.width - breite) / 2) + 'px';
    pop.style.width = breite + 'px';
    pop.style.bottom = Math.round(window.innerHeight - r.top + 10) + 'px';
  }

  /* ── Suche ──────────────────────────────────────────────────────── */

  function rang(c, q) {
    if (!q) return c.r ? 0 : 1;
    const n = c.n;
    if (n === q) return 0;
    if (n.startsWith(q)) return 1;
    if ((c.a || []).some((a) => a.startsWith(q))) return 2;
    if (n.includes(q)) return 3;
    if ((c.a || []).some((a) => a.includes(q))) return 4;
    if ((c.d || '').toLowerCase().includes(q)) return 5;
    return null;
  }

  const GRUPPEN = ['Sitzung', 'Einstellungen', 'Auskunft', 'Werkzeuge',
    'Erweiterungen', 'Kontext', 'Hintergrund', 'Beenden'];

  function suche(q) {
    const sauber = String(q || '').toLowerCase().trim();
    /* Ohne Suchwort nach Gruppe sortieren, damit die Überschriften taugen. */
    if (!sauber) {
      const rangVon = (c) => {
        const i = GRUPPEN.indexOf(c.c);
        return i < 0 ? 99 : i;
      };
      return [...CMDS].sort((a, b) => rangVon(a) - rangVon(b) || a.n.localeCompare(b.n));
    }
    const rows = [];
    for (const c of CMDS) {
      const r = rang(c, sauber);
      if (r === null) continue;
      rows.push({ c, r });
    }
    rows.sort((x, y) => x.r - y.r || x.c.n.localeCompare(y.c.n));
    return rows.map((x) => x.c);
  }

  function zeichne(q) {
    treffer = suche(q);
    zeiger = Math.min(zeiger, Math.max(0, treffer.length - 1));
    if (!treffer.length) {
      list.innerHTML = '<div class="slash-leer">Kein Befehl gefunden.</div>';
      return;
    }
    let html = '';
    let letzteGruppe = null;
    treffer.forEach((c, i) => {
      if (!q && c.c !== letzteGruppe) {
        letzteGruppe = c.c;
        html += '<div class="slash-group">' + escapeHtml(c.c) + '</div>';
      }
      const kurz = c.k ? `<span class="slash-k">${KURZ[c.k]}</span>` : '';
      const hier = c.r ? '<span class="slash-ok">hier</span>' : '';
      html += `<button type="button" class="slash-row${i === zeiger ? ' on' : ''}"`
        + ` data-i="${i}" role="option" aria-selected="${i === zeiger}">`
        + `<span class="slash-name">/${escapeHtml(c.n)}</span>`
        + `<span class="slash-desc">${escapeHtml(c.d)}</span>`
        + `${hier}${kurz}</button>`;
    });
    list.innerHTML = html;
    const on = list.querySelector('.slash-row.on');
    if (on) on.scrollIntoView({ block: 'nearest' });
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"]/g, (m) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[m]));
  }

  /* ── Öffnen, Schließen, Einsetzen ───────────────────────────────── */

  function oeffne(q) {
    if (!pop) baue();
    zeiger = 0;
    zeichne(q);
    if (!offen) {
      offen = true;
      pop.classList.add('on');
      document.body.classList.add('slash-open');
      setzePosition();
    } else {
      setzePosition();
    }
  }

  function schliesse() {
    if (!offen) return;
    offen = false;
    if (pop) pop.classList.remove('on');
    document.body.classList.remove('slash-open');
  }

  function einsetzen() {
    const c = treffer[zeiger];
    if (!c) return;
    const feld = $('#input');
    if (!feld) return;
    const platz = c.g && !c.g.startsWith('[') ? ' ' : (c.g ? ' ' : '');
    feld.value = '/' + c.n + platz;
    feld.dispatchEvent(new Event('input', { bubbles: true }));
    schliesse();
    feld.focus();
    const ende = feld.value.length;
    try { feld.setSelectionRange(ende, ende); } catch { /* egal */ }
  }

  /* ── Ausführen ──────────────────────────────────────────────────── */

  /* ── Ausgabe echter Befehle ─────────────────────────────────────── */

  let out = null;

  function baueAusgabe() {
    out = document.createElement('div');
    out.className = 'cmdout';
    out.innerHTML = '<div class="cmdout-box" role="dialog" aria-modal="true" aria-label="Befehlsausgabe">'
      + '<div class="cmdout-head"><b class="cmdout-name"></b>'
      + '<button type="button" class="cmdout-x" aria-label="Schließen">Schließen</button></div>'
      + '<div class="cmdout-body prose"></div></div>';
    out.addEventListener('click', (e) => { if (e.target === out) schliesseAusgabe(); });
    out.querySelector('.cmdout-x').addEventListener('click', schliesseAusgabe);
    document.body.appendChild(out);
  }

  function zeigeAusgabe(name, text, fehler) {
    if (!out) baueAusgabe();
    out.querySelector('.cmdout-name').textContent = name;
    const body = out.querySelector('.cmdout-body');
    body.classList.toggle('fehler', !!fehler);
    body.innerHTML = fehler
      ? '<p class="cmdout-err">' + escapeHtml(text) + '</p>'
      : md(text, true);
    out.classList.add('on');
  }

  function schliesseAusgabe() {
    if (out) out.classList.remove('on');
  }

  function erklaere(err) {
    const s = String(err || '');
    if (s.includes('unsupported_command')) {
      return 'Dieser Befehl läuft nur in der Hermes-Konsole. Der Gateway führt ihn nicht aus.';
    }
    if (s.includes('no_active_session') || s.includes('not found')) {
      return 'Der Gateway kennt diese Sitzung nicht mehr. Öffne sie neu aus der Übersicht.';
    }
    return s || 'Der Befehl ist fehlgeschlagen.';
  }

  /* Zahl mit deutschen Trennzeichen, ohne Ballast. */
  function zahl(n) {
    return Number(n || 0).toLocaleString('de-DE');
  }

  function zeit(ts) {
    const n = Number(ts);
    if (!n) return '—';
    return new Date(n * 1000).toLocaleString('de-DE', {
      day: '2-digit', month: '2-digit', year: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  }

  /* Baut die Ausgabe aus den Daten, die der Server über seine HTTP-Schnittstelle
     liefert. Kein Umweg, keine fremde Konsole — dieselben Zähler. */
  function baueDaten(name, data) {
    const s = (data && (data.session || data)) || {};
    const tok = (k) => Number(s[k] || 0);

    if (name === 'status') {
      return [
        'Session ID: ' + (s.id || '—'),
        'Title: ' + (s.title || '—'),
        'Created: ' + zeit(s.started_at),
        'Last Activity: ' + zeit(s.ended_at || s.started_at),
        'Model: ' + (s.model || '—') + (s.source ? ' (' + s.source + ')' : ''),
        'Messages: ' + zahl(s.message_count),
        'Tokens: ' + zahl(tok('input_tokens') + tok('output_tokens')) +
          ' (in ' + zahl(tok('input_tokens')) + ', out ' + zahl(tok('output_tokens')) + ')',
        'API calls: ' + zahl(s.api_call_count),
        'Tools: ' + zahl(s.tool_call_count),
      ].join('\n');
    }

    if (name === 'context' || name === 'usage') {
      const zeilen = [
        'Input tokens: ' + zahl(tok('input_tokens')),
        'Output tokens: ' + zahl(tok('output_tokens')),
        'Cache read: ' + zahl(s.cache_read_tokens),
        'Cache write: ' + zahl(s.cache_write_tokens),
        'Reasoning: ' + zahl(s.reasoning_tokens),
        'Messages: ' + zahl(s.message_count),
      ];
      if (Number(s.estimated_cost_usd)) {
        zeilen.push('Estimated cost: $' + Number(s.estimated_cost_usd).toFixed(4));
      }
      return zeilen.join('\n');
    }

    if (name === 'model') {
      return 'Model: ' + (s.model || '—') + (s.source ? ' (' + s.source + ')' : '');
    }

    if (name === 'tools') {
      const arr = (data && (data.data || data)) || [];
      if (!Array.isArray(arr) || !arr.length) return 'Keine Werkzeuggruppen gemeldet.';
      return arr.map((x) =>
        '- ' + (x.name || '?') + ': ' + (x.description || '') +
        ' (' + ((x.tools || []).length) + ' Werkzeuge' + (x.enabled === false ? ', aus' : '') + ')'
      ).join('\n');
    }

    if (name === 'skills') {
      const arr = (data && (data.data || data.skills || data)) || [];
      if (!Array.isArray(arr) || !arr.length) return 'Keine Fähigkeiten gemeldet.';
      return arr.map((x) => '- ' + (x.name || '?') + ': ' + (x.description || '')).join('\n');
    }

    return JSON.stringify(data, null, 2).slice(0, 4000);
  }

  async function ausfuehren(c, arg) {
    if (!S.cur) {
      meldung('Erst eine Sitzung öffnen — Befehle brauchen ihren Kontext.', true);
      return true;
    }
    const befehl = '/' + c.n + (arg ? ' ' + arg : '');
    zeigeAusgabe(befehl, 'Läuft …');
    try {
      const r = await invoke('slash_run', { sessionId: S.cur.id, command: befehl });
      if (r && r.ok && r.raw && r.raw.message && (r.type === 'send' || r.type === 'skill')) {
        schliesseAusgabe();
        await send(r.raw.message);
        return true;
      }
      if (r && r.ok && r.type === 'data') {
        zeigeAusgabe(befehl, baueDaten(r.name, r.data));
        return true;
      }
      if (r && r.ok) {
        zeigeAusgabe(befehl, r.output || 'Der Befehl hat keine Ausgabe geliefert.');
        if (r.session_id) {
          // Neue Sitzung (etwa nach /compress). Der Befehl legt sie an und
          // führt dort weiter — also gleich dorthin wechseln, statt den
          // alten, jetzt archivierten Verlauf stehen zu lassen.
          await loadSessions(true);
          if (!S.sessions.some((s) => s.id === r.session_id)) await loadSessions(true);
          if (S.sessions.some((s) => s.id === r.session_id)) {
            if (!S.cur || S.cur.id !== r.session_id) await openSession(r.session_id);
            schliesseAusgabe();
            meldung('Neue Sitzung geöffnet');
          } else {
            meldung('Sitzung angelegt, im Verzeichnis auswählen: ' + r.session_id, true);
          }
        }
      } else {
        zeigeAusgabe(befehl, erklaere(r && r.error), true);
      }
    } catch (e) {
      zeigeAusgabe(befehl, 'Befehlsdienst nicht erreichbar.\n\n' + String(e && e.message ? e.message : e), true);
    }
    return true;
  }

  function leereFeld() {
    const f = $('#input');
    if (!f) return;
    f.value = '';
    f.dispatchEvent(new Event('input', { bubbles: true }));
    if (typeof grow === 'function') { try { grow(); } catch { /* egal */ } }
  }

  function meldung(text, fehler) {
    if (typeof toast === 'function') toast(text, !!fehler);
  }

  async function tue(c, arg) {
    try {
      switch (c.r) {
        case 'new':
          newChapter();
          return true;
        case 'sessions':
        case 'resume':
          showView('index');
          return true;
        case 'stop':
          if (S.streaming) await abortStream();
          else meldung('Es läuft gerade nichts.');
          return true;
        case 'title': {
          if (!S.cur) { meldung('Erst eine Sitzung beginnen.', true); return true; }
          const t = (arg || '').trim();
          if (!t) { meldung('Titel fehlt: /title Neuer Name', true); return true; }
          await invoke('session_patch', { sessionId: S.cur.id, title: t });
          S.cur.title = t;
          meldung('Titel gesetzt');
          return true;
        }
        case 'status':
        case 'compress':
        case 'help':
          return ausfuehren(c, arg);
        case 'steer':
          if (!arg) { meldung('Text fehlt: /steer …', true); return true; }
          await steer(arg);
          return true;
        case 'retry': {
          const letzte = [...(S.turns || [])].reverse().find((x) => x.user);
          if (!letzte) { meldung('Keine Nachricht zum Wiederholen.', true); return true; }
          await send(letzte.user);
          return true;
        }
        default:
          return ausfuehren(c, arg);
      }
    } catch (e) {
      meldung(String(e && e.message ? e.message : e), true);
      return true;
    }
  }

  function zerlege(text) {
    const m = /^\/([a-zA-Z0-9_-]+)(?:[\s]+([\s\S]*))?$/.exec(String(text || '').trim());
    if (!m) return null;
    const name = m[1].toLowerCase();
    const c = CMDS.find((x) => x.n === name || (x.a || []).includes(name));
    if (!c) return null;
    return { c, arg: (m[2] || '').trim() };
  }

  /* ── Ereignisse ─────────────────────────────────────────────────── */

  function befehlSteil(feld) {
    const wert = feld.value;
    const pos = feld.selectionStart ?? wert.length;
    const vor = wert.slice(0, pos);
    /* nur der reine Befehlsname, ohne Leerzeichen und ohne weitere Zeilen */
    const m = /(^|\n)\/([a-zA-Z0-9_-]*)$/.exec(vor);
    return m ? m[2] : null;
  }

  document.addEventListener('input', (e) => {
    if (e.target && e.target.id === 'input') {
      const q = befehlSteil(e.target);
      if (q === null) schliesse();
      else oeffne(q);
      return;
    }
    /* Klick in die Zeile schließt die Liste. */
    if (offen && !pop.contains(e.target)) schliesse();
  }, true);

  document.addEventListener('keydown', (e) => {
    if (!offen) {
      if (e.key === '/' && document.activeElement && document.activeElement.id === 'input') {
        /* nach dem Zeichen öffnen — der input-Handler greift gleich danach */
      }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopImmediatePropagation();
      const n = treffer.length;
      if (!n) return;
      zeiger = (zeiger + (e.key === 'ArrowDown' ? 1 : n - 1)) % n;
      const rows = list.querySelectorAll('.slash-row');
      rows.forEach((r, i) => {
        r.classList.toggle('on', i === zeiger);
        r.setAttribute('aria-selected', String(i === zeiger));
      });
      const on = rows[zeiger];
      if (on) on.scrollIntoView({ block: 'nearest' });
      return;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      e.stopImmediatePropagation();
      einsetzen();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopImmediatePropagation();
      schliesse();
    }
  }, true);

  /* Absenden abfangen, bevor die App es sieht. */
  document.addEventListener('submit', (e) => {
    const form = e.target;
    if (!form || form.id !== 'composer') return;
    const feld = $('#input');
    if (!feld) return;
    const zerlegt = zerlege(feld.value);
    if (!zerlegt) return;         // kein Befehl → normal senden
    e.preventDefault();
    e.stopImmediatePropagation();
    schliesse();
    tue(zerlegt.c, zerlegt.arg);
    /* Jeder erkannte Befehl wird ausgeführt und verschwindet aus dem Feld. */
    leereFeld();
  }, true);

  window.addEventListener('resize', () => { if (offen) setzePosition(); });
  window.addEventListener('scroll', () => { if (offen) setzePosition(); }, true);

  /* Für Tests und zum Nachsehen von außen. */
  window.aesmyraSlash = {
    oeffne: () => oeffne(''),
    schliesse,
    anzahl: () => CMDS.length,
    suche: (q) => suche(q).map((c) => c.n),
    zerlege,
  };
})();
