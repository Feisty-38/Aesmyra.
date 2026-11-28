#!/usr/bin/env python3
"""Befehlsdienst für Aesmyra.

Führt Hermes-Slash-Befehle über den laufenden Gateway aus und gibt genau die
Ausgabe zurück, die auch die Hermes-Konsole zeigt. Der Dienst ist die Brücke
zwischen der Oberfläche (Rust-Kern) und dem Gateway-RPC `slash.exec`; er hält
selbst keinen Agenten und keine Sitzung.

Kann der Gateway einen Befehl für diese Sitzung nicht ausführen (API-Server-
Sitzungen sind von `slash.exec` ausgenommen, einige Befehle kennt nur die
Konsole), baut der Dienst die Ausgabe aus den echten Sitzungsdaten von
`session.info`/`session.stats`/`session.mutate`. Die Werte sind dieselben,
die auch das Terminal anzeigt.

Start (systemd): aesmyra-commands.service
Aufruf: POST http://127.0.0.1:8122/command
        {"session_id": "...", "command": "/status"}
Antwort: {"ok": true, "type": "exec", "output": "...", "raw": {...}}
"""
from __future__ import annotations

import asyncio
import json
import sys
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from hermes_cli.gateway_client import GatewayClientError, connect_gateway

HOST = "127.0.0.1"
PORT = 8122
MAX_BODY = 64 * 1024

HELP_TEXT = """Verfügbare Befehle:
  /status              Sitzung, Modell, Kontext und Token
  /context             Kontextauslastung und Token-Aufschlüsselung
  /usage               Token-Verbrauch dieser Sitzung
  /model [name]        Modell anzeigen oder wechseln
  /tools               Werkzeuge dieser Sitzung
  /skills              Geladene Skills
  /reasoning [level]   Reasoning-Stufe anzeigen oder setzen
  /compress            Verlauf komprimieren
  /title [text]        Titel der Sitzung setzen
  /stop                Laufenden Turn abbrechen
  /help                Diese Übersicht"""


def _num(value) -> str:
    try:
        return f"{int(value):,}".replace(",", ".")
    except (TypeError, ValueError):
        return "—"


def _stamp(value) -> str:
    """Unix-Zeit als lesbares Datum, sonst den Rohwert."""
    from datetime import datetime

    try:
        return datetime.fromtimestamp(float(value)).strftime("%d.%m.%Y %H:%M")
    except (TypeError, ValueError, OSError):
        return str(value) if value else "—"


def _pct(part, whole) -> str:
    try:
        return f"{round(int(part) * 100 / int(whole))}%"
    except (TypeError, ValueError, ZeroDivisionError):
        return "—"


def _status_text(session_id: str, info: dict, stats: dict) -> str:
    model = info.get("model") or stats.get("model") or "unbekannt"
    provider = info.get("provider") or stats.get("provider") or ""
    model_line = f"{model} ({provider})" if provider else model
    lines = [
        f"Session ID: {session_id}",
        f"Title: {info.get('title') or '(ohne Titel)'}",
        f"Created: {_stamp(info.get('started_at'))}",
        f"Last Activity: {_stamp(info.get('last_active_at'))}",
        f"Model: {model_line}",
        "Context: {} / {} ({})".format(
            _num(stats.get("context_tokens")),
            _num(stats.get("context_length")),
            _pct(stats.get("context_tokens"), stats.get("context_length")),
        ),
        f"Lifetime tokens: {_num(stats.get('total_tokens'))}",
        f"API calls: {_num(stats.get('api_calls'))}",
    ]
    tools = info.get("tool_count")
    if tools:
        lines.append(f"Tools: {tools}")
    return "\n".join(lines)


def _context_text(stats: dict) -> str:
    return "\n".join(
        [
            "Context: {} / {} ({})".format(
                _num(stats.get("context_tokens")),
                _num(stats.get("context_length")),
                _pct(stats.get("context_tokens"), stats.get("context_length")),
            ),
            f"Last prompt: {_num(stats.get('last_prompt_tokens'))}",
            f"Input tokens: {_num(stats.get('input_tokens'))}",
            f"Output tokens: {_num(stats.get('output_tokens'))}",
            f"Cache read: {_num(stats.get('cache_read_tokens'))}",
            f"Reasoning tokens: {_num(stats.get('reasoning_tokens'))}",
            f"Compressions: {_num(stats.get('compression_count'))}",
        ]
    )


def _usage_text(stats: dict) -> str:
    lines = [
        f"Lifetime tokens: {_num(stats.get('total_tokens'))}",
        f"  Input: {_num(stats.get('input_tokens'))}",
        f"  Output: {_num(stats.get('output_tokens'))}",
        f"  Cache read: {_num(stats.get('cache_read_tokens'))}",
        f"  Reasoning: {_num(stats.get('reasoning_tokens'))}",
        f"API calls: {_num(stats.get('api_calls'))}",
    ]
    speed = stats.get("tokens_per_s")
    if speed:
        lines.append(f"Tokens/s: {speed}")
    return "\n".join(lines)


def _model_text(info: dict, stats: dict) -> str:
    model = info.get("model") or stats.get("model") or "unbekannt"
    provider = info.get("provider") or stats.get("provider") or ""
    lines = [f"Model: {model}"]
    if provider:
        lines.append(f"Provider: {provider}")
    fallback = stats.get("fallback_model")
    if fallback:
        lines.append(f"Fallback: {fallback}")
    return "\n".join(lines)


def _tools_text(info: dict) -> str:
    groups = info.get("tools") or {}
    names = sorted({name for tools in groups.values() for name in tools})
    if not names:
        toolsets = info.get("enabled_toolsets") or []
        return f"Enabled toolsets: {', '.join(toolsets)}" if toolsets else "No tools available."
    lines = [f"Available tools ({len(names)}):"]
    lines.extend(f"  {toolset}: {', '.join(sorted(tools))}" for toolset, tools in sorted(groups.items()) if tools)
    return "\n".join(lines)


def _skills_text(info: dict) -> str:
    groups = info.get("skills") or {}
    total = sum(len(names) for names in groups.values())
    if not total:
        return "No skills loaded."
    lines = [f"Skills ({total}):"]
    lines.extend(f"  {cat}: {', '.join(sorted(names))}" for cat, names in sorted(groups.items()) if names)
    return "\n".join(lines)


def _reasoning_text(info: dict) -> str:
    level = info.get("reasoning_effort") or "—"
    wire = info.get("reasoning_effort_wire")
    return f"Reasoning: {level}" + (f" ({wire})" if wire else "")


def _summary_text(result: dict) -> str:
    summary = result.get("summary") or {}
    parts = [summary.get("headline"), summary.get("token_line"), summary.get("note")]
    text = "\n".join(part for part in parts if part)
    if text:
        return text
    if result.get("ok") is False:
        return f"Komprimierung abgelehnt ({result.get('reason') or 'refused'})."
    return "Kontext komprimiert."


async def _mutate(client, session_id: str, operation: str, payload: dict) -> dict:
    """Sitzungsänderung mit den Fences, die `session.mutate` verlangt."""
    snapshot = await client.rpc("session.resume", session_id=session_id, _timeout=30)
    subscription = snapshot.get("subscription_id") if isinstance(snapshot, dict) else None
    try:
        return await client.rpc(
            "session.mutate",
            session_id=session_id,
            operation=operation,
            request_id=uuid.uuid4().hex,
            expected_revision=snapshot.get("revision"),
            expected_generation=snapshot.get("execution_generation"),
            payload=payload,
            _timeout=300,
        )
    finally:
        if subscription:
            try:
                await client.rpc(
                    "session.detach",
                    session_id=session_id,
                    subscription_id=subscription,
                    _timeout=10,
                )
            except Exception:  # noqa: BLE001 — Abmelden darf den Befehl nicht kippen
                pass


async def _fallback(client, session_id: str, command: str):
    """Ausgabe aus echten Sitzungsdaten, wenn `slash.exec` sie verweigert."""
    name, _, arg = command[1:].partition(" ")
    name = name.strip().lower()
    arg = arg.strip()

    if name in {"status", "context", "usage", "model", "tools", "skills", "reasoning"}:
        info = await client.rpc("session.info", session_id=session_id, _timeout=30)
        if name == "tools":
            return {"type": "exec", "output": _tools_text(info)}
        if name == "skills":
            return {"type": "exec", "output": _skills_text(info)}
        if name == "reasoning":
            return {"type": "exec", "output": _reasoning_text(info)}
        stats = await client.rpc("session.stats", session_id=session_id, _timeout=30)
        if name == "status":
            return {"type": "exec", "output": _status_text(session_id, info, stats)}
        if name == "context":
            return {"type": "exec", "output": _context_text(stats)}
        if name == "usage":
            return {"type": "exec", "output": _usage_text(stats)}
        return {"type": "exec", "output": _model_text(info, stats)}

    if name == "model" and arg:
        result = await _mutate(client, session_id, "model", {"model": arg})
        status = result.get("status") if isinstance(result, dict) else None
        if status == "confirmation_required":
            return {"type": "exec", "output": f"Modellwechsel auf {arg} braucht eine Bestätigung."}
        if status == "deferred":
            return {"type": "exec", "output": f"Modellwechsel auf {arg} ist für die nächste Runde vorgemerkt."}
        return {"type": "exec", "output": f"Modell gewechselt: {result.get('model') or arg}"}

    if name == "reasoning" and arg:
        result = await _mutate(client, session_id, "reasoning", {"effort": arg})
        level = result.get("reasoning_effort") if isinstance(result, dict) else arg
        return {"type": "exec", "output": f"Reasoning: {level or arg}"}

    if name == "skills":
        try:
            from tools.skills_tool import find_all_skills, sort_skills

            skills = sort_skills(find_all_skills(skip_disabled=False))
            lines = []
            for item in skills:
                title = item.get("name") or "?"
                desc = (item.get("description") or "").strip()
                lines.append(f"- {title}: {desc}" if desc else f"- {title}")
            return {"type": "exec", "output": "\n".join(lines) or "Keine Fähigkeiten gefunden."}
        except Exception as exc:  # noqa: BLE001 — Meldung statt Absturz
            return {"ok": False, "error": f"Fähigkeiten nicht lesbar: {exc}"}

    if name == "compress":
        result = await _mutate(client, session_id, "compress", {})
        return {"type": "exec", "output": _summary_text(result if isinstance(result, dict) else {})}

    if name in {"new", "clear"}:
        await _mutate(client, session_id, "reset", {})
        return {"type": "exec", "output": "Neue Sitzung gestartet."}

    if name == "title":
        if not arg:
            return {"type": "exec", "output": "Nutzung: /title <text>"}
        await _mutate(client, session_id, "rename", {"title": arg})
        return {"type": "exec", "output": f"Titel gesetzt: {arg}"}

    if name == "stop":
        await client.rpc("session.interrupt", session_id=session_id, _timeout=30)
        return {"type": "exec", "output": "Turn abgebrochen."}

    if name == "help":
        return {"type": "exec", "output": HELP_TEXT}

    return None


async def _exec(session_id: str, command: str) -> dict:
    async with connect_gateway() as client:
        try:
            return await client.rpc(
                "slash.exec",
                session_id=session_id,
                command=command,
                _timeout=600,
            )
        except GatewayClientError as exc:
            reason = str(exc)
            if not any(k in reason for k in ("permission_denied", "unsupported_command", "not_found")):
                raise
            fallback = await _fallback(client, session_id, command)
            if fallback is None:
                raise
            return fallback


def _text(raw: dict) -> str:
    for key in ("output", "notice", "message", "text"):
        value = raw.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return ""


class Handler(BaseHTTPRequestHandler):
    server_version = "AesmyraCommands/2"

    def log_message(self, format: str, *args) -> None:  # noqa: A002 — Signatur der Basisklasse
        return

    def _send(self, code: int, value: dict) -> None:
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if self.path == "/health":
            self._send(200, {"ok": True})
        else:
            self._send(404, {"ok": False, "error": "unbekannter Pfad"})

    def do_POST(self) -> None:
        if self.path != "/command":
            self._send(404, {"ok": False, "error": "unbekannter Pfad"})
            return
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            length = 0
        if length <= 0 or length > MAX_BODY:
            self._send(400, {"ok": False, "error": "ungültige Anfrage"})
            return
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self._send(400, {"ok": False, "error": "kein JSON"})
            return

        session_id = str(body.get("session_id") or "").strip()
        command = str(body.get("command") or "").strip()
        if not session_id or not command.startswith("/"):
            self._send(400, {"ok": False, "error": "session_id und /befehl nötig"})
            return

        try:
            raw = asyncio.run(_exec(session_id, command))
        except GatewayClientError as exc:
            self._send(200, {"ok": False, "kind": "gateway", "error": str(exc)})
            return
        except Exception as exc:  # noqa: BLE001 — dem Aufrufer zeigen, nicht abbrechen
            self._send(500, {"ok": False, "kind": "service", "error": str(exc)})
            return

        if not isinstance(raw, dict):
            self._send(502, {"ok": False, "error": "unerwartete Gateway-Antwort"})
            return
        self._send(
            200,
            {
                "ok": True,
                "type": raw.get("type") or "",
                "output": _text(raw),
                "raw": raw,
            },
        )


def main() -> int:
    server = ThreadingHTTPServer((HOST, PORT), Handler)
    print(f"aesmyra-commands auf http://{HOST}:{PORT}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
