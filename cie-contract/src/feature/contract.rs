//! Feature contract assembly (US-108): a complete brief — scope, actors,
//! rules, failure paths, assumptions and acceptance criteria — where
//! every requirement carries its origin, and material omissions stay
//! visible rather than silently absent.

use serde::{Deserialize, Serialize};

/// Where a requirement came from — distinguishing what the requester
/// actually said from what the assembler filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequirementOrigin {
    UserStated,
    InferredDefault,
    OptionalImprovement,
    NecessaryPrerequisite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub text: String,
    pub origin: RequirementOrigin,
}

impl Requirement {
    pub fn new(text: impl Into<String>, origin: RequirementOrigin) -> Self {
        Requirement { text: text.into(), origin }
    }
}

/// A complete feature brief. `open_omissions` names material gaps the
/// assembler knows about but could not fill — approval can inspect them
/// instead of discovering a silently absent requirement later.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureContract {
    pub scope: String,
    pub actors: Vec<String>,
    pub rules: Vec<Requirement>,
    pub failure_paths: Vec<Requirement>,
    pub assumptions: Vec<Requirement>,
    pub acceptance_criteria: Vec<Requirement>,
    pub open_omissions: Vec<String>,
}

impl FeatureContract {
    pub fn all_requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.rules.iter().chain(&self.failure_paths).chain(&self.assumptions).chain(&self.acceptance_criteria)
    }

    /// A contract is only reviewable as complete when it has at least one
    /// observable acceptance criterion and no open, unnamed omission.
    pub fn is_complete(&self) -> bool {
        !self.acceptance_criteria.is_empty() && self.open_omissions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> FeatureContract {
        FeatureContract {
            scope: "guest checkout".into(),
            actors: vec!["guest".into(), "payment gateway".into()],
            rules: vec![Requirement::new("cart total must exceed $0", RequirementOrigin::UserStated)],
            failure_paths: vec![Requirement::new("payment decline keeps the cart intact", RequirementOrigin::InferredDefault)],
            assumptions: vec![Requirement::new("guest email is unverified", RequirementOrigin::NecessaryPrerequisite)],
            acceptance_criteria: vec![Requirement::new("a guest can complete checkout without registering", RequirementOrigin::UserStated)],
            open_omissions: Vec::new(),
        }
    }

    #[test]
    fn complete_contract_has_criteria_and_no_open_omission() {
        assert!(contract().is_complete());
    }

    #[test]
    fn an_open_omission_keeps_the_contract_incomplete() {
        let mut c = contract();
        c.open_omissions.push("refund policy for abandoned guest carts is undecided".into());
        assert!(!c.is_complete());
    }

    #[test]
    fn requirements_carry_their_origin_distinctly() {
        let c = contract();
        let origins: Vec<RequirementOrigin> = c.all_requirements().map(|r| r.origin).collect();
        assert!(origins.contains(&RequirementOrigin::UserStated));
        assert!(origins.contains(&RequirementOrigin::InferredDefault));
        assert!(origins.contains(&RequirementOrigin::NecessaryPrerequisite));
    }
}
