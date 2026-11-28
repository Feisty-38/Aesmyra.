//! Markdown zu einem kleinen Elementbaum.
//!
//! Bewusst kein vollständiger Parser: Aesmyra zeigt Antworten eines Agenten,
//! und dort kommen Überschriften, Absätze, Listen, Code, Zitate, Tabellen und
//! Trennlinien vor. Genau diese Blöcke versteht dieser Parser; alles andere
//! bleibt Absatztext. Das Ergebnis ist ein `Vec<Block>`, den die Oberfläche
//! ohne weitere Auswertung zeichnet.

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Plain(String),
    Bold(String),
    Italic(String),
    Strike(String),
    Code(String),
    Link { text: String, url: String },
}

impl Inline {
    pub fn text(&self) -> &str {
        match self {
            Inline::Plain(t)
            | Inline::Bold(t)
            | Inline::Italic(t)
            | Inline::Strike(t)
            | Inline::Code(t) => t,
            Inline::Link { text, .. } => text,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(Vec<Inline>),
    Quote(Vec<Vec<Inline>>),
    Bullets(Vec<Vec<Inline>>),
    Numbered(Vec<(String, Vec<Inline>)>),
    Code { lang: String, body: String },
    Table { head: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Rule,
    Image { alt: String, url: String },
}

/// Zerlegt Text in Blöcke.
pub fn parse(src: &str) -> Vec<Block> {
    let binding = src.replace('\r', "");
    let lines: Vec<&str> = binding.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if t.is_empty() {
            i += 1;
            continue;
        }

        // Codeblock
        if let Some(rest) = t.strip_prefix("```") {
            let lang = rest.trim().to_string();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            out.push(Block::Code {
                lang,
                body: body.join("\n"),
            });
            continue;
        }

        // Trennlinie
        if t == "---" || t == "***" || t == "___" {
            out.push(Block::Rule);
            i += 1;
            continue;
        }

        // Überschrift
        if t.starts_with('#') {
            let level = t.chars().take_while(|c| *c == '#').count();
            if level <= 6 && t.chars().nth(level) == Some(' ') {
                out.push(Block::Heading(level as u8, t[level..].trim().to_string()));
                i += 1;
                continue;
            }
        }

        // Tabelle: Kopfzeile mit |, darunter eine Trennerzeile
        if t.starts_with('|') && i + 1 < lines.len() && is_table_rule(lines[i + 1]) {
            let head = split_row(t);
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(split_row(lines[i]).into_iter().map(|c| inline(&c)).collect());
                i += 1;
            }
            out.push(Block::Table {
                head: head.into_iter().map(|c| inline(&c)).collect(),
                rows,
            });
            continue;
        }

        // Zitat
        if t.starts_with('>') {
            let mut parts = Vec::new();
            while i < lines.len() {
                let cur = lines[i].trim();
                if let Some(rest) = cur.strip_prefix('>') {
                    let rest = rest.strip_prefix(' ').unwrap_or(rest);
                    parts.push(inline(rest));
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Block::Quote(parts));
            continue;
        }

        // Aufzählung
        if bullet_item(t).is_some() {
            let mut items = Vec::new();
            while i < lines.len() {
                let cur = lines[i].trim();
                let Some(rest) = bullet_item(cur) else { break };
                let mut text = rest.to_string();
                i += 1;
                // Fortsetzungszeilen (eingerückt, keine neue Markierung)
                while i < lines.len() {
                    let nxt = lines[i];
                    if nxt.trim().is_empty() {
                        break;
                    }
                    if !nxt.starts_with(' ') || bullet_item(nxt.trim()).is_some() {
                        break;
                    }
                    text.push(' ');
                    text.push_str(nxt.trim());
                    i += 1;
                }
                items.push(inline(&text));
            }
            out.push(Block::Bullets(items));
            continue;
        }

        // Nummerierte Liste
        if numbered_item(t).is_some() {
            let mut items = Vec::new();
            while i < lines.len() {
                let cur = lines[i].trim();
                let Some((num, rest)) = numbered_item(cur) else { break };
                let mut text = rest.to_string();
                i += 1;
                while i < lines.len() {
                    let nxt = lines[i];
                    if nxt.trim().is_empty() || !nxt.starts_with(' ') || numbered_item(nxt.trim()).is_some() {
                        break;
                    }
                    text.push(' ');
                    text.push_str(nxt.trim());
                    i += 1;
                }
                items.push((num, inline(&text)));
            }
            out.push(Block::Numbered(items));
            continue;
        }

        // Einzelbild
        if let Some((alt, url)) = image_only(t) {
            out.push(Block::Image { alt, url });
            i += 1;
            continue;
        }

        // Absatz bis zur nächsten Leerzeile oder einem Blockanfang
        let mut para = String::new();
        while i < lines.len() {
            let cur = lines[i].trim();
            if cur.is_empty() || starts_block(&lines, i) {
                break;
            }
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(cur);
            i += 1;
        }
        if !para.is_empty() {
            out.push(Block::Para(inline(&para)));
        } else {
            i += 1;
        }
    }

    out
}

fn starts_block(lines: &[&str], i: usize) -> bool {
    let t = lines[i].trim();
    t.starts_with("```")
        || t.starts_with('#')
        || t.starts_with('>')
        || bullet_item(t).is_some()
        || numbered_item(t).is_some()
        || t == "---"
        || (t.starts_with('|') && i + 1 < lines.len() && is_table_rule(lines[i + 1]))
}

fn is_table_rule(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|')
        && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
        && t.contains('-')
}

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn bullet_item(t: &str) -> Option<&str> {
    for p in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(p) {
            return Some(rest);
        }
    }
    None
}

fn numbered_item(t: &str) -> Option<(String, &str)> {
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &t[digits.len()..];
    let rest = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))?;
    Some((format!("{digits}."), rest))
}

fn image_only(t: &str) -> Option<(String, String)> {
    let t = t.trim();
    if !t.starts_with("![") {
        return None;
    }
    let close = t.find(']')?;
    let alt = t[2..close].to_string();
    let after = t[close..].trim_start_matches(']');
    let after = after.trim();
    if !after.starts_with('(') || !after.ends_with(')') {
        return None;
    }
    let url = after[1..after.len() - 1].trim().to_string();
    Some((alt, url))
}

/// Zerlegt eine Zeile in Inline-Stücke.
pub fn inline(src: &str) -> Vec<Inline> {
    let b: Vec<char> = src.chars().collect();
    let mut out: Vec<Inline> = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;

    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Plain(std::mem::take(buf)));
        }
    };

    while i < b.len() {
        let c = b[i];

        // Code
        if c == '`' {
            if let Some(end) = find_char(&b, i + 1, '`') {
                flush(&mut buf, &mut out);
                let code: String = b[i + 1..end].iter().collect();
                out.push(Inline::Code(code));
                i = end + 1;
                continue;
            }
        }

        // Link
        if c == '[' && i + 1 < b.len() {
            if let Some(close) = find_char(&b, i + 1, ']') {
                if close + 1 < b.len() && b[close + 1] == '(' {
                    if let Some(paren) = find_char(&b, close + 2, ')') {
                        flush(&mut buf, &mut out);
                        let text: String = b[i + 1..close].iter().collect();
                        let url: String = b[close + 2..paren].iter().collect();
                        out.push(Inline::Link {
                            text: text.clone(),
                            url: url.trim().to_string(),
                        });
                        let _ = text;
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }

        // Fett / kursiv / durchgestrichen
        if c == '*' || c == '_' || c == '~' {
            let double = i + 1 < b.len() && b[i + 1] == c;
            let mark_len = if double { 2 } else { 1 };
            if let Some(end) = find_marks(&b, i + mark_len, c, mark_len) {
                flush(&mut buf, &mut out);
                let text: String = b[i + mark_len..end].iter().collect();
                if !text.trim().is_empty() {
                    out.push(match (c, double) {
                        ('~', _) => Inline::Strike(text),
                        (_, true) => Inline::Bold(text),
                        _ => Inline::Italic(text),
                    });
                    i = end + mark_len;
                    continue;
                }
            }
        }

        // Harte Sonderzeichen bleiben Text
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut out);
    out
}

fn find_char(b: &[char], from: usize, want: char) -> Option<usize> {
    (from..b.len()).find(|&k| b[k] == want)
}

/// Sucht die schließende Markierung, ohne von einer einzelnen Markierung
/// innerhalb eines Doppels getäuscht zu werden.
fn find_marks(b: &[char], from: usize, mark: char, len: usize) -> Option<usize> {
    let mut k = from;
    while k + len <= b.len() {
        if b[k] == mark {
            let doubled = k + 1 < b.len() && b[k + 1] == mark;
            if len == 2 && doubled {
                return Some(k);
            }
            if len == 1 {
                // Eine einzelne Markierung vor einer anderen Stelle ist gültig,
                // solange sie nicht Teil eines Doppels ist.
                let prev_same = k > 0 && b[k - 1] == mark;
                if !prev_same && k > from {
                    return Some(k);
                }
            }
        }
        k += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absaetze_und_ueberschriften() {
        let b = parse("# Titel\n\nEin **fetter** Satz mit `code`.\n\n## Zwei");
        assert_eq!(b.len(), 3);
        assert!(matches!(b[0], Block::Heading(1, _)));
        assert!(matches!(b[1], Block::Para(_)));
        assert!(matches!(b[2], Block::Heading(2, _)));
    }

    #[test]
    fn liste_und_code() {
        let b = parse("- eins\n- zwei\n\n```rust\nfn main() {}\n```");
        assert!(matches!(b[0], Block::Bullets(ref v) if v.len() == 2));
        match &b[1] {
            Block::Code { lang, body } => {
                assert_eq!(lang, "rust");
                assert_eq!(body, "fn main() {}");
            }
            other => panic!("erwartet Code, war {other:?}"),
        }
    }

    #[test]
    fn tabelle() {
        let b = parse("| a | b |\n|---|---|\n| 1 | 2 |");
        match &b[0] {
            Block::Table { head, rows } => {
                assert_eq!(head.len(), 2);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("erwartet Tabelle, war {other:?}"),
        }
    }

    #[test]
    fn inline_stile() {
        // Zwischen zwei Markierungen bleibt das Leerzeichen ein eigenes Stück;
        // das ist fürs Setzen richtig, deshalb zählt die Reihe acht Stücke.
        let v = inline("a **b** c *d* `e` [f](http://x)");
        assert_eq!(v.len(), 8);
        assert_eq!(v[0], Inline::Plain("a ".into()));
        assert_eq!(v[1], Inline::Bold("b".into()));
        assert_eq!(v[2], Inline::Plain(" c ".into()));
        assert_eq!(v[3], Inline::Italic("d".into()));
        assert_eq!(v[4], Inline::Plain(" ".into()));
        assert_eq!(v[5], Inline::Code("e".into()));
        assert_eq!(v[6], Inline::Plain(" ".into()));
        assert!(matches!(v[7], Inline::Link { .. }));
    }

    #[test]
    fn zitat_und_regel() {
        let b = parse("> zitiert\n\n---");
        assert!(matches!(b[0], Block::Quote(_)));
        assert_eq!(b[1], Block::Rule);
    }

    #[test]
    fn nummerierte_liste() {
        let b = parse("1. eins\n2. zwei");
        match &b[0] {
            Block::Numbered(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].0, "1.");
            }
            other => panic!("erwartet Numbered, war {other:?}"),
        }
    }
}
