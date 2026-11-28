//! Bausteine der Darstellung: Rich-Text, Markdown-Blöcke, Chips, Knöpfe.

use gpui::{
    div, px, font, AnyElement, Div, FontStyle, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, SharedString, Styled, StyledText, TextRun,
};

use std::sync::atomic::{AtomicU32, Ordering};

use crate::md::{Block, Inline};
use crate::theme::*;

/// Breite des Fensters, wie sie im letzten Zeichnen bekannt war. Die Spalten
/// der Ansichten rechnen damit ihre feste Breite aus — ein Schieberegler aus
/// Prozentwerten würde in GPUI an unbekannten Elternbreiten scheitern.
static VIEWPORT: AtomicU32 = AtomicU32::new(0);

pub fn set_viewport(width: f32) {
    VIEWPORT.store(width.to_bits(), Ordering::Relaxed);
}

/// Spaltenbreite: höchstens `max`, mindestens 320, mit Rand.
pub fn col_width(max: f32) -> f32 {
    let w = f32::from_bits(VIEWPORT.load(Ordering::Relaxed));
    if w <= 0.0 {
        return max;
    }
    (w - 64.0).clamp(320.0, max)
}

/// Setzt nach sehr langen Wortketten eine weiche Trennstelle (U+200B). Ohne sie
/// kann ein einzelnes ununterbrochenes Token (JSON, Base64, URL) den Zeilenumbruch
/// sprengen und die Höhenrechnung des Verlaufs ins Unendliche treiben.
pub fn soften(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 96 + 8);
    let mut run = 0usize;
    for ch in s.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        if ch == '\u{feff}' || ch == '\u{200e}' || ch == '\u{200f}' {
            continue;
        }
        if ch.is_whitespace() {
            run = 0;
            out.push(ch);
        } else {
            run += 1;
            if run > 96 {
                out.push('\u{200B}');
                run = 0;
            }
            out.push(ch);
        }
    }
    out
}

/// Ein Absatz aus mehreren Inline-Stücken — eine einzige Textform, damit
/// Zeilenumbruch und Kerning stimmen.
pub fn rich(theme: &Theme, spans: &[Inline], size: f32, line: f32, color: Hsla) -> AnyhowEl {
    let mut text = String::new();
    let mut runs: Vec<TextRun> = Vec::new();

    for s in spans {
        let piece = s.text();
        if piece.is_empty() {
            continue;
        }
        let start = text.len();
        text.push_str(piece);
        let len = text.len() - start;

        let (family, weight, style, fg, bg, strike, underline) = match s {
            Inline::Plain(_) => (SERIF, FontWeight::NORMAL, FontStyle::Normal, color, None, false, false),
            Inline::Bold(_) => (SERIF, FontWeight::BOLD, FontStyle::Normal, theme.fg().into(), None, false, false),
            Inline::Italic(_) => (SERIF, FontWeight::NORMAL, FontStyle::Italic, color, None, false, false),
            Inline::Strike(_) => (SERIF, FontWeight::NORMAL, FontStyle::Normal, theme.dim().into(), None, true, false),
            Inline::Code(_) => (
                MONO,
                FontWeight::NORMAL,
                FontStyle::Normal,
                theme.acc().into(),
                Some(theme.acc_soft(0.10).into()),
                false,
                false,
            ),
            Inline::Link { .. } => (SERIF, FontWeight::NORMAL, FontStyle::Normal, theme.acc().into(), None, false, true),
        };

        runs.push(TextRun {
            len,
            font: gpui::Font {
                family: SharedString::new_static(family),
                weight,
                style,
                ..font(family)
            },
            color: fg,
            background_color: bg,
            underline: if underline {
                Some(gpui::UnderlineStyle {
                    thickness: px(1.),
                    color: Some(theme.acc_soft(0.5).into()),
                    wavy: false,
                })
            } else {
                None
            },
            strikethrough: if strike {
                Some(gpui::StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(theme.dim().into()),
                })
            } else {
                None
            },
        });
    }

    let el = div()
        .w_full()
        .text_size(px(size))
        .line_height(px(line))
        .text_color(color);
    if text.is_empty() {
        return el.into_any_element();
    }
    el.child(StyledText::new(SharedString::from(text)).with_runs(runs))
        .into_any_element()
}

pub type AnyhowEl = AnyElement;

fn mono_run(text: &str, size: f32, color: Hsla) -> gpui::AnyElement {
    div()
        .w_full()
        .text_size(px(size))
        .line_height(px(size * 1.55))
        .text_color(color)
        .font_family(MONO)
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

/// Markdown-Blöcke als Folge von Elementen.
pub fn blocks(theme: &Theme, src: &str) -> Vec<AnyElement> {
    let softened = soften(src);
    let parsed = crate::md::parse(&softened);
    let mut out: Vec<AnyElement> = Vec::new();
    for b in parsed {
        out.push(match b {
            Block::Heading(level, text) => {
                let (size, gap) = match level {
                    1 => (27.0, 18.0),
                    2 => (22.5, 15.0),
                    3 => (19.5, 12.0),
                    _ => (18.0, 10.0),
                };
                div()
                    .mt(px(gap))
                    .mb(px(4.))
                    .text_size(px(size))
                    .line_height(px(size * 1.35))
                    .text_color(theme.fg())
                    .font_family(SERIF)
                    .child(SharedString::from(text))
                    .into_any_element()
            }
            Block::Para(spans) => div()
                .w_full()
                .child(rich(theme, &spans, BODY_SIZE, BODY_LINE, theme.body().into()))
                .into_any_element(),
            Block::Quote(lines) => div()
                .w_full()
                .pl(px(16.))
                .border_l_2()
                .border_color(theme.acc_soft(0.35))
                .flex()
                .flex_col()
                .gap_1()
                .children(
                    lines
                        .into_iter()
                        .map(|l| rich(theme, &l, BODY_SIZE, BODY_LINE, theme.dim().into())),
                )
                .into_any_element(),
            Block::Bullets(items) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(items.into_iter().enumerate().map(|(i, item)| {
                    let _ = i;
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .mt(px(10.))
                                .size(px(5.))
                                .rounded_full()
                                .bg(theme.acc_soft(0.7))
                                .flex_shrink_0(),
                        )
                        .child(div().flex_1().child(rich(theme, &item, BODY_SIZE, BODY_LINE, theme.body().into())))
                }))
                .into_any_element(),
            Block::Numbered(items) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(items.into_iter().map(|(num, item)| {
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .w(px(24.))
                                .flex_shrink_0()
                                .text_size(px(13.))
                                .line_height(px(BODY_LINE))
                                .text_color(theme.acc())
                                .font_family(MONO)
                                .child(SharedString::from(num)),
                        )
                        .child(div().flex_1().child(rich(theme, &item, BODY_SIZE, BODY_LINE, theme.body().into())))
                }))
                .into_any_element(),
            Block::Code { lang, body } => {
                let label = if lang.trim().is_empty() { "text".to_string() } else { lang };
                div()
                    .w_full()
                    .my(px(4.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(theme.rule2())
                    .bg(if theme.dark { gpui::rgb(0x0d0b13) } else { gpui::rgb(0xf2eff7) })
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(14.))
                            .pt(px(8.))
                            .pb(px(2.))
                            .text_size(px(10.5))
                            .text_color(theme.faint())
                            .font_family(MONO)
                            .child(SharedString::from(label)),
                    )
                    .child(div().px(px(14.)).pb(px(12.)).pt(px(2.)).child(mono_run(&body, 13.5, theme.body().into())))
                    .into_any_element()
            }
            Block::Rule => div().w_full().h(px(1.)).my(px(6.)).bg(theme.rule()).into_any_element(),
            Block::Table { head, rows } => {
                let cols = head.len().max(1);
                let w = 100.0 / cols.max(1) as f32;
                let cell = |spans: &Vec<Inline>, header: bool| {
                    div()
                        .w(gpui::relative(w / 100.0))
                        .px(px(10.))
                        .py(px(6.))
                        .child(rich(
                            theme,
                            spans,
                            if header { 12.5 } else { 15.0 },
                            if header { 18. } else { 22. },
                            if header { theme.dim().into() } else { theme.body().into() },
                        ))
                };
                div()
                    .w_full()
                    .my(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.rule2())
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .bg(if theme.dark { gpui::rgb(0x121019) } else { gpui::rgb(0xf4f1f9) })
                            .children(head.iter().map(|c| cell(c, true))),
                    )
                    .children(rows.iter().map(|r| {
                        div()
                            .w_full()
                            .flex()
                            .border_t_1()
                            .border_color(theme.rule())
                            .children(r.iter().map(|c| cell(c, false)))
                    }))
                    .into_any_element()
            }
            Block::Image { alt, url } => div()
                .w_full()
                .flex()
                .gap_2()
                .items_center()
                .py(px(4.))
                .text_size(px(13.))
                .text_color(theme.dim())
                .child(div().text_color(theme.acc()).child(SharedString::from("Bild")))
                .child(SharedString::from(if alt.is_empty() { url } else { alt }))
                .into_any_element(),
        });
    }
    out
}

/// Abschnittsüberschrift über einem Verlaufsteil.
pub fn label(theme: &Theme, text: impl Into<String>) -> Div {
    div()
        .font_family(MONO)
        .text_size(px(10.5))
        .line_height(px(14.))
        .text_color(theme.faint())
        .child(SharedString::from(text.into()).to_uppercase())
}

/// Ein Knopf mit Rahmen, der auf Hover reagiert.
pub fn button(
    theme: &Theme,
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
    accent: bool,
) -> gpui::Stateful<Div> {
    let id: SharedString = id.into();
    let t = theme;
    div()
        .id(id)
        .px(px(14.))
        .py(px(7.))
        .rounded(px(8.))
        .border_1()
        .border_color(if accent { t.acc_soft(0.45) } else { t.rule2() })
        .bg(if accent { t.acc_soft(0.12) } else { gpui::rgba(0x00000000) })
        .text_size(px(13.))
        .line_height(px(18.))
        .text_color(if accent { t.acc() } else { t.body() })
        .cursor_pointer()
        .hover(move |s| s.bg(if accent { t.acc_soft(0.2) } else { t.acc_soft(0.08) }))
        .child(text.into())
}

/// Waagerechte Trennlinie.
pub fn hairline(theme: &Theme) -> Div {
    div().w_full().h(px(1.)).bg(theme.rule())
}
