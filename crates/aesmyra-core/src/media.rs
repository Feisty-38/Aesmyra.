//! Bilder aus dem Dateisystem als Datenadresse.
//!
//! Der Verlauf nennt Grafiken als Pfad (`MEDIA:/…/bild.png`). Die Oberfläche
//! kann Dateien nicht lesen, und ein `file://`-Verweis wird von der Webansicht
//! blockiert — deshalb liest der Kern die Datei und reicht sie als
//! `data:`-Adresse weiter.

use std::path::Path;

const MAX_BYTES: u64 = 12 * 1024 * 1024;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_b64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64[n as usize & 63] as char } else { '=' });
    }
    out
}

fn mime_for(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

/// Pfad → `data:image/…;base64,…`. Nur Bilder, nur vorhandene Dateien,
/// nur bis zur Größengrenze.
pub fn data_url(path: &str) -> Result<String, String> {
    let p = Path::new(path.trim());
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| format!("Keine Bildendung: {path}"))?;
    let mime = mime_for(&ext).ok_or_else(|| format!("Kein Bildformat: .{ext}"))?;

    let meta = std::fs::metadata(p).map_err(|e| format!("Nicht lesbar: {path} ({e})"))?;
    if !meta.is_file() {
        return Err(format!("Kein reguläre Datei: {path}"));
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("Zu groß ({} Bytes, Grenze {MAX_BYTES})", meta.len()));
    }

    let bytes = std::fs::read(p).map_err(|e| format!("Nicht lesbar: {path} ({e})"))?;
    Ok(format!("data:{mime};base64,{}", encode_b64(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kodiert_die_bekannten_stellen() {
        assert_eq!(encode_b64(b""), "");
        assert_eq!(encode_b64(b"f"), "Zg==");
        assert_eq!(encode_b64(b"fo"), "Zm8=");
        assert_eq!(encode_b64(b"foo"), "Zm9v");
        assert_eq!(encode_b64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_b64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_b64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn lehnt_fremde_formate_ab() {
        assert!(data_url("/etc/passwd").is_err());
        assert!(data_url("/tmp/nichts.png").is_err());
    }
}
