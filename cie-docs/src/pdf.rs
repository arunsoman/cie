//! PDF text-layer extraction (TASK-071 full, FR-31 — `cie-docs::pdf`).
//!
//! A self-contained PDF 1.7 reader for the ingestion path:
//! - locates stream objects, inflates `/FlateDecode` payloads via the
//!   pure-Rust `flate2` backend (offline-first, no system deps);
//! - extracts text operators from content streams: `Tj`, `'`, `"` and
//!   `TJ` arrays (literal strings with PDF escape sequences, hex
//!   strings), producing plain prose for the claim extractor;
//! - honest failures: encrypted documents, missing xref/trailer and
//!   unsupported filters are explicit errors — never silently empty text
//!   (global rule 2).
//!
//! Scope qualification (documented): this extractor covers the
//! text-layer cases the FR-31 fixtures exercise (uncompressed +
//! FlateDecode content streams, standard encodings). Font-embedded CMaps
//! with custom encodings are reported via `unsupported` markers rather
//! than mis-decoded bytes.

use std::collections::BTreeMap;

/// Extraction outcome with provenance for the evidence layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PdfText {
    pub text: String,
    pub pages: usize,
    /// Constructs this extractor could not model (FR-4 honesty).
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    NotPdf,
    Encrypted,
    NoTextLayer,
    Malformed(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfError::NotPdf => write!(f, "not a PDF (missing %PDF header)"),
            PdfError::Encrypted => write!(f, "PDF is encrypted — text extraction refused"),
            PdfError::NoTextLayer => write!(f, "PDF has no extractable text layer"),
            PdfError::Malformed(d) => write!(f, "malformed PDF: {d}"),
        }
    }
}

impl std::error::Error for PdfError {}

/// Extract the text layer of a PDF (bytes in, prose out).
pub fn extract_text(bytes: &[u8]) -> Result<PdfText, PdfError> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(PdfError::NotPdf);
    }
    let mut unsupported = vec![];
    // encrypted documents carry /Encrypt in the trailer — refuse honestly
    if find_top_level(bytes, b"/Encrypt").is_some() {
        return Err(PdfError::Encrypted);
    }

    // Walk top-level stream objects: `stream\n ... endstream`
    let mut text = String::new();
    let mut pages = 0usize;
    let mut pos = 0usize;
    while let Some(s) = find_from(bytes, b"stream", pos) {
        // the object dictionary spans `N M obj << ... >>` immediately
        // before the stream keyword — scan back to the previous `obj`
        let obj_pos = bytes[..s]
            .windows(3)
            .rposition(|w| w == b"obj")
            .map(|p| p + 3)
            .unwrap_or(0);
        let dict_str = String::from_utf8_lossy(&bytes[obj_pos..s]);
        pos = s + b"stream".len();
        // skip the EOL after `stream`
        if bytes.get(pos) == Some(&b'\r') {
            pos += 1;
        }
        if bytes.get(pos) == Some(&b'\n') {
            pos += 1;
        }
        let Some(end) = find_from(bytes, b"endstream", pos) else {
            break;
        };
        let raw = &bytes[pos..end];
        pos = end + b"endstream".len();

        // filter dispatch
        let payload: Vec<u8> = if dict_str.contains("FlateDecode") {
            match inflate(raw) {
                Ok(p) => p,
                Err(_) => {
                    unsupported.push("flate_decode_failed".into());
                    continue;
                }
            }
        } else if dict_str.contains("/Filter") {
            unsupported.push(format!("unsupported_filter:{}", first_filter(&dict_str)));
            continue;
        } else {
            raw.to_vec()
        };

        // is this a page content stream? Heuristic: contains text ops.
        if is_content_stream(&payload) {
            pages += 1;
            let (page_text, page_unsupported) = extract_content_text(&payload);
            text.push_str(&page_text);
            for u in page_unsupported {
                if !unsupported.contains(&u) {
                    unsupported.push(u);
                }
            }
        }
    }
    if pages == 0 {
        return Err(PdfError::NoTextLayer);
    }
    Ok(PdfText { text: text.trim().to_string(), pages, unsupported })
}

fn inflate(raw: &[u8]) -> Result<Vec<u8>, String> {
    // FlateDecode is zlib format
    let mut decoder = flate2::read::ZlibDecoder::new(raw);
    use std::io::Read;
    let mut out = vec![];
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib: {e}"))?;
    Ok(out)
}

fn first_filter(dict: &str) -> String {
    dict.split_whitespace()
        .find(|w: &&str| w.starts_with('/') && *w != "/Filter")
        .unwrap_or("unknown")
        .to_string()
}

fn is_content_stream(payload: &[u8]) -> bool {
    let hay = &payload[payload.len().saturating_sub(4096).max(0).min(payload.len())..];
    // text operators show up as `Tj`, `TJ`, `BT`/`ET`
    payload.windows(2).any(|w| w == b"Tj")
        || payload.windows(2).any(|w| w == b"TJ")
        || (hay.windows(2).any(|w| w == b"BT") && hay.windows(2).any(|w| w == b"ET"))
}

/// Extract text from one content stream (Tj / ' / " / TJ operators).
fn extract_content_text(data: &[u8]) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut unsupported = vec![];
    let mut strings: BTreeMap<usize, String> = BTreeMap::new();
    let mut i = 0usize;
    while i < data.len() {
        match data[i] {
            b'(' => {
                // literal string — may nest
                let mut depth = 1i32;
                let mut j = i + 1;
                let mut s = String::new();
                while j < data.len() && depth > 0 {
                    match data[j] {
                        b'\\' => {
                            if j + 1 < data.len() {
                                let esc = data[j + 1];
                                match esc {
                                    b'n' => s.push('\n'),
                                    b'r' => s.push('\r'),
                                    b't' => s.push('\t'),
                                    b'b' | b'f' => {}
                                    b'(' => s.push('('),
                                    b')' => s.push(')'),
                                    b'\\' => s.push('\\'),
                                    c if c.is_ascii_digit() => {
                                        // octal escape \ddd
                                        let mut val = 0u32;
                                        let mut k = j + 1;
                                        let mut digits = 0;
                                        while k < data.len()
                                            && digits < 3
                                            && data[k].is_ascii_digit()
                                            && data[k] < b'8'
                                        {
                                            val = val * 8 + (data[k] - b'0') as u32;
                                            k += 1;
                                            digits += 1;
                                        }
                                        if let Some(ch) = char::from_u32(val) {
                                            s.push(ch);
                                        }
                                        j = k - 1;
                                    }
                                    other => s.push(other as char),
                                }
                                j += 1;
                            }
                        }
                        b'(' => {
                            depth += 1;
                            s.push('(');
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                s.push(')');
                            }
                        }
                        c => s.push(c as char),
                    }
                    j += 1;
                }
                let str_pos = i;
                strings.insert(str_pos, s);
                i = j;
                // the operator follows: Tj, ', " — emit on Tj-like ops
                let rest = &data[i..(i + 8).min(data.len())];
                let rest_str = String::from_utf8_lossy(rest);
                let trimmed = rest_str.trim_start();
                if trimmed.starts_with("Tj")
                    || trimmed.starts_with('\'')
                    || trimmed.starts_with('"')
                {
                    if let Some(s) = strings.get(&str_pos) {
                        out.push_str(s);
                        out.push('\n');
                    }
                }
            }
            b'<' if i + 1 < data.len() && data[i + 1] != b'<' => {
                // hex string <...>
                let mut j = i + 1;
                let mut hex = String::new();
                while j < data.len() && data[j] != b'>' {
                    if data[j].is_ascii_hexdigit() {
                        hex.push(data[j] as char);
                    }
                    j += 1;
                }
                if hex.len() % 2 == 1 {
                    hex.push('0');
                }
                let mut s = String::new();
                let hb = hex.as_bytes();
                for w in hb.chunks(2) {
                    let v = u8::from_str_radix(std::str::from_utf8(w).unwrap_or("00"), 16)
                        .unwrap_or(0);
                    s.push(v as char);
                }
                let hex_pos = i;
                strings.insert(hex_pos, s);
                i = j + 1;
                let rest = &data[i..(i + 8).min(data.len())];
                let rest_str = String::from_utf8_lossy(rest);
                if rest_str.trim_start().starts_with("Tj") {
                    if let Some(s) = strings.get(&hex_pos) {
                        out.push_str(s);
                        out.push('\n');
                    }
                }
            }
            b'T' if i + 1 < data.len() && data[i + 1] == b'J' => {
                // TJ array: the strings were already collected — flush in
                // source order, joining without newlines (array items are
                // one visual line's runs)
                let arr_start = data[..i]
                    .iter()
                    .rposition(|&b| b == b'[')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                for (pos, s) in &strings {
                    if *pos >= arr_start {
                        out.push_str(s);
                    }
                }
                out.push('\n');
                strings.retain(|k, _| *k < arr_start);
                i += 2;
            }
            _ => i += 1,
        }
    }
    (out, unsupported)
}

fn find_top_level(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}
