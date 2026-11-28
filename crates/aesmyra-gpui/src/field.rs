//! Die Eingabezeile als eigenes kleines Modell.
//!
//! GPUI 0.2 bringt kein Textfeld mit, deshalb steht hier der Zustand: Text,
//! Schreibmarke, Zeilenweise Bewegung. Die Oberfläche zeichnet nur, was hier
//! steht — dadurch lässt sich das Tippen ohne Fenster prüfen.

#[derive(Clone, Debug, Default)]
pub struct Field {
    text: Vec<char>,
    /// Position der Schreibmarke als Zeichenindex (0 … len).
    cursor: usize,
    /// Senkrechte Wunschspalte beim Bewegen über Zeilen.
    goal: Option<usize>,
}

impl Field {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, s: &str) {
        self.text = s.chars().collect();
        self.cursor = self.text.len();
        self.goal = None;
    }

    pub fn take(&mut self) -> String {
        let s: String = self.text.iter().collect();
        self.clear();
        s
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.goal = None;
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.text.iter().all(|c| c.is_whitespace())
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, s: &str) {
        let add: Vec<char> = s.chars().filter(|c| *c != '\r').collect();
        if add.is_empty() {
            return;
        }
        self.text.splice(self.cursor..self.cursor, add.iter().copied());
        self.cursor += add.len();
        self.goal = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
            self.goal = None;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
            self.goal = None;
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.goal = None;
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
        self.goal = None;
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
        self.goal = None;
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
        self.goal = None;
    }

    pub fn word_left(&mut self) {
        while self.cursor > 0 && self.text[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        while self.cursor > 0 && !self.text[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        self.goal = None;
    }

    pub fn word_right(&mut self) {
        while self.cursor < self.text.len() && !self.text[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        while self.cursor < self.text.len() && self.text[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        self.goal = None;
    }

    pub fn up(&mut self) {
        let start = self.line_start();
        if start == 0 {
            self.cursor = 0;
            return;
        }
        let col = self.goal.unwrap_or(self.cursor - start);
        let prev_end = start - 1;
        let prev_start = self.text[..prev_end]
            .iter()
            .rposition(|c| *c == '\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        self.cursor = (prev_start + col).min(prev_end);
        self.goal = Some(col);
    }

    pub fn down(&mut self) {
        let end = self.line_end();
        if end >= self.text.len() {
            self.cursor = self.text.len();
            return;
        }
        let start = self.line_start();
        let col = self.goal.unwrap_or(self.cursor - start);
        let next_start = end + 1;
        let next_end = self.text[next_start..]
            .iter()
            .position(|c| *c == '\n')
            .map(|p| next_start + p)
            .unwrap_or(self.text.len());
        self.cursor = (next_start + col).min(next_end);
        self.goal = Some(col);
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor]
            .iter()
            .rposition(|c| *c == '\n')
            .map(|p| p + 1)
            .unwrap_or(0)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .iter()
            .position(|c| *c == '\n')
            .map(|p| self.cursor + p)
            .unwrap_or(self.text.len())
    }

    /// Zeilen fürs Zeichnen, mit der Schreibmarke als Zeichenindex je Zeile.
    pub fn rows(&self) -> Vec<(String, Option<usize>)> {
        let mut out = Vec::new();
        let mut start = 0usize;
        for (i, c) in self.text.iter().enumerate() {
            if *c == '\n' {
                out.push((self.text[start..i].iter().collect(), self.mark_in(start, i)));
                start = i + 1;
            }
        }
        out.push((self.text[start..].iter().collect(), self.mark_in(start, self.text.len())));
        for (r, (_, mark)) in out.iter_mut().enumerate() {
            if mark.is_none() && r == 0 && self.text.is_empty() {
                *mark = Some(0);
            }
        }
        out
    }

    fn mark_in(&self, start: usize, end: usize) -> Option<usize> {
        if self.cursor >= start && self.cursor <= end {
            Some(self.cursor - start)
        } else {
            None
        }
    }

    /// Die Zeile, in der die Schreibmarke steht.
    #[cfg(test)]
    pub fn cursor_row(&self) -> usize {
        self.text[..self.cursor].iter().filter(|c| **c == '\n').count()
    }
}

/// Ein Slash-Befehl, wie ihn `ui/commands.js` führt.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Command {
    #[serde(default)]
    pub n: String,
    #[serde(default)]
    pub a: Vec<String>,
    #[serde(default)]
    pub d: String,
    #[serde(default)]
    pub r: Option<String>,
}

pub fn load_commands() -> Vec<Command> {
    const RAW: &str = include_str!("../assets/commands.json");
    serde_json::from_str(RAW).unwrap_or_default()
}

/// Welche Befehle passen zum getippten Anfang?
pub fn suggest<'a>(cmds: &'a [Command], input: &str) -> Vec<&'a Command> {
    let q = input.trim_start_matches('/').to_lowercase();
    if !input.starts_with('/') || q.contains(' ') {
        return Vec::new();
    }
    cmds.iter()
        .filter(|c| c.r.is_some())
        .filter(|c| c.n.starts_with(&q) || c.a.iter().any(|a| a.starts_with(&q)))
        .take(8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tippen_und_loeschen() {
        let mut f = Field::new();
        f.insert("Hallo");
        assert_eq!(f.text(), "Hallo");
        assert_eq!(f.cursor(), 5);
        f.backspace();
        assert_eq!(f.text(), "Hall");
        f.left();
        f.insert("X");
        assert_eq!(f.text(), "HalXl");
        assert_eq!(f.cursor(), 4);
    }

    #[test]
    fn mehrere_zeilen() {
        let mut f = Field::new();
        f.insert("eins\nzwei");
        assert_eq!(f.rows().len(), 2);
        assert_eq!(f.cursor_row(), 1);
        f.up();
        assert_eq!(f.cursor_row(), 0);
        f.down();
        assert_eq!(f.cursor_row(), 1);
    }

    #[test]
    fn zeilenanfang_und_ende() {
        let mut f = Field::new();
        f.insert("abc\ndef");
        f.home();
        assert_eq!(f.cursor(), 4);
        f.end();
        assert_eq!(f.cursor(), 7);
    }

    #[test]
    fn wortweise() {
        let mut f = Field::new();
        f.insert("zwei woerter hier");
        f.word_left();
        assert_eq!(f.cursor(), 13);
        f.word_left();
        assert_eq!(f.cursor(), 5);
    }

    #[test]
    fn vorschlaege() {
        let cmds = load_commands();
        assert!(cmds.len() > 20, "Befehlsliste fehlt: {}", cmds.len());
        let v = suggest(&cmds, "/mod");
        assert!(v.iter().any(|c| c.n == "model"));
        assert!(suggest(&cmds, "hallo").is_empty());
    }

    #[test]
    fn schreibmarke_je_zeile() {
        let mut f = Field::new();
        f.insert("ab\ncd");
        let rows = f.rows();
        assert_eq!(rows[0], ("ab".to_string(), None));
        assert_eq!(rows[1], ("cd".to_string(), Some(2)));
    }
}
