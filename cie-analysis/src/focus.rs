//! Follow-up/steering context (US-012): the active selection as removable
//! chips a scoped query can apply, so "show only these failures" or "what
//! about this service?" do not require repeating the whole question.
//! Recognising a steering *phrase* ("ignore X", "pin X", "zoom out") is
//! product/NLU layer; this module is the engine-side primitive it would
//! drive, plus the ambiguous-referent rule: several plausible matches are
//! named, never silently guessed.

use cie_core::types::SymbolId;
use std::collections::BTreeSet;

/// One removable chip in the active context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Chip {
    /// Excluded from results until removed ("ignore X").
    Ignore(SymbolId),
    /// Always kept even if an ignore/filter would otherwise drop it ("pin X").
    Pin(SymbolId),
    /// Narrows results to this symbol's declared neighborhood ("what about X?").
    FocusOn(SymbolId),
}

/// The active selection: an ordered, deduplicated set of chips.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FocusContext {
    pub chips: Vec<Chip>,
}

impl FocusContext {
    pub fn add(&mut self, chip: Chip) {
        if !self.chips.contains(&chip) {
            self.chips.push(chip);
        }
    }

    /// Remove one chip by value (what a removable chip's own "x" does).
    pub fn remove(&mut self, chip: &Chip) {
        self.chips.retain(|c| c != chip);
    }

    /// "zoom out": drop every narrowing FocusOn chip; ignore/pin persist,
    /// since those are durable exclusions, not a zoom level.
    pub fn zoom_out(&mut self) {
        self.chips.retain(|c| !matches!(c, Chip::FocusOn(_)));
    }

    fn ignored(&self) -> BTreeSet<SymbolId> {
        self.chips.iter().filter_map(|c| match c { Chip::Ignore(s) => Some(*s), _ => None }).collect()
    }

    fn pinned(&self) -> BTreeSet<SymbolId> {
        self.chips.iter().filter_map(|c| match c { Chip::Pin(s) => Some(*s), _ => None }).collect()
    }

    /// Most recent FocusOn chip, if any (last one wins — the newest "what
    /// about X" replaces an earlier one without the user removing it).
    pub fn focus(&self) -> Option<SymbolId> {
        self.chips.iter().rev().find_map(|c| match c { Chip::FocusOn(s) => Some(*s), _ => None })
    }

    /// Apply the active chips to a candidate list: ignored symbols are
    /// dropped unless also pinned; if a focus chip is set, the result is
    /// further narrowed to `focus_neighborhood` (the focus symbol's
    /// declared neighbors) when the caller supplies one — an empty
    /// neighborhood is a real "nothing here", not treated as "no focus".
    pub fn apply(&self, candidates: &[SymbolId], focus_neighborhood: Option<&BTreeSet<SymbolId>>) -> Vec<SymbolId> {
        let ignored = self.ignored();
        let pinned = self.pinned();
        let mut out: Vec<SymbolId> = candidates
            .iter()
            .copied()
            .filter(|s| pinned.contains(s) || !ignored.contains(s))
            .collect();
        if let (Some(focus), Some(neigh)) = (self.focus(), focus_neighborhood) {
            out.retain(|s| *s == focus || neigh.contains(s) || pinned.contains(s));
        }
        out
    }
}

/// Resolving a name phrase ("this service") against known matches: the
/// product never guesses among several plausible referents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferentResolution {
    Resolved(SymbolId),
    /// More than one plausible match — named, so the caller can ask rather
    /// than silently pick the first.
    Ambiguous(Vec<SymbolId>),
    NotFound,
}

/// Resolve a referent against a candidate set, preferring one already
/// pinned or focused in `ctx` to break ties without guessing blind.
pub fn resolve_referent(ctx: &FocusContext, matches: &[SymbolId]) -> ReferentResolution {
    match matches.len() {
        0 => ReferentResolution::NotFound,
        1 => ReferentResolution::Resolved(matches[0]),
        _ => {
            let preferred: Vec<SymbolId> = matches
                .iter()
                .copied()
                .filter(|s| ctx.pinned().contains(s) || ctx.focus() == Some(*s))
                .collect();
            match preferred.len() {
                1 => ReferentResolution::Resolved(preferred[0]),
                _ => ReferentResolution::Ambiguous(matches.to_vec()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::SymbolId;

    fn sid(n: u32) -> SymbolId {
        SymbolId(n)
    }

    #[test]
    fn ignore_drops_unless_pinned() {
        let mut ctx = FocusContext::default();
        ctx.add(Chip::Ignore(sid(1)));
        ctx.add(Chip::Pin(sid(2)));
        let out = ctx.apply(&[sid(1), sid(2), sid(3)], None);
        assert_eq!(out, vec![sid(2), sid(3)]);
    }

    #[test]
    fn zoom_out_drops_focus_keeps_ignore_and_pin() {
        let mut ctx = FocusContext::default();
        ctx.add(Chip::Ignore(sid(1)));
        ctx.add(Chip::FocusOn(sid(2)));
        ctx.zoom_out();
        assert!(ctx.focus().is_none());
        assert_eq!(ctx.ignored(), BTreeSet::from([sid(1)]));
    }

    #[test]
    fn focus_narrows_to_neighborhood_without_dropping_focus_itself() {
        let mut ctx = FocusContext::default();
        ctx.add(Chip::FocusOn(sid(5)));
        let neigh = BTreeSet::from([sid(6)]);
        let out = ctx.apply(&[sid(5), sid(6), sid(7)], Some(&neigh));
        assert_eq!(out, vec![sid(5), sid(6)]);
    }

    #[test]
    fn empty_neighborhood_is_a_real_empty_result() {
        let mut ctx = FocusContext::default();
        ctx.add(Chip::FocusOn(sid(5)));
        let neigh = BTreeSet::new();
        let out = ctx.apply(&[sid(6), sid(7)], Some(&neigh));
        assert!(out.is_empty());
    }

    #[test]
    fn ambiguous_referent_is_named_not_guessed() {
        let ctx = FocusContext::default();
        let res = resolve_referent(&ctx, &[sid(1), sid(2)]);
        assert_eq!(res, ReferentResolution::Ambiguous(vec![sid(1), sid(2)]));
    }

    #[test]
    fn pinned_referent_breaks_ambiguity() {
        let mut ctx = FocusContext::default();
        ctx.add(Chip::Pin(sid(2)));
        let res = resolve_referent(&ctx, &[sid(1), sid(2)]);
        assert_eq!(res, ReferentResolution::Resolved(sid(2)));
    }
}
