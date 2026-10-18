//! Clarification flow (US-105): only decisions that affect behaviour are
//! asked about, every question carries a reason, and silence never
//! authorizes a material decision — [`ClarificationSet::fully_answered`]
//! only becomes true once every question has an explicit answer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationQuestion {
    pub id: String,
    pub text: String,
    /// Why this question matters — required, never decorative.
    pub reason: String,
    pub answer: Option<String>,
}

impl ClarificationQuestion {
    /// Fails with a named reason rather than accepting a question with no
    /// stated reason — "every question explains why it matters" by
    /// construction.
    pub fn new(id: impl Into<String>, text: impl Into<String>, reason: impl Into<String>) -> Result<Self, &'static str> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err("a clarification question must state why it matters");
        }
        Ok(ClarificationQuestion { id: id.into(), text: text.into(), reason, answer: None })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationSet {
    pub questions: Vec<ClarificationQuestion>,
}

impl ClarificationSet {
    pub fn add(&mut self, q: ClarificationQuestion) {
        self.questions.push(q);
    }

    pub fn answer(&mut self, id: &str, answer: impl Into<String>) -> Result<(), &'static str> {
        let q = self.questions.iter_mut().find(|q| q.id == id).ok_or("no such question")?;
        q.answer = Some(answer.into());
        Ok(())
    }

    pub fn pending(&self) -> Vec<&ClarificationQuestion> {
        self.questions.iter().filter(|q| q.answer.is_none()).collect()
    }

    /// True only once every question carries an explicit answer; an
    /// unanswered question keeps this false forever — silence never
    /// authorizes anything by itself.
    pub fn fully_answered(&self) -> bool {
        self.pending().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_without_a_reason_is_rejected() {
        let err = ClarificationQuestion::new("q1", "Allow guest checkout?", "").unwrap_err();
        assert!(err.contains("why it matters"));
    }

    #[test]
    fn unanswered_question_keeps_the_set_not_fully_answered() {
        let mut set = ClarificationSet::default();
        set.add(ClarificationQuestion::new("q1", "Allow guest checkout?", "changes the auth gate behaviour").unwrap());
        assert!(!set.fully_answered());
        assert_eq!(set.pending().len(), 1);
        set.answer("q1", "yes").unwrap();
        assert!(set.fully_answered());
    }

    #[test]
    fn independent_work_can_read_pending_without_blocking() {
        let mut set = ClarificationSet::default();
        set.add(ClarificationQuestion::new("q1", "Rate limit per guest?", "affects abuse-prevention design").unwrap());
        set.add(ClarificationQuestion::new("q2", "Keep cart on logout?", "affects data-retention design").unwrap());
        set.answer("q1", "10/min").unwrap();
        assert_eq!(set.pending().len(), 1);
        assert_eq!(set.pending()[0].id, "q2");
    }
}
