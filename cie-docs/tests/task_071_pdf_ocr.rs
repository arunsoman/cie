//! TASK-071 validation: PDF text-layer extraction + OCR seam (FR-31).
//!
//! Sketch deviations (documented, single-point):
//! - The PDF fixture is BUILT IN-TEST (minimal PDF 1.7: header, one
//!   FlateDecode content stream with Tj operators, trailer) so the test
//!   needs no binary asset.
//! - OCR runs through the provider seam: SystemTesseract (runtime probe)
//!   when installed, else the honest NoOcr fallback with
//!   `ocr_unavailable` markers — never invented text (rule 2).

use cie_docs::ocr::{ingest_image_document, OcrProvider, NoOcr, SystemTesseract};
use cie_docs::pdf::{extract_text, PdfError};
use cie_docs::read_binary_doc;
use std::io::Write;

/// Build a minimal one-page PDF whose content stream is FlateDecode
/// compressed and carries `Tj` text operators.
fn fixture_pdf_text_layer() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 720 Td (The system must authorize every charge.) Tj 0 -20 Td (Refunds call the ledger.) Tj ET";
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(content).unwrap();
    let stream = zlib.finish().unwrap();

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");
    pdf.extend_from_slice(
        format!(
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
             2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
             3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n\
             4 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            stream.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\ntrailer\n<< /Root 1 0 R /Size 5 >>\n%%EOF\n");
    pdf
}

/// The sketch's `task_071_pdf_text_layer_extraction` (binding DoD: the
/// text layer yields prose for the claim extractor).
#[test]
fn task_071_pdf_text_layer_extraction() {
    let pdf = fixture_pdf_text_layer();
    let t = extract_text(&pdf).expect("text layer extracted");
    assert_eq!(t.pages, 1);
    assert!(
        t.text.contains("authorize every charge"),
        "text operator content extracted: {:?}",
        t.text
    );
    assert!(t.text.contains("Refunds call the ledger"));
    // provenance: no unsupported constructs on the happy path
    assert!(t.unsupported.is_empty(), "{:?}", t.unsupported);

    // non-PDF input is an explicit error, never silently empty
    assert!(matches!(extract_text(b"not a pdf"), Err(PdfError::NotPdf)));

    // the FR-31 dispatch: read_binary_doc normalizes PDFs to prose
    let dir = std::env::temp_dir().join(format!(
        "cie071_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.pdf");
    std::fs::write(&path, &pdf).unwrap();
    let (prose, provenance) = read_binary_doc(&path).expect("pdf dispatched");
    assert!(prose.contains("authorize"));
    assert!(provenance.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

/// Encrypted PDFs are refused honestly (global rule 2).
#[test]
fn task_071_encrypted_pdf_refused() {
    let text = String::from_utf8_lossy(&fixture_pdf_text_layer()).to_string();
    let with_enc: Vec<u8> = text
        .replace("/Root 1 0 R", "/Root 1 0 R /Encrypt 9 0 R")
        .into_bytes();
    assert!(matches!(extract_text(&with_enc), Err(PdfError::Encrypted)));
}

/// Image-only PDFs (no text layer) fall through to the OCR seam; with no
/// provider installed the outcome carries explicit `ocr_unavailable`
/// markers — never invented text (global rule 2, FR-28 fallback).
#[test]
fn task_071_image_only_document_is_honest() {
    // a PDF whose stream has NO text operators → NoTextLayer
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(b"0 0 100 100 re f").unwrap();
    let stream = zlib.finish().unwrap();
    let mut pdf = b"%PDF-1.7\n".to_vec();
    pdf.extend_from_slice(
        format!(
            "1 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            stream.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");

    // extraction honestly reports no text layer
    assert!(matches!(extract_text(&pdf), Err(PdfError::NoTextLayer)));

    // the OCR seam: the outcome NEVER invents text — it carries an
    // explicit honest marker (`ocr_unavailable` when no provider is
    // installed, `ocr_failed: unsupported input format` when tesseract
    // exists but PDFs are not raster images)
    let img = ingest_image_document(&pdf, "eng");
    assert!(img.text.is_empty(), "the OCR path must not fabricate text");
    assert!(
        img.unsupported
            .iter()
            .any(|u| u.starts_with("ocr_unavailable") || u.starts_with("ocr_failed")),
        "explicit marker: {:?}",
        img.unsupported
    );

    // the provider seam is real: NoOcr reports unavailable
    assert!(!NoOcr.available());
    assert!(NoOcr.ocr(&[], "eng").is_err());
}

/// The SystemTesseract provider probes the runtime honestly: available
/// only when the binary exists and works; a failed run surfaces the
/// error (no silent fabrication).
#[test]
fn task_071_tesseract_provider_probe() {
    let t = SystemTesseract;
    // the probe result matches `tesseract --version` reality
    let installed = std::process::Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert_eq!(t.available(), installed);
    if !installed {
        // unavailable → ocr() errors with evidence context
        assert!(t.ocr(b"img", "eng").is_err());
    }
}
