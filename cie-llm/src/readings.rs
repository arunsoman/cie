//! Interpretation alternatives (CIE-US US-009 engine close).
//!
//! "See how my question was read — and pick an alternative reading." The
//! deterministic offline provider is already inspectable; this module adds
//! the engine half of the interpretation line: for a natural-language
//! query, enumerate *how it was read* — the ranked interpretation
//! candidates with the terms that grounded each reading and a confidence
//! — plus the explicit low-confidence verdict when no reading is
//! well-grounded (the story's honesty rule: a low-confidence general
//! answer says so instead of guessing).
//!
//! Deterministic by construction: token overlap over a supplied lexicon
//! (concept/term → possible interpretations). No model call, no hidden
//! scoring. A chosen reading is never second-guessed: [`apply_choice`]
//! pins the selection and later runs report it as chosen, not re-ranked.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One reading of the question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// The interpretation text (what the question was taken to mean).
    pub interpretation: String,
    /// Grounded confidence 0..1: matched-term share of the reading's
    /// terms (inspectable, not a model score).
    pub confidence: f32,
    /// Query tokens that grounded this reading.
    pub matched_terms: Vec<String>,
    /// True for the top-ranked reading (what the view defaulted to).
    pub primary: bool,
}

/// The interpretation line: primary reading + nearest alternatives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpretationLine {
    pub query: String,
    pub readings: Vec<Reading>,
    /// When no reading reaches the floor, the honest verdict is stated
    /// instead of guessing (US-009 obligation).
    pub low_confidence: bool,
}

/// Confidence floor below which the line declares low confidence.
pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Tokenize a query into lowercase alphanumeric words (deterministic).
fn tokens(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Enumerate readings. `lexicon` maps each candidate interpretation to
/// the domain terms that would ground it (e.g. "payment flow retry
/// semantics" → ["payment", "retry", "idempotency"]). Deterministic
/// ranking: more matched terms first, ties by interpretation asc.
pub fn alternative_readings(
    query: &str,
    lexicon: &BTreeMap<String, Vec<String>>,
) -> InterpretationLine {
    let q = tokens(query);
    let mut readings: Vec<Reading> = lexicon
        .iter()
        .map(|(interp, terms)| {
            let matched: Vec<String> = terms
                .iter()
                .filter(|t| {
                    let tt = t.to_lowercase();
                    q.iter().any(|w| w == &tt || w.contains(&tt) || tt.contains(w))
                })
                .cloned()
                .collect();
            let confidence = if terms.is_empty() {
                0.0
            } else {
                matched.len() as f32 / terms.len() as f32
            };
            Reading {
                interpretation: interp.clone(),
                confidence,
                matched_terms: matched,
                primary: false,
            }
        })
        .collect();
    readings.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.interpretation.cmp(&b.interpretation))
    });
    if let Some(first) = readings.first_mut() {
        first.primary = true;
    }
    let low_confidence = readings.first().map(|r| r.confidence < CONFIDENCE_FLOOR).unwrap_or(true);
    InterpretationLine {
        query: query.to_string(),
        readings,
        low_confidence,
    }
}

/// A chosen reading is pinned: later views report it as chosen (never
/// second-guessed, per the story).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChosenReading {
    pub interpretation: String,
    pub chosen: bool,
}

pub fn apply_choice(line: &InterpretationLine, interpretation: &str) -> Option<ChosenReading> {
    line.readings
        .iter()
        .find(|r| r.interpretation == interpretation)
        .map(|r| ChosenReading {
            interpretation: r.interpretation.clone(),
            chosen: true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon() -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([
            (
                "retry semantics of the payment flow".to_string(),
                vec!["payment".into(), "retry".into(), "idempotency".into()],
            ),
            (
                "refund timing after a failed charge".to_string(),
                vec!["refund".into(), "charge".into(), "timing".into()],
            ),
            (
                "payment webhook replay".to_string(),
                vec!["payment".into(), "webhook".into(), "replay".into()],
            ),
        ])
    }

    #[test]
    fn readings_rank_by_grounded_terms_and_flag_alternatives() {
        let line = alternative_readings("payment retry behavior", &lexicon());
        assert!(!line.low_confidence);
        assert_eq!(line.readings[0].interpretation, "retry semantics of the payment flow");
        assert!(line.readings[0].primary);
        assert!(line.readings[0].matched_terms.contains(&"payment".to_string()));
        // alternatives exist and are ranked below
        assert!(line.readings.len() >= 2);
        assert!(!line.readings[1].primary);
        assert!(line.readings[1].confidence <= line.readings[0].confidence);
    }

    #[test]
    fn no_confident_reading_says_so_instead_of_guessing() {
        let line = alternative_readings("flurb the quxinator", &lexicon());
        assert!(line.low_confidence);
        // still lists what readings exist (visible, not hidden)
        assert_eq!(line.readings.len(), 3);
        assert!(line.readings.iter().all(|r| r.matched_terms.is_empty()));
    }

    #[test]
    fn chosen_reading_is_pinned_not_second_guessed() {
        let line = alternative_readings("payment retry behavior", &lexicon());
        let alt = &line.readings[1].interpretation.clone();
        let chosen = apply_choice(&line, alt).unwrap();
        assert!(chosen.chosen);
        assert_eq!(chosen.interpretation, *alt);
        assert!(apply_choice(&line, "nonexistent").is_none());
    }

    #[test]
    fn determinism_same_query_same_line() {
        let a = alternative_readings("payment retry behavior", &lexicon());
        let b = alternative_readings("payment retry behavior", &lexicon());
        assert_eq!(a, b);
    }
}
