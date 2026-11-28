//! Aesmyra auf GPUI — Fenster, Rahmen und Tastaturbedienung.
//!
//! Die Oberfläche kennt drei Ansichten: Verzeichnis der Sitzungen, Lesefluss
//! und Einstellungen. Jede Sitzung erscheint als Abschnitt aus Frage,
//! Denknotizen, Antwort und Werkzeugfußnoten; der laufende Auftrag wächst
//! unter dem Verlauf mit.

mod field;
mod md;
mod state;
mod theme;
mod ui;



use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, App, Application, Bounds, Context, Entity, FocusHandle, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, Subscription, Window,
    WindowBounds, WindowOptions,
};

use aesmyra_core::model::{Part, Turn};

use state::{AppState, Mode};
use theme::*;
use ui::{blocks, button, hairline, label, soften};

struct Root {
    state: Entity<AppState>,
    focus: FocusHandle,
    _sub: Subscription,
    focused_once: bool,
}

impl Root {
    fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            focus: cx.focus_handle(),
            _sub: sub,
            focused_once: false,
        }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.focus);
        }
        let t = Theme::default();
        ui::set_viewport(f32::from(window.bounds().size.width));
        let st = self.state.clone();
        let s = st.read(cx);
        let view: AnyElement = match s.mode {
            Mode::Index => index_view(s, &t, st.clone()),
            Mode::Reading => reading_view(s, &t, st.clone()),
            Mode::Settings => settings_view(s, &t, st.clone()),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.ink())
            .text_color(t.body())
            .font_family(SERIF)
            .text_size(px(BODY_SIZE))
            .track_focus(&self.focus)
            .on_key_down(move |ev, window, cx| handle_key(&st, ev, window, cx))
            .child(view)
    }
}

// ── Kopfteil ──────────────────────────────────────────────────────────────

fn header(s: &AppState, t: &Theme, subtitle: &str, back: bool) -> AnyElement {
    let dot = if s.busy() {
        t.acc()
    } else if s.notice.as_ref().map(|n| !n.1).unwrap_or(false) {
        gpui::rgb(ERR)
    } else {
        t.acc_soft(0.55)
    };
    div()
        .w_full()
        .h(px(58.))
        .flex_shrink_0()
        .px(px(22.))
        .flex()
        .items_center()
        .gap_3()
        .bg(t.ink())
        .border_b_1()
        .border_color(t.rule())
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(19.))
                        .line_height(px(24.))
                        .text_color(t.fg())
                        .child(SharedString::from("Aesmyra")),
                )
                .child(
                    div()
                        .text_size(px(19.))
                        .line_height(px(24.))
                        .text_color(t.acc())
                        .child(SharedString::from(".")),
                ),
        )
        .child(div().size(px(6.)).rounded_full().bg(dot))
        .child(div().flex_1().min_w_0().child(
            div()
                .truncate()
                .text_size(px(13.5))
                .text_color(t.dim())
                .child(SharedString::from(subtitle.to_string())),
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .when(back, |d| {
                    d.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.dim())
                            .font_family(MONO)
                            .child(SharedString::from("Esc  zurück")),
                    )
                })
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.faint())
                        .font_family(MONO)
                        .child(SharedString::from(truncate(&s.server, 34))),
                ),
        )
        .into_any_element()
}

// ── Verzeichnis ───────────────────────────────────────────────────────────

fn index_view(s: &AppState, t: &Theme, st: Entity<AppState>) -> AnyElement {
    let count = s.sessions.len();
    let rows = s.sessions.iter().enumerate().map(|(i, sess)| {
        let selected = i == s.sel;
        let est = st.clone();
        let title = if !sess.title.trim().is_empty() {
            sess.title.clone()
        } else if !sess.preview.trim().is_empty() {
            sess.preview.clone()
        } else {
            format!("Sitzung {}", sess.id.chars().take(8).collect::<String>())
        };
        let mut row = div()
            .id(("sitzung", i))
            .w_full()
            .flex()
            .items_start()
            .gap_3()
            .px(px(16.))
            .py(px(13.))
            .rounded(px(10.))
            .cursor_pointer()
            .hover(|d| d.bg(t.acc_soft(0.06)))
            .on_click(move |_, _, cx| {
                est.update(cx, |s, cx| s.open_session(i, cx));
            })
            .child(
                div()
                    .w(px(26.))
                    .flex_shrink_0()
                    .pt(px(4.))
                    .text_size(px(11.))
                    .font_family(MONO)
                    .text_color(if selected { t.acc() } else { t.faint() })
                    .child(SharedString::from(format!("{:02}", i + 1))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(17.5))
                            .line_height(px(23.))
                            .text_color(if selected { t.fg() } else { t.body() })
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .text_color(t.faint())
                            .child(SharedString::from(sess.preview.clone())),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(3.))
                    .pt(px(3.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_family(MONO)
                            .text_color(t.dim())
                            .child(SharedString::from(state::ago(sess.last_active))),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_family(MONO)
                            .text_color(t.faint())
                            .child(SharedString::from(format!("{} Nachr.", sess.message_count))),
                    ),
            );
        if selected {
            row = row.bg(t.acc_soft(0.09)).border_l_2().border_color(t.acc_soft(0.7));
        }
        row
    });

    let empty = count == 0;
    let est = st.clone();

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header(
            s,
            t,
            if s.loading {
                "Sitzungen werden geladen …"
            } else {
                "Verzeichnis der Sitzungen"
            },
            false,
        ))
        .child(
            div()
                .id("verzeichnis")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .track_scroll(&s.index_scroll)
                .child(
                    div()
                        .w_full()
                        .py(px(18.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w_full()
                                .w(px(ui::col_width(880.)))
                                .px(px(22.))
                                .flex()
                                .flex_col()
                                .children(rows)
                                .when(empty, |d| {
                                    d.child(
                                        div()
                                            .py(px(60.))
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .text_size(px(16.))
                                                    .text_color(t.dim())
                                                    .child(SharedString::from(if s.cfg_ok {
                                                        "Keine Sitzung auf diesem Server"
                                                    } else {
                                                        "Noch kein Zugang eingerichtet"
                                                    })),
                                            )
                                            .child(
                                                button(t, "einrichten", "Einstellungen öffnen", true)
                                                    .on_click(move |_, _, cx| {
                                                        est.update(cx, |s, cx| {
                                                            s.mode = Mode::Settings;
                                                            cx.notify();
                                                        });
                                                    }),
                                            ),
                                    )
                                }),
                        ),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(30.))
                .flex_shrink_0()
                .px(px(22.))
                .flex()
                .items_center()
                .gap_4()
                .border_t_1()
                .border_color(t.rule())
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(t.faint())
                .child(div().child(SharedString::from("j k  wählen")))
                .child(div().child(SharedString::from("Enter  öffnen")))
                .child(div().child(SharedString::from("n  neue Sitzung")))
                .child(div().child(SharedString::from("d  löschen")))
                .child(div().child(SharedString::from(",  Einstellungen")))
                .child(div().flex_1())
                .child(div().child(SharedString::from(if s.loading {
                    "lädt".to_string()
                } else {
                    format!("{count} Sitzungen")
                }))),
        )
        .into_any_element()
}

// ── Lesefluss ─────────────────────────────────────────────────────────────

fn reading_view(s: &AppState, t: &Theme, st: Entity<AppState>) -> AnyElement {
    let Some(open) = s.open.as_ref() else {
        return index_view(s, t, st);
    };

    let mut turns: Vec<AnyElement> = Vec::new();
    for (i, turn) in open.turns.iter().enumerate() {
        turns.push(turn_block(t, turn, i, s.show_reasoning, false, s.part_budget, st.clone()));
    }

    if s.busy() || !s.live.text.is_empty() || s.live.error.is_some() {
        turns.push(live_block(s, t, st.clone()));
    }

    let sub = format!(
        "{}   ·   Abschnitt {} von {}   ·   {}",
        open.model,
        open.turns.len().max(1),
        open.turns.len().max(1),
        state::ago(open.last_active)
    );

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header(s, t, &open.title, true))
        .child(
            div()
                .id("verlauf")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .track_scroll(&s.scroll)
                .child(
                    div()
                        .w_full()
                        .py(px(26.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w_full()
                                .w(px(ui::col_width(768.)))
                                .px(px(26.))
                                .flex()
                                .flex_col()
                                .gap_6()
                                .children(turns),
                        ),
                ),
        )
        .child(hairline(t))
        .child(composer(s, t, st))
        .child(status_bar(s, t, &sub))
        .into_any_element()
}

/// Grobe Höhe eines Schritts in Pixeln — Grundlage für das Höhenbudget.
fn est_height(part: &Part) -> f32 {
    let r = part.reasoning.chars().count() as f32;
    let x = part.text.chars().count() as f32;
    let lines = r / 82.0 + x / 82.0;
    let tools = part.tools.len() as f32;
    (lines * 23.0 + tools * 28.0 + 70.0).max(60.0)
}

fn turn_block(
    t: &Theme,
    turn: &Turn,
    index: usize,
    show_reasoning: bool,
    live: bool,
    limit: f32,
    st: Entity<AppState>,
) -> AnyElement {
    let mut card = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .pb(px(10.));

    if let Some(q) = &turn.user {
        card = card.child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(label(t, format!("Frage {:02}", index + 1)))
                .child(
                    div()
                        .text_size(px(17.))
                        .line_height(px(26.))
                        .text_color(t.fg())
                        .child(SharedString::from(q.clone())),
                ),
        );
    }

    for steer in &turn.steer {
        card = card.child(
            div()
                .pl(px(12.))
                .border_l_2()
                .border_color(t.acc_soft(0.4))
                .text_size(px(14.))
                .text_color(t.dim())
                .child(SharedString::from(steer.clone())),
        );
    }

    let total = turn.parts.len();
    // GPUI verliert die Zeichnung, wenn ein Verlauf mehrere Tausend Pixel
    // Höhe in einem einzigen Flex-Block hat. Deshalb wird von hinten so viel
    // gezeigt, wie in das Höhenbudget passt; ältere Schritte klappt der Knopf auf.
    let skip = std::env::var("AESMYRA_SKIP")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| {
            let mut used = 0.0f32;
            let mut start = total;
            while start > 0 {
                let h = est_height(&turn.parts[start - 1]);
                if used + h > limit && total - start >= 3 {
                    break;
                }
                used += h;
                start -= 1;
            }
            start
        });
    if skip > 0 {
        let st_more = st.clone();
        card = card.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id(("mehr", index))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .bg(t.acc_soft(0.10))
                        .text_color(t.acc())
                        .text_size(px(11.))
                        .child(SharedString::from(format!("{} ältere zeigen", skip)))
                        .on_click(move |_, _window, cx| {
                            st_more.update(cx, |s, cx| {
                                s.part_budget += 3000.0;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.faint())
                        .child(SharedString::from(format!(
                            "{} von {} Schritten ausgeblendet",
                            skip, total
                        ))),
                ),
        );
    }

    for part in turn.parts.iter().skip(skip) {
        card = card.child(part_block(t, part, show_reasoning, live));
    }

    card.into_any_element()
}

fn part_block(t: &Theme, part: &Part, show_reasoning: bool, live: bool) -> AnyElement {
    let mut col = div().w_full().flex().flex_col().gap_4();

    if show_reasoning && !part.reasoning.trim().is_empty() {
        col = col.child(
            div()
                .w_full()
                .pl(px(12.))
                .py(px(2.))
                .border_l_1()
                .border_color(t.rule2())
                .flex()
                .flex_col()
                .gap_1()
                .child(label(t, "Denken"))
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(23.))
                        .text_color(t.faint())
                        .child(SharedString::from(soften(part.reasoning.trim()))),
                ),
        );
    }

    if !part.text.trim().is_empty() {
        let mut prose = div().w_full().flex().flex_col().gap_3();
        for el in blocks(t, &part.text) {
            prose = prose.child(el);
        }
        col = col.child(prose);
    }

    if !part.tools.is_empty() {
        let mut foot = div().w_full().flex().flex_col().gap_1().pt(px(2.));
        for tool in &part.tools {
            let args = truncate(&tool.args, 96);
            foot = foot.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .text_size(px(11.5))
                    .line_height(px(17.))
                    .font_family(MONO)
                    .text_color(t.faint())
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if tool.out.starts_with('✗') { gpui::rgb(ERR) } else { t.acc_soft(0.8) })
                            .child(SharedString::from(tool.name.clone())),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(SharedString::from(format!("({args})")))),
            );
            if !tool.out.trim().is_empty() {
                foot = foot.child(
                    div()
                        .pl(px(10.))
                        .text_size(px(11.5))
                        .line_height(px(17.))
                        .font_family(MONO)
                        .text_color(t.dim())
                        .child(SharedString::from(truncate(&tool.out, 120))),
                );
            }
        }
        col = col.child(foot);
    }

    let _ = live;
    col.into_any_element()
}

fn live_block(s: &AppState, t: &Theme, st: Entity<AppState>) -> AnyElement {
    let mut col = div().w_full().flex().flex_col().gap_4();

    if !s.live.reasoning.trim().is_empty() {
        col = col.child(
            div()
                .w_full()
                .pl(px(12.))
                .border_l_1()
                .border_color(t.rule2())
                .flex()
                .flex_col()
                .gap_1()
                .child(label(t, "Denken"))
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(23.))
                        .text_color(t.faint())
                        .child(SharedString::from(soften(s.live.reasoning.trim()))),
                ),
        );
    }

    if !s.live.text.trim().is_empty() {
        let mut prose = div().w_full().flex().flex_col().gap_3();
        for el in blocks(t, &s.live.text) {
            prose = prose.child(el);
        }
        col = col.child(prose);
    }

    if s.busy() && s.live.text.trim().is_empty() {
        let phase = if s.live.reconnecting {
            "Verbindung wird wiederhergestellt"
        } else if !s.live.tools.is_empty() {
            "arbeitet"
        } else {
            "denkt nach"
        };
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(13.))
                .text_color(t.dim())
                .child(div().size(px(6.)).rounded_full().bg(t.acc()))
                .child(SharedString::from(phase.to_string())),
        );
    }

    if !s.live.tools.is_empty() {
        let mut foot = div().w_full().flex().flex_col().gap_1();
        for tool in &s.live.tools {
            foot = foot.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .font_family(MONO)
                    .text_size(px(11.5))
                    .line_height(px(17.))
                    .text_color(t.faint())
                    .child(div().flex_shrink_0().text_color(t.acc_soft(0.8)).child(SharedString::from(tool.name.clone())))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(format!("({})", truncate(&tool.args, 96)))),
                    ),
            );
        }
        col = col.child(foot);
    }

    if let Some(err) = &s.live.error {
        col = col.child(
            div()
                .w_full()
                .rounded(px(8.))
                .border_1()
                .border_color(gpui::rgb(ERR))
                .px(px(12.))
                .py(px(8.))
                .text_size(px(13.))
                .text_color(gpui::rgb(ERR))
                .child(SharedString::from(err.clone())),
        );
    }

    for (i, ap) in s.live.approvals.iter().enumerate() {
        let est = st.clone();
        let req = ap.request_id.clone();
        let run = s.live.run_id.clone();
        let gen = ap.execution_generation;
        let mut choices = div().flex().flex_wrap().gap_2();
        for (k, c) in ap.choices.iter().enumerate() {
            let est2 = est.clone();
            let req2 = req.clone();
            let run2 = run.clone();
            let c2 = c.clone();
            choices = choices.child(
                button(t, format!("wahl-{i}-{k}"), c.clone(), k == 0).on_click(move |_, _, cx| {
                    let st = est2.clone();
                    let req = req2.clone();
                    let run = run2.clone();
                    let c = c2.clone();
                    st.update(cx, |s, cx| s.choose_approval(req, run, gen, c, cx));
                }),
            );
        }
        col = col.child(
            div()
                .w_full()
                .rounded(px(10.))
                .border_1()
                .border_color(t.acc_soft(0.45))
                .bg(t.acc_soft(0.07))
                .p(px(14.))
                .flex()
                .flex_col()
                .gap_2()
                .child(label(t, "Freigabe nötig"))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.5))
                        .line_height(px(19.))
                        .text_color(t.fg())
                        .child(SharedString::from(ap.command.clone())),
                )
                .child(choices)
                .child(
                    // Ohne diesen Satz sieht der Nutzer nicht, dass eine
                    // unbeantwortete Freigabe den Lauf beendet.
                    div()
                        .text_size(px(11.5))
                        .line_height(px(16.))
                        .text_color(t.dim())
                        .child(SharedString::from(format!(
                            "seit {} s offen · ohne Antwort verwirft der Server die Freigabe und der Lauf endet",
                            s.clock.saturating_duration_since(ap.since).as_secs()
                        ))),
                ),
        );
    }

    if s.live.reconnecting {
        col = col.child(
            div()
                .text_size(px(11.5))
                .font_family(MONO)
                .text_color(t.faint())
                .child(SharedString::from("Verbindung neu aufbauen …")),
        );
    }

    div()
        .w_full()
        .pl(px(12.))
        .border_l_2()
        .border_color(if s.busy() { t.acc_soft(0.65) } else { t.rule2() })
        .child(col)
        .into_any_element()
}

// ── Eingabezeile ──────────────────────────────────────────────────────────

fn composer(s: &AppState, t: &Theme, st: Entity<AppState>) -> AnyElement {
    let mut box_el = div()
        .id("eingabe")
        .w_full()
        .w(px(ui::col_width(768.)))
        .mx_auto()
        .rounded(px(12.))
        .border_1()
        .border_color(t.rule2())
        .bg(if t.dark { gpui::rgb(0x0e0c15) } else { gpui::rgb(0xffffff) })
        .px(px(16.))
        .py(px(12.))
        .flex()
        .flex_col()
        .gap_1();
    let _ = st;

    if s.open.as_ref().is_some_and(|o| o.read_only) {
        box_el = box_el.child(
            div()
                .w_full()
                .pb(px(8.))
                .mb(px(6.))
                .border_b_1()
                .border_color(t.rule())
                .text_size(px(11.5))
                .line_height(px(16.))
                .text_color(t.dim())
                .child(SharedString::from(
                    "Sitzung aus einem anderen Programm — lesbar, aber nicht fortsetzbar. Links eine neue Sitzung anlegen.",
                )),
        );
    }

    if !s.picker.is_empty() {
        let mut list = div()
            .w_full()
            .pb(px(8.))
            .mb(px(6.))
            .border_b_1()
            .border_color(t.rule())
            .flex()
            .flex_col()
            .gap(px(2.));
        for c in &s.picker {
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .px(px(4.))
                    .py(px(3.))
                    .child(
                        div()
                            .w(px(120.))
                            .flex_shrink_0()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(t.acc())
                            .child(SharedString::from(format!("/{}", c.n))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(t.faint())
                            .child(SharedString::from(c.d.clone())),
                    ),
            );
        }
        box_el = box_el.child(list);
    }

    // Der Eingabetext, zeilenweise mit Schreibmarke.
    let rows = s.composer.rows();
    let mut text_col = div().w_full().flex().flex_col();
    if rows.is_empty() || s.composer.is_empty() {
        text_col = text_col.child(
            div()
                .h(px(BODY_SIZE * 1.45))
                .flex()
                .items_center()
                .text_size(px(BODY_SIZE))
                .text_color(t.faint())
                .child(SharedString::from("Schreiben …  (/ für Befehle)"))
                .child(
                    div()
                        .ml(px(1.))
                        .w(px(1.5))
                        .h(px(BODY_SIZE * 1.15))
                        .bg(t.acc())
                        .opacity(if s.blink { 1.0 } else { 0.15 }),
                ),
        );
    } else {
        for (text, mark) in rows {
            text_col = text_col.child(caret_line(t, &text, mark, s.blink));
        }
    }
    box_el = box_el.child(text_col);

    box_el = box_el.child(
        div()
            .w_full()
            .pt(px(4.))
            .flex()
            .items_center()
            .gap_3()
            .text_size(px(10.5))
            .font_family(MONO)
            .text_color(t.faint())
            .child(SharedString::from("Enter senden"))
            .child(SharedString::from("⇧Enter neue Zeile"))
            .child(div().flex_1())
            .when(s.busy(), |d| {
                d.child(
                    div()
                        .text_color(t.acc())
                        .child(SharedString::from("läuft")),
                )
            }),
    );

    div()
        .w_full()
        .flex_shrink_0()
        .px(px(22.))
        .pb(px(10.))
        .pt(px(8.))
        .flex()
        .flex_col()
        .child(box_el)
        .into_any_element()
}

fn caret_line(t: &Theme, text: &str, mark: Option<usize>, blink: bool) -> AnyElement {
    let chars: Vec<char> = text.chars().collect();
    let base = div()
        .w_full()
        .h(px(BODY_SIZE * 1.45))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .text_size(px(BODY_SIZE))
        .text_color(t.fg());
    match mark {
        Some(m) => {
            let m = m.min(chars.len());
            let pre: String = chars[..m].iter().collect();
            let post: String = chars[m..].iter().collect();
            base.child(div().flex_shrink_0().child(SharedString::from(pre)))
                .child(
                    div()
                        .w(px(1.5))
                        .h(px(BODY_SIZE * 1.1))
                        .flex_shrink_0()
                        .bg(t.acc())
                        .opacity(if blink { 1.0 } else { 0.15 }),
                )
                .child(div().flex_shrink_0().child(SharedString::from(post)))
                .into_any_element()
        }
        None => base.child(SharedString::from(text.to_string())).into_any_element(),
    }
}

fn status_bar(s: &AppState, t: &Theme, sub: &str) -> AnyElement {
    let tokens = if s.live.input_tokens > 0 {
        format!("{} → {} Token", compact(s.live.input_tokens), compact(s.live.output_tokens))
    } else {
        String::new()
    };
    let state_text = if s.busy() {
        format!("läuft {}", state::elapsed(s.live.started))
    } else if let Some((msg, ok)) = &s.notice {
        let short = truncate(msg, 60);
        format!("{}{}", if *ok { "" } else { "Fehler: " }, short)
    } else {
        "bereit".to_string()
    };
    let color = match &s.notice {
        Some((_, false)) => gpui::rgb(ERR),
        Some((_, true)) => t.acc_soft(0.9),
        None => t.faint(),
    };

    div()
        .w_full()
        .h(px(28.))
        .flex_shrink_0()
        .px(px(22.))
        .flex()
        .items_center()
        .gap_4()
        .border_t_1()
        .border_color(t.rule())
        .font_family(MONO)
        .text_size(px(10.5))
        .text_color(t.faint())
        .child(div().flex_1().truncate().child(SharedString::from(sub.to_string())))
        .when(!tokens.is_empty(), |d| d.child(SharedString::from(tokens)))
        .child(div().text_color(color).child(SharedString::from(state_text)))
        .into_any_element()
}

// ── Einstellungen ─────────────────────────────────────────────────────────

fn settings_view(s: &AppState, t: &Theme, st: Entity<AppState>) -> AnyElement {
    let url_mark = if s.form.focus == 0 { Some(s.form.url.cursor()) } else { None };
    let tok_text = if s.form.token.is_empty() && s.form.has_token {
        "••••••••••••  (gespeichert)".to_string()
    } else {
        s.form.token.text()
    };
    let tok_mark = if s.form.focus == 1 {
        if s.form.token.is_empty() && s.form.has_token {
            None
        } else {
            Some(s.form.token.cursor())
        }
    } else {
        None
    };
    let _ = s.form.token.is_empty();

    let field = |label_text: &str, hint: &str, value: String, mark: Option<usize>, active: bool, t: &Theme| {
        let mut box_el = div()
            .w_full()
            .rounded(px(10.))
            .border_1()
            .border_color(if active { t.acc_soft(0.55) } else { t.rule2() })
            .bg(if t.dark { gpui::rgb(0x0e0c15) } else { gpui::rgb(0xffffff) })
            .px(px(14.))
            .py(px(11.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(label(t, label_text));
        box_el = box_el.child(if mark.is_some() {
            caret_line(t, &value, mark, true)
        } else {
            div()
                .w_full()
                .h(px(BODY_SIZE * 1.45))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_size(px(BODY_SIZE))
                .text_color(if value.is_empty() { t.faint() } else { t.fg() })
                .child(SharedString::from(if value.is_empty() { hint.to_string() } else { value.clone() }))
                .into_any_element()
        });
        box_el
    };

    let est = st.clone();
    let est2 = st.clone();

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header(s, t, "Einstellungen", true))
        .child(
            div()
                .id("einstellungen")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .w_full()
                        .py(px(34.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w_full()
                                .w(px(ui::col_width(620.)))
                                .px(px(30.))
                                .flex()
                                .flex_col()
                                .gap_5()
                                .child(
                                    div()
                                        .text_size(px(24.))
                                        .text_color(t.fg())
                                        .child(SharedString::from("Zugang zum Hermes-Server")),
                                )
                        .child(
                            div()
                                .text_size(px(14.))
                                .line_height(px(22.))
                                .text_color(t.dim())
                                .child(SharedString::from(
                                    "Die Adresse liegt im Rust-Kern, nie im Zeichner. Der Schlüssel wird nur maskiert angezeigt.",
                                )),
                        )
                        .child(field(
                            "Serveradresse",
                            "http://127.0.0.1:8080",
                            s.form.url.text(),
                            url_mark,
                            s.form.focus == 0,
                            t,
                        ))
                        .child(field(
                            "Zugangsschlüssel",
                            if s.form.has_token { "gespeichert — leer lassen zum Behalten" } else { "Token einfügen" },
                            tok_text,
                            tok_mark,
                            s.form.focus == 1,
                            t,
                        ))
                        .when(!s.form.hint.is_empty(), |d| {
                            d.child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.5))
                                    .text_color(t.faint())
                                    .child(SharedString::from(format!("Hinweis: {}", s.form.hint))),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .pt(px(6.))
                                .child(button(t, "speichern", "Speichern und verbinden", true).on_click(move |_, _, cx| {
                                    est.update(cx, |s, cx| s.save_settings(cx));
                                }))
                                .child(button(t, "abbrechen", "Abbrechen", false).on_click(move |_, _, cx| {
                                    est2.update(cx, |s, cx| {
                                        s.mode = Mode::Index;
                                        cx.notify();
                                    });
                                })),
                        )
                        .child(
                            div()
                                .pt(px(10.))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(t.faint())
                                .child(SharedString::from("Tab  Feld wechseln"))
                                .child(SharedString::from("Enter  speichern"))
                                .child(SharedString::from("Esc  zurück")),
                        ),
                        ),
                ),
        )
        .into_any_element()
}

// ── Tastatur ──────────────────────────────────────────────────────────────

fn handle_key(st: &Entity<AppState>, ev: &KeyDownEvent, _w: &mut Window, cx: &mut App) {
    let ks = &ev.keystroke;
    let key = ks.key.as_str();
    let m = ks.modifiers;

    if m.control && (key == "q" || key == "w") {
        cx.quit();
        return;
    }

    let mut want_submit = false;
    let mut want_demo_finish = false;

    st.update(cx, |s, cx| {
        // Erst die Sonderwege, dann der Text.
        match s.mode {
            Mode::Index => match key {
                "escape" => {
                    if s.confirm_delete.take().is_some() {
                        cx.notify();
                    }
                }
                "down" => {
                    if s.sel + 1 < s.sessions.len() {
                        s.sel += 1;
                        cx.notify();
                    }
                    return;
                }
                "up" => {
                    s.sel = s.sel.saturating_sub(1);
                    cx.notify();
                    return;
                }
                "enter" => {
                    let sel = s.sel;
                    s.open_session(sel, cx);
                    return;
                }
                "n" => {
                    s.create_session("", cx);
                    return;
                }
                "d" => {
                    let id = s.sessions.get(s.sel).map(|x| x.id.clone());
                    if let Some(id) = id {
                        if s.confirm_delete.as_deref() == Some(id.as_str()) {
                            s.delete_session(id, cx);
                        } else {
                            s.confirm_delete = Some(id);
                            s.notice = Some(("Noch einmal d zum Löschen".into(), false));
                            cx.notify();
                        }
                    }
                    return;
                }
                "," => {
                    s.mode = Mode::Settings;
                    s.form.url.set(&s.server);
                    s.form.token.clear();
                    s.form.focus = 0;
                    cx.notify();
                    return;
                }
                "r" => {
                    s.load_sessions(cx);
                    return;
                }
                "j" => {
                    if s.sel + 1 < s.sessions.len() {
                        s.sel += 1;
                        cx.notify();
                    }
                    return;
                }
                "k" => {
                    s.sel = s.sel.saturating_sub(1);
                    cx.notify();
                    return;
                }
                _ => return,
            },
            Mode::Settings => match key {
                "escape" => {
                    s.mode = Mode::Index;
                    cx.notify();
                    return;
                }
                "tab" => {
                    s.form.focus = 1 - s.form.focus;
                    cx.notify();
                    return;
                }
                "enter" => {
                    s.save_settings(cx);
                    return;
                }
                _ => {}
            },
            Mode::Reading => match key {
                "escape" => {
                    if !s.picker.is_empty() {
                        s.picker.clear();
                    } else {
                        s.mode = Mode::Index;
                        s.open = None;
                        s.live = Default::default();
                    }
                    cx.notify();
                    return;
                }
                "enter" if m.shift => {
                    s.composer.insert("\n");
                    s.update_picker();
                    cx.notify();
                    return;
                }
                "enter" => {
                    want_submit = true;
                    return;
                }
                "backspace" => {
                    s.composer.backspace();
                    s.update_picker();
                    cx.notify();
                    return;
                }
                "delete" => {
                    s.composer.delete();
                    cx.notify();
                    return;
                }
                "left" => {
                    if m.alt {
                        s.composer.word_left();
                    } else {
                        s.composer.left();
                    }
                    cx.notify();
                    return;
                }
                "right" => {
                    if m.alt {
                        s.composer.word_right();
                    } else {
                        s.composer.right();
                    }
                    cx.notify();
                    return;
                }
                "up" => {
                    s.composer.up();
                    cx.notify();
                    return;
                }
                "down" => {
                    s.composer.down();
                    cx.notify();
                    return;
                }
                "home" => {
                    s.composer.home();
                    cx.notify();
                    return;
                }
                "end" => {
                    s.composer.end();
                    cx.notify();
                    return;
                }
                "v" if m.control => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            s.composer.insert(&text);
                            s.update_picker();
                        }
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            },
        }

        // Normaler Text
        let Some(ch) = ks.key_char.as_ref() else { return };
        if m.control || m.alt || m.platform {
            return;
        }
        match s.mode {
            Mode::Settings => {
                if s.form.focus == 0 {
                    s.form.url.insert(ch);
                } else {
                    s.form.token.insert(ch);
                }
            }
            Mode::Reading => {
                s.composer.insert(ch);
                s.update_picker();
                want_demo_finish = false;
            }
            Mode::Index => {}
        }
        cx.notify();
    });

    if want_submit {
        st.update(cx, |s, cx| {
            s.submit(cx);
            if s.demo && s.busy() {
                want_demo_finish = true;
            }
        });
        if want_demo_finish {
            let weak = st.downgrade();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(900))
                    .await;
                weak.update(cx, |s, cx| s.finish_demo(cx)).ok();
            })
            .detach();
        }
    }
}

// ── Kleinkram ─────────────────────────────────────────────────────────────

fn truncate(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return s.replace('\n', " ");
    }
    let part: String = chars[..n].iter().collect();
    format!("{}…", part.replace('\n', " "))
}

fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/newsreader-latin.ttf").as_slice()),
            std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/jetbrains-mono-latin.ttf").as_slice()),
        ]);

        let state = cx.new(AppState::new);
        let rx = state.update(cx, |s, _| s.take_rx());
        if let Some(mut rx) = rx {
            let st = state.clone();
            cx.spawn(async move |cx| {
                while let Some(msg) = rx.recv().await {
                    if st.update(cx, |s, cx| s.drain(msg, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        state.update(cx, |s, cx| s.load_sessions(cx));
        state::tick_loop(state.downgrade(), cx);

        let mut bounds = Bounds::centered(None, gpui::size(px(1120.), px(760.)), cx);
        let full = std::env::var("AESMYRA_FULL").is_ok();
        if full {
            if let Some(display) = cx.displays().first() {
                bounds = display.bounds();
            }
        }
        let options = WindowOptions {
            window_bounds: Some(if full {
                WindowBounds::Fullscreen(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            }),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Aesmyra".into()),
                ..Default::default()
            }),
            window_min_size: Some(gpui::size(px(720.), px(520.))),
            ..Default::default()
        };
        let _ = cx.open_window(options, |_, cx| cx.new(|cx| Root::new(state.clone(), cx)));
        cx.activate(true);
    });
}
