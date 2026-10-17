//! OCR seam (TASK-071 full, FR-31 — `cie-docs::ocr`).
//!
//! Deviation (documented, honest): real OCR is a heavy system dependency
//! (tesseract) outside the offline-first story. The module provides:
//! - the [`OcrProvider`] seam (capabilities, not a vendor — the same
//!   pattern as the TASK-059 VectorStore),
//! - [`SystemTesseract`] — shells out to an INSTALLED `tesseract` binary
//!   (runtime probe, no network), so a properly provisioned host gets
//!   genuine OCR;
//! - [`NoOcr`] — the deterministic fallback (FR-28 spirit): with OCR
//!   unavailable the document is recorded as an image-only artifact via
//!   explicit `unsupported` markers. The pipeline NEVER invents text
//!   (global rule 2).
//!
//! [`ingest_image_document`] is the honest entry: it reports what it did
//! and why, so claim extraction sees `ocr_unavailable` rather than empty
//! text that reads as "no claims".

use std::process::Command;

/// The OCR seam.
pub trait OcrProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    fn ocr(&self, image: &[u8], lang: &str) -> Result<String, String>;
}

/// Shells out to an installed `tesseract` (runtime probe).
pub struct SystemTesseract;

impl OcrProvider for SystemTesseract {
    fn name(&self) -> &'static str {
        "system-tesseract"
    }

    fn available(&self) -> bool {
        Command::new("tesseract")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn ocr(&self, image: &[u8], lang: &str) -> Result<String, String> {
        if !self.available() {
            return Err("tesseract not installed".into());
        }
        // tesseract consumes RASTER images (PNG/JPEG/TIFF) — anything
        // else (e.g. a PDF container) is an honest refusal, not garbage OCR
        let is_raster = image.starts_with(b"\x89PNG")
            || image.starts_with(b"\xFF\xD8\xFF")
            || image.starts_with(b"II*\x00")
            || image.starts_with(b"MM\x00*");
        if !is_raster {
            return Err(
                "unsupported input format: tesseract needs PNG/JPEG/TIFF \
                 (PDF passthrough unsupported; extract embedded images first)"
                    .into(),
            );
        }
        let dir = std::env::temp_dir().join(format!(
            "cie071_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let input = dir.join("in.bin");
        std::fs::write(&input, image).map_err(|e| e.to_string())?;
        let out_base = dir.join("out");
        let out = Command::new("tesseract")
            .arg(&input)
            .arg(&out_base)
            .arg("-l")
            .arg(lang)
            .output()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dir);
        if !out.status.success() {
            return Err(format!(
                "tesseract failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let text = std::fs::read_to_string(out_base.with_extension("txt"))
            .map_err(|e| e.to_string())?;
        Ok(text)
    }
}

/// Deterministic fallback: OCR unavailable — the honest answer.
pub struct NoOcr;

impl OcrProvider for NoOcr {
    fn name(&self) -> &'static str {
        "none"
    }
    fn available(&self) -> bool {
        false
    }
    fn ocr(&self, _image: &[u8], _lang: &str) -> Result<String, String> {
        Err("OCR unavailable in this build (FR-28 fallback)".into())
    }
}

/// The outcome of ingesting an image-only document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageDoc {
    pub text: String,
    /// FR-4 honesty: what happened (ocr provider used / unavailable).
    pub unsupported: Vec<String>,
}

/// Ingest an image-only document: OCR when a provider is available,
/// explicit `ocr_unavailable` markers otherwise (never invented text).
pub fn ingest_image_document(image: &[u8], lang: &str) -> ImageDoc {
    // prefer the system provider when genuinely installed
    let tesseract = SystemTesseract;
    if tesseract.available() {
        match tesseract.ocr(image, lang) {
            Ok(text) => {
                return ImageDoc { text, unsupported: vec![] };
            }
            Err(e) => {
                return ImageDoc {
                    text: String::new(),
                    unsupported: vec![format!("ocr_failed: {e}")],
                };
            }
        }
    }
    ImageDoc {
        text: String::new(),
        unsupported: vec![format!(
            "ocr_unavailable: no provider installed (tried {})",
            tesseract.name()
        )],
    }
}
