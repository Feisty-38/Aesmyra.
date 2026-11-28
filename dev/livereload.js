/* Live-Vorschau: CSS wird getauscht, alles andere lädt neu.
   Ereignisse kommen über SSE; falls der Vermittler (Tailscale) puffert,
   fragt die Seite zusätzlich alle drei Sekunden einen Zeitstempel ab. */
(() => {
  'use strict';
  let tries = 0;
  let live = false;
  let stamp = null;

  const swapCss = () => {
    document.querySelectorAll('link[rel="stylesheet"]').forEach((link) => {
      const url = new URL(link.href, location.href);
      url.searchParams.set('t', Date.now());
      link.href = url.toString();
    });
  };

  const checkStamp = async () => {
    try {
      const res = await fetch('/__stamp', { cache: 'no-store' });
      const now = (await res.text()).trim();
      if (stamp === null) stamp = now;
      else if (now !== stamp) location.reload();
    } catch { /* gleich nochmal */ }
  };

  const connect = () => {
    const es = new EventSource('/__events');
    es.onopen = () => { tries = 0; live = true; };
    es.onmessage = (ev) => {
      live = true;
      let msg = null;
      try { msg = JSON.parse(ev.data); } catch { return; }
      if (!msg) return;
      if (msg.type === 'css') { swapCss(); stamp = null; }
      else location.reload();
    };
    es.onerror = () => {
      live = false;
      es.close();
      tries++;
      setTimeout(connect, Math.min(8000, 400 * tries));
    };
  };

  connect();
  setInterval(() => { if (!live) checkStamp(); }, 3000);
})();
