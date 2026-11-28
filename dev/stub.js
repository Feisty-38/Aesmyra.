/* Attrappe der Tauri-Hülle für die Vorschau im Browser.
   Spricht über /live mit dem Kern (crates/aesmyra-devserver), also mit
   denselben Befehlen, die sonst die Tauri-Hülle bereitstellt — echte
   Sitzungen, echter Verlauf, echter Ereignisstrom.
   Wird nur von dev/serve.py eingebunden, nicht von der App. */
(() => {
  'use strict';

  const API = '/live';

  const isMobile = /Android|iPhone|iPad|Mobile/i.test(navigator.userAgent);
  const forced = new URLSearchParams(location.search).get('platform');
  const platform = forced || (isMobile ? 'android' : 'linux');

  class Channel {
    constructor() { this.onmessage = null; }
    toJSON() { return { __channel__: true }; }
  }

  async function fail(res) {
    try {
      const data = await res.json();
      if (data && data.error) return data.error;
    } catch { /* kein JSON */ }
    return `${res.status} ${res.statusText || ''}`.trim();
  }

  async function get(path) {
    const res = await fetch(API + path, { cache: 'no-store' });
    if (!res.ok) throw await fail(res);
    return res.json();
  }

  async function post(path, body) {
    const res = await fetch(API + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body || {}),
    });
    if (!res.ok) throw await fail(res);
    return res.json();
  }

  /* Ein Strom je Gespräch: ein zweiter Auftrag in einem anderen Gespräch
     darf den laufenden nicht abschneiden. */
  const streams = new Map();

  function chatStart(args) {
    const channel = args && args.onEvent;
    const emit = (ev) => {
      try { channel && channel.onmessage && channel.onmessage(ev); } catch (e) { console.error(e); }
    };

    const sid = args.sessionId || '';
    if (streams.has(sid)) streams.get(sid).abort();
    const ctl = new AbortController();
    streams.set(sid, ctl);
    const signal = ctl.signal;

    (async () => {
      const res = await fetch(API + '/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId: args.sessionId, text: args.text }),
        signal,
      });

      if (!res.ok) {
        emit({ kind: 'error', message: await fail(res) });
        emit({ kind: 'finished', ok: false });
        return;
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        let cut;
        while ((cut = buffer.indexOf('\n\n')) >= 0) {
          const frame = buffer.slice(0, cut);
          buffer = buffer.slice(cut + 2);
          const line = frame.split('\n').find((l) => l.startsWith('data:'));
          if (!line) continue;
          try { emit(JSON.parse(line.slice(5).trim())); } catch (e) { console.error(e); }
        }
      }
    })().catch((err) => {
      if (err && err.name === 'AbortError') return;
      emit({ kind: 'error', message: String((err && err.message) || err) });
      emit({ kind: 'finished', ok: false });
    }).finally(() => {
      if (streams.get(sid) === ctl) streams.delete(sid);
    });

    return Promise.resolve();
  }

  async function invoke(cmd, args) {
    const a = args || {};
    switch (cmd) {
      case 'app_info':
        return { version: 'dev', platform, arch: 'x86_64' };

      case 'settings_get': {
        const state = await get('/state');
        return state.settings;
      }
      case 'settings_save':
        return post('/settings', { serverUrl: a.serverUrl, token: a.token });
      case 'settings_test': {
        const out = await post('/settings/test');
        return out.message;
      }

      case 'sessions_list':
        return get('/sessions');
      case 'messages_load':
        return get('/messages?session=' + encodeURIComponent(a.sessionId));

      case 'session_create':
        return post('/session/create');
      case 'session_patch':
        await post('/session/patch', {
          sessionId: a.sessionId,
          title: a.title,
          pinned: a.pinned,
          archived: a.archived,
          hidden: a.hidden,
        });
        return null;
      case 'session_delete':
        await post('/session/delete', { sessionId: a.sessionId });
        return null;

      case 'chat_start':
        return chatStart(a);
      case 'chat_stop': {
        const sid = a.sessionId || '';
        if (sid && streams.has(sid)) { streams.get(sid).abort(); streams.delete(sid); }
        if (!sid) { for (const c of streams.values()) c.abort(); streams.clear(); }
        try { await post('/stop', { sessionId: sid }); } catch { /* egal */ }
        return null;
      }

      case 'run_steer':
        await post('/steer', { runId: a.runId, message: a.message });
        return null;
      case 'run_approve':
        await post('/approve', {
          runId: a.runId,
          choice: a.choice,
          requestId: a.requestId,
          executionGeneration: a.executionGeneration || 0,
        });
        return null;
      case 'run_state':
        return get('/run/state?runId=' + encodeURIComponent(a.runId));

      case 'slash_run':
        return post('/slash', { sessionId: a.sessionId, command: a.command });

      case 'media_data_url':
        return post('/media', { path: a.path });

      case 'open_external':
        window.open(a.url, '_blank', 'noopener');
        return null;

      default:
        throw 'Befehl unbekannt: ' + cmd;
    }
  }

  window.__TAURI__ = { core: { invoke, Channel } };
  window.__AESMYRA_PREVIEW__ = true;
})();
