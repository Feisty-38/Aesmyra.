//! Auswertung des Ereignisstroms (Server-Sent Events).
//!
//! Bewusst ohne Fremdbibliothek: der Parser arbeitet auf Bytes, weil ein
//! Netzwerkstück mitten in einem Mehrbytezeichen enden kann. Getrennt wird
//! ausschließlich an Leerzeilen, also immer an einer Zeichengrenze.

/// Obergrenze für einen noch nicht abgeschlossenen Block. Ein Strom, der
/// 1 MiB ohne Trenner liefert, ist kein Ereignisstrom mehr — dann wird
/// abgebrochen, statt den Speicher volllaufen zu lassen.
pub const MAX_BUFFER: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    #[error("Ereignisblock überschreitet die Puffergrenze von {0} Bytes")]
    Overflow(usize),
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub event: String,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    /// Bis hierher wurde bereits erfolglos nach einem Trenner gesucht.
    /// Ohne diese Marke beginnt die Suche bei jedem Netzstück wieder bei 0.
    scanned: usize,
}

/// Sucht den nächsten Blocktrenner ab `from`. Ein Trenner ist entweder eine
/// Leerzeile (`\n\n`) oder eine CRLF-Leerzeile (`\r\n\r\n`).
fn separator_from(buf: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i < buf.len() {
        if buf[i] == b'\n' && i + 1 < buf.len() && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if buf[i] == b'\r' && i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, SseError> {
        self.buf.extend_from_slice(chunk);

        let mut out = Vec::new();
        let mut cut = 0usize;
        let mut scan = self.scanned.min(self.buf.len());

        while let Some((idx, len)) = separator_from(&self.buf, scan) {
            let text = String::from_utf8_lossy(&self.buf[cut..idx]);
            if let Some(frame) = parse_frame(&text) {
                out.push(frame);
            }
            // Nach dem Ausschneiden verschiebt sich der Rest nach vorn.
            cut = idx + len;
            scan = cut;
        }

        if cut > 0 {
            // Einmal am Ende ausschneiden: vorher kopierte jeder Rahmen den
            // gesamten Rest, was bei gebündelten Rahmen quadratisch kostete.
            self.buf.drain(..cut);
            self.scanned = 0;
        }

        if self.buf.len() > MAX_BUFFER {
            self.buf.clear();
            self.scanned = 0;
            return Err(SseError::Overflow(MAX_BUFFER));
        }
        if cut == 0 {
            // Ein Trenner ist höchstens vier Bytes lang; nur die letzten
            // drei Bytes können der Anfang eines geteilten Trenners sein.
            self.scanned = self.buf.len().saturating_sub(3);
        }
        Ok(out)
    }

    /// Rest am Ende eines Stroms, falls der letzte Block ohne Leerzeile endet.
    pub fn finish(&mut self) -> Option<Frame> {
        if self.buf.is_empty() {
            return None;
        }
        let text = String::from_utf8_lossy(&self.buf).to_string();
        self.buf.clear();
        self.scanned = 0;
        parse_frame(&text)
    }
}

fn parse_frame(text: &str) -> Option<Frame> {
    let mut event = String::from("message");
    let mut data = String::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some((k, v)) => (k, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match key {
            "event" => event = value.trim().to_string(),
            "data" => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
            _ => {}
        }
    }
    if data.is_empty() {
        return None;
    }
    Some(Frame { event, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zerlegt_bloecke() {
        let mut p = SseParser::default();
        let frames = p
            .push(b"event: assistant.delta\ndata: {\"delta\":\"hal\"}\n\nevent: done\ndata: {}\n\n")
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event, "assistant.delta");
        assert_eq!(frames[0].data, "{\"delta\":\"hal\"}");
        assert_eq!(frames[1].event, "done");
    }

    #[test]
    fn haelt_halbe_bloecke_zurueck() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: x\ndata: {\"a\":\"\xC3").unwrap().is_empty());
        let frames = p.push(b"\xA4\"}\n\n").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "{\"a\":\"\u{00e4}\"}");
    }

    #[test]
    fn vertraegt_crlf() {
        let mut p = SseParser::default();
        let frames = p.push(b"event: done\r\ndata: {}\r\n\r\n").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "done");
    }

    #[test]
    fn trenner_ueber_stueckgrenze() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: done\ndata: {}\n").unwrap().is_empty());
        let frames = p.push(b"\n").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "done");

        // CRLF-Trenner, mitten im \r\n\r\n geteilt.
        let mut p = SseParser::default();
        assert!(p.push(b"event: x\r\ndata: {}\r\n\r").unwrap().is_empty());
        let frames = p.push(b"\n").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, "x");
    }

    #[test]
    fn finish_liefert_restblock() {
        let mut p = SseParser::default();
        let frames = p.push(b"event: done\ndata: {\"ok\":true}").unwrap();
        assert!(frames.is_empty());
        let last = p.finish().expect("Restblock fehlt");
        assert_eq!(last.event, "done");
        assert_eq!(last.data, "{\"ok\":true}");
        assert!(p.finish().is_none(), "zweiter Aufruf muss leer sein");
    }

    #[test]
    fn puffer_grenze_meldet_fehler() {
        let mut p = SseParser::default();
        let stueck = vec![b'x'; 64 * 1024];
        for _ in 0..(MAX_BUFFER / stueck.len()) {
            p.push(&stueck).unwrap();
        }
        let err = p.push(&stueck).expect_err("Grenze nicht gemeldet");
        assert_eq!(err, SseError::Overflow(MAX_BUFFER));
        // Nach dem Fehler ist der Puffer leer und wieder benutzbar.
        assert!(p.push(b"event: done\ndata: {}\n\n").unwrap().len() == 1);
    }

    #[test]
    fn viele_kleine_stuecke() {
        // Viele winzige Netzstücke dürfen nicht quadratisch werden und müssen
        // trotzdem jeden Block genau einmal liefern.
        let mut p = SseParser::default();
        let mut count = 0usize;
        for _ in 0..2000 {
            let frames = p.push(b"event: tick\ndata: {\"n\":1}\n\n").unwrap();
            count += frames.len();
        }
        assert_eq!(count, 2000);
        assert!(p.finish().is_none());
    }
}
