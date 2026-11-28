/* Hermes-Befehle für die Eingabeleiste.
   Quelle: hermes_cli/commands.py (COMMAND_REGISTRY), 104 Einträge.
   Neu erzeugen: python3 ~/.hermes/cache/scratch/gen_commands.py
   n Name · a Aliasse · d Beschreibung · e Original · c Gruppe ·
   g Argumente · s Unterbefehle · k nur Konsole/Bot · r hier ausführbar */
window.HERMES_COMMANDS = [
{
"n": "start",
"a": [],
"d": "Startmeldung einer Plattform bestätigen",
"e": "Acknowledge platform start pings without a reply",
"c": "Sitzung",
"g": "",
"s": [],
"k": "gw",
"r": null
},
{
"n": "new",
"a": [
"reset"
],
"d": "Neue Sitzung beginnen",
"e": "Start a new session (fresh session ID + history)",
"c": "Sitzung",
"g": "[name]",
"s": [],
"k": "",
"r": "new"
},
{
"n": "topic",
"a": [],
"d": "Telegram-Themengespräche ein- oder ausschalten",
"e": "Enable or inspect Telegram DM topic sessions",
"c": "Sitzung",
"g": "[off|help|session-id]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "clear",
"a": [],
"d": "Bildschirm leeren und neue Sitzung",
"e": "Clear screen and start a new session",
"c": "Sitzung",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "redraw",
"a": [],
"d": "Oberfläche neu zeichnen",
"e": "Force a full UI repaint (recovers from terminal drift)",
"c": "Sitzung",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "history",
"a": [],
"d": "Gesprächsverlauf zeigen",
"e": "Show conversation history",
"c": "Sitzung",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "save",
"a": [],
"d": "Gespräch exportieren (json, md, html)",
"e": "Export the current conversation (bare /save shows usage)",
"c": "Sitzung",
"g": "<json|md|html> [filename] [redact]",
"s": [],
"k": "",
"r": null
},
{
"n": "retry",
"a": [],
"d": "Letzte Nachricht erneut senden",
"e": "Retry the last message (resend to agent)",
"c": "Sitzung",
"g": "",
"s": [],
"k": "",
"r": "retry"
},
{
"n": "prompt",
"a": [
"compose"
],
"d": "Nächste Nachricht im Editor schreiben",
"e": "Compose your next prompt in $EDITOR (markdown), then send it",
"c": "Sitzung",
"g": "[initial text]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "undo",
"a": [],
"d": "N Schritte zurück und neu fragen",
"e": "Back up N user turns and re-prompt (default 1)",
"c": "Sitzung",
"g": "[N]",
"s": [],
"k": "",
"r": null
},
{
"n": "title",
"a": [],
"d": "Titel der Sitzung setzen",
"e": "Set a title for the current session",
"c": "Sitzung",
"g": "[name]",
"s": [],
"k": "",
"r": "title"
},
{
"n": "handoff",
"a": [],
"d": "Sitzung an einen Messenger übergeben",
"e": "Hand off this session to a messaging platform (Telegram, Discord, etc.)",
"c": "Sitzung",
"g": "<platform>",
"s": [],
"k": "cli",
"r": null
},
{
"n": "branch",
"a": [
"fork"
],
"d": "Sitzung abzweigen",
"e": "Branch the current session (new thread on Discord/Telegram/Slack/Matrix; --here stays here)",
"c": "Sitzung",
"g": "[--here] [name]",
"s": [],
"k": "",
"r": null
},
{
"n": "worktree",
"a": [],
"d": "Git-Arbeitskopien verwalten",
"e": "Show, list, create, or prune isolated git worktrees",
"c": "Sitzung",
"g": "[new [name]|list|prune [--dry-run]]",
"s": [
"new",
"list",
"prune"
],
"k": "cli",
"r": null
},
{
"n": "compress",
"a": [
"compact"
],
"d": "Kontext verdichten („here N“ behält N Runden)",
"e": "Compress conversation context (add 'here [N]' to keep recent N turns; --preview shows what would happen)",
"c": "Sitzung",
"g": "[here [N] | focus topic | --preview|--dry-run]",
"s": [],
"k": "",
"r": "compress"
},
{
"n": "rollback",
"a": [],
"d": "Datei-Sicherungspunkte wiederherstellen",
"e": "List or restore filesystem checkpoints (restores keep your hand-edits; --all overrides)",
"c": "Sitzung",
"g": "[number] [--all]",
"s": [],
"k": "",
"r": null
},
{
"n": "snapshot",
"a": [
"snap"
],
"d": "Zustand sichern oder wiederherstellen",
"e": "Create or restore state snapshots of Hermes config/state",
"c": "Sitzung",
"g": "[create|restore <id>|prune]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "export",
"a": [],
"d": "Profil als Archiv ausgeben",
"e": "Export a profile (config, skills, theme) to a shareable archive",
"c": "Einstellungen",
"g": "[profile] [-o output.tar.gz]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "import",
"a": [],
"d": "Profil-Archiv einlesen",
"e": "Import a shared profile archive as a new profile",
"c": "Einstellungen",
"g": "<archive.tar.gz> [--name <name>]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "stop",
"a": [],
"d": "Laufende Hintergrundprozesse beenden",
"e": "Kill all running background processes",
"c": "Sitzung",
"g": "",
"s": [],
"k": "",
"r": "stop"
},
{
"n": "discard",
"a": [],
"d": "Verlorenen Zug verwerfen",
"e": "Drop a turn lost when Hermes restarted so this chat's queue runs again",
"c": "Sitzung",
"g": "",
"s": [],
"k": "gw",
"r": null
},
{
"n": "pause",
"a": [],
"d": "Neue Arbeit pausieren",
"e": "Pause new work globally (emergency stop); '/pause off' resumes",
"c": "Sitzung",
"g": "[reason | off]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "approve",
"a": [],
"d": "Gefährlichen Befehl freigeben",
"e": "Approve a pending dangerous command",
"c": "Sitzung",
"g": "[session|always]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "deny",
"a": [],
"d": "Gefährlichen Befehl ablehnen",
"e": "Deny a pending dangerous command (optionally with a reason)",
"c": "Sitzung",
"g": "[all] [reason]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "bg",
"a": [],
"d": "Auftrag in eigener Sitzung im Hintergrund",
"e": "Run a prompt in a separate background session",
"c": "Sitzung",
"g": "<prompt>",
"s": [],
"k": "",
"r": null
},
{
"n": "btw",
"a": [],
"d": "Nebenfrage ohne Unterbrechung stellen",
"e": "Ask a side question about the current conversation without interrupting it",
"c": "Sitzung",
"g": "<question>",
"s": [],
"k": "",
"r": null
},
{
"n": "agents",
"a": [
"tasks"
],
"d": "Aktive Helfer und Aufgaben zeigen",
"e": "Show active agents and running tasks",
"c": "Sitzung",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "journey",
"a": [
"learning",
"memory-graph"
],
"d": "Gelerntes und Erinnerungen ansehen",
"e": "Open the learning journey timeline",
"c": "Sitzung",
"g": "[list|delete <id>|edit <id>]",
"s": [
"list",
"delete",
"edit"
],
"k": "cli",
"r": null
},
{
"n": "queue",
"a": [
"q"
],
"d": "Nachricht für die nächste Runde vormerken",
"e": "Queue a prompt for the next turn, or list/edit/rm/move/clear queued prompts",
"c": "Sitzung",
"g": "[<prompt>|list|edit N <prompt>|rm N|move A B|clear]",
"s": [
"list",
"edit",
"rm",
"move",
"clear",
"add"
],
"k": "",
"r": null
},
{
"n": "steer",
"a": [
"s"
],
"d": "Nachricht nach dem nächsten Werkzeug einschieben",
"e": "Inject a message after the next tool call without interrupting",
"c": "Sitzung",
"g": "<prompt>",
"s": [],
"k": "",
"r": "steer"
},
{
"n": "goal",
"a": [],
"d": "Dauerziel über mehrere Runden setzen",
"e": "Set a standing goal Hermes works on across turns until achieved",
"c": "Sitzung",
"g": "[text | -- <text> | draft <text> | show | gate add <cmd> | pause | resume | clear | status | wait <pid> | unwait]",
"s": [],
"k": "",
"r": null
},
{
"n": "heartbeat",
"a": [
"hb"
],
"d": "Wiederkehrende Nachricht bei Leerlauf",
"e": "Set a recurring prompt that re-enters this session when idle",
"c": "Sitzung",
"g": "[every <interval> <prompt> | status | pause | resume | clear]",
"s": [
"status",
"pause",
"resume",
"clear"
],
"k": "",
"r": null
},
{
"n": "refine",
"a": [],
"d": "Lehren aus diesem Gespräch speichern",
"e": "Review this conversation now and save lessons to memory/skills",
"c": "Sitzung",
"g": "[focus instructions]",
"s": [],
"k": "",
"r": null
},
{
"n": "review",
"a": [],
"d": "Unabhängige Prüfung durch einen Unteragenten",
"e": "Spawn an independent subagent to review the work just discussed (PR, code, docs)",
"c": "Sitzung",
"g": "[review instructions]",
"s": [],
"k": "",
"r": null
},
{
"n": "loop",
"a": [
"proactive"
],
"d": "Auftrag in festem Takt wiederholen",
"e": "Re-run a prompt on a recurring interval in this session",
"c": "Sitzung",
"g": "[interval] <prompt> [--times N] [--until <condition>] | status | pause | resume | stop",
"s": [],
"k": "",
"r": null
},
{
"n": "plan",
"a": [],
"d": "Umsetzungsplan schreiben, ohne zu handeln",
"e": "Write a markdown implementation plan to .hermes/plans/ without executing anything",
"c": "Sitzung",
"g": "[task]",
"s": [],
"k": "",
"r": null
},
{
"n": "moa",
"a": [],
"d": "Auftrag durch Mixture of Agents schicken",
"e": "Run one prompt through the default Mixture of Agents preset, then restore your model",
"c": "Sitzung",
"g": "<prompt>",
"s": [],
"k": "",
"r": null
},
{
"n": "subgoal",
"a": [],
"d": "Zusatzkriterien zum Ziel pflegen",
"e": "Add or manage extra criteria on the active goal",
"c": "Sitzung",
"g": "[text | remove N | clear]",
"s": [],
"k": "",
"r": null
},
{
"n": "status",
"a": [],
"d": "Sitzung, Modell und Kontext zeigen",
"e": "Show session, model, token, and context info",
"c": "Sitzung",
"g": "",
"s": [],
"k": "",
"r": "status"
},
{
"n": "egress",
"a": [],
"d": "Status des Docker-Ausgangsproxy",
"e": "Show Docker egress proxy status",
"c": "Sitzung",
"g": "[status]",
"s": [
"status"
],
"k": "",
"r": null
},
{
"n": "context",
"a": [
"ctx"
],
"d": "Kontextfenster mit Belegung zeigen",
"e": "Show detailed context window view with usage gauge, category breakdown, compression stats, and throughput",
"c": "Sitzung",
"g": "[all]",
"s": [
"all"
],
"k": "",
"r": null
},
{
"n": "whoami",
"a": [],
"d": "Zugriffsstufe der Befehle zeigen",
"e": "Show your slash command access (admin / user)",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "profile",
"a": [],
"d": "Aktives Profil und Verzeichnis",
"e": "Show active profile name and home directory",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "sethome",
"a": [
"set-home"
],
"d": "Diesen Chat als Zuhause setzen",
"e": "Set this chat as the home channel",
"c": "Sitzung",
"g": "",
"s": [],
"k": "gw",
"r": null
},
{
"n": "resume",
"a": [],
"d": "Benannte Sitzung fortsetzen",
"e": "Resume a previously-named session",
"c": "Sitzung",
"g": "[name]",
"s": [],
"k": "",
"r": "resume"
},
{
"n": "sessions",
"a": [],
"d": "Frühere Sitzungen durchsuchen",
"e": "Browse and resume previous sessions",
"c": "Sitzung",
"g": "",
"s": [],
"k": "",
"r": "sessions"
},
{
"n": "config",
"a": [],
"d": "Aktuelle Konfiguration zeigen",
"e": "Show current configuration",
"c": "Einstellungen",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "model",
"a": [],
"d": "Modell wechseln",
"e": "Switch model (session-scoped; --global to persist)",
"c": "Einstellungen",
"g": "[model] [--provider name] [--reasoning level] [--global|--session] [--refresh]",
"s": [],
"k": "",
"r": null
},
{
"n": "codex-runtime",
"a": [
"codex_runtime"
],
"d": "Codex-Laufzeit umschalten",
"e": "Toggle codex app-server runtime for OpenAI/Codex models",
"c": "Einstellungen",
"g": "[auto|codex_app_server]",
"s": [],
"k": "",
"r": null
},
{
"n": "personality",
"a": [],
"d": "Persönlichkeit wählen",
"e": "Set a predefined personality",
"c": "Einstellungen",
"g": "[name]",
"s": [],
"k": "",
"r": null
},
{
"n": "statusbar",
"a": [
"sb"
],
"d": "Statusleiste ein- oder ausschalten",
"e": "Toggle the context/model status bar",
"c": "Einstellungen",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "battery",
"a": [],
"d": "Akkuanzeige ein- oder ausschalten",
"e": "Toggle a color-coded battery indicator in the status bar",
"c": "Einstellungen",
"g": "[on|off|status]",
"s": [
"on",
"off",
"status"
],
"k": "cli",
"r": null
},
{
"n": "timestamps",
"a": [
"ts"
],
"d": "Zeitstempel an Nachrichten zeigen",
"e": "Toggle [HH:MM] timestamps on messages and /history",
"c": "Einstellungen",
"g": "[on|off|status]",
"s": [
"on",
"off",
"status"
],
"k": "cli",
"r": null
},
{
"n": "diff",
"a": [],
"d": "Git-Änderungen im Arbeitsordner",
"e": "Show git changes in the working directory",
"c": "Auskunft",
"g": "[staged|all|session] [--stat] [path...]",
"s": [
"staged",
"all",
"session"
],
"k": "",
"r": null
},
{
"n": "verbose",
"a": [],
"d": "Stufe der Werkzeugausgabe wählen",
"e": "Cycle tool progress display: off -> new -> all -> verbose",
"c": "Einstellungen",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "focus",
"a": [],
"d": "Fokusansicht ein- oder ausschalten",
"e": "Toggle focus view — show only your prompt and the final response",
"c": "Einstellungen",
"g": "[on|off|status]",
"s": [
"on",
"off",
"status"
],
"k": "cli",
"r": null
},
{
"n": "footer",
"a": [],
"d": "Laufzeit-Fußzeile an Antworten",
"e": "Toggle gateway runtime-metadata footer on final replies",
"c": "Einstellungen",
"g": "[on|off|status]",
"s": [
"on",
"off",
"status"
],
"k": "",
"r": null
},
{
"n": "yolo",
"a": [],
"d": "Freigaben überspringen",
"e": "Toggle YOLO mode (skip all dangerous command approvals)",
"c": "Einstellungen",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "approvals",
"a": [],
"d": "Freigabemodus setzen",
"e": "Show or set the persistent dangerous-command approval mode",
"c": "Einstellungen",
"g": "[manual|smart|off]",
"s": [
"manual",
"smart",
"off"
],
"k": "",
"r": null
},
{
"n": "reasoning",
"a": [],
"d": "Denkaufwand und Anzeige steuern",
"e": "Manage reasoning effort and display",
"c": "Einstellungen",
"g": "[level|show|hide|full|clamp] [--global]",
"s": [
"none",
"minimal",
"low",
"medium",
"high",
"xhigh",
"max",
"ultra"
],
"k": "",
"r": null
},
{
"n": "fast",
"a": [],
"d": "Schneller Verarbeitungstarif",
"e": "Fast mode — OpenAI Priority Processing / Anthropic Fast Mode (normal/fast/auto/cold/ultrafast)",
"c": "Einstellungen",
"g": "[normal|fast|auto|cold|ultrafast|status] [--global]",
"s": [
"normal",
"fast",
"auto",
"cold",
"ultrafast",
"ultra",
"status",
"on"
],
"k": "",
"r": null
},
{
"n": "skin",
"a": [],
"d": "Farbschema wählen",
"e": "Show or change the display skin/theme",
"c": "Einstellungen",
"g": "[name]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "indicator",
"a": [],
"d": "Beschäftigungsanzeige der Konsole",
"e": "Pick the TUI busy-indicator style",
"c": "Einstellungen",
"g": "[ascii|emoji|kaomoji|unicode]",
"s": [
"ascii",
"emoji",
"kaomoji",
"unicode"
],
"k": "cli",
"r": null
},
{
"n": "voice",
"a": [],
"d": "Sprachmodus ein- oder ausschalten",
"e": "Toggle voice mode",
"c": "Einstellungen",
"g": "[on|off|tts|status]",
"s": [
"on",
"off",
"tts",
"status"
],
"k": "",
"r": null
},
{
"n": "wake",
"a": [],
"d": "Weckwort-Zuhörer ein- oder ausschalten",
"e": "Toggle the 'Hey Hermes' wake word listener",
"c": "Einstellungen",
"g": "[on|off|status]",
"s": [
"on",
"off",
"status"
],
"k": "cli",
"r": null
},
{
"n": "busy",
"a": [],
"d": "Verhalten während laufender Arbeit",
"e": "Control how messages behave while Hermes is working",
"c": "Einstellungen",
"g": "[queue|steer|interrupt|status]",
"s": [
"queue",
"steer",
"interrupt",
"status"
],
"k": "",
"r": null
},
{
"n": "autoprune",
"a": [],
"d": "Alte Sitzungen automatisch aufräumen",
"e": "Turn automatic pruning of old sessions on or off",
"c": "Einstellungen",
"g": "[on|off|later|status]",
"s": [
"on",
"off",
"later",
"status"
],
"k": "",
"r": null
},
{
"n": "tools",
"a": [],
"d": "Werkzeuge verwalten",
"e": "Manage tools: /tools [list|disable|enable] [name...]",
"c": "Werkzeuge",
"g": "[list|disable|enable] [name...]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "toolsets",
"a": [],
"d": "Werkzeuggruppen auflisten",
"e": "List available toolsets",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "skills",
"a": [],
"d": "Fertigkeiten suchen und verwalten",
"e": "Search, install, inspect, or manage skills",
"c": "Werkzeuge",
"g": "",
"s": [
"search",
"browse",
"inspect",
"install",
"audit",
"pending",
"approve",
"reject"
],
"k": "cli",
"r": null
},
{
"n": "memory",
"a": [],
"d": "Erinnerungen prüfen und freigeben",
"e": "Review pending memory writes / toggle the approval gate",
"c": "Werkzeuge",
"g": "[pending|approve|reject|approval] [id|on|off]",
"s": [
"pending",
"approve",
"reject",
"approval"
],
"k": "",
"r": null
},
{
"n": "bundles",
"a": [],
"d": "Fertigkeitsbündel auflisten",
"e": "List skill bundles (aliases /<name> for multiple skills)",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "pet",
"a": [],
"d": "Maskottchen steuern",
"e": "Toggle or adopt a petdex mascot (/pet, /pet list, /pet <slug>)",
"c": "Werkzeuge",
"g": "[toggle|list|scale <n>|<slug>]",
"s": [
"toggle",
"list",
"scale",
"off"
],
"k": "cli",
"r": null
},
{
"n": "hatch",
"a": [
"generate-pet"
],
"d": "Neues Maskottchen erzeugen",
"e": "Generate a new petdex pet from a description",
"c": "Werkzeuge",
"g": "[description]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "learn",
"a": [],
"d": "Fertigkeit aus Material lernen",
"e": "Learn a reusable skill from anything you describe (dirs, URLs, this chat, notes)",
"c": "Werkzeuge",
"g": "<what to learn from>",
"s": [],
"k": "",
"r": null
},
{
"n": "init",
"a": [],
"d": "Projektanweisungen aus dem Ordner erzeugen",
"e": "Generate or update AGENTS.md project instructions from a repo scan",
"c": "Werkzeuge",
"g": "[notes]",
"s": [],
"k": "",
"r": null
},
{
"n": "cron",
"a": [],
"d": "Zeitpläne verwalten",
"e": "Manage scheduled tasks",
"c": "Werkzeuge",
"g": "[subcommand]",
"s": [
"list",
"add",
"create",
"edit",
"pause",
"resume",
"run",
"remove"
],
"k": "cli",
"r": null
},
{
"n": "suggestions",
"a": [
"suggest"
],
"d": "Vorgeschlagene Automationen prüfen",
"e": "Review suggested automations (accept/dismiss)",
"c": "Werkzeuge",
"g": "[accept|dismiss N | catalog]",
"s": [
"accept",
"dismiss",
"catalog",
"clear"
],
"k": "",
"r": null
},
{
"n": "blueprint",
"a": [
"bp"
],
"d": "Automation aus einer Vorlage einrichten",
"e": "Set up an automation from a blueprint template",
"c": "Werkzeuge",
"g": "[name] [slot=value ...]",
"s": [],
"k": "",
"r": null
},
{
"n": "curator",
"a": [],
"d": "Pflege der Fertigkeiten",
"e": "Background skill maintenance (status, run, pin, archive, list-archived)",
"c": "Werkzeuge",
"g": "[subcommand]",
"s": [
"status",
"run",
"pause",
"resume",
"pin",
"unpin",
"restore",
"list-archived"
],
"k": "",
"r": null
},
{
"n": "kanban",
"a": [],
"d": "Gemeinsames Aufgabenbrett",
"e": "Multi-profile collaboration board (tasks, links, comments)",
"c": "Werkzeuge",
"g": "[subcommand]",
"s": [
"init",
"boards",
"create",
"list",
"ls",
"show",
"assign",
"reclaim"
],
"k": "",
"r": null
},
{
"n": "reload",
"a": [],
"d": "Umgebungsvariablen neu einlesen",
"e": "Reload .env variables into the running session",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "reload-mcp",
"a": [
"reload_mcp"
],
"d": "MCP-Server neu laden",
"e": "Reload MCP servers from config",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "reload-skills",
"a": [
"reload_skills"
],
"d": "Fertigkeiten neu einlesen",
"e": "Re-scan ~/.hermes/skills/ for newly installed or removed skills",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "browser",
"a": [],
"d": "Browser über CDP verbinden",
"e": "Connect browser tools to your live Chromium-family browser via CDP, or switch to Browser Use mode",
"c": "Werkzeuge",
"g": "[connect|disconnect|status|use]",
"s": [
"connect",
"disconnect",
"status",
"use"
],
"k": "cli",
"r": null
},
{
"n": "plugins",
"a": [],
"d": "Erweiterungen auflisten",
"e": "List installed plugins and their status",
"c": "Werkzeuge",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "commands",
"a": [],
"d": "Alle Befehle durchblättern",
"e": "Browse all commands and skills (paginated)",
"c": "Auskunft",
"g": "[page]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "help",
"a": [],
"d": "Befehle anzeigen",
"e": "Show available commands (/help skills lists skill commands, /help <text> filters)",
"c": "Auskunft",
"g": "[skills|<filter>]",
"s": [],
"k": "",
"r": "help"
},
{
"n": "palette",
"a": [],
"d": "Befehlspalette öffnen",
"e": "Open the fuzzy command palette (also Ctrl+P)",
"c": "Auskunft",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "restart",
"a": [],
"d": "Gateway neu starten",
"e": "Gracefully restart the gateway after draining active runs",
"c": "Sitzung",
"g": "",
"s": [],
"k": "gw",
"r": null
},
{
"n": "usage",
"a": [],
"d": "Token-Verbrauch und Grenzen",
"e": "Show token usage and rate limits; `reset` redeems a banked Codex limit reset",
"c": "Auskunft",
"g": "[reset [--force]]",
"s": [],
"k": "",
"r": null
},
{
"n": "subscription",
"a": [
"upgrade"
],
"d": "Nous-Tarif ansehen",
"e": "View your Nous plan and change it in the browser",
"c": "Auskunft",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "login",
"a": [],
"d": "Mit Nous-Konto anmelden",
"e": "Sign in with a Nous account (keeps your connectors)",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "topup",
"a": [],
"d": "Guthaben und Abrechnung",
"e": "Show your Nous balance and manage billing on the portal",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "insights",
"a": [],
"d": "Nutzungsstatistik",
"e": "Show usage insights and analytics",
"c": "Auskunft",
"g": "[days]",
"s": [],
"k": "",
"r": null
},
{
"n": "platforms",
"a": [
"gateway"
],
"d": "Status der Messenger",
"e": "Show gateway/messaging platform status",
"c": "Auskunft",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "platform",
"a": [],
"d": "Messenger pausieren oder fortsetzen",
"e": "Pause, resume, or list a failing gateway platform",
"c": "Auskunft",
"g": "<pause|resume|list> [name]",
"s": [],
"k": "gw",
"r": null
},
{
"n": "copy",
"a": [],
"d": "Letzte Antwort kopieren",
"e": "Copy the last assistant response to clipboard",
"c": "Auskunft",
"g": "[number]",
"s": [],
"k": "cli",
"r": null
},
{
"n": "paste",
"a": [],
"d": "Bild aus der Zwischenablage anhängen",
"e": "Attach clipboard image from your clipboard",
"c": "Auskunft",
"g": "",
"s": [],
"k": "cli",
"r": null
},
{
"n": "image",
"a": [],
"d": "Bilddatei anhängen",
"e": "Attach a local image file for your next prompt",
"c": "Auskunft",
"g": "<path>",
"s": [],
"k": "cli",
"r": null
},
{
"n": "update",
"a": [],
"d": "Hermes aktualisieren",
"e": "Update Hermes Agent to the latest version",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "version",
"a": [
"v"
],
"d": "Version anzeigen",
"e": "Show Hermes Agent version",
"c": "Auskunft",
"g": "",
"s": [],
"k": "",
"r": null
},
{
"n": "debug",
"a": [],
"d": "Fehlerbericht hochladen",
"e": "Upload debug report (system info + logs) and get shareable links",
"c": "Auskunft",
"g": "[nous|local]",
"s": [],
"k": "",
"r": null
},
{
"n": "quit",
"a": [
"exit"
],
"d": "Hermes beenden",
"e": "Exit the CLI (use --delete to also remove session history)",
"c": "Beenden",
"g": "[--delete]",
"s": [],
"k": "cli",
"r": null
}
];
