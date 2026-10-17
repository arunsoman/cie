//! Document ingestion normalizers (TASK-071 full, FR-31): cheap
//! line-based extraction for `.rst` and `.adoc`, tag-stripped text for
//! `.html`, PDF TEXT-LAYER extraction (`pdf` — FlateDecode + Tj/TJ
//! operators) and the OCR seam (`ocr` — SystemTesseract runtime probe
//! with the honest NoOcr fallback). Output is plain prose so the
//! rule-based claim extractor sees the same shape for every format.

/// Normalize a document to plain prose lines based on its extension.
/// Unknown extensions pass through unchanged (markdown/text already are).
pub fn normalize_doc(path: &str, content: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" | "xhtml" => html_to_text(content),
        "rst" => rst_to_text(content),
        "adoc" | "asciidoc" => adoc_to_text(content),
        _ => content.to_string(),
    }
}

/// Read a document file and normalize it for claim extraction.
pub fn read_doc(path: &Path) -> std::io::Result<String> {
    let raw = std::fs::read_to_string(path)?;
    Ok(normalize_doc(&path.to_string_lossy(), &raw))
}

/// FR-31 dispatch for BINARY documents (PDF): extract the text layer and
/// fall back to OCR for image-only pages, recording honest markers. The
/// returned provenance pairs with the claim extractor's Evidence.
pub fn read_binary_doc(path: &Path) -> Result<(String, Vec<String>), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match ext.as_str() {
        "pdf" => {
            let t = pdf::extract_text(&bytes).map_err(|e| e.to_string())?;
            if t.text.trim().is_empty() || !t.unsupported.is_empty() {
                // image-only or partially unsupported: OCR the raw bytes
                let mut provenance = t.unsupported.clone();
                let img = ocr::ingest_image_document(&bytes, "eng");
                if !img.text.is_empty() {
                    return Ok((img.text, provenance));
                }
                provenance.extend(img.unsupported);
                Ok((t.text, provenance))
            } else {
                Ok((t.text, t.unsupported))
            }
        }
        other => Err(format!("read_binary_doc: unsupported binary format `{other}`")),
    }
}

pub mod ocr;
pub mod pdf;

use std::path::Path;

fn html_to_text(content: &str) -> String {
    let no_scripts = content
        .replace("\r\n", "\n")
        .split("\n")
        .collect::<Vec<_>>()
        .join("\n");
    // Drop script/style blocks wholesale.
    let mut out = String::with_capacity(no_scripts.len());
    let mut rest = no_scripts.as_str();
    for block in ["script", "style"] {
        loop {
            let open = format!("<{block}");
            let close = format!("</{block}>");
            match rest.to_ascii_lowercase().find(&open) {
                Some(i) => match rest.to_ascii_lowercase()[i..].find(&close) {
                    Some(j) => {
                        let end = i + j + close.len();
                        out.push_str(&rest[..i]);
                        out.push('\n');
                        rest = &rest[end..];
                    }
                    None => {
                        out.push_str(&rest[..i]);
                        rest = "";
                        break;
                    }
                },
                None => {
                    out.push_str(rest);
                    rest = "";
                    break;
                }
            }
        }
    }
    let mut text = strip_tags(&out);
    for (ent, ch) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
    ] {
        text = text.replace(ent, ch);
    }
    collapse_blanks(&text)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// RST: drop directives (`.. name::`), literal/code blocks (a paragraph
/// ending in `::` plus the indented block that follows), and section
/// underline lines. Prose in RST is never indented, so ANY indented line
/// (literal block, directive body, field list) is dropped — claims live in
/// flush-left prose.
fn rst_to_text(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    let mut out: Vec<String> = vec![];
    for line in normalized.lines() {
        let trimmed = line.trim_end();
        if line.starts_with(' ') || line.starts_with('\t') {
            continue; // indented: literal block / directive body / field list
        }
        if trimmed.starts_with(".. ") && trimmed.contains("::") {
            continue; // directive
        }
        if is_underline(trimmed) {
            continue; // section underline (the title line above stays)
        }
        if trimmed.ends_with("::") {
            // Paragraph introducing a literal block: keep the prose part.
            out.push(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        out.push(trimmed.to_string());
    }
    collapse_blanks(&out.join("\n"))
}

fn is_underline(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty()
        && t.chars()
            .all(|c| matches!(c, '=' | '-' | '~' | '^' | '+' | '*' | '#'))
        && t.len() >= 3
}

/// AsciiDoc: drop `//` comments, code blocks fenced by `----`/`....`, and
/// keep prose (`= Heading` lines stay).
fn adoc_to_text(content: &str) -> String {
    let mut out: Vec<String> = vec![];
    let mut fence: Option<&'static str> = None;
    for line in content.replace("\r\n", "\n").lines() {
        let t = line.trim();
        if let Some(f) = fence {
            if t == f {
                fence = None;
            }
            continue; // inside code block
        }
        if t == "----" || t == "...." {
            fence = Some(if t == "----" { "----" } else { "...." });
            continue;
        }
        if t.starts_with("//") {
            continue; // comment
        }
        if t.starts_with(':') && t.ends_with(':') {
            continue; // attribute entry
        }
        out.push(line.to_string());
    }
    collapse_blanks(&out.join("\n"))
}

fn collapse_blanks(s: &str) -> String {
    let mut out = String::new();
    let mut blank = true;
    for line in s.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            if !blank {
                out.push('\n');
                blank = true;
            }
        } else {
            out.push_str(t);
            out.push('\n');
            blank = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_strips_tags_and_scripts() {
        let doc = "<html><head><style>p{}</style><script>var x=1;</script></head>\
                   <body><h1>Policy</h1><p>The system &amp; users <b>must</b> log.</p></body></html>";
        let text = normalize_doc("policy.html", doc);
        assert!(text.contains("Policy"));
        assert!(text.contains("must log"));
        assert!(!text.contains("var x=1"));
        assert!(!text.contains("<b>"));
        assert!(text.contains("& users"), "entity decoded");
    }

    #[test]
    fn rst_drops_directives_and_literal_blocks() {
        let doc = "Title\n=====\n\nThe service must charge.\n\n.. code-block:: python\n\n    x = 1\n\nThe client should retry.\n\nIntro::\n\n    literal line must stay out\n\nTail text.\n----\n";
        let text = normalize_doc("d.rst", doc);
        assert!(text.contains("The service must charge."));
        assert!(text.contains("The client should retry."));
        assert!(!text.contains("code-block"));
        assert!(!text.contains("x = 1"));
        assert!(!text.contains("literal line must stay out"));
        assert!(
            !text.lines().any(|l| l.trim() == "====="),
            "underline dropped"
        );
    }

    #[test]
    fn adoc_drops_comments_and_code() {
        let doc = "= Policy\n\n// internal note must stay out\nThe agent must verify.\n\n----\ncode line must stay out\n----\n\nThe client will retry.\n";
        let text = normalize_doc("p.adoc", doc);
        assert!(text.contains("= Policy"));
        assert!(text.contains("The agent must verify."));
        assert!(!text.contains("internal note"));
        assert!(!text.contains("code line"));
        assert!(text.contains("The client will retry."));
    }
}
