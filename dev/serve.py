#!/usr/bin/env python3
"""Vorschau-Server für die Aesmyra.-Oberfläche.

Liefert ui/index.html mit einer Tauri-Attrappe aus, damit die Oberfläche ohne
Rust-Hülle im Browser läuft. Änderungen an ui/ werden über SSE gemeldet:
CSS wird ohne Neuladen getauscht, alles andere lädt die Seite neu.

Start:  python3 dev/serve.py [port]
"""
from __future__ import annotations

import json
import mimetypes
import os
import queue
import http.client
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(ROOT, "ui")
WATCH = [UI, os.path.join(ROOT, "dev")]
WATCH_EXT = (".css", ".js", ".html", ".json", ".svg")

_clients: list[queue.Queue] = []
_clients_lock = threading.Lock()


def _scan() -> dict[str, float]:
    seen: dict[str, float] = {}
    for base in WATCH:
        for name in os.listdir(base):
            if not name.endswith(WATCH_EXT):
                continue
            path = os.path.join(base, name)
            try:
                seen[path] = os.stat(path).st_mtime
            except OSError:
                pass
    return seen


def _watcher() -> None:
    last = _scan()
    while True:
        time.sleep(0.4)
        now = _scan()
        changed = [p for p, m in now.items() if last.get(p) != m]
        last = now
        if not changed:
            continue
        only_css = all(p.endswith(".css") for p in changed)
        payload = json.dumps(
            {"type": "css" if only_css else "reload", "files": [os.path.basename(p) for p in changed]}
        )
        with _clients_lock:
            for q in list(_clients):
                try:
                    q.put_nowait(payload)
                except queue.Full:
                    pass


def _page() -> bytes:
    with open(os.path.join(UI, "index.html"), encoding="utf-8") as fh:
        html = fh.read()
    html = html.replace("<head>", '<head>\n<base href="/ui/">', 1)
    html = html.replace(
        '<script src="app.js"></script>',
        '<script src="/dev/stub.js"></script>\n'
        '<script src="app.js"></script>\n'
        '<script src="/dev/livereload.js"></script>',
        1,
    )
    return html.encode("utf-8")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):  # ruhiger
        pass

    def _send(self, code: int, body: bytes, ctype: str, cache: str = "no-store") -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", cache)
        self.end_headers()
        self.wfile.write(body)

    def _proxy(self, rest: str) -> None:
        """Reicht eine Anfrage an den Kern-Dienst weiter und strömt die Antwort
        unverändert durch — auch den Ereignisstrom."""
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else None
        target = "/" + rest.lstrip("/")
        conn = http.client.HTTPConnection("127.0.0.1", 8121, timeout=900)
        try:
            conn.request(
                self.command,
                target,
                body=body,
                headers={"Content-Type": self.headers.get("Content-Type", "application/json")},
            )
            resp = conn.getresponse()
            self.send_response(resp.status)
            self.send_header("Content-Type", resp.getheader("Content-Type", "application/json"))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            while True:
                chunk = resp.read1(4096)
                if not chunk:
                    break
                self.wfile.write(chunk)
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            conn.close()

    def do_POST(self):  # noqa: N802
        if self.path.startswith("/live/"):
            self._proxy(self.path[5:])
            return
        self._send(405, b"nicht erlaubt", "text/plain; charset=utf-8")

    def do_GET(self):  # noqa: N802
        path = self.path.split("?", 1)[0]
        if path == "/__events":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Connection", "keep-alive")
            self.end_headers()
            q: queue.Queue = queue.Queue(maxsize=32)
            with _clients_lock:
                _clients.append(q)
            try:
                self.wfile.write(b": verbunden\n\n")
                self.wfile.flush()
                while True:
                    try:
                        msg = q.get(timeout=20)
                        self.wfile.write(f"data: {msg}\n\n".encode())
                    except queue.Empty:
                        self.wfile.write(b": ping\n\n")
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
            finally:
                with _clients_lock:
                    if q in _clients:
                        _clients.remove(q)
            return

        if path.startswith("/live/"):
            self._proxy(self.path[5:])
            return

        if path == "/__stamp":
            stamp = max(_scan().values(), default=0)
            self._send(200, f"{stamp:.3f}".encode(), "text/plain; charset=utf-8")
            return

        if path in ("/", "/index.html"):
            self._send(200, _page(), "text/html; charset=utf-8")
            return

        rel = path.lstrip("/")
        target = os.path.normpath(os.path.join(ROOT, rel))
        if not target.startswith(ROOT) or not os.path.isfile(target):
            self._send(404, b"nicht gefunden", "text/plain; charset=utf-8")
            return

        ctype = mimetypes.guess_type(target)[0] or "application/octet-stream"
        if ctype.startswith("text/") or ctype in ("application/javascript", "application/json"):
            ctype += "; charset=utf-8"
        with open(target, "rb") as fh:
            body = fh.read()
        self._send(200, body, ctype)


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8120
    threading.Thread(target=_watcher, daemon=True).start()
    srv = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    srv.daemon_threads = True
    print(f"Vorschau auf http://127.0.0.1:{port}/  (Wurzel {ROOT})", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
